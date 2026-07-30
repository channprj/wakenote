#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

#[cfg(target_os = "macos")]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

#[cfg(target_os = "macos")]
const fn macos_paste_keycode() -> u16 {
    // kVK_ANSI_V. A shortcut must use the physical ANSI keycode instead of
    // asking the active input source to translate the character "v". Enigo's
    // character lookup falls back to keycode 0 (ANSI A) when translation
    // fails, which turns Command+V into Command+A.
    0x09
}

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

trait ClipboardPasteBackend {
    fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String>;
    fn can_paste(&self) -> bool;
    fn request_paste_access(&mut self);
    fn paste_clipboard(&mut self) -> Result<(), String>;
}

fn copy_and_paste_with_backend(
    text: &str,
    backend: &mut impl ClipboardPasteBackend,
) -> Result<(), String> {
    backend.copy_to_clipboard(text)?;
    if !backend.can_paste() {
        backend.request_paste_access();
        if !backend.can_paste() {
            return Err(accessibility_permission_error().to_string());
        }
    }
    backend.paste_clipboard()
}

#[cfg(target_os = "macos")]
struct MacClipboardPasteBackend;

#[cfg(target_os = "macos")]
impl ClipboardPasteBackend for MacClipboardPasteBackend {
    fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String> {
        let mut child = Command::new("/usr/bin/pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start clipboard copy: {error}"))?;
        let write_result = child
            .stdin
            .take()
            .ok_or_else(|| "Could not open clipboard input".to_string())
            .and_then(|mut stdin| {
                stdin
                    .write_all(text.as_bytes())
                    .map_err(|error| format!("Could not copy Dictation text: {error}"))
            });
        let status = child
            .wait()
            .map_err(|error| format!("Could not finish clipboard copy: {error}"))?;
        write_result?;
        if !status.success() {
            return Err(format!("pbcopy exited with status {status}"));
        }
        Ok(())
    }

    fn can_paste(&self) -> bool {
        crate::permissions::accessibility_access_is_granted()
    }

    fn request_paste_access(&mut self) {
        let _ = crate::permissions::request_accessibility_access();
    }

    fn paste_clipboard(&mut self) -> Result<(), String> {
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
        let input = input.as_mut().expect("native text input initialized");
        input
            .key(Key::Meta, Direction::Press)
            .map_err(|error| format!("Could not press Command for Dictation paste: {error}"))?;
        let paste_result = input
            .raw(macos_paste_keycode(), Direction::Click)
            .map_err(|error| format!("Could not press V for Dictation paste: {error}"));
        let release_result = input
            .key(Key::Meta, Direction::Release)
            .map_err(|error| format!("Could not release Command after Dictation paste: {error}"));
        paste_result.and(release_result)
    }
}

#[cfg(target_os = "macos")]
fn type_text_into_focused_cursor_platform(text: &str) -> Result<(), String> {
    copy_and_paste_with_backend(text, &mut MacClipboardPasteBackend)
}

#[cfg(not(target_os = "macos"))]
fn type_text_into_focused_cursor_platform(_text: &str) -> Result<(), String> {
    Err("automatic transcript input is only supported on macOS".to_string())
}

pub fn accessibility_permission_error() -> &'static str {
    "The Dictation result was copied to the clipboard. WakeNote needs Accessibility permission to paste it. Open System Settings → Privacy & Security → Accessibility and enable WakeNote."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingClipboardPasteBackend {
        can_paste: bool,
        grant_after_request: bool,
        events: Vec<String>,
    }

    impl ClipboardPasteBackend for RecordingClipboardPasteBackend {
        fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String> {
            self.events.push(format!("copy:{text}"));
            Ok(())
        }

        fn can_paste(&self) -> bool {
            self.can_paste
        }

        fn request_paste_access(&mut self) {
            self.events.push("request-access".to_string());
            self.can_paste = self.grant_after_request;
        }

        fn paste_clipboard(&mut self) -> Result<(), String> {
            self.events.push("paste".to_string());
            Ok(())
        }
    }

    #[test]
    fn dictation_copies_transcript_before_pasting() {
        let mut backend = RecordingClipboardPasteBackend {
            can_paste: true,
            ..Default::default()
        };

        copy_and_paste_with_backend("마이크 테스트", &mut backend).expect("clipboard paste");

        assert_eq!(backend.events, ["copy:마이크 테스트", "paste"]);
    }

    #[test]
    fn denied_paste_access_still_leaves_transcript_on_clipboard() {
        let mut backend = RecordingClipboardPasteBackend::default();

        let error =
            copy_and_paste_with_backend("clipboard fallback", &mut backend).expect_err("denied");

        assert_eq!(
            backend.events,
            ["copy:clipboard fallback", "request-access"]
        );
        assert_eq!(error, accessibility_permission_error());
        assert!(error.contains("copied"));
        assert!(error.contains("clipboard"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn paste_shortcut_uses_layout_independent_ansi_v_keycode() {
        assert_eq!(macos_paste_keycode(), 0x09);
    }

    #[test]
    fn accessibility_error_names_wakenote_and_the_required_permission() {
        let error = accessibility_permission_error();
        assert!(error.contains("WakeNote"));
        assert!(error.contains("Accessibility"));
        assert!(error.contains("Dictation"));
    }
}
