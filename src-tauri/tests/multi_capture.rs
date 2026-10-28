use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use wakenote::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, LiveCaptureError,
};
use wakenote::multi_capture::MultiCaptureRuntime;
use wakenote::settings::MicrophoneSlot;
use wakenote::voice_leveling::AudioFrameProcessor;

thread_local! {
    static CAPTURE_STARTS: Cell<u32> = const { Cell::new(0) };
    static CAPTURE_CALLBACK: RefCell<Option<Arc<dyn Fn(AudioFrame) + Send + Sync>>> =
        const { RefCell::new(None) };
    static CAPTURE_FAIL: Cell<bool> = const { Cell::new(false) };
}

fn reset_capture_script() {
    CAPTURE_STARTS.with(|starts| starts.set(0));
    CAPTURE_CALLBACK.with(|callback| *callback.borrow_mut() = None);
    CAPTURE_FAIL.with(|fail| fail.set(false));
}

fn capture_starts() -> u32 {
    CAPTURE_STARTS.with(|starts| starts.get())
}

fn emit_capture_frame(sample: f32) {
    let callback = CAPTURE_CALLBACK.with(|slot| slot.borrow().clone());
    let callback = callback.expect("capture callback");
    callback(AudioFrame {
        samples: vec![sample],
        duration_ms: 20,
        captured_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
    });
}

struct CountingHandle;

impl AudioStreamHandle for CountingHandle {
    fn runtime_error(&self) -> Option<String> {
        CAPTURE_FAIL.with(|fail| fail.get().then(|| "device stalled".to_string()))
    }
}

#[derive(Default)]
struct CountingInput;

impl AudioInputBackend for CountingInput {
    fn start(
        &mut self,
        _config: AudioInputConfig,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        CAPTURE_STARTS.with(|starts| starts.set(starts.get() + 1));
        CAPTURE_CALLBACK.with(|slot| *slot.borrow_mut() = Some(on_frame));
        Ok(Box::new(CountingHandle))
    }
}

fn device_config(device_id: &str, uid: &str) -> AudioInputConfig {
    AudioInputConfig {
        device_id: device_id.to_string(),
        sample_rate: Some(48_000),
        label_hint: Some(device_id.to_string()),
        core_audio_uid: Some(uid.to_string()),
    }
}

struct StatefulScalingProcessor {
    multiplier: f32,
    frames_seen: usize,
}

impl AudioFrameProcessor for StatefulScalingProcessor {
    fn process(&mut self, mut frame: AudioFrame) -> Vec<AudioFrame> {
        self.frames_seen += 1;
        for sample in &mut frame.samples {
            *sample *= self.multiplier * self.frames_seen as f32;
        }
        vec![frame]
    }

    fn reset(&mut self) {
        self.frames_seen = 0;
    }
}

#[derive(Default)]
struct ScriptedInput;

struct ScriptedHandle {
    runtime_error: Option<String>,
}

impl AudioStreamHandle for ScriptedHandle {
    fn runtime_error(&self) -> Option<String> {
        self.runtime_error.clone()
    }
}

impl AudioInputBackend for ScriptedInput {
    fn start(
        &mut self,
        config: AudioInputConfig,
        on_frame: std::sync::Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        on_frame(AudioFrame {
            samples: vec![if config.device_id == "primary" {
                0.1
            } else {
                0.2
            }],
            duration_ms: 20,
            captured_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
        });
        Ok(Box::new(ScriptedHandle {
            runtime_error: (config.device_id == "failing-secondary")
                .then(|| "secondary disconnected".to_string()),
        }))
    }
}

fn config(device_id: &str) -> AudioInputConfig {
    AudioInputConfig {
        device_id: device_id.to_string(),
        sample_rate: None,
        label_hint: None,
        core_audio_uid: None,
    }
}

#[test]
fn two_microphone_slots_start_dispatch_and_stop_independently() {
    let mut runtime = MultiCaptureRuntime::<ScriptedInput>::default();
    let (primary_tx, primary_rx) = mpsc::channel();
    let (secondary_tx, secondary_rx) = mpsc::channel();

    runtime
        .start_slot(MicrophoneSlot::Primary, config("primary"), move |frame| {
            primary_tx.send(frame).expect("primary frame");
        })
        .expect("start primary");
    runtime
        .start_slot(
            MicrophoneSlot::Secondary,
            config("secondary"),
            move |frame| {
                secondary_tx.send(frame).expect("secondary frame");
            },
        )
        .expect("start secondary");

    assert_eq!(
        runtime.active_slots(),
        vec![MicrophoneSlot::Primary, MicrophoneSlot::Secondary]
    );
    assert_eq!(
        primary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("primary delivered")
            .samples,
        vec![0.1]
    );
    assert_eq!(
        secondary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("secondary delivered")
            .samples,
        vec![0.2]
    );

    runtime.stop_slot(MicrophoneSlot::Secondary);
    assert!(runtime.is_running(MicrophoneSlot::Primary));
    assert!(!runtime.is_running(MicrophoneSlot::Secondary));
}

#[test]
fn microphone_slots_keep_independent_processor_state() {
    let mut runtime = MultiCaptureRuntime::<ScriptedInput>::default();
    let (primary_tx, primary_rx) = mpsc::channel();
    let (secondary_tx, secondary_rx) = mpsc::channel();

    runtime
        .start_slot_processed(
            MicrophoneSlot::Primary,
            config("primary"),
            StatefulScalingProcessor {
                multiplier: 2.0,
                frames_seen: 0,
            },
            move |frame| primary_tx.send(frame).expect("primary frame"),
        )
        .expect("start primary");
    runtime
        .start_slot_processed(
            MicrophoneSlot::Secondary,
            config("secondary"),
            StatefulScalingProcessor {
                multiplier: 3.0,
                frames_seen: 0,
            },
            move |frame| secondary_tx.send(frame).expect("secondary frame"),
        )
        .expect("start secondary");

    assert_eq!(
        primary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("primary processed")
            .samples,
        vec![0.2]
    );
    assert_eq!(
        secondary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("secondary processed")
            .samples,
        vec![0.6]
    );
}

#[test]
fn secondary_runtime_error_does_not_change_primary_diagnostics() {
    let mut runtime = MultiCaptureRuntime::<ScriptedInput>::default();
    runtime
        .start_slot(MicrophoneSlot::Primary, config("primary"), |_| {})
        .expect("start primary");
    runtime
        .start_slot(
            MicrophoneSlot::Secondary,
            config("failing-secondary"),
            |_| {},
        )
        .expect("start secondary");

    let diagnostics = runtime.diagnostics();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].slot, MicrophoneSlot::Primary);
    assert_eq!(diagnostics[0].runtime_error, None);
    assert_eq!(diagnostics[1].slot, MicrophoneSlot::Secondary);
    assert_eq!(
        diagnostics[1].runtime_error.as_deref(),
        Some("secondary disconnected")
    );
    assert!(runtime.is_running(MicrophoneSlot::Primary));
}

#[test]
fn same_physical_microphone_opens_once_and_fans_one_leveling_pass_to_both_slots() {
    reset_capture_script();
    let mut runtime = MultiCaptureRuntime::<CountingInput>::default();
    let (primary_tx, primary_rx) = mpsc::channel();
    let (secondary_tx, secondary_rx) = mpsc::channel();
    let config = device_config("studio", "uid-studio");

    runtime
        .start_slot_processed(
            MicrophoneSlot::Primary,
            config.clone(),
            StatefulScalingProcessor {
                multiplier: 2.0,
                frames_seen: 0,
            },
            move |frame| primary_tx.send(frame.samples).expect("primary frame"),
        )
        .expect("open primary");
    runtime
        .start_slot_processed(
            MicrophoneSlot::Secondary,
            config.clone(),
            StatefulScalingProcessor {
                multiplier: 3.0,
                frames_seen: 0,
            },
            move |frame| secondary_tx.send(frame.samples).expect("secondary frame"),
        )
        .expect("attach secondary");

    assert_eq!(capture_starts(), 1);
    assert_eq!(runtime.open_device_sample_rate(&config), Some(48_000));
    assert!(runtime.is_running(MicrophoneSlot::Primary));
    assert!(runtime.is_running(MicrophoneSlot::Secondary));
    assert_eq!(runtime.diagnostics().len(), 2);
    assert_eq!(runtime.total_dropped_frame_count(), 0);

    emit_capture_frame(0.25);
    assert_eq!(
        primary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("primary leveled"),
        vec![0.5]
    );
    assert_eq!(
        secondary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("secondary shares the primary pass"),
        vec![0.5]
    );

    runtime.stop_slot(MicrophoneSlot::Primary);
    assert!(!runtime.is_running(MicrophoneSlot::Primary));
    assert!(runtime.is_running(MicrophoneSlot::Secondary));
    emit_capture_frame(0.25);
    assert_eq!(
        secondary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("follower keeps the original pass"),
        vec![1.0]
    );
    assert!(primary_rx.try_recv().is_err());

    let (primary_again_tx, primary_again_rx) = mpsc::channel();
    runtime
        .start_slot_processed(
            MicrophoneSlot::Primary,
            config,
            StatefulScalingProcessor {
                multiplier: 3.0,
                frames_seen: 0,
            },
            move |frame| {
                primary_again_tx
                    .send(frame.samples)
                    .expect("reattached primary")
            },
        )
        .expect("reattach primary");
    assert_eq!(capture_starts(), 1);
    emit_capture_frame(0.25);
    assert_eq!(
        primary_again_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("reattached primary frame"),
        vec![1.5]
    );
    assert_eq!(
        secondary_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("shared third frame"),
        vec![1.5]
    );
}

#[test]
fn core_audio_uid_decides_whether_two_slots_share_one_open() {
    // (label, primary (device id, uid), secondary (device id, uid), expected opens)
    let cases = [
        (
            "same uid with different ids shares one open",
            ("default", "built-in"),
            ("internal", "built-in"),
            1,
        ),
        (
            "distinct microphones open independently",
            ("wired", "uid-wired"),
            ("wireless", "uid-wireless"),
            2,
        ),
    ];
    for (label, (primary_id, primary_uid), (secondary_id, secondary_uid), expected_opens) in cases {
        reset_capture_script();
        let mut runtime = MultiCaptureRuntime::<CountingInput>::default();
        runtime
            .start_slot(
                MicrophoneSlot::Primary,
                device_config(primary_id, primary_uid),
                |_| {},
            )
            .expect("start primary");
        runtime
            .start_slot(
                MicrophoneSlot::Secondary,
                device_config(secondary_id, secondary_uid),
                |_| {},
            )
            .expect("start secondary");
        assert_eq!(capture_starts(), expected_opens, "{label}");
        assert_eq!(
            runtime.active_slots(),
            vec![MicrophoneSlot::Primary, MicrophoneSlot::Secondary],
            "{label}"
        );
    }
}

#[test]
fn unhealthy_shared_microphone_is_reopened_instead_of_attached() {
    reset_capture_script();
    let mut runtime = MultiCaptureRuntime::<CountingInput>::default();
    let config = device_config("studio", "uid-studio");
    runtime
        .start_slot(MicrophoneSlot::Primary, config.clone(), |_| {})
        .expect("open studio");
    CAPTURE_FAIL.with(|fail| fail.set(true));
    runtime
        .start_slot(MicrophoneSlot::Secondary, config, |_| {})
        .expect("reopen stalled studio");
    assert_eq!(capture_starts(), 2);
    assert!(!runtime.is_running(MicrophoneSlot::Primary));
    assert!(runtime.is_running(MicrophoneSlot::Secondary));
}
