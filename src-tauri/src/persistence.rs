use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::queue::{COMPLETED_JOB_HISTORY_LIMIT, QueueJobStatus, TranscriptionQueue};
use crate::settings::{AppSettings, SettingsPatch};

#[derive(Debug, Clone)]
pub struct AppPersistence {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct OpenRouterSecrets {
    api_key: String,
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid secret: {0}")]
    InvalidSecret(String),
}

impl AppPersistence {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn load_settings(&self) -> Result<Option<AppSettings>, PersistenceError> {
        let Some(patch) = read_json_if_exists::<SettingsPatch>(&self.settings_path())? else {
            return Ok(None);
        };
        let mut settings = AppSettings::default();
        settings.apply_patch(patch);
        Ok(Some(settings))
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), PersistenceError> {
        write_json_atomic(&self.settings_path(), settings)
    }

    pub fn load_openrouter_api_key(&self) -> Result<Option<String>, PersistenceError> {
        let Some(secrets) =
            read_json_if_exists::<OpenRouterSecrets>(&self.openrouter_secrets_path())?
        else {
            return Ok(None);
        };
        let api_key = secrets.api_key.trim();
        Ok((!api_key.is_empty()).then(|| api_key.to_string()))
    }

    pub fn save_openrouter_api_key(&self, api_key: &str) -> Result<(), PersistenceError> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(PersistenceError::InvalidSecret(
                "OpenRouter API key cannot be blank".to_string(),
            ));
        }
        write_json_atomic(
            &self.openrouter_secrets_path(),
            &OpenRouterSecrets {
                api_key: api_key.to_string(),
            },
        )
    }

    pub fn delete_openrouter_api_key(&self) -> Result<(), PersistenceError> {
        match std::fs::remove_file(self.openrouter_secrets_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn openrouter_api_key_configured(&self) -> Result<bool, PersistenceError> {
        Ok(self.load_openrouter_api_key()?.is_some())
    }

    pub fn load_queue(&self) -> Result<Option<TranscriptionQueue>, PersistenceError> {
        let Some(mut queue) = read_json_if_exists::<TranscriptionQueue>(&self.queue_path())? else {
            return Ok(None);
        };
        let original = queue.clone();
        queue.recover_running_as_pending();
        queue.prune_completed_history(COMPLETED_JOB_HISTORY_LIMIT);
        if queue != original {
            self.save_queue(&queue)?;
        }
        Ok(Some(queue))
    }

    pub fn save_queue(&self, queue: &TranscriptionQueue) -> Result<(), PersistenceError> {
        let mut persisted = queue.clone();
        persisted.prune_completed_history(COMPLETED_JOB_HISTORY_LIMIT);
        write_json_atomic(&self.queue_path(), &persisted)
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn queue_path(&self) -> PathBuf {
        self.root.join("transcription-queue.json")
    }

    fn openrouter_secrets_path(&self) -> PathBuf {
        self.root.join("openrouter-secrets.json")
    }
}

fn read_json_if_exists<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, PersistenceError> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = std::fs::read(path)?;
    let value = serde_json::from_slice(&bytes)?;
    Ok(Some(value))
}

fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), PersistenceError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value)?;
    std::fs::write(&tmp_path, bytes)?;
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

impl TranscriptionQueue {
    pub fn recover_running_as_pending(&mut self) {
        for job in self.jobs_mut() {
            if job.status == QueueJobStatus::Running {
                job.status = QueueJobStatus::Pending;
                job.error = Some("recovered from interrupted app session".to_string());
            }
        }
    }
}
