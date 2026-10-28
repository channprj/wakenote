//! Opt-in diagnostic log. While the "Diagnostic logs" setting is on, error and
//! event lines are appended to `<save root>/logs/YYYY/MM/YYMMDD.log`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{OnceLock, PoisonError, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDate};

static LOG_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static DISPATCHER: OnceLock<SyncSender<Command>> = OnceLock::new();

const QUEUE_CAPACITY: usize = 4096;
const WRITER_THREAD_NAME: &str = "wakenote-debug-log";
/// A collapsed run of duplicate lines is summarized once the log goes quiet
/// for this long, or at least this often while the duplicates keep coming.
const REPEAT_SUMMARY_IDLE: Duration = Duration::from_secs(5);
const REPEAT_SUMMARY_INTERVAL: Duration = Duration::from_secs(60);

/// Writes an event line to stderr and, while diagnostic logging is on, to the log file.
#[macro_export]
macro_rules! log_event {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        eprintln!("{message}");
        $crate::debug_log::event(message);
    }};
}

/// Writes an error line to stderr and, while diagnostic logging is on, to the log file.
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        let message = format!($($arg)*);
        eprintln!("{message}");
        $crate::debug_log::error(message);
    }};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Event,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Event => "EVENT",
            Level::Error => "ERROR",
        }
    }
}

struct Entry {
    root: PathBuf,
    timestamp: DateTime<Local>,
    level: Level,
    message: String,
}

enum Command {
    Write(Entry),
    Flush(mpsc::Sender<()>),
}

/// Points diagnostic logging at `save_root`, or turns it off with `None`.
pub fn configure(save_root: Option<PathBuf>) {
    let current = LOG_ROOT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if current == save_root {
        return;
    }
    if current.is_some() && save_root.is_none() {
        event("[diagnostics] logging disabled");
    }
    *LOG_ROOT.write().unwrap_or_else(PoisonError::into_inner) = save_root.clone();
    ENABLED.store(save_root.is_some(), Ordering::Release);
    if let Some(root) = save_root {
        event(format!(
            "[diagnostics] logging enabled version={} pid={} save_root={}",
            env!("CARGO_PKG_VERSION"),
            std::process::id(),
            root.display()
        ));
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

pub fn event(message: impl Into<String>) {
    record(Level::Event, message);
}

pub fn error(message: impl Into<String>) {
    record(Level::Error, message);
}

fn record(level: Level, message: impl Into<String>) {
    let Some(root) = current_root() else {
        return;
    };
    let dispatcher = DISPATCHER.get_or_init(|| spawn_writer(QUEUE_CAPACITY));
    dispatch(
        dispatcher,
        Command::Write(Entry {
            root,
            timestamp: Local::now(),
            level,
            message: message.into(),
        }),
    );
}

fn current_root() -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    LOG_ROOT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Records a panic synchronously, because the process may abort before the
/// writer thread drains its queue.
pub fn record_panic(info: &std::panic::PanicHookInfo<'_>) {
    let Some(root) = current_root() else {
        return;
    };
    let thread = thread::current();
    // Let earlier lines land first so the file stays in order, unless the
    // writer itself panicked and could never answer.
    if thread.name() != Some(WRITER_THREAD_NAME) {
        flush(Duration::from_millis(300));
    }
    let message = format!(
        "[panic] thread={} {info}\n{}",
        thread.name().unwrap_or("unnamed"),
        std::backtrace::Backtrace::force_capture()
    );
    let _ = append_line(&root, Local::now(), Level::Error, &message);
}

/// Waits briefly for queued lines to reach disk, e.g. right before exit.
pub fn flush(timeout: Duration) {
    let Some(dispatcher) = DISPATCHER.get() else {
        return;
    };
    let deadline = Instant::now() + timeout;
    let (done, finished) = mpsc::channel();
    let mut command = Command::Flush(done);
    loop {
        match dispatcher.try_send(command) {
            Ok(()) => break,
            Err(TrySendError::Full(returned)) if Instant::now() < deadline => {
                command = returned;
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return,
        }
    }
    let _ = finished.recv_timeout(deadline.saturating_duration_since(Instant::now()));
}

pub fn log_path_for(save_root: impl AsRef<Path>, date: NaiveDate) -> PathBuf {
    save_root
        .as_ref()
        .join("logs")
        .join(date.format("%Y").to_string())
        .join(date.format("%m").to_string())
        .join(format!("{}.log", date.format("%y%m%d")))
}

fn dispatch(dispatcher: &SyncSender<Command>, command: Command) {
    // Diagnostic logging must never stall audio callbacks or source watchers.
    // A bounded queue caps memory use; dropping a saturated log entry is safer
    // than applying backpressure to real-time capture.
    if let Err(TrySendError::Full(_)) = dispatcher.try_send(command) {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

fn spawn_writer(capacity: usize) -> SyncSender<Command> {
    let (sender, receiver) = sync_channel(capacity.max(1));
    let _ = thread::Builder::new()
        .name(WRITER_THREAD_NAME.to_string())
        .spawn(move || run_writer(receiver));
    sender
}

fn run_writer(receiver: Receiver<Command>) {
    // Retry loops can emit the same line many times per minute; collapse
    // consecutive duplicates so the log keeps its signal.
    let mut last: Option<Entry> = None;
    let mut repeats = Repeats::default();
    loop {
        let command = if repeats.count > 0 {
            match receiver.recv_timeout(REPEAT_SUMMARY_IDLE) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => {
                    repeats.summarize(last.take().as_ref());
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        };
        match command {
            Command::Write(entry) => {
                let dropped = DROPPED.swap(0, Ordering::Relaxed);
                if dropped > 0 {
                    let _ = append_line(
                        &entry.root,
                        entry.timestamp,
                        Level::Error,
                        &format!(
                            "[diagnostics] dropped {dropped} lines while the log queue was full"
                        ),
                    );
                }
                if last.as_ref().is_some_and(|previous| {
                    previous.level == entry.level
                        && previous.message == entry.message
                        && previous.root == entry.root
                }) {
                    if repeats.record() >= REPEAT_SUMMARY_INTERVAL {
                        repeats.summarize(last.as_ref());
                    }
                    continue;
                }
                repeats.summarize(last.as_ref());
                let _ = append_line(&entry.root, entry.timestamp, entry.level, &entry.message);
                last = Some(entry);
            }
            Command::Flush(done) => {
                repeats.summarize(last.as_ref());
                let _ = done.send(());
            }
        }
    }
}

#[derive(Default)]
struct Repeats {
    count: u64,
    since: Option<Instant>,
}

impl Repeats {
    /// Counts one more duplicate and returns how long this run has gone unsummarized.
    fn record(&mut self) -> Duration {
        self.count += 1;
        self.since.get_or_insert_with(Instant::now).elapsed()
    }

    fn summarize(&mut self, line: Option<&Entry>) {
        if let Some(line) = line
            && self.count > 0
        {
            let _ = append_line(
                &line.root,
                Local::now(),
                line.level,
                &format!(
                    "[diagnostics] repeated {} more times: {}",
                    self.count, line.message
                ),
            );
        }
        *self = Self::default();
    }
}

fn append_line(
    save_root: &Path,
    timestamp: DateTime<Local>,
    level: Level,
    message: &str,
) -> std::io::Result<PathBuf> {
    let path = log_path_for(save_root, timestamp.date_naive());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // One write per line keeps concurrent appends (writer thread and panic hook) whole.
    let line = format!(
        "{} {} {}\n",
        timestamp.format("%Y-%m-%dT%H:%M:%S%.3f%:z"),
        level.label(),
        message.trim_end().replace('\n', "\n    ")
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?
        .write_all(line.as_bytes())?;
    Ok(path)
}

/// Serializes tests that switch the process-wide log configuration.
#[cfg(test)]
pub(crate) static GLOBAL_CONFIG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry(root: &Path, level: Level, message: &str) -> Command {
        Command::Write(Entry {
            root: root.to_path_buf(),
            timestamp: Local::now(),
            level,
            message: message.to_string(),
        })
    }

    fn flush_writer(sender: &SyncSender<Command>) {
        let (done, finished) = mpsc::channel();
        sender.send(Command::Flush(done)).expect("queue flush");
        finished
            .recv_timeout(Duration::from_secs(2))
            .expect("writer flushed");
    }

    fn today_log(root: &Path) -> String {
        std::fs::read_to_string(log_path_for(root, Local::now().date_naive())).unwrap_or_default()
    }

    #[test]
    fn log_files_are_grouped_by_year_and_month() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert_eq!(
            log_path_for("/archive", date),
            PathBuf::from("/archive/logs/2026/03/260309.log")
        );
    }

    #[test]
    fn lines_carry_timestamp_level_and_indented_continuations() {
        let root = tempfile::tempdir().unwrap();
        let timestamp = Local.with_ymd_and_hms(2026, 10, 24, 9, 5, 7).unwrap();
        let path = append_line(
            root.path(),
            timestamp,
            Level::Error,
            "[x] failed\nstack frame",
        )
        .expect("append");
        append_line(root.path(), timestamp, Level::Event, "[x] next").expect("append");

        assert_eq!(path, root.path().join("logs/2026/10/261024.log"));
        let text = std::fs::read_to_string(path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("2026-10-24T09:05:07.000"));
        assert!(lines[0].ends_with(" ERROR [x] failed"));
        assert_eq!(lines[1], "    stack frame");
        assert!(lines[2].ends_with(" EVENT [x] next"));
    }

    #[test]
    fn writer_collapses_consecutive_duplicate_lines() {
        let root = tempfile::tempdir().unwrap();
        let sender = spawn_writer(16);
        for _ in 0..4 {
            sender
                .send(entry(root.path(), Level::Error, "[mic] retry failed"))
                .unwrap();
        }
        sender
            .send(entry(root.path(), Level::Event, "[mic] recovered"))
            .unwrap();
        flush_writer(&sender);

        let text = today_log(root.path());
        // A parallel test may have bumped the process-wide dropped counter.
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.contains("[diagnostics] dropped"))
            .collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].ends_with("ERROR [mic] retry failed"));
        assert!(lines[1].ends_with("[diagnostics] repeated 3 more times: [mic] retry failed"));
        assert!(lines[2].ends_with("EVENT [mic] recovered"));
    }

    #[test]
    fn stalled_writer_never_blocks_capture_callers() {
        let (sender, receiver) = sync_channel(1);
        sender
            .try_send(Command::Flush(mpsc::channel().0))
            .expect("fill the bounded queue");

        let (returned_tx, returned_rx) = mpsc::channel();
        let caller = thread::spawn(move || {
            dispatch(&sender, entry(Path::new("/tmp"), Level::Event, "dropped"));
            returned_tx.send(()).unwrap();
        });

        returned_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("caller returned while the queue was full");
        caller.join().unwrap();
        drop(receiver);
    }

    #[test]
    fn configuration_switches_file_output_on_and_off() {
        let _lock = GLOBAL_CONFIG_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let root = tempfile::tempdir().unwrap();

        configure(None);
        event("[test] before enabling");
        configure(Some(root.path().to_path_buf()));
        assert!(enabled());
        event("[test] while enabled");
        crate::log_error!("[test] macro error {}", 7);
        configure(None);
        assert!(!enabled());
        event("[test] after disabling");
        flush(Duration::from_secs(2));

        let text = today_log(root.path());
        assert!(text.contains("[diagnostics] logging enabled"), "{text}");
        assert!(text.contains("EVENT [test] while enabled"), "{text}");
        assert!(text.contains("ERROR [test] macro error 7"), "{text}");
        assert!(text.contains("[diagnostics] logging disabled"), "{text}");
        assert!(!text.contains("before enabling"), "{text}");
        assert!(!text.contains("after disabling"), "{text}");
    }

    #[test]
    fn panics_are_written_synchronously_with_their_location() {
        let _lock = GLOBAL_CONFIG_TEST_LOCK
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let root = tempfile::tempdir().unwrap();
        configure(Some(root.path().to_path_buf()));
        // Chain to the previous hook so parallel test failures still report.
        let previous_hook: std::sync::Arc<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync> =
            std::panic::take_hook().into();
        let chained = previous_hook.clone();
        std::panic::set_hook(Box::new(move |info| {
            record_panic(info);
            chained(info);
        }));

        let panicked = thread::Builder::new()
            .name("panicking-worker".to_string())
            .spawn(|| panic!("worker invariant broken"))
            .unwrap()
            .join();

        drop(std::panic::take_hook());
        std::panic::set_hook(Box::new(move |info| previous_hook(info)));
        configure(None);
        flush(Duration::from_secs(2));
        assert!(panicked.is_err());
        let text = today_log(root.path());
        assert!(
            text.contains("ERROR [panic] thread=panicking-worker"),
            "{text}"
        );
        assert!(text.contains("worker invariant broken"), "{text}");
        assert!(text.contains("src/debug_log.rs"), "{text}");
    }
}
