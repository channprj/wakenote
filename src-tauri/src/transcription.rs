use std::ffi::{CStr, c_char, c_uint, c_void};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once, OnceLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperSegment,
    get_lang_str,
};

use crate::audio_codec::convert_to_pcm_wav;
use crate::cloud_realtime::{RealtimeStoredResult, realtime_result_store};
use crate::cloud_transcription::{
    CloudTranscriptionClient, CloudTranscriptionError, FailureCategory, TranscriptionCredentials,
    TranscriptionFailure,
};
use crate::dictionary::DictionaryContext;
use crate::models::{
    ModelStore, OPENAI_GPT_4O_TRANSCRIBE_DIARIZE_MODEL, OPENAI_GPT_TRANSCRIBE_MODEL,
    default_model_registry,
};
use crate::queue::{QueueIssueCode, QueueJobIssue, QueueJobStatus, TranscriptionQueue};
use crate::recorded_realtime::{RecordedRealtimeOptions, transcribe_recorded_realtime};
use crate::recorder::{
    ChunkMetadata, ChunkSource, RecordedChunk, RecorderError, TranscriptionSidecar,
};
use crate::settings::{TranscriptionLanguage, expand_user_path};
use crate::soniox_async::SonioxAsyncClient;
use crate::transcription_cost::estimated_provider_cost_usd;

const WHISPER_SAMPLE_RATE: usize = 16_000;
const MIN_TRANSCRIBABLE_SAMPLES: usize = WHISPER_SAMPLE_RATE / 2;
const MIN_TRANSCRIBABLE_RMS: f32 = 0.003;
const MAX_NO_SPEECH_PROBABILITY: f32 = 0.75;
const MIN_AVERAGE_TOKEN_PROBABILITY: f32 = 0.20;
const MIN_DEGENERATE_TOKEN_COUNT: usize = 12;
const MIN_REPEATED_TOKEN_RUN: usize = 6;
const MIN_REPEATED_NGRAM_OCCURRENCES: usize = 3;
const MAX_REPEATED_NGRAM_WIDTH: usize = 8;
const MIN_REPEATED_NGRAM_COVERAGE_PERCENT: usize = 60;
const MIN_BRACKET_GROUPS: usize = 4;
const MIN_BRACKET_COVERAGE_PERCENT: usize = 50;
const MAX_LOW_DIVERSITY_PERCENT: usize = 35;
static NEXT_QWEN_INPUT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
pub struct TranscriptionRequest<'a> {
    pub audio_path: &'a Path,
    pub model_id: &'a str,
    pub language: TranscriptionLanguage,
    pub language_hints: &'a [TranscriptionLanguage],
    pub dictionary: &'a DictionaryContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerTurn {
    pub speaker_id: String,
    pub part_index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionUsage {
    pub provider: Option<String>,
    pub audio_duration_ms: u64,
    pub provider_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionExecution {
    pub text: String,
    pub speaker_turns: Vec<SpeakerTurn>,
    pub requested_model_id: String,
    pub effective_model_id: String,
    pub fallback_from_model_id: Option<String>,
    pub usage: Option<TranscriptionUsage>,
    pub issue: Option<QueueJobIssue>,
}

impl TranscriptionExecution {
    fn direct(text: String, model_id: &str) -> Self {
        Self {
            text,
            speaker_turns: Vec::new(),
            requested_model_id: model_id.to_string(),
            effective_model_id: model_id.to_string(),
            fallback_from_model_id: None,
            usage: None,
            issue: None,
        }
    }

    fn direct_with_issue(text: String, model_id: &str, issue: Option<QueueJobIssue>) -> Self {
        Self {
            issue,
            ..Self::direct(text, model_id)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TranscriptionError {
    #[error("{0}")]
    Failure(TranscriptionFailure),
    #[error("{primary}; fallback model {fallback_model_id} failed: {fallback}")]
    FallbackFailed {
        primary: Box<TranscriptionError>,
        fallback_model_id: String,
        fallback: Box<TranscriptionError>,
    },
    #[error("model file not found: {0}")]
    ModelMissing(PathBuf),
    #[error("unsupported audio format: {0}")]
    UnsupportedAudioFormat(String),
    #[error("m4a decode error: {0}")]
    M4a(String),
    #[error("wav decode error: {0}")]
    Wav(String),
    #[error("transcription engine error: {0}")]
    Engine(String),
}

impl TranscriptionError {
    fn recoverable_message(&self) -> String {
        match self {
            Self::Engine(message) => message.clone(),
            _ => self.to_string(),
        }
    }

    fn is_billing_limit(&self) -> bool {
        matches!(self, Self::Failure(failure) if failure.category == FailureCategory::BillingLimit)
    }

    fn queue_issue(&self) -> QueueJobIssue {
        let message = self.recoverable_message();
        match self {
            Self::Failure(failure) => match failure.category {
                FailureCategory::EmptyTranscript => {
                    QueueJobIssue::warning(QueueIssueCode::EmptyTranscript, message)
                }
                FailureCategory::BillingLimit => {
                    QueueJobIssue::error(QueueIssueCode::BillingLimit, message)
                }
                FailureCategory::RateLimit => {
                    QueueJobIssue::error(QueueIssueCode::RateLimit, message)
                }
                FailureCategory::Authentication => {
                    QueueJobIssue::error(QueueIssueCode::Authentication, message)
                }
                FailureCategory::Provider => {
                    QueueJobIssue::error(QueueIssueCode::Provider, message)
                }
                FailureCategory::Transport => {
                    QueueJobIssue::error(QueueIssueCode::Transport, message)
                }
                FailureCategory::InvalidResponse => {
                    QueueJobIssue::error(QueueIssueCode::InvalidResponse, message)
                }
                FailureCategory::Local => QueueJobIssue::error(QueueIssueCode::LocalIo, message),
                FailureCategory::Cancelled => {
                    QueueJobIssue::warning(QueueIssueCode::Cancelled, message)
                }
            },
            Self::ModelMissing(_) => QueueJobIssue::error(QueueIssueCode::Model, message),
            Self::UnsupportedAudioFormat(_) | Self::M4a(_) | Self::Wav(_) => {
                QueueJobIssue::error(QueueIssueCode::AudioDecode, message)
            }
            Self::FallbackFailed { .. } | Self::Engine(_) => {
                QueueJobIssue::error(QueueIssueCode::Unknown, message)
            }
        }
    }
}

pub trait Transcriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError>;

    fn transcribe_execution(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionExecution, TranscriptionError> {
        let model_id = request.model_id.to_string();
        self.transcribe(request)
            .map(|text| TranscriptionExecution::direct(text, &model_id))
    }
}

#[derive(Debug, Clone)]
pub struct FallbackTranscriber<P, L> {
    primary: P,
    local: L,
    fallback_model_id: Option<String>,
    fallback_supports_diarization: bool,
    diarization_required: bool,
}

impl<P, L> FallbackTranscriber<P, L> {
    pub fn new(
        primary: P,
        local: L,
        fallback_model_id: impl Into<String>,
        fallback_supports_diarization: bool,
        diarization_required: bool,
    ) -> Self {
        Self {
            primary,
            local,
            fallback_model_id: Some(fallback_model_id.into()),
            fallback_supports_diarization,
            diarization_required,
        }
    }

    pub fn configured(
        primary: P,
        local: L,
        fallback_model_id: Option<String>,
        fallback_supports_diarization: bool,
        diarization_required: bool,
    ) -> Self {
        Self {
            primary,
            local,
            fallback_model_id,
            fallback_supports_diarization,
            diarization_required,
        }
    }
}

impl<P: Transcriber, L: Transcriber> Transcriber for FallbackTranscriber<P, L> {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        self.transcribe_execution(request)
            .map(|execution| execution.text)
    }

    fn transcribe_execution(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionExecution, TranscriptionError> {
        match self.primary.transcribe_execution(request) {
            Ok(execution) => Ok(execution),
            Err(primary) if primary.is_billing_limit() => {
                let Some(fallback_model_id) = self.fallback_model_id.as_deref() else {
                    return Err(primary);
                };
                if self.diarization_required && !self.fallback_supports_diarization {
                    return Err(primary);
                }
                let requested_model_id = request.model_id.to_string();
                let fallback_request = TranscriptionRequest {
                    model_id: fallback_model_id,
                    ..request
                };
                match self.local.transcribe_execution(fallback_request) {
                    Ok(mut execution) => {
                        execution.requested_model_id = requested_model_id.clone();
                        execution.effective_model_id = fallback_model_id.to_string();
                        execution.fallback_from_model_id = Some(requested_model_id);
                        Ok(execution)
                    }
                    Err(fallback) => Err(TranscriptionError::FallbackFailed {
                        primary: Box::new(primary),
                        fallback_model_id: fallback_model_id.to_string(),
                        fallback: Box::new(fallback),
                    }),
                }
            }
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Error)]
pub enum TranscriptionWorkerError {
    #[error("recorder error: {0}")]
    Recorder(#[from] RecorderError),
    #[error("queue error: {0}")]
    Queue(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptionJobStatus {
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionJobOutcome {
    pub id: u64,
    pub status: TranscriptionJobStatus,
    pub requested_model_id: Option<String>,
    pub effective_model_id: Option<String>,
    pub fallback_from_model_id: Option<String>,
    pub usage: Option<TranscriptionUsage>,
    pub issue: Option<QueueJobIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptArtifactReason {
    WrappedPhrase,
    BroadcastNewsSignoff,
    CommonHallucination,
    RepeatedTokenRun,
    DominantRepeatedNgram,
    BracketFlood,
    CompoundSignals,
}

impl TranscriptArtifactReason {
    fn code(self) -> &'static str {
        match self {
            Self::WrappedPhrase => "wrapped-phrase",
            Self::BroadcastNewsSignoff => "broadcast-news-signoff",
            Self::CommonHallucination => "common-hallucination",
            Self::RepeatedTokenRun => "repeated-token-run",
            Self::DominantRepeatedNgram => "dominant-repeated-ngram",
            Self::BracketFlood => "bracket-flood",
            Self::CompoundSignals => "compound-signals",
        }
    }
}

impl TranscriptionJobOutcome {
    pub fn completed(id: u64) -> Self {
        Self {
            id,
            status: TranscriptionJobStatus::Completed,
            requested_model_id: None,
            effective_model_id: None,
            fallback_from_model_id: None,
            usage: None,
            issue: None,
        }
    }

    pub fn completed_with_execution(id: u64, execution: &TranscriptionExecution) -> Self {
        Self {
            id,
            status: TranscriptionJobStatus::Completed,
            requested_model_id: Some(execution.requested_model_id.clone()),
            effective_model_id: Some(execution.effective_model_id.clone()),
            fallback_from_model_id: execution.fallback_from_model_id.clone(),
            usage: execution.usage.clone(),
            issue: execution.issue.clone(),
        }
    }

    pub fn completed_with_issue(id: u64, issue: QueueJobIssue) -> Self {
        Self {
            id,
            status: TranscriptionJobStatus::Completed,
            requested_model_id: None,
            effective_model_id: None,
            fallback_from_model_id: None,
            usage: None,
            issue: Some(issue),
        }
    }

    pub fn failed(id: u64, error: impl Into<String>) -> Self {
        let error = error.into();
        Self::failed_with_issue(id, QueueJobIssue::error(QueueIssueCode::Unknown, error))
    }

    pub fn failed_with_issue(id: u64, issue: QueueJobIssue) -> Self {
        Self {
            id,
            status: TranscriptionJobStatus::Failed(issue.message.clone()),
            requested_model_id: None,
            effective_model_id: None,
            fallback_from_model_id: None,
            usage: None,
            issue: Some(issue),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionWorker<T> {
    transcriber: T,
    language: TranscriptionLanguage,
    language_hints: Vec<TranscriptionLanguage>,
    suppress_low_confidence_transcripts: bool,
    dictionary: DictionaryContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptionWorkerOptions {
    pub language: TranscriptionLanguage,
    pub suppress_low_confidence_transcripts: bool,
}

impl<T> TranscriptionWorker<T> {
    pub fn new(transcriber: T) -> Self {
        Self {
            transcriber,
            language: TranscriptionLanguage::Auto,
            language_hints: Vec::new(),
            suppress_low_confidence_transcripts: true,
            dictionary: DictionaryContext::default(),
        }
    }

    pub fn with_language(transcriber: T, language: TranscriptionLanguage) -> Self {
        Self::with_options(
            transcriber,
            TranscriptionWorkerOptions {
                language,
                suppress_low_confidence_transcripts: true,
            },
        )
    }

    pub fn with_options(transcriber: T, options: TranscriptionWorkerOptions) -> Self {
        Self {
            transcriber,
            language: options.language,
            language_hints: Vec::new(),
            suppress_low_confidence_transcripts: options.suppress_low_confidence_transcripts,
            dictionary: DictionaryContext::default(),
        }
    }

    pub fn with_options_and_dictionary(
        transcriber: T,
        options: TranscriptionWorkerOptions,
        dictionary: DictionaryContext,
    ) -> Self {
        Self {
            transcriber,
            language: options.language,
            language_hints: Vec::new(),
            suppress_low_confidence_transcripts: options.suppress_low_confidence_transcripts,
            dictionary,
        }
    }
    pub fn with_language_hints(mut self, hints: &[TranscriptionLanguage]) -> Self {
        self.language_hints = crate::settings::normalize_language_hints(hints);
        self
    }
}

impl<T: Transcriber> TranscriptionWorker<T> {
    pub fn process_started_job(
        &self,
        job: &crate::queue::QueueJob,
    ) -> Result<TranscriptionJobOutcome, TranscriptionWorkerError> {
        self.process_started_job_with_final(job, |_| {})
    }

    /// Final text is available before any sidecar write. The caller only
    /// registers background work here; it must not wait for network delivery.
    pub fn process_started_job_with_final(
        &self,
        job: &crate::queue::QueueJob,
        on_final: impl FnOnce(&TranscriptionExecution),
    ) -> Result<TranscriptionJobOutcome, TranscriptionWorkerError> {
        let chunk = RecordedChunk::from_audio_path(job.audio_path.clone());
        let request = TranscriptionRequest {
            audio_path: &job.audio_path,
            model_id: &job.model_id,
            language: self.language,
            language_hints: &self.language_hints,
            dictionary: &self.dictionary,
        };

        match self.transcriber.transcribe_execution(request) {
            Ok(mut execution) => {
                if let Some(usage) = execution.usage.as_mut() {
                    if usage.audio_duration_ms == 0 {
                        usage.audio_duration_ms = chunk_duration_ms(&chunk).unwrap_or(0);
                    }
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
                let transcript = execution.text.clone();
                let transcript = self.dictionary.correct(&transcript);
                if transcript.trim().is_empty() {
                    let issue = execution.issue.take().unwrap_or_else(|| {
                        QueueJobIssue::warning(QueueIssueCode::NoSpeech, "No speech detected")
                    });
                    TranscriptionSidecar::write_error(&chunk, &issue.message)?;
                    return Ok(TranscriptionJobOutcome::failed_with_issue(job.id, issue));
                }
                let suppress_artifacts = self.suppress_low_confidence_transcripts
                    && should_apply_artifact_suppression(&chunk);
                if suppress_artifacts && let Some(reason) = transcript_artifact_reason(&transcript)
                {
                    eprintln!(
                        "[wakenote] transcription: flagged artifact reason={} path={}; preserving archival transcript",
                        reason.code(),
                        job.audio_path.display()
                    );
                    execution.issue = Some(QueueJobIssue::warning(
                        QueueIssueCode::TranscriptArtifact,
                        format!("Low-confidence transcript: {}", reason.code()),
                    ));
                }
                execution.text = transcript;
                on_final(&execution);
                TranscriptionSidecar::write_success_with_provenance(
                    &chunk,
                    &execution.text,
                    &execution.requested_model_id,
                    &execution.effective_model_id,
                    execution.fallback_from_model_id.as_deref(),
                )?;
                Ok(TranscriptionJobOutcome::completed_with_execution(
                    job.id, &execution,
                ))
            }
            Err(error) => {
                let issue = error.queue_issue();
                TranscriptionSidecar::write_error(&chunk, &issue.message)?;
                Ok(TranscriptionJobOutcome::failed_with_issue(job.id, issue))
            }
        }
    }

    pub fn process_next(
        &self,
        queue: &mut TranscriptionQueue,
    ) -> Result<Option<u64>, TranscriptionWorkerError> {
        let Some(job) = queue.start_next() else {
            return Ok(None);
        };

        match self.process_started_job(&job) {
            Ok(outcome) => apply_outcome(queue, outcome)?,
            Err(error) => queue
                .mark_failed(job.id, error.to_string())
                .map_err(TranscriptionWorkerError::Queue)?,
        }

        Ok(Some(job.id))
    }
}

fn should_apply_artifact_suppression(chunk: &RecordedChunk) -> bool {
    !chunk_is_system_audio(chunk)
}

fn chunk_is_system_audio(chunk: &RecordedChunk) -> bool {
    let Ok(bytes) = std::fs::read(&chunk.metadata_path) else {
        return false;
    };
    let Ok(metadata) = serde_json::from_slice::<ChunkMetadata>(&bytes) else {
        return false;
    };
    metadata.source == ChunkSource::System
}

fn chunk_duration_ms(chunk: &RecordedChunk) -> Option<u64> {
    let bytes = std::fs::read(&chunk.metadata_path).ok()?;
    let metadata = serde_json::from_slice::<ChunkMetadata>(&bytes).ok()?;
    Some(metadata.duration_ms.max(0) as u64)
}

pub fn should_suppress_transcript_artifact(text: &str) -> bool {
    transcript_artifact_reason(text).is_some()
}

pub fn transcript_artifact_reason(text: &str) -> Option<TranscriptArtifactReason> {
    let normalized = normalize_transcript_whitespace(text);
    if normalized.is_empty() {
        return None;
    }

    if let Some(inner) = single_wrapped_phrase(&normalized) {
        let inner = inner.trim();
        if !inner.is_empty() && (inner.chars().count() <= 30 || contains_non_speech_marker(inner)) {
            return Some(TranscriptArtifactReason::WrappedPhrase);
        }
    }

    let lowercase = normalized.to_lowercase();
    let compact = normalized.split_whitespace().collect::<String>();
    let compact_lowercase = compact.to_lowercase();
    if is_broadcast_news_signoff(&normalized, &lowercase) {
        return Some(TranscriptArtifactReason::BroadcastNewsSignoff);
    }
    if is_common_hallucination(&lowercase, &compact_lowercase) {
        return Some(TranscriptArtifactReason::CommonHallucination);
    }

    let tokens = transcript_artifact_tokens(&lowercase);
    if has_repeated_token_run(&tokens) {
        return Some(TranscriptArtifactReason::RepeatedTokenRun);
    }
    if has_dominant_repeated_ngram(&tokens) {
        return Some(TranscriptArtifactReason::DominantRepeatedNgram);
    }
    if has_bracket_flood(&normalized) {
        return Some(TranscriptArtifactReason::BracketFlood);
    }

    let weak_signals = usize::from(normalized.contains('\u{fffd}'))
        + usize::from(contains_known_artifact_fragment(
            &lowercase,
            &compact_lowercase,
            &tokens,
        ))
        + usize::from(has_low_token_diversity(&tokens));
    if weak_signals >= 2 {
        return Some(TranscriptArtifactReason::CompoundSignals);
    }

    None
}

const COMMON_HALLUCINATIONS: [&str; 5] = [
    "thanks for watching",
    "thank you for watching",
    "시청해주셔서감사합니다",
    "시청해 주셔서 감사합니다",
    "끝까지시청해주셔서감사합니다",
];

fn is_common_hallucination(lowercase: &str, compact_lowercase: &str) -> bool {
    COMMON_HALLUCINATIONS
        .iter()
        .any(|marker| lowercase == *marker || compact_lowercase == marker.replace(' ', ""))
}

fn transcript_artifact_tokens(lowercase: &str) -> Vec<String> {
    lowercase
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn has_repeated_token_run(tokens: &[String]) -> bool {
    tokens
        .windows(MIN_REPEATED_TOKEN_RUN)
        .any(|window| window.iter().all(|token| token == &window[0]))
}

fn has_dominant_repeated_ngram(tokens: &[String]) -> bool {
    if tokens.len() < MIN_DEGENERATE_TOKEN_COUNT {
        return false;
    }

    for width in 2..=MAX_REPEATED_NGRAM_WIDTH.min(tokens.len()) {
        let mut occurrences = std::collections::HashMap::<&[String], (usize, usize)>::new();
        for (start, ngram) in tokens.windows(width).enumerate() {
            let (count, next_non_overlapping_start) = occurrences.entry(ngram).or_insert((0, 0));
            if start >= *next_non_overlapping_start {
                *count = count.saturating_add(1);
                *next_non_overlapping_start = start.saturating_add(width);
            }
        }
        if occurrences.values().any(|(count, _)| {
            *count >= MIN_REPEATED_NGRAM_OCCURRENCES
                && count.saturating_mul(width).saturating_mul(100)
                    >= tokens
                        .len()
                        .saturating_mul(MIN_REPEATED_NGRAM_COVERAGE_PERCENT)
        }) {
            return true;
        }
    }

    false
}

fn has_bracket_flood(text: &str) -> bool {
    let mut spans = balanced_wrapped_group_spans(text);
    spans.sort_unstable_by_key(|span| span.0);
    let mut merged_spans: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        if let Some((_, merged_end)) = merged_spans.last_mut()
            && start < *merged_end
        {
            *merged_end = (*merged_end).max(end);
            continue;
        }
        merged_spans.push((start, end));
    }
    if merged_spans.len() < MIN_BRACKET_GROUPS {
        return false;
    }

    let non_space_characters = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    let covered_characters = merged_spans
        .iter()
        .map(|(start, end)| {
            text[*start..*end]
                .chars()
                .filter(|character| !character.is_whitespace())
                .count()
        })
        .sum::<usize>();

    non_space_characters > 0
        && covered_characters.saturating_mul(100)
            >= non_space_characters.saturating_mul(MIN_BRACKET_COVERAGE_PERCENT)
}

fn balanced_wrapped_group_spans(text: &str) -> Vec<(usize, usize)> {
    let mut stack = Vec::new();
    let mut spans = Vec::new();

    for (index, character) in text.char_indices() {
        if let Some(close) = matching_close_bracket(character) {
            stack.push((close, index));
            continue;
        }
        if !is_close_bracket(character) {
            continue;
        }

        match stack.last().copied() {
            Some((expected, start)) if expected == character => {
                stack.pop();
                spans.push((start, index + character.len_utf8()));
            }
            _ => stack.clear(),
        }
    }

    spans
}

fn matching_close_bracket(character: char) -> Option<char> {
    match character {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '（' => Some('）'),
        '【' => Some('】'),
        _ => None,
    }
}

fn is_close_bracket(character: char) -> bool {
    matches!(character, ')' | ']' | '}' | '）' | '】')
}

fn contains_known_artifact_fragment(
    lowercase: &str,
    compact_lowercase: &str,
    tokens: &[String],
) -> bool {
    const BROADCASTERS: [&str; 5] = ["mbc", "kbs", "sbs", "ytn", "jtbc"];
    const COMPACT_BROADCAST_NEWS: [&str; 5] =
        ["mbc뉴스", "kbs뉴스", "sbs뉴스", "ytn뉴스", "jtbc뉴스"];

    tokens
        .windows(2)
        .any(|window| BROADCASTERS.contains(&window[0].as_str()) && window[1].as_str() == "뉴스")
        || tokens
            .iter()
            .any(|token| COMPACT_BROADCAST_NEWS.contains(&token.as_str()))
        || COMMON_HALLUCINATIONS.iter().any(|marker| {
            lowercase.contains(marker) || compact_lowercase.contains(&marker.replace(' ', ""))
        })
}

fn has_low_token_diversity(tokens: &[String]) -> bool {
    if tokens.len() < MIN_DEGENERATE_TOKEN_COUNT {
        return false;
    }

    let unique_tokens = tokens
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>()
        .len();
    unique_tokens.saturating_mul(100) <= tokens.len().saturating_mul(MAX_LOW_DIVERSITY_PERCENT)
}

fn is_broadcast_news_signoff(normalized: &str, lowercase: &str) -> bool {
    if normalized.chars().count() > 32 {
        return false;
    }
    let signoff_ending = normalized.ends_with("입니다.")
        || normalized.ends_with("입니다")
        || normalized.ends_with("였습니다.")
        || normalized.ends_with("였습니다");
    if !signoff_ending {
        return false;
    }

    [
        "mbc 뉴스 ",
        "kbs 뉴스 ",
        "sbs 뉴스 ",
        "ytn 뉴스 ",
        "jtbc 뉴스 ",
        "뉴스 ",
    ]
    .iter()
    .any(|prefix| lowercase.starts_with(prefix))
}

fn normalize_transcript_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn single_wrapped_phrase(text: &str) -> Option<&str> {
    let pairs = [
        ('(', ')'),
        ('[', ']'),
        ('{', '}'),
        ('（', '）'),
        ('【', '】'),
    ];
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    let (last_index, last) = text.char_indices().next_back()?;
    if last_index == 0 {
        return None;
    }

    pairs
        .iter()
        .find(|(open, close)| first == *open && last == *close)
        .map(|_| {
            let start = first.len_utf8();
            &text[start..last_index]
        })
}

fn contains_non_speech_marker(text: &str) -> bool {
    let lowercase = text.to_lowercase();
    [
        "웃음",
        "웃음소리",
        "음악",
        "박수",
        "소음",
        "잡음",
        "무음",
        "침묵",
        "laughter",
        "laugh",
        "music",
        "applause",
        "noise",
        "silence",
    ]
    .iter()
    .any(|marker| lowercase.contains(marker))
}

pub fn apply_outcome(
    queue: &mut TranscriptionQueue,
    outcome: TranscriptionJobOutcome,
) -> Result<(), TranscriptionWorkerError> {
    apply_outcome_at(queue, outcome, Utc::now())
}

pub fn apply_outcome_at(
    queue: &mut TranscriptionQueue,
    mut outcome: TranscriptionJobOutcome,
    occurred_at: DateTime<Utc>,
) -> Result<(), TranscriptionWorkerError> {
    if queue.job(outcome.id).is_some_and(|job| {
        matches!(
            job.status,
            QueueJobStatus::Cancelled | QueueJobStatus::Skipped
        )
    }) {
        return Ok(());
    }

    if outcome
        .issue
        .as_ref()
        .is_some_and(QueueJobIssue::is_discardable_activity_outcome)
    {
        queue.remove_jobs(&[outcome.id]);
        return Ok(());
    }

    if let Some(issue) = outcome.issue.as_mut() {
        issue.occurred_at = Some(occurred_at.to_rfc3339());
    }

    match outcome.status {
        TranscriptionJobStatus::Completed => match outcome.issue {
            Some(issue) => queue
                .mark_completed_with_issue(outcome.id, issue)
                .map_err(TranscriptionWorkerError::Queue),
            None => queue
                .mark_completed(outcome.id)
                .map_err(TranscriptionWorkerError::Queue),
        },
        TranscriptionJobStatus::Failed(error) => queue
            .mark_failed_with_issue(
                outcome.id,
                outcome
                    .issue
                    .unwrap_or_else(|| QueueJobIssue::error(QueueIssueCode::Unknown, error)),
            )
            .map_err(TranscriptionWorkerError::Queue),
    }
}

#[derive(Debug, Clone)]
pub struct WhisperTranscriber {
    model_directory: PathBuf,
    suppress_low_confidence_decode: bool,
}

impl WhisperTranscriber {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
            suppress_low_confidence_decode: true,
        }
    }

    fn model_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.bin"))
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        self.transcribe_execution(request)
            .map(|execution| execution.text)
    }

    fn transcribe_execution(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionExecution, TranscriptionError> {
        let model_path = self.model_path(request.model_id);
        if !model_path.exists() {
            return Err(TranscriptionError::ModelMissing(model_path));
        }

        let samples = decode_audio_for_whisper(request.audio_path)?;
        if should_skip_low_signal_audio(&samples) {
            return Ok(TranscriptionExecution::direct_with_issue(
                String::new(),
                request.model_id,
                Some(QueueJobIssue::warning(
                    QueueIssueCode::NoSpeech,
                    "No speech detected",
                )),
            ));
        }
        let decoded = run_whisper_with_quality(
            &model_path,
            &samples,
            request.language,
            self.suppress_low_confidence_decode,
            request.dictionary.prompt(),
        )?;
        Ok(TranscriptionExecution::direct_with_issue(
            decoded.text,
            request.model_id,
            decoded.issue,
        ))
    }
}

pub type TranscriptionPartialCallback = Arc<dyn Fn(String) + Send + Sync>;

#[derive(Clone)]
pub struct RuntimeTranscriber {
    model_directory: PathBuf,
    suppress_low_confidence_decode: bool,
    cloud: CloudTranscriptionClient,
    soniox_async: SonioxAsyncClient,
    streaming_enabled: bool,
    partial_callback: Option<TranscriptionPartialCallback>,
    recorded_realtime: Option<RecordedRealtimeOptions>,
}

impl fmt::Debug for RuntimeTranscriber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeTranscriber")
            .field("model_directory", &self.model_directory)
            .field(
                "suppress_low_confidence_decode",
                &self.suppress_low_confidence_decode,
            )
            .field("cloud", &self.cloud)
            .field("soniox_async", &self.soniox_async)
            .field("streaming_enabled", &self.streaming_enabled)
            .field(
                "partial_callback_configured",
                &self.partial_callback.is_some(),
            )
            .finish()
    }
}

impl RuntimeTranscriber {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
            suppress_low_confidence_decode: true,
            cloud: CloudTranscriptionClient::default(),
            soniox_async: SonioxAsyncClient::default(),
            streaming_enabled: false,
            partial_callback: None,
            recorded_realtime: None,
        }
    }

    pub fn for_dictation(model_directory: impl AsRef<Path>) -> Self {
        Self::for_archival(model_directory)
    }

    pub fn for_archival(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
            suppress_low_confidence_decode: false,
            cloud: CloudTranscriptionClient::default(),
            soniox_async: SonioxAsyncClient::default(),
            streaming_enabled: false,
            partial_callback: None,
            recorded_realtime: None,
        }
    }

    pub fn for_dictation_with_credentials(
        model_directory: impl AsRef<Path>,
        credentials: TranscriptionCredentials,
    ) -> Result<Self, CloudTranscriptionError> {
        Self::for_archival_with_credentials(model_directory, credentials)
    }

    pub fn for_archival_with_credentials(
        model_directory: impl AsRef<Path>,
        credentials: TranscriptionCredentials,
    ) -> Result<Self, CloudTranscriptionError> {
        let cloud = CloudTranscriptionClient::new(credentials.clone())?;
        let soniox_async = SonioxAsyncClient::new(credentials)?;
        Ok(Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
            suppress_low_confidence_decode: false,
            cloud,
            soniox_async,
            streaming_enabled: false,
            partial_callback: None,
            recorded_realtime: None,
        })
    }

    pub fn with_file_streaming(
        mut self,
        enabled: bool,
        partial_callback: Option<TranscriptionPartialCallback>,
    ) -> Self {
        self.streaming_enabled = enabled;
        self.partial_callback = partial_callback;
        self
    }

    fn model_runtime(&self, model_id: &str) -> String {
        model_runtime_for_id(&self.model_directory, model_id)
    }

    pub fn with_recorded_realtime(mut self, options: Option<RecordedRealtimeOptions>) -> Self {
        self.recorded_realtime = options;
        self
    }

    fn wait_for_realtime_result(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionExecution, TranscriptionError> {
        if let Some(options) = self.recorded_realtime.as_ref() {
            return transcribe_recorded_realtime(request, options);
        }
        match realtime_result_store().wait(request.audio_path) {
            RealtimeStoredResult::Completed(execution) => Ok(execution),
            RealtimeStoredResult::Failed(failure) => Err(TranscriptionError::Failure(failure)),
        }
    }
}

pub fn model_runtime_for_id(model_directory: impl AsRef<Path>, model_id: &str) -> String {
    let store = ModelStore::new(expand_user_path(model_directory.as_ref().to_string_lossy()));
    store
        .load_model_registry()
        .unwrap_or_else(|_| default_model_registry())
        .get(model_id)
        .map(|model| model.provider_runtime.clone())
        .unwrap_or_else(|| "whisper-rs".to_string())
}

pub fn model_supports_live_partials(model_directory: impl AsRef<Path>, model_id: &str) -> bool {
    model_runtime_for_id(model_directory, model_id) == "whisper-rs"
}

impl Transcriber for RuntimeTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        let store = ModelStore::new(&self.model_directory);
        let command_path = store.command_path(request.model_id);
        if command_path.exists() {
            ExternalCommandTranscriber::new(&self.model_directory).transcribe(request)
        } else {
            let runtime = self.model_runtime(request.model_id);
            if matches!(
                runtime.as_str(),
                "openrouter-stt" | "openai-stt" | "soniox-async-stt"
            ) && let Some(execution) =
                silent_audio_execution(request.audio_path, request.model_id)
            {
                return Ok(execution.text);
            }
            match runtime.as_str() {
                "sherpa-onnx" => transcribe_with_sherpa(&self.model_directory, request),
                "qwen3-asr" => Qwen3AsrTranscriber::new(&self.model_directory).transcribe(request),
                "external-command" => {
                    ExternalCommandTranscriber::new(&self.model_directory).transcribe(request)
                }
                "openrouter-stt" => self
                    .cloud
                    .transcribe_openrouter(request.audio_path, request.language)
                    .map_err(|error| TranscriptionError::Failure(error.into_failure())),
                "openai-stt" => self
                    .cloud
                    .transcribe_openai_model(
                        request.audio_path,
                        request.language,
                        request.dictionary,
                        match request.model_id {
                            "openai-gpt-4o-transcribe-diarize" => {
                                OPENAI_GPT_4O_TRANSCRIBE_DIARIZE_MODEL
                            }
                            _ => OPENAI_GPT_TRANSCRIBE_MODEL,
                        },
                        self.streaming_enabled,
                        &mut |text| {
                            if let Some(callback) = self.partial_callback.as_ref() {
                                callback(text.to_string());
                            }
                        },
                    )
                    .map_err(|error| TranscriptionError::Failure(error.into_failure())),
                "openai-realtime" | "soniox-realtime" => self
                    .wait_for_realtime_result(request)
                    .map(|execution| execution.text),
                "soniox-async-stt" => self
                    .soniox_async
                    .transcribe(
                        request.audio_path,
                        request.language_hints,
                        request.dictionary,
                    )
                    .map_err(|error| TranscriptionError::Failure(error.into_failure())),
                _ => {
                    let mut transcriber = WhisperTranscriber::new(&self.model_directory);
                    transcriber.suppress_low_confidence_decode =
                        self.suppress_low_confidence_decode;
                    transcriber.transcribe(request)
                }
            }
        }
    }

    fn transcribe_execution(
        &self,
        request: TranscriptionRequest<'_>,
    ) -> Result<TranscriptionExecution, TranscriptionError> {
        let runtime = self.model_runtime(request.model_id);
        let command_path = ModelStore::new(&self.model_directory).command_path(request.model_id);
        if runtime == "whisper-rs" && !command_path.exists() {
            let mut transcriber = WhisperTranscriber::new(&self.model_directory);
            transcriber.suppress_low_confidence_decode = self.suppress_low_confidence_decode;
            return transcriber.transcribe_execution(request);
        }
        if matches!(
            runtime.as_str(),
            "openrouter-stt" | "openai-stt" | "soniox-async-stt"
        ) && let Some(execution) = silent_audio_execution(request.audio_path, request.model_id)
        {
            return Ok(execution);
        }
        if matches!(runtime.as_str(), "openai-realtime" | "soniox-realtime") {
            return self.wait_for_realtime_result(request);
        }
        if runtime == "openai-stt" {
            let model_id = request.model_id.to_string();
            let provider_model = match request.model_id {
                "openai-gpt-4o-transcribe-diarize" => OPENAI_GPT_4O_TRANSCRIBE_DIARIZE_MODEL,
                _ => OPENAI_GPT_TRANSCRIBE_MODEL,
            };
            return self
                .cloud
                .transcribe_openai_model_result(
                    request.audio_path,
                    request.language,
                    request.dictionary,
                    provider_model,
                    self.streaming_enabled,
                    &mut |text| {
                        if let Some(callback) = self.partial_callback.as_ref() {
                            callback(text.to_string());
                        }
                    },
                )
                .map(|result| TranscriptionExecution {
                    text: result.text,
                    speaker_turns: result
                        .speaker_turns
                        .into_iter()
                        .map(|turn| SpeakerTurn {
                            speaker_id: turn.speaker_id,
                            part_index: 0,
                            start_ms: turn.start_ms,
                            end_ms: turn.end_ms,
                            text: turn.text,
                        })
                        .collect(),
                    requested_model_id: model_id.clone(),
                    effective_model_id: model_id,
                    fallback_from_model_id: None,
                    usage: Some(TranscriptionUsage {
                        provider: Some("OpenAI".to_string()),
                        audio_duration_ms: 0,
                        provider_cost_usd: None,
                    }),
                    issue: None,
                })
                .map_err(|error| TranscriptionError::Failure(error.into_failure()));
        }
        if runtime == "openrouter-stt" {
            let model_id = request.model_id.to_string();
            return self
                .cloud
                .transcribe_openrouter(request.audio_path, request.language)
                .map(|text| TranscriptionExecution {
                    text,
                    speaker_turns: Vec::new(),
                    requested_model_id: model_id.clone(),
                    effective_model_id: model_id,
                    fallback_from_model_id: None,
                    usage: Some(TranscriptionUsage {
                        provider: Some("OpenRouter".to_string()),
                        audio_duration_ms: 0,
                        provider_cost_usd: None,
                    }),
                    issue: None,
                })
                .map_err(|error| TranscriptionError::Failure(error.into_failure()));
        }
        if runtime == "soniox-async-stt" {
            let model_id = request.model_id.to_string();
            return self
                .soniox_async
                .transcribe(
                    request.audio_path,
                    request.language_hints,
                    request.dictionary,
                )
                .map(|text| TranscriptionExecution {
                    text,
                    speaker_turns: Vec::new(),
                    requested_model_id: model_id.clone(),
                    effective_model_id: model_id,
                    fallback_from_model_id: None,
                    usage: Some(TranscriptionUsage {
                        provider: Some("Soniox".to_string()),
                        audio_duration_ms: 0,
                        provider_cost_usd: None,
                    }),
                    issue: None,
                })
                .map_err(|error| TranscriptionError::Failure(error.into_failure()));
        }
        let model_id = request.model_id.to_string();
        self.transcribe(request)
            .map(|text| TranscriptionExecution::direct(text, &model_id))
    }
}

fn qwen3_asr_language_name(language: TranscriptionLanguage) -> Option<&'static str> {
    match language {
        TranscriptionLanguage::Auto => None,
        TranscriptionLanguage::Ko => Some("Korean"),
        TranscriptionLanguage::En => Some("English"),
        TranscriptionLanguage::Ja => Some("Japanese"),
        TranscriptionLanguage::Zh => Some("Chinese"),
        TranscriptionLanguage::Es => Some("Spanish"),
        TranscriptionLanguage::Fr => Some("French"),
        TranscriptionLanguage::De => Some("German"),
    }
}

#[derive(Debug, Clone)]
pub struct Qwen3AsrTranscriber {
    model_directory: PathBuf,
}

impl Qwen3AsrTranscriber {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
        }
    }
}

impl Transcriber for Qwen3AsrTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        let store = ModelStore::new(&self.model_directory);
        let python = store.qwen3_asr_python_path();
        let runner = store.qwen3_asr_runner_path();
        let model_path = store.qwen3_asr_model_dir(request.model_id).ok_or_else(|| {
            TranscriptionError::Engine(format!("unknown Qwen3-ASR model {}", request.model_id))
        })?;
        for (label, path) in [
            ("Python runtime", &python),
            ("WakeNote runner", &runner),
            ("model directory", &model_path),
        ] {
            if !path.exists() {
                return Err(TranscriptionError::Engine(format!(
                    "Qwen3-ASR {label} is missing at {}; run Set up Qwen3-ASR in Models",
                    path.display()
                )));
            }
        }

        let samples = decode_audio_for_whisper(request.audio_path)?;
        if should_skip_low_signal_audio(&samples) {
            return Ok(String::new());
        }
        let audio_path = next_qwen_input_path();
        write_qwen_input_wav(&audio_path, &samples)?;
        let mut command = Command::new(&python);
        command
            .arg(&runner)
            .env("WAKENOTE_AUDIO_PATH", &audio_path)
            .env("WAKENOTE_QWEN3_ASR_MODEL_PATH", &model_path)
            .env(
                "WAKENOTE_QWEN3_ASR_LANGUAGE",
                qwen3_asr_language_name(request.language).unwrap_or(""),
            )
            .env(
                "WAKENOTE_QWEN3_ASR_PROMPT",
                request.dictionary.prompt().unwrap_or(""),
            )
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("TOKENIZERS_PARALLELISM", "false")
            .env("PYTHONNOUSERSITE", "1");
        let output = command.output();
        let _ = std::fs::remove_file(&audio_path);
        let output = output.map_err(|error| TranscriptionError::Engine(error.to_string()))?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(TranscriptionError::Engine(if stderr.is_empty() {
            format!("Qwen3-ASR exited with {}", output.status)
        } else {
            stderr
        }))
    }
}

fn next_qwen_input_path() -> PathBuf {
    let id = NEXT_QWEN_INPUT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "wakenote-qwen3-asr-{}-{id}.wav",
        std::process::id()
    ))
}

fn write_qwen_input_wav(path: &Path, samples: &[f32]) -> Result<(), TranscriptionError> {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: WHISPER_SAMPLE_RATE as u32,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|error| TranscriptionError::Wav(error.to_string()))?;
    for sample in samples {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .map_err(|error| TranscriptionError::Wav(error.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|error| TranscriptionError::Wav(error.to_string()))
}

/// Map the requested transcription language to a SenseVoice language code.
/// SenseVoice recognizes zh/en/ja/ko/yue; anything else (including `Auto` and
/// languages the model doesn't cover) falls back to "auto" detection.
#[cfg(feature = "asr-sherpa")]
fn sense_voice_language(language: TranscriptionLanguage) -> String {
    match language.whisper_code() {
        Some(code @ ("zh" | "en" | "ja" | "ko")) => code.to_string(),
        _ => "auto".to_string(),
    }
}

/// Run a sherpa-onnx model (Parakeet transducer / SenseVoice) in-process. Built
/// only with the `asr-sherpa` feature; otherwise returns an actionable error.
#[cfg(feature = "asr-sherpa")]
fn transcribe_with_sherpa(
    model_directory: &Path,
    request: TranscriptionRequest<'_>,
) -> Result<String, TranscriptionError> {
    use crate::models::{SherpaModelKind, sherpa_model_spec};

    let spec = sherpa_model_spec(request.model_id).ok_or_else(|| {
        TranscriptionError::Engine(format!("no sherpa-onnx layout for {}", request.model_id))
    })?;
    let dir = ModelStore::new(model_directory)
        .sherpa_model_dir(request.model_id)
        .ok_or_else(|| TranscriptionError::Engine("sherpa-onnx model directory missing".into()))?;
    let samples = decode_audio_for_whisper(request.audio_path)?;
    if should_skip_low_signal_audio(&samples) {
        return Ok(String::new());
    }
    let file = |name: &str| dir.join(name).to_string_lossy().into_owned();

    match spec.kind {
        SherpaModelKind::Transducer => {
            // A NeMo/Parakeet transducer has no language parameter — the decoded
            // language is fixed by the trained model, so `request.language` does
            // not apply here (unlike Whisper/SenseVoice).
            let config = sherpa_rs::transducer::TransducerConfig {
                encoder: file("encoder.int8.onnx"),
                decoder: file("decoder.int8.onnx"),
                joiner: file("joiner.int8.onnx"),
                tokens: file("tokens.txt"),
                num_threads: 2,
                sample_rate: 16_000,
                feature_dim: 80,
                model_type: "nemo_transducer".to_string(),
                provider: Some("cpu".to_string()),
                ..Default::default()
            };
            let mut recognizer = sherpa_rs::transducer::TransducerRecognizer::new(config)
                .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
            Ok(recognizer.transcribe(16_000, &samples).trim().to_string())
        }
        SherpaModelKind::OnlineTransducer => {
            transcribe_with_online_transducer(&dir, &samples, request.language)
        }
        SherpaModelKind::SenseVoice => {
            let config = sherpa_rs::sense_voice::SenseVoiceConfig {
                model: file("model.int8.onnx"),
                tokens: file("tokens.txt"),
                language: sense_voice_language(request.language),
                num_threads: Some(2),
                provider: Some("cpu".to_string()),
                ..Default::default()
            };
            let mut recognizer = sherpa_rs::sense_voice::SenseVoiceRecognizer::new(config)
                .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
            Ok(recognizer
                .transcribe(16_000, &samples)
                .text
                .trim()
                .to_string())
        }
    }
}

/// Decode a full clip with a cache-aware streaming transducer (Nemotron 3.5 ASR)
/// via sherpa-onnx's online recognizer. The whole clip is fed as one stream and the
/// decoder drained — the streaming model is used for batch file transcription, not
/// live partials. `sherpa-rs` only wraps the offline recognizer, so the online C API
/// is called directly through the re-exported `sherpa_rs_sys`. Most `const char*`
/// config fields are null-guarded by the C layer (`SHERPA_ONNX_OR`), but the feature
/// dimension must match the model: Nemotron 3.5 exports 128-dim features (its encoder
/// rejects the 80 the offline transducers use). Nemotron is a prompt-conditioned
/// multilingual model, so the language is selected per stream via the `language`
/// option (empty/`auto` lets the model auto-detect).
#[cfg(feature = "asr-sherpa")]
fn transcribe_with_online_transducer(
    dir: &Path,
    samples: &[f32],
    language: TranscriptionLanguage,
) -> Result<String, TranscriptionError> {
    use sherpa_rs::sherpa_rs_sys as sys;
    use std::ffi::{CStr, CString};
    use std::mem;

    let to_cstring = |name: &str| {
        CString::new(dir.join(name).to_string_lossy().into_owned())
            .map_err(|error| TranscriptionError::Engine(error.to_string()))
    };
    let encoder = to_cstring("encoder.int8.onnx")?;
    let decoder = to_cstring("decoder.int8.onnx")?;
    let joiner = to_cstring("joiner.int8.onnx")?;
    let tokens = to_cstring("tokens.txt")?;
    let provider = CString::new("cpu").expect("\"cpu\" has no interior NUL");

    // SAFETY: `config` is a `#[repr(C)]` POD struct whose all-zero bit pattern is
    // valid (null `const char*`, zero ints/floats). The C layer substitutes defaults
    // for unset fields. The `CString`s outlive the `Create` call, which copies them
    // into owned `std::string`s, and every raw pointer is freed before returning.
    unsafe {
        let mut config: sys::SherpaOnnxOnlineRecognizerConfig = mem::zeroed();
        config.feat_config.sample_rate = 16_000;
        config.feat_config.feature_dim = 128;
        config.model_config.transducer.encoder = encoder.as_ptr();
        config.model_config.transducer.decoder = decoder.as_ptr();
        config.model_config.transducer.joiner = joiner.as_ptr();
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.num_threads = 2;
        config.model_config.provider = provider.as_ptr();

        let recognizer = sys::SherpaOnnxCreateOnlineRecognizer(&config);
        if recognizer.is_null() {
            return Err(TranscriptionError::Engine(
                "SherpaOnnxCreateOnlineRecognizer failed".to_string(),
            ));
        }
        let stream = sys::SherpaOnnxCreateOnlineStream(recognizer);
        if stream.is_null() {
            sys::SherpaOnnxDestroyOnlineRecognizer(recognizer);
            return Err(TranscriptionError::Engine(
                "SherpaOnnxCreateOnlineStream failed".to_string(),
            ));
        }

        // Select the prompt language for this stream. `None` (Auto) maps to the
        // model's built-in auto-detect prompt.
        let language_key = CString::new("language").expect("\"language\" has no interior NUL");
        let language_value =
            CString::new(language.whisper_code().unwrap_or("auto")).unwrap_or_default();
        sys::SherpaOnnxOnlineStreamSetOption(
            stream,
            language_key.as_ptr(),
            language_value.as_ptr(),
        );

        sys::SherpaOnnxOnlineStreamAcceptWaveform(
            stream,
            16_000,
            samples.as_ptr(),
            samples.len().try_into().unwrap_or(i32::MAX),
        );
        sys::SherpaOnnxOnlineStreamInputFinished(stream);
        while sys::SherpaOnnxIsOnlineStreamReady(recognizer, stream) != 0 {
            sys::SherpaOnnxDecodeOnlineStream(recognizer, stream);
        }

        let result_ptr = sys::SherpaOnnxGetOnlineStreamResult(recognizer, stream);
        let text = if result_ptr.is_null() {
            String::new()
        } else {
            let raw = result_ptr.read();
            let text = if raw.text.is_null() {
                String::new()
            } else {
                CStr::from_ptr(raw.text).to_string_lossy().into_owned()
            };
            sys::SherpaOnnxDestroyOnlineRecognizerResult(result_ptr);
            text
        };

        sys::SherpaOnnxDestroyOnlineStream(stream);
        sys::SherpaOnnxDestroyOnlineRecognizer(recognizer);
        Ok(text.trim().to_string())
    }
}

#[cfg(not(feature = "asr-sherpa"))]
fn transcribe_with_sherpa(
    _model_directory: &Path,
    _request: TranscriptionRequest<'_>,
) -> Result<String, TranscriptionError> {
    Err(TranscriptionError::Engine(
        "sherpa-onnx runtime is not built into this binary (rebuild with the asr-sherpa feature)"
            .to_string(),
    ))
}

#[derive(Debug, Clone)]
pub struct ExternalCommandTranscriber {
    model_directory: PathBuf,
}

impl ExternalCommandTranscriber {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
        }
    }

    fn command_path(&self, model_id: &str) -> PathBuf {
        ModelStore::new(&self.model_directory).command_path(model_id)
    }
}

impl Transcriber for ExternalCommandTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        let command_path = self.command_path(request.model_id);
        let command = std::fs::read_to_string(&command_path).map_err(|error| {
            TranscriptionError::Engine(format!(
                "external ASR command not found at {}: {error}",
                command_path.display()
            ))
        })?;
        let command = command.trim();
        if command.is_empty() {
            return Err(TranscriptionError::Engine(format!(
                "external ASR command is empty at {}",
                command_path.display()
            )));
        }

        let output = Command::new("/bin/sh")
            .arg("-lc")
            .arg(command)
            .env("WAKENOTE_AUDIO_PATH", request.audio_path)
            .env("WAKENOTE_MODEL_ID", request.model_id)
            .env("WAKENOTE_MODEL_DIRECTORY", &self.model_directory)
            .env(
                "WAKENOTE_LANGUAGE",
                request.language.whisper_code().unwrap_or("auto"),
            )
            .env(
                "WAKENOTE_DICTIONARY_TERMS",
                request.dictionary.canonical_terms().join(","),
            )
            .env(
                "WAKENOTE_DICTIONARY_JSON",
                request.dictionary.serialized_entries(),
            )
            .output()
            .map_err(|error| TranscriptionError::Engine(error.to_string()))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(TranscriptionError::Engine(if stderr.is_empty() {
            format!("external ASR command exited with {}", output.status)
        } else {
            stderr
        }))
    }
}

pub fn should_skip_low_signal_audio(samples: &[f32]) -> bool {
    if samples.len() < MIN_TRANSCRIBABLE_SAMPLES {
        return true;
    }

    let mean_square = samples
        .iter()
        .map(|sample| sample.clamp(-1.0, 1.0).powi(2))
        .sum::<f32>()
        / samples.len() as f32;
    mean_square.sqrt() < MIN_TRANSCRIBABLE_RMS
}

/// Decodes the audio and reports an empty NoSpeech execution when it is
/// effectively silent, so cloud runtimes can skip the API call instead of
/// paying to transcribe (and hallucinate on) silence. Returns `None` when the
/// audio should be transcribed normally — including when a local decode is
/// impossible (e.g. missing file), so the cloud call keeps reporting its own
/// errors such as missing API keys.
fn silent_audio_execution(audio_path: &Path, model_id: &str) -> Option<TranscriptionExecution> {
    let Ok(samples) = decode_audio_for_whisper(audio_path) else {
        return None;
    };
    if !should_skip_low_signal_audio(&samples) {
        return None;
    }
    Some(TranscriptionExecution::direct_with_issue(
        String::new(),
        model_id,
        Some(QueueJobIssue::warning(
            QueueIssueCode::NoSpeech,
            "No speech detected",
        )),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedSegmentQuality {
    pub no_speech_probability: f32,
    pub average_token_probability: f32,
}

pub fn should_suppress_low_confidence_decode(
    text: &str,
    qualities: &[DecodedSegmentQuality],
) -> bool {
    if text.trim().is_empty() || qualities.is_empty() {
        return false;
    }

    let max_no_speech = qualities
        .iter()
        .map(|quality| quality.no_speech_probability)
        .fold(0.0_f32, f32::max);
    if max_no_speech >= MAX_NO_SPEECH_PROBABILITY {
        return true;
    }

    let average_token_probability = qualities
        .iter()
        .map(|quality| quality.average_token_probability)
        .sum::<f32>()
        / qualities.len() as f32;
    average_token_probability < MIN_AVERAGE_TOKEN_PROBABILITY
}

pub(crate) fn decoded_segment_quality(segment: &WhisperSegment<'_>) -> DecodedSegmentQuality {
    let token_count = segment.n_tokens();
    let average_token_probability = if token_count <= 0 {
        0.0
    } else {
        (0..token_count)
            .filter_map(|token_index| segment.get_token(token_index))
            .map(|token| token.token_probability())
            .sum::<f32>()
            / token_count as f32
    };

    DecodedSegmentQuality {
        no_speech_probability: segment.no_speech_probability(),
        average_token_probability,
    }
}

pub(crate) fn configure_whisper_language(
    params: &mut FullParams<'_, '_>,
    language: TranscriptionLanguage,
) {
    match language.whisper_code() {
        Some(code) => params.set_language(Some(code)),
        None => {
            params.set_language(None);
            params.set_detect_language(true);
        }
    }
}

/// Forward whisper.cpp / GGML internal logs (system info, execution-backend
/// selection, timings) into our `[wakenote]` stderr stream. whisper.cpp emits a
/// line like `whisper_backend_init_gpu: using Metal backend` at load time;
/// capturing it lets us confirm from a user's logs whether inference actually
/// ran on the Metal GPU or silently fell back to CPU (e.g. on Intel Macs, or
/// under memory pressure with large models such as Whisper Medium).
unsafe extern "C" fn whisper_log_trampoline(
    // `ggml_log_level` is a `c_uint` typedef; naming the sys type would require
    // whisper-rs's `raw-api` feature, so we use the transparent alias directly.
    _level: c_uint,
    text: *const c_char,
    _user_data: *mut c_void,
) {
    if text.is_null() {
        return;
    }
    let message = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    let message = message.trim_end();
    if !message.is_empty() {
        eprintln!("[wakenote] whisper: {message}");
    }
}

/// Install the whisper.cpp log callback exactly once. Invoked lazily from
/// [`default_whisper_context_parameters`] so every context-creation path routes
/// whisper's backend logs through ours before the backend is initialized,
/// regardless of which entry point loaded the model first.
fn install_whisper_logging() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        whisper_rs::set_log_callback(Some(whisper_log_trampoline), std::ptr::null_mut());
    });
}

pub fn default_whisper_context_parameters() -> WhisperContextParameters<'static> {
    install_whisper_logging();
    let mut params = WhisperContextParameters::default();
    // whisper.cpp uses the Metal GPU backend by default on macOS (the `metal`
    // feature defaults `use_gpu` to true); device 0 selects the primary GPU.
    params.gpu_device(0);
    params
}

struct LoadedReusableContext<T> {
    model_path: PathBuf,
    context: Arc<T>,
}

struct ReusableContextCache<T> {
    loaded: Option<LoadedReusableContext<T>>,
}

impl<T> Default for ReusableContextCache<T> {
    fn default() -> Self {
        Self { loaded: None }
    }
}

impl<T> ReusableContextCache<T> {
    fn get_or_try_load<E>(
        &mut self,
        model_path: &Path,
        load: impl FnOnce() -> Result<T, E>,
    ) -> Result<Arc<T>, E> {
        if let Some(loaded) = self
            .loaded
            .as_ref()
            .filter(|loaded| loaded.model_path == model_path)
        {
            return Ok(loaded.context.clone());
        }

        let context = Arc::new(load()?);
        self.loaded = Some(LoadedReusableContext {
            model_path: model_path.to_path_buf(),
            context: context.clone(),
        });
        Ok(context)
    }
}

pub fn cached_whisper_context(
    model_path: &Path,
) -> Result<Arc<WhisperContext>, TranscriptionError> {
    if !model_path.exists() {
        return Err(TranscriptionError::ModelMissing(model_path.to_path_buf()));
    }

    static CACHE: OnceLock<Mutex<ReusableContextCache<WhisperContext>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(ReusableContextCache::default()))
        .lock()
        .map_err(|error| {
            TranscriptionError::Engine(format!("Whisper context cache lock failed: {error}"))
        })?;
    cache.get_or_try_load(model_path, || {
        let started = std::time::Instant::now();
        eprintln!(
            "[wakenote] whisper: loading reusable context from {}",
            model_path.display()
        );
        let context =
            WhisperContext::new_with_params(model_path, default_whisper_context_parameters())
                .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
        eprintln!(
            "[wakenote] whisper: reusable context loaded in {:?}",
            started.elapsed()
        );
        Ok(context)
    })
}

struct WhisperDecodeResult {
    text: String,
    issue: Option<QueueJobIssue>,
}

fn run_whisper_with_quality(
    model_path: &Path,
    samples: &[f32],
    language: TranscriptionLanguage,
    suppress_low_confidence_decode: bool,
    initial_prompt: Option<&str>,
) -> Result<WhisperDecodeResult, TranscriptionError> {
    let context = cached_whisper_context(model_path)?;
    let (language_code, detect_language) = match language.whisper_code() {
        Some(code) => (Some(code), false),
        None => (None, true),
    };
    let (mut transcript, mut qualities, detected_language) = run_whisper_pass(
        &context,
        samples,
        language_code,
        detect_language,
        initial_prompt,
    )?;
    if let Some(retry_language) = dictation_whisper_retry_language(
        language,
        suppress_low_confidence_decode,
        &transcript,
        detected_language,
    ) {
        (transcript, qualities, _) = run_whisper_pass(
            &context,
            samples,
            Some(retry_language),
            false,
            initial_prompt,
        )?;
    }

    let low_confidence = should_suppress_low_confidence_decode(&transcript, &qualities);
    let issue = low_confidence.then(|| {
        QueueJobIssue::warning(QueueIssueCode::LowConfidence, "Low-confidence transcript")
    });
    Ok(WhisperDecodeResult {
        text: if suppress_low_confidence_decode && low_confidence {
            String::new()
        } else {
            transcript.trim().to_string()
        },
        issue,
    })
}

fn run_whisper_pass(
    context: &WhisperContext,
    samples: &[f32],
    language: Option<&str>,
    detect_language: bool,
    initial_prompt: Option<&str>,
) -> Result<(String, Vec<DecodedSegmentQuality>, Option<&'static str>), TranscriptionError> {
    let mut state = context
        .create_state()
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    params.set_language(language);
    params.set_detect_language(detect_language);
    if let Some(initial_prompt) = initial_prompt {
        params.set_initial_prompt(initial_prompt);
    }

    state
        .full(params, samples)
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;

    let mut transcript = String::new();
    let mut qualities = Vec::new();
    for segment in state.as_iter() {
        transcript.push_str(&segment.to_string());
        qualities.push(decoded_segment_quality(&segment));
    }
    let detected_language = get_lang_str(state.full_lang_id_from_state());
    Ok((transcript.trim().to_string(), qualities, detected_language))
}

fn dictation_whisper_retry_language(
    language: TranscriptionLanguage,
    suppress_low_confidence_decode: bool,
    transcript: &str,
    detected_language: Option<&'static str>,
) -> Option<&'static str> {
    (language == TranscriptionLanguage::Auto
        && !suppress_low_confidence_decode
        && transcript.trim().is_empty())
    .then_some(detected_language)
    .flatten()
}

#[cfg(test)]
fn finalize_whisper_transcript(
    transcript: &str,
    qualities: &[DecodedSegmentQuality],
    suppress_low_confidence_decode: bool,
) -> String {
    let transcript = transcript.trim();
    if suppress_low_confidence_decode
        && should_suppress_low_confidence_decode(transcript, qualities)
    {
        String::new()
    } else {
        transcript.to_string()
    }
}

/// Text decoded from a single audio window using a pre-loaded context.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecodedWindow {
    pub text: String,
    pub no_speech: bool,
}

/// Transcribe a slice of 16 kHz mono samples with an already-loaded context.
///
/// Unlike [`run_whisper`], the caller owns the [`WhisperContext`] so it can be
/// reused across the many segments of one long recording — loading the medium
/// model per segment would dominate the runtime. `progress` receives whisper's
/// 0-100 progress for this window so callers can surface near-realtime status.
pub fn transcribe_samples_with_context(
    context: &WhisperContext,
    samples: &[f32],
    language: TranscriptionLanguage,
    suppress_low_confidence: bool,
    dictionary: &DictionaryContext,
    progress: impl FnMut(i32) + 'static,
) -> Result<DecodedWindow, TranscriptionError> {
    let mut state = context
        .create_state()
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    params.set_progress_callback_safe(progress);
    configure_whisper_language(&mut params, language);
    if let Some(initial_prompt) = dictionary.prompt() {
        params.set_initial_prompt(initial_prompt);
    }

    state
        .full(params, samples)
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;

    let mut text = String::new();
    let mut qualities = Vec::new();
    let mut max_no_speech = 0.0_f32;
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
        let quality = decoded_segment_quality(&segment);
        max_no_speech = max_no_speech.max(quality.no_speech_probability);
        qualities.push(quality);
    }
    let text = text.trim().to_string();
    let suppressed = suppress_low_confidence
        && (should_suppress_transcript_artifact(&text)
            || should_suppress_low_confidence_decode(&text, &qualities));
    Ok(DecodedWindow {
        text: if suppressed {
            String::new()
        } else {
            dictionary.correct(&text)
        },
        no_speech: max_no_speech >= MAX_NO_SPEECH_PROBABILITY,
    })
}

pub fn decode_audio_for_whisper(path: &Path) -> Result<Vec<f32>, TranscriptionError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    if matches_ignore_ascii_case(extension, &["m4a", "mp3"]) {
        return read_native_audio_as_whisper_audio(path);
    }

    read_wav_as_whisper_audio(path)
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn read_wav_as_whisper_audio(path: &Path) -> Result<Vec<f32>, TranscriptionError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| TranscriptionError::Wav(error.to_string()))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map_err(|error| TranscriptionError::Wav(error.to_string())))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => reader
            .samples::<i16>()
            .map(|sample| {
                sample
                    .map(|sample| sample as f32 / i16::MAX as f32)
                    .map_err(|error| TranscriptionError::Wav(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let max = ((1_i64 << (u32::from(spec.bits_per_sample) - 1)) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|sample| sample as f32 / max)
                        .map_err(|error| TranscriptionError::Wav(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };

    let mono = downmix_to_mono(&samples, channels);
    Ok(resample_linear(&mono, spec.sample_rate, 16_000))
}

fn read_native_audio_as_whisper_audio(path: &Path) -> Result<Vec<f32>, TranscriptionError> {
    let wav_path = path.with_extension("decode.wav");
    convert_to_pcm_wav(path, &wav_path, 16_000, None).map_err(TranscriptionError::M4a)?;

    let decoded = read_wav_as_whisper_audio(&wav_path);
    let _ = std::fs::remove_file(&wav_path);
    decoded
}

pub fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

pub fn resample_linear(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if samples.is_empty() || source_rate == 0 || source_rate == target_rate {
        return samples.to_vec();
    }

    let output_len =
        ((samples.len() as f64 * target_rate as f64) / source_rate as f64).round() as usize;
    let mut output = Vec::with_capacity(output_len);
    for index in 0..output_len {
        let source_position = index as f64 * source_rate as f64 / target_rate as f64;
        // Clamp both indices: f64 rounding at the final output sample can push
        // `left_index` to `samples.len()`, which would panic on indexing.
        let left_index = (source_position.floor() as usize).min(samples.len() - 1);
        let right_index = (left_index + 1).min(samples.len() - 1);
        let fraction = (source_position - left_index as f64) as f32;
        let sample = samples[left_index] * (1.0 - fraction) + samples[right_index] * fraction;
        output.push(sample);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    type TranscriptionCall = (PathBuf, String, TranscriptionLanguage, Option<String>);

    #[derive(Clone)]
    struct ScriptedTranscriber {
        result: Result<String, TranscriptionError>,
        calls: Arc<Mutex<Vec<TranscriptionCall>>>,
    }

    impl ScriptedTranscriber {
        fn new(result: Result<String, TranscriptionError>) -> Self {
            Self {
                result,
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Transcriber for ScriptedTranscriber {
        fn transcribe(
            &self,
            request: TranscriptionRequest<'_>,
        ) -> Result<String, TranscriptionError> {
            self.calls.lock().expect("calls").push((
                request.audio_path.to_path_buf(),
                request.model_id.to_string(),
                request.language,
                request.dictionary.prompt().map(str::to_string),
            ));
            self.result.clone()
        }
    }

    fn billing_failure() -> TranscriptionError {
        TranscriptionError::Failure(crate::cloud_transcription::TranscriptionFailure {
            provider: Some("OpenAI"),
            http_status: Some(400),
            provider_code: Some("credit_balance_exhausted".into()),
            safe_message: "OpenAI transcription failed with HTTP 400".into(),
            category: crate::cloud_transcription::FailureCategory::BillingLimit,
        })
    }

    fn provider_failure() -> TranscriptionError {
        TranscriptionError::Failure(crate::cloud_transcription::TranscriptionFailure {
            provider: Some("OpenAI"),
            http_status: Some(500),
            provider_code: None,
            safe_message: "OpenAI transcription failed with HTTP 500".into(),
            category: crate::cloud_transcription::FailureCategory::Provider,
        })
    }

    fn write_test_wav(path: &Path, amplitude: f32) {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .expect("test wav");
        for index in 0..16_000 {
            let sample = amplitude * (index as f32 * 0.05).sin();
            let value = (sample * i16::MAX as f32) as i16;
            writer.write_sample(value).expect("test sample");
        }
        writer.finalize().expect("test wav finalized");
    }

    #[test]
    fn silent_audio_execution_reports_no_speech_for_silence_and_none_for_speech() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let silent_path = tmp.path().join("silent.wav");
        write_test_wav(&silent_path, 0.0);
        let audible_path = tmp.path().join("audible.wav");
        write_test_wav(&audible_path, 0.3);

        let silent = silent_audio_execution(&silent_path, "openai-gpt-transcribe")
            .expect("silent audio is skipped");
        assert_eq!(silent.text, "");
        assert_eq!(
            silent.issue.expect("issue").code,
            QueueIssueCode::NoSpeech
        );
        assert_eq!(silent.effective_model_id, "openai-gpt-transcribe");

        assert!(
            silent_audio_execution(&audible_path, "openai-gpt-transcribe").is_none(),
            "audible audio must not be skipped"
        );

        let missing_path = tmp.path().join("missing.wav");
        assert!(
            silent_audio_execution(&missing_path, "openai-gpt-transcribe").is_none(),
            "undecodable audio must defer to the cloud call's own errors"
        );
    }

    #[test]
    fn fallback_transcriber_retries_one_billing_failure_with_identical_context() {
        let primary = ScriptedTranscriber::new(Err(billing_failure()));
        let local = ScriptedTranscriber::new(Ok("WakeNote transcript".into()));
        let fallback = FallbackTranscriber::new(
            primary.clone(),
            local.clone(),
            "whisper-medium",
            false,
            false,
        );
        let directory = tempfile::tempdir().expect("tempdir");
        let audio = directory.path().join("capture.wav");
        std::fs::write(&audio, b"audio").expect("audio");
        let dictionary = DictionaryContext::compile(
            true,
            &[crate::settings::DictionaryEntry {
                id: "wake".into(),
                term: "WakeNote".into(),
                aliases: Vec::new(),
                enabled: true,
            }],
        );

        let execution = fallback
            .transcribe_execution(TranscriptionRequest {
                language_hints: &[],
                audio_path: &audio,
                model_id: "openai-gpt-transcribe",
                language: TranscriptionLanguage::Ko,
                dictionary: &dictionary,
            })
            .expect("fallback success");

        assert_eq!(execution.text, "WakeNote transcript");
        assert_eq!(execution.requested_model_id, "openai-gpt-transcribe");
        assert_eq!(execution.effective_model_id, "whisper-medium");
        assert_eq!(
            execution.fallback_from_model_id.as_deref(),
            Some("openai-gpt-transcribe")
        );
        let primary_calls = primary.calls.lock().expect("primary calls");
        let local_calls = local.calls.lock().expect("local calls");
        assert_eq!(primary_calls.len(), 1);
        assert_eq!(local_calls.len(), 1);
        assert_eq!(primary_calls[0].0, local_calls[0].0);
        assert_eq!(primary_calls[0].2, local_calls[0].2);
        assert_eq!(primary_calls[0].3, local_calls[0].3);
    }

    #[test]
    fn fallback_transcriber_never_switches_on_non_billing_or_incompatible_requests() {
        for (primary_error, diarization_required) in
            [(provider_failure(), false), (billing_failure(), true)]
        {
            let primary = ScriptedTranscriber::new(Err(primary_error.clone()));
            let local = ScriptedTranscriber::new(Ok("must not run".into()));
            let fallback = FallbackTranscriber::new(
                primary,
                local.clone(),
                "whisper-medium",
                false,
                diarization_required,
            );
            let result = fallback.transcribe_execution(TranscriptionRequest {
                language_hints: &[],
                audio_path: Path::new("/tmp/audio.wav"),
                model_id: "openai-gpt-transcribe",
                language: TranscriptionLanguage::Auto,
                dictionary: &DictionaryContext::default(),
            });
            assert_eq!(result, Err(primary_error));
            assert!(local.calls.lock().expect("local calls").is_empty());
        }
    }

    #[test]
    fn fallback_transcriber_reports_primary_and_local_failure_context() {
        let fallback = FallbackTranscriber::new(
            ScriptedTranscriber::new(Err(billing_failure())),
            ScriptedTranscriber::new(Err(TranscriptionError::Engine(
                "local decode failed".into(),
            ))),
            "whisper-medium",
            false,
            false,
        );
        let error = fallback
            .transcribe_execution(TranscriptionRequest {
                language_hints: &[],
                audio_path: Path::new("/tmp/audio.wav"),
                model_id: "openai-gpt-transcribe",
                language: TranscriptionLanguage::Auto,
                dictionary: &DictionaryContext::default(),
            })
            .expect_err("both attempts fail");
        assert!(error.to_string().contains("OpenAI transcription failed"));
        assert!(error.to_string().contains("whisper-medium"));
        assert!(error.to_string().contains("local decode failed"));
    }

    #[test]
    fn reusable_context_cache_loads_the_same_model_once() {
        let mut cache = ReusableContextCache::default();
        let mut load_count = 0;

        let first = cache
            .get_or_try_load(Path::new("/models/medium.bin"), || {
                load_count += 1;
                Ok::<_, ()>("medium")
            })
            .expect("first load");
        let second = cache
            .get_or_try_load(Path::new("/models/medium.bin"), || {
                load_count += 1;
                Ok::<_, ()>("medium")
            })
            .expect("cached load");

        assert_eq!(load_count, 1);
        assert!(std::sync::Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn reusable_context_cache_replaces_a_different_model() {
        let mut cache = ReusableContextCache::default();
        let first = cache
            .get_or_try_load(Path::new("/models/base.bin"), || Ok::<_, ()>("base"))
            .expect("base load");
        let second = cache
            .get_or_try_load(Path::new("/models/medium.bin"), || Ok::<_, ()>("medium"))
            .expect("medium load");

        assert_eq!(*first, "base");
        assert_eq!(*second, "medium");
        assert!(!std::sync::Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn dictation_whisper_profile_keeps_explicit_low_confidence_speech() {
        let qualities = [DecodedSegmentQuality {
            no_speech_probability: 0.12,
            average_token_probability: 0.12,
        }];

        assert_eq!(
            finalize_whisper_transcript("마이크 테스트", &qualities, true),
            ""
        );
        assert_eq!(
            finalize_whisper_transcript("마이크 테스트", &qualities, false),
            "마이크 테스트"
        );
    }

    #[test]
    fn archival_runtime_profile_preserves_low_confidence_decode_text() {
        assert!(
            !RuntimeTranscriber::for_archival("/tmp/wakenote-models")
                .suppress_low_confidence_decode
        );
        assert!(
            !RuntimeTranscriber::for_dictation("/tmp/wakenote-models")
                .suppress_low_confidence_decode
        );
        assert!(RuntimeTranscriber::new("/tmp/wakenote-models").suppress_low_confidence_decode);
    }

    #[test]
    fn empty_auto_dictation_retries_with_whispers_detected_language() {
        assert_eq!(
            dictation_whisper_retry_language(TranscriptionLanguage::Auto, false, "", Some("ko"),),
            Some("ko")
        );
        assert_eq!(
            dictation_whisper_retry_language(
                TranscriptionLanguage::Auto,
                false,
                "already decoded",
                Some("ko"),
            ),
            None
        );
        assert_eq!(
            dictation_whisper_retry_language(TranscriptionLanguage::Auto, true, "", Some("ko"),),
            None
        );
        assert_eq!(
            dictation_whisper_retry_language(TranscriptionLanguage::Ko, false, "", Some("ko"),),
            None
        );
    }

    #[test]
    fn qwen3_asr_language_names_match_the_official_api() {
        assert_eq!(qwen3_asr_language_name(TranscriptionLanguage::Auto), None);
        assert_eq!(
            qwen3_asr_language_name(TranscriptionLanguage::Ko),
            Some("Korean")
        );
        assert_eq!(
            qwen3_asr_language_name(TranscriptionLanguage::En),
            Some("English")
        );
        assert_eq!(
            qwen3_asr_language_name(TranscriptionLanguage::Zh),
            Some("Chinese")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn runtime_transcriber_decodes_m4a_before_routing_qwen_to_the_local_runner() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let store = ModelStore::new(tmp.path());
        let python = store.qwen3_asr_python_path();
        let runner = store.qwen3_asr_runner_path();
        let model = store
            .qwen3_asr_model_dir("qwen3-asr-0.6b")
            .expect("model dir");
        std::fs::create_dir_all(python.parent().expect("runtime bin")).expect("runtime");
        std::fs::create_dir_all(&model).expect("model");
        std::fs::write(
            &python,
            "#!/bin/sh\nprintf '%s|%s|%s|%s' \"$WAKENOTE_QWEN3_ASR_LANGUAGE\" \"$WAKENOTE_QWEN3_ASR_MODEL_PATH\" \"$WAKENOTE_AUDIO_PATH\" \"$WAKENOTE_QWEN3_ASR_PROMPT\"\n",
        )
        .expect("fake python");
        let mut permissions = std::fs::metadata(&python)
            .expect("python metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&python, permissions).expect("executable python");
        std::fs::write(&runner, "runner").expect("runner");
        let source_wav = tmp.path().join("source.wav");
        let mut writer = hound::WavWriter::create(
            &source_wav,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .expect("source wav");
        for _ in 0..16_000 {
            writer.write_sample(8_192_i16).expect("source sample");
        }
        writer.finalize().expect("source wav finalized");
        let audio = tmp.path().join("capture.m4a");
        crate::recorder::encode_wav_to_m4a(&source_wav, &audio, 96).expect("create m4a fixture");

        let dictionary = DictionaryContext::compile(
            true,
            &[crate::settings::DictionaryEntry {
                id: "wake".into(),
                term: "WakeNote".into(),
                aliases: vec!["wake note".into()],
                enabled: true,
            }],
        );
        let text = RuntimeTranscriber::new(tmp.path())
            .transcribe(TranscriptionRequest {
                language_hints: &[],
                audio_path: &audio,
                model_id: "qwen3-asr-0.6b",
                language: TranscriptionLanguage::Ko,
                dictionary: &dictionary,
            })
            .expect("Qwen route");

        let mut fields = text.split('|');
        assert_eq!(fields.next(), Some("Korean"));
        assert_eq!(fields.next(), Some(model.to_string_lossy().as_ref()));
        let runner_audio = PathBuf::from(fields.next().expect("runner audio path"));
        assert_eq!(fields.next(), Some("WakeNote"));
        assert_eq!(
            runner_audio.extension().and_then(|value| value.to_str()),
            Some("wav")
        );
        assert_ne!(runner_audio, audio);
        assert!(
            !runner_audio.exists(),
            "temporary Qwen input must be removed after transcription"
        );
        assert!(audio.exists(), "archival m4a input must be preserved");
    }

    #[cfg(feature = "asr-sherpa")]
    #[test]
    fn sense_voice_language_maps_supported_codes_and_defaults_to_auto() {
        use crate::settings::TranscriptionLanguage;
        assert_eq!(sense_voice_language(TranscriptionLanguage::Ko), "ko");
        assert_eq!(sense_voice_language(TranscriptionLanguage::Zh), "zh");
        assert_eq!(sense_voice_language(TranscriptionLanguage::En), "en");
        assert_eq!(sense_voice_language(TranscriptionLanguage::Ja), "ja");
        assert_eq!(sense_voice_language(TranscriptionLanguage::Auto), "auto");
        // SenseVoice doesn't cover these languages — fall back to auto-detection.
        assert_eq!(sense_voice_language(TranscriptionLanguage::Es), "auto");
        assert_eq!(sense_voice_language(TranscriptionLanguage::Fr), "auto");
    }
}
