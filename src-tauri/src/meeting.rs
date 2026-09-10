//! Long-form meeting transcription pipeline.
//!
//! This is deliberately separate from the chunk-based near-realtime path
//! (`capture.rs` / `live_transcription.rs`). A meeting is one long uploaded
//! recording (1–2h) stored under its own `meetings/<id>/` directory and
//! transcribed in batch: the file is normalized to a temp 16 kHz WAV, split at
//! silence boundaries into bounded segments, and each segment is decoded with a
//! single reused `WhisperContext`. Results are flushed to `meeting.json` after
//! every segment so a crash or app restart can resume from the first
//! incomplete segment instead of re-running the whole file.

use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Instant;

use chrono::{DateTime, Local, Utc};
use hound::{WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};

use crate::audio_codec::convert_to_pcm_wav;
use crate::cloud_realtime::{RealtimeStoredResult, realtime_result_store};
use crate::cloud_transcription::TranscriptionCredentials;
use crate::dictionary::DictionaryContext;
use crate::recorder::encode_wav_to_m4a;
use crate::settings::TranscriptionLanguage;
use crate::transcription::{
    DecodedWindow, RuntimeTranscriber, SpeakerTurn, Transcriber, TranscriptionError,
    TranscriptionRequest, cached_whisper_context, model_runtime_for_id,
    transcribe_samples_with_context,
};
use crate::transcription_cost::estimated_provider_cost_usd;

/// Frame size used for silence detection. 20 ms is fine-grained enough to find
/// pause boundaries while keeping the RMS envelope small even for 2h files.
const FRAME_MS: u64 = 20;
/// Don't cut a segment shorter than this — whisper needs context for accuracy.
const MIN_SEGMENT_MS: u64 = 30_000;
/// Hard cap so a segment of continuous speech can't grow unbounded; also keeps
/// per-segment progress updates frequent enough to feel live.
const MAX_SEGMENT_MS: u64 = 90_000;
/// Read each segment starting slightly before its cut so a word straddling the
/// boundary isn't clipped. Cuts land in silence, so this overlap is silence and
/// produces no duplicated text.
const OVERLAP_MS: u64 = 800;
/// RMS below this is treated as silence for boundary detection. Slightly above
/// the `MIN_TRANSCRIBABLE_RMS` floor in `transcription.rs`.
const SILENCE_RMS_THRESHOLD: f32 = 0.005;

// ---------------------------------------------------------------------------
// Data model (persisted to meeting.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Recorded,
    Pending,
    Processing,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingSegmentStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingPhase {
    PreparingAudio,
    SplittingAudio,
    Transcribing,
    Finalizing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingProgress {
    #[serde(default)]
    pub phase: Option<MeetingPhase>,
    pub segments_total: usize,
    pub segments_done: usize,
    pub processed_ms: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingSegment {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub status: MeetingSegmentStatus,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub no_speech: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingTranscriptionRequest {
    pub model_id: String,
    pub language: TranscriptionLanguage,
    #[serde(default = "crate::settings::default_transcription_language_hints")]
    pub language_hints: Vec<TranscriptionLanguage>,
    pub streaming_enabled: bool,
    pub speaker_separation_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingRecord {
    pub id: String,
    pub title: String,
    pub source_filename: String,
    /// Audio filename relative to the meeting directory (e.g. `audio.m4a`).
    pub audio_file: String,
    pub audio_format: String,
    pub model_id: String,
    pub language: TranscriptionLanguage,
    pub app_version: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub status: MeetingStatus,
    pub progress: MeetingProgress,
    #[serde(default)]
    pub failed_phase: Option<MeetingPhase>,
    #[serde(default)]
    pub failed_segments: usize,
    pub segments: Vec<MeetingSegment>,
    #[serde(default)]
    pub transcription_request: Option<MeetingTranscriptionRequest>,
    #[serde(default)]
    pub speaker_turns: Vec<SpeakerTurn>,
    #[serde(default)]
    pub api_audio_duration_ms: u64,
    #[serde(default)]
    pub api_cost_microusd: u64,
    #[serde(default)]
    pub api_request_count: u64,
    #[serde(default)]
    pub api_unpriced_request_count: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub capture_warning: Option<String>,
}

impl MeetingRecord {
    fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, path)
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn recompute_progress(&mut self) {
        self.progress.segments_total = self.segments.len();
        self.progress.segments_done = self
            .segments
            .iter()
            .filter(|segment| segment.status == MeetingSegmentStatus::Completed)
            .count();
        self.progress.processed_ms = self
            .segments
            .iter()
            .filter(|segment| segment.status == MeetingSegmentStatus::Completed)
            .map(|segment| segment.end_ms.saturating_sub(segment.start_ms))
            .sum();
    }

    /// Time-ordered transcript built from completed segments only.
    pub fn transcript_text(&self) -> String {
        if !self.speaker_turns.is_empty() {
            let mut turns = self.speaker_turns.iter().collect::<Vec<_>>();
            turns.sort_by_key(|turn| (turn.part_index, turn.start_ms));
            return turns
                .into_iter()
                .filter_map(|turn| {
                    let text = turn.text.trim();
                    (!text.is_empty()).then(|| {
                        format!(
                            "[{}] Part {} · {}: {}",
                            format_timestamp(turn.start_ms),
                            turn.part_index + 1,
                            turn.speaker_id,
                            text
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        self.segments
            .iter()
            .filter(|segment| segment.status == MeetingSegmentStatus::Completed)
            .map(|segment| segment.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn summary(&self) -> MeetingSummary {
        MeetingSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            source_filename: self.source_filename.clone(),
            status: self.status,
            duration_ms: self.duration_ms,
            created_at: self.created_at,
            updated_at: self.updated_at,
            progress: self.progress.clone(),
            model_id: self.model_id.clone(),
            language: self.language,
            error: self.error.clone(),
            capture_warning: self.capture_warning.clone(),
        }
    }
}

fn format_timestamp(milliseconds: u64) -> String {
    let total_seconds = milliseconds / 1_000;
    format!(
        "{:02}:{:02}:{:02}",
        total_seconds / 3_600,
        (total_seconds % 3_600) / 60,
        total_seconds % 60
    )
}

/// Lightweight projection of a meeting for list views (no segment bodies).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub source_filename: String,
    pub status: MeetingStatus,
    pub duration_ms: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub progress: MeetingProgress,
    pub model_id: String,
    pub language: TranscriptionLanguage,
    pub error: Option<String>,
    pub capture_warning: Option<String>,
}

/// Full meeting plus its rendered transcript and absolute audio path.
#[derive(Debug, Clone, Serialize)]
pub struct MeetingDetail {
    pub record: MeetingRecord,
    pub transcript: String,
    pub audio_path: String,
}

// ---------------------------------------------------------------------------
// Events (mapped to Tauri emit by the caller)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct MeetingProgressEvent {
    pub id: String,
    pub status: MeetingStatus,
    pub phase: MeetingPhase,
    pub segments_total: usize,
    pub segments_done: usize,
    pub processed_ms: u64,
    pub duration_ms: u64,
    pub elapsed_ms: u64,
    pub eta_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingSegmentEvent {
    pub id: String,
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingFinishedEvent {
    pub id: String,
    pub status: MeetingStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MeetingEvent {
    Progress(MeetingProgressEvent),
    SegmentCommitted(MeetingSegmentEvent),
    Finished(MeetingFinishedEvent),
}

/// Sink for meeting events. `Arc<dyn Fn>` (mirroring `live_transcription`) so the
/// `'static` whisper progress closure can clone it for in-segment updates.
pub type MeetingEventCallback = Arc<dyn Fn(MeetingEvent) + Send + Sync>;

#[derive(Clone)]
pub struct MeetingRealtimeReplay {
    submit: Arc<dyn Fn(MeetingRealtimeReplayRequest) -> Result<(), String> + Send + Sync>,
    close: Arc<dyn Fn(String) + Send + Sync>,
}

impl MeetingRealtimeReplay {
    pub fn new(
        submit: Arc<dyn Fn(MeetingRealtimeReplayRequest) -> Result<(), String> + Send + Sync>,
        close: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        Self { submit, close }
    }

    pub fn submit(&self, request: MeetingRealtimeReplayRequest) -> Result<(), String> {
        (self.submit)(request)
    }

    pub fn close_source(&self, source_key: String) {
        (self.close)(source_key);
    }
}

#[derive(Clone)]
pub struct MeetingRealtimeReplayRequest {
    pub source_key: String,
    pub source_label: String,
    pub chunk_id: u64,
    pub model_id: String,
    pub language: TranscriptionLanguage,
    pub language_hints: Vec<TranscriptionLanguage>,
    pub dictionary: DictionaryContext,
    pub sample_rate: u32,
    pub samples: Arc<Vec<f32>>,
    pub credentials: TranscriptionCredentials,
    pub audio_path: PathBuf,
}

// ---------------------------------------------------------------------------
// Paths & ids
// ---------------------------------------------------------------------------

pub fn meetings_root(save_root: &Path) -> PathBuf {
    save_root.join("meetings")
}

pub fn meeting_dir(save_root: &Path, id: &str) -> PathBuf {
    meetings_root(save_root).join(id)
}

fn record_path(dir: &Path) -> PathBuf {
    dir.join("meeting.json")
}

fn transcript_path(dir: &Path) -> PathBuf {
    dir.join("transcript.txt")
}

fn work_wav_path(dir: &Path) -> PathBuf {
    dir.join(".audio16k.wav")
}

fn capture_wav_path(dir: &Path) -> PathBuf {
    dir.join(".audio-capture.wav")
}

fn partial_m4a_path(dir: &Path) -> PathBuf {
    dir.join(".audio-encoding.m4a")
}

/// Meeting ids only ever contain `[A-Za-z0-9-]`; reject anything else so a
/// caller-supplied id can never escape the meetings directory.
fn is_valid_meeting_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn slugify(value: &str) -> Option<String> {
    let mut out = String::new();
    let mut pending_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.chars().take(48).collect())
    }
}

fn allocate_meeting_id(
    save_root: &Path,
    timestamp: DateTime<Local>,
    slug: Option<&str>,
) -> std::io::Result<String> {
    let base = match slug {
        Some(slug) => format!("{}-{slug}", timestamp.format("%Y%m%d-%H%M%S")),
        None => timestamp.format("%Y%m%d-%H%M%S").to_string(),
    };
    let root = meetings_root(save_root);
    fs::create_dir_all(&root)?;
    for index in 0..1_000 {
        let id = if index == 0 {
            base.clone()
        } else {
            format!("{base}-{}", index + 1)
        };
        if !root.join(&id).exists() {
            return Ok(id);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no available meeting id after 999 collisions",
    ))
}

// ---------------------------------------------------------------------------
// Segmentation (pure, unit-tested)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct SegmentPlanOptions {
    pub min_segment_ms: u64,
    pub max_segment_ms: u64,
    pub silence_rms_threshold: f32,
}

impl Default for SegmentPlanOptions {
    fn default() -> Self {
        Self {
            min_segment_ms: MIN_SEGMENT_MS,
            max_segment_ms: MAX_SEGMENT_MS,
            silence_rms_threshold: SILENCE_RMS_THRESHOLD,
        }
    }
}

/// Split `[0, total_ms]` into non-overlapping segments, preferring cuts at the
/// center of the longest silence run inside each `[min, max]` window. Falls
/// back to a hard cut at `max` when the window contains no silence.
pub fn plan_segments(
    frame_rms: &[f32],
    frame_ms: u64,
    total_ms: u64,
    opts: SegmentPlanOptions,
) -> Vec<(u64, u64)> {
    if total_ms == 0 {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut start = 0u64;
    while total_ms - start > opts.max_segment_ms {
        let win_lo = start + opts.min_segment_ms;
        let win_hi = (start + opts.max_segment_ms).min(total_ms);
        let cut = best_silence_cut(
            frame_rms,
            frame_ms,
            opts.silence_rms_threshold,
            win_lo,
            win_hi,
        )
        .unwrap_or(win_hi)
        .clamp(win_lo, total_ms)
        .max(start + 1);
        segments.push((start, cut));
        start = cut;
    }
    segments.push((start, total_ms));
    segments
}

/// Center (ms) of the longest silence run within `[lo_ms, hi_ms)`, or `None` if
/// no frame in that window is below the threshold.
fn best_silence_cut(
    frame_rms: &[f32],
    frame_ms: u64,
    threshold: f32,
    lo_ms: u64,
    hi_ms: u64,
) -> Option<u64> {
    if frame_rms.is_empty() || frame_ms == 0 || hi_ms <= lo_ms {
        return None;
    }
    let lo = (lo_ms / frame_ms) as usize;
    let hi = ((hi_ms / frame_ms) as usize).min(frame_rms.len());
    if lo >= hi {
        return None;
    }

    let mut best_len = 0usize;
    let mut best_center: Option<usize> = None;
    let mut run_start: Option<usize> = None;
    let consider =
        |start: usize, end: usize, best_len: &mut usize, best_center: &mut Option<usize>| {
            let len = end - start;
            if len > *best_len {
                *best_len = len;
                *best_center = Some((start + end) / 2);
            }
        };
    for (index, rms) in frame_rms.iter().enumerate().take(hi).skip(lo) {
        if *rms < threshold {
            run_start.get_or_insert(index);
        } else if let Some(start) = run_start.take() {
            consider(start, index, &mut best_len, &mut best_center);
        }
    }
    if let Some(start) = run_start.take() {
        consider(start, hi, &mut best_len, &mut best_center);
    }
    best_center.map(|center| center as u64 * frame_ms)
}

/// Whole-seconds-aware ETA: `elapsed * remaining / processed`, guarding zeros.
pub fn compute_eta_ms(elapsed_ms: u64, processed_ms: u64, remaining_ms: u64) -> u64 {
    if processed_ms == 0 || elapsed_ms == 0 {
        return 0;
    }
    ((elapsed_ms as u128 * remaining_ms as u128) / processed_ms as u128) as u64
}

// ---------------------------------------------------------------------------
// WAV helpers (streaming — never holds the whole file in memory)
// ---------------------------------------------------------------------------

fn hound_io(error: hound::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

/// Convert any supported input to a temp 16 kHz signed-16 WAV. Channels are
/// preserved and downmixed later to match the ordinary transcription decoder.
fn normalize_to_wav16k(src: &Path, dst: &Path) -> Result<(), String> {
    convert_to_pcm_wav(src, dst, 16_000, None)
}

/// Per-frame RMS envelope plus the precise total duration in ms.
fn compute_frame_rms(wav_path: &Path, frame_ms: u64) -> std::io::Result<(Vec<f32>, u64)> {
    let mut reader = WavReader::open(wav_path).map_err(hound_io)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let sample_rate = spec.sample_rate.max(1);
    let total_ms = reader.duration() as u64 * 1000 / sample_rate as u64;
    let frame_values = ((sample_rate as u64 * frame_ms / 1000) as usize).max(1) * channels;

    let mut rms = Vec::new();
    let mut acc = 0f64;
    let mut count = 0usize;
    for sample in reader.samples::<i16>() {
        let value = sample.map_err(hound_io)? as f64 / i16::MAX as f64;
        acc += value * value;
        count += 1;
        if count >= frame_values {
            rms.push((acc / count as f64).sqrt() as f32);
            acc = 0.0;
            count = 0;
        }
    }
    if count > 0 {
        rms.push((acc / count as f64).sqrt() as f32);
    }
    Ok((rms, total_ms))
}

/// Read one window as 16 kHz mono f32 samples without loading the whole file.
fn read_window_samples(wav_path: &Path, start_ms: u64, end_ms: u64) -> std::io::Result<Vec<f32>> {
    let mut reader = WavReader::open(wav_path).map_err(hound_io)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let sample_rate = spec.sample_rate.max(1) as u64;
    let start_frame = (start_ms.saturating_mul(sample_rate) / 1000) as u32;
    let end_frame = (end_ms.saturating_mul(sample_rate) / 1000) as u32;
    let frame_count = end_frame.saturating_sub(start_frame) as usize;
    reader.seek(start_frame)?;

    let mut mono = Vec::with_capacity(frame_count);
    let mut frame_acc = 0f32;
    let mut channel_index = 0usize;
    for sample in reader.samples::<i16>().take(frame_count * channels) {
        let value = sample.map_err(hound_io)? as f32 / i16::MAX as f32;
        frame_acc += value;
        channel_index += 1;
        if channel_index == channels {
            mono.push(frame_acc / channels as f32);
            frame_acc = 0.0;
            channel_index = 0;
        }
    }
    Ok(mono)
}

fn write_samples_wav16k(path: &Path, samples: &[f32]) -> Result<(), String> {
    let mut writer = WavWriter::create(
        path,
        WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|error| error.to_string())?;
    for sample in samples {
        let encoded = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        writer
            .write_sample(encoded)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())
}

fn write_transcript(dir: &Path, record: &MeetingRecord) -> std::io::Result<()> {
    fs::write(transcript_path(dir), record.transcript_text())
}

// ---------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------

pub struct MeetingCaptureRecorder {
    dir: PathBuf,
    record: MeetingRecord,
    writer: Option<WavWriter<BufWriter<fs::File>>>,
    sample_rate: u32,
    audio_bitrate_kbps: u32,
    samples_written: u64,
}

impl std::fmt::Debug for MeetingCaptureRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeetingCaptureRecorder")
            .field("id", &self.record.id)
            .field("dir", &self.dir)
            .field("sample_rate", &self.sample_rate)
            .field("audio_bitrate_kbps", &self.audio_bitrate_kbps)
            .field("samples_written", &self.samples_written)
            .finish()
    }
}

#[allow(clippy::too_many_arguments)]
pub fn start_recorded_meeting_capture(
    save_root: &Path,
    title: &str,
    source_filename: &str,
    model_id: &str,
    language: TranscriptionLanguage,
    app_version: &str,
    sample_rate: u32,
    audio_bitrate_kbps: u32,
    timestamp: DateTime<Local>,
) -> Result<MeetingCaptureRecorder, String> {
    if sample_rate == 0 {
        return Err("meeting capture sample rate must be greater than zero".to_string());
    }
    let slug = slugify(title);
    let id =
        allocate_meeting_id(save_root, timestamp, slug.as_deref()).map_err(|e| e.to_string())?;
    let dir = meeting_dir(save_root, &id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let audio_file = "audio.m4a".to_string();
    let writer = WavWriter::create(
        capture_wav_path(&dir),
        WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|e| e.to_string())?;

    let now = timestamp.with_timezone(&Utc);
    let record = MeetingRecord {
        id,
        title: title.to_string(),
        source_filename: source_filename.to_string(),
        audio_file,
        audio_format: "m4a".to_string(),
        model_id: model_id.to_string(),
        language,
        app_version: app_version.to_string(),
        created_at: now,
        updated_at: now,
        duration_ms: 0,
        status: MeetingStatus::Pending,
        progress: MeetingProgress::default(),
        failed_phase: None,
        failed_segments: 0,
        segments: Vec::new(),
        transcription_request: None,
        speaker_turns: Vec::new(),
        api_audio_duration_ms: 0,
        api_cost_microusd: 0,
        api_request_count: 0,
        api_unpriced_request_count: 0,
        error: None,
        capture_warning: None,
    };

    Ok(MeetingCaptureRecorder {
        dir,
        record,
        writer: Some(writer),
        sample_rate,
        audio_bitrate_kbps,
        samples_written: 0,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn start_manual_recorded_meeting_capture(
    save_root: &Path,
    title: &str,
    source_filename: &str,
    model_id: &str,
    language: TranscriptionLanguage,
    app_version: &str,
    sample_rate: u32,
    audio_bitrate_kbps: u32,
    timestamp: DateTime<Local>,
) -> Result<MeetingCaptureRecorder, String> {
    let mut recorder = start_recorded_meeting_capture(
        save_root,
        title,
        source_filename,
        model_id,
        language,
        app_version,
        sample_rate,
        audio_bitrate_kbps,
        timestamp,
    )?;
    recorder.record.status = MeetingStatus::Recorded;
    Ok(recorder)
}

impl MeetingCaptureRecorder {
    pub fn id(&self) -> &str {
        &self.record.id
    }

    pub fn is_empty(&self) -> bool {
        self.samples_written == 0
    }

    pub fn set_capture_warning(&mut self, warning: impl Into<String>) {
        self.record.capture_warning = Some(warning.into());
    }

    pub fn write_samples(&mut self, samples: &[f32]) -> Result<(), String> {
        let Some(writer) = self.writer.as_mut() else {
            return Err("meeting capture writer is already finalized".to_string());
        };
        for sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            writer
                .write_sample((clamped * i16::MAX as f32) as i16)
                .map_err(|e| e.to_string())?;
        }
        self.samples_written = self.samples_written.saturating_add(samples.len() as u64);
        Ok(())
    }

    pub fn discard(mut self) -> Result<(), String> {
        if let Some(writer) = self.writer.take() {
            writer.finalize().map_err(|e| e.to_string())?;
        }
        fs::remove_dir_all(&self.dir).map_err(|e| e.to_string())
    }

    pub fn finish(self) -> Result<MeetingRecord, String> {
        self.finish_with_encoder(|source, destination, bitrate_kbps| {
            encode_wav_to_m4a(source, destination, bitrate_kbps).map_err(|error| error.to_string())
        })
    }

    fn finish_with_encoder<F>(mut self, encoder: F) -> Result<MeetingRecord, String>
    where
        F: FnOnce(&Path, &Path, u32) -> Result<(), String>,
    {
        if let Some(writer) = self.writer.take() {
            writer.finalize().map_err(|e| e.to_string())?;
        }
        let capture_path = capture_wav_path(&self.dir);
        let capture_size = fs::metadata(&capture_path)
            .map_err(|error| format!("meeting capture audio is unavailable: {error}"))?
            .len();
        if self.samples_written == 0 || capture_size <= 44 {
            return Err("meeting capture audio is empty".to_string());
        }
        let partial_path = partial_m4a_path(&self.dir);
        let final_path = self.dir.join(&self.record.audio_file);
        if let Err(error) = encoder(&capture_path, &partial_path, self.audio_bitrate_kbps) {
            let _ = fs::remove_file(&partial_path);
            return self.finish_recovered_wav(format!(
                "Meeting audio encoding failed; saved recovery WAV instead: {error}"
            ));
        }
        if let Err(error) = fs::rename(&partial_path, &final_path) {
            let _ = fs::remove_file(&partial_path);
            return self.finish_recovered_wav(format!(
                "Meeting audio finalization failed; saved recovery WAV instead: {error}"
            ));
        }
        let _ = fs::remove_file(&capture_path);
        self.record.duration_ms =
            (self.samples_written as u128 * 1_000 / self.sample_rate as u128) as u64;
        self.record.updated_at = Utc::now();
        self.record
            .save_atomic(&record_path(&self.dir))
            .map_err(|e| e.to_string())?;
        Ok(self.record)
    }

    pub fn finish_recovered_wav(
        mut self,
        warning: impl Into<String>,
    ) -> Result<MeetingRecord, String> {
        if let Some(writer) = self.writer.take() {
            writer.finalize().map_err(|error| error.to_string())?;
        }
        let capture_path = capture_wav_path(&self.dir);
        let recovery_filename = "audio-recovery.wav";
        let recovery_path = self.dir.join(recovery_filename);
        let capture_size = fs::metadata(&capture_path)
            .map_err(|error| format!("meeting recovery audio is unavailable: {error}"))?
            .len();
        if self.samples_written == 0 || capture_size <= 44 {
            return Err("meeting recovery audio is empty".to_string());
        }
        fs::rename(&capture_path, &recovery_path)
            .map_err(|error| format!("could not preserve meeting recovery audio: {error}"))?;
        self.record.audio_file = recovery_filename.to_string();
        self.record.audio_format = "wav".to_string();
        self.record.capture_warning = Some(warning.into());
        self.record.duration_ms =
            (self.samples_written as u128 * 1_000 / self.sample_rate as u128) as u64;
        self.record.updated_at = Utc::now();
        self.record
            .save_atomic(&record_path(&self.dir))
            .map_err(|error| error.to_string())?;
        Ok(self.record)
    }
}

/// Copy an uploaded recording into a fresh `meetings/<id>/` directory and write
/// the initial `meeting.json`. The job itself is started separately.
pub fn import_meeting(
    save_root: &Path,
    source_path: &Path,
    model_id: &str,
    language: TranscriptionLanguage,
    app_version: &str,
    timestamp: DateTime<Local>,
) -> Result<MeetingRecord, String> {
    if !source_path.is_file() {
        return Err("upload source is not a file".to_string());
    }
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(extension.as_str(), "mp3" | "m4a" | "wav") {
        return Err("only mp3, m4a, and wav files can be transcribed".to_string());
    }
    let filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("audio")
        .to_string();
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("audio");
    let slug = slugify(stem);
    let id =
        allocate_meeting_id(save_root, timestamp, slug.as_deref()).map_err(|e| e.to_string())?;
    let dir = meeting_dir(save_root, &id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let audio_file = format!("audio.{extension}");
    fs::copy(source_path, dir.join(&audio_file)).map_err(|e| e.to_string())?;

    let now = timestamp.with_timezone(&Utc);
    let record = MeetingRecord {
        id,
        title: stem.to_string(),
        source_filename: filename,
        audio_file,
        audio_format: extension,
        model_id: model_id.to_string(),
        language,
        app_version: app_version.to_string(),
        created_at: now,
        updated_at: now,
        duration_ms: 0,
        status: MeetingStatus::Recorded,
        progress: MeetingProgress::default(),
        failed_phase: None,
        failed_segments: 0,
        segments: Vec::new(),
        transcription_request: None,
        speaker_turns: Vec::new(),
        api_audio_duration_ms: 0,
        api_cost_microusd: 0,
        api_request_count: 0,
        api_unpriced_request_count: 0,
        error: None,
        capture_warning: None,
    };
    record
        .save_atomic(&record_path(&dir))
        .map_err(|e| e.to_string())?;
    Ok(record)
}

pub fn list_meetings(save_root: &Path) -> Vec<MeetingSummary> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(meetings_root(save_root)) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        if let Ok(record) = MeetingRecord::load(&record_path(&entry.path())) {
            out.push(record.summary());
        }
    }
    out.sort_by_key(|item| std::cmp::Reverse(item.created_at));
    out
}

pub fn meeting_detail(save_root: &Path, id: &str) -> Result<MeetingDetail, String> {
    if !is_valid_meeting_id(id) {
        return Err("invalid meeting id".to_string());
    }
    let dir = meeting_dir(save_root, id);
    let record = MeetingRecord::load(&record_path(&dir)).map_err(|e| e.to_string())?;
    let transcript =
        fs::read_to_string(transcript_path(&dir)).unwrap_or_else(|_| record.transcript_text());
    let audio_path = dir.join(&record.audio_file).to_string_lossy().to_string();
    Ok(MeetingDetail {
        record,
        transcript,
        audio_path,
    })
}

pub fn start_recorded_meeting_transcription(
    save_root: &Path,
    id: &str,
    request: MeetingTranscriptionRequest,
) -> Result<MeetingRecord, String> {
    if !is_valid_meeting_id(id) {
        return Err("invalid meeting id".to_string());
    }
    let dir = meeting_dir(save_root, id);
    let path = record_path(&dir);
    let mut record = MeetingRecord::load(&path).map_err(|error| error.to_string())?;
    if !matches!(
        record.status,
        MeetingStatus::Recorded | MeetingStatus::Failed | MeetingStatus::Canceled
    ) {
        return Err("meeting is not ready to start transcription".to_string());
    }
    record.model_id = request.model_id.clone();
    record.language = request.language;
    record.transcription_request = Some(request);
    record.speaker_turns.clear();
    record.api_audio_duration_ms = 0;
    record.api_cost_microusd = 0;
    record.api_request_count = 0;
    record.api_unpriced_request_count = 0;
    record.segments.clear();
    record.progress = MeetingProgress::default();
    record.failed_phase = None;
    record.failed_segments = 0;
    record.status = MeetingStatus::Pending;
    record.error = None;
    record.touch();
    let _ = fs::remove_file(transcript_path(&dir));
    record
        .save_atomic(&path)
        .map_err(|error| error.to_string())?;
    Ok(record)
}

pub fn cancel_pending_meeting(save_root: &Path, id: &str) -> Result<MeetingRecord, String> {
    if !is_valid_meeting_id(id) {
        return Err("invalid meeting id".to_string());
    }
    let dir = meeting_dir(save_root, id);
    let path = record_path(&dir);
    let mut record = MeetingRecord::load(&path).map_err(|error| error.to_string())?;
    if record.status != MeetingStatus::Pending {
        return Err("only a queued meeting can be canceled before processing".to_string());
    }
    record.status = MeetingStatus::Canceled;
    record.touch();
    record
        .save_atomic(&path)
        .map_err(|error| error.to_string())?;
    Ok(record)
}

/// Rewrite orphaned `processing` meetings (their worker thread died with the
/// app) to `failed` so the UI offers Resume. Pending jobs remain pending so the
/// startup scheduler can resume work that was queued behind another meeting.
pub fn reconcile_interrupted(save_root: &Path) {
    let _ = reconcile_interrupted_with_cutoff(save_root, None);
}

/// Rewrite only orphaned meetings that were already processing at `cutoff`.
///
/// Startup recovery runs on a background thread because the meetings directory
/// can be gated by macOS privacy prompts. The cutoff prevents that delayed scan
/// from failing work that started after this app launch.
pub fn reconcile_interrupted_before(
    save_root: &Path,
    cutoff: DateTime<Utc>,
) -> Vec<MeetingFinishedEvent> {
    reconcile_interrupted_with_cutoff(save_root, Some(cutoff))
}

fn reconcile_interrupted_with_cutoff(
    save_root: &Path,
    cutoff: Option<DateTime<Utc>>,
) -> Vec<MeetingFinishedEvent> {
    let Ok(entries) = fs::read_dir(meetings_root(save_root)) else {
        return Vec::new();
    };
    let mut reconciled = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let path = record_path(&entry.path());
        if let Ok(mut record) = MeetingRecord::load(&path) {
            let existed_at_cutoff = cutoff
                .as_ref()
                .map(|cutoff| record.updated_at <= *cutoff)
                .unwrap_or(true);
            if record.status == MeetingStatus::Processing && existed_at_cutoff {
                record.status = MeetingStatus::Failed;
                let error = "Interrupted because the app was closed (resumable)".to_string();
                record.error = Some(error.clone());
                record.touch();
                if record.save_atomic(&path).is_ok() {
                    reconciled.push(MeetingFinishedEvent {
                        id: record.id,
                        status: record.status,
                        error: Some(error),
                    });
                }
            }
        }
    }
    reconciled
}

fn finish_failed(
    record: &mut MeetingRecord,
    record_path: &Path,
    emit: &MeetingEventCallback,
    error: String,
) -> Result<(), String> {
    record.status = MeetingStatus::Failed;
    record.failed_phase = record.progress.phase;
    record.failed_segments = record
        .segments
        .iter()
        .filter(|segment| segment.status == MeetingSegmentStatus::Failed)
        .count();
    record.error = Some(error.clone());
    record.touch();
    let _ = record.save_atomic(record_path);
    emit(MeetingEvent::Finished(MeetingFinishedEvent {
        id: record.id.clone(),
        status: record.status,
        error: Some(error),
    }));
    Ok(())
}

fn concise_meeting_error(context: &str, error: &str) -> String {
    const MAX_ERROR_CHARS: usize = 320;
    let mut redact_next = false;
    let mut safe_tokens = Vec::new();
    for token in error.split_whitespace() {
        if redact_next {
            safe_tokens.push("[redacted]".to_string());
            redact_next = false;
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if lower == "bearer" {
            safe_tokens.push(token.to_string());
            redact_next = true;
        } else if lower.starts_with("api_key=")
            || lower.starts_with("apikey=")
            || lower.starts_with("authorization=")
        {
            let name = token.split('=').next().unwrap_or("credential");
            safe_tokens.push(format!("{name}=[redacted]"));
        } else {
            safe_tokens.push(token.to_string());
        }
    }
    let mut safe = safe_tokens.join(" ");
    if safe.chars().count() > MAX_ERROR_CHARS {
        safe = safe.chars().take(MAX_ERROR_CHARS).collect::<String>();
        safe.push('…');
    }
    if safe.is_empty() {
        context.to_string()
    } else {
        format!("{context}: {safe}")
    }
}

fn emit_progress_running(
    emit: &MeetingEventCallback,
    record: &MeetingRecord,
    elapsed_ms: u64,
    processed_at_start: u64,
) {
    let processed_this_run = record
        .progress
        .processed_ms
        .saturating_sub(processed_at_start);
    let remaining = record
        .duration_ms
        .saturating_sub(record.progress.processed_ms);
    emit(MeetingEvent::Progress(MeetingProgressEvent {
        id: record.id.clone(),
        status: record.status,
        phase: record.progress.phase.unwrap_or(MeetingPhase::Transcribing),
        segments_total: record.progress.segments_total,
        segments_done: record.progress.segments_done,
        processed_ms: record.progress.processed_ms,
        duration_ms: record.duration_ms,
        elapsed_ms,
        eta_ms: compute_eta_ms(elapsed_ms, processed_this_run, remaining),
    }));
}

/// Run (or resume) the batch transcription for one meeting. Blocking — meant to
/// be called from a dedicated worker thread. Records failures into the meeting
/// rather than returning `Err`, so the caller's `Result` only reflects fatal
/// orchestration errors (e.g. the record can't be loaded).
#[allow(clippy::too_many_arguments)]
pub fn run_meeting_job(
    save_root: &Path,
    model_directory: &Path,
    id: &str,
    suppress_low_confidence: bool,
    dictionary: &DictionaryContext,
    credentials: TranscriptionCredentials,
    realtime_replay: Option<MeetingRealtimeReplay>,
    cancel: Arc<AtomicBool>,
    emit: MeetingEventCallback,
) -> Result<(), String> {
    let dir = meeting_dir(save_root, id);
    let rpath = record_path(&dir);
    let mut record = MeetingRecord::load(&rpath).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let transcription_request =
        record
            .transcription_request
            .clone()
            .unwrap_or_else(|| MeetingTranscriptionRequest {
                model_id: record.model_id.clone(),
                language: record.language,
                language_hints: crate::settings::default_transcription_language_hints(),
                streaming_enabled: false,
                speaker_separation_enabled: false,
            });

    let model_runtime = model_runtime_for_id(model_directory, &record.model_id);
    let model_path = model_directory.join(format!("{}.bin", record.model_id));
    record.status = MeetingStatus::Processing;
    record.error = None;
    record.failed_phase = None;
    record.failed_segments = 0;
    record.progress.phase = Some(MeetingPhase::PreparingAudio);
    record.touch();
    let _ = record.save_atomic(&rpath);
    emit_progress_running(&emit, &record, 0, record.progress.processed_ms);
    if model_runtime == "whisper-rs" && !model_path.exists() {
        return finish_failed(
            &mut record,
            &rpath,
            &emit,
            format!("model file not found: {}", model_path.display()),
        );
    }

    // 1. Normalize to a temp 16 kHz WAV (regenerated on resume, deleted at the end).
    let wav = work_wav_path(&dir);
    if let Err(error) = normalize_to_wav16k(&dir.join(&record.audio_file), &wav) {
        return finish_failed(
            &mut record,
            &rpath,
            &emit,
            format!("audio decode failed: {error}"),
        );
    }

    // 2. Plan segments (only when fresh — resume keeps the existing plan).
    record.progress.phase = Some(MeetingPhase::SplittingAudio);
    record.touch();
    let _ = record.save_atomic(&rpath);
    emit_progress_running(
        &emit,
        &record,
        started.elapsed().as_millis() as u64,
        record.progress.processed_ms,
    );
    let (frame_rms, total_ms) = match compute_frame_rms(&wav, FRAME_MS) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&wav);
            return finish_failed(
                &mut record,
                &rpath,
                &emit,
                format!("audio analysis failed: {error}"),
            );
        }
    };
    record.duration_ms = total_ms;
    if record.segments.is_empty() {
        let spans = plan_segments(
            &frame_rms,
            FRAME_MS,
            total_ms,
            SegmentPlanOptions::default(),
        );
        record.segments = spans
            .into_iter()
            .enumerate()
            .map(|(index, (start_ms, end_ms))| MeetingSegment {
                index,
                start_ms,
                end_ms,
                status: MeetingSegmentStatus::Pending,
                text: String::new(),
                no_speech: false,
            })
            .collect();
    }
    record.recompute_progress();
    record.progress.phase = Some(MeetingPhase::Transcribing);
    record.touch();
    let _ = record.save_atomic(&rpath);
    let processed_at_start = record.progress.processed_ms;
    emit_progress_running(
        &emit,
        &record,
        started.elapsed().as_millis() as u64,
        processed_at_start,
    );

    // 3. Load the model once for the whole meeting.
    let context = if model_runtime == "whisper-rs" {
        match cached_whisper_context(&model_path) {
            Ok(context) => Some(context),
            Err(error) => {
                let _ = fs::remove_file(&wav);
                return finish_failed(
                    &mut record,
                    &rpath,
                    &emit,
                    format!("model load failed: {error}"),
                );
            }
        }
    } else {
        None
    };
    let runtime_transcriber = if context.is_none() {
        match RuntimeTranscriber::for_archival_with_credentials(
            model_directory,
            credentials.clone(),
        ) {
            Ok(transcriber) => {
                Some(transcriber.with_file_streaming(transcription_request.streaming_enabled, None))
            }
            Err(error) => {
                let _ = fs::remove_file(&wav);
                return finish_failed(
                    &mut record,
                    &rpath,
                    &emit,
                    format!("transcription client setup failed: {error}"),
                );
            }
        }
    } else {
        None
    };

    let language = record.language;
    let total_ms = record.duration_ms;
    let realtime_run_token = Utc::now().timestamp_micros();
    let realtime_source_key = matches!(
        model_runtime.as_str(),
        "openai-realtime" | "soniox-realtime"
    )
    .then(|| format!("meeting:{}:{realtime_run_token}", record.id));
    // Failed segments are retried below. Only failures from this run decide the
    // final status; otherwise a fully successful resume would remain Failed.
    let mut any_failed = false;

    for idx in 0..record.segments.len() {
        if record.segments[idx].status == MeetingSegmentStatus::Completed {
            continue;
        }
        if cancel.load(Ordering::Acquire) {
            if let (Some(replay), Some(source_key)) =
                (realtime_replay.as_ref(), realtime_source_key.as_ref())
            {
                replay.close_source(source_key.clone());
            }
            let _ = fs::remove_file(&wav);
            record.status = MeetingStatus::Canceled;
            record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
            record.touch();
            let _ = record.save_atomic(&rpath);
            emit(MeetingEvent::Finished(MeetingFinishedEvent {
                id: record.id.clone(),
                status: record.status,
                error: None,
            }));
            return Ok(());
        }

        let start_ms = record.segments[idx].start_ms;
        let end_ms = record.segments[idx].end_ms;
        let read_lo = start_ms.saturating_sub(OVERLAP_MS);

        let samples = match read_window_samples(&wav, read_lo, end_ms) {
            Ok(samples) => samples,
            Err(error) => {
                let detail = concise_meeting_error(
                    &format!("Segment {} audio read failed", idx + 1),
                    &error.to_string(),
                );
                eprintln!(
                    "[wakenote] meeting {}: segment {idx} read failed: {error}",
                    record.id
                );
                record.segments[idx].status = MeetingSegmentStatus::Failed;
                any_failed = true;
                record.error.get_or_insert(detail);
                record.recompute_progress();
                record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
                record.touch();
                let _ = record.save_atomic(&rpath);
                emit_progress_running(
                    &emit,
                    &record,
                    record.progress.elapsed_ms,
                    processed_at_start,
                );
                continue;
            }
        };

        // In-segment progress for a smooth bar: whisper's 0-100 maps onto the
        // overall audio-time progress, throttled to whole-percent increases.
        let base_done_ms = record.progress.processed_ms;
        let span_ms = end_ms.saturating_sub(start_ms);
        let segs_total = record.progress.segments_total;
        let segs_done = record.progress.segments_done;
        let emit_cb = emit.clone();
        let id_cb = record.id.clone();
        let last_overall = Arc::new(AtomicI32::new(-1));
        let progress_cb = move |pct: i32| {
            let pct = pct.clamp(0, 100) as u64;
            let processed = base_done_ms + span_ms * pct / 100;
            let overall = processed
                .saturating_mul(100)
                .checked_div(total_ms)
                .unwrap_or(0) as i32;
            if overall <= last_overall.load(Ordering::Acquire) {
                return;
            }
            last_overall.store(overall, Ordering::Release);
            let elapsed = started.elapsed().as_millis() as u64;
            let processed_this_run = processed.saturating_sub(processed_at_start);
            let remaining = total_ms.saturating_sub(processed);
            emit_cb(MeetingEvent::Progress(MeetingProgressEvent {
                id: id_cb.clone(),
                status: MeetingStatus::Processing,
                phase: MeetingPhase::Transcribing,
                segments_total: segs_total,
                segments_done: segs_done,
                processed_ms: processed,
                duration_ms: total_ms,
                elapsed_ms: elapsed,
                eta_ms: compute_eta_ms(elapsed, processed_this_run, remaining),
            }));
        };

        let mut canceled_during_replay = false;
        let decoded = if let Some(context) = context.as_ref() {
            transcribe_samples_with_context(
                context,
                &samples,
                language,
                suppress_low_confidence,
                dictionary,
                progress_cb,
            )
            .map(|decoded| (decoded, Vec::new(), None))
        } else {
            let segment_wav = if realtime_source_key.is_some() {
                dir.join(format!(".wakenote-segment-{realtime_run_token}-{idx}.wav"))
            } else {
                dir.join(format!(".wakenote-segment-{idx}.wav"))
            };
            let result = write_samples_wav16k(&segment_wav, &samples)
                .map_err(TranscriptionError::Engine)
                .and_then(|()| {
                    if let Some(source_key) = realtime_source_key.as_ref() {
                        let replay = realtime_replay.as_ref().ok_or_else(|| {
                            TranscriptionError::Engine(
                                "saved Meeting realtime replay is unavailable".to_string(),
                            )
                        })?;
                        realtime_result_store().discard(&segment_wav);
                        replay
                            .submit(MeetingRealtimeReplayRequest {
                                source_key: source_key.clone(),
                                source_label: record.title.clone(),
                                chunk_id: idx as u64 + 1,
                                model_id: record.model_id.clone(),
                                language,
                                language_hints: transcription_request.language_hints.clone(),
                                dictionary: dictionary.clone(),
                                sample_rate: 16_000,
                                samples: Arc::new(samples.clone()),
                                credentials: credentials.clone(),
                                audio_path: segment_wav.clone(),
                            })
                            .map_err(TranscriptionError::Engine)?;
                        match realtime_result_store().wait_cancellable(&segment_wav, &cancel) {
                            Some(RealtimeStoredResult::Completed(execution)) => Ok(execution),
                            Some(RealtimeStoredResult::Failed(failure)) => {
                                Err(TranscriptionError::Failure(failure))
                            }
                            None => {
                                canceled_during_replay = true;
                                Err(TranscriptionError::Engine(
                                    "saved Meeting realtime replay was canceled".to_string(),
                                ))
                            }
                        }
                    } else {
                        runtime_transcriber
                            .as_ref()
                            .expect("non-Whisper meetings have a runtime transcriber")
                            .transcribe_execution(TranscriptionRequest {
                                audio_path: &segment_wav,
                                model_id: &record.model_id,
                                language,
                                language_hints: &transcription_request.language_hints,
                                dictionary,
                            })
                    }
                })
                .map(|mut execution| {
                    let text = dictionary.correct(execution.text.trim());
                    let speaker_turns = if transcription_request.speaker_separation_enabled {
                        execution
                            .speaker_turns
                            .into_iter()
                            .map(|mut turn| {
                                turn.part_index = idx;
                                turn.start_ms = turn.start_ms.saturating_add(read_lo);
                                turn.end_ms = turn.end_ms.saturating_add(read_lo);
                                turn.text = dictionary.correct(turn.text.trim());
                                turn
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    if let Some(usage) = execution.usage.as_mut() {
                        usage.audio_duration_ms = samples.len() as u64 * 1_000 / 16_000;
                        if usage.provider_cost_usd.is_none()
                            && let Some(provider) = usage.provider.as_deref()
                        {
                            usage.provider_cost_usd = estimated_provider_cost_usd(
                                provider,
                                &execution.effective_model_id,
                                usage.audio_duration_ms,
                            );
                        }
                    }
                    (
                        DecodedWindow {
                            no_speech: text.is_empty(),
                            text,
                        },
                        speaker_turns,
                        execution.usage,
                    )
                });
            let _ = fs::remove_file(segment_wav);
            result
        };

        if canceled_during_replay {
            if let (Some(replay), Some(source_key)) =
                (realtime_replay.as_ref(), realtime_source_key.as_ref())
            {
                replay.close_source(source_key.clone());
            }
            let _ = fs::remove_file(&wav);
            record.status = MeetingStatus::Canceled;
            record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
            record.touch();
            let _ = record.save_atomic(&rpath);
            emit(MeetingEvent::Finished(MeetingFinishedEvent {
                id: record.id.clone(),
                status: record.status,
                error: None,
            }));
            return Ok(());
        }

        match decoded {
            Ok((decoded, speaker_turns, usage)) => {
                record.segments[idx].status = MeetingSegmentStatus::Completed;
                record.segments[idx].text = decoded.text.clone();
                record.segments[idx].no_speech = decoded.no_speech;
                record.speaker_turns.retain(|turn| turn.part_index != idx);
                record.speaker_turns.extend(speaker_turns);
                if let Some(usage) = usage
                    && let Some(provider) = usage.provider
                {
                    record.api_audio_duration_ms = record
                        .api_audio_duration_ms
                        .saturating_add(usage.audio_duration_ms);
                    record.api_request_count = record.api_request_count.saturating_add(1);
                    if let Some(cost) = usage.provider_cost_usd {
                        record.api_cost_microusd = record
                            .api_cost_microusd
                            .saturating_add((cost * 1_000_000.0).round().max(0.0) as u64);
                    } else {
                        record.api_unpriced_request_count =
                            record.api_unpriced_request_count.saturating_add(1);
                    }
                    let _ = provider;
                }
                record.recompute_progress();
                record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
                record.touch();
                let _ = write_transcript(&dir, &record);
                let _ = record.save_atomic(&rpath);
                emit(MeetingEvent::SegmentCommitted(MeetingSegmentEvent {
                    id: record.id.clone(),
                    index: idx,
                    start_ms,
                    end_ms,
                    text: decoded.text,
                }));
                emit_progress_running(
                    &emit,
                    &record,
                    record.progress.elapsed_ms,
                    processed_at_start,
                );
            }
            Err(error) => {
                let detail = concise_meeting_error(
                    &format!("Segment {} transcription failed", idx + 1),
                    &error.to_string(),
                );
                eprintln!(
                    "[wakenote] meeting {}: segment {idx} decode failed: {error}",
                    record.id
                );
                record.segments[idx].status = MeetingSegmentStatus::Failed;
                any_failed = true;
                record.error.get_or_insert(detail);
                record.recompute_progress();
                record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
                record.touch();
                let _ = record.save_atomic(&rpath);
                emit_progress_running(
                    &emit,
                    &record,
                    record.progress.elapsed_ms,
                    processed_at_start,
                );
            }
        }
    }

    if let (Some(replay), Some(source_key)) =
        (realtime_replay.as_ref(), realtime_source_key.as_ref())
    {
        replay.close_source(source_key.clone());
    }
    record.progress.phase = Some(MeetingPhase::Finalizing);
    record.touch();
    let _ = record.save_atomic(&rpath);
    emit_progress_running(
        &emit,
        &record,
        started.elapsed().as_millis() as u64,
        processed_at_start,
    );
    let _ = fs::remove_file(&wav);
    record.status = if any_failed {
        MeetingStatus::Failed
    } else {
        MeetingStatus::Completed
    };
    record.failed_phase = any_failed.then_some(MeetingPhase::Transcribing);
    record.failed_segments = record
        .segments
        .iter()
        .filter(|segment| segment.status == MeetingSegmentStatus::Failed)
        .count();
    if any_failed && record.error.is_none() {
        record.error = Some("One or more meeting segments failed to transcribe".to_string());
    } else if !any_failed {
        record.error = None;
    }
    record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
    record.recompute_progress();
    record.touch();
    let _ = write_transcript(&dir, &record);
    let _ = record.save_atomic(&rpath);
    emit(MeetingEvent::Finished(MeetingFinishedEvent {
        id: record.id.clone(),
        status: record.status,
        error: record.error.clone(),
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn rms_with_silence(
        total_frames: usize,
        silent: &[(usize, usize)],
        threshold_value: f32,
    ) -> Vec<f32> {
        let mut rms = vec![threshold_value; total_frames];
        for &(start, end) in silent {
            for frame in rms.iter_mut().take(end).skip(start) {
                *frame = 0.0;
            }
        }
        rms
    }

    #[test]
    fn manual_meeting_capture_finishes_as_recorded_without_queueing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let timestamp = Local.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap();
        let mut recorder = start_manual_recorded_meeting_capture(
            temp.path(),
            "Manual meeting",
            "Microphone + System Audio",
            "whisper-medium",
            TranscriptionLanguage::Auto,
            "test",
            16_000,
            96,
            timestamp,
        )
        .expect("manual recorder");
        recorder.write_samples(&vec![0.25; 16_000]).expect("audio");

        let record = recorder.finish().expect("recorded meeting");

        assert_eq!(record.status, MeetingStatus::Recorded);
        assert_eq!(record.duration_ms, 1_000);
        assert_eq!(record.audio_file, "audio.m4a");
        assert_eq!(record.audio_format, "m4a");
        let dir = meeting_dir(temp.path(), &record.id);
        assert!(dir.join("audio.m4a").is_file());
        assert!(!dir.join("audio.wav").exists());
        assert!(!capture_wav_path(&dir).exists());
        assert!(!partial_m4a_path(&dir).exists());
        let afinfo = Command::new("/usr/bin/afinfo")
            .arg(dir.join("audio.m4a"))
            .output()
            .expect("inspect finalized m4a");
        assert!(
            afinfo.status.success(),
            "afinfo rejected finalized m4a: {}",
            String::from_utf8_lossy(&afinfo.stderr)
        );
        assert_eq!(
            MeetingRecord::load(&record_path(&dir))
                .expect("saved record")
                .status,
            MeetingStatus::Recorded
        );
    }

    #[test]
    fn saved_meeting_realtime_replay_completes_through_the_result_store() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("realtime-source.wav");
        write_samples_wav16k(&source, &vec![0.2; 16_000]).expect("source wav");
        let imported = import_meeting(
            temp.path(),
            &source,
            "soniox-realtime-v5",
            TranscriptionLanguage::Ko,
            "test",
            Local.with_ymd_and_hms(2026, 8, 11, 12, 0, 0).unwrap(),
        )
        .expect("import meeting");
        start_recorded_meeting_transcription(
            temp.path(),
            &imported.id,
            MeetingTranscriptionRequest {
                language_hints: crate::settings::default_transcription_language_hints(),
                model_id: "soniox-realtime-v5".into(),
                language: TranscriptionLanguage::Ko,
                streaming_enabled: true,
                speaker_separation_enabled: false,
            },
        )
        .expect("start transcription");

        let submitted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let submitted_for_replay = submitted.clone();
        let closed_for_replay = closed.clone();
        let replay = MeetingRealtimeReplay::new(
            Arc::new(move |request| {
                assert!(request.source_key.starts_with("meeting:"));
                assert_eq!(request.model_id, "soniox-realtime-v5");
                assert_eq!(request.sample_rate, 16_000);
                assert_eq!(request.samples.len(), 16_000);
                submitted_for_replay.fetch_add(1, Ordering::AcqRel);
                realtime_result_store().publish(
                    request.audio_path,
                    RealtimeStoredResult::Completed(crate::transcription::TranscriptionExecution {
                        text: "회의 전사".into(),
                        speaker_turns: Vec::new(),
                        requested_model_id: request.model_id.clone(),
                        effective_model_id: request.model_id,
                        fallback_from_model_id: None,
                        usage: None,
                        issue: None,
                    }),
                );
                Ok(())
            }),
            Arc::new(move |_| {
                closed_for_replay.fetch_add(1, Ordering::AcqRel);
            }),
        );

        run_meeting_job(
            temp.path(),
            &temp.path().join("models"),
            &imported.id,
            false,
            &DictionaryContext::default(),
            TranscriptionCredentials::default(),
            Some(replay),
            Arc::new(AtomicBool::new(false)),
            Arc::new(|_| {}),
        )
        .expect("realtime meeting job");

        let detail = meeting_detail(temp.path(), &imported.id).expect("meeting detail");
        assert_eq!(detail.record.status, MeetingStatus::Completed);
        assert_eq!(detail.transcript, "회의 전사");
        assert_eq!(submitted.load(Ordering::Acquire), 1);
        assert_eq!(closed.load(Ordering::Acquire), 1);
    }

    #[test]
    fn saved_meeting_realtime_replay_cancellation_closes_its_source() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("realtime-cancel.wav");
        write_samples_wav16k(&source, &vec![0.2; 16_000]).expect("source wav");
        let imported = import_meeting(
            temp.path(),
            &source,
            "openai-gpt-live-transcribe",
            TranscriptionLanguage::Auto,
            "test",
            Local.with_ymd_and_hms(2026, 8, 11, 13, 0, 0).unwrap(),
        )
        .expect("import meeting");
        start_recorded_meeting_transcription(
            temp.path(),
            &imported.id,
            MeetingTranscriptionRequest {
                language_hints: crate::settings::default_transcription_language_hints(),
                model_id: "openai-gpt-live-transcribe".into(),
                language: TranscriptionLanguage::Auto,
                streaming_enabled: true,
                speaker_separation_enabled: false,
            },
        )
        .expect("start transcription");

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_on_submit = cancel.clone();
        let closed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let closed_for_replay = closed.clone();
        let replay = MeetingRealtimeReplay::new(
            Arc::new(move |_| {
                cancel_on_submit.store(true, Ordering::Release);
                Ok(())
            }),
            Arc::new(move |_| {
                closed_for_replay.fetch_add(1, Ordering::AcqRel);
            }),
        );

        run_meeting_job(
            temp.path(),
            &temp.path().join("models"),
            &imported.id,
            false,
            &DictionaryContext::default(),
            TranscriptionCredentials::default(),
            Some(replay),
            cancel,
            Arc::new(|_| {}),
        )
        .expect("canceled realtime meeting job");

        let detail = meeting_detail(temp.path(), &imported.id).expect("meeting detail");
        assert_eq!(detail.record.status, MeetingStatus::Canceled);
        assert_eq!(closed.load(Ordering::Acquire), 1);
        assert!(
            fs::read_dir(meeting_dir(temp.path(), &imported.id))
                .expect("meeting directory")
                .flatten()
                .all(|entry| !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".wakenote-segment-"))
        );
    }

    #[test]
    fn imported_meeting_is_recorded_until_the_user_confirms_transcription() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("planning.wav");
        fs::write(&source, b"recording").expect("source audio");
        let timestamp = Local.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap();

        let record = import_meeting(
            temp.path(),
            &source,
            "whisper-medium",
            TranscriptionLanguage::Auto,
            "test",
            timestamp,
        )
        .expect("import meeting");

        assert_eq!(record.status, MeetingStatus::Recorded);
        assert!(record.transcription_request.is_none());
        assert_eq!(record.progress, MeetingProgress::default());
    }

    #[test]
    fn meeting_m4a_failure_preserves_capture_wav_for_recovery() {
        let temp = tempfile::tempdir().expect("tempdir");
        let timestamp = Local.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap();
        let mut recorder = start_manual_recorded_meeting_capture(
            temp.path(),
            "Recovery meeting",
            "Microphone + System Audio",
            "whisper-medium",
            TranscriptionLanguage::Auto,
            "test",
            10,
            96,
            timestamp,
        )
        .expect("manual recorder");
        let id = recorder.id().to_string();
        recorder.write_samples(&[0.25]).expect("audio");

        let record = recorder
            .finish_with_encoder(|_, _, _| {
                Err("m4a encoder failed: forced test failure".to_string())
            })
            .expect("recovery WAV");

        let dir = meeting_dir(temp.path(), &id);
        assert_eq!(record.status, MeetingStatus::Recorded);
        assert_eq!(record.audio_file, "audio-recovery.wav");
        assert_eq!(record.audio_format, "wav");
        assert!(
            record
                .capture_warning
                .as_deref()
                .is_some_and(|warning| warning.contains("m4a encoder failed"))
        );
        assert!(!capture_wav_path(&dir).exists());
        assert!(dir.join("audio-recovery.wav").is_file());
        assert!(!partial_m4a_path(&dir).exists());
        assert!(!dir.join("audio.m4a").exists());
        assert!(record_path(&dir).is_file());
    }

    #[test]
    fn empty_manual_meeting_never_invokes_the_encoder_or_publishes_a_recording() {
        let tmp = tempfile::tempdir().unwrap();
        let recorder = start_manual_recorded_meeting_capture(
            tmp.path(),
            "Empty meeting",
            "Microphone + System Audio",
            "whisper-medium",
            TranscriptionLanguage::Auto,
            "test",
            16_000,
            96,
            Local::now(),
        )
        .unwrap();
        let meeting_id = recorder.id().to_string();

        let error = recorder
            .finish_with_encoder(|_, _, _| panic!("empty audio must not reach the encoder"))
            .unwrap_err();

        assert_eq!(error, "meeting capture audio is empty");
        assert!(meeting_detail(tmp.path(), &meeting_id).is_err());
    }

    #[test]
    fn recorded_meeting_starts_with_selected_model_and_options() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = meeting_dir(temp.path(), "20260803-120000-meeting");
        fs::create_dir_all(&dir).expect("meeting dir");
        let mut record = sample_record();
        record.id = "20260803-120000-meeting".into();
        record.status = MeetingStatus::Recorded;
        record.model_id = "whisper-medium".into();
        record
            .save_atomic(&record_path(&dir))
            .expect("recorded meeting");

        let updated = start_recorded_meeting_transcription(
            temp.path(),
            &record.id,
            MeetingTranscriptionRequest {
                language_hints: crate::settings::default_transcription_language_hints(),
                model_id: "openai-gpt-4o-transcribe-diarize".into(),
                language: TranscriptionLanguage::Ko,
                streaming_enabled: true,
                speaker_separation_enabled: true,
            },
        )
        .expect("start transcription");

        assert_eq!(updated.status, MeetingStatus::Pending);
        assert_eq!(updated.model_id, "openai-gpt-4o-transcribe-diarize");
        assert_eq!(
            updated.transcription_request,
            Some(MeetingTranscriptionRequest {
                language_hints: crate::settings::default_transcription_language_hints(),
                model_id: "openai-gpt-4o-transcribe-diarize".into(),
                language: TranscriptionLanguage::Ko,
                streaming_enabled: true,
                speaker_separation_enabled: true,
            })
        );
        assert!(updated.segments.is_empty());
        assert!(updated.speaker_turns.is_empty());
    }

    #[test]
    fn queued_pending_meeting_can_be_canceled_before_processing() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = meeting_dir(temp.path(), "20260812-120000-queued");
        fs::create_dir_all(&dir).expect("meeting dir");
        let mut record = sample_record();
        record.id = "20260812-120000-queued".into();
        record.status = MeetingStatus::Pending;
        record
            .save_atomic(&record_path(&dir))
            .expect("pending meeting");

        let canceled =
            cancel_pending_meeting(temp.path(), &record.id).expect("cancel queued meeting");

        assert_eq!(canceled.status, MeetingStatus::Canceled);
        assert_eq!(
            meeting_detail(temp.path(), &record.id)
                .expect("saved detail")
                .record
                .status,
            MeetingStatus::Canceled
        );
    }

    #[test]
    fn plan_returns_single_segment_when_short() {
        let rms = vec![0.2f32; 100]; // 2s of audio at 20ms frames
        let segments = plan_segments(&rms, FRAME_MS, 2_000, SegmentPlanOptions::default());
        assert_eq!(segments, vec![(0, 2_000)]);
    }

    #[test]
    fn plan_cuts_at_silence_center() {
        // 200s of audio, loud everywhere except a silence run at 60s-62s.
        let frames = (200_000 / FRAME_MS) as usize;
        let silent = ((60_000 / FRAME_MS) as usize, (62_000 / FRAME_MS) as usize);
        let rms = rms_with_silence(frames, &[silent], 0.2);
        let segments = plan_segments(&rms, FRAME_MS, 200_000, SegmentPlanOptions::default());
        // First cut should land at the center of the silence (~61s).
        assert_eq!(segments[0].0, 0);
        assert_eq!(segments[0].1, 61_000);
        assert_eq!(segments[1].0, 61_000);
    }

    #[test]
    fn plan_force_cuts_at_max_without_silence() {
        let frames = (200_000 / FRAME_MS) as usize;
        let rms = vec![0.3f32; frames]; // never silent
        let segments = plan_segments(&rms, FRAME_MS, 200_000, SegmentPlanOptions::default());
        assert_eq!(segments[0], (0, MAX_SEGMENT_MS));
    }

    #[test]
    fn plan_tiles_without_gaps_or_overlap() {
        let frames = (500_000 / FRAME_MS) as usize;
        let rms = vec![0.3f32; frames];
        let segments = plan_segments(&rms, FRAME_MS, 500_000, SegmentPlanOptions::default());
        assert_eq!(segments.first().unwrap().0, 0);
        assert_eq!(segments.last().unwrap().1, 500_000);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "segments must tile contiguously");
        }
    }

    #[test]
    fn slugify_handles_ascii_and_unicode() {
        assert_eq!(
            slugify("Weekly Sync 2026").as_deref(),
            Some("weekly-sync-2026")
        );
        assert_eq!(slugify("회의록"), None);
        assert_eq!(slugify("  ---  "), None);
    }

    #[test]
    fn meeting_id_validation_rejects_traversal() {
        assert!(is_valid_meeting_id("20260614-143000-sync"));
        assert!(!is_valid_meeting_id("../escape"));
        assert!(!is_valid_meeting_id("a/b"));
        assert!(!is_valid_meeting_id(""));
    }

    #[test]
    fn eta_scales_with_remaining_work() {
        // Spent 10s on 25% → ~30s remain.
        assert_eq!(compute_eta_ms(10_000, 25_000, 75_000), 30_000);
        assert_eq!(compute_eta_ms(0, 1, 1), 0);
        assert_eq!(compute_eta_ms(1, 0, 1), 0);
    }

    #[test]
    fn transcript_text_joins_completed_segments_in_order() {
        let mut record = sample_record();
        record.segments = vec![
            segment(0, 0, 1000, MeetingSegmentStatus::Completed, "hello"),
            segment(1, 1000, 2000, MeetingSegmentStatus::Failed, "dropped"),
            segment(2, 2000, 3000, MeetingSegmentStatus::Completed, "world"),
            segment(3, 3000, 4000, MeetingSegmentStatus::Pending, ""),
        ];
        assert_eq!(record.transcript_text(), "hello\nworld");
    }

    #[test]
    fn recompute_progress_counts_completed_spans() {
        let mut record = sample_record();
        record.segments = vec![
            segment(0, 0, 1000, MeetingSegmentStatus::Completed, "a"),
            segment(1, 1000, 3000, MeetingSegmentStatus::Completed, "b"),
            segment(2, 3000, 4000, MeetingSegmentStatus::Pending, ""),
        ];
        record.recompute_progress();
        assert_eq!(record.progress.segments_total, 3);
        assert_eq!(record.progress.segments_done, 2);
        assert_eq!(record.progress.processed_ms, 3000);
    }

    #[test]
    fn record_serde_round_trips() {
        let mut record = sample_record();
        record.segments = vec![segment(0, 0, 1000, MeetingSegmentStatus::Completed, "hi")];
        record.recompute_progress();
        let json = serde_json::to_string(&record).expect("serialize");
        let parsed: MeetingRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(record, parsed);
        // status serializes snake_case for the frontend.
        assert!(json.contains("\"status\":\"pending\""));
    }

    #[test]
    fn legacy_meeting_progress_and_failure_context_default_safely() {
        let json = serde_json::to_value(sample_record()).expect("meeting json");
        let mut legacy = json.as_object().expect("object").clone();
        legacy
            .get_mut("progress")
            .and_then(serde_json::Value::as_object_mut)
            .expect("progress")
            .remove("phase");
        legacy.remove("failed_phase");
        legacy.remove("failed_segments");

        let parsed: MeetingRecord =
            serde_json::from_value(serde_json::Value::Object(legacy)).expect("legacy meeting");

        assert_eq!(parsed.progress.phase, None);
        assert_eq!(parsed.failed_phase, None);
        assert_eq!(parsed.failed_segments, 0);
    }

    #[test]
    fn meeting_progress_phase_round_trips_in_events() {
        let event = MeetingProgressEvent {
            id: "meeting-1".into(),
            status: MeetingStatus::Processing,
            phase: MeetingPhase::SplittingAudio,
            segments_total: 0,
            segments_done: 0,
            processed_ms: 0,
            duration_ms: 0,
            elapsed_ms: 250,
            eta_ms: 0,
        };

        let value = serde_json::to_value(&event).expect("serialize event");
        assert_eq!(value["phase"], "splitting_audio");
    }

    #[test]
    fn meeting_errors_keep_context_without_persisting_credentials() {
        let safe = concise_meeting_error(
            "Segment 3 transcription failed",
            "HTTP 401 Authorization: Bearer sk-secret-token request denied",
        );

        assert!(safe.contains("Segment 3 transcription failed"));
        assert!(safe.contains("request denied"));
        assert!(!safe.contains("sk-secret-token"));
    }

    #[test]
    fn reconcile_preserves_pending_jobs_but_marks_processing_jobs_failed() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let pending_dir = meeting_dir(tmp.path(), "20260614-143000-pending");
        let processing_dir = meeting_dir(tmp.path(), "20260614-143001-processing");
        fs::create_dir_all(&pending_dir).expect("pending dir");
        fs::create_dir_all(&processing_dir).expect("processing dir");

        let mut pending = sample_record();
        pending.id = "20260614-143000-pending".into();
        pending.status = MeetingStatus::Pending;
        pending
            .save_atomic(&record_path(&pending_dir))
            .expect("save pending");

        let mut processing = sample_record();
        processing.id = "20260614-143001-processing".into();
        processing.status = MeetingStatus::Processing;
        processing
            .save_atomic(&record_path(&processing_dir))
            .expect("save processing");

        reconcile_interrupted(tmp.path());

        let pending = MeetingRecord::load(&record_path(&pending_dir)).expect("load pending");
        let processing =
            MeetingRecord::load(&record_path(&processing_dir)).expect("load processing");
        assert_eq!(pending.status, MeetingStatus::Pending);
        assert_eq!(pending.error, None);
        assert_eq!(processing.status, MeetingStatus::Failed);
        assert!(
            processing
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Interrupted"))
        );
    }

    #[test]
    fn delayed_reconcile_skips_processing_meetings_updated_after_launch_cutoff() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old_dir = meeting_dir(tmp.path(), "20260614-143000-old");
        let fresh_dir = meeting_dir(tmp.path(), "20260614-143001-fresh");
        fs::create_dir_all(&old_dir).expect("old dir");
        fs::create_dir_all(&fresh_dir).expect("fresh dir");

        let mut old = sample_record();
        old.id = "20260614-143000-old".into();
        old.status = MeetingStatus::Processing;
        old.updated_at = Utc.with_ymd_and_hms(2026, 6, 14, 5, 30, 0).unwrap();
        old.save_atomic(&record_path(&old_dir)).expect("save old");

        let mut fresh = sample_record();
        fresh.id = "20260614-143001-fresh".into();
        fresh.status = MeetingStatus::Processing;
        fresh.updated_at = Utc.with_ymd_and_hms(2026, 6, 14, 5, 40, 0).unwrap();
        fresh
            .save_atomic(&record_path(&fresh_dir))
            .expect("save fresh");

        let reconciled = reconcile_interrupted_before(
            tmp.path(),
            Utc.with_ymd_and_hms(2026, 6, 14, 5, 35, 0).unwrap(),
        );

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].id, old.id);
        assert_eq!(reconciled[0].status, MeetingStatus::Failed);
        assert!(
            reconciled[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Interrupted"))
        );

        assert_eq!(
            MeetingRecord::load(&record_path(&old_dir))
                .expect("load old")
                .status,
            MeetingStatus::Failed
        );
        assert_eq!(
            MeetingRecord::load(&record_path(&fresh_dir))
                .expect("load fresh")
                .status,
            MeetingStatus::Processing
        );
    }

    fn sample_record() -> MeetingRecord {
        let now = Utc.with_ymd_and_hms(2026, 6, 14, 5, 30, 0).unwrap();
        MeetingRecord {
            id: "20260614-143000-sync".to_string(),
            title: "sync".to_string(),
            source_filename: "sync.m4a".to_string(),
            audio_file: "audio.m4a".to_string(),
            audio_format: "m4a".to_string(),
            model_id: "whisper-medium".to_string(),
            language: TranscriptionLanguage::Ko,
            app_version: "0.0.0".to_string(),
            created_at: now,
            updated_at: now,
            duration_ms: 0,
            status: MeetingStatus::Pending,
            progress: MeetingProgress::default(),
            failed_phase: None,
            failed_segments: 0,
            segments: Vec::new(),
            transcription_request: None,
            speaker_turns: Vec::new(),
            api_audio_duration_ms: 0,
            api_cost_microusd: 0,
            api_request_count: 0,
            api_unpriced_request_count: 0,
            error: None,
            capture_warning: None,
        }
    }

    fn segment(
        index: usize,
        start_ms: u64,
        end_ms: u64,
        status: MeetingSegmentStatus,
        text: &str,
    ) -> MeetingSegment {
        MeetingSegment {
            index,
            start_ms,
            end_ms,
            status,
            text: text.to_string(),
            no_speech: false,
        }
    }
}
