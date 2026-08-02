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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CloudTranscriptionError {
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
        let response = self
            .transport
            .execute(request)
            .map_err(|error| match error {
                CloudTransportError::Timeout => CloudTranscriptionError::Timeout(provider),
                CloudTransportError::Request => CloudTranscriptionError::Request(provider),
            })?;
        if !(200..300).contains(&response.status) {
            return Err(CloudTranscriptionError::HttpStatus {
                provider,
                status: response.status,
            });
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
        assert_eq!(
            error,
            CloudTranscriptionError::HttpStatus {
                provider: "OpenRouter",
                status: 401,
            }
        );
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
        assert_eq!(
            client.transcribe_openrouter(&audio_path, TranscriptionLanguage::Auto),
            Err(CloudTranscriptionError::Timeout("OpenRouter"))
        );
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
