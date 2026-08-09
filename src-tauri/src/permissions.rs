use std::{path::PathBuf, process::Command};

use serde::{Deserialize, Serialize};

pub const PERMISSION_GUIDANCE_EVENT: &str = "permission-guidance-required";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGuidanceFeature {
    DictationRecording,
    DictationInsertion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGuidanceKind {
    Accessibility,
    Microphone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGuidancePayload {
    pub feature: PermissionGuidanceFeature,
    pub permission: PermissionGuidanceKind,
}

impl PermissionGuidancePayload {
    pub const fn dictation_recording() -> Self {
        Self {
            feature: PermissionGuidanceFeature::DictationRecording,
            permission: PermissionGuidanceKind::Microphone,
        }
    }

    pub const fn dictation_insertion() -> Self {
        Self {
            feature: PermissionGuidanceFeature::DictationInsertion,
            permission: PermissionGuidanceKind::Accessibility,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantStatus {
    Unknown,
    NotDetermined,
    Granted,
    Denied,
    Restricted,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionState {
    pub status: PermissionGrantStatus,
    pub label: &'static str,
    pub detail: &'static str,
    pub can_request: bool,
    pub can_open_settings: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppPermissions {
    pub accessibility: PermissionState,
    pub microphone: PermissionState,
    pub screen_recording: PermissionState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionSettingsRequest {
    pub program: PathBuf,
    pub target: String,
}

pub fn permission_snapshot() -> AppPermissions {
    AppPermissions {
        accessibility: accessibility_permission_state_for_status(accessibility_permission_status()),
        microphone: microphone_permission_state_for_status(microphone_permission_status()),
        screen_recording: screen_recording_permission_state_for_status(
            screen_recording_permission_status(),
        ),
    }
}

pub fn request_microphone_permission() -> AppPermissions {
    AppPermissions {
        accessibility: accessibility_permission_state_for_status(accessibility_permission_status()),
        microphone: microphone_permission_state_for_status(request_microphone_access_if_needed()),
        screen_recording: screen_recording_permission_state_for_status(
            screen_recording_permission_status(),
        ),
    }
}

pub fn request_screen_recording_permission() -> AppPermissions {
    AppPermissions {
        accessibility: accessibility_permission_state_for_status(accessibility_permission_status()),
        microphone: microphone_permission_state_for_status(microphone_permission_status()),
        screen_recording: screen_recording_permission_state_for_status(
            request_screen_recording_access(),
        ),
    }
}

pub fn request_accessibility_permission() -> AppPermissions {
    AppPermissions {
        accessibility: accessibility_permission_state_for_status(
            request_accessibility_access_if_needed(),
        ),
        microphone: microphone_permission_state_for_status(microphone_permission_status()),
        screen_recording: screen_recording_permission_state_for_status(
            screen_recording_permission_status(),
        ),
    }
}

pub fn open_accessibility_permission_settings() -> Result<(), String> {
    let request = accessibility_permission_settings_request();
    Command::new(request.program)
        .arg(request.target)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn accessibility_permission_settings_request() -> PermissionSettingsRequest {
    PermissionSettingsRequest {
        program: PathBuf::from("/usr/bin/open"),
        target: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            .to_string(),
    }
}

pub fn accessibility_permission_state_for_status(status: PermissionGrantStatus) -> PermissionState {
    match status {
        PermissionGrantStatus::Granted => PermissionState {
            status,
            label: "Allowed",
            detail: "WakeNote can type Dictation results into the focused app.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::NotDetermined | PermissionGrantStatus::Denied => PermissionState {
            status,
            label: "Needs access",
            detail: "WakeNote needs Accessibility access to type Dictation results at the cursor.",
            can_request: true,
            can_open_settings: true,
        },
        PermissionGrantStatus::Restricted => PermissionState {
            status,
            label: "Restricted",
            detail: "Accessibility access is restricted by macOS policy.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::Unsupported => PermissionState {
            status,
            label: "Unsupported",
            detail: "This platform does not expose Accessibility permission status.",
            can_request: false,
            can_open_settings: false,
        },
        PermissionGrantStatus::Unknown => PermissionState {
            status,
            label: "Unknown",
            detail: "WakeNote could not determine Accessibility permission status.",
            can_request: true,
            can_open_settings: true,
        },
    }
}

#[cfg(target_os = "macos")]
fn accessibility_permission_status() -> PermissionGrantStatus {
    if accessibility_access_is_granted() {
        PermissionGrantStatus::Granted
    } else {
        PermissionGrantStatus::NotDetermined
    }
}

#[cfg(not(target_os = "macos"))]
fn accessibility_permission_status() -> PermissionGrantStatus {
    PermissionGrantStatus::Unsupported
}

#[cfg(target_os = "macos")]
fn request_accessibility_access_if_needed() -> PermissionGrantStatus {
    if request_accessibility_access() {
        PermissionGrantStatus::Granted
    } else {
        PermissionGrantStatus::Denied
    }
}

#[cfg(not(target_os = "macos"))]
fn request_accessibility_access_if_needed() -> PermissionGrantStatus {
    PermissionGrantStatus::Unsupported
}

#[cfg(target_os = "macos")]
use core_foundation::{
    base::TCFType,
    boolean::CFBoolean,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::{CFString, CFStringRef},
};

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;

    #[link_name = "kAXTrustedCheckOptionPrompt"]
    static AX_TRUSTED_CHECK_OPTION_PROMPT: CFStringRef;
}

#[cfg(target_os = "macos")]
pub fn accessibility_access_is_granted() -> bool {
    accessibility_access_is_granted_with_prompt(false)
}

#[cfg(target_os = "macos")]
pub fn request_accessibility_access() -> bool {
    accessibility_access_is_granted_with_prompt(true)
}

#[cfg(target_os = "macos")]
fn accessibility_access_is_granted_with_prompt(open_prompt: bool) -> bool {
    // SAFETY: The key is a process-lifetime Core Foundation constant. The
    // get-rule wrapper balances its temporary retain when dropped.
    let prompt_key = unsafe { CFString::wrap_under_get_rule(AX_TRUSTED_CHECK_OPTION_PROMPT) };
    let prompt_value = if open_prompt {
        CFBoolean::true_value()
    } else {
        CFBoolean::false_value()
    };
    let options = CFDictionary::from_CFType_pairs(&[(prompt_key, prompt_value)]);

    // SAFETY: AXIsProcessTrustedWithOptions only reads this valid dictionary
    // during the call. This is also the permission gate used by Enigo, the
    // native input backend that emits Dictation's Command+V.
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

pub fn open_screen_recording_permission_settings() -> Result<(), String> {
    let request = screen_recording_permission_settings_request();
    Command::new(request.program)
        .arg(request.target)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn screen_recording_permission_settings_request() -> PermissionSettingsRequest {
    PermissionSettingsRequest {
        program: PathBuf::from("/usr/bin/open"),
        target: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            .to_string(),
    }
}

pub fn screen_recording_permission_state_for_status(
    status: PermissionGrantStatus,
) -> PermissionState {
    match status {
        PermissionGrantStatus::Granted => PermissionState {
            status,
            label: "Allowed",
            detail: "WakeNote can capture system audio (Google Meet, YouTube).",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::NotDetermined => PermissionState {
            status,
            label: "Needs access",
            detail: "Enable Screen & System Audio Recording for WakeNote in System Settings, then restart WakeNote if it was just reinstalled.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::Denied => PermissionState {
            status,
            label: "Denied",
            detail: "Enable Screen Recording for WakeNote in System Settings.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::Restricted => PermissionState {
            status,
            label: "Restricted",
            detail: "Screen Recording access is restricted by macOS policy.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::Unsupported => PermissionState {
            status,
            label: "Unsupported",
            detail: "This platform does not expose Screen Recording permission status.",
            can_request: false,
            can_open_settings: false,
        },
        PermissionGrantStatus::Unknown => PermissionState {
            status,
            label: "Unknown",
            detail: "WakeNote could not determine Screen Recording permission status.",
            can_request: false,
            can_open_settings: true,
        },
    }
}

pub fn open_microphone_permission_settings() -> Result<(), String> {
    let request = microphone_permission_settings_request();
    Command::new(request.program)
        .arg(request.target)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn microphone_permission_settings_request() -> PermissionSettingsRequest {
    PermissionSettingsRequest {
        program: PathBuf::from("/usr/bin/open"),
        target: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            .to_string(),
    }
}

pub fn microphone_permission_state_for_status(status: PermissionGrantStatus) -> PermissionState {
    match status {
        PermissionGrantStatus::Granted => PermissionState {
            status,
            label: "Allowed",
            detail: "WakeNote can use the microphone.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::NotDetermined => PermissionState {
            status,
            label: "Not requested",
            detail: "WakeNote needs microphone access before recording can start.",
            can_request: true,
            can_open_settings: false,
        },
        PermissionGrantStatus::Denied => PermissionState {
            status,
            label: "Denied",
            detail: "Enable Microphone for WakeNote in System Settings.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::Restricted => PermissionState {
            status,
            label: "Restricted",
            detail: "Microphone access is restricted by macOS policy.",
            can_request: false,
            can_open_settings: true,
        },
        PermissionGrantStatus::Unsupported => PermissionState {
            status,
            label: "Unsupported",
            detail: "This platform does not expose microphone permission status.",
            can_request: false,
            can_open_settings: false,
        },
        PermissionGrantStatus::Unknown => PermissionState {
            status,
            label: "Unknown",
            detail: "WakeNote could not determine microphone permission status.",
            can_request: false,
            can_open_settings: true,
        },
    }
}

#[cfg(target_os = "macos")]
fn microphone_permission_status() -> PermissionGrantStatus {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
        return PermissionGrantStatus::Unknown;
    };
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };

    if status == AVAuthorizationStatus::Authorized {
        PermissionGrantStatus::Granted
    } else if status == AVAuthorizationStatus::Denied {
        PermissionGrantStatus::Denied
    } else if status == AVAuthorizationStatus::Restricted {
        PermissionGrantStatus::Restricted
    } else if status == AVAuthorizationStatus::NotDetermined {
        PermissionGrantStatus::NotDetermined
    } else {
        PermissionGrantStatus::Unknown
    }
}

#[cfg(not(target_os = "macos"))]
fn microphone_permission_status() -> PermissionGrantStatus {
    PermissionGrantStatus::Unsupported
}

#[cfg(target_os = "macos")]
fn request_microphone_access_if_needed() -> PermissionGrantStatus {
    use std::{sync::mpsc, time::Duration};

    use block2::{DynBlock, RcBlock};
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};

    if microphone_permission_status() != PermissionGrantStatus::NotDetermined {
        return microphone_permission_status();
    }

    let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
        return PermissionGrantStatus::Unknown;
    };
    let (sender, receiver) = mpsc::channel();
    let block = RcBlock::new(move |granted: Bool| {
        let _ = sender.send(granted.as_bool());
    });
    let block: &DynBlock<dyn Fn(Bool)> = &block;

    unsafe {
        AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, block);
    }

    match receiver.recv_timeout(Duration::from_secs(120)) {
        Ok(_) => microphone_permission_status(),
        Err(_) => PermissionGrantStatus::Unknown,
    }
}

#[cfg(not(target_os = "macos"))]
fn request_microphone_access_if_needed() -> PermissionGrantStatus {
    PermissionGrantStatus::Unsupported
}

#[cfg(target_os = "macos")]
fn screen_recording_permission_status() -> PermissionGrantStatus {
    use objc2_core_graphics::CGPreflightScreenCaptureAccess;

    // Preflight reports granted-or-not without prompting. It can't tell "denied"
    // apart from "never asked", so a non-grant maps to NotDetermined and the UI
    // offers a request (which is a no-op prompt if the user already decided).
    if CGPreflightScreenCaptureAccess() {
        PermissionGrantStatus::Granted
    } else {
        PermissionGrantStatus::NotDetermined
    }
}

#[cfg(not(target_os = "macos"))]
fn screen_recording_permission_status() -> PermissionGrantStatus {
    PermissionGrantStatus::Unsupported
}

#[cfg(target_os = "macos")]
fn request_screen_recording_access() -> PermissionGrantStatus {
    use objc2_core_graphics::CGRequestScreenCaptureAccess;

    // Prompts on first call and returns whether access is granted; later calls
    // return the current grant state without re-prompting.
    if CGRequestScreenCaptureAccess() {
        PermissionGrantStatus::Granted
    } else {
        PermissionGrantStatus::Denied
    }
}

#[cfg(not(target_os = "macos"))]
fn request_screen_recording_access() -> PermissionGrantStatus {
    PermissionGrantStatus::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_guidance_payloads_serialize_closed_identifiers() {
        assert_eq!(
            serde_json::to_value(PermissionGuidancePayload::dictation_recording()).unwrap(),
            serde_json::json!({
                "feature": "dictation_recording",
                "permission": "microphone",
            })
        );
        assert_eq!(
            serde_json::to_value(PermissionGuidancePayload::dictation_insertion()).unwrap(),
            serde_json::json!({
                "feature": "dictation_insertion",
                "permission": "accessibility",
            })
        );
    }

    #[test]
    fn screen_recording_granted_state_is_not_requestable() {
        let state = screen_recording_permission_state_for_status(PermissionGrantStatus::Granted);
        assert_eq!(state.status, PermissionGrantStatus::Granted);
        assert!(!state.can_request);
        assert!(state.can_open_settings);
    }

    #[test]
    fn screen_recording_not_determined_points_to_settings_without_native_request() {
        let state =
            screen_recording_permission_state_for_status(PermissionGrantStatus::NotDetermined);
        assert_eq!(state.label, "Needs access");
        assert!(state.detail.contains("System Settings"));
        assert!(!state.can_request);
        assert!(state.can_open_settings);
    }

    #[test]
    fn screen_recording_denied_points_to_settings() {
        let state = screen_recording_permission_state_for_status(PermissionGrantStatus::Denied);
        assert!(!state.can_request);
        assert!(state.can_open_settings);
    }

    #[test]
    fn screen_recording_settings_link_targets_screen_capture_pane() {
        let request = screen_recording_permission_settings_request();
        assert!(request.target.contains("Privacy_ScreenCapture"));
    }

    #[test]
    fn accessibility_permission_explains_dictation_input() {
        let state = accessibility_permission_state_for_status(PermissionGrantStatus::NotDetermined);
        assert!(state.detail.contains("Dictation"));
        assert!(state.can_request);
        assert!(state.can_open_settings);
    }

    #[test]
    fn accessibility_settings_link_targets_accessibility_pane() {
        let request = accessibility_permission_settings_request();
        assert!(request.target.contains("Privacy_Accessibility"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn accessibility_check_matches_native_text_input_permission() {
        let settings = enigo::Settings {
            open_prompt_to_get_permissions: false,
            ..Default::default()
        };

        assert_eq!(
            accessibility_access_is_granted(),
            enigo::Enigo::new(&settings).is_ok()
        );
    }
}
