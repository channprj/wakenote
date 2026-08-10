use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::audio_codec::convert_to_pcm_wav;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AudioRange {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioWaveform {
    pub duration_seconds: f64,
    pub sample_rate: u32,
    /// Absolute amplitude (0..1) per bucket. Used for skip-silence detection
    /// and as a fallback for rendering when signed peaks are unavailable.
    pub peaks: Vec<f32>,
    /// Signed positive peak per bucket (0..1). Renderers draw bars upward
    /// from the centerline using this value.
    #[serde(default)]
    pub peaks_max: Vec<f32>,
    /// Signed negative peak per bucket (-1..0). Renderers draw bars downward
    /// from the centerline using this value.
    #[serde(default)]
    pub peaks_min: Vec<f32>,
    pub audible_ranges: Vec<AudioRange>,
}

#[derive(Debug, Error)]
pub enum AudioAnalysisError {
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("native audio decode failed: {0}")]
    Decode(String),
    #[error("wav decode error: {0}")]
    Wav(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

const DEFAULT_BUCKET_COUNT: usize = 4096;
const WAVEFORM_DECODE_SAMPLE_RATE: u32 = 8_000;
const DEFAULT_SILENCE_THRESHOLD_DBFS: f32 = -45.0;
const MIN_AUDIBLE_RANGE_SECONDS: f64 = 0.12;
const MERGE_SILENCE_GAP_SECONDS: f64 = 1.0;

pub fn analyze_audio_waveform(
    path: &Path,
    bucket_count: Option<usize>,
) -> Result<AudioWaveform, AudioAnalysisError> {
    let bucket_count = bucket_count
        .unwrap_or(DEFAULT_BUCKET_COUNT)
        .clamp(64, 16_384);
    let (samples, sample_rate) = decode_audio_for_waveform(path)?;
    Ok(audio_waveform_from_samples(
        &samples,
        sample_rate,
        bucket_count,
        DEFAULT_SILENCE_THRESHOLD_DBFS,
    ))
}

pub fn audio_waveform_from_samples(
    samples: &[f32],
    sample_rate: u32,
    bucket_count: usize,
    silence_threshold_dbfs: f32,
) -> AudioWaveform {
    let sample_rate = sample_rate.max(1);
    let duration_seconds = samples.len() as f64 / f64::from(sample_rate);
    let (peaks_max, peaks_min) = signed_peaks_from_samples(samples, bucket_count.max(1));
    let peaks: Vec<f32> = peaks_max
        .iter()
        .zip(peaks_min.iter())
        .map(|(positive, negative)| positive.max(negative.abs()).min(1.0))
        .collect();
    let audible_ranges =
        next_audible_ranges_from_peaks(&peaks, duration_seconds, silence_threshold_dbfs);

    AudioWaveform {
        duration_seconds,
        sample_rate,
        peaks,
        peaks_max,
        peaks_min,
        audible_ranges,
    }
}

pub fn next_audible_ranges_from_peaks(
    peaks: &[f32],
    duration_seconds: f64,
    silence_threshold_dbfs: f32,
) -> Vec<AudioRange> {
    if peaks.is_empty() || duration_seconds <= 0.0 {
        return Vec::new();
    }

    let threshold = dbfs_to_amplitude(silence_threshold_dbfs);
    let seconds_per_bucket = duration_seconds / peaks.len() as f64;
    let mut ranges = Vec::<AudioRange>::new();
    let mut current_start: Option<f64> = None;

    for (index, peak) in peaks.iter().enumerate() {
        let audible = peak.is_finite() && *peak >= threshold;
        if audible {
            if current_start.is_none() {
                current_start = Some(index as f64 * seconds_per_bucket);
            }
            continue;
        }

        if let Some(start) = current_start.take() {
            let end = index as f64 * seconds_per_bucket;
            push_audible_range(&mut ranges, start, end, duration_seconds);
        }
    }

    if let Some(start) = current_start {
        push_audible_range(&mut ranges, start, duration_seconds, duration_seconds);
    }

    ranges
}

fn signed_peaks_from_samples(samples: &[f32], bucket_count: usize) -> (Vec<f32>, Vec<f32>) {
    if samples.is_empty() {
        return (vec![0.0; bucket_count], vec![0.0; bucket_count]);
    }

    let mut peaks_max = Vec::with_capacity(bucket_count);
    let mut peaks_min = Vec::with_capacity(bucket_count);
    for bucket in 0..bucket_count {
        let start = bucket * samples.len() / bucket_count;
        let end = ((bucket + 1) * samples.len() / bucket_count).max(start + 1);
        let slice = &samples[start..end.min(samples.len())];
        let mut bucket_max = 0.0_f32;
        let mut bucket_min = 0.0_f32;
        for sample in slice.iter().copied() {
            if !sample.is_finite() {
                continue;
            }
            if sample > bucket_max {
                bucket_max = sample;
            }
            if sample < bucket_min {
                bucket_min = sample;
            }
        }
        peaks_max.push(bucket_max.min(1.0));
        peaks_min.push(bucket_min.max(-1.0));
    }
    (peaks_max, peaks_min)
}

fn push_audible_range(ranges: &mut Vec<AudioRange>, start: f64, end: f64, duration_seconds: f64) {
    if end - start < MIN_AUDIBLE_RANGE_SECONDS {
        return;
    }

    let start = start.max(0.0).min(duration_seconds);
    let end = end.max(start).min(duration_seconds);
    if let Some(previous) = ranges.last_mut()
        && start - previous.end <= MERGE_SILENCE_GAP_SECONDS
    {
        previous.end = end;
        return;
    }

    ranges.push(AudioRange { start, end });
}

fn dbfs_to_amplitude(dbfs: f32) -> f32 {
    10_f32.powf(dbfs / 20.0)
}

fn decode_audio_for_waveform(path: &Path) -> Result<(Vec<f32>, u32), AudioAnalysisError> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "wav" => read_wav_mono(path),
        "m4a" | "mp3" => decode_native_audio_to_wav(path),
        _ => Err(AudioAnalysisError::UnsupportedFormat(extension)),
    }
}

fn decode_native_audio_to_wav(path: &Path) -> Result<(Vec<f32>, u32), AudioAnalysisError> {
    let wav_path = temporary_waveform_path(path);
    convert_to_pcm_wav(path, &wav_path, WAVEFORM_DECODE_SAMPLE_RATE, None)
        .map_err(AudioAnalysisError::Decode)?;

    let decoded = read_wav_mono(&wav_path);
    let _ = std::fs::remove_file(&wav_path);
    decoded
}

fn temporary_waveform_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("audio");
    std::env::temp_dir().join(format!(
        "wakenote-{stem}-{}-{timestamp}.waveform.wav",
        std::process::id()
    ))
}

fn read_wav_mono(path: &Path) -> Result<(Vec<f32>, u32), AudioAnalysisError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| AudioAnalysisError::Wav(error.to_string()))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let sample_rate = spec.sample_rate;
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map_err(|error| AudioAnalysisError::Wav(error.to_string())))
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => reader
            .samples::<i16>()
            .map(|sample| {
                sample
                    .map(|sample| sample as f32 / i16::MAX as f32)
                    .map_err(|error| AudioAnalysisError::Wav(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            let max = ((1_i64 << (u32::from(spec.bits_per_sample) - 1)) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|sample| sample as f32 / max)
                        .map_err(|error| AudioAnalysisError::Wav(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };

    Ok((downmix_to_mono(&samples, channels), sample_rate))
}

fn downmix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    samples
        .chunks(channels.max(1))
        .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
        .collect()
}
