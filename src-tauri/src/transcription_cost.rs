use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc, Weekday};
use serde::{Deserialize, Serialize};

const LEDGER_VERSION: u32 = 1;
const MAX_LEDGER_ENTRIES: usize = 10_000;
const OPENAI_ESTIMATED_USD_PER_MINUTE: f64 = 0.006;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionCostEntry {
    pub source_id: String,
    pub recorded_at: DateTime<Utc>,
    pub provider: String,
    pub model_id: String,
    pub audio_duration_ms: u64,
    pub estimated_cost_usd: Option<f64>,
    pub request_count: u64,
    pub unpriced_request_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionCostPeriod {
    pub estimated_cost_usd: f64,
    pub audio_duration_ms: u64,
    pub request_count: u64,
    pub unpriced_request_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionCostSnapshot {
    pub currency: String,
    pub generated_at: DateTime<Utc>,
    pub today: TranscriptionCostPeriod,
    pub week: TranscriptionCostPeriod,
    pub month: TranscriptionCostPeriod,
    pub entry_count: usize,
    pub disclosure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedLedger {
    version: u32,
    entries: Vec<TranscriptionCostEntry>,
}

pub struct TranscriptionCostLedger {
    path: PathBuf,
    entries: Vec<TranscriptionCostEntry>,
}

impl TranscriptionCostLedger {
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let entries = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedLedger>(&bytes).ok())
            .filter(|ledger| ledger.version == LEDGER_VERSION)
            .map(|ledger| ledger.entries)
            .unwrap_or_default();
        Self { path, entries }
    }

    pub fn upsert(&mut self, entry: TranscriptionCostEntry) -> Result<(), String> {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|candidate| candidate.source_id == entry.source_id)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
        self.entries.sort_by_key(|entry| entry.recorded_at);
        if self.entries.len() > MAX_LEDGER_ENTRIES {
            self.entries
                .drain(..self.entries.len().saturating_sub(MAX_LEDGER_ENTRIES));
        }
        self.save()
    }

    pub fn snapshot(&self, now: DateTime<Local>) -> TranscriptionCostSnapshot {
        let today_start = local_date_start(now.date_naive(), *now.offset());
        let week_start_date =
            now.date_naive() - Duration::days(days_since_monday(now.weekday()) as i64);
        let week_start = local_date_start(week_start_date, *now.offset());
        let month_start = local_date_start(
            NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("valid month"),
            *now.offset(),
        );
        TranscriptionCostSnapshot {
            currency: "USD".to_string(),
            generated_at: now.with_timezone(&Utc),
            today: summarize_since(&self.entries, today_start),
            week: summarize_since(&self.entries, week_start),
            month: summarize_since(&self.entries, month_start),
            entry_count: self.entries.len(),
            disclosure: "Local estimate; verify final charges in your provider billing dashboard."
                .to_string(),
        }
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(&PersistedLedger {
            version: LEDGER_VERSION,
            entries: self.entries.clone(),
        })
        .map_err(|error| error.to_string())?;
        fs::write(&temporary, body).map_err(|error| error.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

pub fn estimated_provider_cost_usd(
    provider: &str,
    model_id: &str,
    audio_duration_ms: u64,
) -> Option<f64> {
    (provider == "OpenAI" && model_id.starts_with("openai-"))
        .then(|| audio_duration_ms as f64 / 60_000.0 * OPENAI_ESTIMATED_USD_PER_MINUTE)
}

fn local_date_start(date: NaiveDate, offset: chrono::FixedOffset) -> DateTime<chrono::FixedOffset> {
    offset
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
        .single()
        .expect("fixed offset midnight")
}

fn days_since_monday(weekday: Weekday) -> u32 {
    weekday.num_days_from_monday()
}

fn summarize_since(
    entries: &[TranscriptionCostEntry],
    start: DateTime<chrono::FixedOffset>,
) -> TranscriptionCostPeriod {
    entries
        .iter()
        .filter(|entry| entry.recorded_at >= start.with_timezone(&Utc))
        .fold(TranscriptionCostPeriod::default(), |mut total, entry| {
            total.estimated_cost_usd += entry.estimated_cost_usd.unwrap_or(0.0);
            total.audio_duration_ms = total
                .audio_duration_ms
                .saturating_add(entry.audio_duration_ms);
            total.request_count = total.request_count.saturating_add(entry.request_count);
            total.unpriced_request_count = total
                .unpriced_request_count
                .saturating_add(entry.unpriced_request_count);
            total
        })
}

pub fn ledger_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("transcription-costs.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        source_id: &str,
        recorded_at: DateTime<Utc>,
        cost: Option<f64>,
    ) -> TranscriptionCostEntry {
        TranscriptionCostEntry {
            source_id: source_id.to_string(),
            recorded_at,
            provider: "OpenAI".to_string(),
            model_id: "openai-gpt-transcribe".to_string(),
            audio_duration_ms: 60_000,
            estimated_cost_usd: cost,
            request_count: 1,
            unpriced_request_count: u64::from(cost.is_none()),
        }
    }

    #[test]
    fn snapshot_uses_current_local_day_week_and_month_boundaries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut ledger = TranscriptionCostLedger::load(temp.path().join("costs.json"));
        let offset = chrono::FixedOffset::east_opt(9 * 3_600).expect("offset");
        let now = offset.with_ymd_and_hms(2026, 8, 5, 12, 0, 0).unwrap();
        for (id, timestamp, cost) in [
            (
                "today",
                offset.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).unwrap(),
                Some(0.006),
            ),
            (
                "week",
                offset.with_ymd_and_hms(2026, 8, 3, 9, 0, 0).unwrap(),
                Some(0.012),
            ),
            (
                "month",
                offset.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap(),
                None,
            ),
            (
                "old",
                offset.with_ymd_and_hms(2026, 7, 31, 23, 59, 0).unwrap(),
                Some(1.0),
            ),
        ] {
            ledger
                .upsert(entry(id, timestamp.with_timezone(&Utc), cost))
                .expect("upsert");
        }

        let snapshot = ledger.snapshot(now.with_timezone(&Local));

        assert_eq!(snapshot.today.request_count, 1);
        assert_eq!(snapshot.week.request_count, 2);
        assert_eq!(snapshot.month.request_count, 3);
        assert_eq!(snapshot.month.unpriced_request_count, 1);
        assert!((snapshot.month.estimated_cost_usd - 0.018).abs() < f64::EPSILON);
    }

    #[test]
    fn upsert_replaces_the_same_source_instead_of_double_counting() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("costs.json");
        let mut ledger = TranscriptionCostLedger::load(&path);
        let now = Utc::now();
        ledger.upsert(entry("meeting:1", now, Some(0.006))).unwrap();
        ledger.upsert(entry("meeting:1", now, Some(0.012))).unwrap();

        let reloaded = TranscriptionCostLedger::load(path);
        let snapshot = reloaded.snapshot(Local::now());

        assert_eq!(snapshot.entry_count, 1);
        assert!((snapshot.today.estimated_cost_usd - 0.012).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_estimate_is_only_available_for_supported_openai_models() {
        assert_eq!(
            estimated_provider_cost_usd("OpenAI", "openai-gpt-transcribe", 60_000),
            Some(0.006)
        );
        assert_eq!(
            estimated_provider_cost_usd("OpenRouter", "openrouter-qwen3-asr-flash", 60_000),
            None
        );
    }
}
