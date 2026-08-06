use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use sonora::config::{AdaptiveDigital, FixedDigital, GainController2};
use sonora::{AudioProcessing, Config, StreamConfig};

use crate::live_capture::AudioFrame;
use crate::microphone_level::{
    MicrophoneLevelError, MicrophoneLevelService, MicrophoneVolumeBackend,
};
use crate::settings::CaptureMicrophoneEntry;

pub trait AudioFrameProcessor: Send {
    fn process(&mut self, frame: AudioFrame) -> Vec<AudioFrame>;
    fn reset(&mut self);

    fn take_warning(&mut self) -> Option<String> {
        None
    }
}

#[derive(Debug)]
pub struct TenMillisecondPacketizer {
    sample_rate: u32,
    frame_samples: usize,
    pending: VecDeque<f32>,
    pending_start: Option<DateTime<Utc>>,
}

impl TenMillisecondPacketizer {
    pub fn new(sample_rate: u32) -> Self {
        let frame_samples = (sample_rate / 100).max(1) as usize;
        Self {
            sample_rate: sample_rate.max(1),
            frame_samples,
            pending: VecDeque::new(),
            pending_start: None,
        }
    }

    pub fn push(&mut self, frame: AudioFrame) -> Vec<AudioFrame> {
        if frame.samples.is_empty() {
            return Vec::new();
        }
        let frame_start = frame.captured_at - self.sample_duration(frame.samples.len());
        if let Some(pending_start) = self.pending_start {
            let expected_start = pending_start + self.sample_duration(self.pending.len());
            if (frame_start - expected_start).num_milliseconds().abs() > 20 {
                self.pending.clear();
                self.pending_start = None;
            }
        }
        if self.pending_start.is_none() {
            self.pending_start = Some(frame_start);
        }
        self.pending.extend(frame.samples);

        let mut output = Vec::new();
        while self.pending.len() >= self.frame_samples {
            let samples = self.pending.drain(..self.frame_samples).collect::<Vec<_>>();
            let start = self.pending_start.expect("pending samples have a start");
            let captured_at = start + self.sample_duration(self.frame_samples);
            self.pending_start = Some(captured_at);
            output.push(AudioFrame {
                samples,
                duration_ms: 10,
                captured_at,
            });
        }
        if self.pending.is_empty() {
            self.pending_start = None;
        }
        output
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.pending_start = None;
    }

    fn sample_duration(&self, samples: usize) -> TimeDelta {
        let micros = (samples as i128 * 1_000_000 / self.sample_rate as i128)
            .clamp(i64::MIN as i128, i64::MAX as i128) as i64;
        TimeDelta::microseconds(micros)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceLevelingMode {
    Disabled,
    Hybrid,
    DigitalOnly,
    PassThrough,
}

pub trait HardwareLevelControl: Send {
    fn current_percent(&self) -> Result<u8, MicrophoneLevelError>;
    fn set_percent(&mut self, volume_percent: u8) -> Result<u8, MicrophoneLevelError>;
}

pub struct ServiceHardwareLevelControl<B> {
    service: Arc<MicrophoneLevelService<B>>,
    microphone: CaptureMicrophoneEntry,
}

impl<B> ServiceHardwareLevelControl<B> {
    pub fn new(
        service: Arc<MicrophoneLevelService<B>>,
        microphone: CaptureMicrophoneEntry,
    ) -> Self {
        Self {
            service,
            microphone,
        }
    }
}

impl<B: MicrophoneVolumeBackend> HardwareLevelControl for ServiceHardwareLevelControl<B> {
    fn current_percent(&self) -> Result<u8, MicrophoneLevelError> {
        let level = self.service.level_for(&self.microphone)?;
        if !level.writable {
            return Err(MicrophoneLevelError::ReadOnly(
                self.microphone.label.clone(),
            ));
        }
        level
            .volume_percent
            .ok_or_else(|| MicrophoneLevelError::VolumeUnavailable(self.microphone.label.clone()))
    }

    fn set_percent(&mut self, volume_percent: u8) -> Result<u8, MicrophoneLevelError> {
        self.service
            .set_volume(&self.microphone, volume_percent)?
            .volume_percent
            .ok_or_else(|| MicrophoneLevelError::VolumeUnavailable(self.microphone.label.clone()))
    }
}

pub struct VoiceAwareMicrophoneProcessor<C: HardwareLevelControl> {
    sample_rate: u32,
    policy: VoiceLevelingPolicy,
    policy_generation: u64,
    enabled: bool,
    packetizer: TenMillisecondPacketizer,
    processing: Option<AudioProcessing>,
    hardware_control: Option<C>,
    hardware_percent: Option<u8>,
    last_hardware_write_at: Option<Duration>,
    processed_duration: Duration,
    terminal_fallback: bool,
    warning: Option<String>,
}

impl<C: HardwareLevelControl> VoiceAwareMicrophoneProcessor<C> {
    pub fn new(
        sample_rate: u32,
        policy: VoiceLevelingPolicy,
        mut hardware_control: Option<C>,
    ) -> Self {
        let snapshot = policy.snapshot();
        let (hardware_percent, warning) = match hardware_control.as_ref() {
            Some(control) => match control.current_percent() {
                Ok(percent) => (Some(percent.min(100)), None),
                Err(_) => {
                    hardware_control = None;
                    (
                        None,
                        Some(
                            "Voice-aware Auto Level could not read the system input volume; using digital processing only"
                                .to_string(),
                        ),
                    )
                }
            },
            None => (None, None),
        };
        Self {
            sample_rate,
            policy,
            policy_generation: snapshot.generation,
            enabled: snapshot.enabled,
            packetizer: TenMillisecondPacketizer::new(sample_rate),
            processing: snapshot
                .enabled
                .then(|| build_audio_processing(sample_rate, hardware_control.is_some())),
            hardware_control,
            hardware_percent,
            last_hardware_write_at: None,
            processed_duration: Duration::ZERO,
            terminal_fallback: false,
            warning,
        }
    }

    pub fn mode(&self) -> VoiceLevelingMode {
        if !self.enabled {
            VoiceLevelingMode::Disabled
        } else if self.terminal_fallback {
            VoiceLevelingMode::PassThrough
        } else if self.hardware_control.is_some() {
            VoiceLevelingMode::Hybrid
        } else {
            VoiceLevelingMode::DigitalOnly
        }
    }

    pub fn take_warning(&mut self) -> Option<String> {
        self.warning.take()
    }

    fn synchronize_policy(&mut self) {
        let snapshot = self.policy.snapshot();
        if snapshot.generation == self.policy_generation {
            return;
        }
        self.policy_generation = snapshot.generation;
        self.enabled = snapshot.enabled;
        self.reset_state();
    }

    fn reset_state(&mut self) {
        self.packetizer.reset();
        self.processing = self
            .enabled
            .then(|| build_audio_processing(self.sample_rate, self.hardware_control.is_some()));
        self.last_hardware_write_at = None;
        self.processed_duration = Duration::ZERO;
        self.terminal_fallback = false;
        self.warning = None;
    }

    fn apply_recommendation_at(&mut self, recommended_percent: u8, at: Duration) {
        let Some(current_percent) = self.hardware_percent else {
            return;
        };
        let recommended_percent = recommended_percent.min(100);
        if recommended_percent == current_percent {
            return;
        }
        let minimum_interval = if recommended_percent < current_percent {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(250)
        };
        let interval_elapsed = self
            .last_hardware_write_at
            .map_or(at >= minimum_interval, |last| {
                at.saturating_sub(last) >= minimum_interval
            });
        if !interval_elapsed {
            return;
        }

        let result = self
            .hardware_control
            .as_mut()
            .expect("hardware percent requires a controller")
            .set_percent(recommended_percent);
        match result {
            Ok(applied) => {
                self.hardware_percent = Some(applied.min(100));
                self.last_hardware_write_at = Some(at);
            }
            Err(_) => {
                self.hardware_control = None;
                self.hardware_percent = None;
                self.processing = Some(build_audio_processing(self.sample_rate, false));
                if self.warning.is_none() {
                    self.warning = Some(
                        "Voice-aware Auto Level could not adjust the system input volume; continuing with digital processing only"
                            .to_string(),
                    );
                }
            }
        }
    }
}

impl<C: HardwareLevelControl> AudioFrameProcessor for VoiceAwareMicrophoneProcessor<C> {
    fn process(&mut self, frame: AudioFrame) -> Vec<AudioFrame> {
        self.synchronize_policy();
        if !self.enabled || self.terminal_fallback {
            return vec![frame];
        }
        let original = frame.clone();
        let packets = self.packetizer.push(frame);
        let mut output = Vec::with_capacity(packets.len());
        for mut packet in packets {
            let Some(processing) = self.processing.as_mut() else {
                self.terminal_fallback = true;
                return vec![original];
            };
            if let Some(percent) = self.hardware_percent {
                processing.set_stream_analog_level(percent_to_analog_level(percent));
            }
            let source = packet.samples.clone();
            let mut processed = vec![0.0_f32; source.len()];
            let result = processing
                .process_capture_f32(&[source.as_slice()], &mut [processed.as_mut_slice()]);
            if result.is_err() {
                self.packetizer.reset();
                self.terminal_fallback = true;
                self.warning = Some(
                    "Voice-aware Auto Level is unavailable; using unprocessed audio".to_string(),
                );
                return vec![original];
            }
            packet.samples = processed
                .into_iter()
                .map(|sample| {
                    if sample.is_finite() {
                        sample.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
                .collect();
            self.processed_duration += Duration::from_millis(10);
            if self.hardware_control.is_some() {
                let recommendation =
                    analog_level_to_percent(processing.recommended_stream_analog_level());
                self.apply_recommendation_at(recommendation, self.processed_duration);
            }
            output.push(packet);
        }
        output
    }

    fn reset(&mut self) {
        self.reset_state();
    }

    fn take_warning(&mut self) -> Option<String> {
        VoiceAwareMicrophoneProcessor::take_warning(self)
    }
}

fn build_audio_processing(sample_rate: u32, input_volume_controller: bool) -> AudioProcessing {
    let config = Config {
        gain_controller2: Some(GainController2 {
            input_volume_controller,
            adaptive_digital: Some(AdaptiveDigital {
                headroom_db: 6.0,
                max_gain_db: 30.0,
                initial_gain_db: 0.0,
                max_gain_change_db_per_second: 6.0,
                max_output_noise_level_dbfs: -50.0,
            }),
            fixed_digital: FixedDigital { gain_db: 0.0 },
        }),
        ..Default::default()
    };
    AudioProcessing::builder()
        .config(config)
        .capture_config(StreamConfig::new(sample_rate, 1))
        .build()
}

fn percent_to_analog_level(percent: u8) -> i32 {
    ((u32::from(percent.min(100)) * 255 + 50) / 100) as i32
}

fn analog_level_to_percent(level: i32) -> u8 {
    let level = level.clamp(0, 255) as u32;
    ((level * 100 + 127) / 255) as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceLevelingPolicySnapshot {
    pub enabled: bool,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct VoiceLevelingPolicy {
    enabled: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl VoiceLevelingPolicy {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn snapshot(&self) -> VoiceLevelingPolicySnapshot {
        VoiceLevelingPolicySnapshot {
            enabled: self.enabled.load(Ordering::Acquire),
            generation: self.generation.load(Ordering::Acquire),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::AcqRel) != enabled {
            self.generation.fetch_add(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, TimeZone, Utc};
    use std::path::Path;
    use std::sync::Mutex;

    fn frame_at(sample_count: usize, sample_rate: u32, captured_at_ms: i64) -> AudioFrame {
        AudioFrame {
            samples: vec![0.1; sample_count],
            duration_ms: (sample_count as u64).saturating_mul(1_000) / sample_rate as u64,
            captured_at: Utc.timestamp_millis_opt(captured_at_ms).unwrap(),
        }
    }

    #[derive(Debug)]
    struct NoHardwareControl;

    impl HardwareLevelControl for NoHardwareControl {
        fn current_percent(&self) -> Result<u8, MicrophoneLevelError> {
            Err(MicrophoneLevelError::UnsupportedPlatform)
        }

        fn set_percent(&mut self, _volume_percent: u8) -> Result<u8, MicrophoneLevelError> {
            Err(MicrophoneLevelError::UnsupportedPlatform)
        }
    }

    #[derive(Clone)]
    struct RecordingHardwareControl {
        state: Arc<Mutex<(u8, Vec<u8>, bool)>>,
    }

    impl RecordingHardwareControl {
        fn new(current: u8) -> Self {
            Self {
                state: Arc::new(Mutex::new((current, Vec::new(), false))),
            }
        }

        fn failing(current: u8) -> Self {
            Self {
                state: Arc::new(Mutex::new((current, Vec::new(), true))),
            }
        }

        fn writes(&self) -> Vec<u8> {
            self.state.lock().unwrap().1.clone()
        }
    }

    impl HardwareLevelControl for RecordingHardwareControl {
        fn current_percent(&self) -> Result<u8, MicrophoneLevelError> {
            Ok(self.state.lock().unwrap().0)
        }

        fn set_percent(&mut self, volume_percent: u8) -> Result<u8, MicrophoneLevelError> {
            let mut state = self.state.lock().unwrap();
            if state.2 {
                return Err(MicrophoneLevelError::Backend("test failure".into()));
            }
            state.0 = volume_percent;
            state.1.push(volume_percent);
            Ok(volume_percent)
        }
    }

    fn process_in_10ms_frames<C: HardwareLevelControl>(
        processor: &mut VoiceAwareMicrophoneProcessor<C>,
        samples: &[f32],
        sample_rate: u32,
    ) -> Vec<f32> {
        let frame_samples = (sample_rate / 100) as usize;
        let mut output = Vec::new();
        for (index, chunk) in samples.chunks(frame_samples).enumerate() {
            let frame = AudioFrame {
                samples: chunk.to_vec(),
                duration_ms: 10,
                captured_at: Utc.timestamp_millis_opt(((index + 1) * 10) as i64).unwrap(),
            };
            output.extend(
                processor
                    .process(frame)
                    .into_iter()
                    .flat_map(|frame| frame.samples),
            );
        }
        output
    }

    fn quiet_voice_fixture_repeated(repetitions: usize) -> Vec<f32> {
        let mut reader = hound::WavReader::open(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quiet-voice-48khz.wav"),
        )
        .unwrap();
        let source = reader
            .samples::<i16>()
            .map(|sample| sample.unwrap() as f32 / i16::MAX as f32)
            .collect::<Vec<_>>();
        source.repeat(repetitions)
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples
            .iter()
            .map(|sample| f64::from(*sample) * f64::from(*sample))
            .sum::<f64>()
            / samples.len().max(1) as f64)
            .sqrt() as f32
    }

    #[test]
    fn policy_generation_changes_only_when_enabled_value_changes() {
        let policy = VoiceLevelingPolicy::new(true);
        let first = policy.snapshot();
        policy.set_enabled(true);
        assert_eq!(policy.snapshot(), first);
        policy.set_enabled(false);
        assert_eq!(policy.snapshot().generation, first.generation + 1);
        assert!(!policy.snapshot().enabled);
    }

    #[test]
    fn packetizer_emits_exact_ten_millisecond_frames_without_loss() {
        let mut packetizer = TenMillisecondPacketizer::new(48_000);
        let first = packetizer.push(frame_at(700, 48_000, 15));
        let second = packetizer.push(frame_at(260, 48_000, 20));
        let output = [first, second].concat();

        assert_eq!(output.len(), 2);
        assert_eq!(output[0].samples.len(), 480);
        assert_eq!(output[1].samples.len(), 480);
        assert_eq!(output[0].duration_ms, 10);
        assert_eq!(
            output[1].captured_at - output[0].captured_at,
            TimeDelta::milliseconds(10)
        );
        assert_eq!(
            output
                .iter()
                .map(|frame| frame.samples.len())
                .sum::<usize>(),
            960
        );
    }

    #[test]
    fn packetizer_supports_44khz_and_resets_remainder_after_a_gap() {
        let mut packetizer = TenMillisecondPacketizer::new(44_100);
        assert!(packetizer.push(frame_at(220, 44_100, 5)).is_empty());
        let output = packetizer.push(frame_at(441, 44_100, 500));

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].samples.len(), 441);
        assert!(output[0].samples.iter().all(|sample| *sample == 0.1));
    }

    #[test]
    fn quiet_voice_is_raised_after_vad_warm_up() {
        let input = quiet_voice_fixture_repeated(3);
        let policy = VoiceLevelingPolicy::new(true);
        let mut processor =
            VoiceAwareMicrophoneProcessor::<NoHardwareControl>::new(48_000, policy, None);
        let output = process_in_10ms_frames(&mut processor, &input, 48_000);
        let warm_up = 48_000;

        assert!(rms(&output[warm_up..]) > rms(&input[warm_up..]) * 1.5);
        assert!(
            output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
        );
    }

    #[test]
    fn silence_is_not_pumped_and_loud_input_is_bounded() {
        let policy = VoiceLevelingPolicy::new(true);
        let mut processor =
            VoiceAwareMicrophoneProcessor::<NoHardwareControl>::new(48_000, policy, None);
        let silence = vec![0.0; 48_000 * 3];
        let output = process_in_10ms_frames(&mut processor, &silence, 48_000);
        assert!(rms(&output) < 0.0001);

        let loud = vec![0.99; 48_000 * 3];
        let output = process_in_10ms_frames(&mut processor, &loud, 48_000);
        assert!(
            output
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
        );
    }

    #[test]
    fn clipping_reduction_is_applied_before_a_rate_limited_increase() {
        let control = RecordingHardwareControl::new(80);
        let mut processor = VoiceAwareMicrophoneProcessor::new(
            48_000,
            VoiceLevelingPolicy::new(true),
            Some(control.clone()),
        );

        processor.apply_recommendation_at(60, Duration::from_millis(100));
        processor.apply_recommendation_at(85, Duration::from_millis(150));

        assert_eq!(control.writes(), vec![60]);
    }

    #[test]
    fn hardware_write_failure_keeps_digital_processing_alive() {
        let control = RecordingHardwareControl::failing(80);
        let mut processor = VoiceAwareMicrophoneProcessor::new(
            48_000,
            VoiceLevelingPolicy::new(true),
            Some(control),
        );
        processor.apply_recommendation_at(60, Duration::from_millis(100));
        let output = processor.process(frame_at(480, 48_000, 110));

        assert!(output[0].samples.iter().all(|sample| sample.is_finite()));
        assert_eq!(processor.mode(), VoiceLevelingMode::DigitalOnly);
        assert!(processor.take_warning().is_some());
        assert!(processor.take_warning().is_none());
    }

    #[test]
    fn disabled_policy_is_bit_exact_and_live_policy_changes_reset_processing() {
        let policy = VoiceLevelingPolicy::new(false);
        let mut processor =
            VoiceAwareMicrophoneProcessor::<NoHardwareControl>::new(48_000, policy.clone(), None);
        let input = frame_at(137, 48_000, 10);
        assert_eq!(processor.process(input.clone()), vec![input.clone()]);

        policy.set_enabled(true);
        assert!(processor.process(input.clone()).is_empty());
        policy.set_enabled(false);
        assert_eq!(processor.process(input.clone()), vec![input]);
    }
}
