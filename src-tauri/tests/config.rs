use serde_json::Value;

#[test]
fn macos_bundle_declares_microphone_permission_usage() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("tauri.conf.json");
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).expect("tauri config"))
            .expect("valid tauri config json");

    assert_eq!(config["bundle"]["active"], true);
    assert_eq!(config["productName"], "WakeNote");
    assert_eq!(config["app"]["windows"][0]["title"], "WakeNote");
    assert!(
        config["bundle"]["targets"]
            .as_array()
            .expect("bundle targets")
            .iter()
            .any(|target| target.as_str() == Some("app"))
    );
    assert!(
        config["bundle"]["icon"]
            .as_array()
            .expect("bundle icons")
            .iter()
            .any(|icon| icon.as_str() == Some("icons/icon.icns"))
    );

    let info_plist = std::fs::read_to_string(manifest_dir.join("Info.plist")).expect("Info.plist");
    assert!(info_plist.contains("NSMicrophoneUsageDescription"));
    assert!(info_plist.contains("WakeNote uses microphone access"));
    assert!(info_plist.contains("voice-activated recording"));
    assert!(info_plist.contains("LSRequiresCarbon"));
    assert!(info_plist.contains("<false/>"));
}

#[test]
fn macos_bundle_uses_regular_windowing_policy() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("tauri.conf.json");
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).expect("tauri config"))
            .expect("valid tauri config json");

    assert_ne!(config["app"]["macOSPrivateApi"], true);
}
