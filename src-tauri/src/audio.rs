use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputDevice {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateConfig {
    pub threshold_dbfs: f32,
    pub attack_ms: u64,
    pub release_ms: u64,
    pub pre_roll_ms: u64,
    pub post_roll_ms: u64,
    pub min_chunk_ms: u64,
    pub max_chunk_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LevelSnapshot {
    pub current_dbfs: f32,
    pub peak_dbfs: f32,
    pub noise_floor_dbfs: f32,
    pub suggested_threshold_dbfs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision {
    Idle,
    Start {
        started_at_ms: u64,
    },
    Recording,
    End {
        ended_at_ms: u64,
    },
    Rollover {
        ended_at_ms: u64,
        next_started_at_ms: u64,
    },
}

#[derive(Debug)]
pub struct SpeechGate {
    config: GateConfig,
    above_since_ms: Option<u64>,
    below_since_ms: Option<u64>,
    chunk_started_at_ms: Option<u64>,
    post_roll_deadline_ms: Option<u64>,
    recording: bool,
}

#[derive(Debug, Clone)]
pub struct LevelMonitor {
    recent_dbfs: VecDeque<f32>,
    snapshot: LevelSnapshot,
}

impl Default for LevelSnapshot {
    fn default() -> Self {
        Self {
            current_dbfs: -120.0,
            peak_dbfs: -120.0,
            noise_floor_dbfs: -120.0,
            suggested_threshold_dbfs: -90.0,
        }
    }
}

impl Default for LevelMonitor {
    fn default() -> Self {
        Self {
            recent_dbfs: VecDeque::new(),
            snapshot: LevelSnapshot::default(),
        }
    }
}

pub fn dbfs_from_rms(rms: f32) -> f32 {
    if rms <= 0.0 {
        return -120.0;
    }

    (20.0 * rms.log10()).max(-120.0)
}

pub fn dbfs_from_samples(samples: &[f32]) -> f32 {
    dbfs_from_rms(rms_from_samples(samples))
}

pub fn list_input_devices() -> Vec<InputDevice> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let default_available = host.default_input_device().is_some();
    let Ok(inputs) = host.input_devices() else {
        return input_devices_from_labels(default_available, std::iter::empty::<String>());
    };
    let labels = inputs
        .enumerate()
        .map(|(index, device)| {
            device
                .name()
                .unwrap_or_else(|_| format!("Input Device {}", index + 1))
        })
        .collect::<Vec<_>>();

    input_devices_from_labels(default_available, labels)
}

pub fn input_devices_from_labels<I, S>(default_available: bool, labels: I) -> Vec<InputDevice>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut devices = vec![InputDevice {
        id: "default".to_string(),
        label: "System Default".to_string(),
        available: default_available,
    }];

    for (index, label) in labels.into_iter().enumerate() {
        let label = label.as_ref();
        devices.push(InputDevice {
            id: format!("input-{index}-{}", slugify_device_label(&label)),
            label: label.to_string(),
            available: true,
        });
    }

    devices
}

fn slugify_device_label(label: &str) -> String {
    let slug = label
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if character.is_whitespace() || character == '-' || character == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();
    slug.trim_matches('-').to_string()
}

impl LevelMonitor {
    pub fn observe_samples(&mut self, samples: &[f32]) -> LevelSnapshot {
        let current_dbfs = dbfs_from_samples(samples);
        self.recent_dbfs.push_back(current_dbfs);
        while self.recent_dbfs.len() > 300 {
            self.recent_dbfs.pop_front();
        }

        let noise_floor_dbfs = percentile(&self.recent_dbfs, 0.2).unwrap_or(-120.0);
        self.snapshot = LevelSnapshot {
            current_dbfs,
            peak_dbfs: self.snapshot.peak_dbfs.max(current_dbfs),
            noise_floor_dbfs,
            suggested_threshold_dbfs: (noise_floor_dbfs + 12.0).clamp(-90.0, -10.0),
        };
        self.snapshot
    }

    pub fn snapshot(&self) -> LevelSnapshot {
        self.snapshot
    }
}

impl SpeechGate {
    pub fn new(config: GateConfig) -> Self {
        Self {
            config,
            above_since_ms: None,
            below_since_ms: None,
            chunk_started_at_ms: None,
            post_roll_deadline_ms: None,
            recording: false,
        }
    }

    pub fn observe(&mut self, dbfs: f32, time_ms: u64) -> GateDecision {
        self.observe_window(dbfs, time_ms, time_ms)
    }

    pub fn observe_frame(
        &mut self,
        dbfs: f32,
        frame_start_ms: u64,
        frame_end_ms: u64,
    ) -> GateDecision {
        self.observe_window(dbfs, frame_start_ms, frame_end_ms)
    }

    fn observe_window(&mut self, dbfs: f32, signal_start_ms: u64, time_ms: u64) -> GateDecision {
        let signal_start_ms = signal_start_ms.min(time_ms);
        let above_threshold = dbfs >= self.config.threshold_dbfs;

        if !self.recording {
            if !above_threshold {
                self.above_since_ms = None;
                return GateDecision::Idle;
            }

            let above_since_ms = *self.above_since_ms.get_or_insert(signal_start_ms);
            if time_ms.saturating_sub(above_since_ms) >= self.config.attack_ms {
                let started_at_ms = above_since_ms.saturating_sub(self.config.pre_roll_ms);
                self.recording = true;
                self.chunk_started_at_ms = Some(started_at_ms);
                self.below_since_ms = None;
                return GateDecision::Start { started_at_ms };
            }

            return GateDecision::Idle;
        }

        let chunk_started_at_ms = self.chunk_started_at_ms.unwrap_or(time_ms);
        if time_ms.saturating_sub(chunk_started_at_ms) >= self.config.max_chunk_ms {
            self.chunk_started_at_ms = Some(time_ms);
            self.below_since_ms = None;
            self.post_roll_deadline_ms = None;
            self.above_since_ms = if above_threshold { Some(time_ms) } else { None };
            return GateDecision::Rollover {
                ended_at_ms: time_ms,
                next_started_at_ms: time_ms,
            };
        }

        if above_threshold {
            self.below_since_ms = None;
            self.post_roll_deadline_ms = None;
            return GateDecision::Recording;
        }

        let below_since_ms = *self.below_since_ms.get_or_insert(signal_start_ms);
        let release_elapsed = time_ms.saturating_sub(below_since_ms) >= self.config.release_ms;
        let minimum_elapsed =
            time_ms.saturating_sub(chunk_started_at_ms) >= self.config.min_chunk_ms;

        if release_elapsed && minimum_elapsed {
            let post_roll_deadline_ms = *self
                .post_roll_deadline_ms
                .get_or_insert_with(|| time_ms.saturating_add(self.config.post_roll_ms));
            if time_ms < post_roll_deadline_ms {
                return GateDecision::Recording;
            }

            let ended_at_ms = time_ms;
            self.recording = false;
            self.above_since_ms = None;
            self.below_since_ms = None;
            self.chunk_started_at_ms = None;
            self.post_roll_deadline_ms = None;
            return GateDecision::End { ended_at_ms };
        }

        GateDecision::Recording
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }
}

fn rms_from_samples(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let energy = samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
    energy.sqrt()
}

/// Threshold above which a frame is considered to contain real audio rather
/// than digital silence. A working ADC delivers some ambient noise floor; only
/// a wedged stream produces exactly-zero RMS (which clamps to -120 dBFS).
pub const MIC_NONZERO_DBFS: f32 = -119.0;

/// Tunables for [`MicHealthMonitor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicHealthConfig {
    /// Grace period after capture start during which no recovery action fires.
    pub startup_grace: Duration,
    /// How long the stream must stay digitally silent before a recovery action.
    pub stall_threshold: Duration,
    /// Settle window after a recovery action before re-evaluating.
    pub recovery_cooldown: Duration,
    /// Total recovery attempts allowed before giving up.
    pub max_attempts: u32,
}

impl Default for MicHealthConfig {
    fn default() -> Self {
        Self {
            startup_grace: Duration::from_millis(1_500),
            stall_threshold: Duration::from_millis(3_000),
            recovery_cooldown: Duration::from_millis(3_500),
            max_attempts: 2,
        }
    }
}

/// Action requested by [`MicHealthMonitor::tick`] when the input stream looks
/// wedged. Higher layers translate these into capture restart / device-fallback
/// orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicHealthAction {
    /// Restart the live stream with the currently-selected device.
    RestartCurrent { reason: String },
    /// Restart the live stream forced onto the system default device.
    FallbackToDefault { reason: String },
    /// Recovery attempts exhausted; surface a microphone warning.
    GiveUp { reason: String },
}

/// Result of a tick — either no action needed or a recovery action to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicHealthVerdict {
    Healthy,
    AwaitingFirstFrame,
    InCooldown,
    Action(MicHealthAction),
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MicHealthState {
    Idle,
    Watching,
    Cooldown,
    Exhausted,
}

#[derive(Debug, Clone)]
pub struct MicHealthMonitor {
    config: MicHealthConfig,
    state: MicHealthState,
    capture_started_at: Option<Instant>,
    last_nonzero_at: Option<Instant>,
    cooldown_until: Option<Instant>,
    attempts: u32,
    currently_default_device: bool,
}

impl Default for MicHealthMonitor {
    fn default() -> Self {
        Self::with_config(MicHealthConfig::default())
    }
}

impl MicHealthMonitor {
    pub fn with_config(config: MicHealthConfig) -> Self {
        Self {
            config,
            state: MicHealthState::Idle,
            capture_started_at: None,
            last_nonzero_at: None,
            cooldown_until: None,
            attempts: 0,
            currently_default_device: true,
        }
    }

    /// Reset and start watching a new capture session.
    ///
    /// `using_default_device` controls whether the next recovery action escalates
    /// straight to `GiveUp` (we are already on the default device) or first tries
    /// `FallbackToDefault` after `RestartCurrent` fails.
    pub fn capture_started(&mut self, now: Instant, using_default_device: bool) {
        self.state = MicHealthState::Watching;
        self.capture_started_at = Some(now);
        self.last_nonzero_at = None;
        self.cooldown_until = None;
        self.attempts = 0;
        self.currently_default_device = using_default_device;
    }

    /// Mark the session as stopped; subsequent ticks return [`MicHealthVerdict::Healthy`].
    pub fn capture_stopped(&mut self) {
        self.state = MicHealthState::Idle;
        self.capture_started_at = None;
        self.last_nonzero_at = None;
        self.cooldown_until = None;
    }

    /// Record a single observed frame.
    pub fn observe_frame(&mut self, dbfs: f32, now: Instant) {
        if matches!(self.state, MicHealthState::Idle) {
            return;
        }
        if dbfs > MIC_NONZERO_DBFS {
            self.last_nonzero_at = Some(now);
            // Healthy frames imply the stream is alive; clear cooldown so the
            // next stall can be acted on immediately.
            if matches!(self.state, MicHealthState::Cooldown) {
                if let Some(cooldown_until) = self.cooldown_until {
                    if now >= cooldown_until {
                        self.state = MicHealthState::Watching;
                        self.cooldown_until = None;
                        self.attempts = 0;
                    }
                }
            }
        }
    }

    /// Inspect the current state and emit a recovery verdict.
    pub fn tick(&mut self, now: Instant) -> MicHealthVerdict {
        match self.state {
            MicHealthState::Idle => MicHealthVerdict::Healthy,
            MicHealthState::Exhausted => MicHealthVerdict::Exhausted,
            MicHealthState::Cooldown => {
                let Some(cooldown_until) = self.cooldown_until else {
                    self.state = MicHealthState::Watching;
                    return MicHealthVerdict::Healthy;
                };
                if now < cooldown_until {
                    return MicHealthVerdict::InCooldown;
                }
                self.state = MicHealthState::Watching;
                self.cooldown_until = None;
                self.evaluate(now)
            }
            MicHealthState::Watching => self.evaluate(now),
        }
    }

    fn evaluate(&mut self, now: Instant) -> MicHealthVerdict {
        let Some(started_at) = self.capture_started_at else {
            return MicHealthVerdict::Healthy;
        };

        let session_age = now.saturating_duration_since(started_at);
        if session_age < self.config.startup_grace {
            return MicHealthVerdict::AwaitingFirstFrame;
        }

        let silence_anchor = self.last_nonzero_at.unwrap_or(started_at);
        let silence_age = now.saturating_duration_since(silence_anchor);
        if silence_age < self.config.stall_threshold {
            return MicHealthVerdict::Healthy;
        }

        if self.attempts >= self.config.max_attempts {
            self.state = MicHealthState::Exhausted;
            return MicHealthVerdict::Action(MicHealthAction::GiveUp {
                reason: stall_reason(silence_age),
            });
        }

        self.attempts = self.attempts.saturating_add(1);
        self.cooldown_until = Some(now + self.config.recovery_cooldown);
        self.state = MicHealthState::Cooldown;

        let reason = stall_reason(silence_age);
        if !self.currently_default_device && self.attempts >= 2 {
            MicHealthVerdict::Action(MicHealthAction::FallbackToDefault { reason })
        } else if self.currently_default_device && self.attempts >= self.config.max_attempts {
            // Already on default; can't escalate to a different device — surface the warning.
            self.state = MicHealthState::Exhausted;
            MicHealthVerdict::Action(MicHealthAction::GiveUp { reason })
        } else {
            MicHealthVerdict::Action(MicHealthAction::RestartCurrent { reason })
        }
    }
}

fn stall_reason(silence_age: Duration) -> String {
    let secs = silence_age.as_secs_f32();
    format!("no audio input detected for {secs:.1}s")
}

fn percentile(samples: &VecDeque<f32>, percentile: f32) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }

    let mut sorted = samples.iter().copied().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index = ((sorted.len() - 1) as f32 * percentile).round() as usize;
    sorted.get(index).copied()
}

#[cfg(test)]
mod mic_health_tests {
    use super::*;
    use std::time::Duration;

    fn fast_config() -> MicHealthConfig {
        MicHealthConfig {
            startup_grace: Duration::from_millis(100),
            stall_threshold: Duration::from_millis(300),
            recovery_cooldown: Duration::from_millis(200),
            max_attempts: 2,
        }
    }

    #[test]
    fn idle_when_capture_not_started() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let now = Instant::now();
        assert_eq!(monitor.tick(now), MicHealthVerdict::Healthy);
    }

    #[test]
    fn awaiting_first_frame_during_grace_period() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, false);
        let verdict = monitor.tick(started + Duration::from_millis(50));
        assert_eq!(verdict, MicHealthVerdict::AwaitingFirstFrame);
    }

    #[test]
    fn healthy_while_nonzero_frames_keep_arriving() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, false);
        for offset_ms in (50..=500).step_by(50) {
            monitor.observe_frame(-60.0, started + Duration::from_millis(offset_ms));
        }
        let verdict = monitor.tick(started + Duration::from_millis(500));
        assert_eq!(verdict, MicHealthVerdict::Healthy);
    }

    #[test]
    fn pinned_device_escalates_restart_then_fallback_then_giveup() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, false);

        // Only digitally-silent frames.
        for offset_ms in (0..=500).step_by(50) {
            monitor.observe_frame(-120.0, started + Duration::from_millis(offset_ms));
        }

        let first = monitor.tick(started + Duration::from_millis(500));
        assert!(matches!(
            first,
            MicHealthVerdict::Action(MicHealthAction::RestartCurrent { .. })
        ));

        let cooldown = monitor.tick(started + Duration::from_millis(550));
        assert_eq!(cooldown, MicHealthVerdict::InCooldown);

        let second = monitor.tick(started + Duration::from_millis(1_200));
        assert!(matches!(
            second,
            MicHealthVerdict::Action(MicHealthAction::FallbackToDefault { .. })
        ));

        let third = monitor.tick(started + Duration::from_millis(1_900));
        assert!(matches!(
            third,
            MicHealthVerdict::Action(MicHealthAction::GiveUp { .. })
        ));
        assert_eq!(
            monitor.tick(started + Duration::from_millis(2_500)),
            MicHealthVerdict::Exhausted
        );
    }

    #[test]
    fn default_device_skips_fallback_to_default_and_gives_up_on_second_wedge() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, true);

        for offset_ms in (0..=500).step_by(50) {
            monitor.observe_frame(-120.0, started + Duration::from_millis(offset_ms));
        }

        let first = monitor.tick(started + Duration::from_millis(500));
        assert!(matches!(
            first,
            MicHealthVerdict::Action(MicHealthAction::RestartCurrent { .. })
        ));

        let second = monitor.tick(started + Duration::from_millis(1_200));
        assert!(matches!(
            second,
            MicHealthVerdict::Action(MicHealthAction::GiveUp { .. })
        ));
    }

    #[test]
    fn nonzero_frame_during_cooldown_returns_to_healthy() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, false);

        for offset_ms in (0..=500).step_by(50) {
            monitor.observe_frame(-120.0, started + Duration::from_millis(offset_ms));
        }

        let first = monitor.tick(started + Duration::from_millis(500));
        assert!(matches!(
            first,
            MicHealthVerdict::Action(MicHealthAction::RestartCurrent { .. })
        ));

        // Cooldown elapses, then a healthy frame arrives.
        monitor.observe_frame(-65.0, started + Duration::from_millis(800));

        let after = monitor.tick(started + Duration::from_millis(900));
        assert_eq!(after, MicHealthVerdict::Healthy);
    }

    #[test]
    fn observe_frame_after_capture_stopped_is_a_no_op() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, false);
        monitor.capture_stopped();
        monitor.observe_frame(-60.0, started + Duration::from_millis(50));
        assert_eq!(
            monitor.tick(started + Duration::from_millis(500)),
            MicHealthVerdict::Healthy
        );
    }
}
