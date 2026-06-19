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
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Local, Utc};
use hound::WavReader;
use serde::{Deserialize, Serialize};
use whisper_rs::WhisperContext;

use crate::settings::TranscriptionLanguage;
use crate::transcription::{default_whisper_context_parameters, transcribe_samples_with_context};

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeetingProgress {
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
    pub segments: Vec<MeetingSegment>,
    #[serde(default)]
    pub error: Option<String>,
}

impl MeetingRecord {
    fn load(path: &Path) -> std::io::Result<Self> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(std::io::Error::other)?;
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
        }
    }
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
        let cut = best_silence_cut(frame_rms, frame_ms, opts.silence_rms_threshold, win_lo, win_hi)
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
    let consider = |start: usize, end: usize, best_len: &mut usize, best_center: &mut Option<usize>| {
        let len = end - start;
        if len > *best_len {
            *best_len = len;
            *best_center = Some((start + end) / 2);
        }
    };
    for index in lo..hi {
        if frame_rms[index] < threshold {
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
/// preserved (downmixed later) to match `transcription.rs`'s afconvert usage.
fn normalize_to_wav16k(src: &Path, dst: &Path) -> Result<(), String> {
    let output = Command::new("/usr/bin/afconvert")
        .arg("-f")
        .arg("WAVE")
        .arg("-d")
        .arg("LEI16@16000")
        .arg(src)
        .arg(dst)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if message.is_empty() {
        format!("afconvert exited with status {}", output.status)
    } else {
        message
    })
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

fn write_transcript(dir: &Path, record: &MeetingRecord) -> std::io::Result<()> {
    fs::write(transcript_path(dir), record.transcript_text())
}

// ---------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------

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
    let id = allocate_meeting_id(save_root, timestamp, slug.as_deref()).map_err(|e| e.to_string())?;
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
        status: MeetingStatus::Pending,
        progress: MeetingProgress::default(),
        segments: Vec::new(),
        error: None,
    };
    record.save_atomic(&record_path(&dir)).map_err(|e| e.to_string())?;
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
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
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

pub fn delete_meeting(save_root: &Path, id: &str) -> Result<(), String> {
    if !is_valid_meeting_id(id) {
        return Err("invalid meeting id".to_string());
    }
    let dir = meeting_dir(save_root, id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Rewrite orphaned `processing` meetings (their worker thread died with the
/// app) to `failed` so the UI offers Resume. Called once on startup.
pub fn reconcile_interrupted(save_root: &Path) {
    let Ok(entries) = fs::read_dir(meetings_root(save_root)) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let path = record_path(&entry.path());
        if let Ok(mut record) = MeetingRecord::load(&path) {
            if matches!(record.status, MeetingStatus::Processing | MeetingStatus::Pending) {
                record.status = MeetingStatus::Failed;
                record.error = Some("Interrupted because the app was closed (resumable)".to_string());
                record.touch();
                let _ = record.save_atomic(&path);
            }
        }
    }
}

fn finish_failed(
    record: &mut MeetingRecord,
    record_path: &Path,
    emit: &MeetingEventCallback,
    error: String,
) -> Result<(), String> {
    record.status = MeetingStatus::Failed;
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

fn emit_progress_running(
    emit: &MeetingEventCallback,
    record: &MeetingRecord,
    elapsed_ms: u64,
    processed_at_start: u64,
) {
    let processed_this_run = record.progress.processed_ms.saturating_sub(processed_at_start);
    let remaining = record.duration_ms.saturating_sub(record.progress.processed_ms);
    emit(MeetingEvent::Progress(MeetingProgressEvent {
        id: record.id.clone(),
        status: record.status,
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
pub fn run_meeting_job(
    save_root: &Path,
    model_directory: &Path,
    id: &str,
    suppress_low_confidence: bool,
    cancel: Arc<AtomicBool>,
    emit: MeetingEventCallback,
) -> Result<(), String> {
    let dir = meeting_dir(save_root, id);
    let rpath = record_path(&dir);
    let mut record = MeetingRecord::load(&rpath).map_err(|e| e.to_string())?;
    let started = Instant::now();

    let model_path = model_directory.join(format!("{}.bin", record.model_id));
    if !model_path.exists() {
        return finish_failed(
            &mut record,
            &rpath,
            &emit,
            format!("model file not found: {}", model_path.display()),
        );
    }

    record.status = MeetingStatus::Processing;
    record.error = None;
    record.touch();
    let _ = record.save_atomic(&rpath);

    // 1. Normalize to a temp 16 kHz WAV (regenerated on resume, deleted at the end).
    let wav = work_wav_path(&dir);
    if let Err(error) = normalize_to_wav16k(&dir.join(&record.audio_file), &wav) {
        return finish_failed(&mut record, &rpath, &emit, format!("audio decode failed: {error}"));
    }

    // 2. Plan segments (only when fresh — resume keeps the existing plan).
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
        let spans = plan_segments(&frame_rms, FRAME_MS, total_ms, SegmentPlanOptions::default());
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
    let context = match WhisperContext::new_with_params(&model_path, default_whisper_context_parameters())
    {
        Ok(context) => Arc::new(context),
        Err(error) => {
            let _ = fs::remove_file(&wav);
            return finish_failed(&mut record, &rpath, &emit, format!("model load failed: {error}"));
        }
    };

    let language = record.language;
    let total_ms = record.duration_ms;
    let mut any_failed = record
        .segments
        .iter()
        .any(|segment| segment.status == MeetingSegmentStatus::Failed);

    for idx in 0..record.segments.len() {
        if record.segments[idx].status == MeetingSegmentStatus::Completed {
            continue;
        }
        if cancel.load(Ordering::Acquire) {
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
                eprintln!("[wakenote] meeting {}: segment {idx} read failed: {error}", record.id);
                record.segments[idx].status = MeetingSegmentStatus::Failed;
                any_failed = true;
                record.recompute_progress();
                record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
                record.touch();
                let _ = record.save_atomic(&rpath);
                emit_progress_running(&emit, &record, record.progress.elapsed_ms, processed_at_start);
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
            let overall = if total_ms > 0 {
                (processed * 100 / total_ms) as i32
            } else {
                0
            };
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
                segments_total: segs_total,
                segments_done: segs_done,
                processed_ms: processed,
                duration_ms: total_ms,
                elapsed_ms: elapsed,
                eta_ms: compute_eta_ms(elapsed, processed_this_run, remaining),
            }));
        };

        match transcribe_samples_with_context(
            &context,
            &samples,
            language,
            suppress_low_confidence,
            progress_cb,
        ) {
            Ok(decoded) => {
                record.segments[idx].status = MeetingSegmentStatus::Completed;
                record.segments[idx].text = decoded.text.clone();
                record.segments[idx].no_speech = decoded.no_speech;
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
                emit_progress_running(&emit, &record, record.progress.elapsed_ms, processed_at_start);
            }
            Err(error) => {
                eprintln!("[wakenote] meeting {}: segment {idx} decode failed: {error}", record.id);
                record.segments[idx].status = MeetingSegmentStatus::Failed;
                any_failed = true;
                record.recompute_progress();
                record.progress.elapsed_ms = started.elapsed().as_millis() as u64;
                record.touch();
                let _ = record.save_atomic(&rpath);
                emit_progress_running(&emit, &record, record.progress.elapsed_ms, processed_at_start);
            }
        }
    }

    let _ = fs::remove_file(&wav);
    record.status = if any_failed {
        MeetingStatus::Failed
    } else {
        MeetingStatus::Completed
    };
    record.error = if any_failed {
        Some("Some segments failed to transcribe".to_string())
    } else {
        None
    };
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

    fn rms_with_silence(total_frames: usize, silent: &[(usize, usize)], threshold_value: f32) -> Vec<f32> {
        let mut rms = vec![threshold_value; total_frames];
        for &(start, end) in silent {
            for frame in rms.iter_mut().take(end).skip(start) {
                *frame = 0.0;
            }
        }
        rms
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
        assert_eq!(slugify("Weekly Sync 2026").as_deref(), Some("weekly-sync-2026"));
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
            segments: Vec::new(),
            error: None,
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
