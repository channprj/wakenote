use std::collections::HashSet;

use wakenote::queue::{
    BacklogScan, QueueActivityKind, QueueIssueCode, QueueIssueSeverity, QueueJobIssue,
    QueueJobStatus, TranscriptionQueue,
};
use wakenote::settings::TranscriptionOptions;

#[test]
fn queue_can_start_cancel_fail_retry_and_skip_jobs() {
    let mut queue = TranscriptionQueue::new();
    let first = queue.enqueue_file("/recordings/20260506/230709.m4a", "whisper-medium");
    let second = queue.enqueue_file("/recordings/20260506/230810.wav", "whisper-small");

    assert_eq!(queue.snapshot().pending_count, 2);
    assert_eq!(queue.snapshot().running_count, 0);

    let running = queue.start_next().expect("next job");
    assert_eq!(running.id, first);
    assert_eq!(queue.job(first).unwrap().status, QueueJobStatus::Running);

    queue
        .cancel_current("user cancelled")
        .expect("cancel current");
    assert_eq!(queue.job(first).unwrap().status, QueueJobStatus::Cancelled);
    assert_eq!(queue.snapshot().pending_count, 1);

    queue
        .mark_failed(second, "model missing")
        .expect("fail job");
    assert_eq!(queue.job(second).unwrap().status, QueueJobStatus::Failed);
    queue.retry(second).expect("retry failed job");
    assert_eq!(queue.job(second).unwrap().status, QueueJobStatus::Pending);
    queue.skip(second).expect("skip job");
    assert_eq!(queue.job(second).unwrap().status, QueueJobStatus::Skipped);
    assert_eq!(queue.job(second).unwrap().error, None);
}

#[test]
fn queue_cancels_only_the_requested_running_job() {
    let mut queue = TranscriptionQueue::new();
    let first = queue.enqueue_file("/recordings/first.m4a", "whisper-medium");
    let second = queue.enqueue_file("/recordings/second.m4a", "whisper-medium");

    let running = queue.start_next().expect("first running job");
    assert_eq!(running.id, first);
    assert!(queue.has_running_audio_path(&running.audio_path));

    assert_eq!(
        queue.cancel(second, "cancelled by user").unwrap_err(),
        format!("job {second} is not running")
    );
    assert_eq!(queue.job(first).unwrap().status, QueueJobStatus::Running);
    assert_eq!(queue.job(second).unwrap().status, QueueJobStatus::Pending);

    queue
        .cancel(first, "cancelled by user")
        .expect("cancel first");
    assert_eq!(queue.job(first).unwrap().status, QueueJobStatus::Cancelled);
    assert!(!queue.has_running_audio_path(&running.audio_path));
}

#[test]
fn queue_classifies_dictation_live_capture_and_imported_audio() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let live_path = tmp.path().join("live.wav");
    let dictation_path = tmp.path().join("dictation.wav");
    let uploaded_dir = tmp.path().join("uploaded").join("20260807");
    std::fs::create_dir_all(&uploaded_dir).expect("uploaded dir");
    let uploaded_path = uploaded_dir.join("import.wav");
    for path in [&live_path, &dictation_path, &uploaded_path] {
        std::fs::write(path, b"audio").expect("audio fixture");
    }
    std::fs::write(
        live_path.with_extension("json"),
        br#"{"source":"microphone","source_label":"Primary"}"#,
    )
    .expect("live metadata");
    std::fs::write(
        dictation_path.with_extension("json"),
        br#"{"source":"microphone","source_label":"dictation"}"#,
    )
    .expect("dictation metadata");

    let mut queue = TranscriptionQueue::new();
    let live = queue.enqueue_file(&live_path, "whisper-medium");
    let dictation = queue.enqueue_file(&dictation_path, "whisper-medium");
    let uploaded = queue.enqueue_file(&uploaded_path, "whisper-medium");

    assert_eq!(
        queue.job(live).expect("live job").activity_kind,
        QueueActivityKind::LiveTranscription
    );
    assert_eq!(
        queue.job(dictation).expect("dictation job").activity_kind,
        QueueActivityKind::Dictation
    );
    assert_eq!(
        queue.job(uploaded).expect("uploaded job").activity_kind,
        QueueActivityKind::ImportedAudio
    );
}

#[test]
fn queue_persists_warning_issues_and_clears_them_when_retried() {
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file("/recordings/empty.wav", "openai-gpt-transcribe");
    let warning = QueueJobIssue {
        severity: QueueIssueSeverity::Warning,
        code: QueueIssueCode::EmptyTranscript,
        message: "OpenAI returned an empty transcript".to_string(),
        occurred_at: Some("2026-08-04T01:02:03+00:00".to_string()),
    };

    queue
        .mark_failed_with_issue(id, warning.clone())
        .expect("warning outcome");

    let encoded = serde_json::to_string(&queue).expect("serialize queue");
    let mut decoded: TranscriptionQueue = serde_json::from_str(&encoded).expect("queue json");
    assert_eq!(decoded.job(id).expect("job").issue, Some(warning));
    assert_eq!(decoded.unread_attention_count(), 1);
    assert_eq!(decoded.unread_error_count(), 0);

    decoded.retry(id).expect("retry warning");
    assert_eq!(decoded.job(id).expect("retried job").issue, None);
    assert_eq!(decoded.job(id).expect("retried job").error, None);
}

#[test]
fn completed_warnings_participate_in_attention_without_becoming_errors() {
    let mut queue = TranscriptionQueue::new();
    let warning = queue.enqueue_file("/recordings/artifact.wav", "whisper-medium");
    let clean = queue.enqueue_file("/recordings/clean.wav", "whisper-medium");
    queue
        .mark_completed_with_issue(
            warning,
            QueueJobIssue::warning(
                QueueIssueCode::TranscriptArtifact,
                "Low-confidence transcript: common-hallucination",
            ),
        )
        .expect("completed warning");
    queue.mark_completed(clean).expect("clean completion");

    assert_eq!(queue.unread_attention_count(), 1);
    assert_eq!(queue.unread_error_count(), 0);
    assert_eq!(queue.mark_attention_outcomes_read(), 1);
    assert!(queue.job(warning).expect("warning").is_read);
    assert!(!queue.job(clean).expect("clean").is_read);
}

#[test]
fn queue_marks_failed_cancelled_and_skipped_outcomes_read() {
    let mut queue = TranscriptionQueue::new();
    let failed = queue.enqueue_file("/recordings/failed.wav", "whisper-medium");
    let cancelled = queue.enqueue_file("/recordings/cancelled.wav", "whisper-medium");
    let skipped = queue.enqueue_file("/recordings/skipped.wav", "whisper-medium");
    let pending = queue.enqueue_file("/recordings/pending.wav", "whisper-medium");
    let completed = queue.enqueue_file("/recordings/completed.wav", "whisper-medium");

    queue
        .mark_failed(failed, "model missing")
        .expect("fail job");
    assert_eq!(queue.start_next().expect("start job").id, cancelled);
    queue
        .cancel_current("cancelled by user")
        .expect("cancel job");
    queue.skip(skipped).expect("skip job");
    queue.mark_completed(completed).expect("complete job");

    assert_eq!(queue.unread_attention_count(), 3);
    assert_eq!(queue.mark_attention_outcomes_read(), 3);
    assert_eq!(queue.unread_attention_count(), 0);
    assert!(queue.job(failed).expect("failed job").is_read);
    assert!(queue.job(cancelled).expect("cancelled job").is_read);
    assert!(queue.job(skipped).expect("skipped job").is_read);
    assert!(!queue.job(pending).expect("pending job").is_read);
    assert!(!queue.job(completed).expect("completed job").is_read);
    assert_eq!(queue.mark_attention_outcomes_read(), 0);
}

#[test]
fn queue_reopened_outcomes_become_unread_again() {
    let mut queue = TranscriptionQueue::new();
    let failed = queue.enqueue_file("/recordings/failed.wav", "whisper-medium");
    let skipped = queue.enqueue_file("/recordings/skipped.wav", "whisper-medium");
    queue
        .mark_failed(failed, "model missing")
        .expect("fail job");
    queue.skip(skipped).expect("skip job");
    queue.mark_attention_outcomes_read();

    queue.retry(failed).expect("retry failed job");
    queue
        .requeue_file("/recordings/skipped.wav", "whisper-small")
        .expect("requeue skipped job");

    assert!(!queue.job(failed).expect("retried job").is_read);
    assert!(!queue.job(skipped).expect("requeued job").is_read);
}

#[test]
fn changing_a_read_failure_to_skipped_makes_the_new_outcome_unread() {
    let mut queue = TranscriptionQueue::new();
    let failed = queue.enqueue_file("/recordings/failed.wav", "whisper-medium");
    queue
        .mark_failed(failed, "model missing")
        .expect("fail job");
    queue.mark_attention_outcomes_read();

    queue.skip(failed).expect("skip failed job");

    assert_eq!(
        queue.job(failed).expect("skipped job").status,
        QueueJobStatus::Skipped
    );
    assert!(!queue.job(failed).expect("skipped job").is_read);
    assert_eq!(queue.unread_attention_count(), 1);
}

#[test]
fn queue_preserves_transcription_options_across_retry() {
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file("/recordings/fallback.wav", "openai-gpt-transcribe");
    let options = TranscriptionOptions {
        streaming_enabled: true,
        cost_limit_fallback_enabled: true,
        cost_limit_fallback_model_id: Some("whisper-medium".to_string()),
    };
    queue
        .set_transcription_options(id, options.clone())
        .expect("snapshot options");

    queue.mark_failed(id, "billing limit").expect("fail job");
    queue.retry(id).expect("retry job");

    assert_eq!(queue.job(id).unwrap().transcription_options, Some(options));
}

#[test]
fn skipping_failed_or_cancelled_jobs_clears_stale_errors() {
    let mut queue = TranscriptionQueue::new();
    let cancelled = queue.enqueue_file("/recordings/cancelled.wav", "whisper-medium");
    let failed = queue.enqueue_file("/recordings/failed.wav", "whisper-medium");

    queue.start_next().expect("start cancelled job");
    queue
        .cancel_current("user cancelled")
        .expect("cancel current");
    queue
        .mark_failed(failed, "model missing")
        .expect("fail job");

    queue.skip(cancelled).expect("skip cancelled job");
    queue.skip(failed).expect("skip failed job");

    assert_eq!(
        queue.job(cancelled).unwrap().status,
        QueueJobStatus::Skipped
    );
    assert_eq!(queue.job(cancelled).unwrap().error, None);
    assert_eq!(queue.job(failed).unwrap().status, QueueJobStatus::Skipped);
    assert_eq!(queue.job(failed).unwrap().error, None);
}

#[test]
fn starting_pending_jobs_clears_recovery_errors() {
    let mut queue = TranscriptionQueue::new();
    let recovered = queue.enqueue_file("/recordings/recovered.wav", "whisper-medium");
    let recovered_job = queue
        .jobs_mut()
        .iter_mut()
        .find(|job| job.id == recovered)
        .expect("recovered job");
    recovered_job.status = QueueJobStatus::Pending;
    recovered_job.error = Some("recovered from interrupted app session".to_string());

    let running = queue.start_next().expect("start recovered job");

    assert_eq!(running.id, recovered);
    assert_eq!(running.error, None);
    assert_eq!(queue.job(recovered).unwrap().error, None);
}

#[test]
fn queue_does_not_enqueue_duplicate_audio_paths() {
    let mut queue = TranscriptionQueue::new();

    let first = queue.enqueue_file("/recordings/20260506/230709.m4a", "whisper-medium");
    let duplicate = queue.enqueue_file("/recordings/20260506/230709.m4a", "whisper-small");

    assert_eq!(duplicate, first);
    assert_eq!(queue.snapshot().pending_count, 1);
    assert_eq!(queue.snapshot().jobs[0].model_id, "whisper-medium");
}

#[test]
fn queue_does_not_start_next_job_while_another_job_is_running() {
    let mut queue = TranscriptionQueue::new();
    let first = queue.enqueue_file("/recordings/first.wav", "whisper-medium");
    let second = queue.enqueue_file("/recordings/second.wav", "whisper-medium");

    let started = queue.start_next().expect("first job");
    let attempted_second = queue.start_next();

    assert_eq!(started.id, first);
    assert_eq!(attempted_second, None);
    assert_eq!(queue.job(first).unwrap().status, QueueJobStatus::Running);
    assert_eq!(queue.job(second).unwrap().status, QueueJobStatus::Pending);
    assert_eq!(queue.snapshot().running_count, 1);
    assert_eq!(queue.snapshot().pending_count, 1);
}

#[test]
fn queue_starts_only_one_job_even_when_parallel_limit_is_higher() {
    let mut queue = TranscriptionQueue::new();
    let first = queue.enqueue_file("/recordings/first.wav", "whisper-medium");
    let second = queue.enqueue_file("/recordings/second.wav", "whisper-medium");
    let third = queue.enqueue_file("/recordings/third.wav", "whisper-medium");
    let selectable = HashSet::from(["whisper-medium".to_string()]);

    let started_first = queue
        .start_next_for_model_ids_up_to(&selectable, 2)
        .expect("first sequential job");
    let blocked_second = queue.start_next_for_model_ids_up_to(&selectable, 2);
    let blocked_third = queue.start_next_for_model_ids_up_to(&selectable, 2);

    assert_eq!(started_first.id, first);
    assert_eq!(blocked_second, None);
    assert_eq!(blocked_third, None);
    assert_eq!(queue.job(first).unwrap().status, QueueJobStatus::Running);
    assert_eq!(queue.job(second).unwrap().status, QueueJobStatus::Pending);
    assert_eq!(queue.job(third).unwrap().status, QueueJobStatus::Pending);
    assert_eq!(queue.snapshot().running_count, 1);
    assert_eq!(queue.snapshot().pending_count, 2);
}

#[test]
fn queue_prunes_old_completed_jobs_but_keeps_active_and_recent_completed_jobs() {
    let mut queue = TranscriptionQueue::new();
    let old_completed = queue.enqueue_file("/recordings/old-completed.wav", "whisper-medium");
    let recent_completed = queue.enqueue_file("/recordings/recent-completed.wav", "whisper-medium");
    let pending = queue.enqueue_file("/recordings/pending.wav", "whisper-medium");
    let running = queue.enqueue_file("/recordings/running.wav", "whisper-medium");
    let failed = queue.enqueue_file("/recordings/failed.wav", "whisper-medium");

    queue.mark_completed(old_completed).expect("old completed");
    queue
        .mark_completed(recent_completed)
        .expect("recent completed");
    queue.start_next().expect("start pending");
    queue.mark_failed(failed, "model missing").expect("failed");

    assert!(queue.prune_completed_history(1));

    assert_eq!(queue.job(old_completed), None);
    assert_eq!(
        queue.job(recent_completed).unwrap().status,
        QueueJobStatus::Completed
    );
    assert_eq!(queue.job(pending).unwrap().status, QueueJobStatus::Running);
    assert_eq!(queue.job(running).unwrap().status, QueueJobStatus::Pending);
    assert_eq!(queue.job(failed).unwrap().status, QueueJobStatus::Failed);
}

#[test]
fn queue_retries_terminal_jobs_but_rejects_active_jobs() {
    let mut queue = TranscriptionQueue::new();
    let pending = queue.enqueue_file("/recordings/pending.wav", "whisper-medium");
    let running = queue.enqueue_file("/recordings/running.wav", "whisper-medium");
    let completed = queue.enqueue_file("/recordings/completed.wav", "whisper-medium");
    let skipped = queue.enqueue_file("/recordings/skipped.wav", "whisper-medium");

    queue.start_next().expect("start pending");
    queue
        .cancel_current("user cancelled")
        .expect("cancel first");
    queue.start_next().expect("start running");
    queue.mark_completed(completed).expect("complete job");
    queue.skip(skipped).expect("skip job");

    assert!(queue.skip(pending).is_ok(), "cancelled jobs can skip");
    assert!(queue.skip(running).is_err(), "running jobs cannot skip");
    assert!(queue.skip(completed).is_err(), "completed jobs cannot skip");

    assert!(queue.retry(pending).is_ok(), "skipped jobs can rerun");
    assert!(queue.retry(running).is_err(), "running jobs cannot retry");
    assert!(queue.retry(completed).is_ok(), "completed jobs can rerun");
    assert!(queue.retry(skipped).is_ok(), "skipped jobs can rerun");
}

#[test]
fn backlog_scan_enqueues_audio_without_txt_or_error_sidecar() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let day = tmp.path().join("20260506");
    std::fs::create_dir_all(&day).expect("day dir");
    std::fs::write(day.join("230709.m4a"), b"audio").expect("untranscribed audio");
    std::fs::write(day.join("230810.wav"), b"audio").expect("transcribed audio");
    std::fs::write(day.join("230810.txt"), b"text").expect("transcript");
    std::fs::write(day.join("230911.m4a"), b"audio").expect("failed audio");
    std::fs::write(day.join("230911.error.txt"), b"error").expect("error sidecar");
    std::fs::write(day.join("notes.txt"), b"ignore").expect("notes");

    let scan = BacklogScan::scan(tmp.path()).expect("scan");
    assert_eq!(scan.pending_audio.len(), 1);
    assert!(scan.pending_audio[0].ends_with("20260506/230709.m4a"));

    let mut queue = TranscriptionQueue::new();
    let added = queue.enqueue_backlog(scan, "whisper-medium");

    assert_eq!(added.len(), 1);
    assert_eq!(queue.snapshot().pending_count, 1);
    assert!(
        queue
            .job(added[0])
            .unwrap()
            .audio_path
            .ends_with("230709.m4a")
    );
}

#[test]
fn backlog_scan_recovers_whitespace_only_transcript_sidecars() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let day = tmp.path().join("20260731");
    std::fs::create_dir_all(&day).expect("day dir");
    let audio_path = day.join("010203.m4a");
    std::fs::write(&audio_path, b"recoverable audio").expect("audio");
    std::fs::write(audio_path.with_extension("txt"), b"\n").expect("blank transcript");

    let scan = BacklogScan::scan(tmp.path()).expect("scan");

    assert_eq!(scan.pending_audio, [audio_path]);
}

#[test]
fn backlog_enqueue_requeues_completed_job_when_its_transcript_is_missing() {
    let audio_path = std::path::PathBuf::from("/recordings/20260731/010203.m4a");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "old-model");
    let running = queue.start_next().expect("running job");
    assert_eq!(running.id, id);
    queue.mark_completed(id).expect("complete job");

    let added = queue.enqueue_backlog(
        BacklogScan {
            pending_audio: vec![audio_path.clone()],
        },
        "whisper-medium",
    );

    assert_eq!(added, [id]);
    let job = queue.job(id).expect("requeued job");
    assert_eq!(job.status, QueueJobStatus::Pending);
    assert_eq!(job.model_id, "whisper-medium");
    assert_eq!(queue.snapshot().pending_count, 1);
}

#[test]
fn backlog_enqueue_ignores_files_already_in_queue() {
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file("/recordings/20260506/230709.wav", "whisper-medium");

    let added = queue.enqueue_backlog(
        BacklogScan {
            pending_audio: vec![
                "/recordings/20260506/230709.wav".into(),
                "/recordings/20260506/230810.wav".into(),
            ],
        },
        "whisper-medium",
    );

    assert_eq!(added.len(), 1);
    assert_eq!(queue.snapshot().pending_count, 2);
    assert!(
        queue
            .job(added[0])
            .unwrap()
            .audio_path
            .ends_with("20260506/230810.wav")
    );
}
