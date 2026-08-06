use std::sync::mpsc;
use std::time::Duration;

use wakenote::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, LiveCaptureError,
};
use wakenote::multi_capture::MultiCaptureRuntime;
use wakenote::settings::MicrophoneSlot;
use wakenote::voice_leveling::AudioFrameProcessor;

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
