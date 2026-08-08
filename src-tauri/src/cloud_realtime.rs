use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use crate::cloud_transcription::{FailureCategory, TranscriptionCredentials, TranscriptionFailure};
use crate::dictionary::DictionaryContext;
use crate::settings::{MicrophoneSlot, TranscriptionLanguage};
use crate::transcription::TranscriptionExecution;

const RESULT_WAIT_TIMEOUT: Duration = Duration::from_secs(75);

#[derive(Debug, Clone, PartialEq)]
pub enum RealtimeStoredResult {
    Completed(TranscriptionExecution),
    Failed(TranscriptionFailure),
}

#[derive(Clone, Default)]
pub struct RealtimeResultStore {
    inner: Arc<(Mutex<HashMap<PathBuf, RealtimeStoredResult>>, Condvar)>,
}

impl RealtimeResultStore {
    pub fn publish(&self, audio_path: PathBuf, result: RealtimeStoredResult) {
        let (lock, ready) = &*self.inner;
        if let Ok(mut results) = lock.lock() {
            results.insert(audio_path, result);
            ready.notify_all();
        }
    }

    pub fn wait(&self, audio_path: &Path) -> RealtimeStoredResult {
        let (lock, ready) = &*self.inner;
        let Ok(results) = lock.lock() else {
            return RealtimeStoredResult::Failed(result_store_failure(
                FailureCategory::Transport,
                "Live transcription result store is unavailable",
            ));
        };
        let Ok((mut results, timeout)) =
            ready.wait_timeout_while(results, RESULT_WAIT_TIMEOUT, |results| {
                !results.contains_key(audio_path)
            })
        else {
            return RealtimeStoredResult::Failed(result_store_failure(
                FailureCategory::Transport,
                "Live transcription result wait failed",
            ));
        };
        if timeout.timed_out() {
            return RealtimeStoredResult::Failed(result_store_failure(
                FailureCategory::Transport,
                "Live transcription timed out",
            ));
        }
        results.remove(audio_path).unwrap_or_else(|| {
            RealtimeStoredResult::Failed(result_store_failure(
                FailureCategory::InvalidResponse,
                "Live transcription result is missing",
            ))
        })
    }

    #[cfg(test)]
    pub(crate) fn try_take(&self, audio_path: &Path) -> Option<RealtimeStoredResult> {
        let (lock, _) = &*self.inner;
        lock.lock().ok()?.remove(audio_path)
    }
}

pub fn realtime_result_store() -> &'static RealtimeResultStore {
    static STORE: OnceLock<RealtimeResultStore> = OnceLock::new();
    STORE.get_or_init(RealtimeResultStore::default)
}

#[derive(Debug, Clone)]
pub struct RealtimeSamplesRequest {
    pub source_key: String,
    pub source_label: String,
    pub microphone_slot: Option<MicrophoneSlot>,
    pub chunk_id: u64,
    pub model_id: String,
    pub language: TranscriptionLanguage,
    pub dictionary: DictionaryContext,
    pub sample_rate: u32,
    pub samples: Arc<Vec<f32>>,
    pub credentials: TranscriptionCredentials,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimePartial {
    pub source_key: String,
    pub source_label: String,
    pub microphone_slot: Option<MicrophoneSlot>,
    pub chunk_id: u64,
    pub text: String,
}

pub type RealtimePartialCallback = Arc<dyn Fn(RealtimePartial) + Send + Sync>;

fn result_store_failure(category: FailureCategory, message: &str) -> TranscriptionFailure {
    TranscriptionFailure {
        provider: None,
        http_status: None,
        provider_code: None,
        safe_message: message.to_string(),
        category,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn realtime_result_store_delivers_once_by_audio_path() {
        let store = RealtimeResultStore::default();
        let path = PathBuf::from("/tmp/wakenote-live.wav");
        store.publish(
            path.clone(),
            RealtimeStoredResult::Completed(TranscriptionExecution {
                text: "hello".into(),
                speaker_turns: Vec::new(),
                requested_model_id: "realtime-model".into(),
                effective_model_id: "realtime-model".into(),
                fallback_from_model_id: None,
                usage: None,
                issue: None,
            }),
        );

        let RealtimeStoredResult::Completed(result) = store.wait(&path) else {
            panic!("completed result")
        };
        assert_eq!(result.text, "hello");
    }
}
