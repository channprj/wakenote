//! Local delivery history. One worker owns SQLite; capture never waits for disk.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::settings::{LiveTranscriptionWebhookSettings, WebhookPayloadFormat};

pub const HISTORY_WINDOW_MS: i64 = 24 * 60 * 60 * 1_000;
pub const HISTORY_PAGE_SIZE: u32 = 50;
const MAILBOX_CAPACITY: usize = 8_192;
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
const UNAVAILABLE: &str = "Webhook history is unavailable. Delivery continues in the background.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryStatus {
    Queued,
    Sending,
    Retrying,
    Succeeded,
    Failed,
    Cancelled,
    Dropped,
}

impl HistoryStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sending => "sending",
            Self::Retrying => "retrying",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Dropped => "dropped",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "sending" => Ok(Self::Sending),
            "retrying" => Ok(Self::Retrying),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "dropped" => Ok(Self::Dropped),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }

    fn can_retry(self) -> bool {
        matches!(self, Self::Failed | Self::Dropped)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryFilter {
    #[default]
    All,
    NeedsRetry,
    Succeeded,
    InProgress,
    Cancelled,
}

impl HistoryFilter {
    fn predicate(self) -> &'static str {
        match self {
            Self::All => "1 = 1",
            Self::NeedsRetry => "status IN ('failed', 'dropped')",
            Self::Succeeded => "status = 'succeeded'",
            Self::InProgress => "status IN ('queued', 'sending', 'retrying')",
            Self::Cancelled => "status = 'cancelled'",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HistoryCounts {
    pub queued: u64,
    pub sending: u64,
    pub retrying: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub dropped: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub event_id: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub status: HistoryStatus,
    /// Origin only. Never expose credentials, path, or query through this DTO.
    pub endpoint_origin: String,
    pub payload_format: WebhookPayloadFormat,
    pub body_bytes: u64,
    pub attempt_count: u32,
    pub cycle_attempt: u8,
    pub manual_retries: u32,
    pub http_status: Option<u16>,
    pub error_kind: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub retry_blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub counts: HistoryCounts,
    pub page: u32,
    pub page_size: u32,
    pub page_count: u32,
    pub total: u64,
    pub now_ms: i64,
    pub storage_warning: bool,
}

pub(super) struct NewHistoryEntry {
    pub event_id: String,
    pub created_at_ms: i64,
    pub endpoint_url: String,
    pub payload_format: WebhookPayloadFormat,
    pub body: Arc<Vec<u8>>,
    pub status: HistoryStatus,
}

pub(super) struct HistoryUpdate {
    pub event_id: String,
    pub manual_retries: u32,
    pub status: HistoryStatus,
    pub attempt_count: u32,
    pub cycle_attempt: u8,
    pub http_status: Option<u16>,
    pub error_kind: Option<String>,
    pub elapsed_ms: Option<u64>,
}

pub(super) struct RetryRecord {
    pub event_id: String,
    pub created_at_ms: i64,
    pub endpoint_url: String,
    pub payload_format: WebhookPayloadFormat,
    pub body: Arc<Vec<u8>>,
    pub attempt_count: u32,
    pub manual_retries: u32,
    pub previous_status: HistoryStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrySkipped {
    pub event_id: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize)]
pub struct RetryResult {
    pub queued_ids: Vec<String>,
    pub skipped: Vec<RetrySkipped>,
}

pub(super) struct RetryClaim {
    pub records: Vec<RetryRecord>,
    pub skipped: Vec<RetrySkipped>,
}

enum Command {
    Insert(NewHistoryEntry),
    Update(HistoryUpdate),
    Page(
        u32,
        HistoryFilter,
        LiveTranscriptionWebhookSettings,
        i64,
        oneshot::Sender<Result<HistoryPage, String>>,
    ),
    Claim(
        Vec<String>,
        LiveTranscriptionWebhookSettings,
        i64,
        oneshot::Sender<Result<RetryClaim, String>>,
    ),
    Release(String, u32, HistoryStatus),
    Wake,
}

pub(super) struct HistoryStore {
    sender: mpsc::SyncSender<Command>,
    auto_delete: Arc<AtomicBool>,
    storage_warning: Arc<AtomicBool>,
}

impl HistoryStore {
    pub fn open(path: PathBuf, auto_delete: bool) -> Self {
        let (sender, receiver) = mpsc::sync_channel(MAILBOX_CAPACITY);
        let auto_delete = Arc::new(AtomicBool::new(auto_delete));
        let storage_warning = Arc::new(AtomicBool::new(false));
        let cleanup = auto_delete.clone();
        let warning = storage_warning.clone();
        // No join on Drop: application exit must never wait for history I/O.
        let _ = std::thread::Builder::new()
            .name("webhook-history".into())
            .spawn(move || history_worker(path, receiver, cleanup, warning));
        Self {
            sender,
            auto_delete,
            storage_warning,
        }
    }

    fn record(&self, command: Command) {
        if self.sender.try_send(command).is_err() {
            self.storage_warning.store(true, Ordering::Release);
        }
    }

    pub fn insert(&self, record: NewHistoryEntry) {
        self.record(Command::Insert(record));
    }

    pub fn update(&self, update: HistoryUpdate) {
        self.record(Command::Update(update));
    }

    pub fn configure(&self, auto_delete: bool) {
        self.auto_delete.store(auto_delete, Ordering::Release);
        // Atomic preference remains authoritative even if the mailbox is full.
        let _ = self.sender.try_send(Command::Wake);
    }

    pub async fn page(
        &self,
        page: u32,
        filter: HistoryFilter,
        settings: LiveTranscriptionWebhookSettings,
    ) -> Result<HistoryPage, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .try_send(Command::Page(
                page,
                filter,
                settings,
                chrono::Utc::now().timestamp_millis(),
                tx,
            ))
            .map_err(|_| UNAVAILABLE.to_string())?;
        let mut page = rx.await.map_err(|_| UNAVAILABLE.to_string())??;
        page.storage_warning |= self.storage_warning.load(Ordering::Acquire);
        Ok(page)
    }

    pub async fn claim(
        &self,
        ids: Vec<String>,
        settings: LiveTranscriptionWebhookSettings,
    ) -> Result<RetryClaim, String> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .try_send(Command::Claim(
                ids,
                settings,
                chrono::Utc::now().timestamp_millis(),
                tx,
            ))
            .map_err(|_| UNAVAILABLE.to_string())?;
        rx.await.map_err(|_| UNAVAILABLE.to_string())?
    }

    pub fn release(&self, record: &RetryRecord) {
        self.record(Command::Release(
            record.event_id.clone(),
            record.manual_retries,
            record.previous_status,
        ));
    }
}

fn history_worker(
    path: PathBuf,
    receiver: mpsc::Receiver<Command>,
    auto_delete: Arc<AtomicBool>,
    warning: Arc<AtomicBool>,
) {
    let mut database = Database::open(&path);
    if database.is_err() {
        warning.store(true, Ordering::Release);
    }
    let mut last_cleanup = Instant::now();
    let mut cleanup_was_enabled = false;
    loop {
        let cleanup_enabled = auto_delete.load(Ordering::Acquire);
        if cleanup_enabled && (!cleanup_was_enabled || last_cleanup.elapsed() >= CLEANUP_INTERVAL) {
            if let Ok(database) = &database
                && database
                    .cleanup(chrono::Utc::now().timestamp_millis())
                    .is_err()
            {
                warning.store(true, Ordering::Release);
            }
            last_cleanup = Instant::now();
        }
        cleanup_was_enabled = cleanup_enabled;
        let command = match receiver.recv_timeout(CLEANUP_INTERVAL) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let Ok(database) = &mut database else {
            match command {
                Command::Page(_, _, _, _, reply) => {
                    let _ = reply.send(Err(UNAVAILABLE.into()));
                }
                Command::Claim(_, _, _, reply) => {
                    let _ = reply.send(Err(UNAVAILABLE.into()));
                }
                _ => {}
            }
            continue;
        };
        let result = match command {
            Command::Insert(record) => database.insert(record),
            Command::Update(update) => database.update(update),
            Command::Release(id, cycle, status) => database.release(&id, cycle, status),
            Command::Page(page, filter, settings, now, reply) => {
                let _ = reply.send(
                    database
                        .page(page, filter, &settings, now)
                        .map_err(|_| UNAVAILABLE.to_string()),
                );
                Ok(())
            }
            Command::Claim(ids, settings, now, reply) => {
                // A missed result must never cause an ambiguous automatic resend.
                let result = if warning.load(Ordering::Acquire) {
                    Err(UNAVAILABLE.into())
                } else {
                    database
                        .claim(ids, &settings, now)
                        .map_err(|_| UNAVAILABLE.to_string())
                };
                let _ = reply.send(result);
                Ok(())
            }
            Command::Wake => Ok(()),
        };
        if result.is_err() {
            warning.store(true, Ordering::Release);
        }
    }
}

struct Database(Connection);

impl Database {
    fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| rusqlite::Error::InvalidPath(path.into()))?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(path)
            .map_err(|_| rusqlite::Error::InvalidPath(path.into()))?;
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA secure_delete=ON;")?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS webhook_history (
                event_id TEXT PRIMARY KEY,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                endpoint_url TEXT NOT NULL,
                payload_format TEXT NOT NULL,
                payload BLOB NOT NULL,
                status TEXT NOT NULL,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                cycle_attempt INTEGER NOT NULL DEFAULT 0,
                manual_retries INTEGER NOT NULL DEFAULT 0,
                http_status INTEGER,
                error_kind TEXT,
                elapsed_ms INTEGER
            );
            CREATE INDEX IF NOT EXISTS webhook_history_created ON webhook_history(created_at_ms DESC, event_id DESC);
            CREATE INDEX IF NOT EXISTS webhook_history_status_created ON webhook_history(status, created_at_ms DESC, event_id DESC);
            PRAGMA user_version=1;
            UPDATE webhook_history SET status='failed', error_kind='interrupted'
              WHERE status IN ('queued','sending','retrying');"
        )?;
        Ok(Self(connection))
    }

    fn insert(&self, record: NewHistoryEntry) -> rusqlite::Result<()> {
        self.0.execute(
            "INSERT OR IGNORE INTO webhook_history
             (event_id,created_at_ms,updated_at_ms,endpoint_url,payload_format,payload,status,error_kind)
             VALUES (?1,?2,?2,?3,?4,?5,?6,?7)",
            params![record.event_id, record.created_at_ms, record.endpoint_url, format_name(record.payload_format), record.body.as_slice(), record.status.as_str(), (record.status == HistoryStatus::Dropped).then_some("queue_full")],
        )?;
        Ok(())
    }

    fn update(&self, update: HistoryUpdate) -> rusqlite::Result<()> {
        self.0.execute(
            "UPDATE webhook_history SET updated_at_ms=?1,status=?2,attempt_count=?3,cycle_attempt=?4,http_status=?5,error_kind=?6,elapsed_ms=?7
             WHERE event_id=?8 AND manual_retries=?9",
            params![chrono::Utc::now().timestamp_millis(), update.status.as_str(), update.attempt_count, update.cycle_attempt, update.http_status, update.error_kind, update.elapsed_ms.map(|value| value.min(i64::MAX as u64) as i64), update.event_id, update.manual_retries],
        )?;
        Ok(())
    }

    fn cleanup(&self, now_ms: i64) -> rusqlite::Result<usize> {
        self.0.execute(
            "DELETE FROM webhook_history WHERE created_at_ms <= ?1",
            [now_ms - HISTORY_WINDOW_MS],
        )
    }

    fn page(
        &self,
        requested_page: u32,
        filter: HistoryFilter,
        settings: &LiveTranscriptionWebhookSettings,
        now_ms: i64,
    ) -> rusqlite::Result<HistoryPage> {
        let cutoff = now_ms - HISTORY_WINDOW_MS;
        let mut counts = HistoryCounts::default();
        let mut statement = self.0.prepare("SELECT status,count(*) FROM webhook_history WHERE created_at_ms > ?1 AND created_at_ms <= ?2 GROUP BY status")?;
        let groups = statement.query_map(params![cutoff, now_ms], |row| {
            Ok((
                HistoryStatus::parse(&row.get::<_, String>(0)?)?,
                row.get::<_, i64>(1)? as u64,
            ))
        })?;
        for group in groups {
            let (status, count) = group?;
            match status {
                HistoryStatus::Queued => counts.queued = count,
                HistoryStatus::Sending => counts.sending = count,
                HistoryStatus::Retrying => counts.retrying = count,
                HistoryStatus::Succeeded => counts.succeeded = count,
                HistoryStatus::Failed => counts.failed = count,
                HistoryStatus::Cancelled => counts.cancelled = count,
                HistoryStatus::Dropped => counts.dropped = count,
            }
        }
        let predicate = filter.predicate();
        let total: i64 = self.0.query_row(&format!("SELECT count(*) FROM webhook_history WHERE created_at_ms > ?1 AND created_at_ms <= ?2 AND {predicate}"), params![cutoff, now_ms], |row| row.get(0))?;
        let total = total as u64;
        let page_count = total
            .div_ceil(u64::from(HISTORY_PAGE_SIZE))
            .max(1)
            .min(u64::from(u32::MAX)) as u32;
        let page = requested_page.clamp(1, page_count);
        let mut statement = self.0.prepare(&format!(
            "SELECT event_id,created_at_ms,updated_at_ms,status,endpoint_url,payload_format,length(payload),attempt_count,cycle_attempt,manual_retries,http_status,error_kind,elapsed_ms
             FROM webhook_history WHERE created_at_ms > ?1 AND created_at_ms <= ?2 AND {predicate}
             ORDER BY created_at_ms DESC,event_id DESC LIMIT ?3 OFFSET ?4"
        ))?;
        let entries = statement
            .query_map(
                params![
                    cutoff,
                    now_ms,
                    HISTORY_PAGE_SIZE,
                    i64::from(page - 1) * i64::from(HISTORY_PAGE_SIZE)
                ],
                |row| {
                    let status = HistoryStatus::parse(&row.get::<_, String>(3)?)?;
                    let endpoint: String = row.get(4)?;
                    let format = parse_format(&row.get::<_, String>(5)?)?;
                    Ok(HistoryEntry {
                        event_id: row.get(0)?,
                        created_at_ms: row.get(1)?,
                        updated_at_ms: row.get(2)?,
                        status,
                        endpoint_origin: endpoint_origin(&endpoint),
                        payload_format: format,
                        body_bytes: row.get::<_, i64>(6)? as u64,
                        attempt_count: row.get(7)?,
                        cycle_attempt: row.get(8)?,
                        manual_retries: row.get(9)?,
                        http_status: row.get(10)?,
                        error_kind: row.get(11)?,
                        elapsed_ms: row.get::<_, Option<i64>>(12)?.map(|value| value as u64),
                        retry_blocked_reason: retry_blocked_reason(
                            status, &endpoint, format, settings,
                        )
                        .map(str::to_string),
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(HistoryPage {
            entries,
            counts,
            page,
            page_size: HISTORY_PAGE_SIZE,
            page_count,
            total,
            now_ms,
            storage_warning: false,
        })
    }

    fn claim(
        &mut self,
        ids: Vec<String>,
        settings: &LiveTranscriptionWebhookSettings,
        now_ms: i64,
    ) -> rusqlite::Result<RetryClaim> {
        let transaction = self.0.transaction()?;
        let mut result = RetryClaim {
            records: Vec::new(),
            skipped: Vec::new(),
        };
        for id in ids.into_iter().collect::<BTreeSet<_>>() {
            let record = transaction.query_row(
                "SELECT created_at_ms,endpoint_url,payload_format,payload,attempt_count,manual_retries,status FROM webhook_history WHERE event_id=?1",
                [&id], |row| Ok(RetryRecord {
                    event_id: id.clone(), created_at_ms: row.get(0)?, endpoint_url: row.get(1)?,
                    payload_format: parse_format(&row.get::<_, String>(2)?)?, body: Arc::new(row.get(3)?),
                    attempt_count: row.get(4)?, manual_retries: row.get::<_, u32>(5)? + 1,
                    previous_status: HistoryStatus::parse(&row.get::<_, String>(6)?)?,
                }),
            ).optional()?;
            let reason = match &record {
                None => Some("not_found"),
                Some(record)
                    if record.created_at_ms <= now_ms - HISTORY_WINDOW_MS
                        || record.created_at_ms > now_ms =>
                {
                    Some("expired")
                }
                Some(record) => retry_blocked_reason(
                    record.previous_status,
                    &record.endpoint_url,
                    record.payload_format,
                    settings,
                ),
            };
            if let Some(reason) = reason {
                result.skipped.push(RetrySkipped {
                    event_id: id,
                    reason: reason.into(),
                });
                continue;
            }
            if let Some(record) = record {
                transaction.execute("UPDATE webhook_history SET status='queued',manual_retries=?1 WHERE event_id=?2", params![record.manual_retries, id])?;
                result.records.push(record);
            }
        }
        transaction.commit()?;
        Ok(result)
    }

    fn release(&self, id: &str, cycle: u32, status: HistoryStatus) -> rusqlite::Result<()> {
        self.0.execute("UPDATE webhook_history SET status=?1,manual_retries=?2 WHERE event_id=?3 AND manual_retries=?4 AND status='queued'", params![status.as_str(), cycle.saturating_sub(1), id, cycle])?;
        Ok(())
    }
}

fn format_name(format: WebhookPayloadFormat) -> &'static str {
    match format {
        WebhookPayloadFormat::TextOnly => "text_only",
        WebhookPayloadFormat::Json => "json",
    }
}

fn parse_format(value: &str) -> rusqlite::Result<WebhookPayloadFormat> {
    match value {
        "text_only" => Ok(WebhookPayloadFormat::TextOnly),
        "json" => Ok(WebhookPayloadFormat::Json),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn endpoint_origin(endpoint: &str) -> String {
    reqwest::Url::parse(endpoint)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_else(|_| "Invalid endpoint".into())
}

fn retry_blocked_reason(
    status: HistoryStatus,
    endpoint: &str,
    format: WebhookPayloadFormat,
    settings: &LiveTranscriptionWebhookSettings,
) -> Option<&'static str> {
    if !status.can_retry() {
        Some("not_failed")
    } else if !settings.enabled {
        Some("disabled")
    } else if settings.endpoint_url != endpoint || settings.payload_format != format {
        Some("settings_changed")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
