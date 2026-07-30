use wakenote::text_input::{
    accessibility_permission_error, auto_transcript_input_should_type, auto_transcript_input_text,
    should_type_transcript_text,
};

#[test]
fn automatic_transcript_input_requires_setting_live_chunk_and_text() {
    assert!(auto_transcript_input_should_type(true, Some(42), "hello"));
    assert!(!auto_transcript_input_should_type(false, Some(42), "hello"));
    assert!(!auto_transcript_input_should_type(true, None, "hello"));
    assert!(!auto_transcript_input_should_type(true, Some(42), "   "));
}

#[test]
fn transcript_text_typing_skips_empty_or_whitespace_only_text() {
    assert!(should_type_transcript_text("dictation"));
    assert!(!should_type_transcript_text(""));
    assert!(!should_type_transcript_text("  \n\t "));
}

#[test]
fn auto_transcript_input_text_optionally_appends_one_trailing_space() {
    assert_eq!(
        auto_transcript_input_text("hello", false),
        Some("hello".to_string())
    );
    assert_eq!(
        auto_transcript_input_text("hello", true),
        Some("hello ".to_string())
    );
    assert_eq!(
        auto_transcript_input_text("hello   ", true),
        Some("hello ".to_string())
    );
    assert_eq!(auto_transcript_input_text("   ", true), None);
}

#[test]
fn native_text_input_permission_error_is_actionable() {
    let error = accessibility_permission_error();

    assert!(error.contains("WakeNote"));
    assert!(error.contains("Accessibility"));
    assert!(error.contains("System Settings"));
}
