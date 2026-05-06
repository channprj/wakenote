use std::path::{Path, PathBuf};

use thiserror::Error;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::queue::TranscriptionQueue;
use crate::recorder::{RecordedChunk, RecorderError, TranscriptionSidecar};

#[derive(Debug, Clone, Copy)]
pub struct TranscriptionRequest<'a> {
    pub audio_path: &'a Path,
    pub model_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TranscriptionError {
    #[error("model file not found: {0}")]
    ModelMissing(PathBuf),
    #[error("unsupported audio format: {0}")]
    UnsupportedAudioFormat(String),
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
}

impl<T> TranscriptionWorker<T> {
    pub fn new(transcriber: T) -> Self {
        Self { transcriber }
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

        let outcome = self.process_started_job(&job)?;
        apply_outcome(queue, outcome)?;

        Ok(Some(job.id))
    }
}

pub fn apply_outcome(
    queue: &mut TranscriptionQueue,
    outcome: TranscriptionJobOutcome,
) -> Result<(), TranscriptionWorkerError> {
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
    pub fn new(model_directory: impl Into<PathBuf>) -> Self {
        Self {
            model_directory: model_directory.into(),
        }
    }

    fn model_path(&self, model_id: &str) -> PathBuf {
        self.model_directory.join(format!("{model_id}.bin"))
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        if request
            .audio_path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("m4a"))
            .unwrap_or(false)
        {
            return Err(TranscriptionError::UnsupportedAudioFormat(
                "m4a decoding requires the native macOS decoder bridge".to_string(),
            ));
        }

        let model_path = self.model_path(request.model_id);
        if !model_path.exists() {
            return Err(TranscriptionError::ModelMissing(model_path));
        }

        let samples = read_wav_as_whisper_audio(request.audio_path)?;
        run_whisper(&model_path, &samples)
    }
}

fn run_whisper(model_path: &Path, samples: &[f32]) -> Result<String, TranscriptionError> {
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

fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    samples
        .chunks(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}

fn resample_linear(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
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
