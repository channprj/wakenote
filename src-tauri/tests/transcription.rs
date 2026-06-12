use std::path::PathBuf;

use wakenote::queue::{QueueJobStatus, TranscriptionQueue};
use wakenote::recorder::{ChunkMetadata, ChunkSource, TranscriptionStatus};
use wakenote::settings::TranscriptionLanguage;
use wakenote::transcription::{
    DecodedSegmentQuality, RuntimeTranscriber, Transcriber, TranscriptionError,
    TranscriptionJobOutcome, TranscriptionRequest, TranscriptionWorker, TranscriptionWorkerOptions,
    WhisperTranscriber, apply_outcome, decode_audio_for_whisper,
    default_whisper_context_parameters, model_supports_live_partials, should_skip_low_signal_audio,
    should_suppress_low_confidence_decode, should_suppress_transcript_artifact,
};

#[derive(Clone)]
struct StaticTranscriber {
    result: Result<String, String>,
    expected_language: TranscriptionLanguage,
}

impl StaticTranscriber {
    fn success(transcript: &str) -> Self {
        Self {
            result: Ok(transcript.to_string()),
            expected_language: TranscriptionLanguage::Auto,
        }
    }

    fn failure(error: &str) -> Self {
        Self {
            result: Err(error.to_string()),
            expected_language: TranscriptionLanguage::Auto,
        }
    }

    fn expecting_language(mut self, expected_language: TranscriptionLanguage) -> Self {
        self.expected_language = expected_language;
        self
    }
}

impl Transcriber for StaticTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        assert_eq!(request.model_id, "whisper-medium");
        assert_eq!(request.language, self.expected_language);
        assert!(request.audio_path.exists());
        self.result.clone().map_err(TranscriptionError::Engine)
    }
}

#[test]
fn transcription_worker_writes_txt_and_marks_job_completed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230709.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::new(StaticTranscriber::success("안녕하세요 hello"));

    let processed = worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");

    assert_eq!(processed, id);
    let job = queue.job(id).expect("job");
    assert_eq!(job.status, QueueJobStatus::Completed);
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("transcript"),
        "안녕하세요 hello\n"
    );
    assert!(!audio_path.with_extension("error.txt").exists());
}

#[test]
fn transcription_worker_suppresses_bracketed_artifact_transcripts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230710.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::with_options(
        StaticTranscriber::success("[감사합니다]").expecting_language(TranscriptionLanguage::Ko),
        TranscriptionWorkerOptions {
            language: TranscriptionLanguage::Ko,
            suppress_low_confidence_transcripts: true,
        },
    );

    worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");

    assert_eq!(
        queue.job(id).expect("job").status,
        QueueJobStatus::Completed
    );
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt"))
            .expect("suppressed transcript")
            .trim(),
        ""
    );
}

#[test]
fn transcription_worker_can_keep_bracketed_text_when_suppression_is_disabled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230711.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::with_options(
        StaticTranscriber::success("[감사합니다]").expecting_language(TranscriptionLanguage::Ko),
        TranscriptionWorkerOptions {
            language: TranscriptionLanguage::Ko,
            suppress_low_confidence_transcripts: false,
        },
    );

    worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");

    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("transcript"),
        "[감사합니다]\n"
    );
}

#[test]
fn transcription_worker_keeps_system_audio_text_when_suppression_is_enabled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230712-spotify.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let now = chrono::Utc::now();
    let metadata = ChunkMetadata {
        model_id: "whisper-medium".into(),
        device_id: "custom-source-2".into(),
        device_name: "Spotify".into(),
        sample_rate: 16_000,
        threshold_dbfs: -42.0,
        attack_ms: 100,
        release_ms: 1_000,
        pre_roll_ms: 1_000,
        lead_in_padding_ms: 300,
        post_roll_ms: 300,
        min_chunk_ms: 600,
        max_chunk_ms: 120_000,
        started_at: now,
        ended_at: now,
        duration_ms: 1000,
        transcription_status: TranscriptionStatus::Queued,
        app_version: "0.0.0".into(),
        used_fallback_device: false,
        live_capture_chunk_id: None,
        source: ChunkSource::System,
        source_label: Some("Spotify".into()),
    };
    std::fs::write(
        audio_path.with_extension("json"),
        serde_json::to_vec_pretty(&metadata).expect("metadata json"),
    )
    .expect("metadata");
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::with_options(
        StaticTranscriber::success("[감사합니다]").expecting_language(TranscriptionLanguage::Ko),
        TranscriptionWorkerOptions {
            language: TranscriptionLanguage::Ko,
            suppress_low_confidence_transcripts: true,
        },
    );

    worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");

    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("transcript"),
        "[감사합니다]\n"
    );
}

#[test]
fn transcript_artifact_filter_preserves_plain_speech() {
    assert!(should_suppress_transcript_artifact("(웃음)"));
    assert!(should_suppress_transcript_artifact("[감사합니다]"));
    assert!(!should_suppress_transcript_artifact("감사합니다"));
    assert!(!should_suppress_transcript_artifact(
        "오늘 회의 내용을 정리하겠습니다."
    ));
}

#[test]
fn transcript_artifact_filter_suppresses_broadcast_news_signoffs() {
    assert!(should_suppress_transcript_artifact(
        "MBC 뉴스 김수근입니다."
    ));
    assert!(should_suppress_transcript_artifact("KBS 뉴스 이지연입니다"));
    assert!(!should_suppress_transcript_artifact(
        "오늘 MBC 뉴스 사례를 회의에서 검토했습니다."
    ));
}

#[test]
fn low_signal_audio_gate_skips_quiet_or_too_short_chunks() {
    let quiet_one_second = vec![0.0002_f32; 16_000];
    let short_voice = vec![0.08_f32; 4_000];
    let clear_voice = vec![0.08_f32; 16_000];

    assert!(should_skip_low_signal_audio(&quiet_one_second));
    assert!(should_skip_low_signal_audio(&short_voice));
    assert!(!should_skip_low_signal_audio(&clear_voice));
}

#[test]
fn low_confidence_decode_gate_suppresses_no_speech_or_low_token_probability() {
    let high_no_speech = [DecodedSegmentQuality {
        no_speech_probability: 0.92,
        average_token_probability: 0.74,
    }];
    let low_token_probability = [DecodedSegmentQuality {
        no_speech_probability: 0.12,
        average_token_probability: 0.12,
    }];
    let confident_speech = [DecodedSegmentQuality {
        no_speech_probability: 0.08,
        average_token_probability: 0.71,
    }];

    assert!(should_suppress_low_confidence_decode(
        "MBC 뉴스 김수근입니다.",
        &high_no_speech
    ));
    assert!(should_suppress_low_confidence_decode(
        "MBC 뉴스 김수근입니다.",
        &low_token_probability
    ));
    assert!(!should_suppress_low_confidence_decode(
        "오늘 회의 내용을 정리하겠습니다.",
        &confident_speech
    ));
}

#[test]
fn transcription_worker_passes_configured_language_to_transcriber() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230812.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::with_language(
        StaticTranscriber::success("안녕하세요").expecting_language(TranscriptionLanguage::Ko),
        TranscriptionLanguage::Ko,
    );

    worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");
}

#[test]
fn transcription_worker_writes_error_and_marks_job_failed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230810.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::new(StaticTranscriber::failure("model checksum mismatch"));

    let processed = worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");

    assert_eq!(processed, id);
    let job = queue.job(id).expect("job");
    assert_eq!(job.status, QueueJobStatus::Failed);
    assert_eq!(job.error.as_deref(), Some("model checksum mismatch"));
    assert!(audio_path.exists());
    assert!(!audio_path.with_extension("txt").exists());
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("error.txt")).expect("error sidecar"),
        "model checksum mismatch\n"
    );
}

#[test]
fn transcription_worker_marks_job_failed_when_sidecar_write_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("231010.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    std::fs::create_dir(audio_path.with_extension("txt")).expect("block transcript sidecar");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::new(StaticTranscriber::success("transcript"));

    let processed = worker
        .process_next(&mut queue)
        .expect("sidecar write failure should be captured as a failed job")
        .expect("processed job");

    assert_eq!(processed, id);
    let job = queue.job(id).expect("job");
    assert_eq!(job.status, QueueJobStatus::Failed);
    assert!(
        job.error
            .as_deref()
            .unwrap_or("")
            .contains("recorder error")
    );
}

#[test]
fn cancelled_transcription_job_ignores_late_worker_outcome() {
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file("/recordings/20260506/230911.wav", "whisper-medium");
    queue.start_next().expect("start job");
    queue
        .cancel_current("cancelled by user")
        .expect("cancel running job");

    apply_outcome(&mut queue, TranscriptionJobOutcome::completed(id)).expect("late outcome");

    let job = queue.job(id).expect("job");
    assert_eq!(job.status, QueueJobStatus::Cancelled);
    assert_eq!(job.error.as_deref(), Some("cancelled by user"));
}

#[test]
fn whisper_transcriber_reports_missing_model_before_running_inference() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("sample.wav");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let transcriber = WhisperTranscriber::new(tmp.path().join("models"));

    let error = transcriber
        .transcribe(TranscriptionRequest {
            audio_path: &audio_path,
            model_id: "whisper-medium",
            language: TranscriptionLanguage::Auto,
        })
        .expect_err("missing model should fail");

    assert_eq!(
        error,
        TranscriptionError::ModelMissing(PathBuf::from(
            tmp.path().join("models/whisper-medium.bin")
        ))
    );
}

#[test]
fn whisper_transcriber_expands_tilde_model_directory() {
    let home = std::env::var_os("HOME").expect("HOME should be set");
    let audio_path = PathBuf::from("/tmp/wakenote-unused-audio.wav");
    let transcriber = WhisperTranscriber::new("~/Library/Application Support/WakeNote/models");

    let error = transcriber
        .transcribe(TranscriptionRequest {
            audio_path: &audio_path,
            model_id: "missing-model-for-tilde-expansion",
            language: TranscriptionLanguage::Auto,
        })
        .expect_err("missing model should fail");

    assert_eq!(
        error,
        TranscriptionError::ModelMissing(
            PathBuf::from(home)
                .join("Library/Application Support/WakeNote/models")
                .join("missing-model-for-tilde-expansion.bin")
        )
    );
}

#[test]
fn runtime_transcriber_runs_external_command_models_with_audio_environment() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let model_directory = tmp.path().join("models");
    std::fs::create_dir_all(&model_directory).expect("model dir");
    std::fs::write(
        model_directory.join("model-registry.json"),
        r#"[
          {
            "id": "parakeet-tdt-0.6b-v3",
            "display_name": "Parakeet TDT 0.6B V3",
            "engine": "NVIDIA Parakeet",
            "provider_runtime": "external-command",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 1200,
            "languages": ["multi"],
            "speed_score": 8,
            "accuracy_score": 8,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");
    std::fs::write(
        model_directory.join("parakeet-tdt-0.6b-v3.command"),
        "printf '%s:%s' \"$WAKENOTE_MODEL_ID\" \"$WAKENOTE_AUDIO_PATH\"",
    )
    .expect("command file");
    let audio_path = tmp.path().join("sample.wav");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");

    let transcriber = RuntimeTranscriber::new(&model_directory);
    let transcript = transcriber
        .transcribe(TranscriptionRequest {
            audio_path: &audio_path,
            model_id: "parakeet-tdt-0.6b-v3",
            language: TranscriptionLanguage::Auto,
        })
        .expect("external command transcript");

    assert_eq!(
        transcript,
        format!("parakeet-tdt-0.6b-v3:{}", audio_path.display())
    );
}

#[test]
fn runtime_transcriber_routes_cohere_models_to_api_runtime() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let transcriber = RuntimeTranscriber::with_cohere_api_key(tmp.path(), None);
    let audio_path = tmp.path().join("sample.wav");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");

    let error = transcriber
        .transcribe(TranscriptionRequest {
            audio_path: &audio_path,
            model_id: "cohere-transcribe-03-2026",
            language: TranscriptionLanguage::Ko,
        })
        .expect_err("missing API key should fail before whisper path");

    assert_eq!(
        error,
        TranscriptionError::Engine("COHERE_API_KEY is required for Cohere Transcribe".to_string())
    );
}

#[test]
fn live_partial_support_is_limited_to_whisper_runtimes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("model-registry.json"),
        r#"[
          {
            "id": "whisper-medium",
            "display_name": "Whisper Medium",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["ko", "en", "multi"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          },
          {
            "id": "sensevoice-small",
            "display_name": "SenseVoice Small",
            "engine": "SenseVoice",
            "provider_runtime": "external-command",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 1000,
            "languages": ["ko", "en", "multi"],
            "speed_score": 9,
            "accuracy_score": 8,
            "offline": true
          }
        ]"#,
    )
    .expect("registry");

    assert!(model_supports_live_partials(tmp.path(), "whisper-medium"));
    assert!(!model_supports_live_partials(
        tmp.path(),
        "sensevoice-small"
    ));
}

#[test]
fn whisper_context_parameters_prefer_gpu_on_macos_builds() {
    let params = default_whisper_context_parameters();

    #[cfg(target_os = "macos")]
    assert!(
        params.use_gpu,
        "macOS whisper builds should use Metal GPU acceleration"
    );

    #[cfg(not(target_os = "macos"))]
    assert!(
        !params.use_gpu,
        "non-macOS builds should stay CPU-only unless a platform GPU backend is compiled"
    );

    assert_eq!(params.gpu_device, 0);
}

#[test]
fn m4a_audio_is_decoded_through_native_bridge_for_whisper() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let wav_path = tmp.path().join("source.wav");
    let m4a_path = tmp.path().join("source.m4a");
    write_test_wav(&wav_path);
    let output = std::process::Command::new("/usr/bin/afconvert")
        .arg("-f")
        .arg("m4af")
        .arg("-d")
        .arg("aac")
        .arg(&wav_path)
        .arg(&m4a_path)
        .output()
        .expect("afconvert");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let decoded = decode_audio_for_whisper(&m4a_path).expect("decode m4a");

    assert!(!decoded.is_empty());
}

#[test]
fn mp3_audio_is_decoded_through_native_bridge_for_whisper() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let wav_path = tmp.path().join("source.wav");
    let mp3_path = tmp.path().join("source.mp3");
    write_test_wav(&wav_path);
    let output = std::process::Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(&wav_path)
        .args(["-acodec", "libmp3lame", "-b:a", "64k"])
        .arg(&mp3_path)
        .output()
        .expect("ffmpeg");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let decoded = decode_audio_for_whisper(&mp3_path).expect("decode mp3");

    assert!(!decoded.is_empty());
}

fn write_test_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("wav writer");
    for index in 0..16_000 {
        let sample = if index % 2 == 0 { 1200_i16 } else { -1200_i16 };
        writer.write_sample(sample).expect("sample");
    }
    writer.finalize().expect("finalize");
}
