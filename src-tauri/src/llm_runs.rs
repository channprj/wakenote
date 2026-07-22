use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_util::sync::CancellationToken;

use crate::llm::{
    LlmCompletionReason, LlmGenerateRequest, LlmGenerateResponse, LlmProgressEvent,
    LlmProgressStage, LlmReportKind, LlmUsageTotals, format_transcripts_for_llm,
};

const INTERRUPTED_ERROR: &str = "WakeNote closed before this report finished";
static RUN_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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

    pub fn apply_progress(&mut self, event: LlmProgressEvent, now: &str) {
        if self.snapshot.status.is_terminal()
            || self.snapshot.status == LlmReportRunStatus::Stopping
        {
            return;
        }
        self.snapshot.stage = Some(event.stage);
        self.snapshot.iteration = event.iteration;
        self.snapshot.max_iterations = event.max_iterations;
        self.snapshot.message = event.message.clone();
        self.snapshot.detail = event.detail.clone();
        self.snapshot.progress.push(event);
        self.touch(now);
    }

    pub fn mark_stopping(&mut self, now: &str) {
        if self.snapshot.status.is_terminal()
            || self.snapshot.status == LlmReportRunStatus::Stopping
        {
            return;
        }
        self.snapshot.status = LlmReportRunStatus::Stopping;
        self.snapshot.message = "Stopping report generation".into();
        self.snapshot.detail = None;
        self.touch(now);
    }

    pub fn mark_cancelled(&mut self, now: &str) {
        if self.snapshot.status.is_terminal() {
            return;
        }
        self.snapshot.status = LlmReportRunStatus::Cancelled;
        self.snapshot.stage = Some(LlmProgressStage::Cancelled);
        self.snapshot.message = "Report generation cancelled".into();
        self.snapshot.detail = None;
        self.snapshot.error = None;
        self.snapshot.finished_at = Some(now.into());
        self.append_terminal_progress(
            LlmProgressStage::Cancelled,
            "Report generation cancelled",
            None,
        );
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

    pub fn mark_completed(&mut self, response: &LlmGenerateResponse, now: &str) {
        if self.snapshot.status.is_terminal() {
            return;
        }
        let (stage, message) = match response.completion_reason {
            LlmCompletionReason::SuccessCriteriaMet => {
                (LlmProgressStage::Completed, "Success criteria met")
            }
            LlmCompletionReason::MaxIterationsReached => (
                LlmProgressStage::MaxIterationsReached,
                "Maximum iterations reached; saved the latest draft",
            ),
        };
        self.snapshot.status = LlmReportRunStatus::Completed;
        self.snapshot.stage = Some(stage);
        self.snapshot.iteration = response.iterations_used;
        self.snapshot.max_iterations = response.max_iterations;
        self.snapshot.message = message.into();
        self.snapshot.detail = Some(response.quality_feedback.clone());
        self.snapshot.error = None;
        self.snapshot.model = response.model.clone();
        self.snapshot.report_id = Some(response.report_id.clone());
        self.snapshot.report_path = response.report_path.clone();
        self.snapshot.completion_reason = Some(response.completion_reason);
        self.snapshot.success_criteria_met = Some(response.success_criteria_met);
        self.snapshot.quality_feedback = Some(response.quality_feedback.clone());
        self.snapshot.usage = Some(response.usage.clone());
        self.snapshot.finished_at = Some(now.into());
        self.append_terminal_progress(stage, message, Some(response.quality_feedback.clone()));
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
pub struct ActiveLlmRun {
    pub run_id: String,
    pub cancellation: CancellationToken,
}

#[derive(Debug, Default)]
pub struct LlmRunRuntime {
    active: Option<ActiveLlmRun>,
}

impl LlmRunRuntime {
    pub fn active(&self) -> Option<ActiveLlmRun> {
        self.active.clone()
    }

    pub fn start(
        &mut self,
        store: &LlmRunStore,
        mut request: LlmGenerateRequest,
        parent_run_id: Option<String>,
        model: &str,
        max_iterations: u8,
        now: &str,
    ) -> Result<LlmReportRunSnapshot, String> {
        if let Some(active) = &self.active {
            return Err(format!("Report run {} is already active", active.run_id));
        }
        if request.transcripts.is_empty() {
            return Err("Select at least one transcript".into());
        }
        let run_id = next_run_id();
        request.run_id = Some(run_id.clone());
        let record = LlmReportRunRecord::queued(
            run_id.clone(),
            parent_run_id,
            request,
            model.into(),
            max_iterations,
            now.into(),
        );
        store.write(&record)?;
        self.active = Some(ActiveLlmRun {
            run_id,
            cancellation: CancellationToken::new(),
        });
        Ok(record.snapshot())
    }

    pub fn retry(
        &mut self,
        store: &LlmRunStore,
        source_run_id: &str,
        model: &str,
        max_iterations: u8,
        now: &str,
    ) -> Result<LlmReportRunSnapshot, String> {
        if let Some(active) = &self.active {
            return Err(format!("Report run {} is already active", active.run_id));
        }
        let source = store.load(source_run_id)?;
        if !source.snapshot.status.is_terminal() {
            return Err(format!("Report run {source_run_id} is not retryable"));
        }
        self.start(
            store,
            source.request,
            Some(source_run_id.into()),
            model,
            max_iterations,
            now,
        )
    }

    pub fn progress(
        &mut self,
        store: &LlmRunStore,
        event: LlmProgressEvent,
        now: &str,
    ) -> Result<Option<LlmReportRunSnapshot>, String> {
        if !self.matches_active(&event.run_id) {
            return Ok(None);
        }
        let mut record = store.load(&event.run_id)?;
        if record.snapshot.status.is_terminal()
            || record.snapshot.status == LlmReportRunStatus::Stopping
        {
            return Ok(None);
        }
        if record.snapshot.status == LlmReportRunStatus::Queued {
            record.mark_running(now);
        }
        record.apply_progress(event, now);
        store.write(&record)?;
        Ok(Some(record.snapshot()))
    }

    pub fn request_cancel(
        &mut self,
        store: &LlmRunStore,
        run_id: &str,
        now: &str,
    ) -> Result<LlmReportRunSnapshot, String> {
        let mut record = store.load(run_id)?;
        if record.snapshot.status.is_terminal()
            || record.snapshot.status == LlmReportRunStatus::Stopping
        {
            return Ok(record.snapshot());
        }
        let active = self
            .active
            .as_ref()
            .filter(|active| active.run_id == run_id)
            .ok_or_else(|| format!("Report run {run_id} is not active"))?;
        let cancellation = active.cancellation.clone();
        record.mark_stopping(now);
        store.write(&record)?;
        cancellation.cancel();
        Ok(record.snapshot())
    }

    pub fn finish_cancelled(
        &mut self,
        store: &LlmRunStore,
        run_id: &str,
        now: &str,
    ) -> Result<Option<LlmReportRunSnapshot>, String> {
        if !self.matches_active(run_id) {
            return Ok(None);
        }
        let mut record = store.load(run_id)?;
        if record.snapshot.status.is_terminal() {
            self.clear_active(run_id);
            return Ok(Some(record.snapshot()));
        }
        record.mark_cancelled(now);
        store.write(&record)?;
        self.clear_active(run_id);
        Ok(Some(record.snapshot()))
    }

    pub fn fail(
        &mut self,
        store: &LlmRunStore,
        run_id: &str,
        error: &str,
        now: &str,
    ) -> Result<Option<LlmReportRunSnapshot>, String> {
        if !self.matches_active(run_id) {
            return Ok(None);
        }
        let mut record = store.load(run_id)?;
        if record.snapshot.status == LlmReportRunStatus::Stopping {
            record.mark_cancelled(now);
            store.write(&record)?;
            self.clear_active(run_id);
            return Ok(Some(record.snapshot()));
        }
        if record.snapshot.status.is_terminal() {
            self.clear_active(run_id);
            return Ok(None);
        }
        record.mark_failed(error, now);
        store.write(&record)?;
        self.clear_active(run_id);
        Ok(Some(record.snapshot()))
    }

    pub fn complete(
        &mut self,
        store: &LlmRunStore,
        run_id: &str,
        response: &LlmGenerateResponse,
        now: &str,
    ) -> Result<Option<LlmReportRunSnapshot>, String> {
        if !self.matches_active(run_id) {
            return Ok(None);
        }
        let mut record = store.load(run_id)?;
        if record.snapshot.status == LlmReportRunStatus::Stopping {
            return Ok(None);
        }
        if record.snapshot.status.is_terminal() {
            self.clear_active(run_id);
            return Ok(None);
        }
        record.mark_completed(response, now);
        store.write(&record)?;
        self.clear_active(run_id);
        Ok(Some(record.snapshot()))
    }

    fn matches_active(&self, run_id: &str) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.run_id == run_id)
    }

    fn clear_active(&mut self, run_id: &str) {
        if self.matches_active(run_id) {
            self.active = None;
        }
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

    /// Recover only runs that were already active at `cutoff`.
    ///
    /// The startup scan can be delayed by filesystem privacy prompts. Comparing
    /// the persisted update time prevents it from failing a run started after
    /// this process launched. Records with malformed timestamps are left alone
    /// because their age cannot be established safely.
    pub fn recover_interrupted_before(
        &self,
        now: &str,
        cutoff: &str,
    ) -> Result<Vec<LlmReportRunSnapshot>, String> {
        let cutoff = chrono::DateTime::parse_from_rfc3339(cutoff)
            .map_err(|error| format!("Invalid recovery cutoff: {error}"))?;
        let mut recovered = Vec::new();
        for mut record in self.list()? {
            if record.snapshot.status.is_terminal() {
                continue;
            }
            let Ok(updated_at) = chrono::DateTime::parse_from_rfc3339(&record.snapshot.updated_at)
            else {
                continue;
            };
            if updated_at > cutoff {
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

fn next_run_id() -> String {
    let sequence = RUN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "llm-report-{}-{sequence}",
        chrono::Utc::now().timestamp_millis()
    )
}
