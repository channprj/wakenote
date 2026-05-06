use sagwan::models::{ModelDescriptor, ModelStatus, ModelStore};

fn descriptor(id: &str, checksum_sha256: Option<&str>) -> ModelDescriptor {
    ModelDescriptor {
        id: id.to_string(),
        display_name: id.to_string(),
        engine: "whisper.cpp".to_string(),
        provider_runtime: "whisper-rs".to_string(),
        download_url: None,
        checksum_sha256: checksum_sha256.map(str::to_string),
        size_mb: 1,
        languages: vec!["en".to_string()],
        speed_score: 5,
        accuracy_score: 5,
        offline: true,
        status: ModelStatus::Missing,
    }
}

#[test]
fn model_store_marks_missing_ready_and_error_states_from_local_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let good_checksum = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let mut registry = vec![
        descriptor("good", Some(good_checksum)),
        descriptor("bad", Some(good_checksum)),
        descriptor("missing", Some(good_checksum)),
    ];
    std::fs::write(store.model_path("good"), b"hello").expect("good model");
    std::fs::write(store.model_path("bad"), b"wrong").expect("bad model");

    store
        .refresh_statuses(&mut registry)
        .expect("refresh statuses");

    assert_eq!(registry[0].status, ModelStatus::Ready);
    assert_eq!(registry[1].status, ModelStatus::Error);
    assert_eq!(registry[2].status, ModelStatus::Missing);
}

#[test]
fn model_store_delete_removes_model_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let path = store.model_path("whisper-tiny");
    std::fs::create_dir_all(path.parent().unwrap()).expect("models dir");
    std::fs::write(&path, b"model").expect("model file");

    store.delete_model("whisper-tiny").expect("delete");

    assert!(!path.exists());
}
