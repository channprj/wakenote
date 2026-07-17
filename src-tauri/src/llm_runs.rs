use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::llm::{
    LlmCompletionReason, LlmGenerateRequest, LlmProgressEvent, LlmProgressStage, LlmReportKind,
    LlmUsageTotals, format_transcripts_for_llm,
};

const INTERRUPTED_ERROR: &str = "WakeNote closed before this report finished";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmReportRunStatus {
    Queued,
    Running,
    Stopping,
    Cancelled,
    Failed,
    Completed,
}

impl LlmReportRunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Failed | Self::Completed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmReportRunSnapshot {
    pub run_id: String,
    pub parent_run_id: Option<String>,
    pub revision: u64,
    pub status: LlmReportRunStatus,
    pub stage: Option<LlmProgressStage>,
    pub kind: LlmReportKind,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub iteration: u8,
    pub max_iterations: u8,
    pub message: String,
    pub detail: Option<String>,
    pub error: Option<String>,
    pub progress: Vec<LlmProgressEvent>,
    pub model: String,
    pub selected_count: usize,
    pub date_range: String,
    pub report_id: Option<String>,
    pub report_path: Option<String>,
    pub completion_reason: Option<LlmCompletionReason>,
    pub success_criteria_met: Option<bool>,
    pub quality_feedback: Option<String>,
    pub usage: Option<LlmUsageTotals>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmReportRunRecord {
    #[serde(flatten)]
    pub snapshot: LlmReportRunSnapshot,
    pub request: LlmGenerateRequest,
}

impl LlmReportRunRecord {
    pub fn queued(
        run_id: String,
        parent_run_id: Option<String>,
        request: LlmGenerateRequest,
        model: String,
        max_iterations: u8,
        now: String,
    ) -> Self {
        let context = format_transcripts_for_llm(&request.transcripts);
        Self {
            snapshot: LlmReportRunSnapshot {
                run_id,
                parent_run_id,
                revision: 1,
                status: LlmReportRunStatus::Queued,
                stage: None,
                kind: request.kind,
                created_at: now.clone(),
                updated_at: now,
                started_at: None,
                finished_at: None,
                iteration: 0,
                max_iterations,
                message: "Queued for report generation".into(),
                detail: None,
                error: None,
                progress: Vec::new(),
                model,
                selected_count: context.selected_count,
                date_range: context.date_range,
                report_id: None,
                report_path: None,
                completion_reason: None,
                success_criteria_met: None,
                quality_feedback: None,
                usage: None,
            },
            request,
        }
    }

    pub fn snapshot(&self) -> LlmReportRunSnapshot {
        self.snapshot.clone()
    }

    pub fn mark_running(&mut self, now: &str) {
        if self.snapshot.status != LlmReportRunStatus::Queued {
            return;
        }
        self.snapshot.status = LlmReportRunStatus::Running;
        self.snapshot.started_at = Some(now.into());
        self.touch(now);
    }

    pub fn mark_failed(&mut self, error: &str, now: &str) {
        if self.snapshot.status.is_terminal() {
            return;
        }
        self.snapshot.status = LlmReportRunStatus::Failed;
        self.snapshot.stage = Some(LlmProgressStage::Failed);
        self.snapshot.message = "Report generation failed".into();
        self.snapshot.detail = Some(error.into());
        self.snapshot.error = Some(error.into());
        self.snapshot.finished_at = Some(now.into());
        self.append_terminal_progress(
            LlmProgressStage::Failed,
            "Report generation failed",
            Some(error.into()),
        );
        self.touch(now);
    }

    fn append_terminal_progress(
        &mut self,
        stage: LlmProgressStage,
        message: &str,
        detail: Option<String>,
    ) {
        if self
            .snapshot
            .progress
            .last()
            .is_some_and(|event| event.stage == stage)
        {
            return;
        }
        self.snapshot.progress.push(LlmProgressEvent {
            run_id: self.snapshot.run_id.clone(),
            stage,
            iteration: self.snapshot.iteration,
            max_iterations: self.snapshot.max_iterations,
            message: message.into(),
            detail,
        });
    }

    fn touch(&mut self, now: &str) {
        self.snapshot.revision = self.snapshot.revision.saturating_add(1);
        self.snapshot.updated_at = now.into();
    }
}

#[derive(Debug, Clone)]
pub struct LlmRunStore {
    runs_dir: PathBuf,
}

impl LlmRunStore {
    pub fn new(save_root: impl AsRef<Path>) -> Self {
        Self {
            runs_dir: save_root.as_ref().join("reports").join(".runs"),
        }
    }

    pub fn write(&self, record: &LlmReportRunRecord) -> Result<(), String> {
        validate_run_id(&record.snapshot.run_id)?;
        let encoded = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
        fs::create_dir_all(&self.runs_dir).map_err(|error| error.to_string())?;
        let final_path = self.record_path(&record.snapshot.run_id);
        let temp_path = self.temp_path(&record.snapshot.run_id);
        if let Err(error) = fs::write(&temp_path, encoded) {
            let _ = fs::remove_file(&temp_path);
            return Err(error.to_string());
        }
        if let Err(error) = fs::rename(&temp_path, &final_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(error.to_string());
        }
        Ok(())
    }

    pub fn load(&self, run_id: &str) -> Result<LlmReportRunRecord, String> {
        validate_run_id(run_id)?;
        let bytes = fs::read(self.record_path(run_id)).map_err(|error| error.to_string())?;
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    }

    pub fn list(&self) -> Result<Vec<LlmReportRunRecord>, String> {
        let entries = match fs::read_dir(&self.runs_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut records = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let run_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| "Invalid report run file name".to_string())?;
            records.push(self.load(run_id)?);
        }
        records.sort_by(|left, right| right.snapshot.created_at.cmp(&left.snapshot.created_at));
        Ok(records)
    }

    pub fn recover_interrupted(&self, now: &str) -> Result<Vec<LlmReportRunSnapshot>, String> {
        let mut recovered = Vec::new();
        for mut record in self.list()? {
            if record.snapshot.status.is_terminal() {
                continue;
            }
            record.mark_failed(INTERRUPTED_ERROR, now);
            self.write(&record)?;
            recovered.push(record.snapshot());
        }
        Ok(recovered)
    }

    fn record_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir.join(format!("{run_id}.json"))
    }

    fn temp_path(&self, run_id: &str) -> PathBuf {
        self.runs_dir.join(format!("{run_id}.json.tmp"))
    }
}

fn validate_run_id(run_id: &str) -> Result<(), String> {
    let mut characters = run_id.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let valid_rest = characters
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'));
    if valid_first && valid_rest {
        Ok(())
    } else {
        Err(format!("Invalid report run id: {run_id}"))
    }
}
