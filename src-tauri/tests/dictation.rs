use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use wakenote::dictation::{
    DictationAction, DictationRuntime, DictationStage, ShortcutRegistrationChange,
    candidate_dictation_settings, prepare_dictation_audio, shortcut_registration_change,
    transcribe_dictation_recording, validate_dictation_shortcut,
};
use wakenote::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, LiveCaptureError,
};
use wakenote::settings::{AppSettings, SettingsPatch, TranscriptionLanguage};
use wakenote::transcription::{Transcriber, TranscriptionError, TranscriptionRequest};

struct FakeInput {
    frames: Vec<AudioFrame>,
}

struct FakeStream;

impl AudioStreamHandle for FakeStream {}

#[derive(Clone)]
struct FakeTranscriber {
    request: Arc<Mutex<Option<(PathBuf, String, TranscriptionLanguage)>>>,
    text: String,
}

impl Transcriber for FakeTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        *self.request.lock().expect("request lock") = Some((
            request.audio_path.to_path_buf(),
            request.model_id.to_string(),
            request.language,
        ));
        Ok(self.text.clone())
    }
}

impl AudioInputBackend for FakeInput {
    fn start(
        &mut self,
        _config: AudioInputConfig,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        for frame in self.frames.drain(..) {
            on_frame(frame);
        }
        Ok(Box::new(FakeStream))
    }
}

#[test]
fn shortcut_validation_accepts_modified_keys_and_function_keys() {
    assert_eq!(
        validate_dictation_shortcut("  Ctrl+Alt+D  ").expect("valid shortcut"),
        "ctrl+alt+d"
    );
    assert_eq!(
        validate_dictation_shortcut("F8").expect("function key"),
        "f8"
    );
}

#[test]
fn shortcut_validation_rejects_unsafe_or_ambiguous_combinations() {
    for shortcut in ["", "ctrl+shift", "space", "fn+space", "ctrl+a+b", "ctrl++d"] {
        assert!(
            validate_dictation_shortcut(shortcut).is_err(),
            "{shortcut:?} should be rejected"
        );
    }
}

#[test]
fn toggle_state_machine_debounces_repeat_and_ignores_transcribing_presses() {
    let mut runtime = DictationRuntime::new(FakeInput { frames: Vec::new() });
    let started_at = Instant::now();

    assert_eq!(
        runtime.handle_press_at(started_at),
        DictationAction::StartRecording
    );
    assert_eq!(runtime.stage(), DictationStage::Recording);
    assert_eq!(
        runtime.handle_press_at(started_at + Duration::from_millis(299)),
        DictationAction::Ignore
    );
    assert_eq!(
        runtime.handle_press_at(started_at + Duration::from_millis(600)),
        DictationAction::StopAndTranscribe
    );
    assert_eq!(runtime.stage(), DictationStage::Transcribing);
    assert_eq!(
        runtime.handle_press_at(started_at + Duration::from_secs(1)),
        DictationAction::Ignore
    );

    runtime.finish();
    assert_eq!(runtime.stage(), DictationStage::Idle);
}

#[test]
fn dedicated_capture_collects_frames_until_stopped() {
    let frame = AudioFrame {
        samples: vec![0.25; 16_000],
        duration_ms: 1_000,
        captured_at: Utc::now(),
    };
    let mut runtime = DictationRuntime::new(FakeInput {
        frames: vec![frame],
    });
    let started_at = Instant::now();
    assert_eq!(
        runtime.handle_press_at(started_at),
        DictationAction::StartRecording
    );

    let recording_id = runtime
        .start_recording(AudioInputConfig {
            device_id: "fake".to_string(),
            sample_rate: Some(16_000),
            label_hint: Some("Fake microphone".to_string()),
        })
        .expect("capture starts");
    assert!(recording_id > 0);

    assert_eq!(
        runtime.handle_press_at(started_at + Duration::from_secs(1)),
        DictationAction::StopAndTranscribe
    );
    let recording = runtime.stop_recording().expect("capture stops");
    assert_eq!(recording.sample_rate, 16_000);
    assert_eq!(recording.samples, vec![0.25; 16_000]);
}

#[test]
fn transcription_audio_is_resampled_and_low_signal_is_skipped() {
    let audible = vec![0.1; 48_000];
    let prepared = prepare_dictation_audio(&audible, 48_000)
        .expect("valid sample rate")
        .expect("audible recording");
    assert_eq!(prepared.len(), 16_000);

    let quiet = vec![0.0; 48_000];
    assert_eq!(
        prepare_dictation_audio(&quiet, 48_000).expect("valid sample rate"),
        None
    );
    assert!(prepare_dictation_audio(&audible, 0).is_err());
}

#[test]
fn shortcut_registration_change_tracks_enable_replace_and_disable() {
    assert_eq!(
        shortcut_registration_change(false, "alt+space", true, "ctrl+alt+d"),
        ShortcutRegistrationChange::Register("ctrl+alt+d".to_string())
    );
    assert_eq!(
        shortcut_registration_change(true, "alt+space", true, "ctrl+alt+d"),
        ShortcutRegistrationChange::Replace {
            previous: "alt+space".to_string(),
            next: "ctrl+alt+d".to_string(),
        }
    );
    assert_eq!(
        shortcut_registration_change(true, "ctrl+alt+d", false, "ctrl+alt+d"),
        ShortcutRegistrationChange::Unregister("ctrl+alt+d".to_string())
    );
    assert_eq!(
        shortcut_registration_change(true, "ctrl+alt+d", true, "ctrl+alt+d"),
        ShortcutRegistrationChange::Unchanged
    );
}

#[test]
fn automatic_stop_only_finishes_the_matching_recording() {
    let frame = AudioFrame {
        samples: vec![0.25; 16_000],
        duration_ms: 1_000,
        captured_at: Utc::now(),
    };
    let mut runtime = DictationRuntime::new(FakeInput {
        frames: vec![frame],
    });
    let started_at = Instant::now();
    assert_eq!(
        runtime.handle_press_at(started_at),
        DictationAction::StartRecording
    );
    let recording_id = runtime
        .start_recording(AudioInputConfig {
            device_id: "fake".to_string(),
            sample_rate: Some(16_000),
            label_hint: None,
        })
        .expect("capture starts");

    assert_eq!(runtime.stop_if_recording(recording_id + 1), None);
    assert_eq!(runtime.stage(), DictationStage::Recording);

    let recording = runtime
        .stop_if_recording(recording_id)
        .expect("matching recording stops")
        .expect("capture result");
    assert_eq!(recording.samples.len(), 16_000);
    assert_eq!(runtime.stage(), DictationStage::Transcribing);
}

#[test]
fn dictation_transcription_uses_ephemeral_16khz_wav_and_requested_language() {
    let request = Arc::new(Mutex::new(None));
    let transcriber = FakeTranscriber {
        request: request.clone(),
        text: "  hello world  ".to_string(),
    };
    let recording = wakenote::dictation::DictationRecording {
        samples: vec![0.1; 48_000],
        sample_rate: 48_000,
    };

    let text = transcribe_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        transcriber,
    )
    .expect("transcription succeeds");
    assert_eq!(text.as_deref(), Some("hello world"));

    let (audio_path, model_id, language) = request
        .lock()
        .expect("request lock")
        .clone()
        .expect("request");
    assert_eq!(model_id, "whisper-medium");
    assert_eq!(language, TranscriptionLanguage::Auto);
    assert_eq!(
        audio_path.extension().and_then(|value| value.to_str()),
        Some("wav")
    );
    assert!(
        !audio_path.exists(),
        "temporary dictation audio should be removed"
    );
}

#[test]
fn dictation_settings_candidate_rejects_invalid_shortcuts_before_apply() {
    let previous = AppSettings::default();
    assert!(
        candidate_dictation_settings(
            &previous,
            &SettingsPatch {
                dictation_enabled: Some(true),
                dictation_shortcut: Some("   ".to_string()),
                ..Default::default()
            },
        )
        .is_err()
    );
    assert!(!previous.dictation_enabled);
    assert_eq!(previous.dictation_shortcut, "alt+space");

    let candidate = candidate_dictation_settings(
        &previous,
        &SettingsPatch {
            dictation_enabled: Some(true),
            dictation_shortcut: Some(" Ctrl + Alt + D ".to_string()),
            ..Default::default()
        },
    )
    .expect("valid candidate");
    assert!(candidate.dictation_enabled);
    assert_eq!(candidate.dictation_shortcut, "ctrl+alt+d");
}
