//! Tell the enrolled user's voice from other voices. Each clip is reduced to a
//! speaker embedding and compared with the enrolled profile by cosine similarity.
//! Audio and transcripts are never touched; a verdict only labels a clip.
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{ModelCapabilities, ModelDescriptor, ModelStatus, ModelStore};

/// 3D-Speaker ERes2NetV2. Of the sherpa-onnx speaker models it separated Korean
/// speakers best (measured in channprj/wakenote#15); none is trained on Korean.
pub const SPEAKER_MODEL_ID: &str = "speaker-eres2netv2-zh-cn";
pub const SPEAKER_MODEL_RUNTIME: &str = "sherpa-onnx-speaker";
const SPEAKER_MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2netv2_sv_zh-cn_16k-common.onnx";
const SPEAKER_MODEL_SHA256: &str =
    "bf1a75b9930474cf3389ef415e6e5d38ca96fea4a3a00f7e301d080a58ee2239";
const SPEAKER_MODEL_SIZE_MB: u64 = 69;
/// Cosine similarity at or above which a clip counts as the enrolled speaker.
/// Chosen so one value fits both Korean and English 3 s clips.
pub const DEFAULT_THRESHOLD: f32 = 0.65;

pub const SAMPLE_RATE: u32 = 16_000;
/// Shorter clips carry too little voice to judge, so they stay `unknown`.
pub const MIN_SAMPLES: usize = SAMPLE_RATE as usize;
/// Longer clips are judged on their middle six seconds so judging time stays
/// flat however long a chunk runs.
pub const MAX_SAMPLES: usize = 6 * SAMPLE_RATE as usize;
const EMBEDDING_THREADS: usize = 2;

const PROFILE_DIRECTORY: &str = "voice-profile";
const PROFILE_FILE_NAME: &str = "profile.json";
const RECORDINGS_DIRECTORY: &str = "recordings";
const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum SpeakerError {
    #[error("speaker embedding engine is not built into this app")]
    EngineUnavailable,
    #[error("speaker model is not downloaded: {0}")]
    ModelMissing(PathBuf),
    #[error("clip is shorter than one second")]
    TooShort,
    #[error("speaker embedding failed: {0}")]
    Embedding(String),
    #[error("embedding has {actual} dimensions; the profile uses {expected}")]
    DimensionMismatch { expected: usize, actual: usize },
    #[error("invalid recording file name: {0}")]
    InvalidRecordingName(String),
    #[error("voice profile is unreadable: {0}")]
    Profile(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerKind {
    Me,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnknownReason {
    NotEnrolled,
    ProfileModelMismatch,
    ModelMissing,
    EngineUnavailable,
    TooShort,
    DecodeFailed,
    EmbeddingFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerVerdict {
    pub kind: SpeakerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_revision: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<UnknownReason>,
}

impl SpeakerVerdict {
    pub fn unknown(reason: UnknownReason) -> Self {
        Self {
            kind: SpeakerKind::Unknown,
            score: None,
            profile_revision: None,
            reason: Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSampleOrigin {
    /// A sample sentence read aloud during enrollment.
    Enrollment,
    /// A clip the user marked as their own voice.
    Correction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceSample {
    pub origin: VoiceSampleOrigin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// File name under the profile's recordings folder (enrollment only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_file: Option<String>,
    /// Audio of the clip a correction came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_audio_path: Option<String>,
    /// Unit-length speaker embedding.
    pub embedding: Vec<f32>,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub schema_version: u32,
    /// Embeddings only compare within one model; another model needs re-enrollment.
    pub model_id: String,
    /// Bumped on every change so a stored verdict names the profile it came from.
    pub revision: u32,
    pub threshold: f32,
    #[serde(default)]
    pub samples: Vec<VoiceSample>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VoiceProfile {
    pub fn new(model_id: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            model_id: model_id.into(),
            revision: 0,
            threshold: DEFAULT_THRESHOLD,
            samples: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_enrolled(&self) -> bool {
        self.samples
            .iter()
            .any(|sample| sample.origin == VoiceSampleOrigin::Enrollment)
    }

    pub fn add_sample(
        &mut self,
        mut sample: VoiceSample,
        now: DateTime<Utc>,
    ) -> Result<(), SpeakerError> {
        if let Some(expected) = self.samples.first().map(|sample| sample.embedding.len())
            && expected != sample.embedding.len()
        {
            return Err(SpeakerError::DimensionMismatch {
                expected,
                actual: sample.embedding.len(),
            });
        }
        normalize(&mut sample.embedding);
        self.samples.push(sample);
        self.touch(now);
        Ok(())
    }

    pub fn set_threshold(&mut self, threshold: f32, now: DateTime<Utc>) {
        self.threshold = threshold.clamp(-1.0, 1.0);
        self.touch(now);
    }

    /// Unit-length mean of every sample, or `None` before the first sample.
    pub fn centroid(&self) -> Option<Vec<f32>> {
        let dimension = self.samples.first()?.embedding.len();
        let mut sum = vec![0.0; dimension];
        for sample in &self.samples {
            for (total, value) in sum.iter_mut().zip(&sample.embedding) {
                *total += value;
            }
        }
        normalize(&mut sum);
        Some(sum)
    }

    pub fn recording_files(&self) -> impl Iterator<Item = &str> {
        self.samples
            .iter()
            .filter_map(|sample| sample.recording_file.as_deref())
    }

    fn touch(&mut self, now: DateTime<Utc>) {
        self.revision = self.revision.saturating_add(1);
        self.updated_at = now;
    }
}

/// Judge one embedding against the profile. `embedding` must come from the
/// profile's model.
pub fn judge_embedding(profile: &VoiceProfile, embedding: &[f32]) -> SpeakerVerdict {
    let Some(centroid) = profile.centroid() else {
        return SpeakerVerdict::unknown(UnknownReason::NotEnrolled);
    };
    if centroid.len() != embedding.len() {
        return SpeakerVerdict::unknown(UnknownReason::ProfileModelMismatch);
    }
    let score = cosine_similarity(&centroid, embedding);
    SpeakerVerdict {
        kind: if score >= profile.threshold {
            SpeakerKind::Me
        } else {
            SpeakerKind::Other
        },
        score: Some(score),
        profile_revision: Some(profile.revision),
        reason: None,
    }
}

/// Judge 16 kHz mono samples. Anything that prevents a confident comparison
/// yields `unknown` with its reason instead of an error.
pub fn classify_samples(
    embedder: Option<&mut SpeakerEmbedder>,
    profile: Option<&VoiceProfile>,
    samples: &[f32],
) -> SpeakerVerdict {
    let Some(profile) = profile.filter(|profile| profile.is_enrolled()) else {
        return SpeakerVerdict::unknown(UnknownReason::NotEnrolled);
    };
    if profile.model_id != SPEAKER_MODEL_ID {
        return SpeakerVerdict::unknown(UnknownReason::ProfileModelMismatch);
    }
    let Some(embedder) = embedder else {
        return SpeakerVerdict::unknown(UnknownReason::ModelMissing);
    };
    match embedder.embed(samples) {
        Ok(embedding) => judge_embedding(profile, &embedding),
        Err(SpeakerError::TooShort) => SpeakerVerdict::unknown(UnknownReason::TooShort),
        Err(SpeakerError::EngineUnavailable) => {
            SpeakerVerdict::unknown(UnknownReason::EngineUnavailable)
        }
        Err(_) => SpeakerVerdict::unknown(UnknownReason::EmbeddingFailed),
    }
}

/// Decode a recorded clip and judge it.
pub fn classify_audio_file(
    embedder: Option<&mut SpeakerEmbedder>,
    profile: Option<&VoiceProfile>,
    audio_path: &Path,
) -> SpeakerVerdict {
    match crate::transcription::decode_audio_for_whisper(audio_path) {
        Ok(samples) => classify_samples(embedder, profile, &samples),
        Err(_) => SpeakerVerdict::unknown(UnknownReason::DecodeFailed),
    }
}

pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    let (mut dot, mut left_norm, mut right_norm) = (0.0f32, 0.0f32, 0.0f32);
    for (a, b) in left.iter().zip(right) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn normalize(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        values.iter_mut().for_each(|value| *value /= norm);
    }
}

/// Loaded speaker embedding model. Loading costs far more than one clip, so
/// callers keep one around.
pub struct SpeakerEmbedder {
    #[cfg(feature = "asr-sherpa")]
    extractor: sherpa_rs::speaker_id::EmbeddingExtractor,
}

impl SpeakerEmbedder {
    pub fn load(model_path: &Path) -> Result<Self, SpeakerError> {
        if !model_path.is_file() {
            return Err(SpeakerError::ModelMissing(model_path.to_path_buf()));
        }
        #[cfg(feature = "asr-sherpa")]
        {
            let config = sherpa_rs::speaker_id::ExtractorConfig {
                model: model_path.to_string_lossy().into_owned(),
                provider: Some("cpu".to_string()),
                num_threads: Some(EMBEDDING_THREADS),
                debug: false,
            };
            let extractor = sherpa_rs::speaker_id::EmbeddingExtractor::new(config)
                .map_err(|error| SpeakerError::Embedding(error.to_string()))?;
            Ok(Self { extractor })
        }
        #[cfg(not(feature = "asr-sherpa"))]
        {
            let _ = EMBEDDING_THREADS;
            Err(SpeakerError::EngineUnavailable)
        }
    }

    pub fn dimension(&self) -> usize {
        #[cfg(feature = "asr-sherpa")]
        {
            self.extractor.embedding_size
        }
        #[cfg(not(feature = "asr-sherpa"))]
        {
            0
        }
    }

    /// Unit-length embedding of 16 kHz mono samples.
    pub fn embed(&mut self, samples: &[f32]) -> Result<Vec<f32>, SpeakerError> {
        if samples.len() < MIN_SAMPLES {
            return Err(SpeakerError::TooShort);
        }
        #[cfg(feature = "asr-sherpa")]
        {
            let mut embedding = self
                .extractor
                .compute_speaker_embedding(judged_window(samples).to_vec(), SAMPLE_RATE)
                .map_err(|error| SpeakerError::Embedding(error.to_string()))?;
            normalize(&mut embedding);
            Ok(embedding)
        }
        #[cfg(not(feature = "asr-sherpa"))]
        {
            Err(SpeakerError::EngineUnavailable)
        }
    }
}

/// The middle `MAX_SAMPLES` of a long clip; shorter clips are used whole.
pub fn judged_window(samples: &[f32]) -> &[f32] {
    let start = samples.len().saturating_sub(MAX_SAMPLES) / 2;
    &samples[start..samples.len().min(start + MAX_SAMPLES)]
}

/// The speaker model lives in its own folder under the model directory: the
/// model directory treats every top-level `.bin` as a Whisper model.
pub fn speaker_model_store(model_directory: &Path) -> ModelStore {
    ModelStore::new(model_directory.join("speaker"))
}

pub fn speaker_model_path(model_directory: &Path) -> PathBuf {
    speaker_model_store(model_directory).model_path(SPEAKER_MODEL_ID)
}

/// Download descriptor for the speaker model. It is never part of the
/// transcription model registry, so it cannot be picked for transcription.
pub fn speaker_model_descriptor() -> ModelDescriptor {
    ModelDescriptor {
        id: SPEAKER_MODEL_ID.to_string(),
        display_name: "Voice match (ERes2NetV2)".to_string(),
        engine: "speaker-embedding".to_string(),
        provider_runtime: SPEAKER_MODEL_RUNTIME.to_string(),
        download_url: Some(SPEAKER_MODEL_URL.to_string()),
        checksum_sha256: Some(SPEAKER_MODEL_SHA256.to_string()),
        size_mb: SPEAKER_MODEL_SIZE_MB,
        languages: vec!["ko".to_string(), "en".to_string()],
        speed_score: 0,
        accuracy_score: 0,
        offline: true,
        status: ModelStatus::Missing,
        download_progress: None,
        download_error: None,
        capabilities: ModelCapabilities::default(),
    }
}

/// Voice profile and the enrollment recordings it keeps, under the app data folder.
pub struct VoiceProfileStore {
    root: PathBuf,
}

impl VoiceProfileStore {
    pub fn new(app_data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: app_data_dir.as_ref().join(PROFILE_DIRECTORY),
        }
    }

    pub fn profile_path(&self) -> PathBuf {
        self.root.join(PROFILE_FILE_NAME)
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.root.join(RECORDINGS_DIRECTORY)
    }

    /// Path for an enrollment recording. Only plain file names are accepted.
    pub fn recording_path(&self, file_name: &str) -> Result<PathBuf, SpeakerError> {
        let plain = Path::new(file_name)
            .file_name()
            .is_some_and(|name| name == file_name);
        if !plain || file_name.starts_with('.') {
            return Err(SpeakerError::InvalidRecordingName(file_name.to_string()));
        }
        Ok(self.recordings_dir().join(file_name))
    }

    pub fn load(&self) -> Result<Option<VoiceProfile>, SpeakerError> {
        let path = self.profile_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| SpeakerError::Profile(error.to_string()))
    }

    pub fn save(&self, profile: &VoiceProfile) -> Result<(), SpeakerError> {
        fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec_pretty(profile)
            .map_err(|error| SpeakerError::Profile(error.to_string()))?;
        let partial = self.root.join(format!("{PROFILE_FILE_NAME}.partial"));
        fs::write(&partial, bytes)?;
        fs::rename(&partial, self.profile_path())?;
        Ok(())
    }

    /// Remove the profile and every enrollment recording.
    pub fn reset(&self) -> Result<(), SpeakerError> {
        match fs::remove_dir_all(&self.root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => Ok(result?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_790_000_000 + seconds, 0).unwrap()
    }

    fn sample(origin: VoiceSampleOrigin, embedding: Vec<f32>) -> VoiceSample {
        VoiceSample {
            origin,
            language: Some("ko".to_string()),
            prompt: None,
            recording_file: None,
            source_audio_path: None,
            embedding,
            added_at: at(0),
        }
    }

    fn enrolled(embeddings: &[Vec<f32>]) -> VoiceProfile {
        let mut profile = VoiceProfile::new(SPEAKER_MODEL_ID, at(0));
        for embedding in embeddings {
            profile
                .add_sample(
                    sample(VoiceSampleOrigin::Enrollment, embedding.clone()),
                    at(1),
                )
                .unwrap();
        }
        profile
    }

    #[test]
    fn cosine_similarity_handles_direction_and_zero_vectors() {
        assert!((cosine_similarity(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 3.0]).abs() < 1e-6);
        assert!((cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }

    #[test]
    fn long_clips_are_judged_on_their_middle_window() {
        let short = vec![0.0; MIN_SAMPLES];
        assert_eq!(judged_window(&short).len(), MIN_SAMPLES);

        let long = (0..MAX_SAMPLES + 4)
            .map(|index| index as f32)
            .collect::<Vec<_>>();
        let window = judged_window(&long);
        assert_eq!(window.len(), MAX_SAMPLES);
        assert_eq!(window[0], 2.0);
        assert_eq!(window[MAX_SAMPLES - 1], (MAX_SAMPLES + 1) as f32);
    }

    #[test]
    fn centroid_is_the_unit_mean_of_normalized_samples() {
        let profile = enrolled(&[vec![3.0, 0.0], vec![0.0, 0.5]]);
        let centroid = profile.centroid().unwrap();
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!((centroid[0] - half).abs() < 1e-6, "{centroid:?}");
        assert!((centroid[1] - half).abs() < 1e-6, "{centroid:?}");
    }

    #[test]
    fn judge_marks_scores_at_or_above_the_threshold_as_me() {
        let mut profile = enrolled(&[vec![1.0, 0.0]]);
        profile.set_threshold(0.5, at(2));

        let me = judge_embedding(&profile, &[0.5, 0.5]);
        assert_eq!(me.kind, SpeakerKind::Me);
        assert!((me.score.unwrap() - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert_eq!(me.profile_revision, Some(profile.revision));

        let other = judge_embedding(&profile, &[0.1, 1.0]);
        assert_eq!(other.kind, SpeakerKind::Other);
        assert_eq!(other.reason, None);
    }

    #[test]
    fn every_change_bumps_the_profile_revision() {
        let mut profile = enrolled(&[vec![1.0, 0.0]]);
        assert_eq!(profile.revision, 1);
        profile
            .add_sample(sample(VoiceSampleOrigin::Correction, vec![0.0, 1.0]), at(3))
            .unwrap();
        profile.set_threshold(0.3, at(4));
        assert_eq!(profile.revision, 3);
        assert_eq!(profile.updated_at, at(4));
    }

    #[test]
    fn samples_from_another_model_dimension_are_rejected() {
        let mut profile = enrolled(&[vec![1.0, 0.0]]);
        let error = profile
            .add_sample(
                sample(VoiceSampleOrigin::Correction, vec![1.0, 0.0, 0.0]),
                at(2),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            SpeakerError::DimensionMismatch {
                expected: 2,
                actual: 3
            }
        ));
        assert_eq!(
            judge_embedding(&profile, &[1.0, 0.0, 0.0]).reason,
            Some(UnknownReason::ProfileModelMismatch)
        );
    }

    #[test]
    fn classify_returns_unknown_with_a_reason_when_it_cannot_judge() {
        let samples = vec![0.1; MIN_SAMPLES];
        assert_eq!(
            classify_samples(None, None, &samples).reason,
            Some(UnknownReason::NotEnrolled)
        );

        let mut corrections_only = VoiceProfile::new(SPEAKER_MODEL_ID, at(0));
        corrections_only
            .add_sample(sample(VoiceSampleOrigin::Correction, vec![1.0, 0.0]), at(1))
            .unwrap();
        assert_eq!(
            classify_samples(None, Some(&corrections_only), &samples).reason,
            Some(UnknownReason::NotEnrolled)
        );

        let mut other_model = enrolled(&[vec![1.0, 0.0]]);
        other_model.model_id = "another-model".to_string();
        assert_eq!(
            classify_samples(None, Some(&other_model), &samples).reason,
            Some(UnknownReason::ProfileModelMismatch)
        );

        let profile = enrolled(&[vec![1.0, 0.0]]);
        let verdict = classify_samples(None, Some(&profile), &samples);
        assert_eq!(verdict.kind, SpeakerKind::Unknown);
        assert_eq!(verdict.reason, Some(UnknownReason::ModelMissing));
    }

    #[test]
    fn missing_model_file_is_reported_before_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = speaker_model_path(dir.path());
        assert!(matches!(
            SpeakerEmbedder::load(&path),
            Err(SpeakerError::ModelMissing(missing)) if missing == path
        ));
    }

    #[test]
    fn undecodable_audio_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.wav");
        fs::write(&path, b"not audio").unwrap();
        let profile = enrolled(&[vec![1.0, 0.0]]);
        assert_eq!(
            classify_audio_file(None, Some(&profile), &path).reason,
            Some(UnknownReason::DecodeFailed)
        );
    }

    #[test]
    fn verdict_serializes_kind_and_reason_in_snake_case() {
        let json = serde_json::to_value(SpeakerVerdict::unknown(UnknownReason::TooShort)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"kind": "unknown", "reason": "too_short"})
        );
    }

    #[test]
    fn profile_store_saves_loads_and_resets_with_recordings() {
        let dir = tempfile::tempdir().unwrap();
        let store = VoiceProfileStore::new(dir.path());
        assert!(store.load().unwrap().is_none());

        let mut profile = VoiceProfile::new(SPEAKER_MODEL_ID, at(0));
        let mut enrollment = sample(VoiceSampleOrigin::Enrollment, vec![0.6, 0.8]);
        enrollment.prompt = Some("오늘 회의는 세 시에 시작합니다.".to_string());
        enrollment.recording_file = Some("ko-1.wav".to_string());
        profile.add_sample(enrollment, at(1)).unwrap();
        store.save(&profile).unwrap();

        let recording = store.recording_path("ko-1.wav").unwrap();
        fs::create_dir_all(recording.parent().unwrap()).unwrap();
        fs::write(&recording, b"RIFF").unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded, profile);
        assert_eq!(loaded.recording_files().collect::<Vec<_>>(), ["ko-1.wav"]);

        store.reset().unwrap();
        assert!(store.load().unwrap().is_none());
        assert!(!recording.exists());
        store.reset().unwrap();
    }

    #[test]
    fn recording_names_cannot_escape_the_recordings_folder() {
        let store = VoiceProfileStore::new("/tmp/app");
        for name in [
            "../profile.json",
            "nested/ko-1.wav",
            "",
            ".hidden.wav",
            "/abs.wav",
        ] {
            assert!(store.recording_path(name).is_err(), "{name}");
        }
        assert!(store.recording_path("en-2.wav").is_ok());
    }

    #[test]
    fn speaker_model_downloads_into_its_own_folder_and_stays_out_of_transcription_models() {
        let dir = tempfile::tempdir().unwrap();
        let descriptor = speaker_model_descriptor();
        assert!(!crate::models::default_model_registry().contains_key(SPEAKER_MODEL_ID));
        assert_eq!(descriptor.capabilities, ModelCapabilities::default());
        assert_eq!(
            descriptor.checksum_sha256.as_deref().map(str::len),
            Some(64)
        );

        let store = speaker_model_store(dir.path());
        assert_eq!(
            store.verify_model(&descriptor).unwrap(),
            ModelStatus::Missing
        );
        let unpinned = ModelDescriptor {
            checksum_sha256: None,
            ..descriptor.clone()
        };
        store.install_model_bytes(&unpinned, b"onnx").unwrap();
        assert!(speaker_model_path(dir.path()).is_file());
        assert_eq!(store.verify_model(&descriptor).unwrap(), ModelStatus::Error);

        let registry = ModelStore::new(dir.path()).load_model_registry().unwrap();
        assert!(!registry.contains_key(SPEAKER_MODEL_ID));
    }

    /// Needs the speaker model and sherpa-onnx's sample clips:
    /// WAKENOTE_SPEAKER_MODEL=<model.onnx> WAKENOTE_SPEAKER_CLIPS=<dir> \
    /// cargo test --features asr-sherpa speaker -- --ignored
    #[cfg(feature = "asr-sherpa")]
    #[test]
    #[ignore = "needs the speaker model and sample clips"]
    fn enrolled_speaker_is_me_and_other_speakers_are_other() {
        let model = PathBuf::from(std::env::var("WAKENOTE_SPEAKER_MODEL").unwrap());
        let clips = PathBuf::from(std::env::var("WAKENOTE_SPEAKER_CLIPS").unwrap());
        let mut embedder = SpeakerEmbedder::load(&model).unwrap();
        let mut profile = VoiceProfile::new(SPEAKER_MODEL_ID, at(0));
        for name in ["fangjun-sr-1.wav", "fangjun-sr-2.wav"] {
            let samples =
                crate::transcription::decode_audio_for_whisper(&clips.join(name)).unwrap();
            let embedding = embedder.embed(&samples).unwrap();
            profile
                .add_sample(sample(VoiceSampleOrigin::Enrollment, embedding), at(1))
                .unwrap();
        }

        let verdict = |name: &str, embedder: &mut SpeakerEmbedder| {
            classify_audio_file(Some(embedder), Some(&profile), &clips.join(name))
        };
        assert_eq!(
            verdict("fangjun-test-sr-1.wav", &mut embedder).kind,
            SpeakerKind::Me
        );
        assert_eq!(
            verdict("leijun-test-sr-1.wav", &mut embedder).kind,
            SpeakerKind::Other
        );
        assert_eq!(
            verdict("liudehua-test-sr-1.wav", &mut embedder).kind,
            SpeakerKind::Other
        );
        assert_eq!(
            classify_samples(Some(&mut embedder), Some(&profile), &[0.0; MIN_SAMPLES - 1]).reason,
            Some(UnknownReason::TooShort)
        );
    }

    #[test]
    fn speaker_model_checksum_mismatch_is_rejected_on_install() {
        let dir = tempfile::tempdir().unwrap();
        let store = speaker_model_store(dir.path());
        assert!(
            store
                .install_model_bytes(&speaker_model_descriptor(), b"onnx")
                .is_err()
        );
        assert!(!speaker_model_path(dir.path()).exists());
    }
}
