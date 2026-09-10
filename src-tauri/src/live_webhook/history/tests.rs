use super::*;

fn config() -> LiveTranscriptionWebhookSettings {
    LiveTranscriptionWebhookSettings {
        enabled: true,
        endpoint_url: "https://user:secret@example.com/private?token=hidden".into(),
        payload_format: WebhookPayloadFormat::TextOnly,
        ..Default::default()
    }
}

fn entry(id: &str, created_at_ms: i64, status: HistoryStatus) -> NewHistoryEntry {
    NewHistoryEntry {
        event_id: id.into(),
        created_at_ms,
        endpoint_url: config().endpoint_url,
        payload_format: WebhookPayloadFormat::TextOnly,
        body: Arc::new("한글 🙂\nexact body".as_bytes().to_vec()),
        status,
    }
}

#[test]
fn pages_are_bounded_stable_filtered_and_hide_the_exact_24_hour_boundary() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open(&root.path().join("history.sqlite3")).unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    for index in 0..123 {
        database
            .insert(entry(
                &format!("id-{index:03}"),
                now - index,
                if index % 2 == 0 {
                    HistoryStatus::Failed
                } else {
                    HistoryStatus::Succeeded
                },
            ))
            .unwrap();
    }
    database
        .insert(entry(
            "boundary",
            now - HISTORY_WINDOW_MS,
            HistoryStatus::Failed,
        ))
        .unwrap();
    database
        .insert(entry(
            "older",
            now - HISTORY_WINDOW_MS - 1,
            HistoryStatus::Failed,
        ))
        .unwrap();
    database
        .insert(entry("future", now + 1, HistoryStatus::Failed))
        .unwrap();
    let first = database
        .page(0, HistoryFilter::All, &config(), now)
        .unwrap();
    let second = database
        .page(2, HistoryFilter::All, &config(), now)
        .unwrap();
    let last = database
        .page(u32::MAX, HistoryFilter::All, &config(), now)
        .unwrap();
    assert_eq!(
        (
            first.total,
            first.page,
            first.page_count,
            first.entries.len()
        ),
        (123, 1, 3, 50)
    );
    assert_eq!(second.entries.len(), 50);
    assert_eq!((last.page, last.entries.len()), (3, 23));
    let ids: BTreeSet<_> = first
        .entries
        .iter()
        .chain(&second.entries)
        .chain(&last.entries)
        .map(|entry| &entry.event_id)
        .collect();
    assert_eq!(ids.len(), 123);
    assert_eq!(first.entries[0].event_id, "id-000");
    assert_eq!(last.entries.last().unwrap().event_id, "id-122");
    let failed = database
        .page(1, HistoryFilter::NeedsRetry, &config(), now)
        .unwrap();
    assert_eq!(failed.total, 62);
    assert!(
        failed
            .entries
            .iter()
            .all(|entry| entry.status == HistoryStatus::Failed)
    );
    assert_eq!((failed.counts.failed, failed.counts.succeeded), (62, 61));
    let public = serde_json::to_string(&failed).unwrap();
    for secret in ["secret", "/private", "token=hidden", "exact body", "user:"] {
        assert!(!public.contains(secret));
    }
    assert!(public.contains("https://example.com"));
    assert!(
        failed
            .entries
            .iter()
            .all(|entry| entry.retry_blocked_reason.is_none())
    );
    let empty = database
        .page(99, HistoryFilter::Cancelled, &config(), now)
        .unwrap();
    assert_eq!((empty.page, empty.page_count, empty.total), (1, 1, 0));
}

#[test]
fn retries_are_claimed_once_keep_immutable_bytes_and_reject_expired_or_wrong_settings() {
    let root = tempfile::tempdir().unwrap();
    let mut database = Database::open(&root.path().join("history.sqlite3")).unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    for (id, status, age) in [
        ("failed", HistoryStatus::Failed, 1),
        ("dropped", HistoryStatus::Dropped, 2),
        ("success", HistoryStatus::Succeeded, 3),
        ("cancelled", HistoryStatus::Cancelled, 4),
        ("expired", HistoryStatus::Failed, HISTORY_WINDOW_MS),
    ] {
        database.insert(entry(id, now - age, status)).unwrap();
    }
    let claimed = database
        .claim(
            vec![
                "failed".into(),
                "failed".into(),
                "dropped".into(),
                "success".into(),
                "cancelled".into(),
                "expired".into(),
                "missing".into(),
            ],
            &config(),
            now,
        )
        .unwrap();
    assert_eq!(claimed.records.len(), 2);
    assert_eq!(claimed.skipped.len(), 4);
    let original = entry("unused", now, HistoryStatus::Failed);
    for record in &claimed.records {
        assert_eq!(record.body, original.body);
        assert_eq!(record.endpoint_url, config().endpoint_url);
        assert_eq!(record.manual_retries, 1);
        assert!(record.created_at_ms < now);
    }
    assert!(
        database
            .claim(vec!["failed".into()], &config(), now)
            .unwrap()
            .records
            .is_empty()
    );
    let record = &claimed.records[0];
    database
        .release(
            &record.event_id,
            record.manual_retries,
            record.previous_status,
        )
        .unwrap();
    let mut changed = config();
    changed.endpoint_url = "https://different.test".into();
    let result = database
        .claim(vec![record.event_id.clone()], &changed, now)
        .unwrap();
    assert_eq!(result.skipped[0].reason, "settings_changed");
    changed = config();
    changed.payload_format = WebhookPayloadFormat::Json;
    assert_eq!(
        database
            .claim(vec![record.event_id.clone()], &changed, now)
            .unwrap()
            .skipped[0]
            .reason,
        "settings_changed"
    );
    changed = config();
    changed.enabled = false;
    assert_eq!(
        database
            .claim(vec![record.event_id.clone()], &changed, now)
            .unwrap()
            .skipped[0]
            .reason,
        "disabled"
    );
    let retried = database
        .claim(vec![record.event_id.clone()], &config(), now)
        .unwrap();
    assert_eq!(
        retried.records[0].manual_retries, 1,
        "rejected admission rolls the claim back"
    );
}

#[test]
fn reopening_preserves_results_marks_interruptions_failed_and_never_resurrects_old_entries() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.sqlite3");
    let now = chrono::Utc::now().timestamp_millis();
    {
        let database = Database::open(&path).unwrap();
        for status in [
            HistoryStatus::Queued,
            HistoryStatus::Sending,
            HistoryStatus::Retrying,
            HistoryStatus::Succeeded,
            HistoryStatus::Failed,
            HistoryStatus::Cancelled,
        ] {
            database
                .insert(entry(status.as_str(), now - 1, status))
                .unwrap();
        }
        database
            .insert(entry(
                "expired",
                now - HISTORY_WINDOW_MS,
                HistoryStatus::Failed,
            ))
            .unwrap();
    }
    let database = Database::open(&path).unwrap();
    let page = database
        .page(1, HistoryFilter::All, &config(), now)
        .unwrap();
    assert_eq!(
        (
            page.total,
            page.counts.failed,
            page.counts.succeeded,
            page.counts.cancelled
        ),
        (6, 4, 1, 1)
    );
    for row in page
        .entries
        .iter()
        .filter(|entry| ["queued", "sending", "retrying"].contains(&entry.event_id.as_str()))
    {
        assert_eq!(row.error_kind.as_deref(), Some("interrupted"));
        assert_eq!(row.created_at_ms, now - 1);
    }
    assert_eq!(database.cleanup(now).unwrap(), 1);
    assert_eq!(
        database
            .0
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM webhook_history WHERE event_id='expired'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn cleanup_setting_is_independent_of_visibility_and_applies_without_new_deliveries() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("history.sqlite3");
    let now = chrono::Utc::now().timestamp_millis();
    let store = HistoryStore::open(path.clone(), false);
    store.insert(entry(
        "old-failure",
        now - HISTORY_WINDOW_MS - 1,
        HistoryStatus::Failed,
    ));
    store.insert(entry(
        "old-success",
        now - HISTORY_WINDOW_MS - 1,
        HistoryStatus::Succeeded,
    ));
    store.insert(entry("recent", now - 1, HistoryStatus::Failed));
    let page = store.page(1, HistoryFilter::All, config()).await.unwrap();
    assert_eq!(page.total, 1);
    let connection = Connection::open(&path).unwrap();
    let count = || {
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM webhook_history", [], |row| row.get(0))
            .unwrap()
    };
    assert_eq!(count(), 3, "disabled deletion keeps hidden history on disk");
    store.configure(true);
    store.page(1, HistoryFilter::All, config()).await.unwrap();
    assert_eq!(
        count(),
        1,
        "enabling cleanup removes expired rows and their payloads"
    );
    store.configure(false);
    store.insert(entry(
        "retained-again",
        now - HISTORY_WINDOW_MS - 2,
        HistoryStatus::Failed,
    ));
    assert_eq!(
        store
            .page(1, HistoryFilter::All, config())
            .await
            .unwrap()
            .total,
        1
    );
    assert_eq!(count(), 2);
}

#[tokio::test]
async fn storage_failure_returns_a_safe_error_without_panicking_or_exposing_private_paths() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("private-endpoint-secret");
    std::fs::create_dir(&path).unwrap();
    let store = HistoryStore::open(path, true);
    store.insert(entry(
        "failed",
        chrono::Utc::now().timestamp_millis(),
        HistoryStatus::Failed,
    ));
    let error = store
        .page(1, HistoryFilter::All, config())
        .await
        .unwrap_err();
    assert_eq!(error, UNAVAILABLE);
    assert!(!error.contains("private-endpoint-secret"));
    assert!(store.claim(vec!["failed".into()], config()).await.is_err());
}

#[test]
fn an_old_cycle_cannot_overwrite_a_claimed_manual_retry_or_reset_its_age() {
    let root = tempfile::tempdir().unwrap();
    let mut database = Database::open(&root.path().join("history.sqlite3")).unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    database
        .insert(entry("event", now - 20, HistoryStatus::Failed))
        .unwrap();
    database
        .claim(vec!["event".into()], &config(), now)
        .unwrap();
    database
        .update(HistoryUpdate {
            event_id: "event".into(),
            manual_retries: 0,
            status: HistoryStatus::Succeeded,
            attempt_count: 4,
            cycle_attempt: 4,
            http_status: Some(200),
            error_kind: None,
            elapsed_ms: Some(5),
        })
        .unwrap();
    let page = database
        .page(1, HistoryFilter::All, &config(), now)
        .unwrap();
    assert_eq!(page.entries[0].status, HistoryStatus::Queued);
    assert_eq!(page.entries[0].created_at_ms, now - 20);
    assert_eq!(page.entries[0].manual_retries, 1);
}
