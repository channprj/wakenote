use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::settings::{expand_user_path, TranscriptionLanguage};
use crate::transcription::{
    configure_whisper_language, decoded_segment_quality, resample_linear,
    should_suppress_low_confidence_decode, should_suppress_transcript_artifact,
};

/// Whisper requires roughly 1 second of audio for a meaningful pass; below
/// this we skip the partial decode entirely so the live preview never spits
/// out hallucinated content from sub-second slivers.
const MIN_LIVE_DECODE_SAMPLES_16K: usize = 16_000;

/// Cap each partial decode at this many seconds of audio. Long chunks would
/// otherwise force the model to re-decode minutes of audio every interval,
/// destroying responsiveness. The final accurate pass on the full chunk
/// still runs in the regular queue worker after the chunk closes. 10 s gives
/// whisper plenty of context for the trailing utterance while keeping each
/// partial pass fast enough for sub-second perceived latency on long chunks.
const PARTIAL_WINDOW_SECONDS: u64 = 10;

#[derive(Debug, Clone)]
pub struct LivePartialRequest {
    pub chunk_id: u64,
    pub model_id: String,
    pub language: TranscriptionLanguage,
    pub suppress_low_confidence_transcripts: bool,
    pub samples: Arc<Vec<f32>>,
    pub sample_rate: u32,
}

#[derive(Debug, Clone)]
pub struct LivePartialResult {
    pub chunk_id: u64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum LivePartialEvent {
    Text(LivePartialResult),
    ModelMissing { chunk_id: u64, model_id: String },
    EngineError { chunk_id: u64, message: String },
}

pub type LivePartialCallback = Arc<dyn Fn(LivePartialEvent) + Send + Sync>;

pub struct LiveTranscriptionService {
    inner: Arc<LiveTranscriptionInner>,
    join: Option<JoinHandle<()>>,
}

struct LiveTranscriptionInner {
    state: Mutex<LiveTranscriptionState>,
    cond: Condvar,
}

struct LiveTranscriptionState {
    pending: Option<LivePartialRequest>,
    in_flight_chunk_id: Option<u64>,
    closed: bool,
    model_directory: PathBuf,
    loaded_model: Option<LoadedModel>,
    /// Model id the worker should warm up when otherwise idle. Set by
    /// [`LiveTranscriptionService::preload`] so the (5-15 s) whisper load
    /// happens at capture start instead of on the first utterance — without
    /// it, the first several seconds of speech produce no live captions
    /// because the decode can't begin until the model finishes loading.
    preload_model_id: Option<String>,
}

/// One unit of work for the worker thread: either decode a partial, or warm
/// up a model so the first real decode is instant.
enum WorkItem {
    Partial(LivePartialRequest),
    Preload(String),
}

struct LoadedModel {
    model_id: String,
    context: Arc<WhisperContext>,
}

impl LiveTranscriptionService {
    pub fn new(model_directory: impl AsRef<Path>, on_result: LivePartialCallback) -> Self {
        let model_directory = expand_user_path(model_directory.as_ref().to_string_lossy());
        let inner = Arc::new(LiveTranscriptionInner {
            state: Mutex::new(LiveTranscriptionState {
                pending: None,
                in_flight_chunk_id: None,
                closed: false,
                model_directory,
                loaded_model: None,
                preload_model_id: None,
            }),
            cond: Condvar::new(),
        });
        let worker_inner = Arc::clone(&inner);
        let join = thread::spawn(move || worker_loop(worker_inner, on_result));
        Self {
            inner,
            join: Some(join),
        }
    }

    /// Ask the worker to load `model_id` now, before any audio arrives, so
    /// the first utterance decodes immediately instead of waiting out the
    /// 5-15 s model load. No-op if that model is already loaded or a decode
    /// is already pending (the real request will load it anyway).
    pub fn preload(&self, model_id: impl Into<String>) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        let model_id = model_id.into();
        let already_loaded = state
            .loaded_model
            .as_ref()
            .is_some_and(|loaded| loaded.model_id == model_id);
        if already_loaded || state.pending.is_some() {
            return;
        }
        state.preload_model_id = Some(model_id);
        self.inner.cond.notify_one();
    }

    /// Submit the latest partial request for the active chunk. Replaces any
    /// previous pending request — single-flight semantics keep the model
    /// from queueing up stale work behind a slow medium-model decode.
    pub fn submit(&self, request: LivePartialRequest) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        state.pending = Some(request);
        self.inner.cond.notify_one();
    }

    pub fn update_model_directory(&self, dir: impl AsRef<Path>) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        let new_dir = expand_user_path(dir.as_ref().to_string_lossy());
        if state.model_directory != new_dir {
            state.model_directory = new_dir;
            state.loaded_model = None;
        }
    }
}

impl Drop for LiveTranscriptionService {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.closed = true;
            state.pending = None;
        }
        self.inner.cond.notify_all();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn worker_loop(inner: Arc<LiveTranscriptionInner>, on_result: LivePartialCallback) {
    eprintln!("[wakenote] live_transcription worker thread started");
    // Track (chunk_id, model_id) of the most recent ModelMissing emission so
    // we only surface the failure once per chunk — partials fire every few
    // seconds and the UI doesn't need a steady stream of identical errors.
    let mut last_missing: Option<(u64, String)> = None;
    loop {
        let request = match wait_for_request(&inner) {
            Some(WorkItem::Partial(request)) => request,
            Some(WorkItem::Preload(model_id)) => {
                eprintln!("[wakenote] live_transcription: preloading model {model_id}");
                // Warm the model so the first real decode is instant. Ignore
                // the result — a real request (or ModelMissing handling) will
                // surface any load failure to the UI.
                let _ = ensure_context(&inner, &model_id);
                continue;
            }
            None => {
                eprintln!("[wakenote] live_transcription worker exiting (closed)");
                return;
            }
        };

        let context = match ensure_context(&inner, &request.model_id) {
            Some(context) => {
                last_missing = None;
                context
            }
            None => {
                eprintln!(
                    "[wakenote] live_transcription: model not loadable for id={}",
                    request.model_id
                );
                let already_warned =
                    last_missing
                        .as_ref()
                        .is_some_and(|(prev_chunk, prev_model)| {
                            *prev_chunk == request.chunk_id && prev_model == &request.model_id
                        });
                if !already_warned {
                    last_missing = Some((request.chunk_id, request.model_id.clone()));
                    on_result(LivePartialEvent::ModelMissing {
                        chunk_id: request.chunk_id,
                        model_id: request.model_id.clone(),
                    });
                }
                mark_idle(&inner);
                continue;
            }
        };

        let started = std::time::Instant::now();
        let result = run_whisper_partial(
            &context,
            &request.samples,
            request.sample_rate,
            request.language,
            request.suppress_low_confidence_transcripts,
        );
        let elapsed = started.elapsed();
        eprintln!(
            "[wakenote] live_transcription: chunk_id={} decode took {:?}",
            request.chunk_id, elapsed
        );
        mark_idle(&inner);

        match result {
            Ok(Some(text)) => {
                on_result(LivePartialEvent::Text(LivePartialResult {
                    chunk_id: request.chunk_id,
                    text,
                }));
            }
            Ok(None) => {}
            Err(message) => {
                on_result(LivePartialEvent::EngineError {
                    chunk_id: request.chunk_id,
                    message,
                });
            }
        }
    }
}

fn wait_for_request(inner: &Arc<LiveTranscriptionInner>) -> Option<WorkItem> {
    let mut state = inner.state.lock().ok()?;
    loop {
        if state.closed {
            return None;
        }
        // A real decode always wins over a preload — if audio is already
        // waiting, loading the model for it covers the warm-up anyway.
        if let Some(request) = state.pending.take() {
            state.in_flight_chunk_id = Some(request.chunk_id);
            return Some(WorkItem::Partial(request));
        }
        if let Some(model_id) = state.preload_model_id.take() {
            return Some(WorkItem::Preload(model_id));
        }
        state = inner.cond.wait(state).ok()?;
    }
}

fn mark_idle(inner: &Arc<LiveTranscriptionInner>) {
    if let Ok(mut state) = inner.state.lock() {
        state.in_flight_chunk_id = None;
    }
}

fn ensure_context(
    inner: &Arc<LiveTranscriptionInner>,
    model_id: &str,
) -> Option<Arc<WhisperContext>> {
    let (model_path, already_loaded) = {
        let state = inner.state.lock().ok()?;
        let already = state
            .loaded_model
            .as_ref()
            .filter(|loaded| loaded.model_id == model_id)
            .map(|loaded| loaded.context.clone());
        (
            state.model_directory.join(format!("{model_id}.bin")),
            already,
        )
    };
    if let Some(context) = already_loaded {
        return Some(context);
    }
    if !model_path.exists() {
        eprintln!(
            "[wakenote] live_transcription: model file missing at {}",
            model_path.display()
        );
        return None;
    }

    eprintln!(
        "[wakenote] live_transcription: loading model {} (this can take 5-15s)",
        model_path.display()
    );
    let context = match WhisperContext::new_with_params(
        model_path.to_string_lossy().as_ref(),
        WhisperContextParameters::default(),
    ) {
        Ok(ctx) => ctx,
        Err(error) => {
            eprintln!("[wakenote] live_transcription: WhisperContext load error: {error}");
            return None;
        }
    };
    let context = Arc::new(context);

    if let Ok(mut state) = inner.state.lock() {
        state.loaded_model = Some(LoadedModel {
            model_id: model_id.to_string(),
            context: context.clone(),
        });
    }
    Some(context)
}

fn run_whisper_partial(
    context: &WhisperContext,
    samples: &[f32],
    source_rate: u32,
    language: TranscriptionLanguage,
    suppress_low_confidence_transcripts: bool,
) -> Result<Option<String>, String> {
    // Take only the trailing window. Re-decoding minutes of audio every
    // partial cycle would never keep up; the queue worker still gets the
    // full chunk for the final accurate pass.
    let window_samples_at_source =
        (source_rate as u64).saturating_mul(PARTIAL_WINDOW_SECONDS) as usize;
    let trimmed: &[f32] = if samples.len() > window_samples_at_source {
        &samples[samples.len() - window_samples_at_source..]
    } else {
        samples
    };

    let resampled = if source_rate == 16_000 {
        trimmed.to_vec()
    } else {
        resample_linear(trimmed, source_rate, 16_000)
    };
    if resampled.len() < MIN_LIVE_DECODE_SAMPLES_16K {
        return Ok(None);
    }

    let mut state = context
        .create_state()
        .map_err(|error| format!("create_state: {error}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    params.set_single_segment(true);
    configure_whisper_language(&mut params, language);
    state
        .full(params, &resampled)
        .map_err(|error| format!("decode: {error}"))?;

    let mut text = String::new();
    let mut qualities = Vec::new();
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
        qualities.push(decoded_segment_quality(&segment));
    }
    let text = text.trim().to_string();
    if text.is_empty()
        || (suppress_low_confidence_transcripts
            && (should_suppress_transcript_artifact(&text)
                || should_suppress_low_confidence_decode(&text, &qualities)))
    {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}
