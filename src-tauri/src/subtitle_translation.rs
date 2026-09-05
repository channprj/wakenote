use crate::overlay_caption::{OverlayCaptionSnapshot, OverlayCaptionSource};
use crate::settings::{AppSettings, TranscriptionLanguage};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptionKey {
    source: OverlayCaptionSource,
    chunk_id: Option<u64>,
    audio_path: Option<String>,
    language: TranscriptionLanguage,
    model: String,
}

#[derive(Clone)]
pub struct CaptionTranslationRequest {
    key: CaptionKey,
    pub text: String,
    pub settings: AppSettings,
}

impl CaptionTranslationRequest {
    pub fn new(snapshot: &OverlayCaptionSnapshot, settings: &AppSettings) -> Option<Self> {
        if !settings.subtitle_translation_enabled
            || settings.pause_all
            || !snapshot.visible
            || snapshot.text.trim().is_empty()
            || snapshot.source == OverlayCaptionSource::Preview
        {
            return None;
        }
        Some(Self {
            key: CaptionKey {
                source: snapshot.source,
                chunk_id: snapshot.chunk_id,
                audio_path: snapshot
                    .chunk_id
                    .is_none()
                    .then(|| snapshot.audio_path.clone())
                    .flatten(),
                language: settings.subtitle_translation_language,
                model: settings.effective_text_transform_model().into(),
            },
            text: snapshot.text.clone(),
            settings: settings.clone(),
        })
    }
}

#[derive(Default)]
pub struct SubtitleTranslationQueue {
    latest: Option<CaptionTranslationRequest>,
    pending: Option<CaptionTranslationRequest>,
    result: Option<(CaptionTranslationRequest, String)>,
    failed: Option<CaptionKey>,
    cancellation: Option<CancellationToken>,
    active: bool,
}

impl SubtitleTranslationQueue {
    pub fn submit(&mut self, request: CaptionTranslationRequest) -> bool {
        if self
            .latest
            .as_ref()
            .is_some_and(|latest| latest.key == request.key && latest.text == request.text)
        {
            return false;
        }
        if self
            .latest
            .as_ref()
            .is_none_or(|latest| latest.key != request.key)
        {
            if let Some(token) = &self.cancellation {
                token.cancel();
            }
            self.result = None;
            self.failed = None;
        }
        self.latest = Some(request.clone());
        if self.failed.as_ref() == Some(&request.key) {
            return false;
        }
        self.pending = Some(request);
        let start = !self.active;
        self.active = true;
        start
    }

    pub fn take_next(&mut self) -> Option<(CaptionTranslationRequest, CancellationToken)> {
        let Some(request) = self.pending.take() else {
            self.active = false;
            return None;
        };
        let token = CancellationToken::new();
        self.cancellation = Some(token.clone());
        Some((request, token))
    }

    pub fn complete(
        &mut self,
        request: &CaptionTranslationRequest,
        result: Result<String, String>,
    ) -> bool {
        if !self
            .latest
            .as_ref()
            .is_some_and(|latest| latest.key == request.key)
        {
            return false;
        }
        match result {
            Ok(text) => self.result = Some((request.clone(), text)),
            Err(_) => {
                self.failed = Some(request.key.clone());
                self.pending = None;
                self.result = None;
            }
        }
        true
    }

    pub fn translated_text(&self, request: &CaptionTranslationRequest) -> Option<&str> {
        self.result
            .as_ref()
            .filter(|(completed, _)| completed.key == request.key)
            .map(|(_, text)| text.as_str())
    }

    pub fn is_current_result(&self, request: &CaptionTranslationRequest) -> bool {
        self.result.as_ref().is_some_and(|(completed, _)| {
            completed.key == request.key && completed.text == request.text
        })
    }

    pub fn is_waiting(&self, request: &CaptionTranslationRequest) -> bool {
        self.failed.as_ref() != Some(&request.key) && !self.is_current_result(request)
    }

    pub fn clear(&mut self) {
        if let Some(token) = self.cancellation.take() {
            token.cancel();
        }
        self.latest = None;
        self.pending = None;
        self.result = None;
        self.failed = None;
    }
}
