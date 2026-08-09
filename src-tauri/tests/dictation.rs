use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use wakenote::dictation::{
    DictationAction, DictationMicrophoneInput, DictationMixSession, DictationProcessOutcome,
    DictationRecording, DictationRuntime, DictationShortcutEvent, DictationStage, ModifierShortcut,
    ModifierShortcutRuntime, ShortcutRegistrationChange, archive_dictation_recording,
    candidate_dictation_settings, modifier_shortcut, prepare_dictation_audio,
    shortcut_registration_change, transcribe_and_type_dictation_recording,
    transcribe_dictation_recording, transcribe_dictation_recording_execution,
    validate_dictation_shortcut,
};
use wakenote::dictionary::DictionaryContext;
use wakenote::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, LiveCaptureError,
};
use wakenote::recorder::{ChunkMetadata, ChunkSource, TranscriptionStatus};
use wakenote::settings::{
    AppSettings, AudioFormat, CaptureMicrophoneEntry, DictionaryEntry, MicrophoneSlot,
    SettingsPatch, TranscriptionLanguage,
};
use wakenote::transcription::{
    Transcriber, TranscriptionError, TranscriptionExecution, TranscriptionRequest,
    TranscriptionUsage,
};
use wakenote::voice_leveling::AudioFrameProcessor;

struct ScalingProcessor(f32);

fn dictation_input(slot: MicrophoneSlot, id: &str, label: &str) -> DictationMicrophoneInput {
    DictationMicrophoneInput {
        slot,
        device: CaptureMicrophoneEntry {
            id: id.to_string(),
            label: label.to_string(),
            core_audio_uid: None,
        },
    }
}

fn frame(samples: Vec<f32>, duration_ms: u64, captured_at_ms: i64) -> AudioFrame {
    AudioFrame {
        samples,
        duration_ms,
        captured_at: Utc
            .timestamp_millis_opt(captured_at_ms)
            .single()
            .expect("timestamp"),
    }
}

fn flatten_samples(frames: Vec<AudioFrame>) -> Vec<f32> {
    frames.into_iter().flat_map(|frame| frame.samples).collect()
}

fn primary_dictation_mix_session(target_rate: u32) -> DictationMixSession {
    let mut session = DictationMixSession::new(
        chrono::DateTime::<Utc>::UNIX_EPOCH,
        target_rate,
        [MicrophoneSlot::Primary],
    );
    session.mark_started(dictation_input(MicrophoneSlot::Primary, "primary", "Wired"));
    session
}

fn dual_dictation_mix_session() -> DictationMixSession {
    let mut session = DictationMixSession::new(
        chrono::DateTime::<Utc>::UNIX_EPOCH,
        1_000,
        [MicrophoneSlot::Primary, MicrophoneSlot::Secondary],
    );
    session.mark_started(dictation_input(MicrophoneSlot::Primary, "primary", "Wired"));
    session.mark_started(dictation_input(
        MicrophoneSlot::Secondary,
        "secondary",
        "Wireless",
    ));
    session
}

fn push_dictation_pair(
    session: &mut DictationMixSession,
    chunk: i64,
    primary: f32,
    secondary: f32,
) -> Vec<f32> {
    let end = chunk * 100;
    let mut output = flatten_samples(session.push_frame(
        MicrophoneSlot::Primary,
        1_000,
        frame(vec![primary; 100], 100, end),
    ));
    output.extend(flatten_samples(session.push_frame(
        MicrophoneSlot::Secondary,
        1_000,
        frame(vec![secondary; 100], 100, end),
    )));
    output
}

#[test]
fn dictation_mix_session_switches_to_the_sustained_cleaner_microphone() {
    let mut session = dual_dictation_mix_session();
    let mut output = Vec::new();
    for chunk in 1..=4 {
        output.extend(push_dictation_pair(&mut session, chunk, 0.36, 0.08));
    }
    for chunk in 5..=7 {
        output.extend(push_dictation_pair(&mut session, chunk, 0.08, 0.36));
    }
    output.extend(flatten_samples(session.finish()));

    let tail = &output[output.len() - 20..];
    assert!(tail.iter().copied().sum::<f32>() / tail.len() as f32 > 0.34);
    assert!(session.finish().is_empty(), "finish must be idempotent");
}

#[test]
fn dictation_mix_session_continues_after_one_slot_fails() {
    let mut session = dual_dictation_mix_session();
    session.push_frame(
        MicrophoneSlot::Primary,
        1_000,
        frame(vec![0.6; 100], 100, 100),
    );
    let drained = session.mark_failed(
        MicrophoneSlot::Secondary,
        "secondary disconnected".to_string(),
    );

    assert_eq!(flatten_samples(drained), vec![0.6; 100]);
    assert_eq!(session.active_slots(), vec![MicrophoneSlot::Primary]);
}

#[test]
fn dictation_mix_session_resamples_each_slot_to_the_target_rate() {
    let mut session = primary_dictation_mix_session(16_000);
    let output = session.push_frame(
        MicrophoneSlot::Primary,
        48_000,
        frame(vec![0.2; 4_800], 100, 100),
    );
    assert_eq!(flatten_samples(output).len(), 1_600);
}

#[test]
fn dictation_mix_session_orders_actual_inputs_by_slot() {
    let mut session = DictationMixSession::new(
        chrono::DateTime::<Utc>::UNIX_EPOCH,
        1_000,
        [MicrophoneSlot::Primary, MicrophoneSlot::Secondary],
    );
    session.mark_started(dictation_input(
        MicrophoneSlot::Secondary,
        "secondary",
        "Wireless",
    ));
    session.mark_started(dictation_input(MicrophoneSlot::Primary, "primary", "Wired"));

    assert_eq!(
        session
            .started_inputs()
            .iter()
            .map(|input| input.device.id.as_str())
            .collect::<Vec<_>>(),
        vec!["primary", "secondary"],
    );
}

#[test]
fn dictation_mix_session_rejects_a_clipped_secondary() {
    let mut session = dual_dictation_mix_session();
    let mut output = Vec::new();
    for chunk in 1..=8 {
        let end = chunk * 100;
        output.extend(flatten_samples(
            session.push_frame(
                MicrophoneSlot::Primary,
                1_000,
                frame(
                    (0..100)
                        .map(|index| if index % 2 == 0 { 0.32 } else { -0.32 })
                        .collect(),
                    100,
                    end,
                ),
            ),
        ));
        output.extend(flatten_samples(
            session.push_frame(
                MicrophoneSlot::Secondary,
                1_000,
                frame(
                    (0..100)
                        .map(|index| if index % 2 == 0 { 1.0 } else { -1.0 })
                        .collect(),
                    100,
                    end,
                ),
            ),
        ));
    }
    output.extend(flatten_samples(session.finish()));
    let tail_peak = output[output.len() - 200..]
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(tail_peak < 0.4);
}

#[test]
fn dictation_mix_session_orders_failures_and_exhausts_active_slots() {
    let mut session = dual_dictation_mix_session();
    session.mark_failed(
        MicrophoneSlot::Secondary,
        "secondary disconnected".to_string(),
    );
    session.mark_failed(MicrophoneSlot::Primary, "primary disconnected".to_string());

    assert!(session.active_slots().is_empty());
    assert_eq!(
        session
            .failures()
            .into_iter()
            .map(|failure| (failure.slot, failure.error))
            .collect::<Vec<_>>(),
        vec![
            (MicrophoneSlot::Primary, "primary disconnected".to_string()),
            (
                MicrophoneSlot::Secondary,
                "secondary disconnected".to_string(),
            ),
        ],
    );
}

impl AudioFrameProcessor for ScalingProcessor {
    fn process(&mut self, mut frame: AudioFrame) -> Vec<AudioFrame> {
        for sample in &mut frame.samples {
            *sample *= self.0;
        }
        vec![frame]
    }

    fn reset(&mut self) {}
}

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

struct UsageTranscriber;

impl Transcriber for UsageTranscriber {
    fn transcribe(&self, _request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        unreachable!("execution path should preserve usage metadata")
    }

    fn transcribe_execution(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionExecution, TranscriptionError> {
        Ok(TranscriptionExecution {
            text: "wake note".to_string(),
            speaker_turns: Vec::new(),
            requested_model_id: request.model_id.to_string(),
            effective_model_id: request.model_id.to_string(),
            fallback_from_model_id: None,
            usage: Some(TranscriptionUsage {
                provider: Some("OpenAI".to_string()),
                audio_duration_ms: 1_000,
                provider_cost_usd: Some(0.0001),
            }),
            issue: None,
        })
    }
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
        DictationAction::Cancel
    );

    runtime.finish();
    assert_eq!(runtime.stage(), DictationStage::Idle);
}

#[test]
fn dictation_records_processed_microphone_samples() {
    let mut runtime = DictationRuntime::new(FakeInput {
        frames: vec![AudioFrame {
            samples: vec![0.2, -0.2],
            duration_ms: 10,
            captured_at: Utc::now(),
        }],
    });
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::StartRecording
    );
    runtime
        .start_recording_with_processor(
            dictation_input(MicrophoneSlot::Primary, "fake", "Fake microphone"),
            AudioInputConfig {
                device_id: "fake".to_string(),
                sample_rate: Some(48_000),
                label_hint: None,
                core_audio_uid: None,
            },
            ScalingProcessor(2.0),
            |_| {},
        )
        .expect("processed dictation starts");
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::StopAndTranscribe
    );

    assert_eq!(
        runtime.stop_recording().expect("recording stops").samples,
        vec![0.4, -0.4]
    );
}

#[test]
fn cancelling_recording_releases_capture_and_allows_the_next_dictation() {
    let mut runtime = DictationRuntime::new(FakeInput { frames: Vec::new() });
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::StartRecording
    );
    let first_id = runtime
        .start_recording(AudioInputConfig {
            device_id: "fake".to_string(),
            sample_rate: Some(16_000),
            label_hint: None,
            core_audio_uid: None,
        })
        .expect("first capture starts");

    assert!(runtime.cancel_active());
    assert_eq!(runtime.stage(), DictationStage::Idle);
    assert!(!runtime.is_operation_active(first_id));

    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Pressed),
        DictationAction::StartRecording
    );
    let second_id = runtime
        .start_recording(AudioInputConfig {
            device_id: "fake".to_string(),
            sample_rate: Some(16_000),
            label_hint: None,
            core_audio_uid: None,
        })
        .expect("second capture starts");
    assert!(second_id > first_id);
}

#[test]
fn cancelling_transcription_invalidates_late_completion() {
    let mut runtime = DictationRuntime::new(FakeInput { frames: Vec::new() });
    runtime.handle_shortcut_event(DictationShortcutEvent::Pressed);
    let operation_id = runtime
        .start_recording(AudioInputConfig {
            device_id: "fake".to_string(),
            sample_rate: Some(16_000),
            label_hint: None,
            core_audio_uid: None,
        })
        .expect("capture starts");
    assert_eq!(
        runtime.handle_shortcut_event(DictationShortcutEvent::Released),
        DictationAction::StopAndTranscribe
    );
    runtime.stop_recording().expect("capture stops");

    assert!(runtime.is_operation_active(operation_id));
    assert!(runtime.cancel_active());
    assert!(!runtime.finish_if_active(operation_id));
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
            core_audio_uid: None,
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
                core_audio_uid: None,
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
            core_audio_uid: None,
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
    let started_at = Utc::now();
    let request = Arc::new(Mutex::new(None));
    let transcriber = FakeTranscriber {
        request: request.clone(),
        text: "  hello world  ".to_string(),
    };
    let recording = wakenote::dictation::DictationRecording {
        samples: vec![0.1; 48_000],
        sample_rate: 48_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![dictation_input(MicrophoneSlot::Primary, "primary", "Wired")],
    };

    let text = transcribe_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        &DictionaryContext::default(),
        transcriber,
    )
    .expect("transcription succeeds");
    assert_eq!(text.as_deref(), Some("  hello world  "));

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
fn dictation_transcription_applies_the_shared_dictionary() {
    let started_at = Utc::now();
    let dictionary = DictionaryContext::compile(
        true,
        &[DictionaryEntry {
            id: "wake".into(),
            term: "WakeNote".into(),
            aliases: vec!["wake note".into()],
            enabled: true,
        }],
    );
    let recording = DictationRecording {
        samples: vec![0.1; 16_000],
        sample_rate: 16_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![dictation_input(MicrophoneSlot::Primary, "primary", "Wired")],
    };
    let transcriber = OrderedFakeTranscriber {
        events: Arc::new(Mutex::new(Vec::new())),
        text: "wake note".into(),
    };

    let text = transcribe_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        &dictionary,
        transcriber,
    )
    .expect("transcription succeeds");

    assert_eq!(text.as_deref(), Some("WakeNote"));
}

#[test]
fn dictation_transcription_preserves_usage_while_applying_the_dictionary() {
    let started_at = Utc::now();
    let dictionary = DictionaryContext::compile(
        true,
        &[DictionaryEntry {
            id: "wake".into(),
            term: "WakeNote".into(),
            aliases: vec!["wake note".into()],
            enabled: true,
        }],
    );
    let recording = DictationRecording {
        samples: vec![0.1; 16_000],
        sample_rate: 16_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![dictation_input(MicrophoneSlot::Primary, "primary", "Wired")],
    };

    let execution = transcribe_dictation_recording_execution(
        &recording,
        "openai-gpt-transcribe",
        TranscriptionLanguage::Auto,
        &dictionary,
        UsageTranscriber,
    )
    .expect("transcription succeeds")
    .expect("speech result");

    assert_eq!(execution.text, "WakeNote");
    assert_eq!(execution.usage.expect("usage").audio_duration_ms, 1_000);
}

#[test]
fn dictation_types_only_after_transcription_returns() {
    let started_at = Utc::now();
    let events = Arc::new(Mutex::new(Vec::new()));
    let transcriber = OrderedFakeTranscriber {
        events: events.clone(),
        text: "  hello  ".to_string(),
    };
    let sink_events = events.clone();
    let recording = DictationRecording {
        samples: vec![0.1; 48_000],
        sample_rate: 48_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![dictation_input(MicrophoneSlot::Primary, "primary", "Wired")],
    };

    let outcome = transcribe_and_type_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        &DictionaryContext::default(),
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

    assert_eq!(
        outcome,
        DictationProcessOutcome::Typed("  hello  ".to_string())
    );
    assert_eq!(
        *events.lock().expect("events"),
        vec!["transcribed".to_string(), "typed:  hello  ".to_string()]
    );
}

#[test]
fn quiet_dictation_never_calls_the_text_sink() {
    let started_at = Utc::now();
    let transcriber = OrderedFakeTranscriber {
        events: Arc::new(Mutex::new(Vec::new())),
        text: "unused".to_string(),
    };
    let recording = DictationRecording {
        samples: vec![0.0; 48_000],
        sample_rate: 48_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![dictation_input(MicrophoneSlot::Primary, "primary", "Wired")],
    };

    let outcome = transcribe_and_type_dictation_recording(
        &recording,
        "whisper-medium",
        TranscriptionLanguage::Auto,
        &DictionaryContext::default(),
        transcriber,
        |_| panic!("quiet audio must not type"),
    )
    .expect("quiet recording is handled");

    assert_eq!(outcome, DictationProcessOutcome::NoSpeech);
}

#[test]
fn dictation_archive_persists_audio_and_recoverable_transcript_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let started_at = Utc::now();
    let recording = DictationRecording {
        samples: vec![0.1; 16_000],
        sample_rate: 16_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![dictation_input(
            MicrophoneSlot::Primary,
            "input-0-boya-cm40",
            "BOYA CM40",
        )],
    };
    let settings = AppSettings {
        save_root: tmp.path().to_string_lossy().into_owned(),
        audio_format: AudioFormat::Wav,
        selected_model: "qwen3-asr-0.6b".to_string(),
        dictation_model: "whisper-small".to_string(),
        ..AppSettings::default()
    };

    let chunk = archive_dictation_recording(&recording, &settings, "0.260730.1")
        .expect("dictation archive");

    assert!(chunk.audio_path.exists());
    assert!(
        chunk
            .audio_path
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|stem| stem.ends_with("-dictation"))
    );
    let metadata_bytes = std::fs::read(&chunk.metadata_path).expect("metadata");
    let metadata_json: serde_json::Value =
        serde_json::from_slice(&metadata_bytes).expect("valid metadata JSON");
    assert_eq!(metadata_json["source"], "microphone");
    assert_eq!(metadata_json["source_label"], "dictation");
    let metadata: ChunkMetadata = serde_json::from_slice(&metadata_bytes).expect("valid metadata");
    assert_eq!(metadata.source, ChunkSource::Microphone);
    assert_eq!(metadata.source_label.as_deref(), Some("dictation"));
    assert_eq!(metadata.device_id, "input-0-boya-cm40");
    assert_eq!(metadata.device_name, "BOYA CM40");
    assert_eq!(metadata.microphone_inputs.len(), 1);
    assert_eq!(metadata.model_id, "whisper-small");
    assert_eq!(metadata.transcription_status, TranscriptionStatus::Queued);
    assert_eq!(metadata.started_at, recording.started_at);
    assert_eq!(metadata.ended_at, recording.ended_at);
}

#[test]
fn dictation_archive_persists_only_started_microphones_in_slot_order() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let started_at = Utc::now();
    let recording = DictationRecording {
        samples: vec![0.1; 16_000],
        sample_rate: 16_000,
        started_at,
        ended_at: started_at + chrono::Duration::seconds(1),
        microphone_inputs: vec![
            dictation_input(MicrophoneSlot::Secondary, "secondary", "Wireless"),
            dictation_input(MicrophoneSlot::Primary, "primary", "Wired"),
        ],
    };
    let settings = AppSettings {
        save_root: tmp.path().to_string_lossy().into_owned(),
        audio_format: AudioFormat::Wav,
        capture_microphones: vec![
            dictation_input(MicrophoneSlot::Primary, "primary", "Wired").device,
            dictation_input(MicrophoneSlot::Secondary, "secondary", "Wireless").device,
        ],
        ..AppSettings::default()
    };

    let chunk = archive_dictation_recording(&recording, &settings, "0.260809.1")
        .expect("dictation archive");
    let metadata: ChunkMetadata =
        serde_json::from_slice(&std::fs::read(chunk.metadata_path).expect("metadata"))
            .expect("valid metadata");

    assert_eq!(metadata.source_label.as_deref(), Some("dictation"));
    assert_eq!(metadata.microphone_slot, None);
    assert_eq!(metadata.device_id, "primary+secondary");
    assert_eq!(metadata.device_name, "Wired + Wireless");
    assert_eq!(
        metadata
            .microphone_inputs
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["primary", "secondary"],
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
