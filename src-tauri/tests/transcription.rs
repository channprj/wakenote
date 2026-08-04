use std::path::PathBuf;

use wakenote::cloud_realtime::{RealtimeStoredResult, realtime_result_store};
use wakenote::cloud_transcription::FailureCategory;
use wakenote::dictionary::DictionaryContext;
use wakenote::queue::{QueueIssueCode, QueueIssueSeverity, QueueJobStatus, TranscriptionQueue};
use wakenote::recorder::{ChunkMetadata, ChunkSource, TranscriptionStatus};
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
fn transcription_worker_writes_dictionary_corrected_text() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230710.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    queue.enqueue_file(&audio_path, "whisper-medium");
    let dictionary = DictionaryContext::compile(
        true,
        &[DictionaryEntry {
            id: "wake".into(),
            term: "WakeNote".into(),
            aliases: vec!["wake note".into()],
            enabled: true,
        }],
    );
    let worker = TranscriptionWorker::with_options_and_dictionary(
        StaticTranscriber::success("wake note is ready"),
        TranscriptionWorkerOptions {
            language: TranscriptionLanguage::Auto,
            suppress_low_confidence_transcripts: true,
        },
        dictionary,
    );

    worker.process_next(&mut queue).expect("process");

    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("transcript"),
        "WakeNote is ready\n"
    );
}

#[test]
fn transcription_worker_records_empty_output_as_no_speech_without_blank_txt() {
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
    let job = queue.job(id).expect("job");
    assert_eq!(job.status, QueueJobStatus::Failed);
    assert_eq!(job.error.as_deref(), Some("No speech detected"));
    assert_eq!(
        job.issue.as_ref().map(|issue| (issue.severity, issue.code)),
        Some((QueueIssueSeverity::Warning, QueueIssueCode::NoSpeech)),
    );
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
        queue
            .job(id)
            .expect("job")
            .issue
            .as_ref()
            .map(|issue| (issue.severity, issue.code)),
        Some((
            QueueIssueSeverity::Warning,
            QueueIssueCode::TranscriptArtifact,
        )),
    );
    assert_eq!(
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("preserved transcript"),
        "[감사합니다]\n"
    );
}

#[test]
fn transcription_worker_preserves_degenerate_repetition_in_txt() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("20260506").join("230713.wav");
    std::fs::create_dir_all(audio_path.parent().unwrap()).expect("audio dir");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file(&audio_path, "whisper-medium");
    let transcript = format!("{} MBC 뉴스 김수근입니다.", "[끝] ".repeat(6));
    let worker = TranscriptionWorker::with_options(
        StaticTranscriber::success(&transcript).expecting_language(TranscriptionLanguage::Ko),
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
        std::fs::read_to_string(audio_path.with_extension("txt")).expect("preserved transcript"),
        format!("{transcript}\n")
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
fn transcript_artifact_filter_suppresses_repeated_wrapped_signoff() {
    let transcript = format!("{} MBC 뉴스 김수근입니다.", "[끝] ".repeat(6));

    assert!(should_suppress_transcript_artifact(&transcript));
}

#[test]
fn transcript_artifact_filter_suppresses_repeated_sentence_signoff() {
    let transcript = format!(
        "{} MBC 뉴스 김수근입니다.",
        "[오늘은 퇴근하기로 했어요] ".repeat(6)
    );

    assert!(should_suppress_transcript_artifact(&transcript));
}

#[test]
fn transcript_artifact_filter_suppresses_repeated_wrapped_phrase_flood() {
    let transcript = "[머리가 너무 예뻐서] ".repeat(60);

    assert!(should_suppress_transcript_artifact(&transcript));
}

#[test]
fn transcript_artifact_filter_suppresses_replacement_character_and_token_loop() {
    let transcript = format!("해� vor {}", "그리고 ".repeat(40));

    assert!(should_suppress_transcript_artifact(&transcript));
}

#[test]
fn transcript_artifact_filter_suppresses_repeated_microphone_test_signoff() {
    let transcript = format!(
        "{} MBC 뉴스 김수근입니다.",
        "마이크 테스트 하나 둘 셋 ".repeat(6)
    );

    assert!(should_suppress_transcript_artifact(&transcript));
}

#[test]
fn transcript_artifact_filter_preserves_natural_repetition_and_structured_speech() {
    for transcript in [
        "네 네, 확인했습니다.",
        "아니 아니, 그게 아니라 오늘 MBC 뉴스 사례를 회의에서 검토했습니다.",
        "회의는 Zoom Zoom 연결 상태를 확인한 뒤 시작하겠습니다.",
        "[1단계] 요구사항을 검토했고 [2단계] 구현을 마쳤으며 [3단계] 테스트를 실행하고 [4단계] 배포 여부를 확인합니다.",
    ] {
        assert!(
            !should_suppress_transcript_artifact(transcript),
            "natural transcript should be preserved: {transcript}"
        );
    }
}

#[test]
fn transcript_artifact_filter_preserves_five_identical_tokens() {
    let transcript = format!("{}하나 둘 셋 넷 다섯 여섯 일곱", "반복 ".repeat(5));

    assert_eq!(transcript_artifact_reason(&transcript), None);
}

#[test]
fn transcript_artifact_filter_suppresses_six_identical_tokens_as_a_run() {
    assert_eq!(
        transcript_artifact_reason(&"반복 ".repeat(6)),
        Some(TranscriptArtifactReason::RepeatedTokenRun)
    );
}

#[test]
fn transcript_artifact_filter_suppresses_three_non_overlapping_phrase_occurrences() {
    let transcript = format!("{}오늘 회의 종료", "하나 둘 셋 ".repeat(3));

    assert_eq!(
        transcript_artifact_reason(&transcript),
        Some(TranscriptArtifactReason::DominantRepeatedNgram)
    );
}

#[test]
fn transcript_artifact_filter_preserves_replacement_character_with_newsletter() {
    assert_eq!(
        transcript_artifact_reason("해� 뉴스레터 내용을 오늘 회의에서 검토했습니다."),
        None
    );
}

#[test]
fn transcript_artifact_filter_preserves_replacement_character_with_news_agency() {
    assert_eq!(
        transcript_artifact_reason("해� 연합뉴스 기사를 오늘 회의에서 검토했습니다."),
        None
    );
}

#[test]
fn transcript_artifact_filter_combines_replacement_character_with_broadcaster_news() {
    assert_eq!(
        transcript_artifact_reason("해� 오늘 MBC 뉴스 김수근입니다. 내용을 검토했습니다."),
        Some(TranscriptArtifactReason::CompoundSignals)
    );
}

#[test]
fn transcript_artifact_filter_requires_four_dominant_bracket_groups() {
    assert_eq!(transcript_artifact_reason("말 [하나] [둘] [셋] 완료"), None);
    assert_eq!(
        transcript_artifact_reason("말 [하나] [둘] [셋] [넷] 완료"),
        Some(TranscriptArtifactReason::BracketFlood)
    );
}

#[test]
fn transcript_artifact_filter_preserves_nested_bracket_spans_as_one_group() {
    assert_eq!(
        transcript_artifact_reason(
            "([one] [two] [three] We discussed a sufficiently long ordinary project timeline and release plan in detail.)"
        ),
        None
    );
}

#[test]
fn transcript_artifact_filter_counts_adjacent_bracket_groups_separately() {
    assert_eq!(
        transcript_artifact_reason("말 [끝][끝][끝][끝] 완료"),
        Some(TranscriptArtifactReason::BracketFlood)
    );
}

#[test]
fn transcript_artifact_filter_requires_two_weak_signals() {
    assert_eq!(
        transcript_artifact_reason("해� 오늘 회의 내용을 검토했습니다."),
        None
    );
    assert_eq!(
        transcript_artifact_reason("하 a b 하 c a 하 b c b a c"),
        None
    );
    assert_eq!(
        transcript_artifact_reason("해� 오늘 MBC 뉴스 내용을 검토했습니다."),
        Some(TranscriptArtifactReason::CompoundSignals)
    );
}

#[test]
fn transcript_artifact_filter_reports_stable_reasons() {
    assert_eq!(
        transcript_artifact_reason("(웃음)"),
        Some(TranscriptArtifactReason::WrappedPhrase)
    );
    assert_eq!(
        transcript_artifact_reason("MBC 뉴스 김수근입니다."),
        Some(TranscriptArtifactReason::BroadcastNewsSignoff)
    );
    assert_eq!(
        transcript_artifact_reason("thanks for watching"),
        Some(TranscriptArtifactReason::CommonHallucination)
    );
    assert_eq!(
        transcript_artifact_reason(&"그리고 ".repeat(6)),
        Some(TranscriptArtifactReason::RepeatedTokenRun)
    );
    assert_eq!(
        transcript_artifact_reason(&"하나 둘 셋 ".repeat(4)),
        Some(TranscriptArtifactReason::DominantRepeatedNgram)
    );
    assert_eq!(
        transcript_artifact_reason("말 [하나] [둘] [셋] [넷] 완료"),
        Some(TranscriptArtifactReason::BracketFlood)
    );
    assert_eq!(
        transcript_artifact_reason("해� a b 해 c a 해 b c b a c"),
        Some(TranscriptArtifactReason::CompoundSignals)
    );
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
fn applying_an_outcome_stamps_the_issue_at_the_terminal_boundary() {
    let mut queue = TranscriptionQueue::new();
    let id = queue.enqueue_file("/recordings/empty.wav", "openai-gpt-transcribe");
    queue.start_next().expect("start job");
    let occurred_at = chrono::Utc.with_ymd_and_hms(2026, 8, 4, 1, 2, 3).unwrap();

    apply_outcome_at(
        &mut queue,
        TranscriptionJobOutcome::failed_with_issue(
            id,
            wakenote::queue::QueueJobIssue::warning(
                QueueIssueCode::EmptyTranscript,
                "OpenAI returned an empty transcript",
            ),
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
            dictionary: &DictionaryContext::default(),
        })
        .expect_err("missing model should fail");

    assert_eq!(
        error,
        TranscriptionError::ModelMissing(tmp.path().join("models/whisper-medium.bin"))
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
            dictionary: &DictionaryContext::default(),
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
fn runtime_transcriber_consumes_the_realtime_result_without_reuploading_audio() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("live.wav");
    std::fs::write(&audio_path, b"recorded audio").expect("audio");
    realtime_result_store().publish(
        audio_path.clone(),
        RealtimeStoredResult::Completed(wakenote::transcription::TranscriptionExecution {
            text: "live transcript".into(),
            speaker_turns: Vec::new(),
            requested_model_id: "openai-gpt-live-transcribe".into(),
            effective_model_id: "openai-gpt-live-transcribe".into(),
            fallback_from_model_id: None,
            usage: None,
            issue: None,
        }),
    );
    let transcriber = RuntimeTranscriber::new(tmp.path());

    let result = transcriber
        .transcribe_execution(TranscriptionRequest {
            audio_path: &audio_path,
            model_id: "openai-gpt-live-transcribe",
            language: TranscriptionLanguage::Auto,
            dictionary: &DictionaryContext::default(),
        })
        .expect("realtime result");

    assert_eq!(result.text, "live transcript");
    assert_eq!(result.effective_model_id, "openai-gpt-live-transcribe");
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
