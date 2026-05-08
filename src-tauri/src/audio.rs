use std::collections::VecDeque;

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
        let above_threshold = dbfs >= self.config.threshold_dbfs;

        if !self.recording {
            if !above_threshold {
                self.above_since_ms = None;
                return GateDecision::Idle;
            }

            let above_since_ms = *self.above_since_ms.get_or_insert(time_ms);
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

        let below_since_ms = *self.below_since_ms.get_or_insert(time_ms);
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

fn percentile(samples: &VecDeque<f32>, percentile: f32) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }

    let mut sorted = samples.iter().copied().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index = ((sorted.len() - 1) as f32 * percentile).round() as usize;
    sorted.get(index).copied()
}
