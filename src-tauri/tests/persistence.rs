use sagwan::commands::AppBackend;
use sagwan::persistence::AppPersistence;
use sagwan::queue::{QueueJobStatus, TranscriptionQueue};
use sagwan::settings::{AppSettings, SettingsPatch};

#[test]
fn persistence_round_trips_settings_json() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = AppPersistence::new(tmp.path());
    let mut settings = AppSettings::default();
    settings.apply_patch(SettingsPatch {
        recording_enabled: Some(false),
        threshold_dbfs: Some(-38.0),
        selected_microphone: Some("input-1-usb-mic".to_string()),
        selected_microphone_label: Some("USB Mic".to_string()),
        save_root: Some("/tmp/sagwan-recordings".to_string()),
        ..SettingsPatch::default()
    });

    store.save_settings(&settings).expect("save settings");

    let loaded = store
        .load_settings()
        .expect("load settings")
        .expect("settings");
    assert!(!loaded.recording_enabled);
    assert_eq!(loaded.threshold_dbfs, -38.0);
    assert_eq!(loaded.selected_microphone, "input-1-usb-mic");
    assert_eq!(loaded.selected_microphone_label, "USB Mic");
    assert_eq!(loaded.save_root, "/tmp/sagwan-recordings");
    assert!(loaded.save_root_confirmed);
}

#[test]
fn persistence_migrates_legacy_settings_with_missing_fields() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = AppPersistence::new(tmp.path());
    std::fs::write(
        tmp.path().join("settings.json"),
        r#"{
          "recording_enabled": false,
          "threshold_dbfs": -39.5,
          "save_root": "/tmp/legacy-sagwan"
        }"#,
    )
    .expect("legacy settings");

    let loaded = store
        .load_settings()
        .expect("load settings")
        .expect("settings");

    assert!(!loaded.recording_enabled);
    assert_eq!(loaded.threshold_dbfs, -39.5);
    assert_eq!(loaded.save_root, "/tmp/legacy-sagwan");
    assert!(loaded.save_root_confirmed);
    assert!(loaded.transcription_enabled);
    assert!(!loaded.calibration_completed);
    assert_eq!(loaded.attack_ms, 300);
    assert_eq!(loaded.selected_model, "whisper-medium");
    assert_eq!(loaded.theme_primary_color, "#0047AB");
}

#[test]
fn persistence_round_trips_queue_and_recovers_running_jobs_as_pending() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = AppPersistence::new(tmp.path());
    let mut queue = TranscriptionQueue::new();
    let first = queue.enqueue_file("/recordings/20260506/230709.wav", "whisper-medium");
    let second = queue.enqueue_file("/recordings/20260506/230810.wav", "whisper-tiny");
    queue.start_next().expect("running job");
    queue
        .mark_failed(second, "checksum mismatch")
        .expect("fail job");

    store.save_queue(&queue).expect("save queue");

    let loaded = store.load_queue().expect("load queue").expect("queue");
    assert_eq!(loaded.job(first).unwrap().status, QueueJobStatus::Pending);
    assert_eq!(loaded.job(second).unwrap().status, QueueJobStatus::Failed);

    let third = {
        let mut loaded = loaded;
        loaded.enqueue_file("/recordings/20260506/230911.wav", "whisper-medium")
    };
    assert_eq!(third, 3);
}

#[test]
fn backend_loaded_from_dir_persists_settings_and_queue_mutations() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp
        .path()
        .join("recordings")
        .join("20260506")
        .join("230709.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut backend = AppBackend::load_from_dir(tmp.path()).expect("load backend");

    backend.update_settings(SettingsPatch {
        transcription_enabled: Some(false),
        threshold_dbfs: Some(-42.0),
        ..SettingsPatch::default()
    });
    backend.enqueue_audio_file(&audio_path, None);

    let reloaded = AppBackend::load_from_dir(tmp.path()).expect("reload backend");
    assert!(!reloaded.settings().transcription_enabled);
    assert_eq!(reloaded.settings().threshold_dbfs, -42.0);
    assert_eq!(reloaded.queue_snapshot().pending_count, 1);
}
