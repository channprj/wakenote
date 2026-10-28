use std::path::PathBuf;

use wakenote::cloud_transcription::FailureCategory;
use wakenote::dictionary::DictionaryContext;
use wakenote::queue::{QueueIssueCode, QueueIssueSeverity, QueueJobStatus, TranscriptionQueue};
use wakenote::recorder::{ChunkMetadata, ChunkSource, TranscriptionStatus, encode_wav_to_m4a};
use wakenote::settings::{DictionaryEntry, TranscriptionLanguage};
use wakenote::transcription::{
    DecodedSegmentQuality, RuntimeTranscriber, Transcriber, TranscriptArtifactReason,
    TranscriptionError, TranscriptionJobOutcome, TranscriptionRequest, TranscriptionWorker,
    TranscriptionWorkerOptions, WhisperTranscriber, apply_outcome, apply_outcome_at,
    decode_audio_for_whisper, default_whisper_context_parameters, model_supports_live_partials,
    should_skip_low_signal_audio, should_suppress_low_confidence_decode,
    should_suppress_transcript_artifact, transcript_artifact_reason,
};

use chrono::TimeZone;

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
fn finalized_live_text_is_available_before_saving_and_survives_sidecar_failure() {
    use std::cell::Cell;
    let tmp = tempfile::tempdir().unwrap();
    let audio_path = tmp.path().join("live.wav");
    std::fs::write(&audio_path, b"audio").unwrap();
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file(&audio_path, "whisper-medium");
    let job = queue.start_next().unwrap();
    let worker = TranscriptionWorker::new(StaticTranscriber::success("한글 🙂\nnext line"));
    // A directory at the text sidecar path forces the subsequent save to fail.
    std::fs::create_dir(audio_path.with_extension("txt")).unwrap();
    let called = Cell::new(false);
    let result = worker.process_started_job_with_final(&job, |execution| {
        called.set(true);
        assert_eq!(execution.text, "한글 🙂\nnext line");
        assert!(!audio_path.with_extension("json").exists());
    });
    assert!(called.get());
    assert!(
        result.is_err(),
        "the existing save error remains visible independently of delivery"
    );
}

#[test]
fn finalized_callback_runs_once_with_dictionary_corrections_and_never_for_empty_or_failed_results()
{
    use std::cell::Cell;
    let tmp = tempfile::tempdir().unwrap();
    let audio_path = tmp.path().join("live.wav");
    std::fs::write(&audio_path, b"audio").unwrap();
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file(&audio_path, "whisper-medium");
    let job = queue.start_next().unwrap();
    for transcriber in [
        StaticTranscriber::success(" \n\t"),
        StaticTranscriber::failure("provider failure"),
    ] {
        let worker = TranscriptionWorker::new(transcriber);
        worker
            .process_started_job_with_final(&job, |_| panic!("no finalized transcript"))
            .unwrap();
    }
    let worker = TranscriptionWorker::with_options_and_dictionary(
        StaticTranscriber::success("wake note is ready"),
        TranscriptionWorkerOptions {
            language: TranscriptionLanguage::Auto,
            suppress_low_confidence_transcripts: false,
        },
        DictionaryContext::compile(
            true,
            &[DictionaryEntry {
                id: "wake".into(),
                term: "WakeNote".into(),
                aliases: vec!["wake note".into()],
                enabled: true,
            }],
        ),
    );
    let count = Cell::new(0);
    worker
        .process_started_job_with_final(&job, |execution| {
            count.set(count.get() + 1);
            assert_eq!(execution.text, "WakeNote is ready");
            assert!(!audio_path.with_extension("txt").exists());
        })
        .unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).unwrap(),
        "WakeNote is ready\n"
    );
}

#[test]
fn transcription_worker_discards_empty_output_from_activity_without_blank_txt() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230708.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let now = chrono::Utc::now();
    let metadata = ChunkMetadata {
        model_id: "whisper-medium".into(),
        requested_model_id: None,
        effective_model_id: None,
        fallback_from_model_id: None,
        device_id: "default".into(),
        device_name: "System Default".into(),
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
        duration_ms: 1_000,
        transcription_status: TranscriptionStatus::Queued,
        transcribed_at: None,
        transcript_text: None,
        app_version: "0.0.0".into(),
        used_fallback_device: false,
        live_capture_chunk_id: Some(1),
        source: ChunkSource::Microphone,
        source_label: None,
        microphone_slot: None,
        microphone_inputs: Vec::new(),
    };
    std::fs::write(
        audio_path.with_extension("json"),
        serde_json::to_vec_pretty(&metadata).expect("metadata json"),
    )
    .expect("metadata");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "whisper-medium");
    let worker = TranscriptionWorker::new(StaticTranscriber::success(" \n\t"));

    let processed = worker
        .process_next(&mut queue)
        .expect("process")
        .expect("processed job");

    assert_eq!(processed, id);
    assert_eq!(queue.job(id), None);
    assert!(!audio_path.with_extension("txt").exists());
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("error.txt")).expect("error sidecar"),
        "No speech detected\n"
    );
    let stored: ChunkMetadata = serde_json::from_slice(
        &std::fs::read(audio_path.with_extension("json")).expect("stored metadata"),
    )
    .expect("stored metadata json");
    assert_eq!(stored.transcription_status, TranscriptionStatus::Failed);
    assert_eq!(stored.transcript_text, None);
}

#[test]
fn transcription_worker_preserves_bracketed_artifact_transcripts_in_txt() {
    let artifact_warning = Some((
        QueueIssueSeverity::Warning,
        QueueIssueCode::TranscriptArtifact,
    ));
    for (label, transcript, suppress, system_audio, expected_issue) in [
        (
            "bracketed artifact",
            "[감사합니다]".to_string(),
            true,
            false,
            artifact_warning,
        ),
        (
            "degenerate repetition",
            format!("{} MBC 뉴스 김수근입니다.", "[끝] ".repeat(6)),
            true,
            false,
            artifact_warning,
        ),
        (
            "suppression disabled",
            "[감사합니다]".to_string(),
            false,
            false,
            None,
        ),
        (
            "system audio with suppression enabled",
            "[감사합니다]".to_string(),
            true,
            true,
            None,
        ),
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let audio_path = tmp.path().join("20260506").join("230710.wav");
        std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
        std::fs::write(&audio_path, b"wav bytes").expect("audio");
        if system_audio {
            let now = chrono::Utc::now();
            let metadata = ChunkMetadata {
                model_id: "whisper-medium".into(),
                requested_model_id: None,
                effective_model_id: None,
                fallback_from_model_id: None,
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
                transcribed_at: None,
                transcript_text: None,
                app_version: "0.0.0".into(),
                used_fallback_device: false,
                live_capture_chunk_id: None,
                source: ChunkSource::System,
                source_label: Some("Spotify".into()),
                microphone_slot: None,
                microphone_inputs: Vec::new(),
            };
            std::fs::write(
                audio_path.with_extension("json"),
                serde_json::to_vec_pretty(&metadata).expect("metadata json"),
            )
            .expect("metadata");
        }
        let mut queue = TranscriptionQueue::new();
        let id = queue.enqueue_file(&audio_path, "whisper-medium");
        let worker = TranscriptionWorker::with_options(
            StaticTranscriber::success(&transcript).expecting_language(TranscriptionLanguage::Ko),
            TranscriptionWorkerOptions {
                language: TranscriptionLanguage::Ko,
                suppress_low_confidence_transcripts: suppress,
            },
        );

        worker
            .process_next(&mut queue)
            .expect("process")
            .expect("processed job");

        let job = queue.job(id).expect("job");
        assert_eq!(job.status, QueueJobStatus::Completed, "{label}");
        assert_eq!(
            job.issue.as_ref().map(|issue| (issue.severity, issue.code)),
            expected_issue,
            "{label}"
        );
        assert_eq!(
            std::fs::read_to_string(audio_path.with_extension("txt"))
                .expect("preserved transcript"),
            format!("{transcript}\n"),
            "{label}"
        );
    }
}

#[test]
fn transcript_artifact_filter_suppresses_artifacts_and_preserves_speech() {
    for (label, transcript, expected_suppressed) in [
        ("parenthesized aside", "(웃음)".to_string(), true),
        ("bracketed thanks", "[감사합니다]".to_string(), true),
        ("plain thanks", "감사합니다".to_string(), false),
        (
            "plain meeting sentence",
            "오늘 회의 내용을 정리하겠습니다.".to_string(),
            false,
        ),
        (
            "MBC news signoff",
            "MBC 뉴스 김수근입니다.".to_string(),
            true,
        ),
        (
            "KBS news signoff",
            "KBS 뉴스 이지연입니다".to_string(),
            true,
        ),
        (
            "in-sentence news mention",
            "오늘 MBC 뉴스 사례를 회의에서 검토했습니다.".to_string(),
            false,
        ),
        (
            "repeated wrapped signoff",
            format!("{} MBC 뉴스 김수근입니다.", "[끝] ".repeat(6)),
            true,
        ),
        (
            "repeated sentence signoff",
            format!(
                "{} MBC 뉴스 김수근입니다.",
                "[오늘은 퇴근하기로 했어요] ".repeat(6)
            ),
            true,
        ),
        (
            "repeated wrapped phrase flood",
            "[머리가 너무 예뻐서] ".repeat(60),
            true,
        ),
        (
            "replacement character and token loop",
            format!("해� vor {}", "그리고 ".repeat(40)),
            true,
        ),
        (
            "repeated microphone test signoff",
            format!(
                "{} MBC 뉴스 김수근입니다.",
                "마이크 테스트 하나 둘 셋 ".repeat(6)
            ),
            true,
        ),
        (
            "natural repetition",
            "네 네, 확인했습니다.".to_string(),
            false,
        ),
        (
            "natural repetition with news mention",
            "아니 아니, 그게 아니라 오늘 MBC 뉴스 사례를 회의에서 검토했습니다.".to_string(),
            false,
        ),
        (
            "repeated product name",
            "회의는 Zoom Zoom 연결 상태를 확인한 뒤 시작하겠습니다.".to_string(),
            false,
        ),
        (
            "structured bracketed speech",
            "[1단계] 요구사항을 검토했고 [2단계] 구현을 마쳤으며 [3단계] 테스트를 실행하고 [4단계] 배포 여부를 확인합니다.".to_string(),
            false,
        ),
    ] {
        assert_eq!(
            should_suppress_transcript_artifact(&transcript),
            expected_suppressed,
            "{label}: {transcript}"
        );
    }
}

#[test]
fn transcript_artifact_filter_reports_stable_reasons() {
    use TranscriptArtifactReason::*;
    for (label, transcript, expected) in [
        ("wrapped phrase", "(웃음)".to_string(), Some(WrappedPhrase)),
        (
            "broadcast news signoff",
            "MBC 뉴스 김수근입니다.".to_string(),
            Some(BroadcastNewsSignoff),
        ),
        (
            "common hallucination",
            "thanks for watching".to_string(),
            Some(CommonHallucination),
        ),
        (
            "six repeated conjunctions",
            "그리고 ".repeat(6),
            Some(RepeatedTokenRun),
        ),
        (
            "five identical tokens",
            format!("{}하나 둘 셋 넷 다섯 여섯 일곱", "반복 ".repeat(5)),
            None,
        ),
        (
            "six identical tokens as a run",
            "반복 ".repeat(6),
            Some(RepeatedTokenRun),
        ),
        (
            "four repeated phrases",
            "하나 둘 셋 ".repeat(4),
            Some(DominantRepeatedNgram),
        ),
        (
            "three non-overlapping phrase occurrences",
            format!("{}오늘 회의 종료", "하나 둘 셋 ".repeat(3)),
            Some(DominantRepeatedNgram),
        ),
        (
            "three bracket groups",
            "말 [하나] [둘] [셋] 완료".to_string(),
            None,
        ),
        (
            "four bracket groups",
            "말 [하나] [둘] [셋] [넷] 완료".to_string(),
            Some(BracketFlood),
        ),
        (
            "nested bracket spans as one group",
            "([one] [two] [three] We discussed a sufficiently long ordinary project timeline and release plan in detail.)".to_string(),
            None,
        ),
        (
            "adjacent bracket groups counted separately",
            "말 [끝][끝][끝][끝] 완료".to_string(),
            Some(BracketFlood),
        ),
        (
            "replacement character with newsletter",
            "해� 뉴스레터 내용을 오늘 회의에서 검토했습니다.".to_string(),
            None,
        ),
        (
            "replacement character with news agency",
            "해� 연합뉴스 기사를 오늘 회의에서 검토했습니다.".to_string(),
            None,
        ),
        (
            "replacement character with broadcaster news signoff",
            "해� 오늘 MBC 뉴스 김수근입니다. 내용을 검토했습니다.".to_string(),
            Some(CompoundSignals),
        ),
        (
            "replacement character alone",
            "해� 오늘 회의 내용을 검토했습니다.".to_string(),
            None,
        ),
        (
            "scattered short token alone",
            "하 a b 하 c a 하 b c b a c".to_string(),
            None,
        ),
        (
            "replacement character with broadcaster news",
            "해� 오늘 MBC 뉴스 내용을 검토했습니다.".to_string(),
            Some(CompoundSignals),
        ),
        (
            "replacement character with scattered tokens",
            "해� a b 해 c a 해 b c b a c".to_string(),
            Some(CompoundSignals),
        ),
    ] {
        assert_eq!(
            transcript_artifact_reason(&transcript),
            expected,
            "{label}: {transcript}"
        );
    }
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
    assert_eq!(
        job.issue.as_ref().map(|issue| (issue.severity, issue.code)),
        Some((QueueIssueSeverity::Error, QueueIssueCode::Unknown)),
    );
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
fn applying_an_actionable_outcome_stamps_the_issue_at_the_terminal_boundary() {
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file("/recordings/artifact.wav", "whisper-medium");
    queue.start_next().expect("start job");
    let occurred_at = chrono::Utc.with_ymd_and_hms(2026, 8, 4, 1, 2, 3).unwrap();

    apply_outcome_at(
        &mut queue,
        TranscriptionJobOutcome::failed_with_issue(
            id,
            wakenote::queue::QueueJobIssue::error(QueueIssueCode::Provider, "Provider unavailable"),
        ),
        occurred_at,
    )
    .expect("apply warning");

    assert_eq!(
        queue
            .job(id)
            .expect("job")
            .issue
            .as_ref()
            .and_then(|issue| issue.occurred_at.as_deref()),
        Some("2026-08-04T01:02:03+00:00"),
    );
}

#[test]
fn whisper_transcriber_reports_missing_model_with_resolved_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // An existing but undecodable file pins the model check ahead of decoding.
    let invalid_audio_path = tmp.path().join("sample.wav");
    std::fs::write(&invalid_audio_path, b"wav bytes").expect("audio");
    let home = std::env::var_os("HOME").expect("HOME should be set");

    for (model_directory, audio_path, model_id, expected_path) in [
        (
            tmp.path().join("models"),
            invalid_audio_path,
            "whisper-medium",
            tmp.path().join("models/whisper-medium.bin"),
        ),
        (
            PathBuf::from("~/Library/Application Support/WakeNote/models"),
            PathBuf::from("/tmp/wakenote-unused-audio.wav"),
            "missing-model-for-tilde-expansion",
            PathBuf::from(home)
                .join("Library/Application Support/WakeNote/models")
                .join("missing-model-for-tilde-expansion.bin"),
        ),
    ] {
        let error = WhisperTranscriber::new(&model_directory)
            .transcribe(TranscriptionRequest {
                language_hints: &[],
                audio_path: &audio_path,
                model_id,
                language: TranscriptionLanguage::Auto,
                dictionary: &DictionaryContext::default(),
            })
            .expect_err("missing model should fail");

        assert_eq!(
            error,
            TranscriptionError::ModelMissing(expected_path),
            "{}",
            model_directory.display()
        );
    }
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
        "printf '%s|%s|%s|%s' \"$WAKENOTE_MODEL_ID\" \"$WAKENOTE_AUDIO_PATH\" \"$WAKENOTE_DICTIONARY_TERMS\" \"$WAKENOTE_DICTIONARY_JSON\"",
    )
    .expect("command file");
    let audio_path = tmp.path().join("sample.wav");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");

    let dictionary = DictionaryContext::compile(
        true,
        &[DictionaryEntry {
            id: "wake".into(),
            term: "WakeNote".into(),
            aliases: vec!["wake note".into()],
            enabled: true,
        }],
    );
    let transcriber = RuntimeTranscriber::new(&model_directory);
    let transcript = transcriber
        .transcribe(TranscriptionRequest {
            language_hints: &[],
            audio_path: &audio_path,
            model_id: "parakeet-tdt-0.6b-v3",
            language: TranscriptionLanguage::Auto,
            dictionary: &dictionary,
        })
        .expect("external command transcript");

    let fields = transcript.splitn(4, '|').collect::<Vec<_>>();
    assert_eq!(fields[0], "parakeet-tdt-0.6b-v3");
    assert_eq!(fields[1], audio_path.to_string_lossy());
    assert_eq!(fields[2], "WakeNote");
    assert!(fields[3].contains("wake note"));
}

#[test]
fn runtime_transcriber_routes_cloud_models_and_never_falls_back_without_keys() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing_audio = tmp.path().join("missing.wav");
    let transcriber = RuntimeTranscriber::new(tmp.path());

    for (model_id, provider) in [
        ("openrouter-qwen3-asr-flash", "OpenRouter"),
        ("openai-gpt-transcribe", "OpenAI"),
        ("soniox-async-v5", "Soniox"),
    ] {
        let error = transcriber
            .transcribe(TranscriptionRequest {
                language_hints: &[],
                audio_path: &missing_audio,
                model_id,
                language: TranscriptionLanguage::Auto,
                dictionary: &DictionaryContext::default(),
            })
            .expect_err("missing provider key");

        let TranscriptionError::Failure(failure) = error else {
            panic!("structured cloud failure")
        };
        assert_eq!(failure.provider, Some(provider));
        assert_eq!(failure.category, FailureCategory::Authentication);
        assert_eq!(
            failure.safe_message,
            format!("{provider} API key is not configured; add it in Settings > Integrations")
        );
    }
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
fn compressed_audio_is_decoded_through_codec_bridge_for_whisper() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let wav_path = tmp.path().join("source.wav");
    let m4a_path = tmp.path().join("source.m4a");
    let mp3_path = tmp.path().join("source.mp3");
    write_test_wav(&wav_path);
    encode_wav_to_m4a(&wav_path, &m4a_path, 96).expect("M4A fixture");
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

    for (extension, path) in [("m4a", &m4a_path), ("mp3", &mp3_path)] {
        let decoded = decode_audio_for_whisper(path)
            .unwrap_or_else(|error| panic!("decode {extension}: {error:?}"));

        assert!(!decoded.is_empty(), "{extension} decoded no samples");
    }
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
