use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};

use crate::audio::{dbfs_from_samples, GateConfig, GateDecision, SpeechGate};
use crate::recorder::{RecordedChunk, Recorder, RecorderError, RecordingRequest};
use crate::settings::AppSettings;

#[derive(Debug, Clone)]
pub struct CaptureProcessorConfig {
    pub save_root: PathBuf,
    pub settings: AppSettings,
    pub sample_rate: u32,
    pub device_id: String,
    pub device_name: String,
    pub used_fallback_device: bool,
    pub base_time: DateTime<Utc>,
    pub app_version: String,
}

#[derive(Debug, Clone)]
pub struct CaptureControllerConfig {
    pub save_root: PathBuf,
    pub settings: AppSettings,
    pub sample_rate: u32,
    pub device_id: String,
    pub device_name: String,
    pub used_fallback_device: bool,
    pub base_time: DateTime<Utc>,
    pub app_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureControllerEvent {
    ChunkCompleted { chunk: RecordedChunk },
}

#[derive(Debug)]
pub struct CaptureController {
    config: CaptureControllerConfig,
    processor: CaptureProcessor,
    emitted_chunks: usize,
}

#[derive(Debug, Clone)]
struct BufferedFrame {
    samples: Vec<f32>,
    duration_ms: u64,
}

#[derive(Debug)]
pub struct CaptureProcessor {
    config: CaptureProcessorConfig,
    gate: SpeechGate,
    elapsed_ms: u64,
    pre_roll: VecDeque<BufferedFrame>,
    active_samples: Vec<f32>,
    active_started_at_ms: Option<u64>,
    active_has_signal: bool,
    completed_chunks: Vec<RecordedChunk>,
}

impl CaptureController {
    pub fn new(config: CaptureControllerConfig) -> Self {
        let processor = CaptureProcessor::new(processor_config(&config));
        Self {
            config,
            processor,
            emitted_chunks: 0,
        }
    }

    pub fn process_samples(
        &mut self,
        samples: &[f32],
        duration_ms: u64,
    ) -> Result<Vec<CaptureControllerEvent>, RecorderError> {
        if !self.is_listening() {
            return Ok(Vec::new());
        }

        self.processor.process_samples(samples, duration_ms)?;
        Ok(self.drain_new_events())
    }

    pub fn update_settings(
        &mut self,
        settings: AppSettings,
    ) -> Result<Vec<CaptureControllerEvent>, RecorderError> {
        let was_recording = self.processor.is_recording();
        let should_stop = settings.pause_all || !settings.recording_enabled;
        let mut events = Vec::new();

        if was_recording {
            self.processor.flush()?;
            events.extend(self.drain_new_events());
        }

        self.config.settings = settings;
        if should_stop {
            return Ok(events);
        }

        self.processor.reconfigure(processor_config(&self.config));
        Ok(events)
    }

    pub fn flush(&mut self) -> Result<Vec<CaptureControllerEvent>, RecorderError> {
        self.processor.flush()?;
        Ok(self.drain_new_events())
    }

    pub fn is_listening(&self) -> bool {
        self.config.settings.recording_enabled && !self.config.settings.pause_all
    }

    pub fn is_recording(&self) -> bool {
        self.is_listening() && self.processor.is_recording()
    }

    pub fn completed_chunks(&self) -> &[RecordedChunk] {
        self.processor.completed_chunks()
    }

    fn drain_new_events(&mut self) -> Vec<CaptureControllerEvent> {
        let chunks = self.processor.completed_chunks();
        let events = chunks[self.emitted_chunks..]
            .iter()
            .cloned()
            .map(|chunk| CaptureControllerEvent::ChunkCompleted { chunk })
            .collect::<Vec<_>>();
        self.emitted_chunks = chunks.len();
        events
    }
}

impl CaptureProcessor {
    pub fn new(config: CaptureProcessorConfig) -> Self {
        let gate = SpeechGate::new(GateConfig {
            threshold_dbfs: config.settings.threshold_dbfs,
            attack_ms: config.settings.attack_ms,
            release_ms: config.settings.release_ms,
            pre_roll_ms: config.settings.pre_roll_ms,
            post_roll_ms: config.settings.post_roll_ms,
            min_chunk_ms: config.settings.min_chunk_ms,
            max_chunk_ms: config.settings.max_chunk_ms,
        });

        Self {
            config,
            gate,
            elapsed_ms: 0,
            pre_roll: VecDeque::new(),
            active_samples: Vec::new(),
            active_started_at_ms: None,
            active_has_signal: false,
            completed_chunks: Vec::new(),
        }
    }

    pub fn process_samples(
        &mut self,
        samples: &[f32],
        duration_ms: u64,
    ) -> Result<(), RecorderError> {
        if self.config.settings.pause_all || !self.config.settings.recording_enabled {
            self.elapsed_ms = self.elapsed_ms.saturating_add(duration_ms);
            return Ok(());
        }

        let frame = BufferedFrame {
            samples: samples.to_vec(),
            duration_ms,
        };
        let frame_start_ms = self.elapsed_ms;
        let frame_end_ms = frame_start_ms.saturating_add(duration_ms);
        let dbfs = dbfs_from_samples(samples);
        let above_threshold = dbfs >= self.config.settings.threshold_dbfs;
        let decision = self.gate.observe_frame(dbfs, frame_start_ms, frame_end_ms);

        match decision {
            GateDecision::Idle => {
                self.push_pre_roll(frame);
            }
            GateDecision::Start { started_at_ms } => {
                self.active_started_at_ms = Some(started_at_ms);
                self.active_samples.clear();
                self.active_has_signal = true;
                self.drain_pre_roll_into_active(started_at_ms);
                self.active_samples.extend_from_slice(&frame.samples);
            }
            GateDecision::Recording => {
                if self.gate.is_recording() {
                    self.active_has_signal |= above_threshold;
                    self.active_samples.extend_from_slice(&frame.samples);
                } else {
                    self.push_pre_roll(frame);
                }
            }
            GateDecision::End { ended_at_ms } => {
                self.active_samples.extend_from_slice(&frame.samples);
                self.commit_active_chunk(ended_at_ms)?;
                self.push_pre_roll(frame);
            }
            GateDecision::Rollover {
                ended_at_ms,
                next_started_at_ms,
            } => {
                self.active_samples.extend_from_slice(&frame.samples);
                self.commit_active_chunk(ended_at_ms)?;
                self.active_started_at_ms = Some(next_started_at_ms);
                self.active_samples.clear();
                self.active_has_signal = false;
            }
        }

        self.elapsed_ms = frame_end_ms;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), RecorderError> {
        if self.gate.is_recording() && !self.active_samples.is_empty() {
            self.commit_active_chunk(self.elapsed_ms)?;
        }
        Ok(())
    }

    pub fn completed_chunks(&self) -> &[RecordedChunk] {
        &self.completed_chunks
    }

    pub fn is_recording(&self) -> bool {
        self.gate.is_recording()
    }

    fn reconfigure(&mut self, config: CaptureProcessorConfig) {
        self.config = config;
        self.gate = SpeechGate::new(GateConfig {
            threshold_dbfs: self.config.settings.threshold_dbfs,
            attack_ms: self.config.settings.attack_ms,
            release_ms: self.config.settings.release_ms,
            pre_roll_ms: self.config.settings.pre_roll_ms,
            post_roll_ms: self.config.settings.post_roll_ms,
            min_chunk_ms: self.config.settings.min_chunk_ms,
            max_chunk_ms: self.config.settings.max_chunk_ms,
        });
        self.pre_roll.clear();
        self.active_samples.clear();
        self.active_started_at_ms = None;
        self.active_has_signal = false;
    }

    fn push_pre_roll(&mut self, frame: BufferedFrame) {
        self.pre_roll.push_back(frame);
        let mut total_ms: u64 = self.pre_roll.iter().map(|frame| frame.duration_ms).sum();
        // Cap includes attack window so pre-onset frames aren't evicted by Idle frames pushed during attack.
        let cap_ms = self
            .config
            .settings
            .pre_roll_ms
            .saturating_add(self.config.settings.attack_ms);
        while total_ms > cap_ms {
            let Some(removed) = self.pre_roll.pop_front() else {
                break;
            };
            total_ms = total_ms.saturating_sub(removed.duration_ms);
        }
    }

    fn drain_pre_roll_into_active(&mut self, _started_at_ms: u64) {
        while let Some(frame) = self.pre_roll.pop_front() {
            self.active_samples.extend_from_slice(&frame.samples);
        }
    }

    fn commit_active_chunk(&mut self, ended_at_ms: u64) -> Result<(), RecorderError> {
        let started_at_ms = self.active_started_at_ms.unwrap_or(self.elapsed_ms);
        if self.active_samples.is_empty() || !self.active_has_signal {
            self.active_samples.clear();
            self.active_started_at_ms = None;
            self.active_has_signal = false;
            return Ok(());
        }

        let chunk = Recorder::write_chunk(RecordingRequest {
            save_root: &self.config.save_root,
            settings: &self.config.settings,
            samples: &self.active_samples,
            sample_rate: self.config.sample_rate,
            started_at: self.time_at(started_at_ms),
            ended_at: self.time_at(ended_at_ms),
            device_id: &self.config.device_id,
            device_name: &self.config.device_name,
            used_fallback_device: self.config.used_fallback_device,
            transcription_enabled: self.config.settings.transcription_enabled,
            app_version: &self.config.app_version,
        })?;

        self.completed_chunks.push(chunk);
        self.active_samples.clear();
        self.active_started_at_ms = None;
        self.active_has_signal = false;
        Ok(())
    }

    fn time_at(&self, offset_ms: u64) -> DateTime<Utc> {
        self.config.base_time + Duration::milliseconds(offset_ms as i64)
    }
}

fn processor_config(config: &CaptureControllerConfig) -> CaptureProcessorConfig {
    CaptureProcessorConfig {
        save_root: config.save_root.clone(),
        settings: config.settings.clone(),
        sample_rate: config.sample_rate,
        device_id: config.device_id.clone(),
        device_name: config.device_name.clone(),
        used_fallback_device: config.used_fallback_device,
        base_time: config.base_time,
        app_version: config.app_version.clone(),
    }
}
