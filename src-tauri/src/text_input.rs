#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
use enigo::{Enigo, Keyboard, Settings};

pub fn should_type_transcript_text(text: &str) -> bool {
    !text.trim().is_empty()
}

pub fn auto_transcript_input_text(text: &str, trailing_space: bool) -> Option<String> {
    if !should_type_transcript_text(text) {
        return None;
    }

    let mut text = text.trim_end().to_string();
    if trailing_space {
        text.push(' ');
    }
    Some(text)
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
    if !crate::permissions::accessibility_access_is_granted() {
        let _ = crate::permissions::request_accessibility_access();
        if !crate::permissions::accessibility_access_is_granted() {
            return Err(accessibility_permission_error().to_string());
        }
    }

    static INPUT: OnceLock<Mutex<Option<Enigo>>> = OnceLock::new();
    let mut input = INPUT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|error| format!("Could not lock the native text input: {error}"))?;
    if input.is_none() {
        *input = Some(
            Enigo::new(&Settings::default())
                .map_err(|error| format!("Could not initialize native text input: {error}"))?,
        );
    }
    input
        .as_mut()
        .expect("native text input initialized")
        .text(text)
        .map_err(|error| format!("Could not type Dictation text: {error}"))
}

#[cfg(not(target_os = "macos"))]
fn type_text_into_focused_cursor_platform(_text: &str) -> Result<(), String> {
    Err("automatic transcript input is only supported on macOS".to_string())
}

pub fn accessibility_permission_error() -> &'static str {
    "WakeNote needs Accessibility permission to type Dictation results. Open System Settings → Privacy & Security → Accessibility and enable WakeNote."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessibility_error_names_wakenote_and_the_required_permission() {
        let error = accessibility_permission_error();
        assert!(error.contains("WakeNote"));
        assert!(error.contains("Accessibility"));
        assert!(error.contains("Dictation"));
    }
}
