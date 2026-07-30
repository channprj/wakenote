#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::ProtocolObject;
#[cfg(target_os = "macos")]
use objc2::{AnyThread, msg_send};
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSArray, NSData, NSString};

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
pub enum ClipboardAfterPaste {
    KeepInputText,
    RestorePrevious,
}

pub const fn dictation_clipboard_after_paste(copy_to_clipboard: bool) -> ClipboardAfterPaste {
    if copy_to_clipboard {
        ClipboardAfterPaste::KeepInputText
    } else {
        ClipboardAfterPaste::RestorePrevious
    }
}

pub fn type_text_into_focused_cursor(text: &str) -> Result<(), String> {
    type_text_into_focused_cursor_with_clipboard(text, ClipboardAfterPaste::KeepInputText)
}

pub fn type_text_into_focused_cursor_with_clipboard(
    text: &str,
    clipboard_after_paste: ClipboardAfterPaste,
) -> Result<(), String> {
    if !should_type_transcript_text(text) {
        return Ok(());
    }

    type_text_into_focused_cursor_platform(text, clipboard_after_paste)
}

trait ClipboardPasteBackend {
    type Snapshot;

    fn snapshot_clipboard(&mut self) -> Result<Self::Snapshot, String>;
    fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String>;
    fn can_paste(&self) -> bool;
    fn request_paste_access(&mut self);
    fn paste_clipboard(&mut self) -> Result<(), String>;
    fn wait_for_paste(&mut self);
    fn restore_clipboard(&mut self, snapshot: Self::Snapshot) -> Result<(), String>;
}

fn copy_and_paste_with_backend(
    text: &str,
    clipboard_after_paste: ClipboardAfterPaste,
    backend: &mut impl ClipboardPasteBackend,
) -> Result<(), String> {
    match clipboard_after_paste {
        ClipboardAfterPaste::KeepInputText => {
            backend.copy_to_clipboard(text)?;
            if !backend.can_paste() {
                backend.request_paste_access();
                if !backend.can_paste() {
                    return Err(accessibility_permission_error().to_string());
                }
            }
            backend.paste_clipboard()
        }
        ClipboardAfterPaste::RestorePrevious => {
            if !backend.can_paste() {
                backend.request_paste_access();
                if !backend.can_paste() {
                    return Err(accessibility_permission_preserving_clipboard_error().to_string());
                }
            }

            let snapshot = backend.snapshot_clipboard()?;
            if let Err(copy_error) = backend.copy_to_clipboard(text) {
                return restore_after_error(backend, snapshot, copy_error);
            }
            if let Err(paste_error) = backend.paste_clipboard() {
                return restore_after_error(backend, snapshot, paste_error);
            }
            backend.wait_for_paste();
            backend.restore_clipboard(snapshot)
        }
    }
}

fn restore_after_error<B: ClipboardPasteBackend>(
    backend: &mut B,
    snapshot: B::Snapshot,
    operation_error: String,
) -> Result<(), String> {
    match backend.restore_clipboard(snapshot) {
        Ok(()) => Err(operation_error),
        Err(restore_error) => Err(format!(
            "{operation_error}; restoring the previous clipboard also failed: {restore_error}"
        )),
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct MacClipboardSnapshot {
    items: Vec<Vec<(String, Vec<u8>)>>,
}

#[cfg(target_os = "macos")]
struct MacClipboardPasteBackend;

#[cfg(target_os = "macos")]
impl ClipboardPasteBackend for MacClipboardPasteBackend {
    type Snapshot = MacClipboardSnapshot;

    fn snapshot_clipboard(&mut self) -> Result<Self::Snapshot, String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let mut snapshot = MacClipboardSnapshot { items: Vec::new() };
        let Some(items) = pasteboard.pasteboardItems() else {
            return Ok(snapshot);
        };

        for item in items.iter() {
            let mut representations = Vec::new();
            for pasteboard_type in item.types().iter() {
                let data = item.dataForType(&pasteboard_type).ok_or_else(|| {
                    format!("Could not read clipboard data of type {}", pasteboard_type)
                })?;
                representations.push((pasteboard_type.to_string(), data.to_vec()));
            }
            snapshot.items.push(representations);
        }

        Ok(snapshot)
    }

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

    fn wait_for_paste(&mut self) {
        // The key event is delivered asynchronously. Keep the temporary text
        // on the pasteboard long enough for the focused app to consume it.
        thread::sleep(Duration::from_millis(120));
    }

    fn restore_clipboard(&mut self, snapshot: Self::Snapshot) -> Result<(), String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        if snapshot.items.is_empty() {
            pasteboard.clearContents();
            return Ok(());
        }

        let mut items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> =
            Vec::with_capacity(snapshot.items.len());
        for representations in snapshot.items {
            let item: Retained<NSPasteboardItem> =
                unsafe { msg_send![NSPasteboardItem::alloc(), init] };
            for (pasteboard_type, bytes) in representations {
                let pasteboard_type = NSString::from_str(&pasteboard_type);
                let data = NSData::with_bytes(&bytes);
                if !item.setData_forType(&data, &pasteboard_type) {
                    return Err(format!(
                        "Could not restore clipboard data of type {pasteboard_type}"
                    ));
                }
            }
            items.push(ProtocolObject::from_retained(item));
        }
        let items = NSArray::from_retained_slice(&items);
        pasteboard.clearContents();
        if !pasteboard.writeObjects(&items) {
            return Err("Could not restore the previous clipboard".to_string());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn type_text_into_focused_cursor_platform(
    text: &str,
    clipboard_after_paste: ClipboardAfterPaste,
) -> Result<(), String> {
    static CLIPBOARD_INPUT: OnceLock<Mutex<()>> = OnceLock::new();
    let _operation = CLIPBOARD_INPUT
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|error| format!("Could not lock the clipboard text input: {error}"))?;
    copy_and_paste_with_backend(text, clipboard_after_paste, &mut MacClipboardPasteBackend)
}

#[cfg(not(target_os = "macos"))]
fn type_text_into_focused_cursor_platform(
    _text: &str,
    _clipboard_after_paste: ClipboardAfterPaste,
) -> Result<(), String> {
    Err("automatic transcript input is only supported on macOS".to_string())
}

pub fn accessibility_permission_error() -> &'static str {
    "The Dictation result was copied to the clipboard. WakeNote needs Accessibility permission to paste it. Open System Settings → Privacy & Security → Accessibility and enable WakeNote."
}

pub fn accessibility_permission_preserving_clipboard_error() -> &'static str {
    "WakeNote needs Accessibility permission to paste the Dictation result while preserving the clipboard. Open System Settings → Privacy & Security → Accessibility and enable WakeNote."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    struct NativeClipboardRestoreGuard(Option<MacClipboardSnapshot>);

    #[cfg(target_os = "macos")]
    impl Drop for NativeClipboardRestoreGuard {
        fn drop(&mut self) {
            if let Some(snapshot) = self.0.take() {
                let _ = MacClipboardPasteBackend.restore_clipboard(snapshot);
            }
        }
    }

    #[derive(Default)]
    struct RecordingClipboardPasteBackend {
        can_paste: bool,
        grant_after_request: bool,
        copy_error: Option<String>,
        paste_error: Option<String>,
        restore_error: Option<String>,
        events: Vec<String>,
    }

    impl ClipboardPasteBackend for RecordingClipboardPasteBackend {
        type Snapshot = String;

        fn snapshot_clipboard(&mut self) -> Result<Self::Snapshot, String> {
            self.events.push("snapshot".to_string());
            Ok("previous clipboard".to_string())
        }

        fn copy_to_clipboard(&mut self, text: &str) -> Result<(), String> {
            self.events.push(format!("copy:{text}"));
            if let Some(error) = self.copy_error.clone() {
                return Err(error);
            }
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
            if let Some(error) = self.paste_error.clone() {
                return Err(error);
            }
            Ok(())
        }

        fn wait_for_paste(&mut self) {
            self.events.push("wait-for-paste".to_string());
        }

        fn restore_clipboard(&mut self, snapshot: Self::Snapshot) -> Result<(), String> {
            self.events.push(format!("restore:{snapshot}"));
            if let Some(error) = self.restore_error.clone() {
                return Err(error);
            }
            Ok(())
        }
    }

    #[test]
    fn dictation_copies_transcript_before_pasting() {
        let mut backend = RecordingClipboardPasteBackend {
            can_paste: true,
            ..Default::default()
        };

        copy_and_paste_with_backend(
            "마이크 테스트",
            ClipboardAfterPaste::KeepInputText,
            &mut backend,
        )
        .expect("clipboard paste");

        assert_eq!(backend.events, ["copy:마이크 테스트", "paste"]);
    }

    #[test]
    fn denied_paste_access_still_leaves_transcript_on_clipboard() {
        let mut backend = RecordingClipboardPasteBackend::default();

        let error = copy_and_paste_with_backend(
            "clipboard fallback",
            ClipboardAfterPaste::KeepInputText,
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
    fn restore_mode_pastes_then_restores_the_previous_clipboard() {
        let mut backend = RecordingClipboardPasteBackend {
            can_paste: true,
            ..Default::default()
        };

        copy_and_paste_with_backend(
            "temporary text",
            ClipboardAfterPaste::RestorePrevious,
            &mut backend,
        )
        .expect("clipboard paste");

        assert_eq!(
            backend.events,
            [
                "snapshot",
                "copy:temporary text",
                "paste",
                "wait-for-paste",
                "restore:previous clipboard",
            ]
        );
    }

    #[test]
    fn denied_restore_mode_does_not_touch_the_clipboard() {
        let mut backend = RecordingClipboardPasteBackend::default();

        let error = copy_and_paste_with_backend(
            "temporary text",
            ClipboardAfterPaste::RestorePrevious,
            &mut backend,
        )
        .expect_err("denied");

        assert_eq!(backend.events, ["request-access"]);
        assert_eq!(error, accessibility_permission_preserving_clipboard_error());
    }

    #[test]
    fn restore_mode_recovers_the_clipboard_after_copy_failure() {
        let mut backend = RecordingClipboardPasteBackend {
            can_paste: true,
            copy_error: Some("copy failed".to_string()),
            ..Default::default()
        };

        let error = copy_and_paste_with_backend(
            "temporary text",
            ClipboardAfterPaste::RestorePrevious,
            &mut backend,
        )
        .expect_err("copy failure");

        assert_eq!(
            backend.events,
            [
                "snapshot",
                "copy:temporary text",
                "restore:previous clipboard",
            ]
        );
        assert_eq!(error, "copy failed");
    }

    #[test]
    fn restore_mode_recovers_the_clipboard_after_paste_failure() {
        let mut backend = RecordingClipboardPasteBackend {
            can_paste: true,
            paste_error: Some("paste failed".to_string()),
            ..Default::default()
        };

        let error = copy_and_paste_with_backend(
            "temporary text",
            ClipboardAfterPaste::RestorePrevious,
            &mut backend,
        )
        .expect_err("paste failure");

        assert_eq!(
            backend.events,
            [
                "snapshot",
                "copy:temporary text",
                "paste",
                "restore:previous clipboard",
            ]
        );
        assert_eq!(error, "paste failed");
    }

    #[test]
    fn restore_failure_is_reported_after_a_successful_paste() {
        let mut backend = RecordingClipboardPasteBackend {
            can_paste: true,
            restore_error: Some("restore failed".to_string()),
            ..Default::default()
        };

        let error = copy_and_paste_with_backend(
            "temporary text",
            ClipboardAfterPaste::RestorePrevious,
            &mut backend,
        )
        .expect_err("restore failure");

        assert_eq!(error, "restore failed");
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
            dictation_clipboard_after_paste(true),
            ClipboardAfterPaste::KeepInputText
        );
        assert_eq!(
            dictation_clipboard_after_paste(false),
            ClipboardAfterPaste::RestorePrevious
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn paste_shortcut_uses_layout_independent_ansi_v_keycode() {
        assert_eq!(macos_paste_keycode(), 0x09);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "mutates the live macOS pasteboard and restores it before returning"]
    fn macos_pasteboard_snapshot_round_trip_preserves_items_and_types() {
        let mut backend = MacClipboardPasteBackend;
        let original = backend.snapshot_clipboard().expect("original clipboard");
        let _restore_original = NativeClipboardRestoreGuard(Some(original.clone()));
        let expected = MacClipboardSnapshot {
            items: vec![
                vec![
                    (
                        "com.wakenote.test.first".to_string(),
                        b"first representation".to_vec(),
                    ),
                    (
                        "com.wakenote.test.second".to_string(),
                        vec![0, 1, 2, 3, 255],
                    ),
                ],
                vec![(
                    "com.wakenote.test.third".to_string(),
                    b"second item".to_vec(),
                )],
            ],
        };

        backend
            .restore_clipboard(expected.clone())
            .expect("write test clipboard");
        let snapshot = backend
            .snapshot_clipboard()
            .expect("snapshot test clipboard");
        backend
            .copy_to_clipboard("temporary Dictation text")
            .expect("replace clipboard");
        backend
            .restore_clipboard(snapshot)
            .expect("restore test clipboard");

        assert_eq!(
            backend.snapshot_clipboard().expect("restored clipboard"),
            expected
        );
        backend
            .restore_clipboard(original.clone())
            .expect("restore original clipboard");
        assert_eq!(
            backend
                .snapshot_clipboard()
                .expect("original clipboard restored"),
            original
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
