use wakenote::text_input::{
    auto_transcript_input_should_type, auto_transcript_input_text, macos_auto_type_script,
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
fn macos_auto_type_script_pastes_argv_text_and_restores_clipboard() {
    let script = macos_auto_type_script();

    assert!(script.contains("set typedText to item 1 of argv"));
    assert!(script.contains("set previousClipboard to the clipboard"));
    assert!(script.contains("keystroke \"v\" using command down"));
    assert!(!script.contains("delay 0.05"));
    assert!(script.contains("delay 0.01"));
    assert!(script.contains("set the clipboard to previousClipboard"));
}
