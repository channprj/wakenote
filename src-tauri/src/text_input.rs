#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
use enigo::{Enigo, Keyboard, Settings};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

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

pub fn dictation_input_text(text: &str, remove_trailing_space: bool) -> Option<String> {
    if !should_type_transcript_text(text) {
        return None;
    }

    Some(if remove_trailing_space {
        text.trim_end().to_string()
    } else {
        text.to_string()
    })
}

pub fn auto_transcript_input_should_type(
    enabled: bool,
    live_chunk_id: Option<u64>,
    text: &str,
) -> bool {
    enabled && live_chunk_id.is_some() && should_type_transcript_text(text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAfterInput {
    KeepInputText,
    PreservePrevious,
}

pub const fn dictation_clipboard_after_input(copy_to_clipboard: bool) -> ClipboardAfterInput {
    if copy_to_clipboard {
        ClipboardAfterInput::KeepInputText
    } else {
        ClipboardAfterInput::PreservePrevious
    }
}

pub fn type_text_into_focused_cursor(text: &str) -> Result<(), String> {
    type_text_into_focused_cursor_with_clipboard(text, ClipboardAfterInput::KeepInputText)
}

pub fn type_text_into_focused_cursor_with_clipboard(
    text: &str,
    clipboard_after_input: ClipboardAfterInput,
) -> Result<(), String> {
    if !should_type_transcript_text(text) {
        return Ok(());
    }

    type_text_into_focused_cursor_platform(text, clipboard_after_input)
}

pub fn type_dictation_text_into_focused_cursor_with_clipboard(
    text: &str,
    clipboard_after_input: ClipboardAfterInput,
) -> Result<(), String> {
    if !should_type_transcript_text(text) {
        return Ok(());
    }

    type_dictation_text_into_focused_cursor_platform(text, clipboard_after_input)
}

pub fn copy_dictation_text_to_clipboard(text: &str) -> Result<(), String> {
    if !should_type_transcript_text(text) {
        return Ok(());
    }

    copy_dictation_text_to_clipboard_platform(text)
}

trait TextInputBackend {
    fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String>;
    fn can_type(&self) -> bool;
    fn request_type_access(&mut self);
    fn type_text(&mut self, text: &str) -> Result<(), String>;
}

fn input_text_with_backend(
    text: &str,
    clipboard_after_input: ClipboardAfterInput,
    backend: &mut impl TextInputBackend,
) -> Result<(), String> {
    match clipboard_after_input {
        ClipboardAfterInput::KeepInputText => {
            backend.copy_to_clipboard(text)?;
            if !backend.can_type() {
                backend.request_type_access();
                if !backend.can_type() {
                    return Err(accessibility_permission_error().to_string());
                }
            }
            backend.type_text(text)
        }
        ClipboardAfterInput::PreservePrevious => {
            if !backend.can_type() {
                backend.request_type_access();
                if !backend.can_type() {
                    return Err(accessibility_permission_preserving_clipboard_error().to_string());
                }
            }

            backend.type_text(text)
        }
    }
}

fn input_text_without_access_request_with_backend(
    text: &str,
    clipboard_after_input: ClipboardAfterInput,
    backend: &mut impl TextInputBackend,
) -> Result<(), String> {
    match clipboard_after_input {
        ClipboardAfterInput::KeepInputText => {
            backend.copy_to_clipboard(text)?;
            if !backend.can_type() {
                return Err(accessibility_permission_error().to_string());
            }
        }
        ClipboardAfterInput::PreservePrevious => {
            if !backend.can_type() {
                backend.copy_to_clipboard(text)?;
                return Err(accessibility_permission_error().to_string());
            }
        }
    }
    backend.type_text(text)
}

fn copy_dictation_text_with_backend(
    text: &str,
    backend: &mut impl TextInputBackend,
) -> Result<(), String> {
    if !should_type_transcript_text(text) {
        return Ok(());
    }
    backend.copy_to_clipboard(text)
}

#[cfg(target_os = "macos")]
struct MacTextInputBackend;

#[cfg(target_os = "macos")]
impl TextInputBackend for MacTextInputBackend {
    fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let text = NSString::from_str(text);
        pasteboard.clearContents();
        if !pasteboard.setString_forType(&text, unsafe { NSPasteboardTypeString }) {
            return Err("Could not copy Dictation text to the macOS pasteboard".to_string());
        }
        Ok(())
    }

    fn can_type(&self) -> bool {
        crate::permissions::accessibility_access_is_granted()
    }

    fn request_type_access(&mut self) {
        let _ = crate::permissions::request_accessibility_access();
    }

    fn type_text(&mut self, text: &str) -> Result<(), String> {
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
}

#[cfg(target_os = "macos")]
fn type_text_into_focused_cursor_platform(
    text: &str,
    clipboard_after_input: ClipboardAfterInput,
) -> Result<(), String> {
    with_mac_text_input(|backend| input_text_with_backend(text, clipboard_after_input, backend))
}

#[cfg(target_os = "macos")]
fn type_dictation_text_into_focused_cursor_platform(
    text: &str,
    clipboard_after_input: ClipboardAfterInput,
) -> Result<(), String> {
    with_mac_text_input(|backend| {
        input_text_without_access_request_with_backend(text, clipboard_after_input, backend)
    })
}

#[cfg(target_os = "macos")]
fn with_mac_text_input<T>(
    operation: impl FnOnce(&mut MacTextInputBackend) -> Result<T, String>,
) -> Result<T, String> {
    static TEXT_INPUT: OnceLock<Mutex<()>> = OnceLock::new();
    let _operation = TEXT_INPUT
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|error| format!("Could not lock the native text input: {error}"))?;
    operation(&mut MacTextInputBackend)
}

#[cfg(target_os = "macos")]
fn copy_dictation_text_to_clipboard_platform(text: &str) -> Result<(), String> {
    with_mac_text_input(|backend| copy_dictation_text_with_backend(text, backend))
}

#[cfg(not(target_os = "macos"))]
fn type_text_into_focused_cursor_platform(
    _text: &str,
    _clipboard_after_input: ClipboardAfterInput,
) -> Result<(), String> {
    Err("automatic transcript input is only supported on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
fn type_dictation_text_into_focused_cursor_platform(
    _text: &str,
    _clipboard_after_input: ClipboardAfterInput,
) -> Result<(), String> {
    Err("automatic transcript input is only supported on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
fn copy_dictation_text_to_clipboard_platform(_text: &str) -> Result<(), String> {
    Err("copying Dictation text is only supported on macOS".to_string())
}

pub fn accessibility_permission_error() -> &'static str {
    "The Dictation result was copied to the clipboard. WakeNote needs Accessibility permission to type it. Open System Settings → Privacy & Security → Accessibility and enable WakeNote."
}

pub fn accessibility_permission_preserving_clipboard_error() -> &'static str {
    "WakeNote needs Accessibility permission to type the Dictation result while preserving the clipboard. Open System Settings → Privacy & Security → Accessibility and enable WakeNote."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingTextInputBackend {
        can_type: bool,
        grant_after_request: bool,
        copy_error: Option<String>,
        type_error: Option<String>,
        clipboard: String,
        focused_text: String,
        overwrite_before_input: Option<String>,
        events: Vec<String>,
    }

    impl TextInputBackend for RecordingTextInputBackend {
        fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String> {
            self.events.push(format!("copy:{text}"));
            if let Some(error) = self.copy_error.clone() {
                return Err(error);
            }
            self.clipboard = text.to_string();
            Ok(())
        }

        fn can_type(&self) -> bool {
            self.can_type
        }

        fn request_type_access(&mut self) {
            self.events.push("request-access".to_string());
            self.can_type = self.grant_after_request;
        }

        fn type_text(&mut self, text: &str) -> Result<(), String> {
            self.events.push(format!("type:{text}"));
            if let Some(error) = self.type_error.clone() {
                return Err(error);
            }
            if let Some(clipboard) = self.overwrite_before_input.take() {
                self.clipboard = clipboard;
            }
            self.focused_text.push_str(text);
            Ok(())
        }
    }

    #[test]
    fn dictation_copies_transcript_before_typing() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            ..Default::default()
        };

        input_text_with_backend(
            "마이크 테스트",
            ClipboardAfterInput::KeepInputText,
            &mut backend,
        )
        .expect("dictation input");

        assert_eq!(backend.events, ["copy:마이크 테스트", "type:마이크 테스트"]);
        assert_eq!(backend.clipboard, "마이크 테스트");
        assert_eq!(backend.focused_text, "마이크 테스트");
    }

    #[test]
    fn dictation_copy_mode_inserts_transcript_even_if_clipboard_changes_before_delivery() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            clipboard: "existing clipboard".to_string(),
            overwrite_before_input: Some("existing clipboard".to_string()),
            ..Default::default()
        };

        input_text_with_backend(
            "새 디테이션 결과",
            ClipboardAfterInput::KeepInputText,
            &mut backend,
        )
        .expect("dictation input");

        assert_eq!(backend.focused_text, "새 디테이션 결과");
    }

    #[test]
    fn dictation_preserve_mode_inserts_transcript_even_if_clipboard_changes_before_delivery() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            clipboard: "existing clipboard".to_string(),
            overwrite_before_input: Some("existing clipboard".to_string()),
            ..Default::default()
        };

        input_text_with_backend(
            "새 디테이션 결과",
            ClipboardAfterInput::PreservePrevious,
            &mut backend,
        )
        .expect("dictation input");

        assert_eq!(backend.focused_text, "새 디테이션 결과");
    }

    #[test]
    fn denied_type_access_still_leaves_transcript_on_clipboard() {
        let mut backend = RecordingTextInputBackend::default();

        let error = input_text_with_backend(
            "clipboard fallback",
            ClipboardAfterInput::KeepInputText,
            &mut backend,
        )
        .expect_err("denied");

        assert_eq!(
            backend.events,
            ["copy:clipboard fallback", "request-access"]
        );
        assert_eq!(error, accessibility_permission_error());
        assert!(error.contains("copied"));
        assert!(error.contains("clipboard"));
    }

    #[test]
    fn clipboard_only_fallback_never_requests_type_access() {
        let mut backend = RecordingTextInputBackend::default();

        copy_dictation_text_with_backend("clipboard fallback", &mut backend)
            .expect("clipboard fallback");

        assert_eq!(backend.events, ["copy:clipboard fallback"]);
        assert_eq!(backend.clipboard, "clipboard fallback");
        assert!(!backend.can_type);
    }

    #[test]
    fn dictation_input_never_requests_access_when_the_preflight_changes() {
        let mut backend = RecordingTextInputBackend {
            grant_after_request: true,
            ..Default::default()
        };

        let error = input_text_without_access_request_with_backend(
            "clipboard fallback",
            ClipboardAfterInput::KeepInputText,
            &mut backend,
        )
        .expect_err("access changed after preflight");

        assert_eq!(backend.events, ["copy:clipboard fallback"]);
        assert_eq!(backend.clipboard, "clipboard fallback");
        assert_eq!(error, accessibility_permission_error());
    }

    #[test]
    fn dictation_preserve_mode_falls_back_to_clipboard_without_requesting_access() {
        let mut backend = RecordingTextInputBackend {
            clipboard: "previous clipboard".to_string(),
            grant_after_request: true,
            ..Default::default()
        };

        let error = input_text_without_access_request_with_backend(
            "clipboard fallback",
            ClipboardAfterInput::PreservePrevious,
            &mut backend,
        )
        .expect_err("access changed after preflight");

        assert_eq!(backend.events, ["copy:clipboard fallback"]);
        assert_eq!(backend.clipboard, "clipboard fallback");
        assert_eq!(error, accessibility_permission_error());
    }

    #[test]
    fn preserve_mode_types_without_touching_the_clipboard() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            clipboard: "previous clipboard".to_string(),
            ..Default::default()
        };

        input_text_with_backend(
            "temporary text",
            ClipboardAfterInput::PreservePrevious,
            &mut backend,
        )
        .expect("dictation input");

        assert_eq!(backend.events, ["type:temporary text"]);
        assert_eq!(backend.clipboard, "previous clipboard");
        assert_eq!(backend.focused_text, "temporary text");
    }

    #[test]
    fn denied_preserve_mode_does_not_touch_the_clipboard() {
        let mut backend = RecordingTextInputBackend {
            clipboard: "previous clipboard".to_string(),
            ..Default::default()
        };

        let error = input_text_with_backend(
            "temporary text",
            ClipboardAfterInput::PreservePrevious,
            &mut backend,
        )
        .expect_err("denied");

        assert_eq!(backend.events, ["request-access"]);
        assert_eq!(backend.clipboard, "previous clipboard");
        assert_eq!(error, accessibility_permission_preserving_clipboard_error());
    }

    #[test]
    fn copy_mode_stops_before_typing_when_clipboard_write_fails() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            copy_error: Some("copy failed".to_string()),
            ..Default::default()
        };

        let error = input_text_with_backend(
            "temporary text",
            ClipboardAfterInput::KeepInputText,
            &mut backend,
        )
        .expect_err("copy failure");

        assert_eq!(backend.events, ["copy:temporary text"]);
        assert!(backend.focused_text.is_empty());
        assert_eq!(error, "copy failed");
    }

    #[test]
    fn copy_mode_keeps_transcript_on_clipboard_when_direct_input_fails() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            type_error: Some("input failed".to_string()),
            ..Default::default()
        };

        let error = input_text_with_backend(
            "temporary text",
            ClipboardAfterInput::KeepInputText,
            &mut backend,
        )
        .expect_err("input failure");

        assert_eq!(
            backend.events,
            ["copy:temporary text", "type:temporary text"]
        );
        assert_eq!(backend.clipboard, "temporary text");
        assert!(backend.focused_text.is_empty());
        assert_eq!(error, "input failed");
    }

    #[test]
    fn preserve_mode_keeps_existing_clipboard_when_direct_input_fails() {
        let mut backend = RecordingTextInputBackend {
            can_type: true,
            type_error: Some("input failed".to_string()),
            clipboard: "previous clipboard".to_string(),
            ..Default::default()
        };

        let error = input_text_with_backend(
            "temporary text",
            ClipboardAfterInput::PreservePrevious,
            &mut backend,
        )
        .expect_err("input failure");

        assert_eq!(backend.events, ["type:temporary text"]);
        assert_eq!(backend.clipboard, "previous clipboard");
        assert!(backend.focused_text.is_empty());
        assert_eq!(error, "input failed");
    }

    #[test]
    fn dictation_input_can_remove_only_trailing_whitespace() {
        assert_eq!(
            dictation_input_text("  hello \n\t", true),
            Some("  hello".to_string())
        );
        assert_eq!(
            dictation_input_text("  hello \n\t", false),
            Some("  hello \n\t".to_string())
        );
        assert_eq!(dictation_input_text(" \n\t", true), None);
    }

    #[test]
    fn dictation_clipboard_setting_maps_to_the_requested_final_state() {
        assert_eq!(
            dictation_clipboard_after_input(true),
            ClipboardAfterInput::KeepInputText
        );
        assert_eq!(
            dictation_clipboard_after_input(false),
            ClipboardAfterInput::PreservePrevious
        );
    }

    #[test]
    fn accessibility_error_names_wakenote_and_the_required_permission() {
        let error = accessibility_permission_error();
        assert!(error.contains("WakeNote"));
        assert!(error.contains("Accessibility"));
        assert!(error.contains("Dictation"));
    }
}
