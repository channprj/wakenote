use std::{path::PathBuf, time::Duration};

use chrono::TimeZone;
use wakenote::audio::input_devices_from_labels;
use wakenote::commands::{
    AppBackend, AppMode, LiveTranscriptEvent, MainWindowCloseAction, TrayState,
    audio_playback_content_type, main_window_close_action, microphone_devices_from_input_devices,
    open_containing_folder_request, reveal_save_folder_request, tray_icon_image_for_presentation,
    tray_menu_presentation, tray_presentation_for_state, tray_runtime_presentation,
    with_live_runtime_warning, with_runtime_warning,
};
use wakenote::live_capture::AudioFrame;
use wakenote::models::{ModelStatus, ModelStore};
use wakenote::queue::QueueJobStatus;
use wakenote::recorder::{ChunkMetadata, ChunkSource, TranscriptionStatus};
use wakenote::settings::{
    AudioFormat, FloatingOverlayPosition, SettingsPatch, TranscriptionLanguage,
};
use wakenote::transcription::{
    Transcriber, TranscriptionError, TranscriptionJobOutcome, TranscriptionRequest,
};

fn epoch_local_path_parts() -> (String, String) {
    let local = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH.with_timezone(&chrono::Local);
    (
        local.format("%Y%m%d").to_string(),
        local.format("%H%M%S").to_string(),
    )
}

#[derive(Clone)]
struct StaticTranscriber {
    expected_language: TranscriptionLanguage,
}

impl Default for StaticTranscriber {
    fn default() -> Self {
        Self {
            expected_language: TranscriptionLanguage::Ko,
        }
    }
}

impl StaticTranscriber {
    fn expecting_language(expected_language: TranscriptionLanguage) -> Self {
        Self { expected_language }
    }
}

impl Transcriber for StaticTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        assert_eq!(request.model_id, "whisper-medium");
        assert_eq!(request.language, self.expected_language);
        assert!(request.audio_path.exists());
        Ok("queued transcript".to_string())
    }
}

#[test]
fn audio_playback_accepts_recording_formats_only() {
    assert_eq!(
        audio_playback_content_type(&PathBuf::from("/tmp/recording.m4a")),
        Some("audio/mp4")
    );
    assert_eq!(
        audio_playback_content_type(&PathBuf::from("/tmp/recording.wav")),
        Some("audio/wav")
    );
    assert_eq!(
        audio_playback_content_type(&PathBuf::from("/tmp/recording.mp3")),
        Some("audio/mpeg")
    );
    assert_eq!(
        audio_playback_content_type(&PathBuf::from("/tmp/transcript.txt")),
        None
    );
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

fn write_ready_local_models(model_directory: &std::path::Path, model_ids: &[&str]) {
    std::fs::create_dir_all(model_directory).expect("model dir");
    let registry = model_ids
        .iter()
        .map(|model_id| {
            format!(
                r#"{{
            "id": "{model_id}",
            "display_name": "Ready Local {model_id}",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        model_directory.join("model-registry.json"),
        format!("[{registry}]"),
    )
    .expect("registry json");
    for model_id in model_ids {
        std::fs::write(
            model_directory.join(format!("{model_id}.bin")),
            b"ready model",
        )
        .expect("ready model");
    }
}

fn write_transcript_sidecar(save_root: &std::path::Path, relative_path: &str, text: &str) {
    let path = save_root.join(relative_path);
    std::fs::create_dir_all(path.parent().expect("sidecar parent")).expect("sidecar dir");
    std::fs::write(path, text).expect("sidecar text");
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
fn backend_reads_three_newest_transcript_sidecars_from_save_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    write_transcript_sidecar(tmp.path(), "20260509/090000.txt", "first transcript\n");
    write_transcript_sidecar(tmp.path(), "20260509/090100.txt", "second transcript\n");
    write_transcript_sidecar(tmp.path(), "20260509/090200.txt", "third transcript\n");
    write_transcript_sidecar(tmp.path(), "20260509/090300.txt", "fourth transcript\n");
    write_transcript_sidecar(tmp.path(), "20260509/090400.error.txt", "error text\n");

    let transcripts = backend.recent_transcripts(3);

    assert_eq!(
        transcripts
            .iter()
            .map(|transcript| transcript.text.as_str())
            .collect::<Vec<_>>(),
        vec!["fourth transcript", "third transcript", "second transcript"]
    );
}

#[test]
fn backend_transcript_sidecars_include_recording_metadata_for_ui_links() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    write_transcript_sidecar(tmp.path(), "20260510/010203.txt", "linked transcript\n");
    let audio_path = tmp.path().join("20260510/010203.m4a");
    std::fs::write(&audio_path, b"audio").expect("audio file");

    let transcripts = backend.recent_transcripts(1);

    assert_eq!(transcripts[0].text, "linked transcript");
    let audio_path_text = audio_path.to_string_lossy().to_string();
    assert_eq!(
        transcripts[0].audio_path.as_deref(),
        Some(audio_path_text.as_str())
    );
    assert!(
        transcripts[0]
            .recorded_at
            .starts_with("2026-05-10T01:02:03")
    );
}

#[test]
fn backend_transcript_sidecars_include_audio_source_for_ui_badges() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    write_transcript_sidecar(
        tmp.path(),
        "20260510/010203-youtube.txt",
        "system transcript\n",
    );
    let metadata = ChunkMetadata {
        model_id: "whisper-medium".into(),
        device_id: "youtube".into(),
        device_name: "Google Chrome".into(),
        sample_rate: 16_000,
        threshold_dbfs: -42.0,
        attack_ms: 100,
        release_ms: 1_000,
        pre_roll_ms: 1_000,
        lead_in_padding_ms: 300,
        post_roll_ms: 300,
        min_chunk_ms: 600,
        max_chunk_ms: 120_000,
        started_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 1, 2, 3).unwrap(),
        ended_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 1, 2, 4).unwrap(),
        duration_ms: 1_000,
        transcription_status: TranscriptionStatus::Completed,
        app_version: "0.1.1".into(),
        used_fallback_device: false,
        live_capture_chunk_id: None,
        source: ChunkSource::System,
        source_label: Some("youtube".into()),
    };
    let metadata_path = tmp.path().join("20260510/010203-youtube.json");
    std::fs::write(
        metadata_path,
        serde_json::to_vec(&metadata).expect("metadata json"),
    )
    .expect("metadata");

    let transcripts = backend.recent_transcripts(1);

    assert_eq!(transcripts[0].source, ChunkSource::System);
    assert_eq!(transcripts[0].source_label.as_deref(), Some("youtube"));
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
    let devices =
        microphone_devices_from_input_devices("default", input_devices_from_labels(true, ["BY-V"]));

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

    let (dir, stem) = epoch_local_path_parts();
    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(
        snapshot.jobs[0]
            .audio_path
            .ends_with(format!("{dir}/{stem}.wav"))
    );
}

#[test]
fn backend_enqueues_completed_youtube_system_capture_chunks() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        transcription_enabled: Some(true),
        threshold_dbfs: Some(-45.0),
        attack_ms: Some(100),
        release_ms: Some(250),
        pre_roll_ms: Some(0),
        post_roll_ms: Some(0),
        min_chunk_ms: Some(100),
        ..SettingsPatch::default()
    });
    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    backend
        .start_system_capture_session(
            10,
            base_time,
            "Google Chrome".into(),
            "youtube".into(),
            "youtube".into(),
        )
        .expect("start system capture");

    for end_ms in [100, 200] {
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.8],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("youtube audio frame");
    }
    for end_ms in [300, 400, 500] {
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.0],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("youtube silence frame");
    }

    let expected_local = base_time.with_timezone(&chrono::Local);
    let expected_dir = expected_local.format("%Y%m%d").to_string();
    let expected_stem = expected_local.format("%H%M%S").to_string();
    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(
        snapshot.jobs[0]
            .audio_path
            .ends_with(format!("{expected_dir}/{expected_stem}-youtube.wav"))
    );

    let metadata_path = snapshot.jobs[0].audio_path.with_extension("json");
    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(metadata_path).expect("metadata"))
            .expect("metadata json");
    assert_eq!(metadata.source, ChunkSource::System);
    assert_eq!(metadata.source_label.as_deref(), Some("youtube"));
    assert_eq!(metadata.transcription_status, TranscriptionStatus::Queued);
}

#[test]
fn backend_records_microphone_and_youtube_as_separate_parallel_inputs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        transcription_enabled: Some(true),
        threshold_dbfs: Some(-45.0),
        attack_ms: Some(100),
        release_ms: Some(250),
        pre_roll_ms: Some(0),
        post_roll_ms: Some(0),
        min_chunk_ms: Some(100),
        ..SettingsPatch::default()
    });
    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    backend
        .start_capture_session_with_device(10, base_time, "default", "System Default", false)
        .expect("start mic capture");
    backend
        .start_system_capture_session(
            10,
            base_time,
            "Google Chrome".into(),
            "youtube".into(),
            "youtube".into(),
        )
        .expect("start youtube capture");

    for end_ms in [100, 200] {
        backend
            .process_audio_frame(AudioFrame {
                samples: vec![0.8],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("mic frame");
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.7],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("youtube frame");
    }
    for end_ms in [300, 400, 500] {
        backend
            .process_audio_frame(AudioFrame {
                samples: vec![0.0],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("mic silence");
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.0],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("youtube silence");
    }

    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 2);
    let expected_local = base_time.with_timezone(&chrono::Local);
    let expected_stem = expected_local.format("%H%M%S").to_string();
    let mut paths = snapshot
        .jobs
        .iter()
        .map(|job| job.audio_path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with(format!("{expected_stem}.wav")))
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with(format!("{expected_stem}-youtube.wav")))
    );

    let mic_metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(paths[0].with_extension("json")).expect("metadata"))
            .expect("metadata json");
    let youtube_metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(paths[1].with_extension("json")).expect("metadata"))
            .expect("metadata json");
    let sources = [mic_metadata.source, youtube_metadata.source];
    assert!(sources.contains(&ChunkSource::Microphone));
    assert!(sources.contains(&ChunkSource::System));
}

#[test]
fn backend_uses_frame_capture_time_for_recording_filename() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        transcription_enabled: Some(true),
        threshold_dbfs: Some(-45.0),
        attack_ms: Some(100),
        release_ms: Some(250),
        pre_roll_ms: Some(0),
        post_roll_ms: Some(0),
        min_chunk_ms: Some(100),
        ..SettingsPatch::default()
    });
    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    backend
        .start_capture_session_with_device(10, base_time, "default", "System Default", false)
        .expect("start capture session");

    for end_ms in [60_100, 60_200] {
        backend
            .process_audio_frame(AudioFrame {
                samples: vec![0.8],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("speech frame");
    }
    for end_ms in [60_300, 60_400, 60_500] {
        backend
            .process_audio_frame(AudioFrame {
                samples: vec![0.0],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("silence frame");
    }

    let expected_started_at = base_time + chrono::Duration::seconds(60);
    let expected_local = expected_started_at.with_timezone(&chrono::Local);
    let expected_dir = expected_local.format("%Y%m%d").to_string();
    let expected_stem = expected_local.format("%H%M%S").to_string();
    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(
        snapshot.jobs[0]
            .audio_path
            .ends_with(format!("{expected_dir}/{expected_stem}.wav"))
    );

    let metadata_path = snapshot.jobs[0].audio_path.with_extension("json");
    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(metadata_path).expect("metadata"))
            .expect("metadata json");
    assert_eq!(metadata.started_at, expected_started_at);
}

#[test]
fn backend_live_events_include_overlay_context_without_runtime_backend_lookup() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    write_ready_local_model(&model_directory, "whisper-medium");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        floating_overlay_position: Some(FloatingOverlayPosition::Bottom),
        transcription_enabled: Some(true),
        ..SettingsPatch::default()
    });

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

    let events = backend.drain_live_events();
    let started_overlay_position = events.iter().find_map(|event| match event {
        LiveTranscriptEvent::Started {
            overlay_position, ..
        } => Some(*overlay_position),
        _ => None,
    });
    let committed_overlay_context = events.iter().find_map(|event| match event {
        LiveTranscriptEvent::Committed {
            overlay_position,
            will_transcribe,
            ..
        } => Some((*overlay_position, *will_transcribe)),
        _ => None,
    });

    assert_eq!(
        started_overlay_position,
        Some(FloatingOverlayPosition::Bottom)
    );
    assert_eq!(
        committed_overlay_context,
        Some((FloatingOverlayPosition::Bottom, true))
    );
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
fn backend_regenerate_transcript_requeues_completed_audio_and_clears_sidecars() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_dir = tmp.path().join("models");
    write_ready_local_model(&model_dir, "whisper-medium");
    let audio_path = tmp.path().join("20260611").join("024304-spotify.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio parent")).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let now = chrono::Utc::now();
    let metadata = ChunkMetadata {
        model_id: "whisper-medium".into(),
        device_id: "custom-source-2".into(),
        device_name: "Spotify".into(),
        sample_rate: 16_000,
        threshold_dbfs: -42.0,
        attack_ms: 100,
        release_ms: 1_000,
        pre_roll_ms: 1_000,
        lead_in_padding_ms: 300,
        post_roll_ms: 300,
        min_chunk_ms: 600,
        max_chunk_ms: 120_000,
        started_at: now,
        ended_at: now,
        duration_ms: 1000,
        transcription_status: TranscriptionStatus::Queued,
        app_version: "0.0.0".into(),
        used_fallback_device: false,
        live_capture_chunk_id: None,
        source: ChunkSource::System,
        source_label: Some("Spotify".into()),
    };
    std::fs::write(
        audio_path.with_extension("json"),
        serde_json::to_vec_pretty(&metadata).expect("metadata json"),
    )
    .expect("metadata");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_dir.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".into()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));
    backend
        .process_next_transcription_with(StaticTranscriber::default())
        .expect("complete imported job");
    std::fs::write(audio_path.with_extension("txt"), b"old transcript\n").expect("old transcript");
    std::fs::write(audio_path.with_extension("error.txt"), b"old error\n").expect("old error");

    let snapshot = backend
        .regenerate_transcript(&audio_path, None)
        .expect("regenerate transcript");

    assert_eq!(snapshot.pending_count, 1);
    let job = snapshot
        .jobs
        .iter()
        .find(|job| job.audio_path == audio_path)
        .expect("requeued job");
    assert_eq!(job.status, QueueJobStatus::Pending);
    assert_eq!(job.model_id, "whisper-medium");
    assert!(!audio_path.with_extension("txt").exists());
    assert!(!audio_path.with_extension("error.txt").exists());
    let updated: ChunkMetadata = serde_json::from_slice(
        &std::fs::read(audio_path.with_extension("json")).expect("metadata"),
    )
    .expect("metadata json");
    assert_eq!(updated.transcription_status, TranscriptionStatus::Queued);
}

#[test]
fn backend_regenerate_transcript_accepts_an_explicit_ready_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_dir = tmp.path().join("models");
    write_ready_local_models(&model_dir, &["whisper-medium", "whisper-small"]);
    let audio_path = tmp.path().join("20260611").join("024305-youtube.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio parent")).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_dir.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".into()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    let snapshot = backend
        .regenerate_transcript(&audio_path, Some("whisper-small".to_string()))
        .expect("regenerate transcript with tiny");

    let job = snapshot
        .jobs
        .iter()
        .find(|job| job.audio_path == audio_path)
        .expect("requeued job");
    assert_eq!(job.status, QueueJobStatus::Pending);
    assert_eq!(job.model_id, "whisper-small");
}

#[test]
fn backend_system_capture_emits_live_transcript_events() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    write_ready_local_model(&model_directory, "whisper-medium");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        floating_overlay_position: Some(FloatingOverlayPosition::Bottom),
        transcription_enabled: Some(true),
        threshold_dbfs: Some(-45.0),
        attack_ms: Some(100),
        release_ms: Some(250),
        pre_roll_ms: Some(0),
        post_roll_ms: Some(0),
        min_chunk_ms: Some(100),
        ..SettingsPatch::default()
    });
    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    backend
        .start_system_capture_session(
            10,
            base_time,
            "Google Chrome".into(),
            "youtube".into(),
            "youtube".into(),
        )
        .expect("start system capture");

    for end_ms in [100, 200, 300, 400, 500, 600, 700, 800, 900, 1_000, 1_100] {
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.8],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("youtube audio frame");
    }
    for end_ms in [1_200, 1_300, 1_400] {
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.0],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(end_ms),
            })
            .expect("youtube silence frame");
    }

    let events = backend.drain_live_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LiveTranscriptEvent::Started { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, LiveTranscriptEvent::SamplesReady { .. }))
    );
    assert!(events.iter().any(|event| matches!(
        event,
        LiveTranscriptEvent::Committed {
            overlay_position: FloatingOverlayPosition::Bottom,
            will_transcribe: true,
            ..
        }
    )));
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

    let (dir, stem) = epoch_local_path_parts();
    let metadata_path = tmp.path().join(&dir).join(format!("{stem}.json"));
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
    // The failure transitions the watchdog into a pending-recovery state so
    // it keeps re-attempting starts instead of going idle.
    assert!(
        backend.mic_recovery_pending(),
        "capture_start_failed should leave recovery pending"
    );
}

#[test]
fn backend_clears_mic_recovery_pending_after_successful_start() {
    let mut backend = AppBackend::default();
    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    let _ = backend.capture_start_failed("Microphone unavailable: device gone");
    assert!(backend.mic_recovery_pending());

    backend
        .start_capture_session_for_test(10)
        .expect("recovery start succeeds");

    assert!(
        !backend.mic_recovery_pending(),
        "successful start should clear recovery pending",
    );
}

#[test]
fn backend_clears_active_microphone_warning_after_healthy_audio_on_selected_device() {
    let mut backend = AppBackend::default();
    backend
        .start_capture_session_with_device(
            10,
            chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            "default",
            "System Default",
            false,
        )
        .expect("start selected microphone capture");
    backend.set_microphone_warning(
        "Microphone has not produced audio after several recovery attempts. Try Refresh.",
    );
    assert_eq!(
        tray_runtime_presentation(&backend.settings(), &backend.app_status())
            .icon
            .rgba,
        [220, 38, 38, 255],
        "standing microphone warning should force the disconnected tray color",
    );

    backend
        .process_audio_frame(AudioFrame {
            samples: vec![0.8; 10],
            duration_ms: 100,
            captured_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
        })
        .expect("healthy audio frame");

    let status = backend.app_status();
    assert!(
        status.microphone_warning.is_none(),
        "healthy audio on the selected microphone should clear the recoverable warning",
    );
    assert_ne!(
        tray_runtime_presentation(&backend.settings(), &status)
            .icon
            .rgba,
        [220, 38, 38, 255],
        "cleared warning should release the disconnected tray color",
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

    let (dir, stem) = epoch_local_path_parts();
    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(
        snapshot.jobs[0]
            .audio_path
            .ends_with(format!("{dir}/{stem}.wav"))
    );
}

#[test]
fn backend_restarting_capture_session_flushes_active_chunk() {
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

    backend
        .start_capture_session_with_device(
            10,
            chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            "input-1-usb",
            "USB Mic",
            false,
        )
        .expect("restart capture session");

    let (dir, stem) = epoch_local_path_parts();
    let snapshot = backend.queue_snapshot();
    assert_eq!(snapshot.pending_count, 1);
    assert!(
        snapshot.jobs[0]
            .audio_path
            .ends_with(format!("{dir}/{stem}.wav"))
    );
    let status = backend.app_status();
    assert!(status.live_input_active);
    assert_eq!(status.active_microphone, "USB Mic");
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
        .process_next_transcription_with(StaticTranscriber::default())
        .expect("process transcription");

    assert_eq!(snapshot.pending_count, 0);
    assert_eq!(
        snapshot.jobs[0].status,
        wakenote::queue::QueueJobStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("transcript"),
        "queued transcript\n"
    );
}

#[test]
fn backend_starts_one_transcription_job_at_a_time() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    write_ready_local_model(&model_directory, "whisper-medium");
    let first_audio = tmp.path().join("20260506").join("230912.wav");
    let second_audio = tmp.path().join("20260506").join("230913.wav");
    let third_audio = tmp.path().join("20260506").join("230914.wav");
    std::fs::create_dir_all(first_audio.parent().unwrap()).expect("audio dir");
    std::fs::write(&first_audio, b"first").expect("first audio");
    std::fs::write(&second_audio, b"second").expect("second audio");
    std::fs::write(&third_audio, b"third").expect("third audio");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".to_string()),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&first_audio, Some("whisper-medium".to_string()));
    backend.enqueue_audio_file(&second_audio, Some("whisper-medium".to_string()));
    backend.enqueue_audio_file(&third_audio, Some("whisper-medium".to_string()));

    let started = backend.start_transcription_jobs_up_to(2);
    let snapshot = backend.queue_snapshot();

    assert_eq!(started.len(), 1);
    assert_eq!(started[0].job.audio_path, first_audio);
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.pending_count, 2);
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.audio_path == second_audio)
            .expect("second job")
            .status,
        wakenote::queue::QueueJobStatus::Pending
    );
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.audio_path == third_audio)
            .expect("third job")
            .status,
        wakenote::queue::QueueJobStatus::Pending
    );
}

#[test]
fn backend_does_not_persist_queue_for_capture_frames_without_queue_changes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230912.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"audio").expect("audio file");
    let mut backend = AppBackend::load_from_dir(tmp.path()).expect("load backend");
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));
    backend
        .start_capture_session_for_test(16_000)
        .expect("start capture");
    let queue_path = tmp.path().join("transcription-queue.json");
    let before = std::fs::metadata(&queue_path)
        .expect("queue before")
        .modified()
        .expect("mtime before");
    std::thread::sleep(Duration::from_millis(20));

    backend
        .process_audio_samples_for_test(&[0.0; 160], 10)
        .expect("silent frame");

    let after = std::fs::metadata(&queue_path)
        .expect("queue after")
        .modified()
        .expect("mtime after");
    assert_eq!(after, before);
}

#[test]
fn backend_passes_configured_transcription_language_to_worker() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230913.wav");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    write_ready_local_model(&model_directory, "whisper-medium");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-medium".to_string()),
        transcription_language: Some(TranscriptionLanguage::Ko),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));

    backend
        .process_next_transcription_with(StaticTranscriber::expecting_language(
            TranscriptionLanguage::Ko,
        ))
        .expect("process transcription");
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
        .process_next_transcription_with(StaticTranscriber::default())
        .expect("process usable transcription");

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.audio_path == missing_audio)
            .expect("missing job")
            .status,
        wakenote::queue::QueueJobStatus::Pending
    );
    assert_eq!(
        snapshot
            .jobs
            .iter()
            .find(|job| job.audio_path == ready_audio)
            .expect("ready job")
            .status,
        wakenote::queue::QueueJobStatus::Completed
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
        .process_pending_transcriptions_with(StaticTranscriber::default())
        .expect("process pending transcriptions");

    assert_eq!(snapshot.pending_count, 0);
    assert_eq!(snapshot.failed_count, 0);
    assert!(
        snapshot
            .jobs
            .iter()
            .all(|job| job.status == wakenote::queue::QueueJobStatus::Completed)
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
        .process_pending_transcriptions_with(StaticTranscriber::default())
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
        .process_pending_transcriptions_with(StaticTranscriber::default())
        .expect("process pending transcriptions");

    assert_eq!(snapshot.pending_count, 1);
    assert_eq!(
        snapshot.jobs[0].status,
        wakenote::queue::QueueJobStatus::Pending
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
        wakenote::queue::QueueJobStatus::Pending
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
        model_directory: Some("~/Library/Application Support/WakeNote/models".to_string()),
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
        wakenote::queue::QueueJobStatus::Pending
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
fn backend_download_model_rejects_active_downloads_before_fetching() {
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
            "download_url": "http://127.0.0.1:9/custom-local.bin",
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
        .download_model("custom-local")
        .expect_err("active download should not fetch again");

    assert_eq!(error, "model custom-local download is already active");
    let model = backend
        .model_registry()
        .into_iter()
        .find(|model| model.id == "custom-local")
        .expect("custom model");
    assert_eq!(model.status, ModelStatus::Downloading);
    assert_eq!(model.download_progress, Some(50));
    assert_eq!(model.download_error, None);
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
fn backend_delete_model_rejects_active_download_models() {
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
    let temp_path = model_directory.join("custom-local.bin.part");
    std::fs::write(&temp_path, b"partial download").expect("partial model");
    ModelStore::new(&model_directory)
        .record_download_progress("custom-local", 512, Some(1024))
        .expect("record active download");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        ..SettingsPatch::default()
    });

    let error = backend
        .delete_model("custom-local")
        .expect_err("active download should not be deleted");

    assert_eq!(error, "model custom-local download is active");
    assert!(temp_path.exists());
    let model = backend
        .model_registry()
        .into_iter()
        .find(|model| model.id == "custom-local")
        .expect("custom model");
    assert_eq!(model.status, ModelStatus::Downloading);
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
fn tray_presentation_uses_voice_capture_colors_for_active_states() {
    assert_eq!(
        tray_presentation_for_state(TrayState::Idle).rgba,
        [0, 0, 0, 255]
    );
    assert_eq!(
        tray_presentation_for_state(TrayState::Listening).rgba,
        [0, 0, 0, 255]
    );
    assert_eq!(
        tray_presentation_for_state(TrayState::Recording).rgba,
        [22, 163, 74, 255]
    );
    assert_eq!(
        tray_presentation_for_state(TrayState::Transcribing).rgba,
        [217, 119, 6, 255]
    );
    assert_eq!(
        tray_presentation_for_state(TrayState::Recording).tooltip,
        "WakeNote: Recording"
    );
}

#[test]
fn tray_runtime_presentation_uses_red_for_runtime_failures() {
    let mut backend = AppBackend::default();
    let normal = tray_runtime_presentation(&backend.settings(), &backend.app_status());
    assert_eq!(normal.icon.rgba, [0, 0, 0, 255]);

    backend.capture_start_failed("Microphone unavailable: BY-V disconnected");
    let mic_failure = tray_runtime_presentation(&backend.settings(), &backend.app_status());
    assert_eq!(mic_failure.icon.rgba, [220, 38, 38, 255]);

    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("missing.wav");
    std::fs::write(&audio_path, b"audio").expect("audio file");
    let mut queue_failure = AppBackend::load_from_dir(tmp.path()).expect("backend");
    let snapshot =
        queue_failure.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));
    let job_id = snapshot.jobs.last().expect("queued job").id;
    queue_failure
        .finish_transcription_job(TranscriptionJobOutcome::failed(
            job_id,
            "transcription failed",
        ))
        .expect("fail queue job");
    let queue_failure_presentation =
        tray_runtime_presentation(&queue_failure.settings(), &queue_failure.app_status());
    assert_eq!(queue_failure_presentation.icon.rgba, [220, 38, 38, 255]);
}

#[test]
fn tray_state_prioritizes_errors_then_transcribing_then_active_recording() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_dir = tmp.path().join("models");
    write_ready_local_model(&model_dir, "whisper-medium");
    let audio_path = tmp.path().join("recording.wav");
    std::fs::write(&audio_path, b"audio").expect("audio");
    let mut backend = AppBackend::load_from_dir(tmp.path()).expect("backend");
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_dir.to_string_lossy().to_string()),
        audio_format: Some(AudioFormat::Wav),
        transcription_enabled: Some(true),
        threshold_dbfs: Some(-45.0),
        attack_ms: Some(100),
        release_ms: Some(250),
        pre_roll_ms: Some(0),
        post_roll_ms: Some(0),
        min_chunk_ms: Some(100),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, Some("whisper-medium".to_string()));
    let started = backend.start_next_transcription_job();
    assert!(started.is_some());
    assert_eq!(backend.app_status().tray_state, TrayState::Transcribing);

    backend
        .start_capture_session_for_test(10)
        .expect("start capture session");
    for _ in 0..2 {
        backend
            .process_audio_samples_for_test(&[0.8], 100)
            .expect("speech");
    }
    assert_eq!(
        backend.app_status().tray_state,
        TrayState::Transcribing,
        "transcribing should outrank active recording"
    );

    backend.capture_start_failed("Microphone unavailable");
    assert_eq!(
        backend.app_status().tray_state,
        TrayState::Error,
        "fatal runtime errors should outrank transcribing and recording"
    );
}

#[test]
fn system_capture_session_without_audio_does_not_report_recording_tray_state() {
    let mut backend = AppBackend::default();
    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();

    let status = backend
        .start_system_capture_session(
            16_000,
            base_time,
            "Google Chrome".into(),
            "youtube".into(),
            "youtube".into(),
        )
        .expect("start system capture session");

    assert_eq!(status.tray_state, TrayState::Idle);
    assert_eq!(backend.app_status().tray_state, TrayState::Idle);
}

#[test]
fn tray_icon_image_draws_centered_half_size_circle() {
    let presentation = tray_presentation_for_state(TrayState::Listening);
    let image = tray_icon_image_for_presentation(presentation);

    assert_eq!(image.width, 64);
    assert_eq!(image.height, 64);
    assert_eq!(image.rgba.len(), (64 * 64 * 4) as usize);

    let center = ((32 * image.width + 32) * 4) as usize;
    assert_eq!(&image.rgba[center..center + 4], &[0, 0, 0, 255]);
    let corner = 0;
    assert_eq!(&image.rgba[corner..corner + 4], &[0, 0, 0, 0]);

    let filled_pixels = image
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(
        (760..=860).contains(&filled_pixels),
        "32px diameter circle should fill roughly half-size area, got {filled_pixels}"
    );
}

#[test]
fn tray_runtime_presentation_respects_show_tray_icon_setting() {
    let mut backend = AppBackend::default();

    let visible = tray_runtime_presentation(&backend.settings(), &backend.app_status());
    assert!(visible.visible);
    assert_eq!(visible.icon.tooltip, "WakeNote: Idle");

    backend.update_settings(SettingsPatch {
        show_tray_icon: Some(false),
        ..SettingsPatch::default()
    });

    let hidden = tray_runtime_presentation(&backend.settings(), &backend.app_status());
    assert!(!hidden.visible);
    assert_eq!(hidden.icon.tooltip, "WakeNote: Idle");
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
            "id": "whisper-small",
            "display_name": "Whisper Small",
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
    std::fs::write(model_directory.join("whisper-small.bin"), b"ready model").expect("ready model");
    let mut backend = AppBackend::default();
    backend.update_settings(SettingsPatch {
        model_directory: Some(model_directory.to_string_lossy().to_string()),
        selected_model: Some("whisper-small".to_string()),
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
    assert_eq!(menu.active_model_text, "Model: whisper-small");
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
        save_root: Some("~/Documents/WakeNote".to_string()),
        ..SettingsPatch::default()
    });
    let request = reveal_save_folder_request(&backend.settings());

    assert_eq!(request.path, PathBuf::from(home).join("Documents/WakeNote"));
}

#[test]
fn open_containing_folder_request_uses_the_transcript_parent_folder() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let day_dir = tmp.path().join("20260611");
    std::fs::create_dir_all(&day_dir).expect("day dir");
    let transcript_path = day_dir.join("024304-youtube.txt");
    std::fs::write(&transcript_path, b"transcript").expect("transcript");

    let request = open_containing_folder_request(&transcript_path).expect("folder request");

    assert_eq!(request.program, PathBuf::from("/usr/bin/open"));
    assert_eq!(request.path, day_dir);
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

#[test]
fn system_capture_session_lifecycle_reports_active_state() {
    let mut backend = AppBackend::default();
    assert!(!backend.is_system_capturing());

    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    backend
        .start_system_capture_session(
            16_000,
            base_time,
            "Google Chrome".into(),
            "meet".into(),
            "meet".into(),
        )
        .expect("start system capture session");
    assert!(backend.is_system_capturing());

    backend
        .stop_system_capture_session()
        .expect("stop system capture session");
    assert!(!backend.is_system_capturing());
}

#[test]
fn system_capture_session_reports_recording_tray_state() {
    let mut backend = AppBackend::default();
    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();

    backend.update_settings(SettingsPatch {
        threshold_dbfs: Some(-45.0),
        attack_ms: Some(100),
        pre_roll_ms: Some(0),
        ..SettingsPatch::default()
    });
    backend
        .start_system_capture_session(
            16_000,
            base_time,
            "Google Chrome".into(),
            "youtube".into(),
            "youtube".into(),
        )
        .expect("start system capture session");
    let status = backend
        .process_system_audio_frame(AudioFrame {
            samples: vec![0.8; 1_600],
            duration_ms: 100,
            captured_at: base_time + chrono::Duration::milliseconds(100),
        })
        .expect("system audio frame");

    assert_eq!(status.tray_state, TrayState::Recording);
    assert_eq!(backend.app_status().tray_state, TrayState::Recording);
}

#[test]
fn sync_system_capture_settings_keeps_session_and_is_noop_without_one() {
    let mut backend = AppBackend::default();
    // No active session: syncing must not crash or open one.
    backend.sync_system_capture_settings();
    assert!(!backend.is_system_capturing());

    let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    backend
        .start_system_capture_session(
            16_000,
            base_time,
            "Google Chrome".into(),
            "meet".into(),
            "meet".into(),
        )
        .expect("start system capture session");

    // A settings change must propagate to the live controller, not tear down
    // the session (teardown is the runtime/stream-handle owner's job).
    backend.update_settings(SettingsPatch {
        threshold_dbfs: Some(-40.0),
        ..SettingsPatch::default()
    });
    assert!(backend.is_system_capturing());
}
