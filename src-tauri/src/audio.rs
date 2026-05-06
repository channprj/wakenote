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
    recording: bool,
}

pub fn dbfs_from_rms(rms: f32) -> f32 {
    if rms <= 0.0 {
        return -120.0;
    }

    (20.0 * rms.log10()).max(-120.0)
}

pub fn list_input_devices() -> Vec<InputDevice> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let mut devices = vec![InputDevice {
        id: "default".to_string(),
        label: "System Default".to_string(),
        available: true,
    }];

    let host = cpal::default_host();
    let Ok(inputs) = host.input_devices() else {
        return devices;
    };

    for (index, device) in inputs.enumerate() {
        let label = device
            .name()
            .unwrap_or_else(|_| format!("Input Device {}", index + 1));
        devices.push(InputDevice {
            id: format!("input-{index}-{}", slugify_device_label(&label)),
            label,
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

impl SpeechGate {
    pub fn new(config: GateConfig) -> Self {
        Self {
            config,
            above_since_ms: None,
            below_since_ms: None,
            chunk_started_at_ms: None,
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
            self.above_since_ms = if above_threshold { Some(time_ms) } else { None };
            return GateDecision::Rollover {
                ended_at_ms: time_ms,
                next_started_at_ms: time_ms,
            };
        }

        if above_threshold {
            self.below_since_ms = None;
            return GateDecision::Recording;
        }

        let below_since_ms = *self.below_since_ms.get_or_insert(time_ms);
        let release_elapsed = time_ms.saturating_sub(below_since_ms) >= self.config.release_ms;
        let minimum_elapsed =
            time_ms.saturating_sub(chunk_started_at_ms) >= self.config.min_chunk_ms;

        if release_elapsed && minimum_elapsed {
            let ended_at_ms = time_ms.saturating_add(self.config.post_roll_ms);
            self.recording = false;
            self.above_since_ms = None;
            self.below_since_ms = None;
            self.chunk_started_at_ms = None;
            return GateDecision::End { ended_at_ms };
        }

        GateDecision::Recording
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }
}
