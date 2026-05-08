use sagwan::queue::{BacklogScan, QueueJobStatus, TranscriptionQueue};

#[test]
fn queue_can_start_cancel_fail_retry_and_skip_jobs() {
    let mut queue = TranscriptionQueue::new();
    let first = queue.enqueue_file("/recordings/20260506/230709.m4a", "whisper-medium");
    let second = queue.enqueue_file("/recordings/20260506/230810.wav", "whisper-tiny");

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
fn skipping_failed_or_cancelled_jobs_clears_stale_errors() {
    let mut queue = TranscriptionQueue::new();
    let cancelled = queue.enqueue_file("/recordings/cancelled.wav", "whisper-medium");
    let failed = queue.enqueue_file("/recordings/failed.wav", "whisper-medium");

    queue.start_next().expect("start cancelled job");
    queue
        .cancel_current("user cancelled")
        .expect("cancel current");
    queue.mark_failed(failed, "model missing").expect("fail job");

    queue.skip(cancelled).expect("skip cancelled job");
    queue.skip(failed).expect("skip failed job");

    assert_eq!(queue.job(cancelled).unwrap().status, QueueJobStatus::Skipped);
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
    let duplicate = queue.enqueue_file("/recordings/20260506/230709.m4a", "whisper-tiny");

    assert_eq!(duplicate, first);
    assert_eq!(queue.snapshot().pending_count, 1);
    assert_eq!(queue.snapshot().jobs[0].model_id, "whisper-medium");
}

#[test]
fn queue_rejects_retry_and_skip_for_terminal_or_active_jobs() {
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

    assert!(queue.retry(pending).is_ok(), "cancelled jobs can retry");
    assert!(queue.retry(running).is_err(), "running jobs cannot retry");
    assert!(
        queue.retry(completed).is_err(),
        "completed jobs cannot retry"
    );
    assert!(queue.retry(skipped).is_err(), "skipped jobs cannot retry");

    assert!(queue.skip(pending).is_ok(), "pending jobs can skip");
    assert!(queue.skip(running).is_err(), "running jobs cannot skip");
    assert!(queue.skip(completed).is_err(), "completed jobs cannot skip");
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
