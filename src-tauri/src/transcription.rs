use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::queue::{QueueJobStatus, TranscriptionQueue};
use crate::recorder::{RecordedChunk, RecorderError, TranscriptionSidecar};
use crate::settings::{TranscriptionLanguage, expand_user_path};

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
}

impl<T> TranscriptionWorker<T> {
    pub fn new(transcriber: T) -> Self {
        Self {
            transcriber,
            language: TranscriptionLanguage::Auto,
        }
    }

    pub fn with_language(transcriber: T, language: TranscriptionLanguage) -> Self {
        Self {
            transcriber,
            language,
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
        run_whisper(&model_path, &samples, request.language)
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

fn run_whisper(
    model_path: &Path,
    samples: &[f32],
    language: TranscriptionLanguage,
) -> Result<String, TranscriptionError> {
    let context = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
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

    let transcript = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string();
    Ok(transcript)
}

pub fn decode_audio_for_whisper(path: &Path) -> Result<Vec<f32>, TranscriptionError> {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("m4a"))
        .unwrap_or(false)
    {
        return read_m4a_as_whisper_audio(path);
    }

    read_wav_as_whisper_audio(path)
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

fn read_m4a_as_whisper_audio(path: &Path) -> Result<Vec<f32>, TranscriptionError> {
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
        let left_index = source_position.floor() as usize;
        let right_index = (left_index + 1).min(samples.len() - 1);
        let fraction = (source_position - left_index as f64) as f32;
        let sample = samples[left_index] * (1.0 - fraction) + samples[right_index] * fraction;
        output.push(sample);
    }
    output
}
