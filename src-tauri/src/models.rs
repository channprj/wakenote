use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Installed,
    Missing,
    Downloading,
    Verifying,
    Extracting,
    Ready,
    Unloaded,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub id: String,
    pub display_name: String,
    pub engine: String,
    pub provider_runtime: String,
    pub download_url: Option<String>,
    pub checksum_sha256: Option<String>,
    pub size_mb: u64,
    pub languages: Vec<String>,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub offline: bool,
    pub status: ModelStatus,
    pub download_progress: Option<u8>,
    pub download_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ModelRegistryEntry {
    pub id: String,
    pub display_name: String,
    pub engine: String,
    pub provider_runtime: String,
    pub download_url: Option<String>,
    pub checksum_sha256: Option<String>,
    pub size_mb: u64,
    pub languages: Vec<String>,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDownloadRecord {
    pub model_id: String,
    pub status: ModelStatus,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDownloadState {
    pub downloads: BTreeMap<String, ModelDownloadRecord>,
}

impl ModelDownloadRecord {
    pub fn download_progress_percent(&self) -> Option<u8> {
        download_progress_percent(self)
    }
}

#[derive(Debug, Clone)]
pub struct ModelStore {
    model_directory: PathBuf,
}

/// Which in-process sherpa-onnx recognizer a model uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SherpaModelKind {
    /// NeMo/Parakeet offline transducer (encoder/decoder/joiner + tokens).
    Transducer,
    /// Cache-aware streaming transducer (Nemotron 3.5 ASR). Same four-file layout
    /// as [`Transducer`](Self::Transducer) but loaded by the online recognizer.
    OnlineTransducer,
    /// SenseVoice offline model (single model file + tokens).
    SenseVoice,
}

/// Layout of a downloadable sherpa-onnx model: its `.tar.bz2` extracts to
/// [`dir`](Self::dir) under the model directory and contains [`files`](Self::files).
/// The model's `download_url` points at that archive; the bundled sherpa-onnx
/// engine runs it in-process (no external CLI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SherpaModelSpec {
    pub kind: SherpaModelKind,
    /// Top-level directory created when the archive is extracted.
    pub dir: String,
    /// Files (relative to `dir`) that must exist for the model to be ready and
    /// that the recognizer loads. Transducer: `[encoder, decoder, joiner, tokens]`;
    /// SenseVoice: `[model, tokens]`.
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qwen3AsrSpec {
    pub repo_id: &'static str,
    pub directory: &'static str,
}

pub fn qwen3_asr_spec(model_id: &str) -> Option<Qwen3AsrSpec> {
    match model_id {
        "qwen3-asr-0.6b" => Some(Qwen3AsrSpec {
            repo_id: "Qwen/Qwen3-ASR-0.6B-hf",
            directory: "qwen3-asr-0.6b",
        }),
        "qwen3-asr-1.7b" => Some(Qwen3AsrSpec {
            repo_id: "Qwen/Qwen3-ASR-1.7B-hf",
            directory: "qwen3-asr-1.7b",
        }),
        _ => None,
    }
}

const QWEN3_ASR_RUNNER: &str = r#"import os
import torch
from transformers import AutoModelForMultimodalLM, AutoProcessor

audio_path = os.environ["WAKENOTE_AUDIO_PATH"]
model_path = os.environ["WAKENOTE_QWEN3_ASR_MODEL_PATH"]
language = os.environ.get("WAKENOTE_QWEN3_ASR_LANGUAGE") or None
prompt = os.environ.get("WAKENOTE_QWEN3_ASR_PROMPT") or None

processor = AutoProcessor.from_pretrained(model_path, local_files_only=True)
model = AutoModelForMultimodalLM.from_pretrained(
    model_path,
    device_map="auto",
    local_files_only=True,
).eval()
inputs = processor.apply_transcription_request(
    audio=audio_path,
    language=language,
    prompt=prompt,
).to(model.device, model.dtype)
with torch.inference_mode():
    output_ids = model.generate(**inputs, max_new_tokens=256, do_sample=False)
generated_ids = output_ids[:, inputs["input_ids"].shape[1]:]
text = processor.decode(
    generated_ids,
    return_format="transcription_only",
)[0]
print(text.strip())
"#;

const QWEN3_ASR_PACKAGES: [&str; 5] = [
    "torch",
    "transformers>=5.13.0,<6",
    "accelerate",
    "librosa",
    "soundfile",
];

impl SherpaModelSpec {
    fn new(kind: SherpaModelKind, dir: &str, files: &[&str]) -> Self {
        Self {
            kind,
            dir: dir.to_string(),
            files: files.iter().map(|file| file.to_string()).collect(),
        }
    }
}

/// The sherpa-onnx model layout for a known model id, or `None` for models that
/// don't run on the bundled sherpa-onnx engine.
pub fn sherpa_model_spec(model_id: &str) -> Option<SherpaModelSpec> {
    match model_id {
        "parakeet-tdt-0.6b-v3" => Some(SherpaModelSpec::new(
            SherpaModelKind::Transducer,
            "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
            &[
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "joiner.int8.onnx",
                "tokens.txt",
            ],
        )),
        "sensevoice-small" => Some(SherpaModelSpec::new(
            SherpaModelKind::SenseVoice,
            "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17",
            &["model.int8.onnx", "tokens.txt"],
        )),
        "nemotron-3.5-asr-streaming-0.6b" => Some(SherpaModelSpec::new(
            SherpaModelKind::OnlineTransducer,
            "sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-1120ms-int8-2026-06-11",
            &[
                "encoder.int8.onnx",
                "decoder.int8.onnx",
                "joiner.int8.onnx",
                "tokens.txt",
            ],
        )),
        _ => None,
    }
}

#[derive(Debug, Error)]
pub enum ModelStoreError {
    #[error("model {0} not found")]
    NotFound(String),
    #[error("model {0} has no download URL")]
    MissingDownloadUrl(String),
    #[error("model download error: {0}")]
    Download(String),
    #[error("model checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error(
        "not enough disk space for model download: need {required_bytes} bytes, have {available_bytes} bytes"
    )]
    InsufficientDiskSpace {
        required_bytes: u64,
        available_bytes: u64,
    },
    #[error("model download cancelled: {model_id}")]
    Cancelled { model_id: String },
    #[error("model registry error: {0}")]
    Registry(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl ModelStore {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: model_directory.as_ref().to_path_buf(),
        }
    }

    /// Directory the sherpa-onnx archive for `model_id` extracts to, or `None`
    /// if the model isn't a sherpa-onnx model.
    pub fn sherpa_model_dir(&self, model_id: &str) -> Option<PathBuf> {
        sherpa_model_spec(model_id).map(|spec| self.model_directory.join(spec.dir))
    }

    pub fn qwen3_asr_root(&self) -> PathBuf {
        self.model_directory.join("qwen3-asr")
    }

    pub fn qwen3_asr_python_path(&self) -> PathBuf {
        self.qwen3_asr_root().join("runtime/bin/python")
    }

    pub fn qwen3_asr_runner_path(&self) -> PathBuf {
        self.qwen3_asr_root().join("wakenote_qwen3_asr.py")
    }

    pub fn qwen3_asr_model_dir(&self, model_id: &str) -> Option<PathBuf> {
        qwen3_asr_spec(model_id)
            .map(|spec| self.qwen3_asr_root().join("models").join(spec.directory))
    }

    fn qwen3_asr_model_ready(&self, model_id: &str) -> bool {
        let Some(model_dir) = self.qwen3_asr_model_dir(model_id) else {
            return false;
        };
        self.qwen3_asr_python_path().is_file()
            && self.qwen3_asr_runner_path().is_file()
            && model_dir.join("config.json").is_file()
            && model_dir.join("model.safetensors").is_file()
            && model_dir.join("processor_config.json").is_file()
            && model_dir.join("tokenizer.json").is_file()
    }

    /// Whether every file the sherpa-onnx model needs is present on disk.
    fn sherpa_model_ready(&self, model_id: &str) -> bool {
        let Some(spec) = sherpa_model_spec(model_id) else {
            return false;
        };
        let dir = self.model_directory.join(&spec.dir);
        spec.files.iter().all(|file| dir.join(file).exists())
    }

    /// Download a sherpa-onnx model `.tar.bz2` and extract it in place so the
    /// bundled engine can run it. Records Downloading/Extracting/Ready/Error so
    /// the UI shows the same progress as a normal download. Extraction uses the
    /// system `tar` (handles bzip2) — no extra Rust dependency.
    pub fn download_and_extract_sherpa_model(
        &self,
        model: &ModelDescriptor,
    ) -> Result<ModelStatus, ModelStoreError> {
        let url = model
            .download_url
            .as_ref()
            .ok_or_else(|| ModelStoreError::MissingDownloadUrl(model.id.clone()))?;
        if sherpa_model_spec(&model.id).is_none() {
            return Err(ModelStoreError::Registry(format!(
                "no sherpa-onnx layout for {}",
                model.id
            )));
        }
        std::fs::create_dir_all(&self.model_directory)?;

        let agent = ureq::AgentBuilder::new().redirects(10).build();
        let response = agent
            .get(url)
            .set(
                "User-Agent",
                &format!("wakenote/{}", env!("CARGO_PKG_VERSION")),
            )
            .call()
            .map_err(|error| ModelStoreError::Download(error.to_string()))
            .inspect_err(|error| self.record_download_error(&model.id, 0, None, error))?;

        let total_bytes = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok());
        let required_bytes =
            total_bytes.unwrap_or_else(|| model.size_mb.saturating_mul(1024 * 1024));
        let available_bytes = self.available_disk_space()?;
        if let Err(error) = Self::validate_download_space(required_bytes, available_bytes) {
            let _ = self.record_download_status(
                &model.id,
                ModelStatus::Error,
                0,
                total_bytes,
                Some(error.to_string()),
            );
            return Err(error);
        }

        let archive_path = self.temp_download_path(&model.id);
        let mut reader = response.into_reader();
        // Stream to disk. On any failure mid-download, clean up the partial
        // archive and record an Error status — otherwise the UI stays stuck on
        // "Downloading" and `prepare_model_download` rejects the retry as active.
        let downloaded_bytes =
            match self.stream_sherpa_archive(&model.id, &mut reader, &archive_path, total_bytes) {
                Ok(bytes) => bytes,
                Err(error) => {
                    let _ = std::fs::remove_file(&archive_path);
                    if !matches!(error, ModelStoreError::Cancelled { .. }) {
                        self.record_download_error(&model.id, 0, total_bytes, &error);
                    }
                    return Err(error);
                }
            };

        let final_total = total_bytes.or(Some(downloaded_bytes));
        self.record_download_status(
            &model.id,
            ModelStatus::Extracting,
            downloaded_bytes,
            final_total,
            None,
        )?;

        self.extract_sherpa_archive_with_tar(
            &model.id,
            &archive_path,
            downloaded_bytes,
            final_total,
            Path::new("/usr/bin/tar"),
        )?;

        if !self.sherpa_model_ready(&model.id) {
            let message = "extracted archive is missing expected model files".to_string();
            let _ = self.record_download_status(
                &model.id,
                ModelStatus::Error,
                downloaded_bytes,
                final_total,
                Some(message.clone()),
            );
            return Err(ModelStoreError::Download(message));
        }

        self.record_download_status(
            &model.id,
            ModelStatus::Ready,
            downloaded_bytes,
            final_total,
            None,
        )?;
        Ok(ModelStatus::Ready)
    }

    /// Stream a sherpa archive response body to `archive_path`, recording
    /// progress and honoring cancellation. Returns the byte count on success.
    /// The caller is responsible for cleaning up `archive_path` on error.
    fn stream_sherpa_archive(
        &self,
        model_id: &str,
        reader: &mut impl Read,
        archive_path: &Path,
        total_bytes: Option<u64>,
    ) -> Result<u64, ModelStoreError> {
        let mut file = std::fs::File::create(archive_path)?;
        let mut buffer = [0_u8; 64 * 1024];
        let mut downloaded_bytes = 0_u64;
        let mut last_recorded = 0_u64;
        self.record_download_progress(model_id, 0, total_bytes)?;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buffer[..read])?;
            downloaded_bytes += read as u64;
            if self.is_download_cancelled(model_id)? {
                return Err(ModelStoreError::Cancelled {
                    model_id: model_id.to_string(),
                });
            }
            if downloaded_bytes.saturating_sub(last_recorded) >= 5_242_880 {
                self.record_download_progress(model_id, downloaded_bytes, total_bytes)?;
                last_recorded = downloaded_bytes;
            }
        }
        std::io::Write::flush(&mut file)?;
        Ok(downloaded_bytes)
    }

    fn extract_sherpa_archive_with_tar(
        &self,
        model_id: &str,
        archive_path: &Path,
        downloaded_bytes: u64,
        final_total: Option<u64>,
        tar_path: &Path,
    ) -> Result<(), ModelStoreError> {
        if self.is_download_cancelled(model_id)? {
            self.cleanup_cancelled_sherpa_extract(model_id, archive_path);
            return Err(ModelStoreError::Cancelled {
                model_id: model_id.to_string(),
            });
        }

        let mut child = Command::new(tar_path)
            .arg("xjf")
            .arg(archive_path)
            .arg("-C")
            .arg(&self.model_directory)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        loop {
            if child.try_wait()?.is_some() {
                break;
            }
            if self.is_download_cancelled(model_id)? {
                let _ = child.kill();
                let _ = child.wait();
                self.cleanup_cancelled_sherpa_extract(model_id, archive_path);
                return Err(ModelStoreError::Cancelled {
                    model_id: model_id.to_string(),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let output = child.wait_with_output()?;
        let _ = std::fs::remove_file(archive_path);

        if self.is_download_cancelled(model_id)? {
            self.cleanup_cancelled_sherpa_extract(model_id, archive_path);
            return Err(ModelStoreError::Cancelled {
                model_id: model_id.to_string(),
            });
        }

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let message = if stderr.is_empty() {
                format!("tar exited with {}", output.status)
            } else {
                stderr
            };
            let _ = self.record_download_status(
                model_id,
                ModelStatus::Error,
                downloaded_bytes,
                final_total,
                Some(message.clone()),
            );
            return Err(ModelStoreError::Download(message));
        }

        Ok(())
    }

    fn cleanup_cancelled_sherpa_extract(&self, model_id: &str, archive_path: &Path) {
        let _ = std::fs::remove_file(archive_path);
        if let Some(sherpa_dir) = self.sherpa_model_dir(model_id) {
            let _ = std::fs::remove_dir_all(sherpa_dir);
        }
    }

    pub fn model_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.bin"))
    }

    pub fn command_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.command"))
    }

    fn temp_download_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.download"))
    }

    pub fn validate_download_space(
        required_bytes: u64,
        available_bytes: u64,
    ) -> Result<(), ModelStoreError> {
        if available_bytes < required_bytes {
            return Err(ModelStoreError::InsufficientDiskSpace {
                required_bytes,
                available_bytes,
            });
        }

        Ok(())
    }

    pub fn download_state_path(&self) -> PathBuf {
        self.model_directory.join("model-downloads.json")
    }

    pub fn registry_path(&self) -> PathBuf {
        self.model_directory.join("model-registry.json")
    }

    pub fn load_model_registry(
        &self,
    ) -> Result<BTreeMap<String, ModelDescriptor>, ModelStoreError> {
        let path = self.registry_path();
        let mut registry = if path.exists() {
            parse_model_registry_json(&std::fs::read_to_string(path)?)?
        } else {
            default_model_registry()
        };
        self.merge_local_whisper_cpp_models(&mut registry)?;
        Ok(registry)
    }

    pub fn load_download_state(&self) -> Result<ModelDownloadState, ModelStoreError> {
        let path = self.download_state_path();
        if !path.exists() {
            return Ok(ModelDownloadState::default());
        }

        serde_json::from_slice(&std::fs::read(path)?)
            .map_err(|error| ModelStoreError::Download(error.to_string()))
    }

    pub fn refresh_statuses(
        &self,
        registry: &mut [ModelDescriptor],
    ) -> Result<(), ModelStoreError> {
        let download_state = self.load_download_state()?;
        for model in registry {
            let verified_status = self.verify_model(model)?;
            model.download_progress = None;
            model.download_error = None;
            match (verified_status, download_state.downloads.get(&model.id)) {
                (ModelStatus::Ready, _) => {
                    model.status = ModelStatus::Ready;
                    model.download_progress = Some(100);
                }
                (_, Some(record))
                    if matches!(
                        record.status,
                        ModelStatus::Downloading
                            | ModelStatus::Verifying
                            | ModelStatus::Extracting
                            | ModelStatus::Error
                    ) =>
                {
                    model.status = record.status;
                    model.download_progress = download_progress_percent(record);
                    model.download_error = record.error.clone();
                }
                _ => {
                    model.status = verified_status;
                }
            };
        }

        Ok(())
    }

    pub fn verify_model(&self, model: &ModelDescriptor) -> Result<ModelStatus, ModelStoreError> {
        if model.provider_runtime == "sherpa-onnx" {
            return Ok(if self.sherpa_model_ready(&model.id) {
                ModelStatus::Ready
            } else {
                ModelStatus::Missing
            });
        }

        if model.provider_runtime == "qwen3-asr" {
            return Ok(if self.qwen3_asr_model_ready(&model.id) {
                ModelStatus::Ready
            } else {
                ModelStatus::Missing
            });
        }

        if model.provider_runtime == "external-command" {
            return Ok(if self.command_path(&model.id).exists() {
                ModelStatus::Ready
            } else {
                ModelStatus::Missing
            });
        }

        let path = self.model_path(&model.id);
        if !path.exists() {
            return Ok(ModelStatus::Missing);
        }

        let Some(expected_checksum) = &model.checksum_sha256 else {
            return Ok(ModelStatus::Ready);
        };

        let bytes = std::fs::read(path)?;
        let actual_checksum = hex_sha256(&bytes);
        if actual_checksum.eq_ignore_ascii_case(expected_checksum) {
            Ok(ModelStatus::Ready)
        } else {
            Ok(ModelStatus::Error)
        }
    }

    pub fn install_qwen3_asr_model(
        &self,
        model: &ModelDescriptor,
    ) -> Result<ModelStatus, ModelStoreError> {
        let spec = qwen3_asr_spec(&model.id).ok_or_else(|| {
            ModelStoreError::Registry(format!("no Qwen3-ASR specification for {}", model.id))
        })?;
        let total_bytes = model.size_mb.saturating_mul(1024 * 1024);
        let result = (|| {
            std::fs::create_dir_all(self.qwen3_asr_root().join("models"))?;
            let required_bytes = total_bytes.saturating_add(2 * 1024 * 1024 * 1024);
            Self::validate_download_space(required_bytes, self.available_disk_space()?)?;
            self.record_download_progress(&model.id, 0, Some(total_bytes))?;
            self.install_qwen3_asr_model_inner(model, &spec, total_bytes)
        })();
        if let Err(error) = &result
            && !matches!(error, ModelStoreError::Cancelled { .. })
        {
            self.record_download_error(&model.id, 0, Some(total_bytes), error);
        }
        result
    }

    fn install_qwen3_asr_model_inner(
        &self,
        model: &ModelDescriptor,
        spec: &Qwen3AsrSpec,
        total_bytes: u64,
    ) -> Result<ModelStatus, ModelStoreError> {
        let runtime_dir = self.qwen3_asr_root().join("runtime");
        let python_path = self.qwen3_asr_python_path();
        let uv_path = find_qwen_uv();

        if !python_path.is_file() {
            if let Some(uv_path) = uv_path.as_ref() {
                let mut command = Command::new(uv_path);
                command
                    .arg("venv")
                    .arg("--python")
                    .arg("3.12")
                    .arg(&runtime_dir);
                self.run_qwen_setup_command(&model.id, &mut command)?;
            } else {
                let python = find_qwen_python().ok_or_else(|| {
                    ModelStoreError::Download(
                        "Qwen3-ASR setup requires uv or Python 3.10+. Install uv, or set WAKENOTE_QWEN3_ASR_PYTHON."
                            .to_string(),
                    )
                })?;
                let mut command = Command::new(python);
                command.arg("-m").arg("venv").arg(&runtime_dir);
                self.run_qwen_setup_command(&model.id, &mut command)?;
            }
        }

        if !python_path.is_file() {
            return Err(ModelStoreError::Download(format!(
                "Qwen3-ASR Python environment was not created at {}",
                python_path.display()
            )));
        }

        if let Some(uv_path) = uv_path.as_ref() {
            let mut command = Command::new(uv_path);
            command
                .arg("pip")
                .arg("install")
                .arg("--python")
                .arg(&python_path)
                .args(QWEN3_ASR_PACKAGES);
            self.run_qwen_setup_command(&model.id, &mut command)?;
        } else {
            let mut command = Command::new(&python_path);
            command
                .arg("-m")
                .arg("pip")
                .arg("install")
                .args(QWEN3_ASR_PACKAGES);
            self.run_qwen_setup_command(&model.id, &mut command)?;
        }

        std::fs::write(self.qwen3_asr_runner_path(), QWEN3_ASR_RUNNER)?;
        self.record_download_status(
            &model.id,
            ModelStatus::Downloading,
            total_bytes / 20,
            Some(total_bytes),
            None,
        )?;

        let model_dir = self
            .qwen3_asr_model_dir(&model.id)
            .ok_or_else(|| ModelStoreError::NotFound(model.id.clone()))?;
        std::fs::create_dir_all(&model_dir)?;
        let download_script = concat!(
            "from huggingface_hub import snapshot_download\n",
            "import sys\n",
            "snapshot_download(repo_id=sys.argv[1], local_dir=sys.argv[2])\n"
        );
        let mut command = Command::new(&python_path);
        command
            .arg("-c")
            .arg(download_script)
            .arg(spec.repo_id)
            .arg(&model_dir);
        self.run_qwen_setup_command(&model.id, &mut command)?;

        self.record_download_status(
            &model.id,
            ModelStatus::Verifying,
            total_bytes,
            Some(total_bytes),
            None,
        )?;
        let status = self.verify_model(model)?;
        let error = (status != ModelStatus::Ready)
            .then(|| "Qwen3-ASR runtime or model files are incomplete".to_string());
        self.record_download_status(&model.id, status, total_bytes, Some(total_bytes), error)?;
        if status != ModelStatus::Ready {
            return Err(ModelStoreError::Download(
                "Qwen3-ASR setup verification failed".to_string(),
            ));
        }
        Ok(status)
    }

    fn run_qwen_setup_command(
        &self,
        model_id: &str,
        command: &mut Command,
    ) -> Result<(), ModelStoreError> {
        let log_path = self
            .qwen3_asr_root()
            .join(format!("{model_id}.install.log"));
        let log = std::fs::File::create(&log_path)?;
        let stderr = log.try_clone()?;
        let mut child = command
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| {
                ModelStoreError::Download(format!("could not start Qwen3-ASR setup: {error}"))
            })?;
        loop {
            if let Some(status) = child.try_wait()? {
                if status.success() {
                    return Ok(());
                }
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                let tail = log
                    .chars()
                    .rev()
                    .take(4_000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>();
                return Err(ModelStoreError::Download(if tail.trim().is_empty() {
                    format!("Qwen3-ASR setup exited with {status}")
                } else {
                    tail.trim().to_string()
                }));
            }
            if self.is_download_cancelled(model_id)? {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ModelStoreError::Cancelled {
                    model_id: model_id.to_string(),
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn merge_local_whisper_cpp_models(
        &self,
        registry: &mut BTreeMap<String, ModelDescriptor>,
    ) -> Result<(), ModelStoreError> {
        if !self.model_directory.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.model_directory)? {
            let entry = entry?;
            let Some(model) = infer_local_whisper_cpp_model(&entry.path())? else {
                continue;
            };
            registry.entry(model.id.clone()).or_insert(model);
        }

        Ok(())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<(), ModelStoreError> {
        let path = self.model_path(model_id);
        let command_path = self.command_path(model_id);
        let temp_path = self.temp_download_path(model_id);
        let had_download_record = self.clear_download_record(model_id)?;
        let mut removed_anything = had_download_record;

        if path.exists() {
            std::fs::remove_file(path)?;
            removed_anything = true;
        }
        if temp_path.exists() {
            std::fs::remove_file(temp_path)?;
            removed_anything = true;
        }
        if command_path.exists() {
            std::fs::remove_file(command_path)?;
            removed_anything = true;
        }
        if let Some(qwen_dir) = self.qwen3_asr_model_dir(model_id)
            && qwen_dir.is_dir()
        {
            std::fs::remove_dir_all(qwen_dir)?;
            removed_anything = true;
        }
        // sherpa-onnx models extract to a directory rather than a single file.
        if let Some(sherpa_dir) = self.sherpa_model_dir(model_id) {
            if sherpa_dir.is_dir() {
                std::fs::remove_dir_all(sherpa_dir)?;
                removed_anything = true;
            }
        }

        if removed_anything {
            Ok(())
        } else {
            Err(ModelStoreError::NotFound(model_id.to_string()))
        }
    }

    pub fn install_model_bytes(
        &self,
        model: &ModelDescriptor,
        bytes: &[u8],
    ) -> Result<(), ModelStoreError> {
        self.install_model_reader(model, bytes)
    }

    pub fn install_model_reader(
        &self,
        model: &ModelDescriptor,
        reader: impl Read,
    ) -> Result<(), ModelStoreError> {
        self.install_model_reader_inner(model, reader, None, false)
            .map(|_| ())
    }

    fn install_model_reader_inner(
        &self,
        model: &ModelDescriptor,
        mut reader: impl Read,
        total_bytes: Option<u64>,
        track_progress: bool,
    ) -> Result<u64, ModelStoreError> {
        std::fs::create_dir_all(&self.model_directory)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let path = self.model_path(&model.id);
        let temp_path = self.temp_download_path(&model.id);
        // If a previous partial overshoots the expected total, the resume offset is
        // meaningless — start fresh so we never hash bytes we no longer trust.
        if track_progress
            && temp_path.exists()
            && let Some(total) = total_bytes
            && let Ok(metadata) = std::fs::metadata(&temp_path)
            && metadata.len() > total
        {
            let _ = std::fs::remove_file(&temp_path);
        }
        let resume_download = track_progress && temp_path.exists();
        let mut downloaded_bytes = 0_u64;
        if resume_download {
            let mut existing = std::fs::File::open(&temp_path)?;
            loop {
                let read = existing.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                downloaded_bytes += read as u64;
            }
        }

        let mut file = if resume_download {
            std::fs::OpenOptions::new().append(true).open(&temp_path)?
        } else {
            std::fs::File::create(&temp_path)?
        };
        let mut last_recorded_bytes = downloaded_bytes;
        if track_progress {
            self.record_download_progress(&model.id, downloaded_bytes, total_bytes)?;
        }

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            std::io::Write::write_all(&mut file, &buffer[..read])?;
            downloaded_bytes += read as u64;
            if track_progress && self.is_download_cancelled(&model.id)? {
                let _ = std::fs::remove_file(&temp_path);
                return Err(ModelStoreError::Cancelled {
                    model_id: model.id.clone(),
                });
            }
            if track_progress && downloaded_bytes.saturating_sub(last_recorded_bytes) >= 5_242_880 {
                self.record_download_progress(&model.id, downloaded_bytes, total_bytes)?;
                last_recorded_bytes = downloaded_bytes;
            }
        }
        std::io::Write::flush(&mut file)?;
        if track_progress && downloaded_bytes != last_recorded_bytes {
            self.record_download_progress(&model.id, downloaded_bytes, total_bytes)?;
        }

        let actual = hex_digest(hasher.finalize());
        if let Some(expected) = &model.checksum_sha256 {
            if !actual.eq_ignore_ascii_case(expected) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(ModelStoreError::ChecksumMismatch {
                    expected: expected.clone(),
                    actual,
                });
            }
        }

        std::fs::rename(temp_path, path)?;
        Ok(downloaded_bytes)
    }

    pub fn record_download_progress(
        &self,
        model_id: impl Into<String>,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<ModelDownloadRecord, ModelStoreError> {
        self.record_download(
            model_id,
            ModelStatus::Downloading,
            downloaded_bytes,
            total_bytes,
            None,
        )
    }

    pub fn record_download_status(
        &self,
        model_id: impl Into<String>,
        status: ModelStatus,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        error: Option<String>,
    ) -> Result<ModelDownloadRecord, ModelStoreError> {
        self.record_download(model_id, status, downloaded_bytes, total_bytes, error)
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<ModelDownloadRecord, ModelStoreError> {
        let state = self.load_download_state()?;
        let existing = state.downloads.get(model_id);
        self.record_download_status(
            model_id,
            ModelStatus::Error,
            existing.map(|record| record.downloaded_bytes).unwrap_or(0),
            existing.and_then(|record| record.total_bytes),
            Some("cancelled by user".to_string()),
        )
    }

    pub fn cancel_active_download(&self) -> Result<Option<ModelDownloadRecord>, ModelStoreError> {
        let state = self.load_download_state()?;
        let active_model_id = state
            .downloads
            .iter()
            .find(|(_, record)| {
                matches!(
                    record.status,
                    ModelStatus::Downloading | ModelStatus::Verifying | ModelStatus::Extracting
                )
            })
            .map(|(model_id, _)| model_id.clone());

        active_model_id
            .map(|model_id| self.cancel_download(&model_id))
            .transpose()
    }

    pub fn download_model_with<R: Read>(
        &self,
        model: &ModelDescriptor,
        fetch: impl FnOnce(&ModelDescriptor) -> Result<R, ModelStoreError>,
    ) -> Result<ModelStatus, ModelStoreError> {
        let reader = match fetch(model) {
            Ok(reader) => reader,
            Err(error) => {
                self.record_download_error(&model.id, 0, None, &error);
                return Err(error);
            }
        };
        self.download_model_reader(model, reader, None)
    }

    pub fn download_model(&self, model: &ModelDescriptor) -> Result<ModelStatus, ModelStoreError> {
        let url = model
            .download_url
            .as_ref()
            .ok_or_else(|| ModelStoreError::MissingDownloadUrl(model.id.clone()))?;
        let resume_from = self.partial_download_bytes(&model.id)?;
        let agent = ureq::AgentBuilder::new().redirects(10).build();
        let mut request = agent.get(url).set(
            "User-Agent",
            &format!("wakenote/{}", env!("CARGO_PKG_VERSION")),
        );
        if resume_from > 0 {
            request = request.set("Range", &format!("bytes={resume_from}-"));
        }
        let response = request
            .call()
            .map_err(|error| ModelStoreError::Download(error.to_string()))
            .inspect_err(|error| {
                self.record_download_error(&model.id, resume_from, None, error);
            })?;
        let status = response.status();
        let content_length = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok());
        let total_bytes = if resume_from > 0 && status == 206 {
            content_length.map(|value| value.saturating_add(resume_from))
        } else {
            if resume_from > 0 {
                let _ = std::fs::remove_file(self.temp_download_path(&model.id));
            }
            content_length
        };
        self.download_model_reader(model, response.into_reader(), total_bytes)
    }

    fn download_model_reader(
        &self,
        model: &ModelDescriptor,
        reader: impl Read,
        total_bytes: Option<u64>,
    ) -> Result<ModelStatus, ModelStoreError> {
        let required_bytes =
            total_bytes.unwrap_or_else(|| model.size_mb.saturating_mul(1024 * 1024));
        let available_bytes = self.available_disk_space()?;
        if let Err(error) = Self::validate_download_space(required_bytes, available_bytes) {
            let _ = self.record_download_status(
                &model.id,
                ModelStatus::Error,
                0,
                total_bytes,
                Some(error.to_string()),
            );
            return Err(error);
        }

        match self.install_model_reader_inner(model, reader, total_bytes, true) {
            Ok(downloaded_bytes) => {
                let final_total = total_bytes.or(Some(downloaded_bytes));
                self.record_download_status(
                    &model.id,
                    ModelStatus::Verifying,
                    downloaded_bytes,
                    final_total,
                    None,
                )?;
                let status = self.verify_model(model)?;
                self.record_download_status(
                    &model.id,
                    status,
                    downloaded_bytes,
                    final_total,
                    if status == ModelStatus::Error {
                        Some("model verification failed".to_string())
                    } else {
                        None
                    },
                )?;
                Ok(status)
            }
            Err(error) => {
                self.record_download_error(&model.id, 0, total_bytes, &error);
                Err(error)
            }
        }
    }

    fn record_download_error(
        &self,
        model_id: &str,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        error: &ModelStoreError,
    ) {
        let _ = self.record_download_status(
            model_id,
            ModelStatus::Error,
            downloaded_bytes,
            total_bytes,
            Some(download_error_message(error)),
        );
    }
}

fn download_error_message(error: &ModelStoreError) -> String {
    match error {
        ModelStoreError::Cancelled { .. } => "cancelled by user".to_string(),
        ModelStoreError::Download(message) => message.clone(),
        _ => error.to_string(),
    }
}

fn find_qwen_uv() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("WAKENOTE_QWEN3_ASR_UV") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/uv"),
        PathBuf::from("/usr/local/bin/uv"),
    ]);
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/uv"));
    }
    candidates.push(PathBuf::from("uv"));

    candidates.into_iter().find(|path| {
        Command::new(path)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

fn find_qwen_python() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("WAKENOTE_QWEN3_ASR_PYTHON") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/python3"),
        PathBuf::from("/usr/local/bin/python3"),
        PathBuf::from("python3"),
    ]);

    candidates.into_iter().find(|path| {
        Command::new(path)
            .arg("-c")
            .arg("import sys; raise SystemExit(sys.version_info < (3, 10))")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

impl ModelStore {
    fn available_disk_space(&self) -> Result<u64, ModelStoreError> {
        std::fs::create_dir_all(&self.model_directory)?;
        let output = Command::new("/bin/df")
            .arg("-Pk")
            .arg(&self.model_directory)
            .output()?;
        if !output.status.success() {
            return Err(ModelStoreError::Download(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .nth(1)
            .ok_or_else(|| ModelStoreError::Download("unable to parse df output".to_string()))?;
        let available_kib = line
            .split_whitespace()
            .nth(3)
            .ok_or_else(|| ModelStoreError::Download("unable to parse df output".to_string()))?
            .parse::<u64>()
            .map_err(|error| ModelStoreError::Download(error.to_string()))?;
        Ok(available_kib.saturating_mul(1024))
    }

    fn is_download_cancelled(&self, model_id: &str) -> Result<bool, ModelStoreError> {
        let state = self.load_download_state()?;
        Ok(state
            .downloads
            .get(model_id)
            .is_some_and(|record| record.error.as_deref() == Some("cancelled by user")))
    }

    fn partial_download_bytes(&self, model_id: &str) -> Result<u64, ModelStoreError> {
        let path = self.temp_download_path(model_id);
        if !path.exists() {
            return Ok(0);
        }

        Ok(std::fs::metadata(path)?.len())
    }

    fn save_download_state(&self, state: &ModelDownloadState) -> Result<(), ModelStoreError> {
        std::fs::create_dir_all(&self.model_directory)?;
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| ModelStoreError::Download(error.to_string()))?;
        std::fs::write(self.download_state_path(), bytes)?;
        Ok(())
    }

    fn clear_download_record(&self, model_id: &str) -> Result<bool, ModelStoreError> {
        let mut state = self.load_download_state()?;
        let removed = state.downloads.remove(model_id).is_some();
        if removed {
            self.save_download_state(&state)?;
        }
        Ok(removed)
    }

    fn record_download(
        &self,
        model_id: impl Into<String>,
        status: ModelStatus,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        error: Option<String>,
    ) -> Result<ModelDownloadRecord, ModelStoreError> {
        let model_id = model_id.into();
        let mut state = self.load_download_state()?;
        let record = ModelDownloadRecord {
            model_id: model_id.clone(),
            status,
            downloaded_bytes,
            total_bytes,
            error,
        };
        state.downloads.insert(model_id, record.clone());
        self.save_download_state(&state)?;
        Ok(record)
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_digest(digest)
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn download_progress_percent(record: &ModelDownloadRecord) -> Option<u8> {
    let total = record.total_bytes?;
    if total == 0 {
        return Some(0);
    }

    Some(
        ((record.downloaded_bytes as f64 / total as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8,
    )
}

pub fn parse_model_registry_json(
    json: &str,
) -> Result<BTreeMap<String, ModelDescriptor>, ModelStoreError> {
    let entries: Vec<ModelRegistryEntry> =
        serde_json::from_str(json).map_err(|error| ModelStoreError::Registry(error.to_string()))?;
    let mut registry = BTreeMap::new();
    for entry in entries {
        registry.insert(entry.id.clone(), descriptor_from_registry_entry(entry));
    }
    Ok(registry)
}

fn descriptor_from_registry_entry(entry: ModelRegistryEntry) -> ModelDescriptor {
    ModelDescriptor {
        id: entry.id,
        display_name: entry.display_name,
        engine: entry.engine,
        provider_runtime: entry.provider_runtime,
        download_url: entry.download_url,
        checksum_sha256: entry.checksum_sha256,
        size_mb: entry.size_mb,
        languages: entry.languages,
        speed_score: entry.speed_score,
        accuracy_score: entry.accuracy_score,
        offline: entry.offline,
        status: ModelStatus::Missing,
        download_progress: None,
        download_error: None,
    }
}

fn infer_local_whisper_cpp_model(path: &Path) -> Result<Option<ModelDescriptor>, ModelStoreError> {
    if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("bin") {
        return Ok(None);
    }

    let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
        return Ok(None);
    };
    if stem.is_empty() || stem.starts_with("._") {
        return Ok(None);
    }

    let size_mb = std::fs::metadata(path)?
        .len()
        .saturating_add(1024 * 1024 - 1)
        / (1024 * 1024);
    Ok(Some(ModelDescriptor {
        id: stem.to_string(),
        display_name: humanize_local_model_id(stem),
        engine: "whisper.cpp".to_string(),
        provider_runtime: "whisper-rs".to_string(),
        download_url: None,
        checksum_sha256: None,
        size_mb,
        languages: vec!["ko".to_string(), "en".to_string(), "multi".to_string()],
        speed_score: 6,
        accuracy_score: 7,
        offline: true,
        status: ModelStatus::Missing,
        download_progress: None,
        download_error: None,
    }))
}

fn humanize_local_model_id(model_id: &str) -> String {
    model_id
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_uppercase().collect::<String>(),
                    chars.collect::<String>()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn default_model_registry() -> BTreeMap<String, ModelDescriptor> {
    let mut registry = BTreeMap::new();

    registry.insert(
        "whisper-small".to_string(),
        ModelDescriptor {
            id: "whisper-small".to_string(),
            display_name: "Whisper Small".to_string(),
            engine: "whisper.cpp".to_string(),
            provider_runtime: "whisper-rs".to_string(),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
                    .to_string(),
            ),
            checksum_sha256: Some(
                "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b".to_string(),
            ),
            size_mb: 465,
            languages: vec!["ko".to_string(), "en".to_string(), "multi".to_string()],
            speed_score: 8,
            accuracy_score: 6,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry.insert(
        "whisper-medium".to_string(),
        ModelDescriptor {
            id: "whisper-medium".to_string(),
            display_name: "Whisper Medium".to_string(),
            engine: "whisper.cpp".to_string(),
            provider_runtime: "whisper-rs".to_string(),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
                    .to_string(),
            ),
            checksum_sha256: Some(
                "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208".to_string(),
            ),
            size_mb: 1_465,
            languages: vec!["ko".to_string(), "en".to_string(), "multi".to_string()],
            speed_score: 5,
            accuracy_score: 8,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry.insert(
        "whisper-turbo".to_string(),
        ModelDescriptor {
            id: "whisper-turbo".to_string(),
            display_name: "Whisper Turbo".to_string(),
            engine: "whisper.cpp".to_string(),
            provider_runtime: "whisper-rs".to_string(),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
                    .to_string(),
            ),
            checksum_sha256: Some(
                "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69".to_string(),
            ),
            size_mb: 1_550,
            languages: vec!["ko".to_string(), "en".to_string(), "multi".to_string()],
            speed_score: 8,
            accuracy_score: 7,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry.insert(
        "whisper-large".to_string(),
        ModelDescriptor {
            id: "whisper-large".to_string(),
            display_name: "Whisper Large".to_string(),
            engine: "whisper.cpp".to_string(),
            provider_runtime: "whisper-rs".to_string(),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin"
                    .to_string(),
            ),
            checksum_sha256: Some(
                "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1".to_string(),
            ),
            size_mb: 1_031,
            languages: vec!["ko".to_string(), "en".to_string(), "multi".to_string()],
            speed_score: 3,
            accuracy_score: 9,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry.insert(
        "parakeet-tdt-0.6b-v3".to_string(),
        ModelDescriptor {
            id: "parakeet-tdt-0.6b-v3".to_string(),
            display_name: "Parakeet TDT 0.6B V3".to_string(),
            engine: "NVIDIA".to_string(),
            provider_runtime: "sherpa-onnx".to_string(),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2"
                    .to_string(),
            ),
            checksum_sha256: None,
            size_mb: 660,
            languages: vec!["en".to_string(), "multi".to_string()],
            speed_score: 8,
            accuracy_score: 8,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry.insert(
        "sensevoice-small".to_string(),
        ModelDescriptor {
            id: "sensevoice-small".to_string(),
            display_name: "SenseVoice Small".to_string(),
            engine: "SenseVoice".to_string(),
            provider_runtime: "sherpa-onnx".to_string(),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2"
                    .to_string(),
            ),
            checksum_sha256: None,
            size_mb: 250,
            languages: vec![
                "ko".to_string(),
                "en".to_string(),
                "ja".to_string(),
                "zh".to_string(),
                "multi".to_string(),
            ],
            speed_score: 9,
            accuracy_score: 8,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    // Nemotron 3.5 ASR runs in-process via the bundled sherpa-onnx engine, using
    // the cache-aware streaming transducer export (the 1120ms-chunk int8 variant,
    // favoring accuracy on recorded audio). It downloads/extracts like the other
    // sherpa-onnx models but is decoded by the online recognizer (see
    // `transcribe_with_online_transducer`).
    registry.insert(
        "nemotron-3.5-asr-streaming-0.6b".to_string(),
        ModelDescriptor {
            id: "nemotron-3.5-asr-streaming-0.6b".to_string(),
            display_name: "Nemotron 3.5 ASR Streaming 0.6B".to_string(),
            engine: "NVIDIA".to_string(),
            provider_runtime: "sherpa-onnx".to_string(),
            download_url: Some(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-1120ms-int8-2026-06-11.tar.bz2"
                    .to_string(),
            ),
            checksum_sha256: None,
            size_mb: 650,
            languages: vec![
                "ko".to_string(),
                "en".to_string(),
                "ja".to_string(),
                "zh".to_string(),
                "es".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "multi".to_string(),
            ],
            speed_score: 8,
            accuracy_score: 8,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    for (id, display_name, size_mb, speed_score, accuracy_score) in [
        ("qwen3-asr-0.6b", "Qwen3-ASR 0.6B", 1_505, 7, 9),
        ("qwen3-asr-1.7b", "Qwen3-ASR 1.7B", 3_900, 4, 10),
    ] {
        registry.insert(
            id.to_string(),
            ModelDescriptor {
                id: id.to_string(),
                display_name: display_name.to_string(),
                engine: "Qwen".to_string(),
                provider_runtime: "qwen3-asr".to_string(),
                download_url: None,
                checksum_sha256: None,
                size_mb,
                languages: vec![
                    "ko".to_string(),
                    "en".to_string(),
                    "ja".to_string(),
                    "zh".to_string(),
                    "es".to_string(),
                    "fr".to_string(),
                    "de".to_string(),
                    "multi".to_string(),
                ],
                speed_score,
                accuracy_score,
                offline: true,
                status: ModelStatus::Missing,
                download_progress: None,
                download_error: None,
            },
        );
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write as _};
    use std::net::TcpListener;
    use std::thread;

    const WHISPER_SMALL_SHA256: &str =
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b";
    const WHISPER_MEDIUM_SHA256: &str =
        "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208";
    const WHISPER_TURBO_SHA256: &str =
        "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69";
    const WHISPER_LARGE_SHA256: &str =
        "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1";
    // Pre-fix bogus value that shipped to users; never reintroduce.
    const WHISPER_MEDIUM_BOGUS_SHA256: &str =
        "6c14d5adee4f86394037d23e1625d96385c22f032d72d6fdf045dc1741ca091e";

    fn descriptor(id: &str, checksum_sha256: Option<&str>) -> ModelDescriptor {
        ModelDescriptor {
            id: id.to_string(),
            display_name: id.to_string(),
            engine: "whisper.cpp".to_string(),
            provider_runtime: "whisper-rs".to_string(),
            download_url: None,
            checksum_sha256: checksum_sha256.map(str::to_string),
            size_mb: 1,
            languages: vec!["en".to_string()],
            speed_score: 5,
            accuracy_score: 5,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        }
    }

    fn sherpa_descriptor(id: &str, download_url: String) -> ModelDescriptor {
        ModelDescriptor {
            id: id.to_string(),
            display_name: id.to_string(),
            engine: "sherpa-onnx".to_string(),
            provider_runtime: "sherpa-onnx".to_string(),
            download_url: Some(download_url),
            checksum_sha256: None,
            size_mb: 1,
            languages: vec!["ko".to_string(), "en".to_string()],
            speed_score: 7,
            accuracy_score: 7,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        }
    }

    #[cfg(unix)]
    fn build_sherpa_archive(model_id: &str) -> Vec<u8> {
        let tmp = tempfile::tempdir().expect("archive tempdir");
        let spec = sherpa_model_spec(model_id).expect("sherpa model spec");
        let model_root = tmp.path().join(&spec.dir);
        std::fs::create_dir_all(&model_root).expect("model root");
        for file in &spec.files {
            let path = model_root.join(file);
            std::fs::create_dir_all(path.parent().expect("file parent")).expect("file parent dir");
            std::fs::write(path, b"fixture").expect("model fixture file");
        }

        let archive = tmp.path().join("model.tar.bz2");
        let output = Command::new("/usr/bin/tar")
            .arg("cjf")
            .arg(&archive)
            .arg("-C")
            .arg(tmp.path())
            .arg(&spec.dir)
            .output()
            .expect("create tar archive");
        assert!(
            output.status.success(),
            "tar failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read(archive).expect("archive bytes")
    }

    fn serve_once(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        let addr = listener.local_addr().expect("fixture server address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/x-bzip2\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).expect("write headers");
            stream.write_all(&body).expect("write body");
        });
        format!("http://{addr}/model.tar.bz2")
    }

    /// Serve a response that advertises `declared_len` bytes but sends only
    /// `body`, then closes the connection — the client's body read fails partway.
    fn serve_truncated(declared_len: usize, body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        let addr = listener.local_addr().expect("fixture server address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept fixture request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let headers = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {declared_len}\r\nContent-Type: application/x-bzip2\r\nConnection: close\r\n\r\n",
            );
            stream.write_all(headers.as_bytes()).expect("write headers");
            let _ = stream.write_all(&body);
        });
        format!("http://{addr}/model.tar.bz2")
    }

    fn is_lowercase_hex_64(value: &str) -> bool {
        value.len() == 64
            && value
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    }

    #[test]
    fn default_registry_uses_pinned_remote_sha256() {
        let registry = default_model_registry();
        let whisper_models = registry
            .values()
            .filter(|model| model.provider_runtime == "whisper-rs")
            .count();
        assert_eq!(
            whisper_models, 4,
            "registry should ship four pinned Whisper models",
        );

        let small = registry.get("whisper-small").expect("whisper-small entry");
        let small_hash = small
            .checksum_sha256
            .as_deref()
            .expect("whisper-small must have a checksum");
        assert!(
            is_lowercase_hex_64(small_hash),
            "small checksum must be 64 lowercase hex chars: {small_hash}",
        );
        assert_eq!(small_hash, WHISPER_SMALL_SHA256);

        let medium = registry
            .get("whisper-medium")
            .expect("whisper-medium entry");
        let medium_hash = medium
            .checksum_sha256
            .as_deref()
            .expect("whisper-medium must have a checksum");
        assert!(
            is_lowercase_hex_64(medium_hash),
            "medium checksum must be 64 lowercase hex chars: {medium_hash}",
        );
        assert_eq!(medium_hash, WHISPER_MEDIUM_SHA256);
        assert_ne!(
            medium_hash, WHISPER_MEDIUM_BOGUS_SHA256,
            "regression: pre-fix bogus medium checksum must never reappear",
        );

        let turbo = registry.get("whisper-turbo").expect("whisper-turbo entry");
        let turbo_hash = turbo
            .checksum_sha256
            .as_deref()
            .expect("whisper-turbo must have a checksum");
        assert!(
            is_lowercase_hex_64(turbo_hash),
            "turbo checksum must be 64 lowercase hex chars: {turbo_hash}",
        );
        assert_eq!(turbo_hash, WHISPER_TURBO_SHA256);

        let large = registry.get("whisper-large").expect("whisper-large entry");
        let large_hash = large
            .checksum_sha256
            .as_deref()
            .expect("whisper-large must have a checksum");
        assert!(
            is_lowercase_hex_64(large_hash),
            "large checksum must be 64 lowercase hex chars: {large_hash}",
        );
        assert_eq!(large_hash, WHISPER_LARGE_SHA256);
    }

    #[test]
    fn install_model_reader_discards_oversized_partial_before_hashing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        // SHA256 of 50 zero bytes — what we expect when the stale 100-byte partial is dropped.
        let fresh_payload = vec![0_u8; 50];
        let expected = hex_sha256(&fresh_payload);
        let model = descriptor("whisper-test", Some(&expected));

        std::fs::create_dir_all(tmp.path()).expect("models dir");
        std::fs::write(store.temp_download_path("whisper-test"), vec![0xFF_u8; 100])
            .expect("stale partial");

        let downloaded = store
            .install_model_reader_inner(
                &model,
                Cursor::new(fresh_payload.clone()),
                Some(fresh_payload.len() as u64),
                true,
            )
            .expect("install discards stale partial");

        assert_eq!(downloaded, fresh_payload.len() as u64);
        let installed = std::fs::read(store.model_path("whisper-test")).expect("installed model");
        assert_eq!(installed, fresh_payload);
        assert!(!store.temp_download_path("whisper-test").exists());
    }

    #[test]
    fn default_registry_uses_sherpa_onnx_for_parakeet_and_sensevoice() {
        let registry = default_model_registry();

        let parakeet = registry
            .get("parakeet-tdt-0.6b-v3")
            .expect("parakeet entry");
        assert_eq!(parakeet.provider_runtime, "sherpa-onnx");
        assert!(
            parakeet
                .download_url
                .as_deref()
                .is_some_and(|url| url.ends_with(".tar.bz2")),
            "parakeet should download a sherpa-onnx archive",
        );

        let sensevoice = registry.get("sensevoice-small").expect("sensevoice entry");
        assert_eq!(sensevoice.provider_runtime, "sherpa-onnx");
        assert!(sensevoice.download_url.is_some());

        // Nemotron 3.5 ASR now downloads a streaming sherpa-onnx export and runs
        // in-process via the online recognizer.
        let nemotron = registry
            .get("nemotron-3.5-asr-streaming-0.6b")
            .expect("nemotron entry");
        assert_eq!(nemotron.provider_runtime, "sherpa-onnx");
        assert!(
            nemotron
                .download_url
                .as_deref()
                .is_some_and(|url| url.ends_with(".tar.bz2")),
            "nemotron should download a sherpa-onnx archive",
        );
    }

    #[test]
    fn default_registry_includes_official_qwen3_asr_models() {
        assert!(QWEN3_ASR_PACKAGES.contains(&"torch"));
        assert!(QWEN3_ASR_PACKAGES.contains(&"transformers>=5.13.0,<6"));
        let registry = default_model_registry();

        let small = registry
            .get("qwen3-asr-0.6b")
            .expect("Qwen3-ASR 0.6B entry");
        assert_eq!(small.provider_runtime, "qwen3-asr");
        assert_eq!(small.engine, "Qwen");
        assert!(small.download_url.is_none());
        assert!(small.languages.iter().any(|language| language == "ko"));

        let large = registry
            .get("qwen3-asr-1.7b")
            .expect("Qwen3-ASR 1.7B entry");
        assert_eq!(large.provider_runtime, "qwen3-asr");
        assert!(large.accuracy_score > small.accuracy_score);

        let spec = qwen3_asr_spec("qwen3-asr-0.6b").expect("Qwen spec");
        assert_eq!(spec.repo_id, "Qwen/Qwen3-ASR-0.6B-hf");
        assert!(qwen3_asr_spec("whisper-small").is_none());
    }

    #[test]
    fn qwen3_asr_is_ready_only_with_runtime_and_complete_local_snapshot() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        let model = default_model_registry()
            .remove("qwen3-asr-0.6b")
            .expect("Qwen model");
        assert_eq!(
            store.verify_model(&model).expect("missing status"),
            ModelStatus::Missing
        );

        let model_dir = store
            .qwen3_asr_model_dir(&model.id)
            .expect("Qwen model directory");
        std::fs::create_dir_all(store.qwen3_asr_python_path().parent().expect("runtime bin"))
            .expect("runtime directory");
        std::fs::create_dir_all(&model_dir).expect("model directory");
        std::fs::write(store.qwen3_asr_python_path(), b"python").expect("python marker");
        std::fs::write(store.qwen3_asr_runner_path(), b"runner").expect("runner marker");
        for file in [
            "config.json",
            "model.safetensors",
            "processor_config.json",
            "tokenizer.json",
        ] {
            std::fs::write(model_dir.join(file), b"ready").expect("model marker");
        }

        assert_eq!(
            store.verify_model(&model).expect("ready status"),
            ModelStatus::Ready
        );
    }

    #[test]
    fn sherpa_model_spec_describes_known_models() {
        let parakeet = sherpa_model_spec("parakeet-tdt-0.6b-v3").expect("parakeet spec");
        assert_eq!(parakeet.kind, SherpaModelKind::Transducer);
        assert!(parakeet.files.iter().any(|f| f == "encoder.int8.onnx"));
        assert!(parakeet.files.iter().any(|f| f == "tokens.txt"));

        let sensevoice = sherpa_model_spec("sensevoice-small").expect("sensevoice spec");
        assert_eq!(sensevoice.kind, SherpaModelKind::SenseVoice);
        assert!(sensevoice.files.iter().any(|f| f == "model.int8.onnx"));

        let nemotron = sherpa_model_spec("nemotron-3.5-asr-streaming-0.6b").expect("nemotron spec");
        assert_eq!(nemotron.kind, SherpaModelKind::OnlineTransducer);
        assert!(nemotron.files.iter().any(|f| f == "encoder.int8.onnx"));
        assert!(nemotron.files.iter().any(|f| f == "tokens.txt"));

        assert!(sherpa_model_spec("whisper-small").is_none());
    }

    #[test]
    fn sherpa_model_ready_only_when_extracted_files_exist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        let model = ModelDescriptor {
            provider_runtime: "sherpa-onnx".to_string(),
            ..descriptor("parakeet-tdt-0.6b-v3", None)
        };

        assert_eq!(
            store.verify_model(&model).expect("verify before"),
            ModelStatus::Missing
        );

        let spec = sherpa_model_spec("parakeet-tdt-0.6b-v3").expect("spec");
        let dir = store
            .sherpa_model_dir("parakeet-tdt-0.6b-v3")
            .expect("sherpa dir");
        std::fs::create_dir_all(&dir).expect("model dir");
        for file in &spec.files {
            std::fs::write(dir.join(file), b"x").expect("model file");
        }

        assert_eq!(
            store.verify_model(&model).expect("verify after"),
            ModelStatus::Ready
        );
    }

    #[test]
    fn delete_model_removes_extracted_sherpa_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        let dir = store
            .sherpa_model_dir("sensevoice-small")
            .expect("sherpa dir");
        std::fs::create_dir_all(&dir).expect("model dir");
        std::fs::write(dir.join("model.int8.onnx"), b"x").expect("model file");

        store.delete_model("sensevoice-small").expect("delete");
        assert!(!dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn download_and_extract_sherpa_model_accepts_http_archive_fixture() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path().join("models"));
        let model_id = "sensevoice-small";
        let url = serve_once(build_sherpa_archive(model_id));
        let model = sherpa_descriptor(model_id, url);

        let status = store
            .download_and_extract_sherpa_model(&model)
            .expect("download and extract sherpa model");

        assert_eq!(status, ModelStatus::Ready);
        assert_eq!(
            store.verify_model(&model).expect("verify extracted model"),
            ModelStatus::Ready
        );
        assert!(!store.temp_download_path(model_id).exists());
        let state = store.load_download_state().expect("download state");
        let record = state
            .downloads
            .get(model_id)
            .expect("ready download record");
        assert_eq!(record.status, ModelStatus::Ready);
        assert_eq!(record.download_progress_percent(), Some(100));
    }

    #[test]
    fn sherpa_download_failure_cleans_up_partial_and_records_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path().join("models"));
        let model_id = "sensevoice-small";
        // Advertise 8 MiB but send only 4 KiB, then close — the download fails
        // partway through instead of completing.
        let url = serve_truncated(8 * 1024 * 1024, vec![0_u8; 4096]);
        let model = sherpa_descriptor(model_id, url);

        let error = store
            .download_and_extract_sherpa_model(&model)
            .expect_err("truncated download should fail");
        assert!(
            !matches!(error, ModelStoreError::Cancelled { .. }),
            "a network failure must not be reported as a cancellation",
        );

        // No orphaned partial archive, and the status is Error (not stuck on
        // Downloading) so the UI surfaces the failure and a retry is accepted.
        assert!(!store.temp_download_path(model_id).exists());
        let state = store.load_download_state().expect("download state");
        let record = state
            .downloads
            .get(model_id)
            .expect("failed download record");
        assert_eq!(record.status, ModelStatus::Error);
    }

    #[cfg(unix)]
    #[test]
    fn sherpa_extraction_stops_when_cancelled_after_tar_starts() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{Duration, Instant};

        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        let model_id = "sensevoice-small";
        let archive_path = store.temp_download_path(model_id);
        std::fs::write(&archive_path, b"fake archive").expect("archive");

        let spec = sherpa_model_spec(model_id).expect("sherpa spec");
        let tar_path = tmp.path().join("fake-tar.sh");
        std::fs::write(
            &tar_path,
            format!(
                "#!/bin/sh\nmkdir -p \"$4/{dir}\"\ntouch \"$4/{dir}/partial\"\nexec /bin/sleep 5\n",
                dir = spec.dir
            ),
        )
        .expect("fake tar");
        let mut permissions = std::fs::metadata(&tar_path)
            .expect("fake tar metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tar_path, permissions).expect("fake tar executable");

        store
            .record_download_status(model_id, ModelStatus::Extracting, 12, Some(12), None)
            .expect("extracting status");
        let cancelling_store = store.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            cancelling_store
                .cancel_download(model_id)
                .expect("cancel download");
        });

        let started = Instant::now();
        let error = store
            .extract_sherpa_archive_with_tar(model_id, &archive_path, 12, Some(12), &tar_path)
            .expect_err("cancelled extraction should fail");

        canceller.join().expect("canceller");
        assert!(matches!(
            error,
            ModelStoreError::Cancelled { model_id: id } if id == "sensevoice-small"
        ));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cancelled extraction should not wait for the fake tar sleep"
        );
        assert!(!archive_path.exists());
        assert!(
            !store
                .sherpa_model_dir(model_id)
                .expect("sherpa dir")
                .exists()
        );
        let state = store.load_download_state().expect("download state");
        let record = state.downloads.get(model_id).expect("cancelled record");
        assert_eq!(record.status, ModelStatus::Error);
        assert_eq!(record.error.as_deref(), Some("cancelled by user"));
    }
}
