use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use chrono::{DateTime, Local, NaiveDate};

static DEBUG_LOG_WRITE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
static DEBUG_LOG_DISPATCHER: OnceLock<SyncSender<DebugLogEntry>> = OnceLock::new();

const DEBUG_LOG_QUEUE_CAPACITY: usize = 256;

struct DebugLogEntry {
    save_root: PathBuf,
    timestamp: DateTime<Local>,
    message: String,
}

fn spawn_debug_log_dispatcher<Writer>(
    capacity: usize,
    mut writer: Writer,
) -> SyncSender<DebugLogEntry>
where
    Writer: FnMut(DebugLogEntry) + Send + 'static,
{
    let (sender, receiver) = sync_channel(capacity.max(1));
    let _ = thread::Builder::new()
        .name("wakenote-debug-log".to_string())
        .spawn(move || {
            while let Ok(entry) = receiver.recv() {
                writer(entry);
            }
        });
    sender
}

fn dispatch_debug_log(dispatcher: &SyncSender<DebugLogEntry>, entry: DebugLogEntry) {
    // Diagnostic logging must never stall audio callbacks or source watchers.
    // A bounded queue caps memory use; dropping a saturated log entry is safer
    // than applying backpressure to real-time capture.
    let _ = dispatcher.try_send(entry);
}

fn debug_log_write_lock(path: &Path) -> Arc<Mutex<()>> {
    let mut locks = DEBUG_LOG_WRITE_LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub fn debug_log_path_for(save_root: impl AsRef<Path>, date: NaiveDate) -> PathBuf {
    save_root
        .as_ref()
        .join("debug")
        .join(format!("{}.log", date.format("%y%m%d")))
}

pub fn append_debug_log(save_root: impl AsRef<Path>, message: impl AsRef<str>) {
    let _ = append_debug_log_at(save_root, Local::now(), message);
}

pub fn append_debug_log_nonblocking(save_root: impl AsRef<Path>, message: impl AsRef<str>) {
    let dispatcher = DEBUG_LOG_DISPATCHER.get_or_init(|| {
        spawn_debug_log_dispatcher(DEBUG_LOG_QUEUE_CAPACITY, |entry| {
            let _ = append_debug_log_at(entry.save_root, entry.timestamp, entry.message);
        })
    });
    dispatch_debug_log(
        dispatcher,
        DebugLogEntry {
            save_root: save_root.as_ref().to_path_buf(),
            timestamp: Local::now(),
            message: message.as_ref().to_string(),
        },
    );
}

pub fn append_debug_log_at(
    save_root: impl AsRef<Path>,
    timestamp: DateTime<Local>,
    message: impl AsRef<str>,
) -> std::io::Result<PathBuf> {
    let path = debug_log_path_for(save_root, timestamp.date_naive());
    let write_lock = debug_log_write_lock(&path);
    let _guard = write_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn entry(message: &str) -> DebugLogEntry {
        DebugLogEntry {
            save_root: PathBuf::from("/tmp/wakenote-debug-log-test"),
            timestamp: Local::now(),
            message: message.to_string(),
        }
    }

    #[test]
    fn stalled_debug_log_writer_never_blocks_capture_callers() {
        let (writer_started_tx, writer_started_rx) = mpsc::channel();
        let (release_writer_tx, release_writer_rx) = mpsc::channel();
        let mut release_first_write = Some(release_writer_rx);
        let dispatcher = spawn_debug_log_dispatcher(1, move |_| {
            let Some(release_writer_rx) = release_first_write.take() else {
                return;
            };
            writer_started_tx.send(()).expect("writer started");
            release_writer_rx.recv().expect("release writer");
        });

        dispatch_debug_log(&dispatcher, entry("writer blocks on this entry"));
        writer_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("writer received first entry");
        dispatch_debug_log(&dispatcher, entry("fills bounded queue"));

        let (caller_returned_tx, caller_returned_rx) = mpsc::channel();
        let caller_dispatcher = dispatcher.clone();
        let caller = thread::spawn(move || {
            dispatch_debug_log(&caller_dispatcher, entry("dropped instead of blocking"));
            caller_returned_tx.send(()).expect("caller returned");
        });
        let returned = caller_returned_rx.recv_timeout(Duration::from_millis(100));
        release_writer_tx.send(()).expect("release writer");

        returned.expect("capture caller returned while log writer was stalled");
        caller.join().expect("caller completed");
    }

    #[test]
    fn stalled_log_path_does_not_lock_unrelated_log_paths() {
        let blocked = debug_log_write_lock(Path::new("/tmp/wakenote-blocked.log"));
        let unrelated = debug_log_write_lock(Path::new("/tmp/wakenote-unrelated.log"));
        let blocked_again = debug_log_write_lock(Path::new("/tmp/wakenote-blocked.log"));

        assert!(std::sync::Arc::ptr_eq(&blocked, &blocked_again));
        assert!(!std::sync::Arc::ptr_eq(&blocked, &unrelated));

        let blocked_guard = blocked.lock().expect("hold blocked path lock");
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let caller = thread::spawn(move || {
            let _unrelated_guard = unrelated.lock().expect("lock unrelated path");
            acquired_tx.send(()).expect("unrelated path acquired");
        });

        acquired_rx
            .recv_timeout(Duration::from_millis(100))
            .expect("unrelated log path was not blocked");
        drop(blocked_guard);
        caller.join().expect("unrelated caller completed");
    }
}
