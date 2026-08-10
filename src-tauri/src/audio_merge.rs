use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::DateTime;
use serde::{Deserialize, Serialize};

use crate::audio_codec::convert_to_pcm_wav;
use crate::recorder::encode_wav_to_m4a;

const MIN_INPUTS: usize = 2;
const MAX_INPUTS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeAudioInput {
    pub audio_path: PathBuf,
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeAudioRequest {
    pub operation_id: String,
    pub destination_path: PathBuf,
    pub inputs: Vec<MergeAudioInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioMergeStage {
    Preparing,
    Converting,
    Combining,
    Encoding,
    Saved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMergeProgress {
    pub operation_id: String,
    pub stage: AudioMergeStage,
    pub completed_inputs: usize,
    pub total_inputs: usize,
    pub percent: u8,
    pub current_input_label: Option<String>,
    pub destination_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMergeResult {
    pub operation_id: String,
    pub destination_path: PathBuf,
    pub input_count: usize,
}

pub fn merge_audio_to_m4a<F>(
    request: MergeAudioRequest,
    mut on_progress: F,
) -> Result<AudioMergeResult, String>
where
    F: FnMut(AudioMergeProgress) + Send,
{
    let operation_id = request.operation_id.clone();
    let destination_path = request.destination_path.clone();
    let inputs = validate_and_order_request(request)?;
    let total_inputs = inputs.len();

    emit_progress(
        &mut on_progress,
        &operation_id,
        AudioMergeStage::Preparing,
        0,
        total_inputs,
        0,
        None,
        None,
    );

    let work_dir = create_work_directory(&destination_path, &operation_id)?;
    let _cleanup = WorkDirectoryGuard(work_dir.clone());
    let combined_path = work_dir.join("combined.wav");
    let partial_path = work_dir.join("merged.partial.m4a");
    let combined_spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut combined = hound::WavWriter::create(&combined_path, combined_spec)
        .map_err(|error| format!("audio merge could not prepare combined audio: {error}"))?;

    for (index, input) in inputs.iter().enumerate() {
        let input_label = input
            .audio_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio input")
            .to_string();
        let starting_percent = 5 + (index * 80 / total_inputs) as u8;
        emit_progress(
            &mut on_progress,
            &operation_id,
            AudioMergeStage::Converting,
            index,
            total_inputs,
            starting_percent,
            Some(input_label.clone()),
            None,
        );

        let normalized_path = work_dir.join(format!("input-{index:04}.wav"));
        normalize_input(&input.audio_path, &normalized_path).map_err(|_| {
            format!(
                "audio merge could not normalize input {}",
                input.audio_path.display()
            )
        })?;
        append_normalized_wav(&normalized_path, &mut combined).map_err(|error| {
            format!(
                "audio merge could not append input {}: {error}",
                input.audio_path.display()
            )
        })?;

        let completed = index + 1;
        let completed_percent = 5 + (completed * 80 / total_inputs) as u8;
        emit_progress(
            &mut on_progress,
            &operation_id,
            AudioMergeStage::Combining,
            completed,
            total_inputs,
            completed_percent,
            Some(input_label),
            None,
        );
    }

    combined
        .finalize()
        .map_err(|error| format!("audio merge could not finalize combined audio: {error}"))?;
    emit_progress(
        &mut on_progress,
        &operation_id,
        AudioMergeStage::Encoding,
        total_inputs,
        total_inputs,
        90,
        None,
        None,
    );
    encode_m4a(&combined_path, &partial_path)?;
    fs::rename(&partial_path, &destination_path).map_err(|error| {
        format!(
            "audio merge could not save destination {}: {error}",
            destination_path.display()
        )
    })?;

    emit_progress(
        &mut on_progress,
        &operation_id,
        AudioMergeStage::Saved,
        total_inputs,
        total_inputs,
        100,
        None,
        Some(destination_path.clone()),
    );
    Ok(AudioMergeResult {
        operation_id,
        destination_path,
        input_count: total_inputs,
    })
}

pub fn validate_and_order_request(
    request: MergeAudioRequest,
) -> Result<Vec<MergeAudioInput>, String> {
    validate_operation_id(&request.operation_id)?;
    if !(MIN_INPUTS..=MAX_INPUTS).contains(&request.inputs.len()) {
        return Err(format!(
            "audio merge requires at least {MIN_INPUTS} and at most {MAX_INPUTS} inputs"
        ));
    }
    if !has_extension(&request.destination_path, "m4a") {
        return Err("audio merge destination must use the .m4a extension".to_string());
    }

    let destination_parent = request
        .destination_path
        .parent()
        .ok_or_else(|| "audio merge destination requires a parent directory".to_string())?;
    if !destination_parent.is_dir() {
        return Err("audio merge destination parent directory does not exist".to_string());
    }
    let destination_comparison_path = comparison_path(&request.destination_path)?;

    let mut seen = HashSet::with_capacity(request.inputs.len());
    let mut prepared = Vec::with_capacity(request.inputs.len());
    for input in request.inputs {
        if !input.audio_path.is_file() {
            return Err(format!(
                "audio merge input is not an existing file: {}",
                input.audio_path.display()
            ));
        }
        if !has_any_extension(&input.audio_path, &["m4a", "mp3", "wav"]) {
            return Err(format!(
                "audio merge input has an unsupported extension: {}",
                input.audio_path.display()
            ));
        }
        let canonical = fs::canonicalize(&input.audio_path).map_err(|error| {
            format!(
                "audio merge could not resolve input {}: {error}",
                input.audio_path.display()
            )
        })?;
        if !seen.insert(canonical.clone()) {
            return Err(format!(
                "audio merge input is duplicated: {}",
                input.audio_path.display()
            ));
        }
        if canonical == destination_comparison_path {
            return Err("audio merge destination cannot replace a source input".to_string());
        }
        let recorded_at = DateTime::parse_from_rfc3339(&input.recorded_at).map_err(|_| {
            format!(
                "audio merge recorded_at must be a valid RFC 3339 timestamp: {}",
                input.recorded_at
            )
        })?;
        prepared.push((recorded_at, input));
    }

    prepared.sort_by(|(left_time, left), (right_time, right)| {
        left_time
            .cmp(right_time)
            .then_with(|| left.audio_path.cmp(&right.audio_path))
    });
    Ok(prepared.into_iter().map(|(_, input)| input).collect())
}

fn validate_operation_id(operation_id: &str) -> Result<(), String> {
    if operation_id.is_empty()
        || operation_id.len() > 128
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(
            "audio merge operation id must use 1-128 ASCII letters, digits, hyphens, or underscores"
                .to_string(),
        );
    }
    Ok(())
}

fn comparison_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|error| {
            format!(
                "audio merge could not resolve destination {}: {error}",
                path.display()
            )
        });
    }
    let parent = path
        .parent()
        .ok_or_else(|| "audio merge destination requires a parent directory".to_string())?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "audio merge destination requires a file name".to_string())?;
    fs::canonicalize(parent)
        .map(|canonical_parent| canonical_parent.join(file_name))
        .map_err(|error| {
            format!(
                "audio merge could not resolve destination parent {}: {error}",
                parent.display()
            )
        })
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

fn has_any_extension(path: &Path, expected: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            expected
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

#[allow(clippy::too_many_arguments)]
fn emit_progress<F>(
    callback: &mut F,
    operation_id: &str,
    stage: AudioMergeStage,
    completed_inputs: usize,
    total_inputs: usize,
    percent: u8,
    current_input_label: Option<String>,
    destination_path: Option<PathBuf>,
) where
    F: FnMut(AudioMergeProgress),
{
    callback(AudioMergeProgress {
        operation_id: operation_id.to_string(),
        stage,
        completed_inputs,
        total_inputs,
        percent,
        current_input_label,
        destination_path,
    });
}

fn create_work_directory(destination: &Path, operation_id: &str) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "audio merge destination requires a parent directory".to_string())?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("audio merge could not create timestamp: {error}"))?
        .as_nanos();
    for attempt in 0..10 {
        let work_dir = parent.join(format!(
            ".wakenote-merge-{}-{timestamp}-{operation_id}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&work_dir) {
            Ok(()) => return Ok(work_dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "audio merge could not create temporary workspace: {error}"
                ));
            }
        }
    }
    Err("audio merge could not allocate a unique temporary workspace".to_string())
}

fn normalize_input(source: &Path, destination: &Path) -> Result<(), String> {
    convert_to_pcm_wav(source, destination, 44_100, Some(1))
}

fn append_normalized_wav(
    source: &Path,
    destination: &mut hound::WavWriter<std::io::BufWriter<fs::File>>,
) -> Result<(), String> {
    let mut reader = hound::WavReader::open(source).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 44_100
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("normalized input did not use 44.1 kHz mono 16-bit PCM".to_string());
    }
    for sample in reader.samples::<i16>() {
        destination
            .write_sample(sample.map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn encode_m4a(source: &Path, destination: &Path) -> Result<(), String> {
    encode_wav_to_m4a(source, destination, 96)
        .map_err(|error| format!("audio merge could not encode the M4A destination: {error}"))
}

struct WorkDirectoryGuard(PathBuf);

impl Drop for WorkDirectoryGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
