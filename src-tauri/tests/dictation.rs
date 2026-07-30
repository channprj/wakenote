use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use wakenote::dictation::{
    DictationAction, DictationProcessOutcome, DictationRecording, DictationRuntime,
    DictationShortcutEvent, DictationStage, ModifierShortcut, ModifierShortcutRuntime,
    ShortcutRegistrationChange, candidate_dictation_settings, modifier_shortcut,
    prepare_dictation_audio, shortcut_registration_change, transcribe_and_type_dictation_recording,
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
struct OrderedFakeTranscriber {
    events: Arc<Mutex<Vec<String>>>,
    text: String,
}

impl Transcriber for OrderedFakeTranscriber {
    fn transcribe(&self, _request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        self.events
            .lock()
            .expect("events")
            .push("transcribed".to_string());
        Ok(self.text.clone())
    }
}

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
fn shortcut_validation_accepts_requested_combinations_and_single_keys() {
    assert_eq!(
        validate_dictation_shortcut("  Ctrl+Alt+D  ").expect("valid shortcut"),
        "ctrl+alt+d"
    );
    assert_eq!(
        validate_dictation_shortcut("F8").expect("function key"),
        "f8"
    );
    assert_eq!(
        validate_dictation_shortcut(" Shift + Control ").expect("modifier chord"),
        "ctrl+shift"
    );
    for (shortcut, normalized) in [
        ("Control+V", "ctrl+v"),
        ("Option+Command", "alt+cmd"),
        ("Shift+Z", "shift+z"),
        ("Z", "z"),
        ("Space", "space"),
        ("LeftControl", "leftctrl"),
    ] {
        assert_eq!(
            validate_dictation_shortcut(shortcut).as_deref(),
            Ok(normalized),
            "{shortcut} should be supported"
        );
    }
}

#[test]
fn shortcut_validation_rejects_unsafe_or_ambiguous_combinations() {
    for shortcut in ["", "ctrl", "fn+space", "ctrl+a+b", "ctrl++d"] {
        assert!(
            validate_dictation_shortcut(shortcut).is_err(),
            "{shortcut:?} should be rejected"
        );
    }
}

#[test]
fn modifier_shortcut_emits_one_press_and_release_per_exact_cycle() {
    assert!(
        modifier_shortcut("alt+d")
            .expect("valid keyed shortcut")
            .is_none()
    );
    let chord = modifier_shortcut("ctrl+shift")
        .expect("valid shortcut")
        .expect("modifier shortcut");
    let mut runtime = ModifierShortcutRuntime::default();
    runtime.register(chord).expect("shortcut registers");

    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, false, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        Some(DictationShortcutEvent::Pressed)
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, true, true, false)),
        Some(DictationShortcutEvent::Released)
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, false, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        Some(DictationShortcutEvent::Pressed)
    );
}

#[test]
fn physical_modifier_shortcut_is_routed_to_the_native_monitor() {
    let shortcut = modifier_shortcut("LeftControl")
        .expect("valid physical shortcut")
        .expect("native shortcut");

    assert_eq!(shortcut.canonical_string().as_deref(), Some("leftctrl"));
}

#[test]
fn physical_modifier_shortcut_emits_edges_only_for_the_selected_side() {
    let shortcut = modifier_shortcut("leftctrl")
        .expect("valid physical shortcut")
        .expect("native shortcut");
    let mut runtime = ModifierShortcutRuntime::default();
    runtime.register(shortcut).expect("shortcut registers");

    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(false, false, false, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(shortcut),
        Some(DictationShortcutEvent::Pressed)
    );
    assert_eq!(runtime.handle_modifiers(shortcut), None);
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::Physical(
            wakenote::dictation::PhysicalModifierKey::RightControl,
        )),
        Some(DictationShortcutEvent::Released)
    );
    assert_eq!(runtime.handle_modifiers(shortcut), None);
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(false, false, false, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(shortcut),
        Some(DictationShortcutEvent::Pressed)
    );
}

#[test]
fn modifier_shortcut_requires_an_exact_chord_and_rearms_after_required_release() {
    let chord = modifier_shortcut("ctrl+shift")
        .expect("valid shortcut")
        .expect("modifier shortcut");
    let mut runtime = ModifierShortcutRuntime::default();
    runtime.register(chord).expect("shortcut registers");

    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, true, false, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, true, true, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(false, false, true, false)),
        None
    );
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        Some(DictationShortcutEvent::Pressed)
    );

    runtime.unregister(chord);
    assert_eq!(
        runtime.handle_modifiers(ModifierShortcut::new(true, false, true, false)),
        None
    );
}

#[test]
fn modifier_shortcut_serializes_supported_chords_in_canonical_order() {
    assert_eq!(
        ModifierShortcut::new(true, false, true, false).canonical_string(),
        Some("ctrl+shift".to_string())
    );
    assert_eq!(
        ModifierShortcut::new(true, true, true, true).canonical_string(),
        Some("ctrl+alt+shift+cmd".to_string())
    );
    assert_eq!(
        ModifierShortcut::new(true, false, false, false).canonical_string(),
        None
    );
}

#[test]
fn backend_parses_every_main_key_emitted_by_the_frontend_capture_helper() {
    let mut main_keys = (b'a'..=b'z')
        .map(|value| char::from(value).to_string())
        .chain((b'0'..=b'9').map(|value| char::from(value).to_string()))
        .chain((1..=24).map(|value| format!("f{value}")))
        .collect::<Vec<_>>();
    main_keys.extend(
        [
            "space",
            "enter",
            "tab",
            "backspace",
            "delete",
            "arrowup",
            "arrowdown",
            "arrowleft",
            "arrowright",
            "home",
            "end",
            "pageup",
            "pagedown",
        ]
        .map(str::to_string),
    );

    for main_key in main_keys {
        let function_key = main_key
            .strip_prefix('f')
            .and_then(|value| value.parse::<u8>().ok())
            .is_some_and(|value| (1..=24).contains(&value));
        let shortcut = if function_key {
            main_key.clone()
        } else {
            format!("alt+{main_key}")
        };
        assert_eq!(
            validate_dictation_shortcut(&shortcut).as_deref(),
            Ok(shortcut.as_str()),
            "{shortcut} should parse"
        );
    }
}

#[test]
fn hold_to_talk_stops_only_on_release() {
    let mut runtime = DictationRuntime::new(FakeInput { frames: Vec::new() });

    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::StartRecording
    );
    assert_eq!(runtime.stage(), DictationStage::Recording);
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::Ignore
    );
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::StopAndTranscribe
    );
    assert_eq!(runtime.stage(), DictationStage::Transcribing);
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::Ignore
    );
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::Ignore
    );

    runtime.finish();
    assert_eq!(runtime.stage(), DictationStage::Idle);
}

#[test]
fn hold_to_talk_plays_stop_cue_after_capture_start_failure() {
    let mut runtime = DictationRuntime::new(FakeInput { frames: Vec::new() });

    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::StartRecording
    );
    runtime.fail();
    assert!(runtime.reset_error());
    assert_eq!(runtime.stage(), DictationStage::Idle);

    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::PlayStopCue
    );
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::Ignore
    );
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
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
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
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::StopAndTranscribe
    );
    let recording = runtime.stop_recording().expect("capture stops");
    assert_eq!(recording.sample_rate, 16_000);
    assert_eq!(recording.samples, vec![0.25; 16_000]);
}

#[test]
fn dictation_capture_forwards_each_frame_for_global_feedback() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_for_callback = observed.clone();
    let mut runtime = DictationRuntime::new(FakeInput {
        frames: vec![AudioFrame {
            samples: vec![0.25; 16_000],
            duration_ms: 1_000,
            captured_at: Utc::now(),
        }],
    });
    runtime.handle_shortcut_event(DictationShortcutEvent::Pressed);
    runtime
        .start_recording_with_frame_handler(
            AudioInputConfig {
                device_id: "fake".to_string(),
                sample_rate: Some(16_000),
                label_hint: None,
            },
            move |frame| {
                observed_for_callback
                    .lock()
                    .expect("observed")
                    .push(frame.samples.len());
            },
        )
        .expect("capture starts");
    runtime.handle_shortcut_event(DictationShortcutEvent::Released);
    runtime.stop_recording().expect("capture stops");

    assert_eq!(*observed.lock().expect("observed"), vec![16_000]);
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
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
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
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::Ignore
    );
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
fn dictation_types_only_after_transcription_returns() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transcriber = OrderedFakeTranscriber {
        events: events.clone(),
        text: "  hello  ".to_string(),
    };
    let sink_events = events.clone();
    let recording = DictationRecording {
        samples: vec![0.1; 48_000],
        sample_rate: 48_000,
    };

    let outcome = transcribe_and_type_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        transcriber,
        move |text| {
            sink_events
                .lock()
                .expect("events")
                .push(format!("typed:{text}"));
            Ok(())
        },
    )
    .expect("operation succeeds");

    assert_eq!(outcome, DictationProcessOutcome::Typed("hello".to_string()));
    assert_eq!(
        *events.lock().expect("events"),
        vec!["transcribed".to_string(), "typed:hello".to_string()]
    );
}

#[test]
fn quiet_dictation_never_calls_the_text_sink() {
    let transcriber = OrderedFakeTranscriber {
        events: Arc::new(Mutex::new(Vec::new())),
        text: "unused".to_string(),
    };
    let recording = DictationRecording {
        samples: vec![0.0; 48_000],
        sample_rate: 48_000,
    };

    let outcome = transcribe_and_type_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        transcriber,
        |_| panic!("quiet audio must not type"),
    )
    .expect("quiet recording is handled");

    assert_eq!(outcome, DictationProcessOutcome::NoSpeech);
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
