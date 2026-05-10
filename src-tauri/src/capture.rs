use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use crate::audio::{GateConfig, GateDecision, SpeechGate, dbfs_from_samples};
use crate::recorder::{RecordedChunk, Recorder, RecorderError, RecordingRequest};
use crate::settings::AppSettings;

/// How often the live transcription tap fires while a chunk is recording.
/// Smaller values = snappier preview, larger values = lighter CPU. Whisper
/// medium-model partial decodes typically take ~1.5–3 s on Apple Silicon, so
/// 2.5 s avoids stacking work the worker can't keep up with.
const LIVE_PARTIAL_INTERVAL_MS: u64 = 2_500;

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

#[derive(Debug, Clone, PartialEq)]
pub enum CaptureControllerEvent {
    ChunkStarted {
        chunk_id: u64,
        started_at: DateTime<Utc>,
    },
    LiveSamplesReady {
        chunk_id: u64,
        sample_rate: u32,
        samples: Arc<Vec<f32>>,
    },
    ChunkCompleted {
        chunk_id: u64,
        chunk: RecordedChunk,
    },
}

#[derive(Debug)]
pub struct CaptureController {
    config: CaptureControllerConfig,
    processor: CaptureProcessor,
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
    next_chunk_id: u64,
    current_chunk_id: Option<u64>,
    last_partial_emit_offset_ms: u64,
    completed_chunks: Vec<RecordedChunk>,
    pending_events: Vec<CaptureControllerEvent>,
}

impl CaptureController {
    pub fn new(config: CaptureControllerConfig) -> Self {
        let processor = CaptureProcessor::new(processor_config(&config));
        Self { config, processor }
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
        self.processor.take_pending_events()
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
            next_chunk_id: 1,
            current_chunk_id: None,
            last_partial_emit_offset_ms: 0,
            completed_chunks: Vec::new(),
            pending_events: Vec::new(),
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
                let chunk_id = self.next_chunk_id;
                self.next_chunk_id = self.next_chunk_id.saturating_add(1);
                self.current_chunk_id = Some(chunk_id);
                self.active_started_at_ms = Some(started_at_ms);
                self.active_samples.clear();
                self.active_has_signal = true;
                self.last_partial_emit_offset_ms = 0;
                self.drain_pre_roll_into_active(started_at_ms);
                self.active_samples.extend_from_slice(&frame.samples);

                self.pending_events
                    .push(CaptureControllerEvent::ChunkStarted {
                        chunk_id,
                        started_at: self.time_at(started_at_ms),
                    });
                self.maybe_emit_live_partial();
            }
            GateDecision::Recording => {
                if self.gate.is_recording() {
                    self.active_has_signal |= above_threshold;
                    self.active_samples.extend_from_slice(&frame.samples);
                    self.maybe_emit_live_partial();
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
                let overlap_samples = self.rollover_overlap_samples();
                let overlap_started_at_ms = next_started_at_ms.saturating_sub(active_duration_ms(
                    overlap_samples.len(),
                    self.config.sample_rate,
                ));
                self.commit_active_chunk(ended_at_ms)?;
                let chunk_id = self.next_chunk_id;
                self.next_chunk_id = self.next_chunk_id.saturating_add(1);
                self.current_chunk_id = Some(chunk_id);
                self.active_started_at_ms = Some(overlap_started_at_ms);
                self.active_samples = overlap_samples;
                self.active_has_signal = false;
                self.last_partial_emit_offset_ms = 0;

                self.pending_events
                    .push(CaptureControllerEvent::ChunkStarted {
                        chunk_id,
                        started_at: self.time_at(overlap_started_at_ms),
                    });
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

    pub fn pending_events(&self) -> &[CaptureControllerEvent] {
        &self.pending_events
    }

    pub fn take_pending_events(&mut self) -> Vec<CaptureControllerEvent> {
        std::mem::take(&mut self.pending_events)
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
        self.current_chunk_id = None;
        self.last_partial_emit_offset_ms = 0;
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

    fn rollover_overlap_samples(&self) -> Vec<f32> {
        let overlap_count =
            sample_count_for_duration_ms(self.config.settings.pre_roll_ms, self.config.sample_rate);
        if overlap_count == 0 || self.active_samples.is_empty() {
            return Vec::new();
        }

        let keep_count = overlap_count.min(self.active_samples.len());
        self.active_samples[self.active_samples.len() - keep_count..].to_vec()
    }

    fn maybe_emit_live_partial(&mut self) {
        let Some(chunk_id) = self.current_chunk_id else {
            return;
        };
        let active_duration_ms =
            active_duration_ms(self.active_samples.len(), self.config.sample_rate);
        if active_duration_ms < LIVE_PARTIAL_INTERVAL_MS {
            return;
        }
        if active_duration_ms.saturating_sub(self.last_partial_emit_offset_ms)
            < LIVE_PARTIAL_INTERVAL_MS
        {
            return;
        }
        self.last_partial_emit_offset_ms = active_duration_ms;
        self.pending_events
            .push(CaptureControllerEvent::LiveSamplesReady {
                chunk_id,
                sample_rate: self.config.sample_rate,
                samples: Arc::new(self.active_samples.clone()),
            });
    }

    fn commit_active_chunk(&mut self, ended_at_ms: u64) -> Result<(), RecorderError> {
        let started_at_ms = self.active_started_at_ms.unwrap_or(self.elapsed_ms);
        let chunk_id = self.current_chunk_id;
        if self.active_samples.is_empty() || !self.active_has_signal {
            self.active_samples.clear();
            self.active_started_at_ms = None;
            self.active_has_signal = false;
            self.current_chunk_id = None;
            self.last_partial_emit_offset_ms = 0;
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
            live_capture_chunk_id: chunk_id,
        })?;

        self.completed_chunks.push(chunk.clone());
        if let Some(chunk_id) = chunk_id {
            self.pending_events
                .push(CaptureControllerEvent::ChunkCompleted { chunk_id, chunk });
        }
        self.active_samples.clear();
        self.active_started_at_ms = None;
        self.active_has_signal = false;
        self.current_chunk_id = None;
        self.last_partial_emit_offset_ms = 0;
        Ok(())
    }

    fn time_at(&self, offset_ms: u64) -> DateTime<Utc> {
        self.config.base_time + Duration::milliseconds(offset_ms as i64)
    }
}

fn active_duration_ms(sample_count: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    ((sample_count as u128 * 1_000) / sample_rate as u128) as u64
}

fn sample_count_for_duration_ms(duration_ms: u64, sample_rate: u32) -> usize {
    if duration_ms == 0 || sample_rate == 0 {
        return 0;
    }
    ((duration_ms as u128 * sample_rate as u128) / 1_000) as usize
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
