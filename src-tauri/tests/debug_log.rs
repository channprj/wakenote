use chrono::{Local, TimeZone};
use std::sync::Arc;
use tempfile::tempdir;
use wakenote::debug_log::{append_debug_log_at, debug_log_path_for};

#[test]
fn debug_log_path_uses_save_root_debug_yymmdd_log() {
    let tmp = tempdir().expect("tempdir");
    let date = chrono::NaiveDate::from_ymd_opt(2026, 6, 11).expect("date");

    assert_eq!(
        debug_log_path_for(tmp.path(), date),
        tmp.path().join("debug").join("260611.log")
    );
}

#[test]
fn append_debug_log_creates_daily_file_and_appends_lines() {
    let tmp = tempdir().expect("tempdir");
    let now = Local
        .with_ymd_and_hms(2026, 6, 11, 2, 3, 4)
        .single()
        .expect("local timestamp");

    let path = append_debug_log_at(tmp.path(), now, "source capture started").expect("first log");
    append_debug_log_at(tmp.path(), now, "source capture stopped").expect("second log");

    assert_eq!(path, tmp.path().join("debug").join("260611.log"));
    let contents = std::fs::read_to_string(path).expect("log contents");
    assert!(contents.contains("source capture started"));
    assert!(contents.contains("source capture stopped"));
    assert_eq!(contents.lines().count(), 2);
}

#[test]
fn append_debug_log_serializes_concurrent_writes() {
    let tmp = tempdir().expect("tempdir");
    let root = Arc::new(tmp.path().to_path_buf());
    let now = Local
        .with_ymd_and_hms(2026, 6, 11, 2, 3, 4)
        .single()
        .expect("local timestamp");

    let handles = (0..32)
        .map(|index| {
            let root = Arc::clone(&root);
            std::thread::spawn(move || {
                append_debug_log_at(&*root, now, format!("message-{index:02}"))
                    .expect("append log");
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("thread joined");
    }

    let contents =
        std::fs::read_to_string(root.join("debug").join("260611.log")).expect("log contents");
    let lines = contents.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 32);
    for line in lines {
        assert!(line.starts_with("2026-06-11T02:03:04"));
        assert_eq!(line.matches("message-").count(), 1, "line: {line}");
    }
}
