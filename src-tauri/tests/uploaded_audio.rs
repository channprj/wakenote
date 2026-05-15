use chrono::TimeZone;
use wakenote::storage::{copy_uploaded_audio_file, uploaded_audio_target};

#[test]
fn uploaded_audio_target_uses_uploaded_local_date_and_original_filename() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let source_path = tmp.path().join("meeting.mp3");
    let timestamp = chrono::Local
        .with_ymd_and_hms(2026, 5, 16, 9, 30, 0)
        .unwrap();

    let target = uploaded_audio_target(tmp.path(), &source_path, timestamp).expect("target");

    assert!(target.ends_with("uploaded/20260516/meeting.mp3"));
}

#[test]
fn copy_uploaded_audio_file_preserves_source_and_avoids_name_collisions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let source_path = tmp.path().join("meeting.mp3");
    std::fs::write(&source_path, b"mp3 bytes").expect("source");
    let timestamp = chrono::Local
        .with_ymd_and_hms(2026, 5, 16, 9, 30, 0)
        .unwrap();

    let first = copy_uploaded_audio_file(tmp.path(), &source_path, timestamp).expect("first copy");
    let second =
        copy_uploaded_audio_file(tmp.path(), &source_path, timestamp).expect("second copy");

    assert!(first.ends_with("uploaded/20260516/meeting.mp3"));
    assert!(second.ends_with("uploaded/20260516/meeting-001.mp3"));
    assert_eq!(
        std::fs::read(&source_path).expect("source remains"),
        b"mp3 bytes"
    );
    assert_eq!(std::fs::read(&first).expect("first copied"), b"mp3 bytes");
    assert_eq!(std::fs::read(&second).expect("second copied"), b"mp3 bytes");
}

#[test]
fn copy_uploaded_audio_file_rejects_non_audio_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let source_path = tmp.path().join("notes.txt");
    std::fs::write(&source_path, b"text").expect("source");
    let timestamp = chrono::Local
        .with_ymd_and_hms(2026, 5, 16, 9, 30, 0)
        .unwrap();

    let error = copy_uploaded_audio_file(tmp.path(), &source_path, timestamp)
        .expect_err("txt should be rejected");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}
