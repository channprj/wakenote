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

#[test]
fn model_store_installs_model_bytes_after_checksum_verification() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let model = descriptor(
        "whisper-test",
        Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
    );

    store
        .install_model_bytes(&model, b"hello")
        .expect("install model");

    assert_eq!(
        std::fs::read(store.model_path("whisper-test")).expect("installed model"),
        b"hello"
    );
    assert_eq!(
        store.verify_model(&model).expect("verify"),
        ModelStatus::Ready
    );
}

#[test]
fn model_store_rejects_checksum_mismatch_without_installing_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let model = descriptor(
        "whisper-test",
        Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
    );

    let error = store
        .install_model_bytes(&model, b"wrong")
        .expect_err("checksum mismatch should fail");

    assert!(error.to_string().contains("model checksum mismatch"));
    assert!(!store.model_path("whisper-test").exists());
}

#[test]
fn model_store_downloads_with_fetcher_and_installs_ready_model() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let model = descriptor(
        "whisper-test",
        Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"),
    );

    let status = store
        .download_model_with(&model, |_| Ok(std::io::Cursor::new(b"hello".to_vec())))
        .expect("download model");

    assert_eq!(status, ModelStatus::Ready);
    assert_eq!(
        std::fs::read(store.model_path("whisper-test")).expect("installed model"),
        b"hello"
    );
}
