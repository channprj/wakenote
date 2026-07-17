use wakenote::commands::RecentTranscript;
use wakenote::llm::{LlmGenerateRequest, LlmReportKind};
use wakenote::llm_runs::{LlmReportRunRecord, LlmReportRunStatus, LlmRunStore};
use wakenote::recorder::ChunkSource;

fn request() -> LlmGenerateRequest {
    LlmGenerateRequest {
        kind: LlmReportKind::Summary,
        transcripts: vec![RecentTranscript {
            transcript_path: "/tmp/WakeNote/20260718/090000.txt".into(),
            audio_path: None,
            recorded_at: "2026-07-18T09:00:00+09:00".into(),
            text: "Exact retry evidence".into(),
            source: ChunkSource::Microphone,
            source_label: None,
        }],
        run_id: None,
    }
}

#[test]
fn run_store_round_trips_private_request_but_public_snapshot_omits_it() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());
    let record = LlmReportRunRecord::queued(
        "llm-report-test-1".into(),
        None,
        request(),
        "z-ai/glm-5.2".into(),
        3,
        "2026-07-18T00:00:00Z".into(),
    );

    store.write(&record).unwrap();
    let loaded = store.load("llm-report-test-1").unwrap();
    let snapshot = loaded.snapshot();
    let runs_dir = temp.path().join("reports").join(".runs");

    assert_eq!(loaded.request.transcripts[0].text, "Exact retry evidence");
    assert_eq!(snapshot.selected_count, 1);
    assert_eq!(snapshot.run_id, "llm-report-test-1");
    assert_eq!(snapshot.status, LlmReportRunStatus::Queued);
    assert!(snapshot.progress.is_empty());
    let public_json = serde_json::to_string(&snapshot).unwrap();
    assert!(!public_json.contains("Exact retry evidence"));
    assert!(!public_json.contains("090000.txt"));
    assert!(runs_dir.join("llm-report-test-1.json").exists());
    assert!(!runs_dir.join("llm-report-test-1.json.tmp").exists());
}

#[test]
fn recovery_marks_non_terminal_records_failed_and_retryable() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());
    let mut record = LlmReportRunRecord::queued(
        "llm-report-test-2".into(),
        None,
        request(),
        "z-ai/glm-5.2".into(),
        3,
        "2026-07-18T00:00:00Z".into(),
    );
    record.mark_running("2026-07-18T00:00:01Z");
    store.write(&record).unwrap();

    let recovered = store.recover_interrupted("2026-07-18T00:05:00Z").unwrap();

    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].status, LlmReportRunStatus::Failed);
    assert_eq!(
        recovered[0].error.as_deref(),
        Some("WakeNote closed before this report finished")
    );
    assert!(recovered[0].revision > record.snapshot.revision);
}

#[test]
fn run_store_rejects_path_traversal_ids() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());

    let error = store.load("../settings").unwrap_err();

    assert!(error.contains("Invalid report run id"));
}
