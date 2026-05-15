use wakenote::permissions::{
    PermissionGrantStatus, microphone_permission_settings_request,
    microphone_permission_state_for_status,
};

#[test]
fn microphone_permission_state_allows_request_only_before_first_decision() {
    let state = microphone_permission_state_for_status(PermissionGrantStatus::NotDetermined);

    assert_eq!(state.label, "Not requested");
    assert!(state.can_request);
    assert!(!state.can_open_settings);
}

#[test]
fn microphone_permission_state_opens_settings_after_denial() {
    let state = microphone_permission_state_for_status(PermissionGrantStatus::Denied);

    assert_eq!(state.label, "Denied");
    assert!(!state.can_request);
    assert!(state.can_open_settings);
    assert!(state.detail.contains("System Settings"));
}

#[test]
fn microphone_permission_settings_request_targets_macos_microphone_privacy() {
    let request = microphone_permission_settings_request();

    assert_eq!(request.program, std::path::PathBuf::from("/usr/bin/open"));
    assert!(request.target.contains("Privacy_Microphone"));
}
