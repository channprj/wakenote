//! Explicit opt-in smoke test. Reads existing settings/credentials without writing them.
use tokio_util::sync::CancellationToken;
use wakenote::llm::ReqwestOpenRouterClient;
use wakenote::persistence::AppPersistence;
use wakenote::settings::TranscriptionLanguage;
use wakenote::text_transform::{
    TextTransformKind, TextTransformRequest, transform_text_with_client,
};

#[tokio::test]
#[ignore = "Sends two synthetic text requests to OpenRouter; requires WAKENOTE_TEXT_SMOKE_APP_DATA"]
async fn translates_and_enhances_synthetic_text_with_saved_provider_configuration() {
    let root =
        std::env::var("WAKENOTE_TEXT_SMOKE_APP_DATA").expect("Set an explicit app-data path");
    let persistence = AppPersistence::new(root);
    let settings = persistence.load_settings().unwrap().unwrap_or_default();
    let key = persistence
        .load_openrouter_api_key()
        .unwrap()
        .expect("Configure an OpenRouter key");
    let client = ReqwestOpenRouterClient::default();
    for (kind, source) in [
        (
            TextTransformKind::Translate,
            "The meeting starts at 9. Please bring 3 examples.",
        ),
        (
            TextTransformKind::EnhancePrompt,
            "Help me write a short product announcement for WakeNote. Include exactly 3 bullet points. Don't invent pricing.",
        ),
    ] {
        let result = transform_text_with_client(
            &client,
            &settings,
            &key,
            &TextTransformRequest {
                kind,
                text: source.into(),
                target_language: Some(TranscriptionLanguage::Ko),
            },
            &CancellationToken::new(),
        )
        .await
        .expect("Provider transformation failed");
        assert_eq!(result.kind, kind);
        assert!(
            result.text.chars().any(|ch| ('가'..='힣').contains(&ch)),
            "Expected Korean output"
        );
        assert!(
            result.text.contains('3'),
            "The source count must be preserved"
        );
        assert!(!result.text.trim().is_empty());
        println!(
            "{kind:?}: received {} characters in Korean using {}",
            result.text.chars().count(),
            result.model
        );
    }
}
