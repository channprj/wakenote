use chrono::{TimeZone, Utc};
use wakenote::audio::{
    GateConfig, GateDecision, SpeechGate, dbfs_from_rms, input_devices_from_labels,
};
use wakenote::models::{ModelStatus, default_model_registry};
use wakenote::settings::{
    AppSettings, AudioFormat, LaunchAtLoginAction, LiveCaptureRuntimeAction, SettingsPatch,
    TranscriptionLanguage, expand_user_path, launch_at_login_action_for_patch,
    live_capture_runtime_action_for_patch, live_capture_should_run,
    live_capture_should_start_on_launch,
};
use wakenote::storage::{OutputBasename, next_available_output};

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
    let local = timestamp.with_timezone(&chrono::Local);
    let dir = local.format("%Y%m%d").to_string();
    let stem = local.format("%H%M%S").to_string();

    let first = next_available_output(tmp.path(), timestamp, AudioFormat::M4a).expect("first path");
    assert_eq!(first.basename, OutputBasename::new(stem.clone()));
    assert!(first.audio_path.ends_with(format!("{dir}/{stem}.m4a")));
    assert!(first.metadata_path.ends_with(format!("{dir}/{stem}.json")));
    assert!(first.transcript_path.ends_with(format!("{dir}/{stem}.txt")));
    assert!(
        first
            .error_path
            .ends_with(format!("{dir}/{stem}.error.txt"))
    );

    std::fs::create_dir_all(first.audio_path.parent().unwrap()).expect("date dir");
    std::fs::write(&first.audio_path, b"existing").expect("collision file");

    let second =
        next_available_output(tmp.path(), timestamp, AudioFormat::M4a).expect("second path");
    assert_eq!(second.basename, OutputBasename::new(format!("{stem}-001")));
    assert!(second.audio_path.ends_with(format!("{dir}/{stem}-001.m4a")));
}

#[test]
fn user_paths_expand_current_home_tilde_without_rewriting_other_paths() {
    let home = std::env::var_os("HOME").expect("HOME should be set for path expansion tests");

    assert_eq!(
        expand_user_path("~/Documents/WakeNote"),
        std::path::PathBuf::from(&home).join("Documents/WakeNote")
    );
    assert_eq!(expand_user_path("~"), std::path::PathBuf::from(&home));
    assert_eq!(
        expand_user_path("/tmp/wakenote"),
        std::path::PathBuf::from("/tmp/wakenote")
    );
    assert_eq!(
        expand_user_path("relative/wakenote"),
        std::path::PathBuf::from("relative/wakenote")
    );
    assert_eq!(
        expand_user_path("~other/wakenote"),
        std::path::PathBuf::from("~other/wakenote")
    );
}

#[test]
fn default_settings_match_prd_mvp_defaults() {
    let settings = AppSettings::default();

    assert!(settings.recording_enabled);
    assert!(settings.transcription_enabled);
    assert!(!settings.save_root_confirmed);
    assert!(!settings.calibration_completed);
    assert_eq!(settings.selected_microphone, "default");
    assert_eq!(settings.audio_format, AudioFormat::M4a);
    assert_eq!(settings.threshold_dbfs, -60.0);
    assert_eq!(settings.transcription_language, TranscriptionLanguage::Ko);
    assert!(settings.suppress_low_confidence_transcripts);
    assert_eq!(settings.attack_ms, 200);
    assert_eq!(settings.release_ms, 1_500);
    assert_eq!(settings.pre_roll_ms, 400);
    assert_eq!(settings.post_roll_ms, 400);
    assert_eq!(settings.max_chunk_ms, 120_000);
    assert_eq!(settings.selected_model, "whisper-medium");
    assert_eq!(settings.save_root, "~/Documents/WakeNote");
    assert_eq!(
        settings.model_directory,
        "~/Library/Application Support/WakeNote/models"
    );
    assert!(settings.show_dock_icon);
    assert!(settings.show_tray_icon);
    assert!(settings.start_live_input_on_launch);
    assert_eq!(settings.theme_primary_color, "#000");
}

#[test]
fn settings_patch_updates_dock_icon_visibility() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        show_dock_icon: Some(false),
        ..SettingsPatch::default()
    });

    assert!(!settings.show_dock_icon);
}

#[test]
fn settings_patch_updates_start_live_input_on_launch() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        start_live_input_on_launch: Some(false),
        ..SettingsPatch::default()
    });

    assert!(!settings.start_live_input_on_launch);
}

#[test]
fn settings_patch_updates_transcription_language() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        transcription_language: Some(TranscriptionLanguage::Ko),
        ..SettingsPatch::default()
    });

    assert_eq!(settings.transcription_language, TranscriptionLanguage::Ko);
}

#[test]
fn settings_patch_updates_low_confidence_transcript_suppression() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        suppress_low_confidence_transcripts: Some(false),
        ..SettingsPatch::default()
    });

    assert!(!settings.suppress_low_confidence_transcripts);
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
fn settings_patch_marks_non_empty_save_root_confirmed() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        save_root: Some("/tmp/wakenote-recordings".to_string()),
        ..SettingsPatch::default()
    });

    assert_eq!(settings.save_root, "/tmp/wakenote-recordings");
    assert!(settings.save_root_confirmed);

    settings.apply_patch(SettingsPatch {
        save_root: Some("   ".to_string()),
        ..SettingsPatch::default()
    });

    assert_eq!(settings.save_root, "   ");
    assert!(!settings.save_root_confirmed);
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
fn settings_patch_records_completed_threshold_calibration() {
    let mut settings = AppSettings::default();

    settings.apply_patch(SettingsPatch {
        threshold_dbfs: Some(-43.0),
        calibration_completed: Some(true),
        ..SettingsPatch::default()
    });

    assert_eq!(settings.threshold_dbfs, -43.0);
    assert!(settings.calibration_completed);
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
fn live_capture_should_start_on_launch_respects_auto_start_toggle() {
    assert!(live_capture_should_start_on_launch(&AppSettings::default()));
    assert!(!live_capture_should_start_on_launch(&AppSettings {
        start_live_input_on_launch: false,
        ..AppSettings::default()
    }));
    assert!(!live_capture_should_start_on_launch(&AppSettings {
        recording_enabled: false,
        ..AppSettings::default()
    }));
    assert!(!live_capture_should_start_on_launch(&AppSettings {
        pause_all: true,
        ..AppSettings::default()
    }));
}

#[test]
fn default_model_registry_includes_whisper_medium_and_fast_local_fallback() {
    let registry = default_model_registry();

    let small = registry.get("whisper-small").expect("small model");
    assert_eq!(small.display_name, "Whisper Small");
    assert!(small.languages.iter().any(|language| language == "ko"));
    assert_eq!(small.size_mb, 465);
    assert!(small.offline);
    assert_eq!(small.status, ModelStatus::Missing);
    assert!(small.checksum_sha256.is_some());

    let medium = registry.get("whisper-medium").expect("medium model");
    assert_eq!(medium.display_name, "Whisper Medium");
    assert!(medium.offline);
    assert_eq!(medium.status, ModelStatus::Missing);
    assert!(medium.checksum_sha256.is_some());

    let fast = registry.get("whisper-tiny").expect("fast fallback");
    assert!(fast.speed_score > medium.speed_score);
    assert!(fast.offline);
    assert!(fast.languages.iter().any(|language| language == "ko"));

    let turbo = registry.get("whisper-turbo").expect("turbo model");
    assert_eq!(turbo.display_name, "Whisper Turbo");
    assert!(turbo.offline);
    assert_eq!(turbo.status, ModelStatus::Missing);
    assert!(turbo.speed_score > medium.speed_score);
    assert!(turbo.accuracy_score < medium.accuracy_score);
    assert!(turbo.languages.iter().any(|language| language == "ko"));

    let large = registry.get("whisper-large").expect("large model");
    assert_eq!(large.display_name, "Whisper Large");
    assert!(large.languages.iter().any(|language| language == "ko"));
    assert_eq!(large.size_mb, 1_031);
    assert!(large.offline);
    assert_eq!(large.status, ModelStatus::Missing);
    assert!(large.accuracy_score >= medium.accuracy_score);
    assert!(large.checksum_sha256.is_some());
}

#[test]
fn default_registry_uses_pinned_remote_sha256() {
    // Pin the exact SHA256 checksums for each bundled whisper.cpp model.
    // Bumping or changing any of these strings means the binary content has
    // shifted; verify against `curl -L <url> | shasum -a 256` before updating.
    const EXPECTED: &[(&str, &str)] = &[
        (
            "whisper-small",
            "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        ),
        (
            "whisper-medium",
            "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        ),
        (
            "whisper-tiny",
            "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        ),
        (
            "whisper-turbo",
            "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        ),
        (
            "whisper-large",
            "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
        ),
    ];

    let registry = default_model_registry();
    for (model_id, expected_hash) in EXPECTED {
        let model = registry
            .get(*model_id)
            .unwrap_or_else(|| panic!("registry missing {model_id}"));
        let actual = model
            .checksum_sha256
            .as_deref()
            .unwrap_or_else(|| panic!("{model_id} missing checksum_sha256"));
        assert_eq!(actual, *expected_hash, "{model_id} checksum mismatch");
        assert_eq!(actual.len(), 64, "{model_id} checksum must be 64 hex chars");
        assert!(
            actual
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{model_id} checksum must be lowercase hex"
        );
    }
}
