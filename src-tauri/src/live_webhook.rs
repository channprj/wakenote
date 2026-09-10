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

pub mod history;
use history::{
    HISTORY_WINDOW_MS, HistoryFilter, HistoryPage, HistoryStatus, HistoryStore, HistoryUpdate,
    NewHistoryEntry, RetryResult, RetrySkipped,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];
// New deliveries and automatic/manual retries share these request slots.
const MAX_REQUESTS: usize = 4;
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
    body: Arc<Vec<u8>>,
    content_type: &'static str,
    payload_format: WebhookPayloadFormat,
    created_at_ms: i64,
    manual_retries: u32,
    attempt_count: u32,
    cycle_attempt: u8,
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
    history: HistoryStore,
    notify: Notify,
    shutdown: CancellationToken,
    instance_id: String,
    policy: Policy,
}

pub struct WebhookService {
    inner: Arc<Inner>,
}

impl WebhookService {
    pub fn new(
        settings: LiveTranscriptionWebhookSettings,
        log_root: PathBuf,
        history_path: PathBuf,
    ) -> Self {
        let service = Self::build(settings, log_root, Policy::default(), history_path);
        tauri::async_runtime::spawn(run(service.inner.clone()));
        service
    }

    #[cfg(test)]
    fn unstarted(
        settings: LiveTranscriptionWebhookSettings,
        log_root: PathBuf,
        policy: Policy,
    ) -> Self {
        let history_path = log_root.join("webhook-history.sqlite3");
        Self::build(settings, log_root, policy, history_path)
    }

    fn build(
        settings: LiveTranscriptionWebhookSettings,
        log_root: PathBuf,
        policy: Policy,
        history_path: PathBuf,
    ) -> Self {
        let history = HistoryStore::open(history_path, settings.auto_delete_history);
        Self {
            inner: Arc::new(Inner {
                history,
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
        if state.settings.enabled != settings.enabled
            || state.settings.endpoint_url != settings.endpoint_url
            || state.settings.payload_format != settings.payload_format
        {
            state.generation.cancel();
            cancel_waiting(&self.inner, &mut state);
            state.generation = CancellationToken::new();
        }
        self.inner.history.configure(settings.auto_delete_history);
        state.settings = settings;
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

    pub async fn list_history(
        &self,
        page: u32,
        filter: HistoryFilter,
    ) -> Result<HistoryPage, String> {
        let settings = self
            .inner
            .state
            .lock()
            .map_err(|_| "Webhook settings are unavailable.".to_string())?
            .settings
            .clone();
        self.inner.history.page(page, filter, settings).await
    }

    pub async fn retry_history(&self, ids: Vec<String>) -> Result<RetryResult, String> {
        if ids.is_empty() || ids.len() > 100 || ids.iter().any(|id| id.is_empty() || id.len() > 160)
        {
            return Err("Select between 1 and 100 recent failed webhook events.".into());
        }
        let settings = self
            .inner
            .state
            .lock()
            .map_err(|_| "Webhook settings are unavailable.".to_string())?
            .settings
            .clone();
        let claimed = self.inner.history.claim(ids, settings).await?;
        let mut result = RetryResult {
            queued_ids: Vec::new(),
            skipped: claimed.skipped,
        };
        let Ok(mut state) = self.inner.state.lock() else {
            for record in &claimed.records {
                self.inner.history.release(record);
            }
            return Err("Webhook settings are unavailable.".into());
        };
        for record in claimed.records {
            let now = chrono::Utc::now().timestamp_millis();
            let blocked = if self.inner.shutdown.is_cancelled() {
                Some("shutting_down")
            } else if !state.settings.enabled {
                Some("disabled")
            } else if state.settings.endpoint_url != record.endpoint_url
                || state.settings.payload_format != record.payload_format
            {
                Some("settings_changed")
            } else if record.created_at_ms <= now - HISTORY_WINDOW_MS || record.created_at_ms > now
            {
                Some("expired")
            } else if state.stats.pending >= self.inner.policy.max_events {
                Some("queue_full")
            } else {
                None
            };
            if let Some(reason) = blocked {
                self.inner.history.release(&record);
                result.skipped.push(RetrySkipped {
                    event_id: record.event_id,
                    reason: reason.into(),
                });
                continue;
            }
            let sequence = state.next_sequence;
            state.next_sequence += 1;
            result.queued_ids.push(record.event_id.clone());
            let event = Event {
                sequence,
                id: record.event_id,
                endpoint: record.endpoint_url,
                body: record.body,
                content_type: content_type(record.payload_format),
                payload_format: record.payload_format,
                created_at_ms: record.created_at_ms,
                manual_retries: record.manual_retries,
                attempt_count: record.attempt_count,
                cycle_attempt: 0,
                attempt: 1,
                ready_at: Instant::now(),
                cancel: state.generation.clone(),
            };
            state.queue.insert(sequence, event);
            state.stats.pending += 1;
        }
        drop(state);
        self.inner.notify.notify_one();
        Ok(result)
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        if let Ok(mut state) = self.inner.state.lock() {
            state.generation.cancel();
            cancel_waiting(&self.inner, &mut state);
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
            body: Arc::new(body),
            content_type,
            payload_format: state.settings.payload_format,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
            manual_retries: 0,
            attempt_count: 0,
            cycle_attempt: 0,
            endpoint: state.settings.endpoint_url.clone(),
            attempt: 1,
            ready_at: Instant::now(),
            cancel: state.generation.clone(),
        };
        let full = state.stats.pending >= self.inner.policy.max_events;
        self.inner.history.insert(NewHistoryEntry {
            event_id: event.id.clone(),
            created_at_ms: event.created_at_ms,
            endpoint_url: event.endpoint.clone(),
            payload_format: event.payload_format,
            body: event.body.clone(),
            status: if full {
                HistoryStatus::Dropped
            } else {
                HistoryStatus::Queued
            },
        });
        if full {
            state.stats.dropped += 1;
            record(&state, &event.id, 0, "queue_full", None, 0);
            return;
        }
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

fn cancel_waiting(inner: &Inner, state: &mut State) {
    let waiting = std::mem::take(&mut state.queue);
    for event in waiting.into_values() {
        state.stats.pending -= 1;
        state.stats.cancelled += 1;
        remember(inner, &event, HistoryStatus::Cancelled, None, None, None);
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
    inner: Arc<Inner>,
    client: reqwest::Client,
    mut event: Event,
    limit: Duration,
) -> (Event, AttemptResult, Duration) {
    if event.cancel.is_cancelled() {
        return (event, AttemptResult::Cancelled, Duration::ZERO);
    }
    event.attempt_count += 1;
    event.cycle_attempt = event.attempt;
    remember(&inner, &event, HistoryStatus::Sending, None, None, None);
    let started = Instant::now();
    let result = tokio::select! {
        biased;
        _ = event.cancel.cancelled() => AttemptResult::Cancelled,
        result = timeout(limit, client.post(&event.endpoint)
            .header(reqwest::header::CONTENT_TYPE, event.content_type)
            .header("X-Transcription-Event-Id", &event.id)
            .header("X-Transcription-Attempt", event.attempt.to_string())
            .body(event.body.as_ref().clone()).send()) => {
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
        remember(
            inner,
            &event,
            HistoryStatus::Cancelled,
            status,
            None,
            Some(elapsed),
        );
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
        remember(
            inner,
            &event,
            HistoryStatus::Succeeded,
            status,
            None,
            Some(elapsed),
        );
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
            remember(
                inner,
                &event,
                HistoryStatus::Retrying,
                status,
                Some(error),
                Some(elapsed),
            );
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
            remember(
                inner,
                &event,
                HistoryStatus::Failed,
                status,
                Some(error),
                Some(elapsed),
            );
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
                cancel_waiting(&inner, &mut state);
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
                active.push(send(
                    inner.clone(),
                    client.clone(),
                    event,
                    inner.policy.timeout,
                ));
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

fn content_type(format: WebhookPayloadFormat) -> &'static str {
    match format {
        WebhookPayloadFormat::TextOnly => "text/plain; charset=utf-8",
        WebhookPayloadFormat::Json => "application/json; charset=utf-8",
    }
}

fn remember(
    inner: &Inner,
    event: &Event,
    status: HistoryStatus,
    http_status: Option<u16>,
    error_kind: Option<&str>,
    elapsed: Option<Duration>,
) {
    inner.history.update(HistoryUpdate {
        event_id: event.id.clone(),
        manual_retries: event.manual_retries,
        status,
        attempt_count: event.attempt_count,
        cycle_attempt: event.cycle_attempt,
        http_status,
        error_kind: error_kind.map(str::to_string),
        elapsed_ms: elapsed.map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64),
    });
}

#[cfg(test)]
mod tests;
