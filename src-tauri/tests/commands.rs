use std::{collections::HashSet, path::PathBuf};

use sagwan::commands::{
    AppBackend, AppMode, MainWindowCloseAction, TrayState, main_window_close_action,
    reveal_save_folder_request, tray_menu_presentation, tray_presentation_for_state,
    tray_runtime_presentation, with_live_runtime_warning, with_runtime_warning,
};
use sagwan::models::{ModelStatus, ModelStore};
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

fn write_ready_local_model(model_directory: &std::path::Path, model_id: &str) {
    std::fs::create_dir_all(model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        format!(
            r#"[
          {{
            "id": "{model_id}",
            "display_name": "Ready Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }}
        ]"#
        ),
    )
    .expect("registry json");
    std::fs::write(
        model_directory.join(format!("{model_id}.bin")),
        b"ready model",
    )
    .expect("ready model");
}

#[test]
fn backend_derives_four_product_modes_from_independent_toggles() {
    let mut backend = AppBackend::default();

    assert_eq!(
        backend.app_status().mode,
        AppMode::RecordingAndTranscription
    );
    assert_eq!(backend.app_status().tray_state, TrayState::Idle);
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
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
    for _ in 0..21 {
        backend
            .process_audio_samples_for_test(&[0.0; 1], 100)
            .expect("silence");
    }

    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(snapshot.jobs[0].audio_path.ends_with("19700101/000000.wav"));
}

#[test]
fn backend_manual_import_queues_only_existing_audio_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("meeting.wav");
    let text_path = tmp.path().join("notes.txt");
    let directory_path = tmp.path().join("folder.m4a");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    std::fs::write(&text_path, b"not audio").expect("text");
    std::fs::create_dir(&directory_path).expect("directory");
    let mut backend = AppBackend::default();

    backend.enqueue_audio_file(&text_path, Some("whisper-medium".to_string()));
    backend.enqueue_audio_file(&directory_path, Some("whisper-medium".to_string()));
    let snapshot = backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(snapshot.jobs[0].audio_path, audio_path);
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
fn backend_surfaces_capture_start_failure_as_microphone_warning() {
    let mut backend = AppBackend::default();
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");

    let status = backend.capture_start_failed("No input device is available");

    assert_eq!(status.tray_state, TrayState::Error);
    assert_eq!(
        status.microphone_warning.as_deref(),
        Some("No input device is available")
    );
    assert_eq!(
        backend
            .process_audio_samples_for_test(&[0.8; 1], 100)
            .expect_err("capture should be stopped"),
        "capture session is not running"
    );
}

#[test]
fn backend_reports_idle_after_capture_session_stops() {
    let mut backend = AppBackend::default();
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    let active_status = backend.app_status();
    assert!(active_status.live_input_active);
    assert_eq!(active_status.tray_state, TrayState::Listening);

    let status = backend
        .stop_capture_session()
        .expect("stop capture session");

    assert!(!status.live_input_active);
    assert_eq!(status.mode, AppMode::RecordingAndTranscription);
    assert_eq!(status.tray_state, TrayState::Idle);
}

#[test]
fn backend_resets_level_snapshot_after_capture_session_stops() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(wav_settings_patch(tmp.path()));
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    let live_status = backend
        .process_audio_samples_for_test(&[0.5; 1], 100)
        .expect("speech");
    assert!(live_status.level.current_dbfs > -120.0);

    let stopped_status = backend
        .stop_capture_session()
        .expect("stop capture session");

    assert!(!stopped_status.live_input_active);
    assert_eq!(stopped_status.level.current_dbfs, -120.0);
    assert_eq!(stopped_status.level.peak_dbfs, -120.0);
    assert_eq!(stopped_status.level.noise_floor_dbfs, -120.0);
    assert_eq!(stopped_status.level.suggested_threshold_dbfs, -90.0);
}

#[test]
fn backend_resets_level_snapshot_when_recording_is_disabled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(wav_settings_patch(tmp.path()));
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    let live_status = backend
        .process_audio_samples_for_test(&[0.5; 1], 100)
        .expect("speech");
    assert!(live_status.level.current_dbfs > -120.0);

    backend.update_settings(SettingsPatch {
        recording_enabled: Some(false),
        ..SettingsPatch::default()
    });
    let stopped_status = backend.app_status();

    assert!(!stopped_status.live_input_active);
    assert_eq!(stopped_status.level.current_dbfs, -120.0);
    assert_eq!(stopped_status.level.peak_dbfs, -120.0);
    assert_eq!(stopped_status.level.noise_floor_dbfs, -120.0);
    assert_eq!(stopped_status.level.suggested_threshold_dbfs, -90.0);
}

#[test]
fn backend_ignores_late_audio_frames_after_capture_session_stops() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(wav_settings_patch(tmp.path()));
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    backend
        .process_audio_samples_for_test(&[0.5; 1], 100)
        .expect("speech");
    backend
        .stop_capture_session()
        .expect("stop capture session");

    let error = backend
        .process_audio_samples_for_test(&[0.8; 1], 100)
        .expect_err("late frame should be rejected");
    let status = backend.app_status();

    assert_eq!(error, "capture session is not running");
    assert!(!status.live_input_active);
    assert_eq!(status.level.current_dbfs, -120.0);
    assert_eq!(status.level.peak_dbfs, -120.0);
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
fn runtime_warning_summarizes_dropped_audio_frames_without_overriding_state() {
    let mut backend = AppBackend::default();
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    let status = with_runtime_warning(backend.app_status(), 42);

    assert_eq!(status.tray_state, TrayState::Listening);
    assert_eq!(
        status.runtime_warning.as_deref(),
        Some("Live input dropped 42 stale audio frames while processing was busy")
    );
}

#[test]
fn runtime_warning_surfaces_input_stream_errors_before_dropped_frames() {
    let mut backend = AppBackend::default();
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    let status = with_live_runtime_warning(
        backend.app_status(),
        42,
        Some("default input stream disconnected".to_string()),
    );

    assert_eq!(status.tray_state, TrayState::Error);
    assert_eq!(
        status.runtime_warning.as_deref(),
        Some("Live input stream error: default input stream disconnected")
    );
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
    assert!(!status.live_input_active);
    assert_eq!(backend.queue_snapshot().pending_count, 1);
}

#[test]
fn backend_queues_active_capture_started_before_transcription_is_disabled() {
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
        transcription_enabled: Some(false),
        ..SettingsPatch::default()
    });

    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(snapshot.jobs[0].audio_path.ends_with("19700101/000000.wav"));
}

#[test]
fn backend_processes_next_transcription_job_and_writes_sidecar() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230912.wav");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    write_ready_local_model(&model_directory, "whisper-medium");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".to_string()),
        ..SettingsPatch::default()
    });
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
fn backend_process_next_transcription_skips_unusable_pending_job_models() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "whisper-medium",
            "display_name": "Whisper Medium",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          },
          {
            "id": "missing-local",
            "display_name": "Missing Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    let missing_audio = tmp.path().join("20260506").join("missing.wav");
    let ready_audio = tmp.path().join("20260506").join("ready.wav");
    std::fs::create_dir_all(missing_audio.parent().unwrap()).expect("audio dir");
    std::fs::write(&missing_audio, b"wav bytes").expect("missing audio");
    std::fs::write(&ready_audio, b"wav bytes").expect("ready audio");
    std::fs::write(model_directory.join("whisper-medium.bin"), b"ready model")
        .expect("ready model");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&missing_audio, Some("missing-local".to_string()));
    backend.enqueue_audio_file(&ready_audio, Some("whisper-medium".to_string()));

    let snapshot = backend
        .process_next_transcription_with(StaticTranscriber)
        .expect("process usable transcription");

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.audio_path == missing_audio)
            .expect("missing job")
            .status,
        sagwan::queue::QueueJobStatus::Pending
    );
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.audio_path == ready_audio)
            .expect("ready job")
            .status,
        sagwan::queue::QueueJobStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(ready_audio.with_extension("txt")).expect("transcript"),
        "queued transcript\n"
    );
    assert!(!missing_audio.with_extension("error.txt").exists());
}

#[test]
fn backend_cancel_current_transcription_is_noop_without_running_job() {
    let mut backend = AppBackend::default();

    let snapshot = backend
        .cancel_current_transcription()
        .expect("empty queue cancel should not surface a command error");

    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.pending_count, 0);
    assert_eq!(snapshot.failed_count, 0);
}

#[test]
fn backend_processes_all_pending_transcription_jobs_with_worker_loop() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let first_audio = tmp.path().join("20260506").join("231114.wav");
    let second_audio = tmp.path().join("20260506").join("231115.wav");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(first_audio.parent().unwrap()).expect("audio dir");
    std::fs::write(&first_audio, b"wav bytes").expect("first audio");
    std::fs::write(&second_audio, b"wav bytes").expect("second audio");
    write_ready_local_model(&model_directory, "whisper-medium");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".to_string()),
        ..SettingsPatch::default()
    });
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
fn backend_pending_transcription_marks_worker_errors_as_failed_jobs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("231119.wav");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    std::fs::create_dir(audio_path.with_extension("txt")).expect("block transcript sidecar");
    write_ready_local_model(&model_directory, "whisper-medium");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    let snapshot = backend
        .process_pending_transcriptions_with(StaticTranscriber)
        .expect("worker sidecar errors should be captured as failed jobs");

    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.pending_count, 0);
    assert_eq!(snapshot.failed_count, 1);
    assert!(
        snapshot.jobs[0]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("recorder error")
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
fn backend_default_transcription_worker_waits_when_model_is_missing() {
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

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(snapshot.failed_count, 0);
    assert_eq!(
        snapshot.jobs[0].status,
        sagwan::queue::QueueJobStatus::Pending
    );
    assert_eq!(backend.app_status().tray_state, TrayState::Idle);
    assert!(audio_path.exists());
    assert!(!audio_path.with_extension("error.txt").exists());
}

#[test]
fn backend_manual_transcription_waits_when_model_directory_has_no_usable_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("231014.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some("~/Library/Application Support/Sagwan/models".to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(
        &audio_path,
        Some("missing-model-for-tilde-expansion".to_string()),
    );

    let snapshot = backend
        .process_next_transcription()
        .expect("process transcription");

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(snapshot.failed_count, 0);
    assert_eq!(
        snapshot.jobs[0].status,
        sagwan::queue::QueueJobStatus::Pending
    );
    assert!(!audio_path.with_extension("error.txt").exists());
}

#[test]
fn backend_cancel_model_download_marks_model_as_recoverable_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    let store = ModelStore::new(&model_directory);
    store
        .record_download_progress("whisper-medium", 512, Some(1024))
        .expect("record download progress");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let models = backend
        .cancel_model_download("whisper-medium")
        .expect("cancel model download");

    let model = models
        .iter()
        .find(|model| model.id == "whisper-medium")
        .expect("whisper medium");
    assert_eq!(model.status, ModelStatus::Error);
    assert_eq!(model.download_error.as_deref(), Some("cancelled by user"));
}

#[test]
fn backend_cancel_model_download_rejects_inactive_models() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let error = backend
        .cancel_model_download("whisper-medium")
        .expect_err("inactive model download should not be cancelled");

    assert_eq!(error, "model whisper-medium has no active download");
    let model = backend
        .model_registry()
        .into_iter()
        .find(|model| model.id == "whisper-medium")
        .expect("whisper medium");
    assert_eq!(model.status, ModelStatus::Missing);
    assert_eq!(model.download_error, None);
}

#[test]
fn backend_prepare_model_download_marks_model_downloading_before_fetch() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "custom-local",
            "display_name": "Custom Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": "https://example.invalid/custom-local.bin",
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");

    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let prepared = backend
        .prepare_model_download("custom-local")
        .expect("prepare download");

    assert_eq!(prepared.model.id, "custom-local");
    assert_eq!(prepared.model_directory, model_directory);
    let model = prepared
        .registry
        .iter()
        .find(|model| model.id == "custom-local")
        .expect("custom model");
    assert_eq!(model.status, ModelStatus::Downloading);
    assert_eq!(model.download_progress, Some(0));
    assert!(!model_directory.join("custom-local.bin").exists());
}

#[test]
fn backend_prepare_model_download_rejects_active_downloads() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "custom-local",
            "display_name": "Custom Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": "https://example.invalid/custom-local.bin",
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    ModelStore::new(&model_directory)
        .record_download_progress("custom-local", 512, Some(1024))
        .expect("record active download");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let error = backend
        .prepare_model_download("custom-local")
        .expect_err("active download should not be prepared again");

    assert_eq!(error, "model custom-local download is already active");
}

#[test]
fn backend_prepare_model_download_rejects_installed_models() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "custom-local",
            "display_name": "Custom Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": "https://example.invalid/custom-local.bin",
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    std::fs::write(model_directory.join("custom-local.bin"), b"ready model").expect("model file");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let error = backend
        .prepare_model_download("custom-local")
        .expect_err("installed model should not be prepared for download");

    assert_eq!(error, "model custom-local is already installed");
}

#[test]
fn backend_settings_ignore_unusable_selected_model_patches() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "ready-local",
            "display_name": "Ready Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          },
          {
            "id": "missing-local",
            "display_name": "Missing Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    std::fs::write(model_directory.join("ready-local.bin"), b"ready model").expect("ready model");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("ready-local".to_string()),
        ..SettingsPatch::default()
    });

    backend.update_settings(SettingsPatch {
        selected_model: Some("missing-local".to_string()),
        ..SettingsPatch::default()
    });
    backend.update_settings(SettingsPatch {
        selected_model: Some("unknown-model".to_string()),
        ..SettingsPatch::default()
    });

    assert_eq!(backend.settings().selected_model, "ready-local");
}

#[test]
fn backend_delete_model_rejects_active_selected_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "ready-local",
            "display_name": "Ready Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    let model_path = model_directory.join("ready-local.bin");
    std::fs::write(&model_path, b"ready model").expect("ready model");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("ready-local".to_string()),
        ..SettingsPatch::default()
    });

    let error = backend
        .delete_model("ready-local")
        .expect_err("active model should not be deleted");

    assert_eq!(error, "cannot delete active model ready-local");
    assert!(model_path.exists());
    assert_eq!(backend.settings().selected_model, "ready-local");
}

#[test]
fn backend_auto_transcription_waits_for_usable_selected_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "ready-local",
            "display_name": "Ready Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    let audio_path = tmp.path().join("20260506").join("pending.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("ready-local".to_string()));

    assert!(!backend.should_process_transcriptions());

    std::fs::write(model_directory.join("ready-local.bin"), b"ready model").expect("ready model");
    backend.update_settings(SettingsPatch {
        selected_model: Some("ready-local".to_string()),
        ..SettingsPatch::default()
    });

    assert!(backend.should_process_transcriptions());
}

#[test]
fn backend_auto_transcription_waits_for_pending_job_model_readiness() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "ready-local",
            "display_name": "Ready Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          },
          {
            "id": "missing-local",
            "display_name": "Missing Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    let audio_path = tmp.path().join("20260506").join("pending.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    std::fs::write(model_directory.join("ready-local.bin"), b"ready model").expect("ready model");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("ready-local".to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("missing-local".to_string()));

    assert!(!backend.should_process_transcriptions());
    assert!(backend.start_next_transcription_job().is_none());

    std::fs::write(model_directory.join("missing-local.bin"), b"ready model").expect("ready model");

    assert!(backend.should_process_transcriptions());
    let started = backend
        .start_next_transcription_job()
        .expect("ready job should start");
    assert_eq!(started.job.model_id, "missing-local");
}

#[test]
fn backend_cancel_current_operation_cancels_active_model_download() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    let store = ModelStore::new(&model_directory);
    store
        .record_download_progress("whisper-medium", 512, Some(1024))
        .expect("record download progress");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    backend
        .cancel_current_operation()
        .expect("cancel current operation");

    let model = backend
        .model_registry()
        .into_iter()
        .find(|model| model.id == "whisper-medium")
        .expect("whisper medium");
    assert_eq!(model.status, ModelStatus::Error);
    assert_eq!(model.download_error.as_deref(), Some("cancelled by user"));
}

#[test]
fn tray_presentation_uses_distinct_icon_colors_for_prd_states() {
    let states = [
        TrayState::Idle,
        TrayState::Listening,
        TrayState::Recording,
        TrayState::Transcribing,
        TrayState::Paused,
        TrayState::Error,
    ];
    let colors = states
        .iter()
        .map(|state| tray_presentation_for_state(*state).rgba)
        .collect::<HashSet<_>>();

    assert_eq!(colors.len(), states.len());
    assert_eq!(
        tray_presentation_for_state(TrayState::Listening).rgba,
        [0, 71, 171, 255]
    );
    assert_eq!(
        tray_presentation_for_state(TrayState::Recording).tooltip,
        "Sagwan: Recording"
    );
}

#[test]
fn tray_runtime_presentation_respects_show_tray_icon_setting() {
    let mut backend = AppBackend::default();

    let visible = tray_runtime_presentation(&backend.settings(), &backend.app_status());
    assert!(visible.visible);
    assert_eq!(visible.icon.tooltip, "Sagwan: Idle");

    backend.update_settings(SettingsPatch {
        show_tray_icon: Some(false),
        ..SettingsPatch::default()
    });

    let hidden = tray_runtime_presentation(&backend.settings(), &backend.app_status());
    assert!(!hidden.visible);
    assert_eq!(hidden.icon.tooltip, "Sagwan: Idle");
}

#[test]
fn tray_menu_presentation_reflects_current_settings_and_status() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "whisper-tiny",
            "display_name": "Whisper Tiny",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 75,
            "languages": ["ko", "en", "multi"],
            "speed_score": 9,
            "accuracy_score": 4,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    std::fs::write(model_directory.join("whisper-tiny.bin"), b"ready model").expect("ready model");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-tiny".to_string()),
        selected_microphone_label: Some("USB Mic".to_string()),
        threshold_dbfs: Some(-37.0),
        transcription_enabled: Some(false),
        pause_all: Some(true),
        ..SettingsPatch::default()
    });

    let menu = tray_menu_presentation(&backend.settings(), &backend.app_status());

    assert!(menu.recording_checked);
    assert!(!menu.transcription_checked);
    assert!(menu.pause_all_checked);
    assert_eq!(menu.active_model_text, "Model: whisper-tiny");
    assert_eq!(menu.active_microphone_text, "Microphone: USB Mic");
    assert_eq!(menu.threshold_text, "Threshold: -37 dBFS");
}

#[test]
fn reveal_save_folder_request_uses_current_save_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = std::env::var_os("HOME").expect("HOME should be set for path expansion tests");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let request = reveal_save_folder_request(&backend.settings());

    assert_eq!(request.program, PathBuf::from("/usr/bin/open"));
    assert_eq!(request.path, tmp.path());

    backend.update_settings(SettingsPatch {
        save_root: Some("~/Documents/Sagwan".to_string()),
        ..SettingsPatch::default()
    });
    let request = reveal_save_folder_request(&backend.settings());

    assert_eq!(request.path, PathBuf::from(home).join("Documents/Sagwan"));
}

#[test]
fn main_window_close_hides_settings_to_support_tray_only_mode() {
    assert_eq!(
        main_window_close_action("main"),
        MainWindowCloseAction::HideToTray
    );
    assert_eq!(
        main_window_close_action("secondary"),
        MainWindowCloseAction::AllowClose
    );
}
