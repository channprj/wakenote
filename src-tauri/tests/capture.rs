use chrono::{TimeZone, Utc};
use sagwan::capture::{
    CaptureController, CaptureControllerConfig, CaptureControllerEvent, CaptureProcessor,
    CaptureProcessorConfig,
};
use sagwan::settings::{AppSettings, AudioFormat};

fn settings() -> AppSettings {
    AppSettings {
        audio_format: AudioFormat::Wav,
        transcription_enabled: false,
        threshold_dbfs: -45.0,
        attack_ms: 300,
        release_ms: 1_500,
        pre_roll_ms: 200,
        post_roll_ms: 200,
        min_chunk_ms: 500,
        max_chunk_ms: 30_000,
        ..AppSettings::default()
    }
}

#[test]
fn capture_processor_rejects_spikes_shorter_than_attack() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut processor = CaptureProcessor::new(CaptureProcessorConfig {
        save_root: tmp.path().to_path_buf(),
        settings: settings(),
        sample_rate: 10,
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device: false,
        base_time: Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap(),
        app_version: "0.1.0".to_string(),
    });

    processor.process_samples(&[0.0; 1], 100).expect("silence");
    processor.process_samples(&[0.9; 1], 100).expect("spike");
    processor.process_samples(&[0.0; 1], 100).expect("silence");
    processor.flush().expect("flush");

    assert!(processor.completed_chunks().is_empty());
}

#[test]
fn capture_processor_writes_chunk_after_attack_and_release() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut processor = CaptureProcessor::new(CaptureProcessorConfig {
        save_root: tmp.path().to_path_buf(),
        settings: settings(),
        sample_rate: 10,
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device: false,
        base_time: Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap(),
        app_version: "0.1.0".to_string(),
    });

    for _ in 0..5 {
        processor.process_samples(&[0.8; 1], 100).expect("speech");
    }
    for _ in 0..18 {
        processor.process_samples(&[0.0; 1], 100).expect("silence");
    }

    let chunks = processor.completed_chunks();
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].audio_path.ends_with("20260506/230709.wav"));
    assert!(chunks[0].metadata_path.exists());
    assert!(chunks[0].audio_path.exists());
}

#[test]
fn capture_processor_rolls_over_at_max_chunk_without_dropping_stream() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut config_settings = settings();
    config_settings.attack_ms = 100;
    config_settings.max_chunk_ms = 500;
    let mut processor = CaptureProcessor::new(CaptureProcessorConfig {
        save_root: tmp.path().to_path_buf(),
        settings: config_settings,
        sample_rate: 10,
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device: false,
        base_time: Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap(),
        app_version: "0.1.0".to_string(),
    });

    for _ in 0..10 {
        processor.process_samples(&[0.8; 1], 100).expect("speech");
    }

    let chunks = processor.completed_chunks();
    assert_eq!(chunks.len(), 2);
    assert!(processor.is_recording());
}

#[test]
fn capture_controller_flushes_active_chunk_when_recording_is_disabled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut controller = CaptureController::new(CaptureControllerConfig {
        save_root: tmp.path().to_path_buf(),
        settings: settings(),
        sample_rate: 10,
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device: false,
        base_time: Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap(),
        app_version: "0.1.0".to_string(),
    });

    for _ in 0..5 {
        controller.process_samples(&[0.8; 1], 100).expect("speech");
    }
    assert!(controller.is_recording());

    let mut disabled = settings();
    disabled.recording_enabled = false;
    let events = controller.update_settings(disabled).expect("disable");

    assert!(!controller.is_listening());
    assert!(!controller.is_recording());
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        CaptureControllerEvent::ChunkCompleted { .. }
    ));
    assert!(controller.completed_chunks()[0].audio_path.exists());
}

#[test]
fn capture_controller_flushes_active_chunk_when_paused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut controller = CaptureController::new(CaptureControllerConfig {
        save_root: tmp.path().to_path_buf(),
        settings: settings(),
        sample_rate: 10,
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device: false,
        base_time: Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap(),
        app_version: "0.1.0".to_string(),
    });

    for _ in 0..5 {
        controller.process_samples(&[0.8; 1], 100).expect("speech");
    }

    let mut paused = settings();
    paused.pause_all = true;
    let events = controller.update_settings(paused).expect("pause");

    assert!(!controller.is_listening());
    assert_eq!(events.len(), 1);
    assert!(controller.completed_chunks()[0].audio_path.exists());
}

#[test]
fn capture_controller_flushes_active_chunk_before_threshold_settings_change() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut controller = CaptureController::new(CaptureControllerConfig {
        save_root: tmp.path().to_path_buf(),
        settings: settings(),
        sample_rate: 10,
        device_id: "default".to_string(),
        device_name: "System Default".to_string(),
        used_fallback_device: false,
        base_time: Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap(),
        app_version: "0.1.0".to_string(),
    });

    for _ in 0..5 {
        controller.process_samples(&[0.8; 1], 100).expect("speech");
    }
    assert!(controller.is_recording());

    let mut changed = settings();
    changed.threshold_dbfs = -35.0;
    let events = controller
        .update_settings(changed)
        .expect("change threshold");

    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        CaptureControllerEvent::ChunkCompleted { .. }
    ));
    assert!(!controller.is_recording());
    assert!(controller.completed_chunks()[0].audio_path.exists());
}
