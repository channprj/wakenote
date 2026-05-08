use chrono::{TimeZone, Utc};
use sagwan::audio::{
    GateConfig, GateDecision, SpeechGate, dbfs_from_rms, input_devices_from_labels,
};
use sagwan::models::{ModelStatus, default_model_registry};
use sagwan::settings::{
    AppSettings, AudioFormat, LaunchAtLoginAction, LiveCaptureRuntimeAction, SettingsPatch,
    expand_user_path, launch_at_login_action_for_patch, live_capture_runtime_action_for_patch,
    live_capture_should_run,
};
use sagwan::storage::{OutputBasename, next_available_output};

#[test]
fn dbfs_conversion_clamps_silence_and_reports_full_scale() {
    assert_eq!(dbfs_from_rms(0.0), -120.0);
    assert_eq!(dbfs_from_rms(-1.0), -120.0);
    assert!((dbfs_from_rms(1.0) - 0.0).abs() < 0.001);
    assert!((dbfs_from_rms(0.5) - -6.0206).abs() < 0.001);
}

#[test]
fn input_device_list_marks_system_default_unavailable_without_default_input() {
    let devices = input_devices_from_labels(false, ["USB Mic", "Built-in Microphone"]);

    assert_eq!(devices[0].id, "default");
    assert_eq!(devices[0].label, "System Default");
    assert!(!devices[0].available);
    assert_eq!(devices[1].id, "input-0-usb-mic");
    assert!(devices[1].available);
}

#[test]
fn speech_gate_rejects_transient_spikes_shorter_than_attack() {
    let mut gate = SpeechGate::new(GateConfig {
        threshold_dbfs: -45.0,
        attack_ms: 300,
        release_ms: 1_500,
        pre_roll_ms: 300,
        post_roll_ms: 300,
        min_chunk_ms: 500,
        max_chunk_ms: 30_000,
    });

    assert_eq!(gate.observe(-80.0, 0), GateDecision::Idle);
    assert_eq!(gate.observe(-30.0, 100), GateDecision::Idle);
    assert_eq!(gate.observe(-80.0, 250), GateDecision::Idle);
    assert!(!gate.is_recording());
}

#[test]
fn speech_gate_starts_after_attack_and_ends_after_release() {
    let mut gate = SpeechGate::new(GateConfig {
        threshold_dbfs: -45.0,
        attack_ms: 300,
        release_ms: 1_500,
        pre_roll_ms: 300,
        post_roll_ms: 300,
        min_chunk_ms: 500,
        max_chunk_ms: 30_000,
    });

    assert_eq!(gate.observe(-30.0, 0), GateDecision::Idle);
    assert_eq!(gate.observe(-31.0, 299), GateDecision::Idle);
    assert_eq!(
        gate.observe(-32.0, 300),
        GateDecision::Start { started_at_ms: 0 }
    );
    assert_eq!(gate.observe(-80.0, 1_000), GateDecision::Recording);
    assert_eq!(gate.observe(-82.0, 2_499), GateDecision::Recording);
    assert_eq!(gate.observe(-82.0, 2_500), GateDecision::Recording);
    assert_eq!(
        gate.observe(-82.0, 2_800),
        GateDecision::End { ended_at_ms: 2_800 }
    );
    assert!(!gate.is_recording());
}

#[test]
fn speech_gate_rolls_over_at_max_chunk_length_without_stopping() {
    let mut gate = SpeechGate::new(GateConfig {
        threshold_dbfs: -45.0,
        attack_ms: 100,
        release_ms: 1_500,
        pre_roll_ms: 0,
        post_roll_ms: 300,
        min_chunk_ms: 500,
        max_chunk_ms: 1_000,
    });

    assert_eq!(gate.observe(-30.0, 0), GateDecision::Idle);
    assert_eq!(
        gate.observe(-30.0, 100),
        GateDecision::Start { started_at_ms: 0 }
    );
    assert_eq!(
        gate.observe(-30.0, 1_100),
        GateDecision::Rollover {
            ended_at_ms: 1_100,
            next_started_at_ms: 1_100
        }
    );
    assert!(gate.is_recording());
}

#[test]
fn output_paths_use_date_time_format_and_collision_suffix() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let timestamp = Utc.with_ymd_and_hms(2026, 5, 6, 23, 7, 9).unwrap();

    let first = next_available_output(tmp.path(), timestamp, AudioFormat::M4a).expect("first path");
    assert_eq!(first.basename, OutputBasename::new("230709"));
    assert!(first.audio_path.ends_with("20260506/230709.m4a"));
    assert!(first.metadata_path.ends_with("20260506/230709.json"));
    assert!(first.transcript_path.ends_with("20260506/230709.txt"));
    assert!(first.error_path.ends_with("20260506/230709.error.txt"));

    std::fs::create_dir_all(first.audio_path.parent().unwrap()).expect("date dir");
    std::fs::write(&first.audio_path, b"existing").expect("collision file");

    let second =
        next_available_output(tmp.path(), timestamp, AudioFormat::M4a).expect("second path");
    assert_eq!(second.basename, OutputBasename::new("230709-001"));
    assert!(second.audio_path.ends_with("20260506/230709-001.m4a"));
}

#[test]
fn user_paths_expand_current_home_tilde_without_rewriting_other_paths() {
    let home = std::env::var_os("HOME").expect("HOME should be set for path expansion tests");

    assert_eq!(
        expand_user_path("~/Documents/Sagwan"),
        std::path::PathBuf::from(&home).join("Documents/Sagwan")
    );
    assert_eq!(expand_user_path("~"), std::path::PathBuf::from(&home));
    assert_eq!(
        expand_user_path("/tmp/sagwan"),
        std::path::PathBuf::from("/tmp/sagwan")
    );
    assert_eq!(
        expand_user_path("relative/sagwan"),
        std::path::PathBuf::from("relative/sagwan")
    );
    assert_eq!(
        expand_user_path("~other/sagwan"),
        std::path::PathBuf::from("~other/sagwan")
    );
}

#[test]
fn default_settings_match_prd_mvp_defaults() {
    let settings = AppSettings::default();

    assert!(settings.recording_enabled);
    assert!(settings.transcription_enabled);
    assert_eq!(settings.selected_microphone, "default");
    assert_eq!(settings.audio_format, AudioFormat::M4a);
    assert_eq!(settings.threshold_dbfs, -45.0);
    assert_eq!(settings.attack_ms, 300);
    assert_eq!(settings.release_ms, 1_500);
    assert_eq!(settings.pre_roll_ms, 300);
    assert_eq!(settings.post_roll_ms, 300);
    assert_eq!(settings.selected_model, "whisper-medium");
    assert_eq!(settings.theme_primary_color, "#0047AB");
}

#[test]
fn settings_patch_clamps_voice_gate_values_to_ui_ranges() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        threshold_dbfs: Some(12.0),
        attack_ms: Some(0),
        release_ms: Some(10_000),
        pre_roll_ms: Some(2_000),
        post_roll_ms: Some(4_000),
        min_chunk_ms: Some(0),
        max_chunk_ms: Some(1_000_000),
        ..SettingsPatch::default()
    });

    assert_eq!(settings.threshold_dbfs, -10.0);
    assert_eq!(settings.attack_ms, 50);
    assert_eq!(settings.release_ms, 5_000);
    assert_eq!(settings.pre_roll_ms, 1_500);
    assert_eq!(settings.post_roll_ms, 2_000);
    assert_eq!(settings.min_chunk_ms, 100);
    assert_eq!(settings.max_chunk_ms, 900_000);
}

#[test]
fn settings_patch_keeps_max_chunk_at_least_min_chunk() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        min_chunk_ms: Some(5_000),
        max_chunk_ms: Some(2_000),
        ..SettingsPatch::default()
    });

    assert_eq!(settings.min_chunk_ms, 5_000);
    assert_eq!(settings.max_chunk_ms, 10_000);
}

#[test]
fn launch_at_login_action_follows_explicit_settings_patch() {
    let settings = AppSettings::default();

    assert_eq!(
        launch_at_login_action_for_patch(&settings, &SettingsPatch::default()),
        LaunchAtLoginAction::Unchanged
    );
    assert_eq!(
        launch_at_login_action_for_patch(
            &settings,
            &SettingsPatch {
                launch_at_login: Some(true),
                ..SettingsPatch::default()
            }
        ),
        LaunchAtLoginAction::Enable
    );

    let enabled = AppSettings {
        launch_at_login: true,
        ..AppSettings::default()
    };

    assert_eq!(
        launch_at_login_action_for_patch(
            &enabled,
            &SettingsPatch {
                launch_at_login: Some(false),
                ..SettingsPatch::default()
            }
        ),
        LaunchAtLoginAction::Disable
    );
}

#[test]
fn live_capture_runtime_action_stops_when_recording_is_disabled_or_paused() {
    let settings = AppSettings::default();

    assert_eq!(
        live_capture_runtime_action_for_patch(
            &settings,
            &SettingsPatch {
                recording_enabled: Some(false),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Stop
    );
    assert_eq!(
        live_capture_runtime_action_for_patch(
            &settings,
            &SettingsPatch {
                pause_all: Some(true),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Stop
    );
    assert_eq!(
        live_capture_runtime_action_for_patch(&settings, &SettingsPatch::default()),
        LiveCaptureRuntimeAction::Unchanged
    );

    let already_paused = AppSettings {
        pause_all: true,
        ..AppSettings::default()
    };

    assert_eq!(
        live_capture_runtime_action_for_patch(
            &already_paused,
            &SettingsPatch {
                pause_all: Some(true),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Unchanged
    );
}

#[test]
fn live_capture_runtime_action_starts_and_restarts_for_active_recording_settings() {
    let disabled = AppSettings {
        recording_enabled: false,
        ..AppSettings::default()
    };

    assert_eq!(
        live_capture_runtime_action_for_patch(
            &disabled,
            &SettingsPatch {
                recording_enabled: Some(true),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Start
    );

    let paused = AppSettings {
        pause_all: true,
        ..AppSettings::default()
    };

    assert_eq!(
        live_capture_runtime_action_for_patch(
            &paused,
            &SettingsPatch {
                pause_all: Some(false),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Start
    );

    let active = AppSettings::default();

    assert_eq!(
        live_capture_runtime_action_for_patch(
            &active,
            &SettingsPatch {
                selected_microphone: Some("usb-mic".to_string()),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Restart
    );
    assert_eq!(
        live_capture_runtime_action_for_patch(
            &active,
            &SettingsPatch {
                selected_microphone: Some(active.selected_microphone.clone()),
                ..SettingsPatch::default()
            }
        ),
        LiveCaptureRuntimeAction::Unchanged
    );
}

#[test]
fn live_capture_should_run_only_when_recording_is_enabled_and_not_paused() {
    assert!(live_capture_should_run(&AppSettings::default()));
    assert!(!live_capture_should_run(&AppSettings {
        recording_enabled: false,
        ..AppSettings::default()
    }));
    assert!(!live_capture_should_run(&AppSettings {
        pause_all: true,
        ..AppSettings::default()
    }));
}

#[test]
fn default_model_registry_includes_whisper_medium_and_fast_local_fallback() {
    let registry = default_model_registry();

    let medium = registry.get("whisper-medium").expect("medium model");
    assert_eq!(medium.display_name, "Whisper Medium");
    assert!(medium.offline);
    assert_eq!(medium.status, ModelStatus::Missing);
    assert!(medium.checksum_sha256.is_some());

    let fast = registry.get("whisper-tiny").expect("fast fallback");
    assert!(fast.speed_score > medium.speed_score);
    assert!(fast.offline);
}
