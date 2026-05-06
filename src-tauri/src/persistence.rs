use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::queue::{QueueJobStatus, TranscriptionQueue};
use crate::settings::AppSettings;

#[derive(Debug, Clone)]
pub struct AppPersistence {
    root: PathBuf,
}

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl AppPersistence {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn load_settings(&self) -> Result<Option<AppSettings>, PersistenceError> {
        read_json_if_exists(&self.settings_path())
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), PersistenceError> {
        write_json_atomic(&self.settings_path(), settings)
    }

    pub fn load_queue(&self) -> Result<Option<TranscriptionQueue>, PersistenceError> {
        let Some(mut queue) = read_json_if_exists::<TranscriptionQueue>(&self.queue_path())? else {
            return Ok(None);
        };
        queue.recover_running_as_pending();
        Ok(Some(queue))
    }

    pub fn save_queue(&self, queue: &TranscriptionQueue) -> Result<(), PersistenceError> {
        write_json_atomic(&self.queue_path(), queue)
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn queue_path(&self) -> PathBuf {
        self.root.join("transcription-queue.json")
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
