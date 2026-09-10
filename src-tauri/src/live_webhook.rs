//! Best-effort delivery of newly finalized live chunks. No network or retry
//! wait occurs on the transcription thread, and no payload is read from disk.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{StreamExt, stream::FuturesUnordered};
use serde::Serialize;
use tokio::sync::Notify;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

use crate::debug_log::append_debug_log_nonblocking;
use crate::recorder::ChunkMetadata;
use crate::settings::{LiveTranscriptionWebhookSettings, WebhookPayloadFormat};
use crate::transcription::TranscriptionExecution;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
const MAX_REQUESTS: usize = 3;
const MAX_EVENTS: usize = 1_000;

/// One handle per captured chunk, consumed only by its first live decode.
/// Keeping the one-shot flag with the result avoids an ever-growing ID cache.
pub struct LiveWebhookTranscript {
    metadata: Arc<ChunkMetadata>,
    finalized: AtomicBool,
}

impl LiveWebhookTranscript {
    pub fn new(metadata: Arc<ChunkMetadata>) -> Self {
        Self {
            metadata,
            finalized: AtomicBool::new(false),
        }
    }

    pub fn finalize(&self, service: &WebhookService, execution: &TranscriptionExecution) {
        // Mark even disabled/empty/dropped events as seen: enabling or retrying
        // a duplicate callback must never retroactively send old speech.
        if self.finalized.swap(true, Ordering::AcqRel) || execution.text.trim().is_empty() {
            return;
        }
        let Some(chunk_id) = self.metadata.live_capture_chunk_id else {
            return;
        };
        let mut metadata = (*self.metadata).clone();
        metadata.complete_transcription(
            &execution.text,
            &execution.requested_model_id,
            &execution.effective_model_id,
            execution.fallback_from_model_id.as_deref(),
            chrono::Utc::now(),
        );
        service.enqueue(chunk_id, &metadata);
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WebhookStats {
    pub succeeded: u64,
    pub retries: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub dropped: u64,
    pub pending: usize,
    pub in_flight: usize,
}

#[derive(Clone, Copy)]
struct Policy {
    timeout: Duration,
    retry_delays: [Duration; 3],
    max_requests: usize,
    max_events: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            timeout: REQUEST_TIMEOUT,
            retry_delays: RETRY_DELAYS,
            max_requests: MAX_REQUESTS,
            max_events: MAX_EVENTS,
        }
    }
}

struct Event {
    sequence: u64,
    id: String,
    endpoint: String,
    body: Vec<u8>,
    content_type: &'static str,
    attempt: u8,
    ready_at: Instant,
    cancel: CancellationToken,
}

struct State {
    settings: LiveTranscriptionWebhookSettings,
    generation: CancellationToken,
    queue: BTreeMap<u64, Event>,
    next_sequence: u64,
    stats: WebhookStats,
    log_root: PathBuf,
}

struct Inner {
    state: Mutex<State>,
    notify: Notify,
    shutdown: CancellationToken,
    instance_id: String,
    policy: Policy,
}

pub struct WebhookService {
    inner: Arc<Inner>,
}

impl WebhookService {
    pub fn new(settings: LiveTranscriptionWebhookSettings, log_root: PathBuf) -> Self {
        let service = Self::unstarted(settings, log_root, Policy::default());
        tauri::async_runtime::spawn(run(service.inner.clone()));
        service
    }

    fn unstarted(
        settings: LiveTranscriptionWebhookSettings,
        log_root: PathBuf,
        policy: Policy,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    settings,
                    generation: CancellationToken::new(),
                    queue: BTreeMap::new(),
                    next_sequence: 0,
                    stats: WebhookStats::default(),
                    log_root,
                }),
                notify: Notify::new(),
                shutdown: CancellationToken::new(),
                policy,
                instance_id: format!(
                    "{}-{}",
                    chrono::Utc::now().timestamp_micros(),
                    std::process::id()
                ),
            }),
        }
    }

    /// Called under the settings commit lock, after successful persistence.
    pub fn configure(&self, settings: LiveTranscriptionWebhookSettings, log_root: PathBuf) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        if state.settings != settings {
            state.generation.cancel();
            cancel_waiting(&mut state);
            state.generation = CancellationToken::new();
            state.settings = settings;
        }
        state.log_root = log_root;
        drop(state);
        self.inner.notify.notify_one();
    }

    pub fn stats(&self) -> WebhookStats {
        self.inner
            .state
            .lock()
            .map(|s| s.stats.clone())
            .unwrap_or_default()
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        if let Ok(mut state) = self.inner.state.lock() {
            state.generation.cancel();
            cancel_waiting(&mut state);
        }
        self.inner.notify.notify_one();
    }

    fn enqueue(&self, chunk_id: u64, metadata: &ChunkMetadata) {
        let Ok(mut state) = self.inner.state.lock() else {
            return;
        };
        if self.inner.shutdown.is_cancelled()
            || !state.settings.enabled
            || state.settings.validate().is_err()
        {
            return;
        }
        let id = format!("{}-{chunk_id}", self.inner.instance_id);
        if state.stats.pending >= self.inner.policy.max_events {
            state.stats.dropped += 1;
            record(&state, &id, 0, "queue_full", None, 0);
            return;
        }
        let (body, content_type) = match state.settings.payload_format {
            WebhookPayloadFormat::TextOnly => (
                metadata
                    .transcript_text
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes()
                    .to_vec(),
                "text/plain; charset=utf-8",
            ),
            WebhookPayloadFormat::Json => match metadata.json_bytes() {
                Ok(body) => (body, "application/json; charset=utf-8"),
                Err(_) => {
                    state.stats.failed += 1;
                    record(&state, &id, 0, "serialization", None, 0);
                    return;
                }
            },
        };
        let sequence = state.next_sequence;
        state.next_sequence += 1;
        let event = Event {
            sequence,
            id,
            body,
            content_type,
            endpoint: state.settings.endpoint_url.clone(),
            attempt: 1,
            ready_at: Instant::now(),
            cancel: state.generation.clone(),
        };
        state.queue.insert(sequence, event);
        state.stats.pending += 1;
        drop(state);
        self.inner.notify.notify_one();
    }
}

impl Drop for WebhookService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn cancel_waiting(state: &mut State) {
    let waiting = std::mem::take(&mut state.queue);
    for event in waiting.into_values() {
        state.stats.pending -= 1;
        state.stats.cancelled += 1;
        record(state, &event.id, event.attempt, "cancelled", None, 0);
    }
}

fn record(
    state: &State,
    id: &str,
    attempt: u8,
    outcome: &str,
    status: Option<u16>,
    elapsed_ms: u128,
) {
    // Never format reqwest::Error, URLs, request/response bodies, or metadata.
    append_debug_log_nonblocking(
        &state.log_root,
        format!(
            "[live-webhook] event_id={id} attempt={attempt} outcome={outcome} status={status:?} elapsed_ms={elapsed_ms} succeeded={} retries={} failed={} cancelled={} dropped={} pending={} in_flight={}",
            state.stats.succeeded,
            state.stats.retries,
            state.stats.failed,
            state.stats.cancelled,
            state.stats.dropped,
            state.stats.pending,
            state.stats.in_flight,
        ),
    );
}

enum AttemptResult {
    Http(u16),
    Error(&'static str),
    Cancelled,
}

async fn send(
    client: reqwest::Client,
    event: Event,
    limit: Duration,
) -> (Event, AttemptResult, Duration) {
    let started = Instant::now();
    let result = tokio::select! {
        biased;
        _ = event.cancel.cancelled() => AttemptResult::Cancelled,
        result = timeout(limit, client.post(&event.endpoint)
            .header(reqwest::header::CONTENT_TYPE, event.content_type)
            .header("X-Transcription-Event-Id", &event.id)
            .header("X-Transcription-Attempt", event.attempt.to_string())
            .body(event.body.clone()).send()) => {
            match result {
                // send resolves at response headers. Drop the response without
                // reading the body, including a body that never finishes.
                Ok(Ok(response)) => AttemptResult::Http(response.status().as_u16()),
                Ok(Err(error)) => AttemptResult::Error(if error.is_timeout() { "timeout" }
                    else if error.is_connect() { "connect" } else { "transport" }),
                Err(_) => AttemptResult::Error("timeout"),
            }
        }
    };
    (event, result, started.elapsed())
}

fn complete(inner: &Inner, mut event: Event, result: AttemptResult, elapsed: Duration) {
    let Ok(mut state) = inner.state.lock() else {
        return;
    };
    state.stats.in_flight -= 1;
    let status = match result {
        AttemptResult::Http(status) => Some(status),
        _ => None,
    };
    if event.cancel.is_cancelled() || matches!(result, AttemptResult::Cancelled) {
        state.stats.cancelled += 1;
        state.stats.pending -= 1;
        record(
            &state,
            &event.id,
            event.attempt,
            "cancelled",
            status,
            elapsed.as_millis(),
        );
    } else if status.is_some_and(|status| (200..300).contains(&status)) {
        state.stats.succeeded += 1;
        state.stats.pending -= 1;
        record(
            &state,
            &event.id,
            event.attempt,
            "succeeded",
            status,
            elapsed.as_millis(),
        );
    } else {
        let error = match result {
            AttemptResult::Error(error) => error,
            _ => "http",
        };
        if let Some(delay) = inner
            .policy
            .retry_delays
            .get(usize::from(event.attempt - 1))
        {
            state.stats.retries += 1;
            record(
                &state,
                &event.id,
                event.attempt,
                &format!("retry_{error}"),
                status,
                elapsed.as_millis(),
            );
            event.attempt += 1;
            event.ready_at = Instant::now() + *delay;
            state.queue.insert(event.sequence, event);
        } else {
            state.stats.failed += 1;
            state.stats.pending -= 1;
            record(
                &state,
                &event.id,
                event.attempt,
                &format!("failed_{error}"),
                status,
                elapsed.as_millis(),
            );
        }
    }
}

async fn run(inner: Arc<Inner>) {
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(inner.policy.timeout)
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            // TLS/resolver initialization failure must not affect capture.
            inner.shutdown.cancel();
            if let Ok(mut state) = inner.state.lock() {
                cancel_waiting(&mut state);
                record(&state, "none", 0, "client_initialization", None, 0);
            }
            return;
        }
    };
    let mut active = FuturesUnordered::new();
    loop {
        let deadline = {
            let Ok(mut state) = inner.state.lock() else {
                return;
            };
            while !inner.shutdown.is_cancelled() && active.len() < inner.policy.max_requests {
                let ready = state
                    .queue
                    .iter()
                    .find(|(_, event)| event.ready_at <= Instant::now())
                    .map(|(id, _)| *id);
                let Some(event) = ready.and_then(|id| state.queue.remove(&id)) else {
                    break;
                };
                state.stats.in_flight += 1;
                active.push(send(client.clone(), event, inner.policy.timeout));
            }
            if active.len() < inner.policy.max_requests {
                state.queue.values().map(|event| event.ready_at).min()
            } else {
                None
            }
        };
        if inner.shutdown.is_cancelled() {
            // Generation cancellation drops each request future immediately;
            // no delivery flush or network wait during shutdown.
            while let Some((event, result, elapsed)) = active.next().await {
                complete(&inner, event, result, elapsed);
            }
            return;
        }
        tokio::select! {
            biased;
            _ = inner.shutdown.cancelled() => {},
            Some((event, result, elapsed)) = active.next(), if !active.is_empty() => {
                complete(&inner, event, result, elapsed);
            }
            _ = inner.notify.notified() => {},
            _ = async {
                match deadline {
                    Some(deadline) => sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => {},
        }
    }
}

#[cfg(test)]
mod tests;
