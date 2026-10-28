use wakenote::permissions::{
    PermissionGrantStatus, microphone_permission_settings_request,
    microphone_permission_state_for_status,
};

#[test]
fn microphone_permission_state_requests_before_decision_and_opens_settings_after_denial() {
    for (status, label, can_request, can_open_settings) in [
        (
            PermissionGrantStatus::NotDetermined,
            "Not requested",
            true,
            false,
        ),
        (PermissionGrantStatus::Denied, "Denied", false, true),
    ] {
        let state = microphone_permission_state_for_status(status);

        assert_eq!(state.label, label, "{status:?}");
        assert_eq!(state.can_request, can_request, "{status:?}");
        assert_eq!(state.can_open_settings, can_open_settings, "{status:?}");
        if status == PermissionGrantStatus::Denied {
            assert!(state.detail.contains("System Settings"));
        }
    }
}

#[test]
fn microphone_permission_settings_request_targets_macos_microphone_privacy() {
    let request = microphone_permission_settings_request();

    assert_eq!(request.program, std::path::PathBuf::from("/usr/bin/open"));
    assert!(request.target.contains("Privacy_Microphone"));
}
