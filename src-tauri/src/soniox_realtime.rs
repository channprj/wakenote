use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use crate::cloud_realtime::{
    RealtimePartial, RealtimePartialCallback, RealtimeSamplesRequest, RealtimeStoredResult,
    realtime_result_store,
};
use crate::cloud_transcription::{FailureCategory, TranscriptionFailure};
use crate::transcription::{TranscriptionExecution, TranscriptionUsage};

const PROVIDER: &str = "Soniox";
const SONIOX_REALTIME_URL: &str = "wss://stt-rt.soniox.com/transcribe-websocket";
const SONIOX_REALTIME_MODEL: &str = "stt-rt-v5";
const SONIOX_REALTIME_MODEL_ID: &str = "soniox-realtime-v5";
const COMMIT_COMPLETION_WAIT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct SonioxRealtimeManager {
    tx: mpsc::UnboundedSender<ManagerCommand>,
}

impl SonioxRealtimeManager {
    pub fn new(on_partial: RealtimePartialCallback) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(manager_loop(rx, on_partial));
        Self { tx }
    }

    pub fn submit_samples(&self, request: RealtimeSamplesRequest) {
        let _ = self.tx.send(ManagerCommand::Samples(Box::new(request)));
    }

    pub fn commit(&self, source_key: String, chunk_id: u64, audio_path: PathBuf) {
        let _ = self.tx.send(ManagerCommand::Commit {
            source_key,
            chunk_id,
            audio_path,
        });
    }

    pub fn close_source(&self, source_key: String) {
        let _ = self.tx.send(ManagerCommand::Close(source_key));
    }
}

enum ManagerCommand {
    Samples(Box<RealtimeSamplesRequest>),
    Commit {
        source_key: String,
        chunk_id: u64,
        audio_path: PathBuf,
    },
    Close(String),
}

enum ConnectionCommand {
    Append(Vec<u8>),
    Commit {
        audio_path: PathBuf,
        audio_duration_ms: u64,
    },
    Close,
}

struct ActiveSession {
    chunk_id: u64,
    model_id: String,
    source_rate: u32,
    sent_source_samples: usize,
    tx: mpsc::UnboundedSender<ConnectionCommand>,
    failure: Arc<Mutex<Option<TranscriptionFailure>>>,
}

async fn manager_loop(
    mut rx: mpsc::UnboundedReceiver<ManagerCommand>,
    on_partial: RealtimePartialCallback,
) {
    let mut sessions: HashMap<String, ActiveSession> = HashMap::new();
    while let Some(command) = rx.recv().await {
        match command {
            ManagerCommand::Samples(request) => {
                let request = *request;
                if request.model_id != SONIOX_REALTIME_MODEL_ID {
                    continue;
                }
                let replace = sessions.get(&request.source_key).is_none_or(|session| {
                    session.chunk_id != request.chunk_id
                        || session.model_id != request.model_id
                        || session.source_rate != request.sample_rate
                        || request.samples.len() < session.sent_source_samples
                });
                if replace {
                    if let Some(previous) = sessions.remove(&request.source_key) {
                        let _ = previous.tx.send(ConnectionCommand::Close);
                    }
                    let failure = Arc::new(Mutex::new(None));
                    let (tx, command_rx) = mpsc::unbounded_channel();
                    tauri::async_runtime::spawn(connection_loop(
                        request.clone(),
                        command_rx,
                        on_partial.clone(),
                        failure.clone(),
                    ));
                    sessions.insert(
                        request.source_key.clone(),
                        ActiveSession {
                            chunk_id: request.chunk_id,
                            model_id: request.model_id.clone(),
                            source_rate: request.sample_rate,
                            sent_source_samples: 0,
                            tx,
                            failure,
                        },
                    );
                }
                if let Some(session) = sessions.get_mut(&request.source_key) {
                    let (unseen, sent_source_samples) =
                        unseen_samples(&request.samples, session.sent_source_samples);
                    session.sent_source_samples = sent_source_samples;
                    if !unseen.is_empty() {
                        let _ = session
                            .tx
                            .send(ConnectionCommand::Append(encode_pcm_s16le(unseen)));
                    }
                }
            }
            ManagerCommand::Commit {
                source_key,
                chunk_id,
                audio_path,
            } => {
                let Some(session) = sessions.remove(&source_key) else {
                    realtime_result_store().publish(
                        audio_path,
                        RealtimeStoredResult::Failed(local_failure(
                            FailureCategory::Transport,
                            "Soniox real-time transcription session is unavailable",
                        )),
                    );
                    continue;
                };
                if session.chunk_id != chunk_id
                    || session
                        .tx
                        .send(ConnectionCommand::Commit {
                            audio_path: audio_path.clone(),
                            audio_duration_ms: source_duration_ms(
                                session.sent_source_samples,
                                session.source_rate,
                            ),
                        })
                        .is_err()
                {
                    let failure = session
                        .failure
                        .lock()
                        .ok()
                        .and_then(|failure| failure.clone())
                        .unwrap_or_else(transport_failure);
                    realtime_result_store()
                        .publish(audio_path, RealtimeStoredResult::Failed(failure));
                }
            }
            ManagerCommand::Close(source_key) => {
                if let Some(session) = sessions.remove(&source_key) {
                    let _ = session.tx.send(ConnectionCommand::Close);
                }
            }
        }
    }
    for session in sessions.into_values() {
        let _ = session.tx.send(ConnectionCommand::Close);
    }
}

async fn connection_loop(
    request: RealtimeSamplesRequest,
    mut commands: mpsc::UnboundedReceiver<ConnectionCommand>,
    on_partial: RealtimePartialCallback,
    shared_failure: Arc<Mutex<Option<TranscriptionFailure>>>,
) {
    let Some(api_key) = request.credentials.soniox_api_key() else {
        store_shared_failure(
            &shared_failure,
            local_failure(
                FailureCategory::Authentication,
                "Soniox API key is not configured; add it in Settings > Integrations",
            ),
        );
        return;
    };
    let Ok((socket, _)) = connect_async(SONIOX_REALTIME_URL).await else {
        store_shared_failure(&shared_failure, transport_failure());
        return;
    };
    let (mut writer, mut reader) = socket.split();
    let configuration = configuration_message(
        api_key,
        request.sample_rate,
        request.language.whisper_code(),
        request.dictionary.canonical_terms().to_vec(),
    );
    if writer
        .send(Message::Text(configuration.to_string().into()))
        .await
        .is_err()
    {
        store_shared_failure(&shared_failure, transport_failure());
        return;
    }

    let mut transcript = TokenAccumulator::default();
    let mut committed_path: Option<PathBuf> = None;
    let mut committed_audio_duration_ms = 0;
    let mut completion_deadline: Option<Instant> = None;
    loop {
        let deadline =
            completion_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            command = commands.recv() => match command {
                Some(ConnectionCommand::Append(audio)) => {
                    if writer.send(Message::Binary(audio.into())).await.is_err() {
                        finish_with_failure(&shared_failure, &mut committed_path, transport_failure());
                        return;
                    }
                }
                Some(ConnectionCommand::Commit { audio_path, audio_duration_ms }) => {
                    committed_path = Some(audio_path);
                    committed_audio_duration_ms = audio_duration_ms;
                    completion_deadline = Some(Instant::now() + COMMIT_COMPLETION_WAIT);
                    if writer.send(Message::Binary(Vec::new().into())).await.is_err() {
                        finish_with_failure(&shared_failure, &mut committed_path, transport_failure());
                        return;
                    }
                }
                Some(ConnectionCommand::Close) | None => {
                    let _ = writer.close().await;
                    return;
                }
            },
            message = reader.next() => {
                let value = match message {
                    Some(Ok(Message::Text(message))) => serde_json::from_str::<Value>(&message),
                    Some(Ok(Message::Binary(message))) => serde_json::from_slice::<Value>(&message),
                    Some(Ok(Message::Ping(payload))) => {
                        if writer.send(Message::Pong(payload)).await.is_err() {
                            finish_with_failure(&shared_failure, &mut committed_path, transport_failure());
                            return;
                        }
                        continue;
                    }
                    Some(Ok(Message::Pong(_))) => continue,
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                        finish_with_failure(&shared_failure, &mut committed_path, transport_failure());
                        return;
                    }
                    Some(Ok(_)) => continue,
                };
                let Ok(value) = value else {
                    finish_with_failure(&shared_failure, &mut committed_path, invalid_response_failure());
                    return;
                };
                match parse_server_response(&value) {
                    Ok(response) => {
                        if transcript.apply(&response.tokens).is_err() {
                            finish_with_failure(&shared_failure, &mut committed_path, invalid_response_failure());
                            return;
                        }
                        if !response.tokens.is_empty() {
                            on_partial(RealtimePartial {
                                source_key: request.source_key.clone(),
                                source_label: request.source_label.clone(),
                                microphone_slot: request.microphone_slot,
                                chunk_id: request.chunk_id,
                                text: transcript.text(),
                            });
                        }
                        if response.finished {
                            let Some(path) = committed_path.take() else {
                                store_shared_failure(&shared_failure, invalid_response_failure());
                                return;
                            };
                            publish_completed_result(
                                path,
                                &request.model_id,
                                transcript.final_text(),
                                committed_audio_duration_ms,
                            );
                            let _ = writer.close().await;
                            return;
                        }
                    }
                    Err(failure) => {
                        finish_with_failure(&shared_failure, &mut committed_path, failure);
                        return;
                    }
                }
            },
            _ = sleep_until(deadline), if completion_deadline.is_some() => {
                finish_with_failure(&shared_failure, &mut committed_path, timeout_failure());
                return;
            }
        }
    }
}

fn configuration_message(
    api_key: &str,
    sample_rate: u32,
    language: Option<&str>,
    terms: Vec<String>,
) -> Value {
    let mut configuration = json!({
        "api_key": api_key,
        "model": SONIOX_REALTIME_MODEL,
        "audio_format": "s16le",
        "num_channels": 1,
        "sample_rate": sample_rate,
        "client_reference_id": format!("WakeNote/{}", env!("CARGO_PKG_VERSION")),
    });
    if let Some(language) = language {
        configuration["language_hints"] = json!([language]);
    }
    if !terms.is_empty() {
        configuration["context"] = json!({ "terms": terms });
    }
    configuration
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SonioxToken {
    text: String,
    is_final: bool,
    start_ms: Option<u64>,
    end_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SonioxResponse {
    tokens: Vec<SonioxToken>,
    finished: bool,
}

fn parse_server_response(value: &Value) -> Result<SonioxResponse, TranscriptionFailure> {
    if value.get("error_code").is_some() {
        return Err(provider_failure_from_value(value));
    }
    let tokens = value
        .get("tokens")
        .and_then(Value::as_array)
        .ok_or_else(invalid_response_failure)?
        .iter()
        .map(|token| {
            Some(SonioxToken {
                text: token.get("text")?.as_str()?.to_string(),
                is_final: token.get("is_final")?.as_bool()?,
                start_ms: token.get("start_ms").and_then(Value::as_u64),
                end_ms: token.get("end_ms").and_then(Value::as_u64),
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(invalid_response_failure)?;
    let finished = match value.get("finished") {
        Some(finished) => finished.as_bool().ok_or_else(invalid_response_failure)?,
        None => false,
    };
    Ok(SonioxResponse { tokens, finished })
}

#[derive(Default)]
struct TokenAccumulator {
    final_text: String,
    non_final_text: String,
    finalized_timed_tokens: HashSet<(u64, u64, String)>,
}

impl TokenAccumulator {
    fn apply(&mut self, tokens: &[SonioxToken]) -> Result<(), ()> {
        let mut non_final_text = String::new();
        for token in tokens {
            if token.is_final {
                let should_append = match (token.start_ms, token.end_ms) {
                    (Some(start_ms), Some(end_ms)) => {
                        self.finalized_timed_tokens
                            .insert((start_ms, end_ms, token.text.clone()))
                    }
                    (None, None) => true,
                    _ => return Err(()),
                };
                if should_append {
                    self.final_text.push_str(&token.text);
                }
            } else {
                non_final_text.push_str(&token.text);
            }
        }
        self.non_final_text = non_final_text;
        Ok(())
    }

    fn text(&self) -> String {
        format!("{}{}", self.final_text, self.non_final_text)
            .trim()
            .to_string()
    }

    fn final_text(&self) -> String {
        self.final_text.trim().to_string()
    }
}

fn encode_pcm_s16le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        bytes.extend_from_slice(&pcm.to_le_bytes());
    }
    bytes
}

fn unseen_samples(samples: &[f32], sent_samples: usize) -> (&[f32], usize) {
    (&samples[sent_samples.min(samples.len())..], samples.len())
}

fn source_duration_ms(samples: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        0
    } else {
        (samples as u64).saturating_mul(1_000) / sample_rate as u64
    }
}

fn publish_completed_result(
    audio_path: PathBuf,
    model_id: &str,
    text: String,
    audio_duration_ms: u64,
) {
    realtime_result_store().publish(
        audio_path,
        RealtimeStoredResult::Completed(TranscriptionExecution {
            text,
            speaker_turns: Vec::new(),
            requested_model_id: model_id.to_string(),
            effective_model_id: model_id.to_string(),
            fallback_from_model_id: None,
            usage: Some(TranscriptionUsage {
                provider: Some(PROVIDER.to_string()),
                audio_duration_ms,
                provider_cost_usd: None,
            }),
            issue: None,
        }),
    );
}

fn provider_failure_from_value(value: &Value) -> TranscriptionFailure {
    let http_status = value
        .get("error_code")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok());
    let provider_code = value
        .get("error_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let category = if http_status == Some(402)
        || matches!(
            provider_code.as_deref(),
            Some(
                "organization_balance_exhausted"
                    | "organization_monthly_budget_exhausted"
                    | "project_monthly_budget_exhausted"
            )
        ) {
        FailureCategory::BillingLimit
    } else if http_status == Some(429) || provider_code.as_deref() == Some("limit_exceeded") {
        FailureCategory::RateLimit
    } else if matches!(http_status, Some(401 | 403))
        || matches!(
            provider_code.as_deref(),
            Some("unauthenticated" | "temp_api_key_session_expired")
        )
    {
        FailureCategory::Authentication
    } else {
        FailureCategory::Provider
    };
    let mut safe_message = match http_status {
        Some(status) => format!("Soniox real-time transcription failed with HTTP {status}"),
        None => "Soniox real-time transcription failed".to_string(),
    };
    if let Some(code) = provider_code.as_deref() {
        safe_message.push_str(&format!(" ({code})"));
    }
    if let Some(request_id) = value.get("request_id").and_then(Value::as_str) {
        safe_message.push_str(&format!(" [request {request_id}]"));
    }
    TranscriptionFailure {
        provider: Some(PROVIDER),
        http_status,
        provider_code,
        safe_message,
        category,
    }
}

fn local_failure(category: FailureCategory, message: &str) -> TranscriptionFailure {
    TranscriptionFailure {
        provider: Some(PROVIDER),
        http_status: None,
        provider_code: None,
        safe_message: message.to_string(),
        category,
    }
}

fn transport_failure() -> TranscriptionFailure {
    local_failure(
        FailureCategory::Transport,
        "Soniox real-time transcription connection failed",
    )
}

fn timeout_failure() -> TranscriptionFailure {
    TranscriptionFailure {
        provider: Some(PROVIDER),
        http_status: None,
        provider_code: Some("completion_timeout".to_string()),
        safe_message: "Soniox real-time transcription timed out".to_string(),
        category: FailureCategory::Transport,
    }
}

fn invalid_response_failure() -> TranscriptionFailure {
    local_failure(
        FailureCategory::InvalidResponse,
        "Soniox returned an invalid real-time transcription response",
    )
}

fn store_shared_failure(
    shared: &Arc<Mutex<Option<TranscriptionFailure>>>,
    failure: TranscriptionFailure,
) {
    if let Ok(mut slot) = shared.lock() {
        *slot = Some(failure);
    }
}

fn finish_with_failure(
    shared: &Arc<Mutex<Option<TranscriptionFailure>>>,
    committed_path: &mut Option<PathBuf>,
    failure: TranscriptionFailure,
) {
    store_shared_failure(shared, failure.clone());
    if let Some(path) = committed_path.take() {
        realtime_result_store().publish(path, RealtimeStoredResult::Failed(failure));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soniox_realtime_builds_documented_configuration_and_pcm_frames() {
        let configuration =
            configuration_message("secret", 16_000, Some("ko"), vec!["WakeNote".into()]);
        assert_eq!(configuration["api_key"], "secret");
        assert_eq!(configuration["model"], "stt-rt-v5");
        assert_eq!(configuration["audio_format"], "s16le");
        assert_eq!(configuration["num_channels"], 1);
        assert_eq!(configuration["sample_rate"], 16_000);
        assert_eq!(configuration["language_hints"], json!(["ko"]));
        assert_eq!(configuration["context"]["terms"], json!(["WakeNote"]));

        let automatic = configuration_message("secret", 48_000, None, Vec::new());
        assert!(automatic.get("language_hints").is_none());
        assert!(automatic.get("context").is_none());

        let bytes = encode_pcm_s16le(&[-1.0, 0.0, 1.0]);
        assert_eq!(bytes.len(), 6);
        assert_eq!(i16::from_le_bytes([bytes[0], bytes[1]]), -i16::MAX);
        assert_eq!(i16::from_le_bytes([bytes[2], bytes[3]]), 0);
        assert_eq!(i16::from_le_bytes([bytes[4], bytes[5]]), i16::MAX);

        let cumulative = [0.0, 0.5, -0.5, 1.0];
        let (unseen, sent) = unseen_samples(&cumulative, 2);
        assert_eq!(unseen, &[-0.5, 1.0]);
        assert_eq!(sent, 4);
        assert_eq!(source_duration_ms(16_000, 16_000), 1_000);
    }

    #[test]
    fn soniox_realtime_promotes_tokens_without_duplicate_final_text() {
        let mut transcript = TokenAccumulator::default();
        transcript
            .apply(&[SonioxToken {
                text: "Wake".into(),
                is_final: false,
                start_ms: Some(0),
                end_ms: Some(100),
            }])
            .expect("partial");
        assert_eq!(transcript.text(), "Wake");

        transcript
            .apply(&[
                SonioxToken {
                    text: "Wake".into(),
                    is_final: true,
                    start_ms: Some(0),
                    end_ms: Some(100),
                },
                SonioxToken {
                    text: "Note".into(),
                    is_final: false,
                    start_ms: Some(100),
                    end_ms: Some(200),
                },
            ])
            .expect("promotion");
        assert_eq!(transcript.text(), "WakeNote");

        transcript
            .apply(&[
                SonioxToken {
                    text: "Wake".into(),
                    is_final: true,
                    start_ms: Some(0),
                    end_ms: Some(100),
                },
                SonioxToken {
                    text: "Note".into(),
                    is_final: true,
                    start_ms: Some(100),
                    end_ms: Some(200),
                },
            ])
            .expect("finalization");
        assert_eq!(transcript.final_text(), "WakeNote");
    }

    #[test]
    fn soniox_realtime_parses_finished_and_safe_typed_errors() {
        assert_eq!(
            parse_server_response(&json!({
                "tokens": [{
                    "text": "WakeNote",
                    "start_ms": 0,
                    "end_ms": 500,
                    "is_final": true
                }],
                "finished": true
            }))
            .expect("response"),
            SonioxResponse {
                tokens: vec![SonioxToken {
                    text: "WakeNote".into(),
                    is_final: true,
                    start_ms: Some(0),
                    end_ms: Some(500),
                }],
                finished: true,
            }
        );

        for (status, error_type, category) in [
            (401, "unauthenticated", FailureCategory::Authentication),
            (
                402,
                "organization_balance_exhausted",
                FailureCategory::BillingLimit,
            ),
            (429, "limit_exceeded", FailureCategory::RateLimit),
            (503, "service_unavailable", FailureCategory::Provider),
        ] {
            let failure = parse_server_response(&json!({
                "tokens": [],
                "error_code": status,
                "error_type": error_type,
                "error_message": "private provider prose",
                "request_id": "request-1"
            }))
            .expect_err("provider error");
            assert_eq!(failure.category, category);
            assert_eq!(failure.http_status, Some(status));
            assert!(failure.safe_message.contains("request-1"));
            assert!(!failure.safe_message.contains("private"));
        }

        assert_eq!(
            parse_server_response(&json!({ "finished": true }))
                .expect_err("missing tokens")
                .category,
            FailureCategory::InvalidResponse
        );
        assert_eq!(
            parse_server_response(&json!({ "tokens": [], "finished": "yes" }))
                .expect_err("invalid finished")
                .category,
            FailureCategory::InvalidResponse
        );
    }
}
