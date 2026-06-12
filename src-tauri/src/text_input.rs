use std::process::Command;

pub fn should_type_transcript_text(text: &str) -> bool {
    !text.trim().is_empty()
}

pub fn auto_transcript_input_should_type(
    enabled: bool,
    live_chunk_id: Option<u64>,
    text: &str,
) -> bool {
    enabled && live_chunk_id.is_some() && should_type_transcript_text(text)
}

pub fn type_text_into_focused_cursor(text: &str) -> Result<(), String> {
    if !should_type_transcript_text(text) {
        return Ok(());
    }

    type_text_into_focused_cursor_platform(text)
}

#[cfg(target_os = "macos")]
fn type_text_into_focused_cursor_platform(text: &str) -> Result<(), String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(macos_auto_type_script())
        .arg(text)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err("osascript failed to paste transcript text".to_string())
    } else {
        Err(stderr)
    }
}

#[cfg(not(target_os = "macos"))]
fn type_text_into_focused_cursor_platform(_text: &str) -> Result<(), String> {
    Err("automatic transcript input is only supported on macOS".to_string())
}

pub fn macos_auto_type_script() -> &'static str {
    r#"on run argv
set typedText to item 1 of argv
set previousClipboard to the clipboard
set the clipboard to typedText
delay 0.05
tell application "System Events"
  keystroke "v" using command down
end tell
delay 0.05
set the clipboard to previousClipboard
end run"#
}
