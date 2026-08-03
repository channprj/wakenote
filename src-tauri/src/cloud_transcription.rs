use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use reqwest::blocking::multipart::{Form, Part};
use serde::Serialize;
use thiserror::Error;

use crate::dictionary::DictionaryContext;
use crate::models::{OPENAI_GPT_TRANSCRIBE_MODEL, OPENROUTER_QWEN3_ASR_MODEL};
use crate::settings::TranscriptionLanguage;

const OPENROUTER_TRANSCRIPTIONS_URL: &str = "https://openrouter.ai/api/v1/audio/transcriptions";
const OPENAI_TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
const CLOUD_TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(55);

#[derive(Clone, Default)]
pub struct TranscriptionCredentials {
    openrouter_api_key: Option<String>,
    openai_api_key: Option<String>,
}

impl TranscriptionCredentials {
    pub fn new(openrouter_api_key: Option<String>, openai_api_key: Option<String>) -> Self {
        Self {
            openrouter_api_key: normalize_secret(openrouter_api_key),
            openai_api_key: normalize_secret(openai_api_key),
        }
    }

    pub fn openrouter_configured(&self) -> bool {
        self.openrouter_api_key.is_some()
    }

    pub fn openai_configured(&self) -> bool {
        self.openai_api_key.is_some()
    }
}

impl fmt::Debug for TranscriptionCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TranscriptionCredentials")
            .field("openrouter_configured", &self.openrouter_configured())
            .field("openai_configured", &self.openai_configured())
            .finish()
    }
}

fn normalize_secret(secret: Option<String>) -> Option<String> {
    secret.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    BillingLimit,
    RateLimit,
    Authentication,
    Provider,
    Transport,
    InvalidResponse,
    Local,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionFailure {
    pub provider: Option<&'static str>,
    pub http_status: Option<u16>,
    pub provider_code: Option<String>,
    pub safe_message: String,
    pub category: FailureCategory,
}

impl fmt::Display for TranscriptionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.safe_message)
    }
}

impl std::error::Error for TranscriptionFailure {}

const OPENAI_BILLING_LIMIT_CODES: [&str; 4] = [
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
];

pub fn classify_http_failure(
    provider: &'static str,
    status: u16,
    response_body: &str,
) -> TranscriptionFailure {
    let provider_code = (provider == "OpenAI")
        .then(|| serde_json::from_str::<serde_json::Value>(response_body).ok())
        .flatten()
        .and_then(|body| {
            body.get("error")
                .and_then(|error| error.get("code"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
    let category = if (provider == "OpenRouter" && status == 402)
        || (provider == "OpenAI"
            && provider_code
                .as_deref()
                .is_some_and(|code| OPENAI_BILLING_LIMIT_CODES.contains(&code)))
    {
        FailureCategory::BillingLimit
    } else if status == 429 {
        FailureCategory::RateLimit
    } else if matches!(status, 401 | 403) {
        FailureCategory::Authentication
    } else {
        FailureCategory::Provider
    };
    TranscriptionFailure {
        provider: Some(provider),
        http_status: Some(status),
        provider_code,
        safe_message: format!("{provider} transcription failed with HTTP {status}"),
        category,
    }
}

pub fn failure_for_transport(
    provider: &'static str,
    error: CloudTransportError,
) -> TranscriptionFailure {
    let safe_message = match error {
        CloudTransportError::Timeout => format!("{provider} transcription request timed out"),
        CloudTransportError::Request => format!("{provider} transcription request failed"),
    };
    TranscriptionFailure {
        provider: Some(provider),
        http_status: None,
        provider_code: None,
        safe_message,
        category: FailureCategory::Transport,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CloudTranscriptionError {
    #[error("{0}")]
    Failure(TranscriptionFailure),
    #[error("{0} API key is not configured; add it in Settings > Integrations")]
    MissingKey(&'static str),
    #[error("could not read audio for {provider}: {message}")]
    Audio {
        provider: &'static str,
        message: String,
    },
    #[error("unsupported audio format for {0}")]
    UnsupportedAudioFormat(&'static str),
    #[error("{0} transcription request timed out")]
    Timeout(&'static str),
    #[error("{0} transcription request failed")]
    Request(&'static str),
    #[error("{provider} transcription failed with HTTP {status}")]
    HttpStatus { provider: &'static str, status: u16 },
    #[error("{0} returned an invalid transcription response")]
    InvalidResponse(&'static str),
    #[error("{0} returned an empty transcript")]
    EmptyTranscript(&'static str),
}

impl CloudTranscriptionError {
    pub fn into_failure(self) -> TranscriptionFailure {
        let safe_message = self.to_string();
        match self {
            Self::Failure(failure) => failure,
            Self::MissingKey(provider) => TranscriptionFailure {
                provider: Some(provider),
                http_status: None,
                provider_code: None,
                safe_message,
                category: FailureCategory::Authentication,
            },
            Self::Audio { provider, .. } | Self::UnsupportedAudioFormat(provider) => {
                TranscriptionFailure {
                    provider: Some(provider),
                    http_status: None,
                    provider_code: None,
                    safe_message,
                    category: FailureCategory::Local,
                }
            }
            Self::Timeout(provider) | Self::Request(provider) => TranscriptionFailure {
                provider: Some(provider),
                http_status: None,
                provider_code: None,
                safe_message,
                category: FailureCategory::Transport,
            },
            Self::HttpStatus { provider, status } => TranscriptionFailure {
                provider: Some(provider),
                http_status: Some(status),
                provider_code: None,
                safe_message,
                category: if status == 429 {
                    FailureCategory::RateLimit
                } else if matches!(status, 401 | 403) {
                    FailureCategory::Authentication
                } else {
                    FailureCategory::Provider
                },
            },
            Self::InvalidResponse(provider) | Self::EmptyTranscript(provider) => {
                TranscriptionFailure {
                    provider: Some(provider),
                    http_status: None,
                    provider_code: None,
                    safe_message,
                    category: FailureCategory::InvalidResponse,
                }
            }
        }
    }
}

#[derive(Clone)]
pub enum CloudTranscriptionRequest {
    OpenRouter {
        url: &'static str,
        api_key: String,
        body: OpenRouterTranscriptionBody,
    },
    OpenAi {
        url: &'static str,
        api_key: String,
        file_name: String,
        mime_type: &'static str,
        audio: Vec<u8>,
        fields: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpenRouterTranscriptionBody {
    pub model: &'static str,
    pub input_audio: OpenRouterInputAudio,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<&'static str>,
    pub temperature: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenRouterInputAudio {
    pub data: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudTranscriptionResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudTransportError {
    Timeout,
    Request,
}

pub trait CloudTranscriptionTransport: Send + Sync {
    fn execute(
        &self,
        request: CloudTranscriptionRequest,
    ) -> Result<CloudTranscriptionResponse, CloudTransportError>;
}

#[derive(Clone)]
pub struct ReqwestCloudTranscriptionTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestCloudTranscriptionTransport {
    pub fn new() -> Result<Self, CloudTranscriptionError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(CLOUD_TRANSCRIPTION_TIMEOUT)
            .build()
            .map_err(|_| CloudTranscriptionError::Request("Cloud"))?;
        Ok(Self { client })
    }
}

impl CloudTranscriptionTransport for ReqwestCloudTranscriptionTransport {
    fn execute(
        &self,
        request: CloudTranscriptionRequest,
    ) -> Result<CloudTranscriptionResponse, CloudTransportError> {
        let response = match request {
            CloudTranscriptionRequest::OpenRouter { url, api_key, body } => self
                .client
                .post(url)
                .bearer_auth(api_key)
                .json(&body)
                .send(),
            CloudTranscriptionRequest::OpenAi {
                url,
                api_key,
                file_name,
                mime_type,
                audio,
                fields,
            } => {
                let part = Part::bytes(audio)
                    .file_name(file_name)
                    .mime_str(mime_type)
                    .map_err(|_| CloudTransportError::Request)?;
                let mut form = Form::new().part("file", part);
                for (name, value) in fields {
                    form = form.text(name, value);
                }
                self.client
                    .post(url)
                    .bearer_auth(api_key)
                    .multipart(form)
                    .send()
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
pub struct CloudTranscriptionClient {
    credentials: TranscriptionCredentials,
    transport: Arc<dyn CloudTranscriptionTransport>,
}

impl fmt::Debug for CloudTranscriptionClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloudTranscriptionClient")
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl Default for CloudTranscriptionClient {
    fn default() -> Self {
        Self::new(TranscriptionCredentials::default())
            .expect("the static cloud HTTP client configuration is valid")
    }
}

impl CloudTranscriptionClient {
    pub fn new(credentials: TranscriptionCredentials) -> Result<Self, CloudTranscriptionError> {
        Ok(Self {
            credentials,
            transport: Arc::new(ReqwestCloudTranscriptionTransport::new()?),
        })
    }

    pub fn with_transport(
        credentials: TranscriptionCredentials,
        transport: Arc<dyn CloudTranscriptionTransport>,
    ) -> Self {
        Self {
            credentials,
            transport,
        }
    }

    pub fn transcribe_openrouter(
        &self,
        audio_path: &Path,
        language: TranscriptionLanguage,
    ) -> Result<String, CloudTranscriptionError> {
        const PROVIDER: &str = "OpenRouter";
        let api_key = self
            .credentials
            .openrouter_api_key
            .clone()
            .ok_or(CloudTranscriptionError::MissingKey(PROVIDER))?;
        let (audio, format, _) = read_audio(audio_path, PROVIDER)?;
        let request = CloudTranscriptionRequest::OpenRouter {
            url: OPENROUTER_TRANSCRIPTIONS_URL,
            api_key,
            body: OpenRouterTranscriptionBody {
                model: OPENROUTER_QWEN3_ASR_MODEL,
                input_audio: OpenRouterInputAudio {
                    data: base64::engine::general_purpose::STANDARD.encode(audio),
                    format,
                },
                language: language.whisper_code(),
                temperature: 0.0,
            },
        };
        self.execute_and_parse(PROVIDER, request)
    }

    pub fn transcribe_openai(
        &self,
        audio_path: &Path,
        language: TranscriptionLanguage,
        dictionary: &DictionaryContext,
    ) -> Result<String, CloudTranscriptionError> {
        const PROVIDER: &str = "OpenAI";
        let api_key = self
            .credentials
            .openai_api_key
            .clone()
            .ok_or(CloudTranscriptionError::MissingKey(PROVIDER))?;
        let (audio, format, mime_type) = read_audio(audio_path, PROVIDER)?;
        let mut fields = vec![
            ("model".to_string(), OPENAI_GPT_TRANSCRIBE_MODEL.to_string()),
            ("response_format".to_string(), "json".to_string()),
        ];
        if let Some(language) = language.whisper_code() {
            fields.push(("language".to_string(), language.to_string()));
        }
        fields.extend(
            dictionary
                .canonical_terms()
                .iter()
                .cloned()
                .map(|term| ("keywords[]".to_string(), term)),
        );
        let request = CloudTranscriptionRequest::OpenAi {
            url: OPENAI_TRANSCRIPTIONS_URL,
            api_key,
            file_name: format!("audio.{format}"),
            mime_type,
            audio,
            fields,
        };
        self.execute_and_parse(PROVIDER, request)
    }

    fn execute_and_parse(
        &self,
        provider: &'static str,
        request: CloudTranscriptionRequest,
    ) -> Result<String, CloudTranscriptionError> {
        let response = self.transport.execute(request).map_err(|error| {
            CloudTranscriptionError::Failure(failure_for_transport(provider, error))
        })?;
        if !(200..300).contains(&response.status) {
            return Err(CloudTranscriptionError::Failure(classify_http_failure(
                provider,
                response.status,
                &response.body,
            )));
        }
        let parsed: serde_json::Value = serde_json::from_str(&response.body)
            .map_err(|_| CloudTranscriptionError::InvalidResponse(provider))?;
        let text = parsed
            .get("text")
            .and_then(serde_json::Value::as_str)
            .ok_or(CloudTranscriptionError::InvalidResponse(provider))?
            .trim();
        if text.is_empty() {
            return Err(CloudTranscriptionError::EmptyTranscript(provider));
        }
        Ok(text.to_string())
    }
}

fn read_audio(
    audio_path: &Path,
    provider: &'static str,
) -> Result<(Vec<u8>, String, &'static str), CloudTranscriptionError> {
    let format = audio_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or(CloudTranscriptionError::UnsupportedAudioFormat(provider))?;
    let mime_type = match format.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        _ => return Err(CloudTranscriptionError::UnsupportedAudioFormat(provider)),
    };
    let audio = std::fs::read(audio_path).map_err(|error| CloudTranscriptionError::Audio {
        provider,
        message: error.to_string(),
    })?;
    Ok((audio, format, mime_type))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::settings::DictionaryEntry;

    struct FakeTransport {
        requests: Mutex<Vec<CloudTranscriptionRequest>>,
        response: Mutex<Option<Result<CloudTranscriptionResponse, CloudTransportError>>>,
    }

    impl FakeTransport {
        fn responding(response: CloudTranscriptionResponse) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                response: Mutex::new(Some(Ok(response))),
            })
        }

        fn failing(error: CloudTransportError) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                response: Mutex::new(Some(Err(error))),
            })
        }
    }

    impl CloudTranscriptionTransport for FakeTransport {
        fn execute(
            &self,
            request: CloudTranscriptionRequest,
        ) -> Result<CloudTranscriptionResponse, CloudTransportError> {
            self.requests.lock().expect("requests").push(request);
            self.response
                .lock()
                .expect("response")
                .take()
                .expect("one fake response")
        }
    }

    fn audio_fixture(extension: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join(format!("sample.{extension}"));
        std::fs::write(&path, b"audio bytes").expect("audio fixture");
        (directory, path)
    }

    fn success_response(text: &str) -> CloudTranscriptionResponse {
        CloudTranscriptionResponse {
            status: 200,
            body: serde_json::json!({ "text": text }).to_string(),
        }
    }

    #[test]
    fn billing_limit_classification_accepts_only_exact_provider_signals() {
        for code in [
            "credit_balance_exhausted",
            "organization_spend_limit_exceeded",
            "project_spend_limit_exceeded",
            "organization_usage_limit_exceeded",
        ] {
            let failure = classify_http_failure(
                "OpenAI",
                400,
                &serde_json::json!({ "error": { "code": code, "message": "private" } }).to_string(),
            );
            assert_eq!(failure.category, FailureCategory::BillingLimit, "{code}");
            assert_eq!(failure.provider_code.as_deref(), Some(code));
            assert!(!failure.safe_message.contains("private"));
        }

        let openrouter = classify_http_failure("OpenRouter", 402, "billing body");
        assert_eq!(openrouter.category, FailureCategory::BillingLimit);
        assert_eq!(openrouter.provider_code, None);
        assert!(!openrouter.safe_message.contains("billing body"));
    }

    #[test]
    fn billing_limit_classification_rejects_rate_auth_provider_and_malformed_failures() {
        for (provider, status, body, category) in [
            ("OpenAI", 429, "{}", FailureCategory::RateLimit),
            ("OpenAI", 401, "{}", FailureCategory::Authentication),
            ("OpenAI", 403, "{}", FailureCategory::Authentication),
            (
                "OpenAI",
                400,
                r#"{"error":{"code":"invalid_request_error"}}"#,
                FailureCategory::Provider,
            ),
            ("OpenAI", 500, "{}", FailureCategory::Provider),
            ("OpenAI", 400, "not json", FailureCategory::Provider),
            ("OpenRouter", 429, "{}", FailureCategory::RateLimit),
        ] {
            assert_eq!(
                classify_http_failure(provider, status, body).category,
                category,
                "{provider} {status}"
            );
        }
        assert_eq!(
            failure_for_transport("OpenAI", CloudTransportError::Timeout).category,
            FailureCategory::Transport
        );
        assert_eq!(
            failure_for_transport("OpenAI", CloudTransportError::Request).category,
            FailureCategory::Transport
        );
    }

    #[test]
    fn openrouter_builds_documented_audio_json_without_dictionary_fields() {
        let (_directory, audio_path) = audio_fixture("wav");
        let transport = FakeTransport::responding(success_response("hello"));
        let client = CloudTranscriptionClient::with_transport(
            TranscriptionCredentials::new(Some("sk-or-secret".into()), None),
            transport.clone(),
        );

        assert_eq!(
            client
                .transcribe_openrouter(&audio_path, TranscriptionLanguage::Ko)
                .expect("transcript"),
            "hello"
        );
        let requests = transport.requests.lock().expect("requests");
        let CloudTranscriptionRequest::OpenRouter { url, api_key, body } = &requests[0] else {
            panic!("OpenRouter request")
        };
        assert_eq!(*url, OPENROUTER_TRANSCRIPTIONS_URL);
        assert_eq!(api_key, "sk-or-secret");
        assert_eq!(body.model, OPENROUTER_QWEN3_ASR_MODEL);
        assert_eq!(body.language, Some("ko"));
        assert_eq!(body.temperature, 0.0);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&body.input_audio.data)
                .expect("base64"),
            b"audio bytes"
        );
        let json = serde_json::to_value(body).expect("request json");
        assert!(json.get("prompt").is_none());
        assert!(json.get("keywords").is_none());
    }

    #[test]
    fn openai_builds_multipart_with_language_and_repeated_keywords() {
        let (_directory, audio_path) = audio_fixture("m4a");
        let transport = FakeTransport::responding(success_response("안녕하세요"));
        let dictionary = DictionaryContext::compile(
            true,
            &[
                DictionaryEntry {
                    id: "wake".into(),
                    term: "WakeNote".into(),
                    aliases: vec!["wake note".into()],
                    enabled: true,
                },
                DictionaryEntry {
                    id: "qwen".into(),
                    term: "Qwen3-ASR".into(),
                    aliases: Vec::new(),
                    enabled: true,
                },
            ],
        );
        let client = CloudTranscriptionClient::with_transport(
            TranscriptionCredentials::new(None, Some("sk-openai-secret".into())),
            transport.clone(),
        );

        client
            .transcribe_openai(&audio_path, TranscriptionLanguage::Ko, &dictionary)
            .expect("transcript");

        let requests = transport.requests.lock().expect("requests");
        let CloudTranscriptionRequest::OpenAi {
            url,
            api_key,
            file_name,
            mime_type,
            audio,
            fields,
        } = &requests[0]
        else {
            panic!("OpenAI request")
        };
        assert_eq!(*url, OPENAI_TRANSCRIPTIONS_URL);
        assert_eq!(api_key, "sk-openai-secret");
        assert_eq!(file_name, "audio.m4a");
        assert_eq!(*mime_type, "audio/mp4");
        assert_eq!(audio, b"audio bytes");
        assert!(fields.contains(&("model".into(), OPENAI_GPT_TRANSCRIBE_MODEL.into())));
        assert!(fields.contains(&("response_format".into(), "json".into())));
        assert!(fields.contains(&("language".into(), "ko".into())));
        assert_eq!(
            fields
                .iter()
                .filter(|(name, _)| name == "keywords[]")
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>(),
            vec!["WakeNote", "Qwen3-ASR"]
        );
    }

    #[test]
    fn automatic_language_omits_language_fields() {
        let (_directory, audio_path) = audio_fixture("wav");
        let transport = FakeTransport::responding(success_response("hello"));
        let client = CloudTranscriptionClient::with_transport(
            TranscriptionCredentials::new(None, Some("sk-openai-secret".into())),
            transport.clone(),
        );

        client
            .transcribe_openai(
                &audio_path,
                TranscriptionLanguage::Auto,
                &DictionaryContext::default(),
            )
            .expect("transcript");

        let requests = transport.requests.lock().expect("requests");
        let CloudTranscriptionRequest::OpenAi { fields, .. } = &requests[0] else {
            panic!("OpenAI request")
        };
        assert!(
            !fields
                .iter()
                .any(|(name, _)| { name == "language" || name == "languages[]" })
        );
    }

    #[test]
    fn missing_key_fails_before_trying_to_read_audio() {
        let transport = FakeTransport::responding(success_response("unused"));
        let client = CloudTranscriptionClient::with_transport(
            TranscriptionCredentials::default(),
            transport.clone(),
        );
        let missing_path = Path::new("/definitely/missing/audio.wav");

        assert_eq!(
            client.transcribe_openrouter(missing_path, TranscriptionLanguage::Auto),
            Err(CloudTranscriptionError::MissingKey("OpenRouter"))
        );
        assert_eq!(
            client.transcribe_openai(
                missing_path,
                TranscriptionLanguage::Auto,
                &DictionaryContext::default(),
            ),
            Err(CloudTranscriptionError::MissingKey("OpenAI"))
        );
        assert!(transport.requests.lock().expect("requests").is_empty());
    }

    #[test]
    fn provider_errors_never_include_response_bodies_or_secrets() {
        let (_directory, audio_path) = audio_fixture("wav");
        let sentinel = "sk-secret audio-base64-sentinel";
        let transport = FakeTransport::responding(CloudTranscriptionResponse {
            status: 401,
            body: sentinel.to_string(),
        });
        let client = CloudTranscriptionClient::with_transport(
            TranscriptionCredentials::new(Some("sk-secret".into()), None),
            transport,
        );

        let error = client
            .transcribe_openrouter(&audio_path, TranscriptionLanguage::Auto)
            .expect_err("provider error");
        let message = error.to_string();
        let CloudTranscriptionError::Failure(failure) = error else {
            panic!("structured provider failure")
        };
        assert_eq!(failure.provider, Some("OpenRouter"));
        assert_eq!(failure.http_status, Some(401));
        assert_eq!(failure.category, FailureCategory::Authentication);
        assert!(!message.contains("sk-secret"));
        assert!(!message.contains("base64"));
    }

    #[test]
    fn malformed_blank_and_timeout_responses_are_actionable() {
        let (_directory, audio_path) = audio_fixture("wav");
        for (response, expected) in [
            (
                CloudTranscriptionResponse {
                    status: 200,
                    body: "not json".into(),
                },
                CloudTranscriptionError::InvalidResponse("OpenRouter"),
            ),
            (
                success_response("   "),
                CloudTranscriptionError::EmptyTranscript("OpenRouter"),
            ),
        ] {
            let client = CloudTranscriptionClient::with_transport(
                TranscriptionCredentials::new(Some("key".into()), None),
                FakeTransport::responding(response),
            );
            assert_eq!(
                client.transcribe_openrouter(&audio_path, TranscriptionLanguage::Auto),
                Err(expected)
            );
        }

        let client = CloudTranscriptionClient::with_transport(
            TranscriptionCredentials::new(Some("key".into()), None),
            FakeTransport::failing(CloudTransportError::Timeout),
        );
        let error = client
            .transcribe_openrouter(&audio_path, TranscriptionLanguage::Auto)
            .expect_err("timeout");
        let CloudTranscriptionError::Failure(failure) = error else {
            panic!("structured timeout failure")
        };
        assert_eq!(failure.category, FailureCategory::Transport);
        assert_eq!(failure.provider, Some("OpenRouter"));
    }

    #[test]
    fn debug_output_reports_configuration_without_secrets() {
        let credentials = TranscriptionCredentials::new(
            Some("sk-openrouter-secret".into()),
            Some("sk-openai-secret".into()),
        );
        let debug = format!("{credentials:?}");
        assert!(debug.contains("openrouter_configured: true"));
        assert!(debug.contains("openai_configured: true"));
        assert!(!debug.contains("sk-openrouter-secret"));
        assert!(!debug.contains("sk-openai-secret"));
    }
}
