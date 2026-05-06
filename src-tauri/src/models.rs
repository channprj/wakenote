use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

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

    pub fn refresh_statuses(
        &self,
        registry: &mut [ModelDescriptor],
    ) -> Result<(), ModelStoreError> {
        for model in registry {
            model.status = self.verify_model(model)?;
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
        if path.exists() {
            std::fs::remove_file(path)?;
            return Ok(());
        }

        Err(ModelStoreError::NotFound(model_id.to_string()))
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
        mut reader: impl Read,
    ) -> Result<(), ModelStoreError> {
        std::fs::create_dir_all(&self.model_directory)?;
        let path = self.model_path(&model.id);
        let temp_path = self.model_directory.join(format!("{}.download", model.id));
        let mut file = std::fs::File::create(&temp_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            std::io::Write::write_all(&mut file, &buffer[..read])?;
        }
        std::io::Write::flush(&mut file)?;

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
        Ok(())
    }

    pub fn download_model_with<R: Read>(
        &self,
        model: &ModelDescriptor,
        fetch: impl FnOnce(&ModelDescriptor) -> Result<R, ModelStoreError>,
    ) -> Result<ModelStatus, ModelStoreError> {
        let reader = fetch(model)?;
        self.install_model_reader(model, reader)?;
        self.verify_model(model)
    }

    pub fn download_model(&self, model: &ModelDescriptor) -> Result<ModelStatus, ModelStoreError> {
        let url = model
            .download_url
            .as_ref()
            .ok_or_else(|| ModelStoreError::MissingDownloadUrl(model.id.clone()))?;
        self.download_model_with(model, |_| {
            let response = ureq::get(url)
                .call()
                .map_err(|error| ModelStoreError::Download(error.to_string()))?;
            Ok(response.into_reader())
        })
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
        },
    );

    registry
}
