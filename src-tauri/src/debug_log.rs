use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate};

pub fn debug_log_path_for(save_root: impl AsRef<Path>, date: NaiveDate) -> PathBuf {
    save_root
        .as_ref()
        .join("debug")
        .join(format!("{}.log", date.format("%y%m%d")))
}

pub fn append_debug_log(save_root: impl AsRef<Path>, message: impl AsRef<str>) {
    let _ = append_debug_log_at(save_root, Local::now(), message);
}

pub fn append_debug_log_at(
    save_root: impl AsRef<Path>,
    timestamp: DateTime<Local>,
    message: impl AsRef<str>,
) -> std::io::Result<PathBuf> {
    let path = debug_log_path_for(save_root, timestamp.date_naive());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    writeln!(file, "{} {}", timestamp.to_rfc3339(), message.as_ref())?;
    Ok(path)
}
