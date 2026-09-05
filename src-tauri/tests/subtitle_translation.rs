use wakenote::overlay_caption::{OverlayCaptionRuntime, OverlayCaptionSource};
use wakenote::settings::{AppSettings, FloatingOverlayPosition, TranscriptionLanguage};
use wakenote::subtitle_translation::{CaptionTranslationRequest, SubtitleTranslationQueue};

#[test]
fn final_translation_gets_time_to_arrive_and_then_a_fresh_reading_interval() {
    let settings = AppSettings::default();
    let mut captions = OverlayCaptionRuntime::default();
    captions.show_partial(
        1,
        "source",
        FloatingOverlayPosition::Top,
        settings.floating_overlay_caption_style(),
    );
    captions.show_final_at(
        Some(1),
        std::path::PathBuf::from("/tmp/test-caption.wav"),
        "source",
        FloatingOverlayPosition::Top,
        settings.floating_overlay_caption_style(),
    );
    let old_generation = captions.snapshot().generation;
    assert!(captions.set_translation_pending(true));
    let waiting_generation = captions.snapshot().generation;
    assert!(!captions.hide_if_generation(old_generation));
    assert!(
        captions
            .hide_delay_for_generation(waiting_generation, std::time::Instant::now())
            .unwrap()
            .as_secs()
            >= 45
    );
    assert!(captions.set_translation_pending(false));
    assert!(!captions.hide_if_generation(waiting_generation));
    assert!(
        captions
            .hide_delay_for_generation(captions.snapshot().generation, std::time::Instant::now())
            .unwrap()
            .as_secs()
            <= 10
    );
}

#[test]
fn coalesces_partials_but_accepts_same_utterance_progress_without_starvation() {
    let settings = AppSettings {
        subtitle_translation_enabled: true,
        ..Default::default()
    };
    let mut captions = OverlayCaptionRuntime::default();
    captions.show_partial(
        1,
        "first words",
        FloatingOverlayPosition::Top,
        settings.floating_overlay_caption_style(),
    );
    let first = CaptionTranslationRequest::new(&captions.snapshot(), &settings).unwrap();
    let mut queue = SubtitleTranslationQueue::default();
    assert!(queue.submit(first.clone()));
    let (_, token) = queue.take_next().unwrap();
    captions.show_partial(
        1,
        "first words and more",
        FloatingOverlayPosition::Top,
        settings.floating_overlay_caption_style(),
    );
    let latest = CaptionTranslationRequest::new(&captions.snapshot(), &settings).unwrap();
    assert!(!queue.submit(latest.clone()));
    assert!(!token.is_cancelled());
    assert!(queue.complete(&first, Ok("첫 단어".into())));
    assert_eq!(queue.translated_text(&latest), Some("첫 단어"));
    assert!(!queue.is_current_result(&latest));
    let (pending, _) = queue.take_next().unwrap();
    assert_eq!(pending.text, "first words and more");
    queue.complete(&pending, Ok("첫 단어 그리고 추가 내용".into()));
    assert!(queue.is_current_result(&latest));
    assert!(queue.take_next().is_none());
    assert!(!queue.submit(latest));
}

#[test]
fn language_or_chunk_changes_cancel_old_work_and_reject_late_results() {
    let mut settings = AppSettings {
        subtitle_translation_enabled: true,
        ..Default::default()
    };
    let mut captions = OverlayCaptionRuntime::default();
    captions.show_partial(
        1,
        "text",
        FloatingOverlayPosition::Top,
        settings.floating_overlay_caption_style(),
    );
    let first = CaptionTranslationRequest::new(&captions.snapshot(), &settings).unwrap();
    let mut queue = SubtitleTranslationQueue::default();
    queue.submit(first.clone());
    let (_, token) = queue.take_next().unwrap();
    settings.subtitle_translation_language = TranscriptionLanguage::Ja;
    let changed = CaptionTranslationRequest::new(&captions.snapshot(), &settings).unwrap();
    queue.submit(changed.clone());
    assert!(token.is_cancelled());
    assert!(!queue.complete(&first, Ok("stale translation".into())));
    assert_eq!(queue.translated_text(&changed), None);
    queue.clear();
    assert!(queue.take_next().is_none());
    assert!(!queue.complete(&changed, Ok("late translation".into())));
    captions.hide_source(OverlayCaptionSource::LiveTranscription);
    assert!(CaptionTranslationRequest::new(&captions.snapshot(), &settings).is_none());
}
