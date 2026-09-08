//! Replay saved Activity audio through the existing cloud realtime transports.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cloud_realtime::{RealtimeSamplesRequest, RealtimeStoredResult, realtime_result_store};
use crate::cloud_transcription::TranscriptionCredentials;
use crate::meeting::{MeetingRealtimeReplay, MeetingRealtimeReplayRequest};
use crate::openai_realtime::OpenAiRealtimeManager;
use crate::soniox_realtime::SonioxRealtimeManager;
use crate::transcription::{
    TranscriptionError, TranscriptionExecution, TranscriptionRequest, decode_audio_for_whisper,
};

static NEXT_REPLAY: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct RecordedRealtimeOptions {
    pub credentials: TranscriptionCredentials,
    pub is_cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}

pub fn transcribe_recorded_realtime(
    request: TranscriptionRequest<'_>,
    options: &RecordedRealtimeOptions,
) -> Result<TranscriptionExecution, TranscriptionError> {
    // Each replay owns its transport so a live microphone session cannot be
    // replaced, closed, or sent archived partials by Activity reprocessing.
    let replay = match request.model_id {
        "openai-gpt-live-transcribe" => {
            let manager = OpenAiRealtimeManager::new(Arc::new(|_| {}));
            let submit_manager = manager.clone();
            MeetingRealtimeReplay::new(
                Arc::new(move |request| {
                    let source_key = request.source_key.clone();
                    let chunk_id = request.chunk_id;
                    let audio_path = request.audio_path.clone();
                    submit_manager.submit_recorded_samples(samples_request(request));
                    submit_manager.commit(source_key, chunk_id, audio_path);
                    Ok(())
                }),
                Arc::new(move |source_key| manager.close_source(source_key)),
            )
        }
        "soniox-realtime-v5" => {
            let manager = SonioxRealtimeManager::new(Arc::new(|_| {}));
            let submit_manager = manager.clone();
            MeetingRealtimeReplay::new(
                Arc::new(move |request| {
                    let source_key = request.source_key.clone();
                    let chunk_id = request.chunk_id;
                    let audio_path = request.audio_path.clone();
                    submit_manager.submit_recorded_samples(samples_request(request));
                    submit_manager.commit(source_key, chunk_id, audio_path);
                    Ok(())
                }),
                Arc::new(move |source_key| manager.close_source(source_key)),
            )
        }
        _ => {
            return Err(TranscriptionError::Engine(
                "unsupported recorded realtime model".into(),
            ));
        }
    };
    replay_recorded_audio(request, options, &replay)
}

fn samples_request(request: MeetingRealtimeReplayRequest) -> RealtimeSamplesRequest {
    RealtimeSamplesRequest {
        source_key: request.source_key,
        source_label: request.source_label,
        microphone_slot: None,
        chunk_id: request.chunk_id,
        model_id: request.model_id,
        language: request.language,
        dictionary: request.dictionary,
        sample_rate: request.sample_rate,
        samples: request.samples,
        credentials: request.credentials,
    }
}

fn replay_recorded_audio(
    request: TranscriptionRequest<'_>,
    options: &RecordedRealtimeOptions,
    replay: &MeetingRealtimeReplay,
) -> Result<TranscriptionExecution, TranscriptionError> {
    if (options.is_cancelled)() {
        return Err(TranscriptionError::Engine(
            "Activity reprocessing cancelled".into(),
        ));
    }
    let samples = decode_audio_for_whisper(request.audio_path)?;
    if (options.is_cancelled)() {
        return Err(TranscriptionError::Engine(
            "Activity reprocessing cancelled".into(),
        ));
    }
    let token = format!(
        "{}-{}",
        std::process::id(),
        NEXT_REPLAY.fetch_add(1, Ordering::Relaxed)
    );
    let source_key = format!("activity:{token}");
    // This is a result-store key, not a file. Late results from an earlier
    // attempt (or the original live recording) must not satisfy this replay.
    let result_path = request.audio_path.with_extension(format!("replay-{token}"));
    let result = replay
        .submit(MeetingRealtimeReplayRequest {
            source_key: source_key.clone(),
            source_label: "Activity reprocessing".into(),
            chunk_id: 1,
            model_id: request.model_id.into(),
            language: request.language,
            dictionary: request.dictionary.clone(),
            sample_rate: 16_000,
            samples: Arc::new(samples),
            credentials: options.credentials.clone(),
            audio_path: result_path.clone(),
        })
        .map_err(TranscriptionError::Engine)
        .and_then(|()| {
            match realtime_result_store()
                .wait_until_cancelled(&result_path, || (options.is_cancelled)())
            {
                Some(RealtimeStoredResult::Completed(execution)) => Ok(execution),
                Some(RealtimeStoredResult::Failed(failure)) => {
                    Err(TranscriptionError::Failure(failure))
                }
                None => Err(TranscriptionError::Engine(
                    "Activity reprocessing cancelled".into(),
                )),
            }
        });
    replay.close_source(source_key);
    realtime_result_store().discard(&result_path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::DictionaryContext;
    use crate::settings::TranscriptionLanguage;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    fn execution(text: &str, model_id: &str) -> TranscriptionExecution {
        TranscriptionExecution {
            text: text.into(),
            requested_model_id: model_id.into(),
            effective_model_id: model_id.into(),
            fallback_from_model_id: None,
            speaker_turns: Vec::new(),
            usage: None,
            issue: None,
        }
    }

    fn write_audio(path: &std::path::Path) {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..16_000 {
            writer.write_sample(8_000_i16).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn replay_sends_saved_samples_and_uses_fresh_results_for_each_attempt() {
        let temp = tempfile::tempdir().unwrap();
        let audio_path = temp.path().join("saved.wav");
        write_audio(&audio_path);
        let paths = Arc::new(Mutex::new(Vec::new()));
        let submitted_paths = paths.clone();
        let closed = Arc::new(AtomicUsize::new(0));
        let closed_for_replay = closed.clone();
        let replay = MeetingRealtimeReplay::new(
            Arc::new(move |request| {
                assert!(request.source_key.starts_with("activity:"));
                assert_eq!(request.sample_rate, 16_000);
                assert_eq!(request.samples.len(), 16_000);
                assert!(request.samples[0] > 0.2);
                assert_eq!(request.language, TranscriptionLanguage::Ko);
                submitted_paths
                    .lock()
                    .unwrap()
                    .push(request.audio_path.clone());
                realtime_result_store().publish(
                    request.audio_path,
                    RealtimeStoredResult::Completed(execution(
                        "fresh transcript",
                        &request.model_id,
                    )),
                );
                Ok(())
            }),
            Arc::new(move |_| {
                closed_for_replay.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let options = RecordedRealtimeOptions {
            credentials: TranscriptionCredentials::default(),
            is_cancelled: Arc::new(|| false),
        };
        for model_id in ["soniox-realtime-v5", "openai-gpt-live-transcribe"] {
            realtime_result_store().publish(
                audio_path.clone(),
                RealtimeStoredResult::Completed(execution("stale", model_id)),
            );
            let result = replay_recorded_audio(
                TranscriptionRequest {
                    audio_path: &audio_path,
                    model_id,
                    language: TranscriptionLanguage::Ko,
                    dictionary: &DictionaryContext::default(),
                },
                &options,
                &replay,
            )
            .unwrap();
            assert_eq!(result.text, "fresh transcript");
            assert_eq!(result.effective_model_id, model_id);
        }
        let paths = paths.lock().unwrap();
        assert_ne!(paths[0], paths[1]);
        assert!(paths.iter().all(|path| path != &audio_path));
        assert_eq!(closed.load(Ordering::Relaxed), 2);
        realtime_result_store().discard(&audio_path);
    }

    #[test]
    fn cancellation_after_submission_closes_the_replay_without_waiting_for_a_result() {
        let temp = tempfile::tempdir().unwrap();
        let audio_path = temp.path().join("cancel.wav");
        write_audio(&audio_path);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_submit = cancelled.clone();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_replay = closed.clone();
        let replay = MeetingRealtimeReplay::new(
            Arc::new(move |_| {
                cancelled_for_submit.store(true, Ordering::Release);
                Ok(())
            }),
            Arc::new(move |_| {
                closed_for_replay.store(true, Ordering::Release);
            }),
        );
        let result = replay_recorded_audio(
            TranscriptionRequest {
                audio_path: &audio_path,
                model_id: "soniox-realtime-v5",
                language: TranscriptionLanguage::Auto,
                dictionary: &DictionaryContext::default(),
            },
            &RecordedRealtimeOptions {
                credentials: TranscriptionCredentials::default(),
                is_cancelled: Arc::new(move || cancelled.load(Ordering::Acquire)),
            },
            &replay,
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert!(closed.load(Ordering::Acquire));
    }

    #[test]
    fn submission_failure_closes_the_replay() {
        let temp = tempfile::tempdir().unwrap();
        let audio_path = temp.path().join("failure.wav");
        write_audio(&audio_path);
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_replay = closed.clone();
        let replay = MeetingRealtimeReplay::new(
            Arc::new(|_| Err("transport unavailable".into())),
            Arc::new(move |_| {
                closed_for_replay.store(true, Ordering::Release);
            }),
        );
        let result = replay_recorded_audio(
            TranscriptionRequest {
                audio_path: &audio_path,
                model_id: "openai-gpt-live-transcribe",
                language: TranscriptionLanguage::Auto,
                dictionary: &DictionaryContext::default(),
            },
            &RecordedRealtimeOptions {
                credentials: TranscriptionCredentials::default(),
                is_cancelled: Arc::new(|| false),
            },
            &replay,
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("transport unavailable")
        );
        assert!(closed.load(Ordering::Acquire));
    }

    #[test]
    fn runtime_reprocessing_decodes_saved_audio_instead_of_consuming_a_live_result() {
        use crate::transcription::{RuntimeTranscriber, Transcriber};
        let temp = tempfile::tempdir().unwrap();
        let audio_path = temp.path().join("invalid.wav");
        std::fs::write(&audio_path, b"invalid audio").unwrap();
        for model_id in ["soniox-realtime-v5", "openai-gpt-live-transcribe"] {
            realtime_result_store().publish(
                audio_path.clone(),
                RealtimeStoredResult::Completed(execution("live result", model_id)),
            );
            let dictionary = DictionaryContext::default();
            let request = TranscriptionRequest {
                audio_path: &audio_path,
                model_id,
                language: TranscriptionLanguage::Auto,
                dictionary: &dictionary,
            };
            let runtime = RuntimeTranscriber::new(temp.path());
            let replay = runtime
                .clone()
                .with_recorded_realtime(Some(RecordedRealtimeOptions {
                    credentials: TranscriptionCredentials::default(),
                    is_cancelled: Arc::new(|| false),
                }));
            assert!(matches!(
                replay.transcribe_execution(request),
                Err(TranscriptionError::Wav(_))
            ));
            assert_eq!(
                runtime.transcribe_execution(request).unwrap().text,
                "live result"
            );
        }
    }
}
