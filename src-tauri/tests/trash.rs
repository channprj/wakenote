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
