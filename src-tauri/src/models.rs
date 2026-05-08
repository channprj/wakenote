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

    pub fn model_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.bin"))
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
        if !path.exists() {
            return Ok(default_model_registry());
        }

        parse_model_registry_json(&std::fs::read_to_string(path)?)
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

    pub fn delete_model(&self, model_id: &str) -> Result<(), ModelStoreError> {
        let path = self.model_path(model_id);
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
        let mut request = ureq::get(url);
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

    Some(((record.downloaded_bytes as f64 / total as f64) * 100.0).round() as u8)
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

pub fn default_model_registry() -> BTreeMap<String, ModelDescriptor> {
    let mut registry = BTreeMap::new();

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
                "6c14d5adee4f86394037d23e1625d96385c22f032d72d6fdf045dc1741ca091e".to_string(),
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
                "bd577a113a864445d4c299885e0cb97d4ba92b5fca5b2bce5b656d95d0f941a2".to_string(),
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

    registry
}
