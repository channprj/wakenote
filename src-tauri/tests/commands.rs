use sagwan::commands::{AppBackend, AppMode, TrayState};
use sagwan::recorder::ChunkMetadata;
use sagwan::settings::{AudioFormat, SettingsPatch};
use sagwan::transcription::{Transcriber, TranscriptionError, TranscriptionRequest};

#[derive(Clone)]
struct StaticTranscriber;

impl Transcriber for StaticTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        assert_eq!(request.model_id, "whisper-medium");
        assert!(request.audio_path.exists());
        Ok("queued transcript".to_string())
    }
}

fn wav_settings_patch(save_root: &std::path::Path) -> SettingsPatch {
    SettingsPatch {
        save_root: Some(save_root.to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        transcription_enabled: Some(true),
        ..SettingsPatch::default()
    }
}

#[test]
fn backend_derives_four_product_modes_from_independent_toggles() {
    let mut backend = AppBackend::default();

    assert_eq!(
        backend.app_status().mode,
        AppMode::RecordingAndTranscription
    );
    assert_eq!(backend.app_status().tray_state, TrayState::Listening);

    backend.update_settings(SettingsPatch {
        transcription_enabled: Some(false),
        ..SettingsPatch::default()
    });
    assert_eq!(backend.app_status().mode, AppMode::RecordingOnly);
    assert_eq!(backend.app_status().tray_state, TrayState::Listening);

    backend.update_settings(SettingsPatch {
        recording_enabled: Some(false),
        transcription_enabled: Some(true),
        ..SettingsPatch::default()
    });
    assert_eq!(backend.app_status().mode, AppMode::TranscriptionOnly);
    assert_eq!(backend.app_status().tray_state, TrayState::Idle);

    backend.update_settings(SettingsPatch {
        transcription_enabled: Some(false),
        ..SettingsPatch::default()
    });
    assert_eq!(backend.app_status().mode, AppMode::Paused);
    assert_eq!(backend.app_status().tray_state, TrayState::Paused);
}

#[test]
fn pause_all_overrides_enabled_toggles() {
    let mut backend = AppBackend::default();

    backend.update_settings(SettingsPatch {
        pause_all: Some(true),
        ..SettingsPatch::default()
    });

    assert_eq!(backend.app_status().mode, AppMode::Paused);
    assert_eq!(backend.app_status().tray_state, TrayState::Paused);
}

#[test]
fn microphone_picker_always_includes_system_default() {
    let backend = AppBackend::default();
    let devices = backend.list_microphones();

    assert_eq!(devices[0].id, "default");
    assert_eq!(devices[0].label, "System Default");
    assert!(devices[0].available);
}

#[test]
fn backend_enqueues_completed_capture_chunks_when_transcription_is_enabled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(wav_settings_patch(tmp.path()));
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");

    for _ in 0..5 {
        backend
            .process_audio_samples_for_test(&[0.8; 1], 100)
            .expect("speech");
    }
    for _ in 0..18 {
        backend
            .process_audio_samples_for_test(&[0.0; 1], 100)
            .expect("silence");
    }

    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(snapshot.jobs[0].audio_path.ends_with("19700101/000000.wav"));
}

#[test]
fn backend_records_fallback_microphone_in_status_and_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        selected_microphone: Some("input-missing-airpods".to_string()),
        selected_microphone_label: Some("Missing AirPods".to_string()),
        ..SettingsPatch::default()
    });
    let status = backend
        .start_capture_session_with_device(
            10,
            chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            "default",
            "System Default",
            true,
        )
        .expect("start fallback capture session");

    assert_eq!(status.active_microphone, "System Default");
    assert_eq!(
        status.microphone_warning.as_deref(),
        Some("Pinned microphone Missing AirPods is unavailable; using System Default")
    );

    for _ in 0..5 {
        backend
            .process_audio_samples_for_test(&[0.8; 1], 100)
            .expect("speech");
    }
    backend.stop_capture_session().expect("stop");

    let metadata_path = tmp.path().join("19700101").join("000000.json");
    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(metadata_path).expect("metadata"))
            .expect("metadata json");
    assert_eq!(metadata.device_id, "default");
    assert_eq!(metadata.device_name, "System Default");
    assert!(metadata.used_fallback_device);
}

#[test]
fn backend_reports_real_level_snapshot_from_processed_audio_frames() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(wav_settings_patch(tmp.path()));
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");

    let silent_status = backend
        .process_audio_samples_for_test(&[0.0; 1], 100)
        .expect("silence");
    assert_eq!(silent_status.level.current_dbfs, -120.0);
    assert_eq!(silent_status.level.peak_dbfs, -120.0);

    let speech_status = backend
        .process_audio_samples_for_test(&[0.5; 1], 100)
        .expect("speech");
    assert!((speech_status.level.current_dbfs - -6.0206).abs() < 0.001);
    assert!((speech_status.level.peak_dbfs - -6.0206).abs() < 0.001);
    assert_eq!(speech_status.level.noise_floor_dbfs, -120.0);
    assert_eq!(speech_status.level.suggested_threshold_dbfs, -90.0);
}

#[test]
fn backend_flushes_active_capture_when_recording_is_disabled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(wav_settings_patch(tmp.path()));
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");

    for _ in 0..5 {
        backend
            .process_audio_samples_for_test(&[0.8; 1], 100)
            .expect("speech");
    }

    backend.update_settings(SettingsPatch {
        recording_enabled: Some(false),
        ..SettingsPatch::default()
    });

    let status = backend.app_status();
    assert_eq!(status.tray_state, TrayState::Idle);
    assert_eq!(backend.queue_snapshot().pending_count, 1);
}

#[test]
fn backend_processes_next_transcription_job_and_writes_sidecar() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230912.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::default();
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    let snapshot = backend
        .process_next_transcription_with(StaticTranscriber)
        .expect("process transcription");

    assert_eq!(snapshot.pending_count, 0);
    assert_eq!(
        snapshot.jobs[0].status,
        sagwan::queue::QueueJobStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("transcript"),
        "queued transcript\n"
    );
}

#[test]
fn backend_processes_all_pending_transcription_jobs_with_worker_loop() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let first_audio = tmp.path().join("20260506").join("231114.wav");
    let second_audio = tmp.path().join("20260506").join("231115.wav");
    std::fs::create_dir_all(first_audio.parent().unwrap()).expect("audio dir");
    std::fs::write(&first_audio, b"wav bytes").expect("first audio");
    std::fs::write(&second_audio, b"wav bytes").expect("second audio");
    let mut backend = AppBackend::default();
    backend.enqueue_audio_file(&first_audio, Some("whisper-medium".to_string()));
    backend.enqueue_audio_file(&second_audio, Some("whisper-medium".to_string()));

    let snapshot = backend
        .process_pending_transcriptions_with(StaticTranscriber)
        .expect("process pending transcriptions");

    assert_eq!(snapshot.pending_count, 0);
    assert_eq!(snapshot.failed_count, 0);
    assert!(
        snapshot
            .jobs
            .iter()
            .all(|job| job.status == sagwan::queue::QueueJobStatus::Completed)
    );
    assert_eq!(
        std::fs::read_to_string(first_audio.with_extension("txt")).expect("first transcript"),
        "queued transcript\n"
    );
    assert_eq!(
        std::fs::read_to_string(second_audio.with_extension("txt")).expect("second transcript"),
        "queued transcript\n"
    );
}

#[test]
fn backend_auto_transcription_loop_respects_disabled_transcription_toggle() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("231216.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        transcription_enabled: Some(false),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    let snapshot = backend
        .process_pending_transcriptions_with(StaticTranscriber)
        .expect("process pending transcriptions");

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(
        snapshot.jobs[0].status,
        sagwan::queue::QueueJobStatus::Pending
    );
    assert!(!audio_path.with_extension("txt").exists());
}

#[test]
fn backend_default_transcription_worker_writes_error_when_model_is_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("231013.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(tmp.path().join("models").to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    let snapshot = backend
        .process_next_transcription()
        .expect("process transcription");

    assert_eq!(snapshot.failed_count, 1);
    assert_eq!(
        snapshot.jobs[0].status,
        sagwan::queue::QueueJobStatus::Failed
    );
    assert!(audio_path.exists());
    assert!(
        std::fs::read_to_string(audio_path.with_extension("error.txt"))
            .expect("error sidecar")
            .contains("model file not found")
    );
}
