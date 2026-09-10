use wakenote::commands::AppBackend;
use wakenote::persistence::AppPersistence;
use wakenote::settings::{
    AppSettings, LiveTranscriptionWebhookSettings, SettingsPatch, WebhookPayloadFormat,
};

#[test]
fn webhook_defaults_and_settings_persistence_restore_without_a_queue() {
    let root = tempfile::tempdir().unwrap();
    let persistence = AppPersistence::new(root.path());
    let mut settings = AppSettings::default();
    assert!(!settings.live_transcription_webhook.enabled);
    assert!(settings.live_transcription_webhook.endpoint_url.is_empty());
    assert_eq!(
        settings.live_transcription_webhook.payload_format,
        WebhookPayloadFormat::TextOnly
    );
    settings.live_transcription_webhook = LiveTranscriptionWebhookSettings {
        enabled: true,
        endpoint_url: "https://example.com/hook".into(),
        payload_format: WebhookPayloadFormat::Json,
    };
    persistence.save_settings(&settings).unwrap();
    assert_eq!(
        persistence
            .load_settings()
            .unwrap()
            .unwrap()
            .live_transcription_webhook,
        settings.live_transcription_webhook
    );
    let mut legacy = serde_json::to_value(&settings).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("live_transcription_webhook");
    assert_eq!(
        serde_json::from_value::<AppSettings>(legacy)
            .unwrap()
            .live_transcription_webhook,
        LiveTranscriptionWebhookSettings::default()
    );
}

#[test]
fn invalid_enabled_webhook_is_rejected_without_mutating_or_persisting_settings() {
    let root = tempfile::tempdir().unwrap();
    let mut backend = AppBackend::load_from_dir(root.path()).unwrap();
    for endpoint in [
        "",
        "file:///tmp/hook",
        "https://",
        "https:example.com",
        "https://bad host.test",
    ] {
        let result = backend.try_update_settings(SettingsPatch {
            live_transcription_webhook: Some(LiveTranscriptionWebhookSettings {
                enabled: true,
                endpoint_url: endpoint.into(),
                payload_format: WebhookPayloadFormat::Json,
            }),
            ..Default::default()
        });
        assert!(result.is_err());
        assert!(!backend.settings().live_transcription_webhook.enabled);
    }
    assert!(
        AppPersistence::new(root.path())
            .load_settings()
            .unwrap()
            .is_none()
    );
    let valid = LiveTranscriptionWebhookSettings {
        enabled: true,
        endpoint_url: " http://[::1]:8765/hook ".into(),
        payload_format: WebhookPayloadFormat::Json,
    };
    backend
        .try_update_settings(SettingsPatch {
            live_transcription_webhook: Some(valid),
            ..Default::default()
        })
        .unwrap();
    let restored = AppBackend::load_from_dir(root.path()).unwrap().settings();
    assert!(restored.live_transcription_webhook.enabled);
    assert_eq!(
        restored.live_transcription_webhook.endpoint_url,
        "http://[::1]:8765/hook"
    );
    backend
        .try_update_settings(SettingsPatch {
            live_transcription_webhook: Some(LiveTranscriptionWebhookSettings::default()),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn settings_debug_output_redacts_the_entire_endpoint() {
    let mut settings = AppSettings::default();
    settings.live_transcription_webhook.endpoint_url =
        "https://user:password@example.com/secret?token=value".into();
    let debug = format!("{settings:?}");
    for secret in ["password", "example.com", "token=value"] {
        assert!(!debug.contains(secret));
    }
}
