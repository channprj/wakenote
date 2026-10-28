use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use chrono::Local;
use tempfile::tempdir;
use wakenote::debug_log;

/// Tests here switch the process-wide diagnostic log, so they run one at a time.
static GLOBAL_LOG: Mutex<()> = Mutex::new(());

fn today_log(root: &std::path::Path) -> String {
    let path = debug_log::log_path_for(root, Local::now().date_naive());
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn diagnostic_log_keeps_concurrent_lines_whole() {
    let _guard = GLOBAL_LOG.lock().unwrap_or_else(PoisonError::into_inner);
    let tmp = tempdir().expect("tempdir");
    debug_log::configure(Some(tmp.path().to_path_buf()));

    let handles = (0..32)
        .map(|index| std::thread::spawn(move || debug_log::event(format!("message-{index:02}"))))
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().expect("thread joined");
    }
    debug_log::configure(None);
    debug_log::flush(Duration::from_secs(2));

    let contents = today_log(tmp.path());
    let lines = contents
        .lines()
        .filter(|line| line.contains("message-"))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 32, "{contents}");
    let today = Local::now().format("%Y-%m-%dT").to_string();
    for line in lines {
        assert!(line.starts_with(&today), "line: {line}");
        assert_eq!(line.matches("message-").count(), 1, "line: {line}");
    }
}
