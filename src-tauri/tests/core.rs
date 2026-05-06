use chrono::{TimeZone, Utc};
use sagwan::audio::{GateConfig, GateDecision, SpeechGate, dbfs_from_rms};
use sagwan::models::{ModelStatus, default_model_registry};
use sagwan::settings::{AppSettings, AudioFormat};
use sagwan::storage::{OutputBasename, next_available_output};

#[test]
fn dbfs_conversion_clamps_silence_and_reports_full_scale() {
    assert_eq!(dbfs_from_rms(0.0), -120.0);
    assert_eq!(dbfs_from_rms(-1.0), -120.0);
    assert!((dbfs_from_rms(1.0) - 0.0).abs() < 0.001);
    assert!((dbfs_from_rms(0.5) - -6.0206).abs() < 0.001);
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
    assert_eq!(
        gate.observe(-82.0, 2_500),
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
