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

/// How long we run on a non-top-priority microphone before voluntarily
/// re-attempting the top-priority device. Used as the default for
/// [`MicHealthConfig::top_priority_recheck`].
pub const TOP_PRIORITY_RECHECK: Duration = Duration::from_secs(600);

/// Tunables for [`MicHealthMonitor`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicHealthConfig {
    /// Grace period after capture start during which no recovery action fires.
    pub startup_grace: Duration,
    /// How long the stream must stay digitally silent before the watchdog
    /// surfaces a UI warning. Does NOT trigger any device switch.
    pub stall_threshold: Duration,
    /// How long the cpal data callback can be silent (no frames at all) before
    /// the stream is declared wedged. Primary stall signal — catches the common
    /// failure mode where the audio thread stops firing but `is_running()`
    /// still reports true (the visible symptom: waveform freezes).
    pub heartbeat_threshold: Duration,
    /// Base settle window between recovery actions. Grows with
    /// `consecutive_failures` up to [`Self::max_recovery_cooldown`] so a
    /// wedged-everywhere system does not hammer cpal at full rate forever.
    pub recovery_cooldown: Duration,
    /// Hard ceiling on the per-attempt cooldown after repeated failures.
    pub max_recovery_cooldown: Duration,
    /// While running on a non-top-priority device, how long the watchdog waits
    /// before voluntarily re-attempting the top-priority device.
    pub top_priority_recheck: Duration,
}

impl Default for MicHealthConfig {
    fn default() -> Self {
        Self {
            startup_grace: Duration::from_millis(1_500),
            stall_threshold: Duration::from_secs(12),
            heartbeat_threshold: Duration::from_millis(1_000),
            recovery_cooldown: Duration::from_millis(3_500),
            max_recovery_cooldown: Duration::from_secs(30),
            top_priority_recheck: TOP_PRIORITY_RECHECK,
        }
    }
}

/// Action requested by [`MicHealthMonitor::tick`]. The watchdog translates a
/// `SwitchTo` into a stop+start of the live capture with the given device id.
/// Recovery is infinite: the monitor never gives up — it cycles forever
/// through the priority list, with growing cooldowns when every device fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicHealthAction {
    /// Stop the current stream and re-open with `device_id`. The id either
    /// names the next entry in the priority cycle (after a stall) or the top
    /// of the priority list (during a periodic upgrade attempt).
    SwitchTo { device_id: String, reason: String },
}

/// Result of a tick — either no action needed or a recovery action to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicHealthVerdict {
    Healthy,
    AwaitingFirstFrame,
    InCooldown,
    Action(MicHealthAction),
    /// Stream is alive (callbacks firing) but every frame has been digital
    /// silence for `since`. The watchdog does NOT switch devices — surfaced
    /// to the UI for the user to investigate manually.
    SilenceWarning { since: Duration },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MicHealthState {
    Idle,
    Watching,
    Cooldown,
    /// Live capture is not running but the user still wants recording on, so
    /// the watchdog must keep re-attempting starts. Set by `mark_start_failed`
    /// and cleared on the next successful `capture_started`.
    AwaitingRestart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StallKind {
    /// No data callback at all for `heartbeat_threshold`.
    Heartbeat,
}

#[derive(Debug, Clone)]
pub struct MicHealthMonitor {
    config: MicHealthConfig,
    state: MicHealthState,
    capture_started_at: Option<Instant>,
    last_frame_at: Option<Instant>,
    last_nonzero_at: Option<Instant>,
    cooldown_until: Option<Instant>,
    /// Ordered list of device ids the watchdog will cycle through. Position 0
    /// is the user's top choice; the monitor wraps around indefinitely.
    priority: Vec<String>,
    /// Index in `priority` of the device the current capture session opened.
    /// Saturates at `priority.len()` for devices not in the list (e.g.
    /// transient override targets).
    active_index: usize,
    /// Failures since the last non-silent frame was observed. Drives the
    /// growing cooldown between switch attempts — a wedged-everywhere system
    /// must not retry at full rate forever.
    consecutive_failures: u32,
    /// Wall-clock anchor for the periodic "try the top-priority device"
    /// upgrade attempt. Updated whenever we open or re-attempt `priority[0]`.
    last_top_priority_attempt_at: Option<Instant>,
    /// Reason latched by an out-of-band stall signal (cpal `StreamError`, OS
    /// device-change, sleep/wake). Consumed by the next non-cooldown `tick`,
    /// short-circuiting heartbeat/silence checks for immediate recovery.
    pending_recovery_reason: Option<String>,
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
            last_frame_at: None,
            last_nonzero_at: None,
            cooldown_until: None,
            priority: Vec::new(),
            active_index: 0,
            consecutive_failures: 0,
            last_top_priority_attempt_at: None,
            pending_recovery_reason: None,
        }
    }

    /// Reset and start watching a new capture session.
    ///
    /// `priority` is the user's ordered microphone preference (must be
    /// non-empty in practice — the watchdog seeds at least one entry).
    /// `active_index` is the position in that list of the device the live
    /// capture actually opened. Devices that fail to be located in the
    /// priority list saturate at `priority.len()` so the next stall still
    /// advances back into the list (mod len).
    pub fn capture_started(
        &mut self,
        now: Instant,
        priority: Vec<String>,
        active_index: usize,
    ) {
        self.state = MicHealthState::Watching;
        self.capture_started_at = Some(now);
        self.last_frame_at = None;
        self.last_nonzero_at = None;
        self.cooldown_until = None;
        self.pending_recovery_reason = None;

        self.priority = priority;
        self.active_index = if self.priority.is_empty() {
            0
        } else {
            active_index.min(self.priority.len())
        };
        // Anchor the top-priority retry timer whenever we (re)open priority[0]
        // so the 10-minute upgrade clock starts fresh.
        if self.active_index == 0 {
            self.last_top_priority_attempt_at = Some(now);
        }
    }

    /// Mark the session as stopped; subsequent ticks return [`MicHealthVerdict::Healthy`].
    pub fn capture_stopped(&mut self) {
        self.state = MicHealthState::Idle;
        self.capture_started_at = None;
        self.last_frame_at = None;
        self.last_nonzero_at = None;
        self.cooldown_until = None;
        self.pending_recovery_reason = None;
        self.consecutive_failures = 0;
    }

    /// Record that the most recent live-capture start attempt failed before
    /// the data callback ever fired. Transitions the monitor into
    /// `AwaitingRestart` so the watchdog keeps cycling through the priority
    /// list with a growing cooldown, rather than going idle.
    pub fn mark_start_failed(&mut self, now: Instant, reason: impl Into<String>) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.cooldown_until = Some(now + self.current_cooldown());
        self.state = MicHealthState::AwaitingRestart;
        self.pending_recovery_reason = Some(reason.into());
        self.last_frame_at = None;
        self.last_nonzero_at = None;

        // Advance to the next priority entry so the next emitted action
        // targets a different device — this keeps a wedged device from being
        // re-tried at full rate.
        let next_index = self.next_priority_index();
        self.active_index = next_index;
        if next_index == 0 {
            self.last_top_priority_attempt_at = Some(now);
        }
    }

    /// True iff the monitor is waiting to re-attempt a capture start (i.e.
    /// the previous start failed and no live session is running). Used by the
    /// watchdog to decide whether to keep ticking when capture is `None`.
    pub fn is_awaiting_restart(&self) -> bool {
        matches!(self.state, MicHealthState::AwaitingRestart)
    }

    /// Resync the priority list without touching health state. Used right
    /// before [`Self::mark_start_failed`] to make sure the monitor cycles
    /// over the user's current selection, including the case where the
    /// start failure happened before [`Self::capture_started`] could prime
    /// the list.
    pub fn refresh_priority(&mut self, priority: Vec<String>) {
        self.priority = priority;
        // If active_index is now out of range, saturate at the end so the
        // next cycle wraps back to position 0.
        if !self.priority.is_empty() && self.active_index > self.priority.len() {
            self.active_index = self.priority.len();
        }
    }

    /// Latch an out-of-band stall signal (e.g. a cpal `StreamError`). The next
    /// non-cooldown `tick` will produce a recovery action regardless of how
    /// long heartbeat/silence have been quiet, since the source already knows
    /// the stream is broken.
    pub fn request_recovery(&mut self, reason: impl Into<String>) {
        if matches!(self.state, MicHealthState::Idle) {
            return;
        }
        self.pending_recovery_reason = Some(reason.into());
    }

    /// Record a single observed frame.
    pub fn observe_frame(&mut self, dbfs: f32, now: Instant) {
        if matches!(self.state, MicHealthState::Idle) {
            return;
        }
        // Frame arrival alone is the primary heartbeat — even an all-zero
        // buffer means the audio thread is still firing.
        self.last_frame_at = Some(now);
        if dbfs > MIC_NONZERO_DBFS {
            self.last_nonzero_at = Some(now);
            // Real audio is flowing — drop the backoff counter so the next
            // failure starts at base cooldown, not the accumulated penalty
            // from earlier dead-device cycles.
            self.consecutive_failures = 0;
            // Healthy frames imply the stream is alive; clear cooldown so the
            // next stall can be acted on immediately.
            if matches!(self.state, MicHealthState::Cooldown) {
                if let Some(cooldown_until) = self.cooldown_until {
                    if now >= cooldown_until {
                        self.state = MicHealthState::Watching;
                        self.cooldown_until = None;
                    }
                }
            }
        }
    }

    /// Inspect the current state and emit a recovery verdict.
    pub fn tick(&mut self, now: Instant) -> MicHealthVerdict {
        match self.state {
            MicHealthState::Idle => MicHealthVerdict::Healthy,
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
            MicHealthState::AwaitingRestart => {
                if let Some(cooldown_until) = self.cooldown_until {
                    if now < cooldown_until {
                        return MicHealthVerdict::InCooldown;
                    }
                }
                self.emit_restart_action(now)
            }
        }
    }

    fn emit_restart_action(&mut self, now: Instant) -> MicHealthVerdict {
        let device_id = self
            .priority
            .get(self.active_index)
            .cloned()
            .unwrap_or_else(|| "default".to_string());
        let reason = self
            .pending_recovery_reason
            .take()
            .unwrap_or_else(|| "retrying microphone capture after start failure".to_string());
        // Re-arm the cooldown so a slow watchdog (or a test that keeps ticking
        // without acting) does not emit at full rate. The cooldown grows with
        // `consecutive_failures`; it is reset only on a successful
        // `capture_started` followed by a non-silent frame.
        self.cooldown_until = Some(now + self.current_cooldown());
        MicHealthVerdict::Action(MicHealthAction::SwitchTo { device_id, reason })
    }

    fn evaluate(&mut self, now: Instant) -> MicHealthVerdict {
        // External signals (cpal StreamError, OS device-change) are definitive
        // and bypass the startup-grace window: the source has already told us
        // the stream is dead.
        if let Some(reason) = self.pending_recovery_reason.take() {
            return self.escalate(now, reason);
        }

        let Some(started_at) = self.capture_started_at else {
            return MicHealthVerdict::Healthy;
        };

        let session_age = now.saturating_duration_since(started_at);
        if session_age < self.config.startup_grace {
            return MicHealthVerdict::AwaitingFirstFrame;
        }

        // Heartbeat is the only signal that triggers an automatic restart —
        // it means cpal stopped firing the data callback (CoreAudio wedge,
        // device hot-swap, sleep/wake).
        let heartbeat_anchor = self.last_frame_at.unwrap_or(started_at);
        let heartbeat_age = now.saturating_duration_since(heartbeat_anchor);
        if heartbeat_age >= self.config.heartbeat_threshold {
            return self.escalate(now, stall_reason(StallKind::Heartbeat, heartbeat_age));
        }

        // Digital silence is NOT a failure: it could be a quiet user, hardware
        // mute, OS noise gate. Report it as a warning so the UI can prompt the
        // user, but don't switch devices.
        let silence_anchor = self.last_nonzero_at.unwrap_or(started_at);
        let silence_age = now.saturating_duration_since(silence_anchor);
        if silence_age >= self.config.stall_threshold {
            return MicHealthVerdict::SilenceWarning { since: silence_age };
        }

        // Audio is flowing — but if we are running on a non-top-priority
        // device, periodically attempt to re-acquire the top device.
        if self.should_attempt_top_priority_upgrade(now) {
            let device_id = self.priority[0].clone();
            self.last_top_priority_attempt_at = Some(now);
            return MicHealthVerdict::Action(MicHealthAction::SwitchTo {
                device_id,
                reason: format!(
                    "retrying top-priority microphone after {:.0}s on a fallback device",
                    self.config.top_priority_recheck.as_secs_f32()
                ),
            });
        }

        MicHealthVerdict::Healthy
    }

    fn should_attempt_top_priority_upgrade(&self, now: Instant) -> bool {
        if self.priority.len() <= 1 || self.active_index == 0 {
            return false;
        }
        let anchor = self
            .last_top_priority_attempt_at
            .or(self.capture_started_at)
            .unwrap_or(now);
        now.saturating_duration_since(anchor) >= self.config.top_priority_recheck
    }

    fn current_cooldown(&self) -> Duration {
        // Linear-then-exponential backoff: failures 1..=4 double the base
        // cooldown, beyond that we sit at the configured ceiling. Resets
        // every time a non-silent frame arrives (see [`observe_frame`]).
        let exponent = self.consecutive_failures.saturating_sub(1).min(4);
        let multiplier = 1u32.checked_shl(exponent).unwrap_or(u32::MAX);
        let scaled = self
            .config
            .recovery_cooldown
            .saturating_mul(multiplier);
        scaled.min(self.config.max_recovery_cooldown)
    }

    fn next_priority_index(&self) -> usize {
        if self.priority.is_empty() {
            return 0;
        }
        // If active_index is out of range — running on a device not in the
        // priority list (e.g. a recovery override that resolved to an
        // off-list device) — the next switch returns to the top of the list.
        if self.active_index >= self.priority.len() {
            return 0;
        }
        (self.active_index + 1) % self.priority.len()
    }

    fn escalate(&mut self, now: Instant, reason: String) -> MicHealthVerdict {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.cooldown_until = Some(now + self.current_cooldown());
        self.state = MicHealthState::Cooldown;

        // Advance to the next entry in the priority cycle. Recovery is
        // infinite: even on a single-device list this just re-opens the same
        // device, separated by the growing cooldown so we don't hammer cpal.
        let next_index = self.next_priority_index();
        let device_id = self
            .priority
            .get(next_index)
            .cloned()
            .unwrap_or_else(|| "default".to_string());

        if next_index == 0 {
            self.last_top_priority_attempt_at = Some(now);
        }

        MicHealthVerdict::Action(MicHealthAction::SwitchTo { device_id, reason })
    }
}

fn stall_reason(kind: StallKind, age: Duration) -> String {
    let secs = age.as_secs_f32();
    match kind {
        StallKind::Heartbeat => format!("no audio frames received for {secs:.1}s"),
    }
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
            heartbeat_threshold: Duration::from_millis(200),
            recovery_cooldown: Duration::from_millis(200),
            max_recovery_cooldown: Duration::from_millis(800),
            top_priority_recheck: Duration::from_millis(500),
        }
    }

    fn priority(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn expect_switch_to(verdict: MicHealthVerdict) -> (String, String) {
        match verdict {
            MicHealthVerdict::Action(MicHealthAction::SwitchTo { device_id, reason }) => {
                (device_id, reason)
            }
            other => panic!("expected SwitchTo, got {other:?}"),
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
        monitor.capture_started(started, priority(&["default"]), 0);
        let verdict = monitor.tick(started + Duration::from_millis(50));
        assert_eq!(verdict, MicHealthVerdict::AwaitingFirstFrame);
    }

    #[test]
    fn healthy_while_nonzero_frames_keep_arriving() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["default"]), 0);
        for offset_ms in (50..=500).step_by(50) {
            monitor.observe_frame(-60.0, started + Duration::from_millis(offset_ms));
        }
        let verdict = monitor.tick(started + Duration::from_millis(500));
        assert_eq!(verdict, MicHealthVerdict::Healthy);
    }

    #[test]
    fn observe_frame_after_capture_stopped_is_a_no_op() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["default"]), 0);
        monitor.capture_stopped();
        monitor.observe_frame(-60.0, started + Duration::from_millis(50));
        assert_eq!(
            monitor.tick(started + Duration::from_millis(500)),
            MicHealthVerdict::Healthy
        );
    }

    #[test]
    fn digital_silence_alone_does_not_switch_devices() {
        // Regression guard for the 2026-05-26 silence-as-warning change: even when
        // the stream sits at digital silence forever, the watchdog must not emit
        // a SwitchTo action. Once silence has accumulated past stall_threshold we
        // also positively assert that SilenceWarning fires — otherwise a bug that
        // silently turns the silence path into Healthy would pass this guard.
        let mut monitor = MicHealthMonitor::with_config(MicHealthConfig {
            startup_grace: Duration::from_millis(100),
            stall_threshold: Duration::from_millis(300),
            heartbeat_threshold: Duration::from_secs(60),
            recovery_cooldown: Duration::from_millis(100),
            max_recovery_cooldown: Duration::from_secs(1),
            top_priority_recheck: Duration::from_secs(300),
        });
        let prio = vec!["mic-a".to_string(), "mic-b".to_string()];
        let start = Instant::now();
        let mut now = start;
        monitor.capture_started(now, prio, 0);

        let mut warning_seen_after_threshold = false;
        for _ in 0..40 {
            now += Duration::from_millis(120);
            monitor.observe_frame(-120.0, now);
            let verdict = monitor.tick(now);
            assert!(
                !matches!(verdict, MicHealthVerdict::Action(_)),
                "silence must not produce an action; got {verdict:?}"
            );
            if now.saturating_duration_since(start) >= Duration::from_millis(500) {
                // 100ms grace + 300ms stall_threshold = 400ms; give one tick of
                // slack and require SilenceWarning beyond that point.
                if matches!(verdict, MicHealthVerdict::SilenceWarning { .. }) {
                    warning_seen_after_threshold = true;
                } else {
                    assert!(
                        matches!(verdict, MicHealthVerdict::SilenceWarning { .. }),
                        "expected SilenceWarning after threshold, got {verdict:?}"
                    );
                }
            }
        }
        assert!(
            warning_seen_after_threshold,
            "test never observed a SilenceWarning verdict — regression"
        );
    }

    #[test]
    fn cycle_wraps_around_priority_list_indefinitely() {
        // Priority [A, B, C]. Each device wedges in turn. The cycle goes
        // A → B → C → A → B ... and never reaches an exhausted state.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let mut now = Instant::now();
        let prio = priority(&["mic-A", "mic-B", "mic-C"]);

        let mut expected_targets = ["mic-B", "mic-C", "mic-A", "mic-B"].into_iter();
        let mut active_index = 0usize;
        for _ in 0..4 {
            monitor.capture_started(now, prio.clone(), active_index);
            // Push past the grace window without ever delivering a frame so
            // the heartbeat path fires.
            now += Duration::from_millis(400);
            let (device_id, _) = expect_switch_to(monitor.tick(now));
            assert_eq!(device_id, expected_targets.next().unwrap());
            active_index = prio.iter().position(|id| id == &device_id).unwrap();
            // Let the cooldown elapse before the next capture_started.
            now += Duration::from_millis(900);
        }
    }

    #[test]
    fn single_device_priority_keeps_retrying_same_device() {
        // Only default in the priority list. We never give up — every stall
        // emits SwitchTo("default") with a growing cooldown, so a totally
        // dead system idles at the cooldown ceiling instead of failing hard.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["default"]), 0);

        let (first_id, _) = expect_switch_to(monitor.tick(started + Duration::from_millis(400)));
        assert_eq!(first_id, "default");

        monitor.capture_started(
            started + Duration::from_millis(1_500),
            priority(&["default"]),
            0,
        );
        let (second_id, _) = expect_switch_to(
            monitor.tick(started + Duration::from_millis(1_500 + 400)),
        );
        assert_eq!(second_id, "default");
    }

    #[test]
    fn cooldown_grows_with_consecutive_failures_then_caps() {
        // recovery_cooldown=200ms, max_recovery_cooldown=800ms. Failures 1..=4
        // produce cooldowns 200, 400, 800, 800 (capped).
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        let prio = priority(&["default"]);

        let cooldowns_ms = [200u64, 400, 800, 800];
        let mut now = started;
        for &expected_ms in &cooldowns_ms {
            monitor.capture_started(now, prio.clone(), 0);
            now += Duration::from_millis(400);
            // Stall: heartbeat path.
            assert!(matches!(
                monitor.tick(now),
                MicHealthVerdict::Action(MicHealthAction::SwitchTo { .. })
            ));
            // Still in cooldown one ms before expiry, healthy one ms after —
            // verifies the actual cooldown duration the monitor scheduled.
            assert_eq!(
                monitor.tick(now + Duration::from_millis(expected_ms - 1)),
                MicHealthVerdict::InCooldown,
                "expected still in cooldown for cooldown={expected_ms}ms",
            );
            now += Duration::from_millis(expected_ms + 50);
        }
    }

    #[test]
    fn healthy_frame_resets_consecutive_failures() {
        // After a stall escalation, a non-silent frame in the next session
        // must drop the backoff counter so a fresh stall cycles at the base
        // cooldown rather than the accumulated penalty.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        let prio = priority(&["mic-A", "default"]);

        // Session 1 on mic-A: heartbeat stall → switch to default.
        monitor.capture_started(started, prio.clone(), 0);
        assert!(matches!(
            monitor.tick(started + Duration::from_millis(400)),
            MicHealthVerdict::Action(MicHealthAction::SwitchTo { .. })
        ));

        // Session 2 on default: a healthy frame arrives.
        let second = started + Duration::from_millis(1_500);
        monitor.capture_started(second, prio.clone(), 1);
        monitor.observe_frame(-50.0, second + Duration::from_millis(50));

        // Session 3 on mic-A again: heartbeat stall — cooldown should be the
        // base recovery_cooldown (200ms), not the doubled 400ms a still-armed
        // failure counter would have produced.
        let third = second + Duration::from_millis(600);
        monitor.capture_started(third, prio.clone(), 0);
        let stall_at = third + Duration::from_millis(400);
        assert!(matches!(
            monitor.tick(stall_at),
            MicHealthVerdict::Action(MicHealthAction::SwitchTo { .. })
        ));
        assert_eq!(
            monitor.tick(stall_at + Duration::from_millis(199)),
            MicHealthVerdict::InCooldown
        );
        // 1ms after the base cooldown elapsed, we're back to Watching.
        assert!(!matches!(
            monitor.tick(stall_at + Duration::from_millis(201)),
            MicHealthVerdict::InCooldown
        ));
    }

    #[test]
    fn periodic_top_priority_recheck_upgrades_from_secondary_to_top() {
        // Sitting on priority[1] with healthy audio. After top_priority_recheck
        // elapses, the watchdog volunteers a SwitchTo(priority[0]) even
        // though nothing is wrong with the current stream.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        let prio = priority(&["mic-A", "default"]);
        monitor.capture_started(started, prio.clone(), 1);

        // Feed healthy audio continuously through the recheck window.
        for offset_ms in (50..=600).step_by(50) {
            monitor.observe_frame(-50.0, started + Duration::from_millis(offset_ms));
        }

        // Just under the recheck window: still healthy.
        assert_eq!(
            monitor.tick(started + Duration::from_millis(499)),
            MicHealthVerdict::Healthy
        );

        // Past the recheck window: voluntary upgrade attempt.
        let (device_id, reason) = expect_switch_to(monitor.tick(started + Duration::from_millis(600)));
        assert_eq!(device_id, "mic-A");
        assert!(
            reason.contains("retrying top-priority"),
            "expected upgrade reason, got {reason}"
        );
    }

    #[test]
    fn periodic_top_priority_recheck_does_not_fire_when_already_on_top() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["mic-A", "default"]), 0);

        for offset_ms in (50..=1_200).step_by(50) {
            monitor.observe_frame(-50.0, started + Duration::from_millis(offset_ms));
        }
        assert_eq!(
            monitor.tick(started + Duration::from_millis(1_200)),
            MicHealthVerdict::Healthy
        );
    }

    #[test]
    fn periodic_top_priority_recheck_does_not_fire_for_single_device_priority() {
        // No fallback in the list → nothing to upgrade FROM.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["default"]), 0);

        for offset_ms in (50..=1_200).step_by(50) {
            monitor.observe_frame(-50.0, started + Duration::from_millis(offset_ms));
        }
        assert_eq!(
            monitor.tick(started + Duration::from_millis(1_200)),
            MicHealthVerdict::Healthy
        );
    }

    #[test]
    fn heartbeat_triggers_when_data_callback_stops_firing() {
        // CoreAudio wedge: frames arrive normally, then the cpal data callback
        // stops entirely even though the higher-level capture state still
        // thinks it's running.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["mic-A", "default"]), 0);

        for offset_ms in (0..=400).step_by(50) {
            monitor.observe_frame(-50.0, started + Duration::from_millis(offset_ms));
        }
        assert_eq!(
            monitor.tick(started + Duration::from_millis(400)),
            MicHealthVerdict::Healthy
        );

        let (device_id, reason) =
            expect_switch_to(monitor.tick(started + Duration::from_millis(650)));
        assert_eq!(device_id, "default");
        assert!(
            reason.contains("no audio frames received"),
            "expected heartbeat reason, got {reason}"
        );
    }

    #[test]
    fn heartbeat_fires_before_silence_when_both_could_apply() {
        // Heartbeat threshold (200ms) is shorter than silence threshold (300ms),
        // so a complete callback stop wins the race.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["mic-A", "default"]), 0);

        monitor.observe_frame(-50.0, started + Duration::from_millis(120));

        let (_, reason) = expect_switch_to(monitor.tick(started + Duration::from_millis(370)));
        assert!(
            reason.contains("no audio frames received"),
            "expected heartbeat reason, got {reason}"
        );
    }

    #[test]
    fn silence_path_emits_warning_when_callbacks_keep_arriving_but_are_all_zero() {
        // Callbacks arrive (heartbeat healthy) but every frame is digital silence.
        // The watchdog must emit SilenceWarning, not a SwitchTo action.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["mic-A", "default"]), 0);

        monitor.observe_frame(-50.0, started + Duration::from_millis(50));
        for offset_ms in (100..=500).step_by(50) {
            monitor.observe_frame(-120.0, started + Duration::from_millis(offset_ms));
        }

        let verdict = monitor.tick(started + Duration::from_millis(500));
        assert!(
            matches!(verdict, MicHealthVerdict::SilenceWarning { .. }),
            "expected SilenceWarning, got {verdict:?}"
        );
    }

    #[test]
    fn request_recovery_during_grace_period_fires_immediately() {
        // cpal can fire a StreamError during the startup grace window
        // (e.g. unsupported sample-format negotiation). The external signal
        // must override the grace gate.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["mic-A", "default"]), 0);

        monitor.request_recovery("cpal init failed");
        let (device_id, reason) =
            expect_switch_to(monitor.tick(started + Duration::from_millis(10)));
        assert_eq!(device_id, "default");
        assert_eq!(reason, "cpal init failed");
    }

    #[test]
    fn request_recovery_when_idle_is_a_no_op() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        monitor.request_recovery("ignored");
        assert_eq!(monitor.tick(Instant::now()), MicHealthVerdict::Healthy);
    }

    #[test]
    fn request_recovery_during_cooldown_is_consumed_when_cooldown_elapses() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, priority(&["mic-A", "default"]), 0);

        monitor.observe_frame(-50.0, started + Duration::from_millis(120));
        let (first_id, _) = expect_switch_to(monitor.tick(started + Duration::from_millis(370)));
        assert_eq!(first_id, "default");

        // Cooldown of 200ms, so we sit in InCooldown at 450ms.
        assert_eq!(
            monitor.tick(started + Duration::from_millis(450)),
            MicHealthVerdict::InCooldown
        );

        monitor.request_recovery("device disconnected");

        // After cooldown elapses, the latched reason wins. We have not
        // started a new capture session, so active_index is still 0 and the
        // next cycle target is "default" again.
        let (second_id, reason) =
            expect_switch_to(monitor.tick(started + Duration::from_millis(700)));
        assert_eq!(second_id, "default");
        assert_eq!(reason, "device disconnected");
    }

    #[test]
    fn out_of_priority_active_index_wraps_to_first_entry_on_next_stall() {
        // active_index == priority.len() is the "we are running on a device
        // that isn't in the priority list" sentinel. The next switch must
        // bring us back to position 0.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        let prio = priority(&["mic-A", "mic-B"]);
        monitor.capture_started(started, prio.clone(), prio.len());

        let (device_id, _) = expect_switch_to(monitor.tick(started + Duration::from_millis(400)));
        assert_eq!(device_id, "mic-A");
    }

    #[test]
    fn mark_start_failed_keeps_watchdog_in_retry_mode_after_cooldown() {
        // The live capture start itself failed (e.g. cpal returned an error),
        // so the backend has no active session — but we still want recording
        // and the watchdog must keep trying instead of going Idle.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        let prio = priority(&["mic-A", "default"]);
        monitor.capture_started(started, prio.clone(), 0);
        monitor.mark_start_failed(started, "cpal failed to open mic-A");

        assert!(monitor.is_awaiting_restart());
        // One ms before the base cooldown expires we are still gated.
        assert_eq!(
            monitor.tick(started + Duration::from_millis(199)),
            MicHealthVerdict::InCooldown
        );
        // One ms after, the watchdog volunteers the next priority entry.
        let (device_id, reason) =
            expect_switch_to(monitor.tick(started + Duration::from_millis(201)));
        assert_eq!(device_id, "default");
        assert_eq!(reason, "cpal failed to open mic-A");
    }

    #[test]
    fn consecutive_start_failures_cycle_through_priority_with_growing_cooldown() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let mut now = Instant::now();
        let prio = priority(&["mic-A", "mic-B", "mic-C"]);
        monitor.capture_started(now, prio.clone(), 0);

        // Each failure advances the cycle by one entry and applies the next
        // cooldown step (200 → 400 → 800ms in the fast config).
        let expected = [("mic-B", 200u64), ("mic-C", 400), ("mic-A", 800)];
        for (target, cooldown_ms) in expected {
            monitor.mark_start_failed(now, format!("failed to open {target}"));
            assert_eq!(
                monitor.tick(now + Duration::from_millis(cooldown_ms - 1)),
                MicHealthVerdict::InCooldown,
                "expected cooldown gate at {cooldown_ms}ms for {target}",
            );
            let (device_id, _) =
                expect_switch_to(monitor.tick(now + Duration::from_millis(cooldown_ms + 1)));
            assert_eq!(device_id, target);
            now += Duration::from_millis(cooldown_ms + 5);
        }
    }

    #[test]
    fn successful_capture_started_clears_awaiting_restart() {
        // After a start failure the monitor is in AwaitingRestart. When the
        // next start succeeds, capture_started must reset the state so we go
        // back to normal heartbeat watching.
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        let prio = priority(&["mic-A", "default"]);
        monitor.capture_started(started, prio.clone(), 0);
        monitor.mark_start_failed(started, "failure");
        assert!(monitor.is_awaiting_restart());

        // The watchdog called start_live_capture_runtime and it succeeded.
        let resumed = started + Duration::from_millis(500);
        monitor.capture_started(resumed, prio.clone(), 1);
        assert!(!monitor.is_awaiting_restart());
        // No frames yet — but we are inside the grace window.
        assert_eq!(
            monitor.tick(resumed + Duration::from_millis(10)),
            MicHealthVerdict::AwaitingFirstFrame
        );
    }

    #[test]
    fn mark_start_failed_without_priority_falls_back_to_default() {
        let mut monitor = MicHealthMonitor::with_config(fast_config());
        let started = Instant::now();
        monitor.capture_started(started, Vec::new(), 0);
        monitor.mark_start_failed(started, "no priority configured");

        let (device_id, _) =
            expect_switch_to(monitor.tick(started + Duration::from_millis(300)));
        assert_eq!(device_id, "default");
    }

    #[test]
    fn silence_emits_warning_without_switching() {
        // Stream is running on priority[0], frames keep arriving but every sample
        // is digital silence. Watchdog must emit SilenceWarning, NOT a SwitchTo.
        let mut monitor = MicHealthMonitor::with_config(MicHealthConfig {
            startup_grace: Duration::from_millis(100),
            stall_threshold: Duration::from_millis(500),
            heartbeat_threshold: Duration::from_millis(5_000),
            recovery_cooldown: Duration::from_millis(100),
            max_recovery_cooldown: Duration::from_secs(1),
            top_priority_recheck: Duration::from_secs(300),
        });
        let prio = vec!["mic-a".to_string(), "mic-b".to_string()];
        let mut now = Instant::now();
        monitor.capture_started(now, prio.clone(), 0);

        // Past startup grace, deliver frames that are all digital silence.
        for _ in 0..6 {
            now += Duration::from_millis(150);
            monitor.observe_frame(-120.0, now);
        }

        let verdict = monitor.tick(now);
        match verdict {
            MicHealthVerdict::SilenceWarning { since } => {
                assert!(since >= Duration::from_millis(500), "since={since:?}");
            }
            other => panic!("expected SilenceWarning, got {other:?}"),
        }
    }

    #[test]
    fn silence_warning_clears_when_nonzero_frame_arrives() {
        let mut monitor = MicHealthMonitor::with_config(MicHealthConfig {
            startup_grace: Duration::from_millis(100),
            stall_threshold: Duration::from_millis(500),
            heartbeat_threshold: Duration::from_millis(5_000),
            recovery_cooldown: Duration::from_millis(100),
            max_recovery_cooldown: Duration::from_secs(1),
            top_priority_recheck: Duration::from_secs(300),
        });
        let prio = vec!["mic-a".to_string()];
        let mut now = Instant::now();
        monitor.capture_started(now, prio, 0);

        for _ in 0..6 {
            now += Duration::from_millis(150);
            monitor.observe_frame(-120.0, now);
        }
        assert!(matches!(
            monitor.tick(now),
            MicHealthVerdict::SilenceWarning { .. }
        ));

        // A real audio frame arrives — next tick should be Healthy.
        now += Duration::from_millis(150);
        monitor.observe_frame(-40.0, now);
        let verdict = monitor.tick(now);
        assert!(
            matches!(verdict, MicHealthVerdict::Healthy),
            "expected Healthy after non-silent frame, got {verdict:?}"
        );
    }
}
