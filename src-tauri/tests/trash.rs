#[cfg(target_os = "macos")]
#[test]
fn macos_trash_moves_a_file_out_of_place_and_returns_its_recoverable_location() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let source = tmp.path().join("wakenote-trash-test.wav");
    std::fs::write(&source, b"temporary audio").expect("audio fixture");

    let trashed = wakenote::trash::move_to_trash(&source).expect("move to macOS Trash");

    assert!(!source.exists());
    assert!(trashed.exists());

    std::fs::remove_file(trashed).expect("remove test fixture from Trash");
}

#[test]
fn recording_bundle_moves_audio_text_metadata_and_error_as_one_recoverable_unit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let day = tmp.path().join("20260807");
    let fake_trash = tmp.path().join("Trash");
    std::fs::create_dir_all(&day).expect("day");
    std::fs::create_dir_all(&fake_trash).expect("fake Trash");
    let transcript = day.join("260807-120000-mic.txt");
    let expected = [
        transcript.with_extension("m4a"),
        transcript.clone(),
        transcript.with_extension("json"),
        transcript.with_extension("error.txt"),
    ];
    for path in &expected {
        std::fs::write(path, path.to_string_lossy().as_bytes()).expect("bundle fixture");
    }

    let bundle = wakenote::trash::move_recording_bundle_to_trash_with(&transcript, |stage_path| {
        let destination = fake_trash.join("WakeNote recording bundle");
        std::fs::rename(stage_path, &destination).map_err(|error| error.to_string())?;
        Ok(destination)
    })
    .expect("move bundle")
    .expect("bundle existed");

    for path in &expected {
        assert!(!path.exists(), "original remained: {}", path.display());
        assert!(
            bundle.trash_path.join(path.file_name().unwrap()).exists(),
            "Trash bundle omitted: {}",
            path.display()
        );
    }
    wakenote::trash::restore_recording_bundle(&bundle).expect("restore bundle");
    for path in &expected {
        assert!(path.exists(), "restore omitted: {}", path.display());
    }
}

#[test]
fn recording_bundle_rolls_back_when_trash_rejects_the_staged_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let transcript = tmp.path().join("260807-120000-mic.txt");
    let audio = transcript.with_extension("m4a");
    let metadata = transcript.with_extension("json");
    for path in [&transcript, &audio, &metadata] {
        std::fs::write(path, b"fixture").expect("bundle fixture");
    }

    let error = wakenote::trash::move_recording_bundle_to_trash_with(&transcript, |_stage_path| {
        Err("Trash unavailable".to_string())
    })
    .expect_err("Trash failure");

    assert!(error.contains("Trash unavailable"));
    assert!(transcript.exists());
    assert!(audio.exists());
    assert!(metadata.exists());
    assert!(
        std::fs::read_dir(tmp.path())
            .expect("tempdir entries")
            .all(|entry| !entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with("WakeNote recording bundle - "))
    );
}
