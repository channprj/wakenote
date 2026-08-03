use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

use chrono::{DateTime, Utc};

use crate::meeting::{MeetingCaptureRecorder, MeetingRecord};

pub const MANUAL_MEETING_SAMPLE_RATE: u32 = 16_000;
const FRAME_QUEUE_CAPACITY: usize = 256;
const MIX_JITTER_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualMeetingSource {
    Microphone,
    System,
}

#[derive(Debug, Clone)]
pub struct ManualMeetingFrame {
    pub source: ManualMeetingSource,
    pub captured_at: DateTime<Utc>,
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

#[derive(Debug)]
pub struct ManualMeetingMixer {
    started_at: DateTime<Utc>,
    cursor: u64,
    sums: Vec<f32>,
    counts: Vec<u8>,
    microphone_end: Option<u64>,
    system_end: Option<u64>,
    dropped_late_frames: u64,
}

impl ManualMeetingMixer {
    pub fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            cursor: 0,
            sums: Vec::new(),
            counts: Vec::new(),
            microphone_end: None,
            system_end: None,
            dropped_late_frames: 0,
        }
    }

    pub fn push(&mut self, frame: ManualMeetingFrame) -> Vec<f32> {
        let samples = resample(
            &frame.samples,
            frame.sample_rate,
            MANUAL_MEETING_SAMPLE_RATE,
        );
        if samples.is_empty() {
            return Vec::new();
        }
        let offset_us = frame
            .captured_at
            .signed_duration_since(self.started_at)
            .num_microseconds()
            .unwrap_or(0)
            .max(0) as u64;
        let mut start = offset_us.saturating_mul(MANUAL_MEETING_SAMPLE_RATE as u64) / 1_000_000;
        let mut samples = samples.as_slice();
        if start < self.cursor {
            let late = (self.cursor - start) as usize;
            if late >= samples.len() {
                self.dropped_late_frames = self.dropped_late_frames.saturating_add(1);
                return Vec::new();
            }
            samples = &samples[late..];
            start = self.cursor;
            self.dropped_late_frames = self.dropped_late_frames.saturating_add(1);
        }
        let relative_start = (start - self.cursor) as usize;
        let required = relative_start.saturating_add(samples.len());
        if self.sums.len() < required {
            self.sums.resize(required, 0.0);
            self.counts.resize(required, 0);
        }
        for (index, sample) in samples.iter().enumerate() {
            let target = relative_start + index;
            self.sums[target] += sample.clamp(-1.0, 1.0);
            self.counts[target] = self.counts[target].saturating_add(1);
        }
        let end = start.saturating_add(samples.len() as u64);
        match frame.source {
            ManualMeetingSource::Microphone => self.microphone_end = Some(end),
            ManualMeetingSource::System => self.system_end = Some(end),
        }
        self.commit_ready(false)
    }

    pub fn finish(&mut self) -> Vec<f32> {
        self.commit_ready(true)
    }

    pub fn dropped_late_frames(&self) -> u64 {
        self.dropped_late_frames
    }

    fn commit_ready(&mut self, flush: bool) -> Vec<f32> {
        let furthest = self
            .microphone_end
            .into_iter()
            .chain(self.system_end)
            .max()
            .unwrap_or(self.cursor);
        let threshold = if flush {
            furthest
        } else {
            let jitter = MIX_JITTER_MS.saturating_mul(MANUAL_MEETING_SAMPLE_RATE as u64) / 1_000;
            match (self.microphone_end, self.system_end) {
                (Some(microphone), Some(system)) => microphone.min(system).saturating_sub(jitter),
                _ => self.cursor,
            }
        };
        let count = threshold
            .saturating_sub(self.cursor)
            .min(self.sums.len() as u64) as usize;
        if count == 0 {
            return Vec::new();
        }
        let mixed = self
            .sums
            .drain(..count)
            .zip(self.counts.drain(..count))
            .map(|(sum, count)| {
                if count == 0 {
                    0.0
                } else {
                    (sum / count as f32).clamp(-1.0, 1.0)
                }
            })
            .collect();
        self.cursor = self.cursor.saturating_add(count as u64);
        mixed
    }
}

pub struct ManualMeetingWriter {
    tx: SyncSender<WriterCommand>,
    overflowed: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

enum WriterCommand {
    Frame(ManualMeetingFrame),
    Finish(mpsc::Sender<Result<MeetingRecord, String>>),
    Abort(mpsc::Sender<Result<(), String>>),
}

impl ManualMeetingWriter {
    pub fn start(recorder: MeetingCaptureRecorder, started_at: DateTime<Utc>) -> Self {
        let (tx, rx) = mpsc::sync_channel(FRAME_QUEUE_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let join = thread::spawn(move || {
            let mut recorder = Some(recorder);
            let mut mixer = ManualMeetingMixer::new(started_at);
            while let Ok(command) = rx.recv() {
                match command {
                    WriterCommand::Frame(frame) => {
                        let mixed = mixer.push(frame);
                        if !mixed.is_empty()
                            && let Some(recorder) = recorder.as_mut()
                            && recorder.write_samples(&mixed).is_err()
                        {
                            break;
                        }
                    }
                    WriterCommand::Finish(response) => {
                        let result = recorder
                            .take()
                            .ok_or_else(|| "manual meeting recorder is unavailable".to_string())
                            .and_then(|mut recorder| {
                                let tail = mixer.finish();
                                recorder.write_samples(&tail)?;
                                recorder.finish()
                            });
                        let _ = response.send(result);
                        break;
                    }
                    WriterCommand::Abort(response) => {
                        let result = recorder
                            .take()
                            .map(MeetingCaptureRecorder::discard)
                            .unwrap_or(Ok(()));
                        let _ = response.send(result);
                        break;
                    }
                }
            }
        });
        Self {
            tx,
            overflowed,
            join: Some(join),
        }
    }

    pub fn sender(&self, source: ManualMeetingSource) -> ManualMeetingFrameSender {
        ManualMeetingFrameSender {
            source,
            tx: self.tx.clone(),
            overflowed: self.overflowed.clone(),
        }
    }

    pub fn overflowed(&self) -> bool {
        self.overflowed.load(Ordering::Acquire)
    }

    pub fn finish(mut self) -> Result<MeetingRecord, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(WriterCommand::Finish(response_tx))
            .map_err(|_| "manual meeting writer stopped unexpectedly".to_string())?;
        let result = response_rx
            .recv()
            .map_err(|_| "manual meeting writer did not return a result".to_string())?;
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        result
    }

    pub fn abort(mut self) -> Result<(), String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(WriterCommand::Abort(response_tx))
            .map_err(|_| "manual meeting writer stopped unexpectedly".to_string())?;
        let result = response_rx
            .recv()
            .map_err(|_| "manual meeting writer did not acknowledge abort".to_string())?;
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        result
    }
}

#[derive(Clone)]
pub struct ManualMeetingFrameSender {
    source: ManualMeetingSource,
    tx: SyncSender<WriterCommand>,
    overflowed: Arc<AtomicBool>,
}

impl ManualMeetingFrameSender {
    pub fn try_send(&self, frame: crate::live_capture::AudioFrame, sample_rate: u32) {
        let frame = ManualMeetingFrame {
            source: self.source,
            captured_at: frame.captured_at,
            sample_rate,
            samples: frame.samples,
        };
        if let Err(error) = self.tx.try_send(WriterCommand::Frame(frame))
            && matches!(error, TrySendError::Full(_))
        {
            self.overflowed.store(true, Ordering::Release);
        }
    }
}

fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if samples.is_empty() || source_rate == 0 || target_rate == 0 {
        return Vec::new();
    }
    if source_rate == target_rate {
        return samples.to_vec();
    }
    let output_len = ((samples.len() as u64 * target_rate as u64) / source_rate as u64) as usize;
    (0..output_len)
        .map(|index| {
            let source = index as f64 * source_rate as f64 / target_rate as f64;
            let left = source.floor() as usize;
            let right = (left + 1).min(samples.len() - 1);
            let fraction = (source - left as f64) as f32;
            samples[left] * (1.0 - fraction) + samples[right] * fraction
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn manual_meeting_mixer_aligns_sources_limits_overlap_and_keeps_silence() {
        let started = Utc::now();
        let mut mixer = ManualMeetingMixer::new(started);
        assert!(
            mixer
                .push(ManualMeetingFrame {
                    source: ManualMeetingSource::Microphone,
                    captured_at: started,
                    sample_rate: 1_000,
                    samples: vec![0.8; 1_000],
                })
                .is_empty()
        );
        let mixed = mixer.push(ManualMeetingFrame {
            source: ManualMeetingSource::System,
            captured_at: started,
            sample_rate: 1_000,
            samples: vec![0.8; 1_000],
        });
        assert_eq!(mixed.len(), 12_000);
        assert!(mixed.iter().all(|sample| *sample <= 1.0));
        assert!(mixed.iter().all(|sample| (*sample - 0.8).abs() < 0.001));

        let _ = mixer.push(ManualMeetingFrame {
            source: ManualMeetingSource::Microphone,
            captured_at: started + TimeDelta::seconds(2),
            sample_rate: 1_000,
            samples: vec![0.5; 500],
        });
        let tail = mixer.finish();
        assert!(tail.contains(&0.0));
    }

    #[test]
    fn manual_meeting_mixer_drops_frames_older_than_committed_cursor() {
        let started = Utc::now();
        let mut mixer = ManualMeetingMixer::new(started);
        for source in [ManualMeetingSource::Microphone, ManualMeetingSource::System] {
            mixer.push(ManualMeetingFrame {
                source,
                captured_at: started,
                sample_rate: MANUAL_MEETING_SAMPLE_RATE,
                samples: vec![0.1; MANUAL_MEETING_SAMPLE_RATE as usize],
            });
        }
        mixer.push(ManualMeetingFrame {
            source: ManualMeetingSource::System,
            captured_at: started,
            sample_rate: MANUAL_MEETING_SAMPLE_RATE,
            samples: vec![0.2; 100],
        });
        assert_eq!(mixer.dropped_late_frames(), 1);
    }
}
