use std::path::PathBuf;

use sagwan::queue::{QueueJobStatus, TranscriptionQueue};
use sagwan::transcription::{
    Transcriber, TranscriptionError, TranscriptionRequest, TranscriptionWorker, WhisperTranscriber,
};

#[derive(Clone)]
struct StaticTranscriber {
    result: Result<String, String>,
}

impl StaticTranscriber {
    fn success(transcript: &str) -> Self {
        Self {
            result: Ok(transcript.to_string()),
        }
    }

    fn failure(error: &str) -> Self {
        Self {
            result: Err(error.to_string()),
        }
    }
}

impl Transcriber for StaticTranscriber {
    fn transcribe(&self, request: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
        assert_eq!(request.model_id, "whisper-medium");
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
fn whisper_transcriber_reports_missing_model_before_running_inference() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let audio_path = tmp.path().join("sample.wav");
    std::fs::write(&audio_path, b"wav bytes").expect("audio");
    let transcriber = WhisperTranscriber::new(tmp.path().join("models"));

    let error = transcriber
        .transcribe(TranscriptionRequest {
            audio_path: &audio_path,
            model_id: "whisper-medium",
        })
        .expect_err("missing model should fail");

    assert_eq!(
        error,
        TranscriptionError::ModelMissing(PathBuf::from(
            tmp.path().join("models/whisper-medium.bin")
        ))
    );
}
