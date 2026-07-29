use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

use crate::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, LiveCaptureError, LiveCaptureRuntime,
};
use crate::settings::MicrophoneSlot;

pub const MICROPHONE_MIX_REORDER_MS: u64 = 100;

#[derive(Debug, Clone, Copy, Default)]
struct PendingMixSample {
    primary: Option<f32>,
    secondary: Option<f32>,
}

#[derive(Debug)]
pub struct MicrophoneMixer {
    target_sample_rate: u32,
    base_time: DateTime<Utc>,
    reorder_samples: i64,
    pending: BTreeMap<i64, PendingMixSample>,
    latest_end_by_slot: BTreeMap<MicrophoneSlot, i64>,
    next_output_sample: Option<i64>,
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
        &self,
        _start_sample: i64,
        end_sample: i64,
        pending: &[PendingMixSample],
    ) -> AudioFrame {
        let primary_rms = slot_rms(pending.iter().filter_map(|sample| sample.primary));
        let secondary_rms = slot_rms(pending.iter().filter_map(|sample| sample.secondary));
        let total_rms = primary_rms + secondary_rms;
        let (primary_weight, secondary_weight) = if total_rms > f32::EPSILON {
            (primary_rms / total_rms, secondary_rms / total_rms)
        } else {
            (0.5, 0.5)
        };
        let samples = pending
            .iter()
            .map(|sample| match (sample.primary, sample.secondary) {
                (Some(primary), Some(secondary)) => {
                    (primary * primary_weight + secondary * secondary_weight).clamp(-1.0, 1.0)
                }
                (Some(primary), None) => primary.clamp(-1.0, 1.0),
                (None, Some(secondary)) => secondary.clamp(-1.0, 1.0),
                (None, None) => 0.0,
            })
            .collect::<Vec<_>>();
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
        on_frame: impl Fn(AudioFrame) + Send + Sync + 'static,
    ) -> Result<(), LiveCaptureError> {
        if let Some(runtime) = self.runtimes.get_mut(&slot) {
            return runtime.start(config, on_frame);
        }

        let mut runtime = LiveCaptureRuntime::new(B::default());
        runtime.start(config, on_frame)?;
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
