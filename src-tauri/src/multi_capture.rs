use std::collections::{BTreeMap, VecDeque};

use chrono::{DateTime, Duration, Utc};

use crate::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, LiveCaptureError, LiveCaptureRuntime,
};
use crate::settings::MicrophoneSlot;
use crate::voice_leveling::AudioFrameProcessor;

pub const MICROPHONE_MIX_REORDER_MS: u64 = 100;
const DUPLICATE_ANALYSIS_RATE_HZ: u32 = 2_000;
const DUPLICATE_ANALYSIS_INTERVAL_MS: u64 = 50;
const DUPLICATE_ANALYSIS_MIN_HISTORY_MS: u64 = 80;
const DUPLICATE_ANALYSIS_MAX_HISTORY_MS: u64 = 500;
const DUPLICATE_ANALYSIS_MAX_DELAY_MS: u64 = 200;
const DUPLICATE_ANALYSIS_MIN_OVERLAP_MS: u64 = 60;
const DUPLICATE_MIN_DELAY_MS: u64 = 2;
const DUPLICATE_MIN_RMS: f32 = 0.004;
const DUPLICATE_CORRELATION_THRESHOLD: f64 = 0.78;
const DUPLICATE_RELEASE_THRESHOLD: f64 = 0.55;
const DUPLICATE_RELEASE_WINDOWS: u8 = 2;
const DUPLICATE_CROSSFADE_MS: u64 = 20;

#[derive(Debug, Clone, Copy, Default)]
struct PendingMixSample {
    primary: Option<f32>,
    secondary: Option<f32>,
}

#[derive(Debug, Default)]
struct DuplicateSignalTracker {
    primary: VecDeque<f32>,
    secondary: VecDeque<f32>,
    samples_since_analysis: usize,
    preferred_slot: Option<MicrophoneSlot>,
    uncorrelated_windows: u8,
}

impl DuplicateSignalTracker {
    fn observe(
        &mut self,
        pending: &[PendingMixSample],
        sample_rate: u32,
    ) -> Option<MicrophoneSlot> {
        if pending
            .iter()
            .any(|sample| sample.primary.is_none() || sample.secondary.is_none())
        {
            self.reset();
            return None;
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
            return self.preferred_slot;
        }
        self.samples_since_analysis = 0;

        match estimate_duplicate_delay(&self.primary, &self.secondary, sample_rate) {
            DuplicateAnalysis::Correlated { lag_samples } => {
                self.preferred_slot = Some(if lag_samples > 0 {
                    MicrophoneSlot::Primary
                } else {
                    MicrophoneSlot::Secondary
                });
                self.uncorrelated_windows = 0;
            }
            DuplicateAnalysis::Uncorrelated => {
                self.uncorrelated_windows = self.uncorrelated_windows.saturating_add(1);
                if self.uncorrelated_windows >= DUPLICATE_RELEASE_WINDOWS {
                    self.preferred_slot = None;
                    self.uncorrelated_windows = 0;
                }
            }
            DuplicateAnalysis::Silent => {}
        }
        self.preferred_slot
    }

    fn reset(&mut self) {
        self.primary.clear();
        self.secondary.clear();
        self.samples_since_analysis = 0;
        self.preferred_slot = None;
        self.uncorrelated_windows = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DuplicateAnalysis {
    Correlated { lag_samples: i64 },
    Uncorrelated,
    Silent,
}

#[derive(Debug)]
pub struct MicrophoneMixer {
    target_sample_rate: u32,
    base_time: DateTime<Utc>,
    reorder_samples: i64,
    pending: BTreeMap<i64, PendingMixSample>,
    latest_end_by_slot: BTreeMap<MicrophoneSlot, i64>,
    next_output_sample: Option<i64>,
    duplicate_signal: DuplicateSignalTracker,
    primary_mix_weight: Option<f32>,
}

impl MicrophoneMixer {
    pub fn new(target_sample_rate: u32, base_time: DateTime<Utc>) -> Self {
        assert!(target_sample_rate > 0, "mixer sample rate must be non-zero");
        let reorder_samples =
            ((target_sample_rate as u64 * MICROPHONE_MIX_REORDER_MS).div_ceil(1_000)) as i64;
        Self {
            target_sample_rate,
            base_time,
            reorder_samples: reorder_samples.max(1),
            pending: BTreeMap::new(),
            latest_end_by_slot: BTreeMap::new(),
            next_output_sample: None,
            duplicate_signal: DuplicateSignalTracker::default(),
            primary_mix_weight: None,
        }
    }

    pub fn target_sample_rate(&self) -> u32 {
        self.target_sample_rate
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
        self.emit_until(ready_end)
    }

    pub fn flush(&mut self) -> Vec<AudioFrame> {
        let pending_end = self
            .pending
            .last_key_value()
            .map(|(index, _)| index.saturating_add(1));
        let observed_end = self.latest_end_by_slot.values().copied().max();
        let Some(end_sample) = pending_end.into_iter().chain(observed_end).max() else {
            return Vec::new();
        };
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
        self.pending.retain(|index, _| *index >= end_sample);
        self.next_output_sample = Some(end_sample);
        if ready.is_empty() {
            return Vec::new();
        }

        let mut frames = Vec::new();
        let mut group_start = ready[0].0;
        let mut previous = group_start;
        let mut group = vec![ready[0].1];
        for (index, sample) in ready.into_iter().skip(1) {
            if index != previous.saturating_add(1) {
                frames.push(self.mixed_frame(group_start, previous.saturating_add(1), &group));
                group_start = index;
                group.clear();
            }
            group.push(sample);
            previous = index;
        }
        frames.push(self.mixed_frame(group_start, previous.saturating_add(1), &group));
        frames
    }

    fn mixed_frame(
        &mut self,
        _start_sample: i64,
        end_sample: i64,
        pending: &[PendingMixSample],
    ) -> AudioFrame {
        let primary_rms = slot_rms(pending.iter().filter_map(|sample| sample.primary));
        let secondary_rms = slot_rms(pending.iter().filter_map(|sample| sample.secondary));
        let total_rms = primary_rms + secondary_rms;
        let energy_weight = if total_rms > f32::EPSILON {
            primary_rms / total_rms
        } else {
            0.5
        };
        let preferred_slot = self
            .duplicate_signal
            .observe(pending, self.target_sample_rate);
        let target_primary_weight = match preferred_slot {
            Some(MicrophoneSlot::Primary) => 1.0,
            Some(MicrophoneSlot::Secondary) => 0.0,
            None => energy_weight,
        };
        let mut primary_weight = self.primary_mix_weight.unwrap_or(target_primary_weight);
        let maximum_weight_step =
            1.0 / duration_samples(self.target_sample_rate, DUPLICATE_CROSSFADE_MS).max(1) as f32;
        let samples = pending
            .iter()
            .map(|sample| match (sample.primary, sample.secondary) {
                (Some(primary), Some(secondary)) => {
                    primary_weight =
                        move_toward(primary_weight, target_primary_weight, maximum_weight_step);
                    (primary * primary_weight + secondary * (1.0 - primary_weight)).clamp(-1.0, 1.0)
                }
                (Some(primary), None) => primary.clamp(-1.0, 1.0),
                (None, Some(secondary)) => secondary.clamp(-1.0, 1.0),
                (None, None) => 0.0,
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

fn estimate_duplicate_delay(
    primary: &VecDeque<f32>,
    secondary: &VecDeque<f32>,
    sample_rate: u32,
) -> DuplicateAnalysis {
    let stride = sample_rate.div_ceil(DUPLICATE_ANALYSIS_RATE_HZ).max(1) as usize;
    let primary = primary.iter().step_by(stride).copied().collect::<Vec<_>>();
    let secondary = secondary
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

    let primary_mean = primary
        .iter()
        .take(len)
        .map(|sample| f64::from(*sample))
        .sum::<f64>()
        / len as f64;
    let secondary_mean = secondary
        .iter()
        .take(len)
        .map(|sample| f64::from(*sample))
        .sum::<f64>()
        / len as f64;
    let primary_rms = (primary
        .iter()
        .take(len)
        .map(|sample| (f64::from(*sample) - primary_mean).powi(2))
        .sum::<f64>()
        / len as f64)
        .sqrt();
    let secondary_rms = (secondary
        .iter()
        .take(len)
        .map(|sample| (f64::from(*sample) - secondary_mean).powi(2))
        .sum::<f64>()
        / len as f64)
        .sqrt();
    if primary_rms < f64::from(DUPLICATE_MIN_RMS) || secondary_rms < f64::from(DUPLICATE_MIN_RMS) {
        return DuplicateAnalysis::Silent;
    }

    let maximum_delay = duration_samples(analysis_rate, DUPLICATE_ANALYSIS_MAX_DELAY_MS)
        .min(len.saturating_sub(minimum_overlap));
    let mut best_lag = 0_i64;
    let mut best_correlation = 0.0_f64;
    for lag in -(maximum_delay as i64)..=(maximum_delay as i64) {
        let (primary_start, secondary_start) = if lag >= 0 {
            (0, lag as usize)
        } else {
            ((-lag) as usize, 0)
        };
        let overlap = len - primary_start.max(secondary_start);
        if overlap < minimum_overlap {
            continue;
        }
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
        if denominator <= f64::EPSILON {
            continue;
        }
        let correlation = (cross / denominator).abs();
        if correlation > best_correlation {
            best_correlation = correlation;
            best_lag = lag;
        }
    }

    let minimum_delay = duration_samples(analysis_rate, DUPLICATE_MIN_DELAY_MS) as i64;
    if best_correlation >= DUPLICATE_CORRELATION_THRESHOLD && best_lag.abs() >= minimum_delay {
        DuplicateAnalysis::Correlated {
            lag_samples: best_lag.saturating_mul(stride as i64),
        }
    } else if best_correlation < DUPLICATE_RELEASE_THRESHOLD || best_lag.abs() < minimum_delay {
        DuplicateAnalysis::Uncorrelated
    } else {
        DuplicateAnalysis::Silent
    }
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
