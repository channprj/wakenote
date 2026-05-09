use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::settings::expand_user_path;
use crate::transcription::resample_linear;

/// Whisper requires roughly 1 second of audio for a meaningful pass; below
/// this we skip the partial decode entirely so the live preview never spits
/// out hallucinated content from sub-second slivers.
const MIN_LIVE_DECODE_SAMPLES_16K: usize = 16_000;

/// Cap each partial decode at this many seconds of audio. Long chunks would
/// otherwise force the model to re-decode minutes of audio every interval,
/// destroying responsiveness. The final accurate pass on the full chunk
/// still runs in the regular queue worker after the chunk closes.
const PARTIAL_WINDOW_SECONDS: u64 = 20;

#[derive(Debug, Clone)]
pub struct LivePartialRequest {
    pub chunk_id: u64,
    pub model_id: String,
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
    cancelled_chunks: Vec<u64>,
    model_directory: PathBuf,
    loaded_model: Option<LoadedModel>,
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
                cancelled_chunks: Vec::new(),
                model_directory,
                loaded_model: None,
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

    /// Mark a chunk as cancelled; any in-flight or queued work for that
    /// chunk_id will be discarded by the worker before emitting.
    pub fn cancel_chunk(&self, chunk_id: u64) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        state.cancelled_chunks.push(chunk_id);
        if state.pending.as_ref().map(|p| p.chunk_id) == Some(chunk_id) {
            state.pending = None;
        }
        // Trim history; we only need recent ids.
        if state.cancelled_chunks.len() > 64 {
            let drop = state.cancelled_chunks.len() - 64;
            state.cancelled_chunks.drain(..drop);
        }
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
    eprintln!("[sagwan] live_transcription worker thread started");
    // Track (chunk_id, model_id) of the most recent ModelMissing emission so
    // we only surface the failure once per chunk — partials fire every few
    // seconds and the UI doesn't need a steady stream of identical errors.
    let mut last_missing: Option<(u64, String)> = None;
    loop {
        let request = match wait_for_request(&inner) {
            Some(request) => request,
            None => {
                eprintln!("[sagwan] live_transcription worker exiting (closed)");
                return;
            }
        };

        if is_cancelled(&inner, request.chunk_id) {
            eprintln!(
                "[sagwan] live_transcription: dropping cancelled chunk_id={}",
                request.chunk_id
            );
            mark_idle(&inner);
            continue;
        }

        let context = match ensure_context(&inner, &request.model_id) {
            Some(context) => {
                last_missing = None;
                context
            }
            None => {
                eprintln!(
                    "[sagwan] live_transcription: model not loadable for id={}",
                    request.model_id
                );
                let already_warned = last_missing
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
        let result = run_whisper_partial(&context, &request.samples, request.sample_rate);
        let elapsed = started.elapsed();
        eprintln!(
            "[sagwan] live_transcription: chunk_id={} decode took {:?}",
            request.chunk_id, elapsed
        );
        mark_idle(&inner);

        if is_cancelled(&inner, request.chunk_id) {
            continue;
        }

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

fn wait_for_request(inner: &Arc<LiveTranscriptionInner>) -> Option<LivePartialRequest> {
    let mut state = inner.state.lock().ok()?;
    loop {
        if state.closed {
            return None;
        }
        if let Some(request) = state.pending.take() {
            state.in_flight_chunk_id = Some(request.chunk_id);
            return Some(request);
        }
        state = inner.cond.wait(state).ok()?;
    }
}

fn mark_idle(inner: &Arc<LiveTranscriptionInner>) {
    if let Ok(mut state) = inner.state.lock() {
        state.in_flight_chunk_id = None;
    }
}

fn is_cancelled(inner: &Arc<LiveTranscriptionInner>, chunk_id: u64) -> bool {
    inner
        .state
        .lock()
        .map(|state| state.cancelled_chunks.contains(&chunk_id))
        .unwrap_or(false)
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
            "[sagwan] live_transcription: model file missing at {}",
            model_path.display()
        );
        return None;
    }

    eprintln!(
        "[sagwan] live_transcription: loading model {} (this can take 5-15s)",
        model_path.display()
    );
    let context = match WhisperContext::new_with_params(
        model_path.to_string_lossy().as_ref(),
        WhisperContextParameters::default(),
    ) {
        Ok(ctx) => ctx,
        Err(error) => {
            eprintln!("[sagwan] live_transcription: WhisperContext load error: {error}");
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
    state
        .full(params, &resampled)
        .map_err(|error| format!("decode: {error}"))?;

    let text = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}
