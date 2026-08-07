use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::settings::TranscriptionOptions;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueIssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueIssueCode {
    EmptyTranscript,
    NoSpeech,
    LowConfidence,
    TranscriptArtifact,
    Authentication,
    BillingLimit,
    RateLimit,
    Provider,
    Transport,
    InvalidResponse,
    Model,
    AudioDecode,
    LocalIo,
    Cancelled,
    Skipped,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueActivityKind {
    Dictation,
    LiveTranscription,
    ImportedAudio,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueJobIssue {
    pub severity: QueueIssueSeverity,
    pub code: QueueIssueCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
}

impl QueueJobIssue {
    pub fn warning(code: QueueIssueCode, message: impl Into<String>) -> Self {
        Self {
            severity: QueueIssueSeverity::Warning,
            code,
            message: message.into(),
            occurred_at: None,
        }
    }

    pub fn error(code: QueueIssueCode, message: impl Into<String>) -> Self {
        Self {
            severity: QueueIssueSeverity::Error,
            code,
            message: message.into(),
            occurred_at: None,
        }
    }

    pub fn stamp_if_missing(&mut self) {
        if self.occurred_at.is_none() {
            self.occurred_at = Some(Utc::now().to_rfc3339());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueJob {
    pub id: u64,
    pub audio_path: PathBuf,
    pub model_id: String,
    pub status: QueueJobStatus,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<QueueJobIssue>,
    #[serde(default)]
    pub is_read: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription_options: Option<TranscriptionOptions>,
    #[serde(default)]
    pub activity_kind: QueueActivityKind,
}

impl QueueJob {
    pub fn is_reprocessable(&self) -> bool {
        matches!(
            self.status,
            QueueJobStatus::Completed
                | QueueJobStatus::Failed
                | QueueJobStatus::Cancelled
                | QueueJobStatus::Skipped
        )
    }

    pub fn is_reprocessable_attention_outcome(&self) -> bool {
        matches!(
            self.status,
            QueueJobStatus::Failed | QueueJobStatus::Cancelled | QueueJobStatus::Skipped
        ) || (self.status == QueueJobStatus::Completed
            && self
                .issue
                .as_ref()
                .is_some_and(|issue| issue.severity == QueueIssueSeverity::Warning))
    }
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
        let activity_kind = activity_kind_for_audio_path(&audio_path);
        self.jobs.push(QueueJob {
            id,
            audio_path,
            model_id: model_id.into(),
            status: QueueJobStatus::Pending,
            error: None,
            issue: None,
            is_read: false,
            transcription_options: None,
            activity_kind,
        });
        (id, true)
    }

    pub fn set_transcription_options(
        &mut self,
        id: u64,
        options: TranscriptionOptions,
    ) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        job.transcription_options = Some(options);
        Ok(())
    }

    pub fn enqueue_backlog(&mut self, scan: BacklogScan, model_id: impl Into<String>) -> Vec<u64> {
        let model_id = model_id.into();
        let mut enqueued = Vec::new();
        for audio_path in scan.pending_audio {
            if let Some(index) = self
                .jobs
                .iter()
                .position(|job| job.audio_path == audio_path)
            {
                let job = &mut self.jobs[index];
                if job.status == QueueJobStatus::Completed {
                    job.model_id = model_id.clone();
                    job.status = QueueJobStatus::Pending;
                    job.error = None;
                    job.issue = None;
                    job.is_read = false;
                    enqueued.push(job.id);
                }
                continue;
            }

            enqueued.push(self.enqueue_file(audio_path, model_id.clone()));
        }
        enqueued
    }

    pub fn requeue_file(
        &mut self,
        audio_path: impl Into<PathBuf>,
        model_id: impl Into<String>,
    ) -> Result<u64, String> {
        let audio_path = audio_path.into();
        let model_id = model_id.into();
        if let Some(job) = self
            .jobs
            .iter_mut()
            .find(|job| job.audio_path == audio_path)
        {
            if job.status == QueueJobStatus::Running {
                return Err(format!("job {} is currently running", job.id));
            }
            job.model_id = model_id;
            job.status = QueueJobStatus::Pending;
            job.error = None;
            job.issue = None;
            job.is_read = false;
            return Ok(job.id);
        }

        Ok(self.enqueue_file(audio_path, model_id))
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
        job.issue = None;
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
        job.issue = None;
        Some(job.clone())
    }

    pub fn cancel_current(&mut self, reason: impl Into<String>) -> Result<(), String> {
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.status == QueueJobStatus::Running)
            .ok_or_else(|| "no running job".to_string())?;
        let reason = reason.into();
        let mut issue = QueueJobIssue::warning(QueueIssueCode::Cancelled, reason.clone());
        issue.stamp_if_missing();
        job.status = QueueJobStatus::Cancelled;
        job.error = Some(reason);
        job.issue = Some(issue);
        job.is_read = false;
        Ok(())
    }

    pub fn mark_failed(&mut self, id: u64, error: impl Into<String>) -> Result<(), String> {
        let error = error.into();
        self.mark_failed_with_issue(id, QueueJobIssue::error(QueueIssueCode::Unknown, error))
    }

    pub fn mark_failed_with_issue(
        &mut self,
        id: u64,
        mut issue: QueueJobIssue,
    ) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        issue.stamp_if_missing();
        job.status = QueueJobStatus::Failed;
        job.error = Some(issue.message.clone());
        job.issue = Some(issue);
        job.is_read = false;
        Ok(())
    }

    pub fn mark_completed(&mut self, id: u64) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        job.status = QueueJobStatus::Completed;
        job.error = None;
        job.issue = None;
        job.is_read = false;
        Ok(())
    }

    pub fn mark_completed_with_issue(
        &mut self,
        id: u64,
        mut issue: QueueJobIssue,
    ) -> Result<(), String> {
        if issue.severity != QueueIssueSeverity::Warning {
            return Err("completed jobs can only carry warning issues".to_string());
        }
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        issue.stamp_if_missing();
        job.status = QueueJobStatus::Completed;
        job.error = None;
        job.issue = Some(issue);
        job.is_read = false;
        Ok(())
    }

    pub fn retry(&mut self, id: u64) -> Result<(), String> {
        let job = self
            .job_mut(id)
            .ok_or_else(|| format!("job {id} not found"))?;
        if !job.is_reprocessable() {
            return Err(format!("job {id} cannot be retried from {:?}", job.status));
        }
        job.status = QueueJobStatus::Pending;
        job.error = None;
        job.issue = None;
        job.is_read = false;
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
        let mut issue = QueueJobIssue::warning(QueueIssueCode::Skipped, "Skipped by user");
        issue.stamp_if_missing();
        job.issue = Some(issue);
        job.is_read = false;
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

    pub fn unread_attention_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| !job.is_read && job_requires_attention(job))
            .count()
    }

    pub fn unread_error_count(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| {
                !job.is_read
                    && job_requires_attention(job)
                    && job
                        .issue
                        .as_ref()
                        .map(|issue| issue.severity == QueueIssueSeverity::Error)
                        .unwrap_or(job.status == QueueJobStatus::Failed)
            })
            .count()
    }

    pub fn mark_attention_outcomes_read(&mut self) -> usize {
        let mut marked_count = 0;
        for job in &mut self.jobs {
            if !job.is_read && job_requires_attention(job) {
                job.is_read = true;
                marked_count += 1;
            }
        }
        marked_count
    }

    pub fn jobs_mut(&mut self) -> &mut [QueueJob] {
        &mut self.jobs
    }

    pub fn normalize_legacy_issues(&mut self) -> bool {
        let mut changed = false;
        for job in &mut self.jobs {
            if job.issue.is_some() {
                continue;
            }
            let issue = match job.status {
                QueueJobStatus::Failed => {
                    let message = job.error.as_deref().unwrap_or("Transcription failed");
                    let normalized = message.trim();
                    if normalized == "No speech detected" {
                        QueueJobIssue::warning(QueueIssueCode::NoSpeech, message)
                    } else if normalized.ends_with("returned an empty transcript") {
                        QueueJobIssue::warning(QueueIssueCode::EmptyTranscript, message)
                    } else {
                        QueueJobIssue::error(QueueIssueCode::Unknown, message)
                    }
                }
                QueueJobStatus::Cancelled => QueueJobIssue::warning(
                    QueueIssueCode::Cancelled,
                    job.error.as_deref().unwrap_or("Cancelled"),
                ),
                QueueJobStatus::Skipped => {
                    QueueJobIssue::warning(QueueIssueCode::Skipped, "Skipped by user")
                }
                _ => continue,
            };
            job.issue = Some(issue);
            changed = true;
        }
        changed
    }

    pub fn hydrate_activity_kinds(&mut self) -> bool {
        let mut changed = false;
        for job in &mut self.jobs {
            if job.activity_kind != QueueActivityKind::Other {
                continue;
            }
            let inferred = activity_kind_for_audio_path(&job.audio_path);
            if inferred != QueueActivityKind::Other {
                job.activity_kind = inferred;
                changed = true;
            }
        }
        changed
    }

    pub fn remove_jobs(&mut self, ids: &[u64]) -> usize {
        let ids = ids.iter().copied().collect::<HashSet<_>>();
        let before = self.jobs.len();
        self.jobs.retain(|job| !ids.contains(&job.id));
        before - self.jobs.len()
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

fn activity_kind_for_audio_path(audio_path: &Path) -> QueueActivityKind {
    if audio_path
        .components()
        .any(|component| component.as_os_str() == "uploaded")
    {
        return QueueActivityKind::ImportedAudio;
    }

    let metadata_path = audio_path.with_extension("json");
    let Ok(bytes) = std::fs::read(metadata_path) else {
        return QueueActivityKind::Other;
    };
    let Ok(metadata) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return QueueActivityKind::Other;
    };
    if metadata
        .get("source_label")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|label| label.eq_ignore_ascii_case("dictation"))
    {
        QueueActivityKind::Dictation
    } else if metadata
        .get("source")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|source| matches!(source, "microphone" | "system"))
    {
        QueueActivityKind::LiveTranscription
    } else {
        QueueActivityKind::Other
    }
}

fn job_requires_attention(job: &QueueJob) -> bool {
    job.is_reprocessable_attention_outcome()
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

    let transcript_path = parent.join(format!("{stem}.txt"));
    parent.join(format!("{stem}.error.txt")).exists()
        || std::fs::metadata(transcript_path)
            .map(|metadata| metadata.len() > 1)
            .unwrap_or(false)
}
