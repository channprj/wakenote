use serde_json::Value;

#[test]
fn macos_bundle_declares_microphone_permission_usage() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("tauri.conf.json");
    let config: Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).expect("tauri config"))
            .expect("valid tauri config json");

    assert_eq!(config["bundle"]["active"], true);
    assert!(
        config["bundle"]["targets"]
            .as_array()
            .expect("bundle targets")
            .iter()
            .any(|target| target.as_str() == Some("app"))
    );

    let info_plist = std::fs::read_to_string(manifest_dir.join("Info.plist")).expect("Info.plist");
    assert!(info_plist.contains("NSMicrophoneUsageDescription"));
    assert!(info_plist.contains("voice-activated recording"));
}
