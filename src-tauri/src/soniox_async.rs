use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use reqwest::blocking::multipart::{Form, Part};
use serde_json::{Value, json};

use crate::cloud_transcription::{
    CloudTranscriptionError, CloudTranscriptionResponse, CloudTransportError, FailureCategory,
    TranscriptionCredentials, TranscriptionFailure, failure_for_transport,
};
use crate::dictionary::DictionaryContext;
use crate::settings::TranscriptionLanguage;

const PROVIDER: &str = "Soniox";
const SONIOX_API_BASE_URL: &str = "https://api.soniox.com/v1";
const SONIOX_ASYNC_MODEL: &str = "stt-async-v5";
const SONIOX_REQUEST_TIMEOUT: Duration = Duration::from_secs(55);
const SONIOX_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SONIOX_MAX_POLLS: usize = 1_800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SonioxRestMethod {
    Get,
    Post,
    Delete,
}

#[derive(Clone)]
pub enum SonioxRestRequest {
    Upload {
        api_key: String,
        file_name: String,
        mime_type: &'static str,
        audio: Vec<u8>,
    },
    Json {
        method: SonioxRestMethod,
        url: String,
        api_key: String,
        body: Option<Value>,
    },
}

pub trait SonioxAsyncTransport: Send + Sync {
    fn execute(
        &self,
        request: SonioxRestRequest,
    ) -> Result<CloudTranscriptionResponse, CloudTransportError>;
}

#[derive(Clone)]
struct ReqwestSonioxAsyncTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestSonioxAsyncTransport {
    fn new() -> Result<Self, CloudTranscriptionError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(SONIOX_REQUEST_TIMEOUT)
            .build()
            .map_err(|_| CloudTranscriptionError::Request(PROVIDER))?;
        Ok(Self { client })
    }
}

impl SonioxAsyncTransport for ReqwestSonioxAsyncTransport {
    fn execute(
        &self,
        request: SonioxRestRequest,
    ) -> Result<CloudTranscriptionResponse, CloudTransportError> {
        let response = match request {
            SonioxRestRequest::Upload {
                api_key,
                file_name,
                mime_type,
                audio,
            } => {
                let part = Part::bytes(audio)
                    .file_name(file_name)
                    .mime_str(mime_type)
                    .map_err(|_| CloudTransportError::Request)?;
                self.client
                    .post(format!("{SONIOX_API_BASE_URL}/files"))
                    .bearer_auth(api_key)
                    .multipart(Form::new().part("file", part))
                    .send()
            }
            SonioxRestRequest::Json {
                method,
                url,
                api_key,
                body,
            } => {
                let request = match method {
                    SonioxRestMethod::Get => self.client.get(url),
                    SonioxRestMethod::Post => self.client.post(url),
                    SonioxRestMethod::Delete => self.client.delete(url),
                }
                .bearer_auth(api_key);
                match body {
                    Some(body) => request.json(&body).send(),
                    None => request.send(),
                }
            }
        }
        .map_err(|error| {
            if error.is_timeout() {
                CloudTransportError::Timeout
            } else {
                CloudTransportError::Request
            }
        })?;

        let status = response.status().as_u16();
        let body = response.text().map_err(|_| CloudTransportError::Request)?;
        Ok(CloudTranscriptionResponse { status, body })
    }
}

#[derive(Clone)]
pub struct SonioxAsyncClient {
    api_key: Option<String>,
    transport: Arc<dyn SonioxAsyncTransport>,
    poll_interval: Duration,
    max_polls: usize,
}

impl fmt::Debug for SonioxAsyncClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SonioxAsyncClient")
            .field("configured", &self.api_key.is_some())
            .field("poll_interval", &self.poll_interval)
            .field("max_polls", &self.max_polls)
            .finish_non_exhaustive()
    }
}

impl Default for SonioxAsyncClient {
    fn default() -> Self {
        Self::new(TranscriptionCredentials::default())
            .expect("the static Soniox HTTP client configuration is valid")
    }
}

impl SonioxAsyncClient {
    pub fn new(credentials: TranscriptionCredentials) -> Result<Self, CloudTranscriptionError> {
        Ok(Self {
            api_key: credentials.soniox_api_key().map(str::to_string),
            transport: Arc::new(ReqwestSonioxAsyncTransport::new()?),
            poll_interval: SONIOX_POLL_INTERVAL,
            max_polls: SONIOX_MAX_POLLS,
        })
    }

    #[cfg(test)]
    fn with_transport(
        credentials: TranscriptionCredentials,
        transport: Arc<dyn SonioxAsyncTransport>,
        poll_interval: Duration,
        max_polls: usize,
    ) -> Self {
        Self {
            api_key: credentials.soniox_api_key().map(str::to_string),
            transport,
            poll_interval,
            max_polls,
        }
    }

    pub fn transcribe(
        &self,
        audio_path: &Path,
        language_hints: &[TranscriptionLanguage],
        dictionary: &DictionaryContext,
    ) -> Result<String, CloudTranscriptionError> {
        let api_key = self
            .api_key
            .clone()
            .ok_or(CloudTranscriptionError::MissingKey(PROVIDER))?;
        let (audio, file_name, mime_type) = read_audio(audio_path)?;
        let upload = self.execute(SonioxRestRequest::Upload {
            api_key: api_key.clone(),
            file_name,
            mime_type,
            audio,
        })?;
        let file_id = response_id(&upload.body)?;
        let mut transcription_id = None;

        let result = (|| {
            let create = self.execute(SonioxRestRequest::Json {
                method: SonioxRestMethod::Post,
                url: format!("{SONIOX_API_BASE_URL}/transcriptions"),
                api_key: api_key.clone(),
                body: Some(create_body(&file_id, language_hints, dictionary)),
            })?;
            let id = response_id(&create.body)?;
            transcription_id = Some(id.clone());
            self.wait_until_complete(&api_key, &id)?;
            let transcript = self.execute(SonioxRestRequest::Json {
                method: SonioxRestMethod::Get,
                url: format!("{SONIOX_API_BASE_URL}/transcriptions/{id}/transcript"),
                api_key: api_key.clone(),
                body: None,
            })?;
            parse_transcript(&transcript.body)
        })();

        self.cleanup(&api_key, transcription_id.as_deref(), &file_id);
        result
    }

    fn wait_until_complete(
        &self,
        api_key: &str,
        transcription_id: &str,
    ) -> Result<(), CloudTranscriptionError> {
        for _ in 0..self.max_polls {
            let response = self.execute(SonioxRestRequest::Json {
                method: SonioxRestMethod::Get,
                url: format!("{SONIOX_API_BASE_URL}/transcriptions/{transcription_id}"),
                api_key: api_key.to_string(),
                body: None,
            })?;
            let value: Value = serde_json::from_str(&response.body)
                .map_err(|_| CloudTranscriptionError::InvalidResponse(PROVIDER))?;
            match value.get("status").and_then(Value::as_str) {
                Some("queued" | "processing") => {
                    if !self.poll_interval.is_zero() {
                        thread::sleep(self.poll_interval);
                    }
                }
                Some("completed") => return Ok(()),
                Some("error") => {
                    return Err(CloudTranscriptionError::Failure(soniox_failure_from_value(
                        None, &value,
                    )));
                }
                _ => return Err(CloudTranscriptionError::InvalidResponse(PROVIDER)),
            }
        }
        Err(CloudTranscriptionError::Failure(TranscriptionFailure {
            provider: Some(PROVIDER),
            http_status: None,
            provider_code: Some("poll_timeout".to_string()),
            safe_message: "Soniox transcription timed out".to_string(),
            category: FailureCategory::Transport,
        }))
    }

    fn execute(
        &self,
        request: SonioxRestRequest,
    ) -> Result<CloudTranscriptionResponse, CloudTranscriptionError> {
        let response = self.transport.execute(request).map_err(|error| {
            CloudTranscriptionError::Failure(failure_for_transport(PROVIDER, error))
        })?;
        if !(200..300).contains(&response.status) {
            let value = serde_json::from_str::<Value>(&response.body).unwrap_or(Value::Null);
            return Err(CloudTranscriptionError::Failure(soniox_failure_from_value(
                Some(response.status),
                &value,
            )));
        }
        Ok(response)
    }

    fn cleanup(&self, api_key: &str, transcription_id: Option<&str>, file_id: &str) {
        if let Some(transcription_id) = transcription_id {
            let _ = self.transport.execute(SonioxRestRequest::Json {
                method: SonioxRestMethod::Delete,
                url: format!("{SONIOX_API_BASE_URL}/transcriptions/{transcription_id}"),
                api_key: api_key.to_string(),
                body: None,
            });
        }
        let _ = self.transport.execute(SonioxRestRequest::Json {
            method: SonioxRestMethod::Delete,
            url: format!("{SONIOX_API_BASE_URL}/files/{file_id}"),
            api_key: api_key.to_string(),
            body: None,
        });
    }
}

fn create_body(
    file_id: &str,
    language_hints: &[TranscriptionLanguage],
    dictionary: &DictionaryContext,
) -> Value {
    let mut body = json!({
        "model": SONIOX_ASYNC_MODEL,
        "file_id": file_id,
        "client_reference_id": format!("WakeNote/{}", env!("CARGO_PKG_VERSION")),
    });
    let hints: Vec<_> = crate::settings::normalize_language_hints(language_hints)
        .into_iter()
        .filter_map(TranscriptionLanguage::whisper_code)
        .collect();
    if !hints.is_empty() {
        body["language_hints"] = json!(hints);
    }
    let terms = dictionary.canonical_terms();
    if !terms.is_empty() {
        body["context"] = json!({ "terms": terms });
    }
    body
}

fn response_id(body: &str) -> Result<String, CloudTranscriptionError> {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("id").and_then(Value::as_str).map(str::to_string))
        .filter(|id| !id.trim().is_empty())
        .ok_or(CloudTranscriptionError::InvalidResponse(PROVIDER))
}

fn parse_transcript(body: &str) -> Result<String, CloudTranscriptionError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| CloudTranscriptionError::InvalidResponse(PROVIDER))?;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or(CloudTranscriptionError::InvalidResponse(PROVIDER))?
        .trim();
    if text.is_empty() {
        return Err(CloudTranscriptionError::EmptyTranscript(PROVIDER));
    }
    Ok(text.to_string())
}

fn soniox_failure_from_value(status: Option<u16>, value: &Value) -> TranscriptionFailure {
    let provider_code = value
        .get("error_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let request_id = value.get("request_id").and_then(Value::as_str);
    let category = if status == Some(402)
        || provider_code.as_deref() == Some("organization_balance_exhausted")
    {
        FailureCategory::BillingLimit
    } else if status == Some(429) || provider_code.as_deref() == Some("limit_exceeded") {
        FailureCategory::RateLimit
    } else if matches!(status, Some(401 | 403))
        || provider_code.as_deref() == Some("unauthenticated")
    {
        FailureCategory::Authentication
    } else {
        FailureCategory::Provider
    };
    let mut safe_message = match status {
        Some(status) => format!("Soniox transcription failed with HTTP {status}"),
        None => "Soniox transcription failed".to_string(),
    };
    if let Some(code) = provider_code.as_deref() {
        safe_message.push_str(&format!(" ({code})"));
    }
    if let Some(request_id) = request_id {
        safe_message.push_str(&format!(" [request {request_id}]"));
    }
    TranscriptionFailure {
        provider: Some(PROVIDER),
        http_status: status,
        provider_code,
        safe_message,
        category,
    }
}

fn read_audio(
    audio_path: &Path,
) -> Result<(Vec<u8>, String, &'static str), CloudTranscriptionError> {
    let extension = audio_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or(CloudTranscriptionError::UnsupportedAudioFormat(PROVIDER))?;
    let mime_type = match extension.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        _ => return Err(CloudTranscriptionError::UnsupportedAudioFormat(PROVIDER)),
    };
    let audio = std::fs::read(audio_path).map_err(|error| CloudTranscriptionError::Audio {
        provider: PROVIDER,
        message: error.to_string(),
    })?;
    let file_name = audio_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .filter(|file_name| !file_name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("audio.{extension}"));
    Ok((audio, file_name, mime_type))
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::cloud_transcription::{
        CloudTranscriptionError, CloudTranscriptionResponse, CloudTransportError, FailureCategory,
        TranscriptionCredentials,
    };
    use crate::dictionary::DictionaryContext;
    use crate::settings::{DictionaryEntry, TranscriptionLanguage};

    struct ScriptedTransport {
        requests: Mutex<Vec<SonioxRestRequest>>,
        responses: Mutex<VecDeque<Result<CloudTranscriptionResponse, CloudTransportError>>>,
    }

    impl ScriptedTransport {
        fn new(
            responses: impl IntoIterator<Item = Result<CloudTranscriptionResponse, CloudTransportError>>,
        ) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                responses: Mutex::new(responses.into_iter().collect()),
            })
        }
    }

    impl SonioxAsyncTransport for ScriptedTransport {
        fn execute(
            &self,
            request: SonioxRestRequest,
        ) -> Result<CloudTranscriptionResponse, CloudTransportError> {
            self.requests.lock().expect("requests").push(request);
            self.responses
                .lock()
                .expect("responses")
                .pop_front()
                .expect("scripted response")
        }
    }

    fn response(status: u16, body: serde_json::Value) -> CloudTranscriptionResponse {
        CloudTranscriptionResponse {
            status,
            body: body.to_string(),
        }
    }

    fn empty_response(status: u16) -> CloudTranscriptionResponse {
        CloudTranscriptionResponse {
            status,
            body: String::new(),
        }
    }

    fn audio_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("sample.wav");
        std::fs::write(&path, b"audio bytes").expect("audio fixture");
        (directory, path)
    }

    fn client(transport: Arc<ScriptedTransport>, key: Option<&str>) -> SonioxAsyncClient {
        client_with_max_polls(transport, key, 8)
    }

    fn client_with_max_polls(
        transport: Arc<ScriptedTransport>,
        key: Option<&str>,
        max_polls: usize,
    ) -> SonioxAsyncClient {
        SonioxAsyncClient::with_transport(
            TranscriptionCredentials::new(None, None, key.map(str::to_string)),
            transport,
            Duration::ZERO,
            max_polls,
        )
    }

    #[test]
    fn soniox_async_runs_documented_flow_with_hints_context_and_cleanup() {
        let (_directory, audio_path) = audio_fixture();
        let transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(200, json!({ "id": "transcription-1" }))),
            Ok(response(200, json!({ "status": "queued" }))),
            Ok(response(200, json!({ "status": "processing" }))),
            Ok(response(200, json!({ "status": "completed" }))),
            Ok(response(
                200,
                json!({ "text": "WakeNote works", "tokens": [] }),
            )),
            Ok(empty_response(204)),
            Ok(empty_response(204)),
        ]);
        let dictionary = DictionaryContext::compile(
            true,
            &[DictionaryEntry {
                id: "wake".into(),
                term: "WakeNote".into(),
                aliases: Vec::new(),
                enabled: true,
            }],
        );

        let text = client(transport.clone(), Some("soniox-secret"))
            .transcribe(
                &audio_path,
                &[TranscriptionLanguage::En, TranscriptionLanguage::Ko],
                &dictionary,
            )
            .expect("transcript");

        assert_eq!(text, "WakeNote works");
        let requests = transport.requests.lock().expect("requests");
        assert_eq!(requests.len(), 8);
        let SonioxRestRequest::Json {
            body: Some(body), ..
        } = &requests[1]
        else {
            panic!("create request")
        };
        assert_eq!(body["model"], "stt-async-v5");
        assert_eq!(body["file_id"], "file-1");
        assert_eq!(body["language_hints"], json!(["en", "ko"]));
        assert_eq!(body["context"]["terms"], json!(["WakeNote"]));
        assert!(matches!(
            &requests[6],
            SonioxRestRequest::Json { method: SonioxRestMethod::Delete, url, .. }
                if url.ends_with("/transcriptions/transcription-1")
        ));
        assert!(matches!(
            &requests[7],
            SonioxRestRequest::Json { method: SonioxRestMethod::Delete, url, .. }
                if url.ends_with("/files/file-1")
        ));
    }

    #[test]
    fn soniox_async_auto_omits_language_hints_and_missing_key_precedes_file_io() {
        let (_directory, audio_path) = audio_fixture();
        let transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(200, json!({ "id": "transcription-1" }))),
            Ok(response(200, json!({ "status": "completed" }))),
            Ok(response(200, json!({ "text": "hello" }))),
            Ok(empty_response(204)),
            Ok(empty_response(204)),
        ]);
        client(transport.clone(), Some("key"))
            .transcribe(&audio_path, &[], &DictionaryContext::default())
            .expect("transcript");
        let requests = transport.requests.lock().expect("requests");
        let SonioxRestRequest::Json {
            body: Some(body), ..
        } = &requests[1]
        else {
            panic!("create request")
        };
        assert!(body.get("language_hints").is_none());
        assert!(body.get("context").is_none());
        drop(requests);

        let missing_transport = ScriptedTransport::new([]);
        assert_eq!(
            client(missing_transport.clone(), None).transcribe(
                Path::new("/definitely/missing.wav"),
                &[],
                &DictionaryContext::default(),
            ),
            Err(CloudTranscriptionError::MissingKey("Soniox"))
        );
        assert!(missing_transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn soniox_async_distinguishes_blank_malformed_and_safe_provider_errors() {
        for (transcript, expected) in [
            (
                json!({ "text": "   " }),
                CloudTranscriptionError::EmptyTranscript("Soniox"),
            ),
            (
                json!({ "tokens": [] }),
                CloudTranscriptionError::InvalidResponse("Soniox"),
            ),
        ] {
            let (_directory, audio_path) = audio_fixture();
            let transport = ScriptedTransport::new([
                Ok(response(200, json!({ "id": "file-1" }))),
                Ok(response(200, json!({ "id": "transcription-1" }))),
                Ok(response(200, json!({ "status": "completed" }))),
                Ok(response(200, transcript)),
                Ok(empty_response(204)),
                Ok(empty_response(204)),
            ]);
            assert_eq!(
                client(transport, Some("key")).transcribe(
                    &audio_path,
                    &[],
                    &DictionaryContext::default(),
                ),
                Err(expected)
            );
        }

        let (_directory, audio_path) = audio_fixture();
        let transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(200, json!({ "id": "transcription-1" }))),
            Ok(response(
                200,
                json!({
                    "status": "error",
                    "error_type": "organization_balance_exhausted",
                    "error_message": "private soniox provider prose",
                    "request_id": "request-1"
                }),
            )),
            Ok(empty_response(204)),
            Ok(empty_response(204)),
        ]);
        let error = client(transport, Some("key"))
            .transcribe(&audio_path, &[], &DictionaryContext::default())
            .expect_err("provider error");
        let CloudTranscriptionError::Failure(failure) = error else {
            panic!("typed failure")
        };
        assert_eq!(failure.category, FailureCategory::BillingLimit);
        assert_eq!(
            failure.provider_code.as_deref(),
            Some("organization_balance_exhausted")
        );
        assert!(failure.safe_message.contains("request-1"));
        assert!(!failure.safe_message.contains("private"));
    }

    #[test]
    fn soniox_async_cleanup_failures_do_not_replace_success() {
        let (_directory, audio_path) = audio_fixture();
        let transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(200, json!({ "id": "transcription-1" }))),
            Ok(response(200, json!({ "status": "completed" }))),
            Ok(response(200, json!({ "text": "kept" }))),
            Ok(response(500, json!({ "error_type": "internal_error" }))),
            Err(CloudTransportError::Request),
        ]);

        assert_eq!(
            client(transport.clone(), Some("key"))
                .transcribe(&audio_path, &[], &DictionaryContext::default(),)
                .expect("successful transcript"),
            "kept"
        );
        assert_eq!(transport.requests.lock().unwrap().len(), 6);
    }

    #[test]
    fn soniox_async_classifies_http_errors_without_exposing_provider_prose() {
        for (status, error_type, expected_category) in [
            (401, "unauthenticated", FailureCategory::Authentication),
            (
                402,
                "organization_balance_exhausted",
                FailureCategory::BillingLimit,
            ),
            (429, "limit_exceeded", FailureCategory::RateLimit),
        ] {
            let (_directory, audio_path) = audio_fixture();
            let transport = ScriptedTransport::new([Ok(response(
                status,
                json!({
                    "error_type": error_type,
                    "error_message": "private provider prose",
                    "request_id": "request-1"
                }),
            ))]);

            let error = client(transport, Some("key"))
                .transcribe(&audio_path, &[], &DictionaryContext::default())
                .expect_err("provider error");
            let CloudTranscriptionError::Failure(failure) = error else {
                panic!("typed failure")
            };
            assert_eq!(failure.category, expected_category);
            assert_eq!(failure.http_status, Some(status));
            assert!(failure.safe_message.contains("request-1"));
            assert!(!failure.safe_message.contains("private"));
        }
    }

    #[test]
    fn soniox_async_times_out_and_rejects_unknown_statuses_with_cleanup() {
        let (_directory, audio_path) = audio_fixture();
        let timeout_transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(200, json!({ "id": "transcription-1" }))),
            Ok(response(200, json!({ "status": "processing" }))),
            Ok(empty_response(204)),
            Ok(empty_response(204)),
        ]);
        let timeout = client_with_max_polls(timeout_transport.clone(), Some("key"), 1)
            .transcribe(&audio_path, &[], &DictionaryContext::default())
            .expect_err("poll timeout");
        let CloudTranscriptionError::Failure(failure) = timeout else {
            panic!("typed failure")
        };
        assert_eq!(failure.provider_code.as_deref(), Some("poll_timeout"));
        assert_eq!(failure.category, FailureCategory::Transport);
        assert_eq!(timeout_transport.requests.lock().unwrap().len(), 5);

        let unknown_transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(200, json!({ "id": "transcription-1" }))),
            Ok(response(200, json!({ "status": "paused" }))),
            Ok(empty_response(204)),
            Ok(empty_response(204)),
        ]);
        assert_eq!(
            client(unknown_transport.clone(), Some("key")).transcribe(
                &audio_path,
                &[],
                &DictionaryContext::default(),
            ),
            Err(CloudTranscriptionError::InvalidResponse("Soniox"))
        );
        assert_eq!(unknown_transport.requests.lock().unwrap().len(), 5);
    }

    #[test]
    fn soniox_async_cleans_up_uploaded_file_when_creation_fails() {
        let (_directory, audio_path) = audio_fixture();
        let transport = ScriptedTransport::new([
            Ok(response(200, json!({ "id": "file-1" }))),
            Ok(response(500, json!({ "error_type": "internal_error" }))),
            Ok(empty_response(204)),
        ]);

        client(transport.clone(), Some("key"))
            .transcribe(&audio_path, &[], &DictionaryContext::default())
            .expect_err("creation failure");

        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(matches!(
            &requests[2],
            SonioxRestRequest::Json { method: SonioxRestMethod::Delete, url, .. }
                if url.ends_with("/files/file-1")
        ));
    }
}
