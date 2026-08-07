use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, Duration, Utc};

use crate::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, LiveCaptureError, LiveCaptureRuntime,
};
use crate::settings::MicrophoneSlot;
use crate::voice_leveling::AudioFrameProcessor;

pub const MICROPHONE_MIX_REORDER_MS: u64 = 100;
pub const MICROPHONE_MIX_BUFFER_MS: u64 = 220;
const DUPLICATE_ANALYSIS_RATE_HZ: u32 = 2_000;
const DUPLICATE_ANALYSIS_INTERVAL_MS: u64 = 50;
const DUPLICATE_ANALYSIS_MIN_HISTORY_MS: u64 = 80;
const DUPLICATE_ANALYSIS_MAX_HISTORY_MS: u64 = 500;
const DUPLICATE_ANALYSIS_MAX_DELAY_MS: u64 = 200;
const DUPLICATE_ANALYSIS_MIN_OVERLAP_MS: u64 = 60;
const DUPLICATE_MIN_RMS: f32 = 0.004;
const DUPLICATE_CORRELATION_THRESHOLD: f64 = 0.78;
const DUPLICATE_RELEASE_THRESHOLD: f64 = 0.55;
const ALIGNMENT_BLEND_CORRELATION_THRESHOLD: f64 = 0.93;
const ALIGNMENT_CONFIDENCE_WINDOWS: u8 = 3;
const ALIGNMENT_TRACKING_RADIUS_MS: u64 = 20;
const DUPLICATE_RELEASE_WINDOWS: u8 = 2;
const DUPLICATE_CROSSFADE_MS: u64 = 20;
const ALIGNMENT_ESTIMATE_HISTORY: usize = 80;
const ALIGNMENT_MAX_DRIFT_PPM: f64 = 2_500.0;
const ALIGNMENT_PROMINENCE_THRESHOLD: f64 = 0.012;
const PRIORITY_CROSSFADE_MS: u64 = 50;
const PRIORITY_SWITCH_MARGIN: f32 = 0.05;
const PRIORITY_SWITCH_WINDOWS: u8 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MicrophoneMixMode {
    #[default]
    Merge,
    Priority,
}

#[derive(Debug, Clone, Copy, Default)]
struct PendingMixSample {
    primary: Option<f32>,
    secondary: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
struct PreparedMixSample {
    primary: Option<f32>,
    secondary: Option<f32>,
    aligned_secondary: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
struct AlignmentState {
    lag_samples: f64,
    drift_ppm: f64,
    correlation: f64,
    confident: bool,
    secondary_gain: f32,
    secondary_polarity: f32,
    observed_at_sample: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MicrophoneAlignmentSnapshot {
    pub lag_samples: f64,
    pub drift_ppm: f64,
    pub correlation: f64,
    pub confident: bool,
    pub secondary_gain: f32,
    pub secondary_polarity: f32,
}

#[derive(Debug, Default)]
struct DuplicateSignalTracker {
    primary: VecDeque<f32>,
    secondary: VecDeque<f32>,
    samples_since_analysis: usize,
    confident_windows: u8,
    uncorrelated_windows: u8,
    alignment: AlignmentState,
    lag_observations: VecDeque<(i64, f64)>,
    drift_anchor: Option<(i64, f64)>,
}

#[derive(Debug, Default)]
struct PrioritySignalTracker {
    selected_slot: Option<MicrophoneSlot>,
    candidate_slot: Option<MicrophoneSlot>,
    candidate_windows: u8,
}

impl PrioritySignalTracker {
    fn observe(&mut self, pending: &[PreparedMixSample]) -> Option<MicrophoneSlot> {
        let primary = slot_quality(pending.iter().filter_map(|sample| sample.primary));
        let secondary = slot_quality(pending.iter().filter_map(|sample| sample.secondary));
        let best = match (primary.is_finite(), secondary.is_finite()) {
            (false, false) => return self.selected_slot,
            (true, false) => MicrophoneSlot::Primary,
            (false, true) => MicrophoneSlot::Secondary,
            (true, true) if secondary > primary => MicrophoneSlot::Secondary,
            (true, true) => MicrophoneSlot::Primary,
        };
        let selected = self.selected_slot.get_or_insert(best);
        if *selected == best {
            self.candidate_slot = None;
            self.candidate_windows = 0;
            return Some(*selected);
        }

        let (selected_quality, best_quality) = match (*selected, best) {
            (MicrophoneSlot::Primary, MicrophoneSlot::Secondary) => (primary, secondary),
            (MicrophoneSlot::Secondary, MicrophoneSlot::Primary) => (secondary, primary),
            _ => unreachable!("microphone slots are exhaustive"),
        };
        if !selected_quality.is_finite() || best_quality > selected_quality + PRIORITY_SWITCH_MARGIN
        {
            if self.candidate_slot == Some(best) {
                self.candidate_windows = self.candidate_windows.saturating_add(1);
            } else {
                self.candidate_slot = Some(best);
                self.candidate_windows = 1;
            }
            if self.candidate_windows >= PRIORITY_SWITCH_WINDOWS {
                *selected = best;
                self.candidate_slot = None;
                self.candidate_windows = 0;
            }
        } else {
            self.candidate_slot = None;
            self.candidate_windows = 0;
        }
        Some(*selected)
    }
}

impl DuplicateSignalTracker {
    fn observe(
        &mut self,
        pending: &[PendingMixSample],
        sample_rate: u32,
        observed_at_sample: i64,
    ) -> AlignmentState {
        if pending
            .iter()
            .any(|sample| sample.primary.is_none() || sample.secondary.is_none())
        {
            self.reset();
            return self.alignment;
        }

        for sample in pending {
            self.primary
                .push_back(sample.primary.expect("paired primary sample"));
            self.secondary
                .push_back(sample.secondary.expect("paired secondary sample"));
        }
        let max_history_samples = duration_samples(sample_rate, DUPLICATE_ANALYSIS_MAX_HISTORY_MS);
        while self.primary.len() > max_history_samples {
            self.primary.pop_front();
            self.secondary.pop_front();
        }
        self.samples_since_analysis = self.samples_since_analysis.saturating_add(pending.len());

        let analysis_interval = duration_samples(sample_rate, DUPLICATE_ANALYSIS_INTERVAL_MS);
        let minimum_history = duration_samples(sample_rate, DUPLICATE_ANALYSIS_MIN_HISTORY_MS);
        if self.primary.len() < minimum_history || self.samples_since_analysis < analysis_interval {
            return self.alignment;
        }
        self.samples_since_analysis = 0;

        let predicted_lag = (!self.lag_observations.is_empty()).then(|| {
            let elapsed =
                observed_at_sample.saturating_sub(self.alignment.observed_at_sample) as f64;
            self.alignment.lag_samples + elapsed * self.alignment.drift_ppm / 1_000_000.0
        });
        match estimate_alignment(&self.primary, &self.secondary, sample_rate, predicted_lag) {
            DuplicateAnalysis::Correlated {
                lag_samples,
                correlation,
                prominence,
                secondary_gain,
            } => {
                let first = self.lag_observations.is_empty();
                self.alignment.lag_samples = if first {
                    lag_samples
                } else {
                    self.alignment.lag_samples * 0.72 + lag_samples * 0.28
                };
                self.alignment.correlation = correlation;
                self.alignment.secondary_gain = if first {
                    secondary_gain
                } else {
                    self.alignment.secondary_gain * 0.85 + secondary_gain * 0.15
                };
                self.alignment.secondary_polarity = if correlation < 0.0 { -1.0 } else { 1.0 };
                let strong_alignment = correlation.abs() >= ALIGNMENT_BLEND_CORRELATION_THRESHOLD
                    && prominence >= ALIGNMENT_PROMINENCE_THRESHOLD;
                if strong_alignment {
                    self.confident_windows = self.confident_windows.saturating_add(1);
                    self.uncorrelated_windows = 0;
                    if self.confident_windows >= ALIGNMENT_CONFIDENCE_WINDOWS {
                        self.alignment.confident = true;
                    }
                } else {
                    self.confident_windows = 0;
                    self.release_confidence();
                }
                self.alignment.observed_at_sample = observed_at_sample;
                self.lag_observations
                    .push_back((observed_at_sample, lag_samples));
                while self.lag_observations.len() > ALIGNMENT_ESTIMATE_HISTORY {
                    self.lag_observations.pop_front();
                }
                let anchored_drift = match self.drift_anchor {
                    Some((anchor_sample, anchor_lag))
                        if observed_at_sample.saturating_sub(anchor_sample)
                            >= i64::from(sample_rate) / 2 =>
                    {
                        Some(
                            (lag_samples - anchor_lag)
                                / observed_at_sample.saturating_sub(anchor_sample).max(1) as f64
                                * 1_000_000.0,
                        )
                    }
                    Some(_) => None,
                    None => {
                        self.drift_anchor = Some((observed_at_sample, lag_samples));
                        None
                    }
                };
                if let Some(drift_ppm) =
                    anchored_drift.or_else(|| estimate_drift_ppm(&self.lag_observations))
                {
                    self.alignment.drift_ppm = if first {
                        drift_ppm
                    } else {
                        self.alignment.drift_ppm * 0.75 + drift_ppm * 0.25
                    }
                    .clamp(-ALIGNMENT_MAX_DRIFT_PPM, ALIGNMENT_MAX_DRIFT_PPM);
                }
            }
            DuplicateAnalysis::Uncorrelated => {
                self.confident_windows = 0;
                self.release_confidence();
            }
            DuplicateAnalysis::Silent => {}
        }
        self.alignment
    }

    fn reset(&mut self) {
        self.primary.clear();
        self.secondary.clear();
        self.samples_since_analysis = 0;
        self.confident_windows = 0;
        self.uncorrelated_windows = 0;
        self.alignment.confident = false;
        self.lag_observations.clear();
        self.drift_anchor = None;
    }

    fn release_confidence(&mut self) {
        self.uncorrelated_windows = self.uncorrelated_windows.saturating_add(1);
        if self.uncorrelated_windows >= DUPLICATE_RELEASE_WINDOWS {
            self.alignment.confident = false;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DuplicateAnalysis {
    Correlated {
        lag_samples: f64,
        correlation: f64,
        prominence: f64,
        secondary_gain: f32,
    },
    Uncorrelated,
    Silent,
}

#[derive(Debug)]
pub struct MicrophoneMixer {
    mode: MicrophoneMixMode,
    target_sample_rate: u32,
    base_time: DateTime<Utc>,
    reorder_samples: i64,
    alignment_buffer_samples: i64,
    alignment_history_samples: i64,
    pending: BTreeMap<i64, PendingMixSample>,
    latest_end_by_slot: BTreeMap<MicrophoneSlot, i64>,
    last_active_slots: Vec<MicrophoneSlot>,
    next_output_sample: Option<i64>,
    duplicate_signal: DuplicateSignalTracker,
    priority_signal: PrioritySignalTracker,
    primary_mix_weight: Option<f32>,
    alignment: AlignmentState,
}

impl MicrophoneMixer {
    pub fn new(target_sample_rate: u32, base_time: DateTime<Utc>) -> Self {
        Self::with_mode(target_sample_rate, base_time, MicrophoneMixMode::Merge)
    }

    pub fn with_mode(
        target_sample_rate: u32,
        base_time: DateTime<Utc>,
        mode: MicrophoneMixMode,
    ) -> Self {
        assert!(target_sample_rate > 0, "mixer sample rate must be non-zero");
        let reorder_samples =
            ((target_sample_rate as u64 * MICROPHONE_MIX_REORDER_MS).div_ceil(1_000)) as i64;
        let alignment_buffer_samples =
            duration_samples(target_sample_rate, MICROPHONE_MIX_BUFFER_MS) as i64;
        let alignment_history_samples =
            duration_samples(target_sample_rate, DUPLICATE_ANALYSIS_MAX_DELAY_MS) as i64 + 2;
        Self {
            mode,
            target_sample_rate,
            base_time,
            reorder_samples: reorder_samples.max(1),
            alignment_buffer_samples: alignment_buffer_samples.max(1),
            alignment_history_samples: alignment_history_samples.max(2),
            pending: BTreeMap::new(),
            latest_end_by_slot: BTreeMap::new(),
            last_active_slots: Vec::new(),
            next_output_sample: None,
            duplicate_signal: DuplicateSignalTracker::default(),
            priority_signal: PrioritySignalTracker::default(),
            primary_mix_weight: None,
            alignment: AlignmentState {
                secondary_gain: 1.0,
                secondary_polarity: 1.0,
                ..AlignmentState::default()
            },
        }
    }

    pub fn target_sample_rate(&self) -> u32 {
        self.target_sample_rate
    }

    pub fn alignment_snapshot(&self) -> MicrophoneAlignmentSnapshot {
        MicrophoneAlignmentSnapshot {
            lag_samples: self.alignment.lag_samples,
            drift_ppm: self.alignment.drift_ppm,
            correlation: self.alignment.correlation,
            confident: self.alignment.confident,
            secondary_gain: self.alignment.secondary_gain,
            secondary_polarity: self.alignment.secondary_polarity,
        }
    }

    pub fn push_frame(
        &mut self,
        slot: MicrophoneSlot,
        input_sample_rate: u32,
        frame: AudioFrame,
        active_slots: &[MicrophoneSlot],
    ) -> Vec<AudioFrame> {
        let samples = resample_mono(&frame.samples, input_sample_rate, self.target_sample_rate);
        if samples.is_empty() {
            return self.drain(active_slots);
        }

        let end_sample = self.timeline_sample(frame.captured_at);
        let start_sample = end_sample.saturating_sub(samples.len() as i64);
        let output_floor = *self.next_output_sample.get_or_insert(start_sample);
        for (offset, sample) in samples.into_iter().enumerate() {
            let index = start_sample.saturating_add(offset as i64);
            if index < output_floor {
                continue;
            }
            let pending = self.pending.entry(index).or_default();
            match slot {
                MicrophoneSlot::Primary => pending.primary = Some(sample),
                MicrophoneSlot::Secondary => pending.secondary = Some(sample),
            }
        }
        self.latest_end_by_slot
            .entry(slot)
            .and_modify(|latest| *latest = (*latest).max(end_sample))
            .or_insert(end_sample);

        self.drain(active_slots)
    }

    pub fn drain(&mut self, active_slots: &[MicrophoneSlot]) -> Vec<AudioFrame> {
        self.last_active_slots = active_slots.to_vec();
        if active_slots.is_empty() {
            return Vec::new();
        }
        let Some(latest_end) = self.latest_end_by_slot.values().copied().max() else {
            return Vec::new();
        };
        let all_active_seen = active_slots
            .iter()
            .all(|slot| self.latest_end_by_slot.contains_key(slot));
        let timeout_end = latest_end.saturating_sub(self.reorder_samples);
        let ready_end = if all_active_seen {
            let synchronized_end = active_slots
                .iter()
                .filter_map(|slot| self.latest_end_by_slot.get(slot))
                .copied()
                .min()
                .unwrap_or(latest_end);
            synchronized_end.max(timeout_end)
        } else {
            timeout_end
        };
        let ready_end = if self.mode == MicrophoneMixMode::Merge && active_slots.len() > 1 {
            ready_end.saturating_sub(self.alignment_buffer_samples)
        } else {
            ready_end
        };
        self.emit_until(ready_end)
    }

    pub fn flush(&mut self) -> Vec<AudioFrame> {
        let pending_end = self
            .pending
            .last_key_value()
            .map(|(index, _)| index.saturating_add(1));
        let observed_end = self.latest_end_by_slot.values().copied().max();
        let Some(mut end_sample) = pending_end.into_iter().chain(observed_end).max() else {
            return Vec::new();
        };
        if self.last_active_slots.len() > 1 {
            let active_ends = self
                .last_active_slots
                .iter()
                .filter_map(|slot| self.latest_end_by_slot.get(slot).copied())
                .collect::<Vec<_>>();
            if active_ends.len() == self.last_active_slots.len() {
                let earliest_end = *active_ends.iter().min().expect("active ends are non-empty");
                let latest_end = *active_ends.iter().max().expect("active ends are non-empty");
                if latest_end.saturating_sub(earliest_end) <= self.reorder_samples {
                    end_sample = end_sample.min(earliest_end);
                }
            }
        }
        self.emit_until(end_sample)
    }

    fn timeline_sample(&self, captured_at: DateTime<Utc>) -> i64 {
        let elapsed_us = (captured_at - self.base_time)
            .num_microseconds()
            .unwrap_or(i64::MAX)
            .max(0);
        ((elapsed_us as f64 * self.target_sample_rate as f64) / 1_000_000.0).round() as i64
    }

    fn emit_until(&mut self, end_sample: i64) -> Vec<AudioFrame> {
        let Some(start_sample) = self.next_output_sample else {
            return Vec::new();
        };
        if end_sample <= start_sample {
            return Vec::new();
        }

        let ready = self
            .pending
            .range(start_sample..end_sample)
            .map(|(index, sample)| (*index, *sample))
            .collect::<Vec<_>>();
        self.next_output_sample = Some(end_sample);
        if ready.is_empty() {
            return Vec::new();
        }

        if self.mode == MicrophoneMixMode::Merge {
            let raw = ready.iter().map(|(_, sample)| *sample).collect::<Vec<_>>();
            self.alignment =
                self.duplicate_signal
                    .observe(&raw, self.target_sample_rate, end_sample);
        }
        let prepared = ready
            .iter()
            .map(|(index, sample)| PreparedMixSample {
                primary: sample.primary,
                secondary: sample.secondary,
                aligned_secondary: self
                    .alignment
                    .confident
                    .then(|| {
                        let elapsed =
                            index.saturating_sub(self.alignment.observed_at_sample) as f64;
                        let predicted_lag = self.alignment.lag_samples
                            + elapsed * self.alignment.drift_ppm / 1_000_000.0;
                        sample_at_slot(
                            &self.pending,
                            MicrophoneSlot::Secondary,
                            *index as f64 + predicted_lag,
                        )
                        .map(|value| {
                            value
                                * self.alignment.secondary_gain
                                * self.alignment.secondary_polarity
                        })
                    })
                    .flatten(),
            })
            .collect::<Vec<_>>();

        let mut frames = Vec::new();
        let mut group_start = ready[0].0;
        let mut previous = group_start;
        let mut group = vec![prepared[0]];
        for ((index, _), sample) in ready.into_iter().zip(prepared).skip(1) {
            if index != previous.saturating_add(1) {
                frames.push(self.mixed_frame(group_start, previous.saturating_add(1), &group));
                group_start = index;
                group.clear();
            }
            group.push(sample);
            previous = index;
        }
        frames.push(self.mixed_frame(group_start, previous.saturating_add(1), &group));
        let retain_from = end_sample.saturating_sub(self.alignment_history_samples);
        self.pending.retain(|index, _| *index >= retain_from);
        frames
    }

    fn mixed_frame(
        &mut self,
        _start_sample: i64,
        end_sample: i64,
        pending: &[PreparedMixSample],
    ) -> AudioFrame {
        let primary_rms = slot_rms(pending.iter().filter_map(|sample| sample.primary));
        let secondary_rms = slot_rms(pending.iter().filter_map(|sample| sample.secondary));
        let total_rms = primary_rms + secondary_rms;
        let energy_weight = if total_rms > f32::EPSILON {
            primary_rms / total_rms
        } else {
            0.5
        };
        let preferred_slot = match self.mode {
            MicrophoneMixMode::Merge if self.alignment.confident => None,
            MicrophoneMixMode::Merge => {
                Some(if pending.iter().any(|sample| sample.primary.is_some()) {
                    MicrophoneSlot::Primary
                } else {
                    MicrophoneSlot::Secondary
                })
            }
            MicrophoneMixMode::Priority => self.priority_signal.observe(pending),
        };
        let target_primary_weight = match (self.mode, self.alignment.confident, preferred_slot) {
            (MicrophoneMixMode::Merge, true, _) => 0.5,
            (_, _, Some(MicrophoneSlot::Primary)) => 1.0,
            (_, _, Some(MicrophoneSlot::Secondary)) => 0.0,
            (_, _, None) => energy_weight,
        };
        let mut primary_weight = self.primary_mix_weight.unwrap_or(target_primary_weight);
        let crossfade_ms = match self.mode {
            MicrophoneMixMode::Merge => DUPLICATE_CROSSFADE_MS,
            MicrophoneMixMode::Priority => PRIORITY_CROSSFADE_MS,
        };
        let maximum_weight_step =
            1.0 / duration_samples(self.target_sample_rate, crossfade_ms).max(1) as f32;
        let samples = pending
            .iter()
            .map(|sample| {
                let secondary = if self.mode == MicrophoneMixMode::Merge && self.alignment.confident
                {
                    sample.aligned_secondary
                } else {
                    sample.secondary
                };
                match (sample.primary, secondary) {
                    (Some(primary), Some(secondary)) => {
                        primary_weight =
                            move_toward(primary_weight, target_primary_weight, maximum_weight_step);
                        let (primary_gain, secondary_gain) = match self.mode {
                            MicrophoneMixMode::Merge => (primary_weight, 1.0 - primary_weight),
                            MicrophoneMixMode::Priority => {
                                (primary_weight.sqrt(), (1.0 - primary_weight).sqrt())
                            }
                        };
                        (primary * primary_gain + secondary * secondary_gain).clamp(-1.0, 1.0)
                    }
                    (Some(primary), None) => primary.clamp(-1.0, 1.0),
                    (None, Some(secondary)) => secondary.clamp(-1.0, 1.0),
                    (None, None) => 0.0,
                }
            })
            .collect::<Vec<_>>();
        self.primary_mix_weight = Some(primary_weight);
        let duration_ms =
            ((samples.len() as f64 / self.target_sample_rate as f64) * 1_000.0).round() as u64;
        let captured_at = self.base_time
            + Duration::microseconds(
                ((end_sample as i128 * 1_000_000) / self.target_sample_rate as i128)
                    .clamp(i64::MIN as i128, i64::MAX as i128) as i64,
            );
        AudioFrame {
            samples,
            duration_ms: duration_ms.max(1),
            captured_at,
        }
    }
}

fn sample_at_slot(
    timeline: &BTreeMap<i64, PendingMixSample>,
    slot: MicrophoneSlot,
    position: f64,
) -> Option<f32> {
    let left_index = position.floor() as i64;
    let right_index = position.ceil() as i64;
    let value = |index| {
        timeline.get(&index).and_then(|sample| match slot {
            MicrophoneSlot::Primary => sample.primary,
            MicrophoneSlot::Secondary => sample.secondary,
        })
    };
    let left = value(left_index)?;
    if right_index == left_index {
        return Some(left);
    }
    let right = value(right_index)?;
    let fraction = (position - left_index as f64) as f32;
    Some(left + (right - left) * fraction)
}

fn duration_samples(sample_rate: u32, duration_ms: u64) -> usize {
    ((u64::from(sample_rate) * duration_ms).div_ceil(1_000))
        .max(1)
        .min(usize::MAX as u64) as usize
}

fn move_toward(current: f32, target: f32, maximum_step: f32) -> f32 {
    if current < target {
        (current + maximum_step).min(target)
    } else {
        (current - maximum_step).max(target)
    }
}

fn estimate_alignment(
    primary: &VecDeque<f32>,
    secondary: &VecDeque<f32>,
    sample_rate: u32,
    predicted_lag: Option<f64>,
) -> DuplicateAnalysis {
    let stride = sample_rate.div_ceil(DUPLICATE_ANALYSIS_RATE_HZ).max(1) as usize;
    let primary_native = primary.iter().copied().collect::<Vec<_>>();
    let secondary_native = secondary.iter().copied().collect::<Vec<_>>();
    let primary = primary_native
        .iter()
        .step_by(stride)
        .copied()
        .collect::<Vec<_>>();
    let secondary = secondary_native
        .iter()
        .step_by(stride)
        .copied()
        .collect::<Vec<_>>();
    let len = primary.len().min(secondary.len());
    let analysis_rate = sample_rate.div_ceil(stride as u32).max(1);
    let minimum_overlap = duration_samples(analysis_rate, DUPLICATE_ANALYSIS_MIN_OVERLAP_MS);
    if len <= minimum_overlap {
        return DuplicateAnalysis::Silent;
    }

    let maximum_delay = duration_samples(analysis_rate, DUPLICATE_ANALYSIS_MAX_DELAY_MS)
        .min(len.saturating_sub(minimum_overlap));
    let mut best_lag = 0_i64;
    let mut best_correlation = 0.0_f64;
    let mut coarse_candidates = Vec::with_capacity(maximum_delay.saturating_mul(2) + 1);
    let tracking_radius = duration_samples(analysis_rate, ALIGNMENT_TRACKING_RADIUS_MS) as i64;
    let (minimum_lag, maximum_lag) = predicted_lag
        .map(|lag| {
            let expected = (lag / stride as f64).round() as i64;
            (
                (expected - tracking_radius).max(-(maximum_delay as i64)),
                (expected + tracking_radius).min(maximum_delay as i64),
            )
        })
        .unwrap_or((-(maximum_delay as i64), maximum_delay as i64));
    for lag in minimum_lag..=maximum_lag {
        let Some((correlation, _, _)) =
            correlation_at_lag(&primary, &secondary, lag, minimum_overlap)
        else {
            continue;
        };
        coarse_candidates.push((lag, correlation.abs()));
        if correlation.abs() > best_correlation {
            best_correlation = correlation.abs();
            best_lag = lag;
        }
    }

    let coarse_native_lag = best_lag.saturating_mul(stride as i64);
    let native_maximum_delay =
        duration_samples(sample_rate, DUPLICATE_ANALYSIS_MAX_DELAY_MS) as i64;
    let native_minimum_overlap = duration_samples(sample_rate, DUPLICATE_ANALYSIS_MIN_OVERLAP_MS);
    let refinement_radius = stride.max(1) as i64 * 2;
    let mut fine_candidates = Vec::new();
    for lag in (coarse_native_lag - refinement_radius).max(-native_maximum_delay)
        ..=(coarse_native_lag + refinement_radius).min(native_maximum_delay)
    {
        if let Some((correlation, primary_rms, secondary_rms)) = correlation_at_lag(
            &primary_native,
            &secondary_native,
            lag,
            native_minimum_overlap,
        ) {
            fine_candidates.push((lag, correlation, primary_rms, secondary_rms));
        }
    }
    let Some((fine_index, (_, signed_correlation, primary_rms, secondary_rms))) = fine_candidates
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.1.abs().total_cmp(&right.1.abs()))
    else {
        return DuplicateAnalysis::Silent;
    };
    if *primary_rms < f64::from(DUPLICATE_MIN_RMS) || *secondary_rms < f64::from(DUPLICATE_MIN_RMS)
    {
        return DuplicateAnalysis::Silent;
    }
    let integer_lag = fine_candidates[fine_index].0;
    let fractional_offset = if fine_index > 0 && fine_index + 1 < fine_candidates.len() {
        let left = fine_candidates[fine_index - 1].1.abs();
        let center = signed_correlation.abs();
        let right = fine_candidates[fine_index + 1].1.abs();
        let denominator = left - 2.0 * center + right;
        if denominator.abs() > 1e-9 {
            (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
        } else {
            0.0
        }
    } else {
        0.0
    };
    let second_best = coarse_candidates
        .iter()
        .filter(|(lag, _)| (lag - best_lag).abs() > 2)
        .map(|(_, correlation)| *correlation)
        .fold(0.0_f64, f64::max);
    let prominence = (signed_correlation.abs() - second_best).max(0.0);

    if signed_correlation.abs() >= DUPLICATE_CORRELATION_THRESHOLD {
        DuplicateAnalysis::Correlated {
            lag_samples: integer_lag as f64 + fractional_offset,
            correlation: *signed_correlation,
            prominence,
            secondary_gain: (*primary_rms / *secondary_rms).clamp(0.5, 2.0) as f32,
        }
    } else if signed_correlation.abs() < DUPLICATE_RELEASE_THRESHOLD {
        DuplicateAnalysis::Uncorrelated
    } else {
        DuplicateAnalysis::Silent
    }
}

fn correlation_at_lag(
    primary: &[f32],
    secondary: &[f32],
    lag: i64,
    minimum_overlap: usize,
) -> Option<(f64, f64, f64)> {
    let len = primary.len().min(secondary.len());
    let (primary_start, secondary_start) = if lag >= 0 {
        (0, lag as usize)
    } else {
        ((-lag) as usize, 0)
    };
    let overlap = len.saturating_sub(primary_start.max(secondary_start));
    if overlap < minimum_overlap {
        return None;
    }
    let primary_mean = primary[primary_start..primary_start + overlap]
        .iter()
        .map(|sample| f64::from(*sample))
        .sum::<f64>()
        / overlap as f64;
    let secondary_mean = secondary[secondary_start..secondary_start + overlap]
        .iter()
        .map(|sample| f64::from(*sample))
        .sum::<f64>()
        / overlap as f64;
    let mut cross = 0.0_f64;
    let mut primary_energy = 0.0_f64;
    let mut secondary_energy = 0.0_f64;
    for offset in 0..overlap {
        let primary_sample = f64::from(primary[primary_start + offset]) - primary_mean;
        let secondary_sample = f64::from(secondary[secondary_start + offset]) - secondary_mean;
        cross += primary_sample * secondary_sample;
        primary_energy += primary_sample * primary_sample;
        secondary_energy += secondary_sample * secondary_sample;
    }
    let denominator = (primary_energy * secondary_energy).sqrt();
    (denominator > f64::EPSILON).then(|| {
        (
            cross / denominator,
            (primary_energy / overlap as f64).sqrt(),
            (secondary_energy / overlap as f64).sqrt(),
        )
    })
}

fn estimate_drift_ppm(observations: &VecDeque<(i64, f64)>) -> Option<f64> {
    if observations.len() < 4 {
        return None;
    }
    let mean_x =
        observations.iter().map(|(x, _)| *x as f64).sum::<f64>() / observations.len() as f64;
    let mean_y = observations.iter().map(|(_, y)| *y).sum::<f64>() / observations.len() as f64;
    let (covariance, variance) =
        observations
            .iter()
            .fold((0.0_f64, 0.0_f64), |(covariance, variance), (x, y)| {
                let centered_x = *x as f64 - mean_x;
                (
                    covariance + centered_x * (*y - mean_y),
                    variance + centered_x * centered_x,
                )
            });
    (variance > f64::EPSILON).then(|| covariance / variance * 1_000_000.0)
}

fn resample_mono(samples: &[f32], input_sample_rate: u32, output_sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() || input_sample_rate == 0 || output_sample_rate == 0 {
        return Vec::new();
    }
    if input_sample_rate == output_sample_rate {
        return samples.to_vec();
    }
    let output_len = ((samples.len() as u128 * output_sample_rate as u128
        + input_sample_rate as u128 / 2)
        / input_sample_rate as u128)
        .max(1)
        .min(usize::MAX as u128) as usize;
    if output_len == 1 || samples.len() == 1 {
        return vec![samples[0]; output_len];
    }

    let input_span = (samples.len() - 1) as f64;
    let output_span = (output_len - 1) as f64;
    (0..output_len)
        .map(|index| {
            let source_position = index as f64 * input_span / output_span;
            let left = source_position.floor() as usize;
            let right = (left + 1).min(samples.len() - 1);
            let fraction = (source_position - left as f64) as f32;
            samples[left] + (samples[right] - samples[left]) * fraction
        })
        .collect()
}

fn slot_rms(samples: impl Iterator<Item = f32>) -> f32 {
    let (energy, count) = samples.fold((0.0_f64, 0_u64), |(energy, count), sample| {
        (energy + f64::from(sample) * f64::from(sample), count + 1)
    });
    if count == 0 {
        0.0
    } else {
        (energy / count as f64).sqrt() as f32
    }
}

fn slot_quality(samples: impl Iterator<Item = f32>) -> f32 {
    let mut energy = 0.0_f64;
    let mut count = 0_u64;
    let mut clipped = 0_u64;
    for sample in samples {
        let sample = sample.clamp(-1.0, 1.0);
        energy += f64::from(sample) * f64::from(sample);
        count += 1;
        if sample.abs() >= 0.98 {
            clipped += 1;
        }
    }
    if count == 0 {
        return f32::NEG_INFINITY;
    }
    let rms = (energy / count as f64).sqrt() as f32;
    if rms < DUPLICATE_MIN_RMS {
        return f32::NEG_INFINITY;
    }
    let clipping_ratio = clipped as f32 / count as f32;
    rms.min(0.5) - clipping_ratio * 2.0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrophoneRuntimeDiagnostic {
    pub slot: MicrophoneSlot,
    pub running: bool,
    pub dropped_frame_count: u64,
    pub runtime_error: Option<String>,
}

pub struct MultiCaptureRuntime<B: AudioInputBackend + Default> {
    runtimes: BTreeMap<MicrophoneSlot, LiveCaptureRuntime<B>>,
}

impl<B: AudioInputBackend + Default> Default for MultiCaptureRuntime<B> {
    fn default() -> Self {
        Self {
            runtimes: BTreeMap::new(),
        }
    }
}

impl<B: AudioInputBackend + Default> MultiCaptureRuntime<B> {
    pub fn start_slot(
        &mut self,
        slot: MicrophoneSlot,
        config: AudioInputConfig,
        on_frame: impl FnMut(AudioFrame) + Send + 'static,
    ) -> Result<(), LiveCaptureError> {
        if let Some(runtime) = self.runtimes.get_mut(&slot) {
            return runtime.start(config, on_frame);
        }

        let mut runtime = LiveCaptureRuntime::new(B::default());
        runtime.start(config, on_frame)?;
        self.runtimes.insert(slot, runtime);
        Ok(())
    }

    pub fn start_slot_processed<P, F>(
        &mut self,
        slot: MicrophoneSlot,
        config: AudioInputConfig,
        processor: P,
        on_frame: F,
    ) -> Result<(), LiveCaptureError>
    where
        P: AudioFrameProcessor + 'static,
        F: FnMut(AudioFrame) + Send + 'static,
    {
        if let Some(runtime) = self.runtimes.get_mut(&slot) {
            return runtime.start_processed(config, processor, on_frame);
        }

        let mut runtime = LiveCaptureRuntime::new(B::default());
        runtime.start_processed(config, processor, on_frame)?;
        self.runtimes.insert(slot, runtime);
        Ok(())
    }

    pub fn start_slot_processed_with_warnings<P, F, W>(
        &mut self,
        slot: MicrophoneSlot,
        config: AudioInputConfig,
        processor: P,
        on_frame: F,
        on_warning: W,
    ) -> Result<(), LiveCaptureError>
    where
        P: AudioFrameProcessor + 'static,
        F: FnMut(AudioFrame) + Send + 'static,
        W: FnMut(String) + Send + 'static,
    {
        if let Some(runtime) = self.runtimes.get_mut(&slot) {
            return runtime.start_processed_with_warnings(config, processor, on_frame, on_warning);
        }

        let mut runtime = LiveCaptureRuntime::new(B::default());
        runtime.start_processed_with_warnings(config, processor, on_frame, on_warning)?;
        self.runtimes.insert(slot, runtime);
        Ok(())
    }

    pub fn stop_slot(&mut self, slot: MicrophoneSlot) {
        if let Some(mut runtime) = self.runtimes.remove(&slot) {
            runtime.stop();
        }
    }

    pub fn stop_all(&mut self) {
        for runtime in self.runtimes.values_mut() {
            runtime.stop();
        }
        self.runtimes.clear();
    }

    pub fn is_running(&self, slot: MicrophoneSlot) -> bool {
        self.runtimes
            .get(&slot)
            .is_some_and(LiveCaptureRuntime::is_running)
    }

    pub fn active_slots(&self) -> Vec<MicrophoneSlot> {
        self.runtimes
            .iter()
            .filter_map(|(slot, runtime)| runtime.is_running().then_some(*slot))
            .collect()
    }

    pub fn diagnostics(&self) -> Vec<MicrophoneRuntimeDiagnostic> {
        self.runtimes
            .iter()
            .map(|(slot, runtime)| MicrophoneRuntimeDiagnostic {
                slot: *slot,
                running: runtime.is_running(),
                dropped_frame_count: runtime.dropped_frame_count(),
                runtime_error: runtime.runtime_error(),
            })
            .collect()
    }

    pub fn diagnostic(&self, slot: MicrophoneSlot) -> Option<MicrophoneRuntimeDiagnostic> {
        self.runtimes
            .get(&slot)
            .map(|runtime| MicrophoneRuntimeDiagnostic {
                slot,
                running: runtime.is_running(),
                dropped_frame_count: runtime.dropped_frame_count(),
                runtime_error: runtime.runtime_error(),
            })
    }

    pub fn total_dropped_frame_count(&self) -> u64 {
        self.runtimes
            .values()
            .map(LiveCaptureRuntime::dropped_frame_count)
            .sum()
    }

    pub fn first_runtime_error(&self) -> Option<String> {
        self.runtimes
            .values()
            .find_map(LiveCaptureRuntime::runtime_error)
    }
}
