use wakenote::text_input::{
    auto_transcript_input_should_type, macos_auto_type_script, should_type_transcript_text,
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
fn macos_auto_type_script_pastes_argv_text_and_restores_clipboard() {
    let script = macos_auto_type_script();

    assert!(script.contains("set typedText to item 1 of argv"));
    assert!(script.contains("set previousClipboard to the clipboard"));
    assert!(script.contains("keystroke \"v\" using command down"));
    assert!(script.contains("set the clipboard to previousClipboard"));
}
