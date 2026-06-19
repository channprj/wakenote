use std::ffi::{CStr, c_char, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Once;

use serde::Deserialize;
use thiserror::Error;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperSegment,
};

use crate::models::{ModelStore, default_model_registry};
use crate::queue::{QueueJobStatus, TranscriptionQueue};
use crate::recorder::{
    ChunkMetadata, ChunkSource, RecordedChunk, RecorderError, TranscriptionSidecar,
};
use crate::settings::{TranscriptionLanguage, expand_user_path};

const WHISPER_SAMPLE_RATE: usize = 16_000;
const MIN_TRANSCRIBABLE_SAMPLES: usize = WHISPER_SAMPLE_RATE / 2;
const MIN_TRANSCRIBABLE_RMS: f32 = 0.003;
const MAX_NO_SPEECH_PROBABILITY: f32 = 0.75;
const MIN_AVERAGE_TOKEN_PROBABILITY: f32 = 0.20;

#[derive(Debug, Clone, Copy)]
pub struct TranscriptionRequest<'a> {
    pub audio_path: &'a Path,
    pub model_id: &'a str,
    pub language: TranscriptionLanguage,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TranscriptionError {
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
}

pub trait Transcriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError>;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionJobOutcome {
    pub id: u64,
    pub status: TranscriptionJobStatus,
}

impl TranscriptionJobOutcome {
    pub fn completed(id: u64) -> Self {
        Self {
            id,
            status: TranscriptionJobStatus::Completed,
        }
    }

    pub fn failed(id: u64, error: impl Into<String>) -> Self {
        Self {
            id,
            status: TranscriptionJobStatus::Failed(error.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionWorker<T> {
    transcriber: T,
    language: TranscriptionLanguage,
    suppress_low_confidence_transcripts: bool,
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
            suppress_low_confidence_transcripts: true,
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
            suppress_low_confidence_transcripts: options.suppress_low_confidence_transcripts,
        }
    }
}

impl<T: Transcriber> TranscriptionWorker<T> {
    pub fn process_started_job(
        &self,
        job: &crate::queue::QueueJob,
    ) -> Result<TranscriptionJobOutcome, TranscriptionWorkerError> {
        let chunk = RecordedChunk::from_audio_path(job.audio_path.clone());
        let request = TranscriptionRequest {
            audio_path: &job.audio_path,
            model_id: &job.model_id,
            language: self.language,
        };

        match self.transcriber.transcribe(request) {
            Ok(transcript) => {
                let suppress_artifacts = self.suppress_low_confidence_transcripts
                    && should_apply_artifact_suppression(&chunk);
                let transcript =
                    if suppress_artifacts && should_suppress_transcript_artifact(&transcript) {
                        String::new()
                    } else {
                        transcript
                    };
                TranscriptionSidecar::write_success(&chunk, &transcript)?;
                Ok(TranscriptionJobOutcome::completed(job.id))
            }
            Err(error) => {
                let message = error.recoverable_message();
                TranscriptionSidecar::write_error(&chunk, &message)?;
                Ok(TranscriptionJobOutcome::failed(job.id, message))
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

pub fn should_suppress_transcript_artifact(text: &str) -> bool {
    let normalized = normalize_transcript_whitespace(text);
    if normalized.is_empty() {
        return false;
    }

    if let Some(inner) = single_wrapped_phrase(&normalized) {
        let inner = inner.trim();
        if inner.is_empty() {
            return false;
        }
        if inner.chars().count() <= 30 {
            return true;
        }
        return contains_non_speech_marker(inner);
    }

    let lowercase = normalized.to_lowercase();
    let compact = normalized.split_whitespace().collect::<String>();
    let compact_lowercase = compact.to_lowercase();
    if is_broadcast_news_signoff(&normalized, &lowercase) {
        return true;
    }
    let common_hallucinations = [
        "thanks for watching",
        "thank you for watching",
        "시청해주셔서감사합니다",
        "시청해 주셔서 감사합니다",
        "끝까지시청해주셔서감사합니다",
    ];

    common_hallucinations
        .iter()
        .any(|marker| lowercase == *marker || compact_lowercase == marker.replace(' ', ""))
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
    if queue.job(outcome.id).is_some_and(|job| {
        matches!(
            job.status,
            QueueJobStatus::Cancelled | QueueJobStatus::Skipped
        )
    }) {
        return Ok(());
    }

    match outcome.status {
        TranscriptionJobStatus::Completed => queue
            .mark_completed(outcome.id)
            .map_err(TranscriptionWorkerError::Queue),
        TranscriptionJobStatus::Failed(error) => queue
            .mark_failed(outcome.id, error)
            .map_err(TranscriptionWorkerError::Queue),
    }
}

#[derive(Debug, Clone)]
pub struct WhisperTranscriber {
    model_directory: PathBuf,
}

impl WhisperTranscriber {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
        }
    }

    fn model_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.bin"))
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        let model_path = self.model_path(request.model_id);
        if !model_path.exists() {
            return Err(TranscriptionError::ModelMissing(model_path));
        }

        let samples = decode_audio_for_whisper(request.audio_path)?;
        if should_skip_low_signal_audio(&samples) {
            return Ok(String::new());
        }
        run_whisper(&model_path, &samples, request.language)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeTranscriber {
    model_directory: PathBuf,
    cohere_api_key: Option<String>,
}

impl RuntimeTranscriber {
    pub fn new(model_directory: impl AsRef<Path>) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
            cohere_api_key: cohere_api_key_from_env(),
        }
    }

    pub fn with_cohere_api_key(
        model_directory: impl AsRef<Path>,
        cohere_api_key: Option<String>,
    ) -> Self {
        Self {
            model_directory: expand_user_path(model_directory.as_ref().to_string_lossy()),
            cohere_api_key,
        }
    }

    fn model_runtime(&self, model_id: &str) -> String {
        model_runtime_for_id(&self.model_directory, model_id)
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
            return ExternalCommandTranscriber::new(&self.model_directory).transcribe(request);
        }

        match self.model_runtime(request.model_id).as_str() {
            "sherpa-onnx" => transcribe_with_sherpa(&self.model_directory, request),
            "external-command" => {
                ExternalCommandTranscriber::new(&self.model_directory).transcribe(request)
            }
            "cohere-api" => CohereTranscriber::new(self.cohere_api_key.clone()).transcribe(request),
            _ => WhisperTranscriber::new(&self.model_directory).transcribe(request),
        }
    }
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
            // not apply here (unlike Whisper/SenseVoice/Cohere).
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
            Ok(recognizer.transcribe(16_000, &samples).text.trim().to_string())
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

#[derive(Debug, Clone)]
pub struct CohereTranscriber {
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CohereTranscriptionResponse {
    text: String,
}

impl CohereTranscriber {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

impl Transcriber for CohereTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        let api_key = self
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                TranscriptionError::Engine(
                    "COHERE_API_KEY is required for Cohere Transcribe".to_string(),
                )
            })?;
        let language = request.language.whisper_code().ok_or_else(|| {
            TranscriptionError::Engine(
                "Cohere Transcribe requires an explicit transcription language".to_string(),
            )
        })?;
        let (audio_path, cleanup_path) = cohere_supported_audio_path(request.audio_path)?;
        let result = post_cohere_transcription(api_key, request.model_id, language, &audio_path);
        if let Some(path) = cleanup_path {
            let _ = std::fs::remove_file(path);
        }
        result
    }
}

fn cohere_api_key_from_env() -> Option<String> {
    std::env::var("COHERE_API_KEY")
        .or_else(|_| std::env::var("CO_API_KEY"))
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Resolve the Cohere key to use for transcription: the configured (settings)
/// value when non-empty, otherwise the environment.
pub fn effective_cohere_api_key(configured: Option<&str>) -> Option<String> {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(cohere_api_key_from_env)
}

fn cohere_supported_audio_path(
    path: &Path,
) -> Result<(PathBuf, Option<PathBuf>), TranscriptionError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    if matches_ignore_ascii_case(extension, &["flac", "mp3", "mpeg", "mpga", "ogg", "wav"]) {
        return Ok((path.to_path_buf(), None));
    }

    let wav_path = path.with_extension("cohere.wav");
    let output = Command::new("/usr/bin/afconvert")
        .arg("-f")
        .arg("WAVE")
        .arg("-d")
        .arg("LEI16@16000")
        .arg(path)
        .arg(&wav_path)
        .output()
        .map_err(|error| TranscriptionError::M4a(error.to_string()))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(TranscriptionError::M4a(if message.is_empty() {
            format!("afconvert exited with status {}", output.status)
        } else {
            message
        }));
    }

    Ok((wav_path.clone(), Some(wav_path)))
}

fn post_cohere_transcription(
    api_key: &str,
    model_id: &str,
    language: &str,
    audio_path: &Path,
) -> Result<String, TranscriptionError> {
    let file_bytes =
        std::fs::read(audio_path).map_err(|error| TranscriptionError::M4a(error.to_string()))?;
    let file_name = audio_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio.wav");
    let boundary = "----wakenote-cohere-transcribe-boundary";
    let body = cohere_multipart_body(boundary, model_id, language, file_name, &file_bytes);
    let response = ureq::post("https://api.cohere.com/v2/audio/transcriptions")
        .set("Authorization", &format!("Bearer {api_key}"))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    let response_text = response
        .into_string()
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    let value: CohereTranscriptionResponse = serde_json::from_str(&response_text)
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    Ok(value.text.trim().to_string())
}

fn cohere_multipart_body(
    boundary: &str,
    model_id: &str,
    language: &str,
    file_name: &str,
    file_bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    body.extend_from_slice(model_id.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"language\"\r\n\r\n");
    body.extend_from_slice(language.as_bytes());
    body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
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

fn run_whisper(
    model_path: &Path,
    samples: &[f32],
    language: TranscriptionLanguage,
) -> Result<String, TranscriptionError> {
    let context = WhisperContext::new_with_params(model_path, default_whisper_context_parameters())
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    let mut state = context
        .create_state()
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_context(true);
    configure_whisper_language(&mut params, language);

    state
        .full(params, samples)
        .map_err(|error| TranscriptionError::Engine(error.to_string()))?;

    let mut transcript = String::new();
    let mut qualities = Vec::new();
    for segment in state.as_iter() {
        transcript.push_str(&segment.to_string());
        qualities.push(decoded_segment_quality(&segment));
    }
    let transcript = transcript.trim().to_string();
    if should_suppress_low_confidence_decode(&transcript, &qualities) {
        Ok(String::new())
    } else {
        Ok(transcript)
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
        text: if suppressed { String::new() } else { text },
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
    let output = Command::new("/usr/bin/afconvert")
        .arg("-f")
        .arg("WAVE")
        .arg("-d")
        .arg("LEI16@16000")
        .arg(path)
        .arg(&wav_path)
        .output()
        .map_err(|error| TranscriptionError::M4a(error.to_string()))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(TranscriptionError::M4a(if message.is_empty() {
            format!("afconvert exited with status {}", output.status)
        } else {
            message
        }));
    }

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

    #[test]
    fn effective_cohere_api_key_uses_configured_value_and_ignores_blanks() {
        // A configured, non-empty key is returned as-is and short-circuits the
        // environment fallback — the settings key takes precedence over env.
        assert_eq!(
            effective_cohere_api_key(Some("configured")),
            Some("configured".to_string())
        );
        assert_eq!(
            effective_cohere_api_key(Some("  trimmed  ")),
            Some("trimmed".to_string())
        );
        // A blank configured key never counts as a key; it falls through to the
        // environment (whose value we don't assert, to stay independent of it).
        let blank = effective_cohere_api_key(Some("   "));
        assert_ne!(blank, Some("   ".to_string()));
        assert_ne!(blank, Some(String::new()));
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
