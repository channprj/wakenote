use sagwan::commands::{AppBackend, AppMode, TrayState};
use sagwan::settings::SettingsPatch;

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
