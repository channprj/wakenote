use wakenote::commands::RecentTranscript;
use wakenote::llm::{LlmGenerateRequest, LlmProgressEvent, LlmProgressStage, LlmReportKind};
use wakenote::llm_runs::{LlmReportRunRecord, LlmReportRunStatus, LlmRunRuntime, LlmRunStore};
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

#[test]
fn stale_progress_cannot_overwrite_a_cancelled_run() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());
    let mut runtime = LlmRunRuntime::default();
    let started = runtime
        .start(
            &store,
            request(),
            None,
            "z-ai/glm-5.2",
            3,
            "2026-07-18T00:00:00Z",
        )
        .unwrap();
    let stopping = runtime
        .request_cancel(&store, &started.run_id, "2026-07-18T00:00:01Z")
        .unwrap();
    assert_eq!(stopping.status, LlmReportRunStatus::Stopping);
    runtime
        .finish_cancelled(&store, &started.run_id, "2026-07-18T00:00:01Z")
        .unwrap();

    let ignored = runtime
        .progress(
            &store,
            LlmProgressEvent {
                run_id: started.run_id.clone(),
                stage: LlmProgressStage::Generating,
                iteration: 1,
                max_iterations: 3,
                message: "Late event".into(),
                detail: None,
            },
            "2026-07-18T00:00:02Z",
        )
        .unwrap();

    assert!(ignored.is_none());
    assert_eq!(
        store.load(&started.run_id).unwrap().snapshot.status,
        LlmReportRunStatus::Cancelled
    );
}

#[test]
fn accepted_progress_is_persisted_in_arrival_order() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());
    let mut runtime = LlmRunRuntime::default();
    let started = runtime
        .start(
            &store,
            request(),
            None,
            "z-ai/glm-5.2",
            3,
            "2026-07-18T00:00:00Z",
        )
        .unwrap();

    for (stage, message) in [
        (LlmProgressStage::Preparing, "Preparing input"),
        (LlmProgressStage::Generating, "Drafting report"),
        (LlmProgressStage::Evaluating, "Checking success criteria"),
    ] {
        runtime
            .progress(
                &store,
                LlmProgressEvent {
                    run_id: started.run_id.clone(),
                    stage,
                    iteration: 1,
                    max_iterations: 3,
                    message: message.into(),
                    detail: None,
                },
                "2026-07-18T00:00:01Z",
            )
            .unwrap();
    }

    let progress = store.load(&started.run_id).unwrap().snapshot.progress;
    assert_eq!(
        progress
            .iter()
            .map(|event| event.message.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Preparing input",
            "Drafting report",
            "Checking success criteria"
        ]
    );
}

#[test]
fn runtime_rejects_a_second_active_run() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());
    let mut runtime = LlmRunRuntime::default();
    let first = runtime
        .start(
            &store,
            request(),
            None,
            "z-ai/glm-5.2",
            3,
            "2026-07-18T00:00:00Z",
        )
        .unwrap();

    let error = runtime
        .start(
            &store,
            request(),
            None,
            "z-ai/glm-5.2",
            3,
            "2026-07-18T00:00:01Z",
        )
        .unwrap_err();

    assert!(error.contains(&first.run_id));
}

#[test]
fn retry_clones_exact_input_under_a_new_linked_run() {
    let temp = tempfile::tempdir().unwrap();
    let store = LlmRunStore::new(temp.path());
    let mut runtime = LlmRunRuntime::default();
    let first = runtime
        .start(
            &store,
            request(),
            None,
            "z-ai/glm-5.2",
            3,
            "2026-07-18T00:00:00Z",
        )
        .unwrap();
    runtime
        .fail(
            &store,
            &first.run_id,
            "OpenRouter unavailable",
            "2026-07-18T00:00:01Z",
        )
        .unwrap();

    let retry = runtime
        .retry(
            &store,
            &first.run_id,
            "z-ai/glm-5.2",
            3,
            "2026-07-18T00:00:02Z",
        )
        .unwrap();
    let retry_record = store.load(&retry.run_id).unwrap();

    assert_ne!(retry.run_id, first.run_id);
    assert_eq!(retry.parent_run_id.as_deref(), Some(first.run_id.as_str()));
    assert_eq!(
        retry_record.request.transcripts[0].text,
        "Exact retry evidence"
    );
    assert_eq!(
        retry_record.request.run_id.as_deref(),
        Some(retry.run_id.as_str())
    );
}
