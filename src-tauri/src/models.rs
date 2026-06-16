use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    /// Resolved Cohere API key (settings value or environment fallback). Used
    /// only to decide whether a `cohere-api` model is `Ready`; the transcriber
    /// carries its own copy of the key.
    cohere_api_key: Option<String>,
}

/// How to install and run an `external-command` model end to end. The install
/// flow detects [`runtime_command`](Self::runtime_command), runs the optional
/// [`setup_command`](Self::setup_command) to fetch the weights, and writes
/// [`command_template`](Self::command_template) to `<model_id>.command`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRecipe {
    /// CLI that must be on `PATH` for this model to run (detected via `command -v`).
    pub runtime_command: String,
    /// Human-readable hint shown when the runtime command is missing.
    pub install_hint: String,
    /// Optional shell command that downloads/prepares the model weights. Runs
    /// with `WAKENOTE_MODEL_DIRECTORY` set. `None` means the runtime fetches its
    /// own weights on first use.
    pub setup_command: Option<String>,
    /// Contents written to `<model_id>.command`, executed once per transcription.
    pub command_template: String,
}

/// Built-in install recipe for a known `external-command` model, or `None` for
/// models that must be configured by hand.
pub fn adapter_recipe(model_id: &str) -> Option<AdapterRecipe> {
    match model_id {
        "parakeet-tdt-0.6b-v3" => Some(macparakeet_recipe("parakeet", "parakeet-v3")),
        "nemotron-3.5-asr" => Some(macparakeet_recipe("nemotron", "nemotron-multilingual-1120ms")),
        "sensevoice-small" => Some(sensevoice_recipe()),
        _ => None,
    }
}

/// FluidAudio / macparakeet CoreML adapter (Apple Neural Engine, on-device).
/// `engine` selects the macparakeet engine, `model_name` the weights to prefetch.
fn macparakeet_recipe(engine: &str, model_name: &str) -> AdapterRecipe {
    AdapterRecipe {
        runtime_command: "macparakeet-cli".to_string(),
        install_hint: "Install the FluidAudio CLI first: brew install moona3k/tap/macparakeet-cli"
            .to_string(),
        setup_command: Some(format!("macparakeet-cli models download {model_name}")),
        command_template: format!(
            "#!/bin/sh\n\
             # WakeNote external ASR adapter (auto-generated). Edit to match your CLI.\n\
             exec macparakeet-cli transcribe \"$WAKENOTE_AUDIO_PATH\" \\\n\
             \x20 --engine {engine} --language \"$WAKENOTE_LANGUAGE\" \\\n\
             \x20 --format transcript --no-history\n"
        ),
    }
}

/// sherpa-onnx adapter for SenseVoice. The transcribe template is a documented
/// starting point; flags and stdout parsing may need tuning per sherpa version.
fn sensevoice_recipe() -> AdapterRecipe {
    let archive = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17";
    AdapterRecipe {
        runtime_command: "sherpa-onnx-offline".to_string(),
        install_hint: "Install sherpa-onnx first: pip install sherpa-onnx".to_string(),
        setup_command: Some(format!(
            "set -e\n\
             cd \"$WAKENOTE_MODEL_DIRECTORY\"\n\
             curl -fL -o \"{archive}.tar.bz2\" \"https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{archive}.tar.bz2\"\n\
             tar xjf \"{archive}.tar.bz2\"\n\
             rm -f \"{archive}.tar.bz2\"\n"
        )),
        command_template: format!(
            "#!/bin/sh\n\
             # WakeNote external ASR adapter (auto-generated, sherpa-onnx template).\n\
             # Adjust flags/parsing to match your installed sherpa-onnx version.\n\
             MODEL_DIR=\"$WAKENOTE_MODEL_DIRECTORY/{archive}\"\n\
             sherpa-onnx-offline \\\n\
             \x20 --sense-voice-model=\"$MODEL_DIR/model.int8.onnx\" \\\n\
             \x20 --tokens=\"$MODEL_DIR/tokens.txt\" \\\n\
             \x20 --num-threads=2 \\\n\
             \x20 \"$WAKENOTE_AUDIO_PATH\" 2>/dev/null \\\n\
             \x20 | sed -n 's/.*text:[[:space:]]*//p' | tail -n 1\n"
        ),
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
            cohere_api_key: cohere_api_key_from_env(),
        }
    }

    /// Like [`Self::new`] but with an explicit Cohere API key (e.g. from
    /// settings). A blank/`None` key falls back to the environment so the
    /// configured key wins but env credentials still work.
    pub fn with_cohere_api_key(
        model_directory: impl AsRef<Path>,
        cohere_api_key: Option<String>,
    ) -> Self {
        let cohere_api_key = cohere_api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(cohere_api_key_from_env);
        Self {
            model_directory: model_directory.as_ref().to_path_buf(),
            cohere_api_key,
        }
    }

    /// Whether a usable Cohere API key is configured (settings or environment).
    pub fn cohere_key_available(&self) -> bool {
        self.cohere_api_key.is_some()
    }

    /// Write `contents` to `<model_id>.command`, marking an external-command
    /// model ready. Creates the model directory if needed.
    pub fn write_adapter_command(
        &self,
        model_id: &str,
        contents: &str,
    ) -> Result<(), ModelStoreError> {
        std::fs::create_dir_all(&self.model_directory)?;
        std::fs::write(self.command_path(model_id), contents)?;
        Ok(())
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
        if model.provider_runtime == "external-command" {
            return Ok(if self.command_path(&model.id).exists() {
                ModelStatus::Ready
            } else {
                ModelStatus::Missing
            });
        }

        if model.provider_runtime == "cohere-api" {
            return Ok(
                if self.command_path(&model.id).exists() || self.cohere_key_available() {
                    ModelStatus::Ready
                } else {
                    ModelStatus::Missing
                },
            );
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

/// The Cohere API key from the environment (`COHERE_API_KEY`, then
/// `CO_API_KEY`), trimmed and only if non-empty.
fn cohere_api_key_from_env() -> Option<String> {
    std::env::var("COHERE_API_KEY")
        .or_else(|_| std::env::var("CO_API_KEY"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
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
            download_url: Some("https://blob.handy.computer/ggml-small.bin".to_string()),
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
        "whisper-tiny".to_string(),
        ModelDescriptor {
            id: "whisper-tiny".to_string(),
            display_name: "Whisper Tiny".to_string(),
            engine: "whisper.cpp".to_string(),
            provider_runtime: "whisper-rs".to_string(),
            download_url: Some(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin"
                    .to_string(),
            ),
            checksum_sha256: Some(
                "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21".to_string(),
            ),
            size_mb: 75,
            languages: vec!["ko".to_string(), "en".to_string(), "multi".to_string()],
            speed_score: 9,
            accuracy_score: 4,
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
            download_url: Some("https://blob.handy.computer/ggml-large-v3-q5_0.bin".to_string()),
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
            engine: "NVIDIA Parakeet".to_string(),
            provider_runtime: "external-command".to_string(),
            download_url: None,
            checksum_sha256: None,
            size_mb: 1_200,
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
            provider_runtime: "external-command".to_string(),
            download_url: None,
            checksum_sha256: None,
            size_mb: 1_000,
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

    registry.insert(
        "cohere-transcribe-03-2026".to_string(),
        ModelDescriptor {
            id: "cohere-transcribe-03-2026".to_string(),
            display_name: "Cohere Transcribe".to_string(),
            engine: "Cohere Transcribe".to_string(),
            provider_runtime: "cohere-api".to_string(),
            download_url: None,
            checksum_sha256: None,
            size_mb: 0,
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
            offline: false,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry.insert(
        "nemotron-3.5-asr".to_string(),
        ModelDescriptor {
            id: "nemotron-3.5-asr".to_string(),
            display_name: "Nemotron 3.5 ASR".to_string(),
            engine: "NVIDIA Nemotron".to_string(),
            provider_runtime: "external-command".to_string(),
            download_url: None,
            checksum_sha256: None,
            size_mb: 1_200,
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
            speed_score: 9,
            accuracy_score: 9,
            offline: true,
            status: ModelStatus::Missing,
            download_progress: None,
            download_error: None,
        },
    );

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const WHISPER_TINY_SHA256: &str =
        "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21";
    const WHISPER_SMALL_SHA256: &str =
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b";
    const WHISPER_MEDIUM_SHA256: &str =
        "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208";
    const WHISPER_TURBO_SHA256: &str =
        "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69";
    const WHISPER_LARGE_SHA256: &str =
        "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1";
    // Pre-fix bogus values that shipped to users; never reintroduce.
    const WHISPER_TINY_BOGUS_SHA256: &str =
        "bd577a113a864445d4c299885e0cb97d4ba92b5fca5b2bce5b656d95d0f941a2";
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
            whisper_models, 5,
            "registry should ship five pinned Whisper models",
        );

        let tiny = registry.get("whisper-tiny").expect("whisper-tiny entry");
        let tiny_hash = tiny
            .checksum_sha256
            .as_deref()
            .expect("whisper-tiny must have a checksum");
        assert!(
            is_lowercase_hex_64(tiny_hash),
            "tiny checksum must be 64 lowercase hex chars: {tiny_hash}",
        );
        assert_eq!(tiny_hash, WHISPER_TINY_SHA256);
        assert_ne!(
            tiny_hash, WHISPER_TINY_BOGUS_SHA256,
            "regression: pre-fix bogus tiny checksum must never reappear",
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
    fn default_registry_includes_nemotron_35_asr() {
        let registry = default_model_registry();
        let nemotron = registry
            .get("nemotron-3.5-asr")
            .expect("nemotron-3.5-asr entry");
        assert_eq!(nemotron.provider_runtime, "external-command");
        assert!(nemotron.download_url.is_none());
        assert!(nemotron.offline);
        assert!(nemotron.languages.iter().any(|lang| lang == "ko"));
    }

    #[test]
    fn adapter_recipe_known_models_use_expected_runtime() {
        let parakeet = adapter_recipe("parakeet-tdt-0.6b-v3").expect("parakeet recipe");
        assert_eq!(parakeet.runtime_command, "macparakeet-cli");
        assert!(parakeet.command_template.contains("--engine parakeet"));

        let nemotron = adapter_recipe("nemotron-3.5-asr").expect("nemotron recipe");
        assert_eq!(nemotron.runtime_command, "macparakeet-cli");
        assert!(nemotron.command_template.contains("--engine nemotron"));
        assert!(
            nemotron
                .setup_command
                .as_deref()
                .is_some_and(|cmd| cmd.contains("models download nemotron"))
        );

        let sensevoice = adapter_recipe("sensevoice-small").expect("sensevoice recipe");
        assert_eq!(sensevoice.runtime_command, "sherpa-onnx-offline");

        assert!(adapter_recipe("whisper-small").is_none());
    }

    #[test]
    fn write_adapter_command_marks_external_command_model_ready() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        let model = ModelDescriptor {
            provider_runtime: "external-command".to_string(),
            ..descriptor("nemotron-3.5-asr", None)
        };

        assert_eq!(
            store.verify_model(&model).expect("verify before"),
            ModelStatus::Missing
        );

        store
            .write_adapter_command("nemotron-3.5-asr", "printf transcript")
            .expect("write adapter");

        assert_eq!(
            store.verify_model(&model).expect("verify after"),
            ModelStatus::Ready
        );
    }

    #[test]
    fn cohere_model_ready_when_api_key_configured() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model = ModelDescriptor {
            provider_runtime: "cohere-api".to_string(),
            ..descriptor("cohere-transcribe-03-2026", None)
        };

        let with_key = ModelStore::with_cohere_api_key(tmp.path(), Some("secret-key".to_string()));
        assert!(with_key.cohere_key_available());
        assert_eq!(
            with_key.verify_model(&model).expect("verify with key"),
            ModelStatus::Ready
        );
    }
}
