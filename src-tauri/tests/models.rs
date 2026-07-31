use std::io::{Cursor, Read};

use wakenote::models::{
    ModelDescriptor, ModelDownloadRecord, ModelStatus, ModelStore, ModelStoreError,
};

struct CancelAfterFirstRead {
    store: ModelStore,
    model_id: String,
    data: Cursor<Vec<u8>>,
    cancelled: bool,
}

impl Read for CancelAfterFirstRead {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.data.read(buffer)?;
        if read > 0 && !self.cancelled {
            self.store
                .cancel_download(&self.model_id)
                .expect("cancel download");
            self.cancelled = true;
        }
        Ok(read)
    }
}

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
        download_progress: None,
        download_error: None,
    }
}

#[test]
fn model_store_loads_metadata_only_json_registry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    std::fs::write(
        tmp.path().join("model-registry.json"),
        r#"[
          {
            "id": "custom-local",
            "display_name": "Custom Local",
            "engine": "whisper.cpp",
            "provider_runtime": "whisper-rs",
            "download_url": null,
            "checksum_sha256": null,
            "size_mb": 42,
            "languages": ["en"],
            "speed_score": 7,
            "accuracy_score": 6,
            "offline": true
          }
        ]"#,
    )
    .expect("registry json");

    let registry = store.load_model_registry().expect("load registry");
    let model = registry.get("custom-local").expect("custom model");

    assert_eq!(model.display_name, "Custom Local");
    assert_eq!(model.engine, "whisper.cpp");
    assert_eq!(model.status, ModelStatus::Missing);
    assert_eq!(model.download_progress, None);
    assert_eq!(model.download_error, None);
}

#[test]
fn default_registry_includes_requested_asr_provider_models() {
    let registry = wakenote::models::default_model_registry();

    let parakeet = registry
        .get("parakeet-tdt-0.6b-v3")
        .expect("Parakeet V3 model");
    assert_eq!(parakeet.display_name, "Parakeet TDT 0.6B V3");
    assert_eq!(parakeet.provider_runtime, "sherpa-onnx");
    assert!(
        parakeet
            .download_url
            .as_deref()
            .is_some_and(|url| url.ends_with(".tar.bz2")),
        "Parakeet should acquire a sherpa-onnx archive via Download",
    );
    assert!(
        parakeet
            .languages
            .iter()
            .any(|language| language == "multi")
    );

    let sensevoice = registry.get("sensevoice-small").expect("SenseVoice model");
    assert_eq!(sensevoice.display_name, "SenseVoice Small");
    assert_eq!(sensevoice.provider_runtime, "sherpa-onnx");
    assert!(
        sensevoice
            .download_url
            .as_deref()
            .is_some_and(|url| url.ends_with(".tar.bz2")),
        "SenseVoice should acquire a sherpa-onnx archive via Download",
    );
    assert!(sensevoice.languages.iter().any(|language| language == "ko"));

    let nemotron = registry
        .get("nemotron-3.5-asr-streaming-0.6b")
        .expect("Nemotron 3.5 ASR model");
    assert_eq!(nemotron.display_name, "Nemotron 3.5 ASR Streaming 0.6B");
    assert_eq!(nemotron.engine, "NVIDIA");
    assert_eq!(nemotron.provider_runtime, "sherpa-onnx");
    assert!(
        nemotron
            .download_url
            .as_deref()
            .is_some_and(|url| url.ends_with(".tar.bz2")),
        "Nemotron should acquire a sherpa-onnx archive via Download",
    );
    assert!(nemotron.languages.iter().any(|language| language == "ko"));
    assert!(
        nemotron
            .languages
            .iter()
            .any(|language| language == "multi")
    );
    assert!(nemotron.offline);
}

#[test]
fn default_whisper_downloads_use_the_upstream_model_repository() {
    let registry = wakenote::models::default_model_registry();

    for model_id in [
        "whisper-small",
        "whisper-medium",
        "whisper-turbo",
        "whisper-large",
    ] {
        let model = registry.get(model_id).expect("default Whisper model");
        let url = model
            .download_url
            .as_deref()
            .expect("default Whisper download URL");
        assert!(
            url.starts_with("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/"),
            "{model_id} must download from the upstream whisper.cpp model repository: {url}",
        );
    }
}

#[test]
fn model_store_marks_external_command_models_ready_from_command_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let mut registry = vec![ModelDescriptor {
        id: "parakeet-tdt-0.6b-v3".to_string(),
        display_name: "Parakeet TDT 0.6B V3".to_string(),
        engine: "NVIDIA Parakeet".to_string(),
        provider_runtime: "external-command".to_string(),
        download_url: None,
        checksum_sha256: None,
        size_mb: 1200,
        languages: vec!["multi".to_string()],
        speed_score: 8,
        accuracy_score: 8,
        offline: true,
        status: ModelStatus::Missing,
        download_progress: None,
        download_error: None,
    }];

    store
        .refresh_statuses(&mut registry)
        .expect("refresh without command");
    assert_eq!(registry[0].status, ModelStatus::Missing);

    std::fs::write(
        tmp.path().join("parakeet-tdt-0.6b-v3.command"),
        "printf transcript",
    )
    .expect("command file");

    store
        .refresh_statuses(&mut registry)
        .expect("refresh with command");
    assert_eq!(registry[0].status, ModelStatus::Ready);
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
fn model_store_discovers_local_whisper_cpp_bins_without_json_registry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    std::fs::write(tmp.path().join("ggml-large-v3-turbo.bin"), b"turbo").expect("turbo model");
    std::fs::write(tmp.path().join("whisper-medium-q4_1.bin"), b"medium q4").expect("q4 model");
    std::fs::create_dir_all(tmp.path().join("sense-voice-int8")).expect("onnx dir");
    std::fs::write(
        tmp.path().join("sense-voice-int8").join("model.int8.onnx"),
        b"onnx",
    )
    .expect("onnx model");

    let registry = store.load_model_registry().expect("load registry");
    let turbo = registry
        .get("ggml-large-v3-turbo")
        .expect("local turbo model is discovered");
    let medium = registry
        .get("whisper-medium-q4_1")
        .expect("local medium q4 model is discovered");

    assert_eq!(turbo.provider_runtime, "whisper-rs");
    assert_eq!(turbo.engine, "whisper.cpp");
    assert_eq!(medium.provider_runtime, "whisper-rs");
    assert!(
        !registry.contains_key("sense-voice-int8"),
        "onnx model families are not advertised as selectable whisper-rs models",
    );
}

#[test]
fn model_store_delete_removes_model_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let path = store.model_path("whisper-small");
    std::fs::create_dir_all(path.parent().unwrap()).expect("models dir");
    std::fs::write(&path, b"model").expect("model file");
    store
        .record_download_status("whisper-small", ModelStatus::Ready, 5, Some(5), None)
        .expect("ready record");

    store.delete_model("whisper-small").expect("delete");

    assert!(!path.exists());
    assert!(
        !store
            .load_download_state()
            .expect("download state")
            .downloads
            .contains_key("whisper-small")
    );
}

#[test]
fn model_store_delete_clears_stale_download_error_without_model_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    std::fs::write(tmp.path().join("whisper-test.download"), b"partial").expect("partial download");
    store
        .record_download_status(
            "whisper-test",
            ModelStatus::Error,
            7,
            Some(42),
            Some("cancelled by user".to_string()),
        )
        .expect("error record");

    store
        .delete_model("whisper-test")
        .expect("delete stale state");

    assert!(!tmp.path().join("whisper-test.download").exists());
    assert!(
        !store
            .load_download_state()
            .expect("download state")
            .downloads
            .contains_key("whisper-test")
    );
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
    assert_eq!(
        store
            .load_download_state()
            .expect("download state")
            .downloads
            .get("whisper-test")
            .map(|record| (record.status, record.download_progress_percent())),
        Some((ModelStatus::Ready, Some(100)))
    );
}

#[test]
fn model_store_records_fetch_failure_as_download_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let model = descriptor("whisper-test", None);

    let error = store
        .download_model_with(&model, |_| {
            Err(ModelStoreError::Download("network unavailable".to_string()))
                as Result<Cursor<Vec<u8>>, ModelStoreError>
        })
        .expect_err("fetch failure should fail");

    assert!(error.to_string().contains("network unavailable"));
    let state = store.load_download_state().expect("download state");
    let record = state
        .downloads
        .get("whisper-test")
        .expect("failed download record");
    assert_eq!(record.status, ModelStatus::Error);
    assert_eq!(record.error.as_deref(), Some("network unavailable"));
}

#[test]
fn model_store_resumes_partial_download_from_temp_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let model = descriptor(
        "whisper-test",
        Some("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"),
    );
    std::fs::write(tmp.path().join("whisper-test.download"), b"hello ").expect("partial model");
    store
        .record_download_progress("whisper-test", 6, Some(11))
        .expect("record partial progress");

    let status = store
        .download_model_with(&model, |_| Ok(Cursor::new(b"world".to_vec())))
        .expect("resume model download");

    assert_eq!(status, ModelStatus::Ready);
    assert_eq!(
        std::fs::read(store.model_path("whisper-test")).expect("installed model"),
        b"hello world"
    );
    assert!(!tmp.path().join("whisper-test.download").exists());
    assert_eq!(
        store
            .load_download_state()
            .expect("download state")
            .downloads
            .get("whisper-test")
            .map(|record| (record.status, record.download_progress_percent())),
        Some((ModelStatus::Ready, Some(100)))
    );
}

#[test]
fn model_store_persists_download_progress_across_instances() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());

    let record = store
        .record_download_progress("whisper-test", 512, Some(1024))
        .expect("record progress");

    assert_eq!(
        record,
        ModelDownloadRecord {
            model_id: "whisper-test".to_string(),
            status: ModelStatus::Downloading,
            downloaded_bytes: 512,
            total_bytes: Some(1024),
            error: None,
        }
    );

    let reloaded = ModelStore::new(tmp.path());
    let state = reloaded.load_download_state().expect("download state");

    assert_eq!(
        state.downloads.get("whisper-test"),
        Some(&ModelDownloadRecord {
            model_id: "whisper-test".to_string(),
            status: ModelStatus::Downloading,
            downloaded_bytes: 512,
            total_bytes: Some(1024),
            error: None,
        })
    );
}

#[test]
fn model_download_progress_is_capped_at_one_hundred_percent() {
    let record = ModelDownloadRecord {
        model_id: "whisper-test".to_string(),
        status: ModelStatus::Downloading,
        downloaded_bytes: 1536,
        total_bytes: Some(1024),
        error: None,
    };

    assert_eq!(record.download_progress_percent(), Some(100));
}

#[test]
fn model_store_overlays_download_progress_on_model_registry_status() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let mut registry = vec![descriptor("whisper-test", None)];
    store
        .record_download_progress("whisper-test", 512, Some(1024))
        .expect("record progress");

    store
        .refresh_statuses(&mut registry)
        .expect("refresh statuses");

    assert_eq!(registry[0].status, ModelStatus::Downloading);
    assert_eq!(registry[0].download_progress, Some(50));
}

#[test]
fn model_store_does_not_trust_stale_ready_download_records_without_valid_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let good_checksum = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let mut registry = vec![
        descriptor("missing", Some(good_checksum)),
        descriptor("corrupt", Some(good_checksum)),
    ];
    std::fs::write(store.model_path("corrupt"), b"wrong").expect("corrupt model");
    store
        .record_download_status("missing", ModelStatus::Ready, 0, Some(0), None)
        .expect("stale missing ready record");
    store
        .record_download_status("corrupt", ModelStatus::Ready, 5, Some(5), None)
        .expect("stale corrupt ready record");

    store
        .refresh_statuses(&mut registry)
        .expect("refresh statuses");

    assert_eq!(registry[0].status, ModelStatus::Missing);
    assert_eq!(registry[0].download_progress, None);
    assert_eq!(registry[1].status, ModelStatus::Error);
    assert_eq!(registry[1].download_progress, None);
}

#[test]
fn model_store_rejects_download_when_available_disk_space_is_too_low() {
    let error = ModelStore::validate_download_space(2_048, 1_024)
        .expect_err("insufficient disk space should fail");

    assert!(matches!(
        error,
        ModelStoreError::InsufficientDiskSpace {
            required_bytes: 2_048,
            available_bytes: 1_024,
        }
    ));
}

#[test]
fn model_store_cancels_active_download_and_removes_partial_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = ModelStore::new(tmp.path());
    let model = descriptor("whisper-test", None);
    let reader = CancelAfterFirstRead {
        store: store.clone(),
        model_id: model.id.clone(),
        data: Cursor::new(vec![7; 128 * 1024]),
        cancelled: false,
    };

    let error = store
        .download_model_with(&model, |_| Ok(reader))
        .expect_err("cancelled download should fail");

    assert!(matches!(
        error,
        ModelStoreError::Cancelled { model_id } if model_id == "whisper-test"
    ));
    assert!(!store.model_path("whisper-test").exists());
    assert!(!tmp.path().join("whisper-test.download").exists());

    let state = store.load_download_state().expect("download state");
    let record = state
        .downloads
        .get("whisper-test")
        .expect("cancelled download record");
    assert_eq!(record.status, ModelStatus::Error);
    assert_eq!(record.error.as_deref(), Some("cancelled by user"));
}
