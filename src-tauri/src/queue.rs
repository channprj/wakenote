use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const COMPLETED_JOB_HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueJob {
    pub id: u64,
    pub audio_path: PathBuf,
    pub model_id: String,
    pub status: QueueJobStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub jobs: Vec<QueueJob>,
    pub pending_count: usize,
    pub running_count: usize,
    pub failed_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BacklogScan {
    pub pending_audio: Vec<PathBuf>,
}

impl BacklogScan {
    pub fn scan(root: &Path) -> std::io::Result<Self> {
        let mut pending_audio = Vec::new();
        collect_pending_audio(root, &mut pending_audio)?;
        pending_audio.sort();
        Ok(Self { pending_audio })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionQueue {
    jobs: Vec<QueueJob>,
    next_id: u64,
}

impl TranscriptionQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue_file(
        &mut self,
        audio_path: impl Into<PathBuf>,
        model_id: impl Into<String>,
    ) -> u64 {
        self.enqueue_file_if_new(audio_path, model_id).0
    }

    pub fn enqueue_file_if_new(
        &mut self,
        audio_path: impl Into<PathBuf>,
        model_id: impl Into<String>,
    ) -> (u64, bool) {
        let audio_path = audio_path.into();
        if let Some(job) = self.jobs.iter().find(|job| job.audio_path == audio_path) {
            return (job.id, false);
        }

        self.next_id += 1;
        let id = self.next_id;
        self.jobs.push(QueueJob {
            id,
            audio_path,
            model_id: model_id.into(),
            status: QueueJobStatus::Pending,
            error: None,
        });
        (id, true)
    }

    pub fn enqueue_backlog(&mut self, scan: BacklogScan, model_id: impl Into<String>) -> Vec<u64> {
        let model_id = model_id.into();
        let mut existing_paths = self
            .jobs
            .iter()
            .map(|job| job.audio_path.clone())
            .collect::<std::collections::HashSet<_>>();
        scan.pending_audio
            .into_iter()
            .filter_map(|audio_path| {
                if existing_paths.insert(audio_path.clone()) {
                    Some(self.enqueue_file(audio_path, model_id.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn start_next(&mut self) -> Option<QueueJob> {
        if self
            .jobs
            .iter()
            .any(|job| job.status == QueueJobStatus::Running)
        {
            return None;
        }

        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.status == QueueJobStatus::Pending)?;
        job.status = QueueJobStatus::Running;
        job.error = None;
        Some(job.clone())
    }

    pub fn has_pending_for_model_ids(&self, model_ids: &HashSet<String>) -> bool {
        self.jobs
            .iter()
            .any(|job| job.status == QueueJobStatus::Pending && model_ids.contains(&job.model_id))
    }

    pub fn start_next_for_model_ids(&mut self, model_ids: &HashSet<String>) -> Option<QueueJob> {
        self.start_next_for_model_ids_up_to(model_ids, 1)
    }

    pub fn start_next_for_model_ids_up_to(
        &mut self,
        model_ids: &HashSet<String>,
        max_running: usize,
    ) -> Option<QueueJob> {
        if max_running == 0 || self.running_job_count() > 0 {
            return None;
        }

        let job = self.jobs.iter_mut().find(|job| {
            job.status == QueueJobStatus::Pending && model_ids.contains(&job.model_id)
        })?;
        job.status = QueueJobStatus::Running;
        job.error = None;
        Some(job.clone())
    }

    pub fn cancel_current(&mut self, reason: impl Into<String>) -> Result<(), String> {
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.status == QueueJobStatus::Running)
            .ok_or_else(|| "no running job".to_string())?;
        job.status = QueueJobStatus::Cancelled;
        job.error = Some(reason.into());
        Ok(())
    }

    pub fn mark_failed(&mut self, id: u64, error: impl Into<String>) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        job.status = QueueJobStatus::Failed;
        job.error = Some(error.into());
        Ok(())
    }

    pub fn mark_completed(&mut self, id: u64) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        job.status = QueueJobStatus::Completed;
        job.error = None;
        Ok(())
    }

    pub fn retry(&mut self, id: u64) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        if !matches!(
            job.status,
            QueueJobStatus::Failed | QueueJobStatus::Cancelled
        ) {
            return Err(format!("job {id} cannot be retried from {:?}", job.status));
        }
        job.status = QueueJobStatus::Pending;
        job.error = None;
        Ok(())
    }

    pub fn skip(&mut self, id: u64) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        if !matches!(
            job.status,
            QueueJobStatus::Pending | QueueJobStatus::Failed | QueueJobStatus::Cancelled
        ) {
            return Err(format!("job {id} cannot be skipped from {:?}", job.status));
        }
        job.status = QueueJobStatus::Skipped;
        job.error = None;
        Ok(())
    }

    pub fn job(&self, id: u64) -> Option<&QueueJob> {
        self.jobs.iter().find(|job| job.id == id)
    }

    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            jobs: self.jobs.clone(),
            pending_count: self
                .jobs
                .iter()
                .filter(|job| job.status == QueueJobStatus::Pending)
                .count(),
            running_count: self
                .jobs
                .iter()
                .filter(|job| job.status == QueueJobStatus::Running)
                .count(),
            failed_count: self
                .jobs
                .iter()
                .filter(|job| job.status == QueueJobStatus::Failed)
                .count(),
        }
    }

    pub fn jobs_mut(&mut self) -> &mut [QueueJob] {
        &mut self.jobs
    }

    pub fn prune_completed_history(&mut self, max_completed: usize) -> bool {
        let completed_count = self
            .jobs
            .iter()
            .filter(|job| job.status == QueueJobStatus::Completed)
            .count();
        if completed_count <= max_completed {
            return false;
        }

        let mut remaining_to_prune = completed_count - max_completed;
        self.jobs.retain(|job| {
            if job.status == QueueJobStatus::Completed && remaining_to_prune > 0 {
                remaining_to_prune -= 1;
                return false;
            }
            true
        });
        true
    }

    fn job_mut(&mut self, id: u64) -> Option<&mut QueueJob> {
        self.jobs.iter_mut().find(|job| job.id == id)
    }

    fn running_job_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| job.status == QueueJobStatus::Running)
            .count()
    }
}

fn collect_pending_audio(root: &Path, pending_audio: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_pending_audio(&path, pending_audio)?;
            continue;
        }

        if !file_type.is_file() || !is_audio_path(&path) || has_transcription_sidecar(&path) {
            continue;
        }

        pending_audio.push(path);
    }

    Ok(())
}

pub fn is_importable_audio_path(path: &Path) -> bool {
    path.is_file() && is_audio_path(path)
}

fn is_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "m4a" | "wav"))
        .unwrap_or(false)
}

fn has_transcription_sidecar(audio_path: &Path) -> bool {
    let Some(parent) = audio_path.parent() else {
        return false;
    };
    let Some(stem) = audio_path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };

    parent.join(format!("{stem}.txt")).exists() || parent.join(format!("{stem}.error.txt")).exists()
}
