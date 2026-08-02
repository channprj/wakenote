use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

use wakenote::commands::RecentTranscript;
use wakenote::llm::{
    LlmCompletionReason, LlmGenerateError, LlmGenerateRequest, LlmProgressStage,
    LlmReportHistoryItem, LlmReportKind, OpenRouterClient, build_openrouter_request_body,
    build_openrouter_request_body_with_system, export_llm_report, format_transcripts_for_llm,
    generate_transcript_report_with_client_and_progress,
    generate_transcript_report_with_client_progress_and_cancel, list_llm_report_history,
    load_llm_report_history_detail, parse_openrouter_chat_content, parse_openrouter_chat_response,
    parse_quality_evaluation, render_prompt_template,
};
use wakenote::recorder::ChunkSource;
use wakenote::settings::{AppSettings, SettingsPatch};

fn transcript(path: &str, recorded_at: &str, text: &str) -> RecentTranscript {
    RecentTranscript {
        transcript_path: path.to_string(),
        audio_path: None,
        recorded_at: recorded_at.to_string(),
        text: text.to_string(),
        source: ChunkSource::Microphone,
        source_label: None,
        device_id: None,
        device_name: None,
        microphone_slot: None,
    }
}

#[test]
fn prompt_template_replaces_all_supported_placeholders() {
    let rendered = render_prompt_template(
        "{{date_range}} / {{selected_count}} / {{language}}\n{{transcripts}}",
        "2026-07-13 10:00 - 2026-07-13 11:00",
        2,
        "ko",
        "10:00 hello\n11:00 world",
    );

    assert_eq!(
        rendered,
        "2026-07-13 10:00 - 2026-07-13 11:00 / 2 / ko\n10:00 hello\n11:00 world"
    );
}

#[test]
fn transcript_context_formats_entries_with_date_range() {
    let entries = vec![
        transcript(
            "/tmp/WakeNote/20260713/100000.txt",
            "2026-07-13T10:00:00+09:00",
            "first note",
        ),
        transcript(
            "/tmp/WakeNote/20260713/110000.txt",
            "2026-07-13T11:00:00+09:00",
            "second note",
        ),
    ];

    let context = format_transcripts_for_llm(&entries);

    assert_eq!(
        context.date_range,
        "2026-07-13T10:00:00+09:00 - 2026-07-13T11:00:00+09:00"
    );
    assert_eq!(context.selected_count, 2);
    assert!(context.transcripts.contains("[2026-07-13T10:00:00+09:00]"));
    assert!(context.transcripts.contains("first note"));
    assert!(context.transcripts.contains("second note"));
}

#[test]
fn openrouter_request_body_uses_selected_model_and_prompt() {
    let body = build_openrouter_request_body("z-ai/glm-5.2", "summarize this");

    assert_eq!(body["model"], "z-ai/glm-5.2");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "summarize this");
}

#[test]
fn openrouter_evaluation_request_separates_system_rules_from_untrusted_report_data() {
    let body = build_openrouter_request_body_with_system(
        "z-ai/glm-5.2",
        "Judge the report against the success criteria.",
        "Transcript and candidate report",
    );

    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(
        body["messages"][0]["content"],
        "Judge the report against the success criteria."
    );
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(
        body["messages"][1]["content"],
        "Transcript and candidate report"
    );
}

#[test]
fn openrouter_response_parser_extracts_assistant_content() {
    let body = r#"{
      "choices": [
        { "message": { "role": "assistant", "content": "report body" } }
      ]
    }"#;

    assert_eq!(parse_openrouter_chat_content(body).unwrap(), "report body");
}

#[test]
fn openrouter_response_parser_extracts_usage_and_provider_reported_cost() {
    let body = r#"{
      "choices": [
        { "message": { "role": "assistant", "content": "report body" } }
      ],
      "usage": {
        "prompt_tokens": 120,
        "completion_tokens": 30,
        "total_tokens": 150,
        "cost": 0.00125
      }
    }"#;

    let response = parse_openrouter_chat_response(body).unwrap();

    assert_eq!(response.content, "report body");
    let usage = response.usage.expect("usage");
    assert_eq!(usage.prompt_tokens, 120);
    assert_eq!(usage.completion_tokens, 30);
    assert_eq!(usage.total_tokens, 150);
    assert_eq!(usage.cost, Some(0.00125));
}

#[test]
fn malformed_usage_does_not_discard_an_otherwise_valid_completion() {
    let body = r#"{
      "choices": [
        { "message": { "role": "assistant", "content": "report body" } }
      ],
      "usage": { "prompt_tokens": "unknown" }
    }"#;

    let response = parse_openrouter_chat_response(body).unwrap();

    assert_eq!(response.content, "report body");
    assert!(response.usage.is_none());
}

#[derive(Clone)]
struct ScriptedClient {
    prompts: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    responses: Arc<Mutex<VecDeque<String>>>,
}

impl ScriptedClient {
    fn new(responses: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            prompts: Arc::new(Mutex::new(Vec::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(
                responses
                    .into_iter()
                    .map(openrouter_response)
                    .collect::<VecDeque<_>>(),
            )),
        }
    }

    fn from_raw(responses: impl IntoIterator<Item = String>) -> Self {
        Self {
            prompts: Arc::new(Mutex::new(Vec::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses.into_iter().collect::<VecDeque<_>>())),
        }
    }
}

#[async_trait]
impl OpenRouterClient for ScriptedClient {
    async fn chat(&self, _api_key: &str, body: serde_json::Value) -> Result<String, String> {
        let prompt = body["messages"]
            .as_array()
            .and_then(|messages| messages.last())
            .and_then(|message| message["content"].as_str())
            .expect("prompt")
            .to_string();
        self.prompts.lock().expect("lock").push(prompt);
        self.requests.lock().expect("lock").push(body);
        self.responses
            .lock()
            .expect("lock")
            .pop_front()
            .ok_or_else(|| "unexpected OpenRouter call".to_string())
    }
}

fn openrouter_response(content: &str) -> String {
    serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": content } }]
    })
    .to_string()
}

fn openrouter_response_with_usage(
    content: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
    cost: f64,
) -> String {
    serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": content } }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens,
            "cost": cost,
        }
    })
    .to_string()
}

fn generation_settings(save_root: &std::path::Path, max_iterations: u8) -> AppSettings {
    let mut settings = AppSettings::default();
    settings.apply_patch(SettingsPatch {
        save_root: Some(save_root.to_string_lossy().to_string()),
        llm_max_iterations: Some(max_iterations),
        ..Default::default()
    });
    settings
}

#[derive(Clone)]
struct PendingClient {
    dropped: Arc<AtomicBool>,
}

struct DropSignal(Arc<AtomicBool>);

impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[async_trait]
impl OpenRouterClient for PendingClient {
    async fn chat(&self, _api_key: &str, _body: serde_json::Value) -> Result<String, String> {
        let _signal = DropSignal(self.dropped.clone());
        std::future::pending::<Result<String, String>>().await
    }
}

struct UnexpectedClient;

#[async_trait]
impl OpenRouterClient for UnexpectedClient {
    async fn chat(&self, _api_key: &str, _body: serde_json::Value) -> Result<String, String> {
        panic!("HTTP must not start after run progress persistence fails");
    }
}

#[tokio::test]
async fn progress_persistence_failure_stops_before_http() {
    let temp = tempfile::tempdir().unwrap();
    let settings = generation_settings(temp.path(), 3);
    let cancellation = CancellationToken::new();

    let error = generate_transcript_report_with_client_progress_and_cancel(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::Summary),
        &UnexpectedClient,
        &cancellation,
        |_| Err("Could not persist report run progress".into()),
    )
    .await
    .unwrap_err();

    assert_eq!(
        error,
        LlmGenerateError::Failed("Could not persist report run progress".into())
    );
    assert!(!temp.path().join("reports").exists());
}

#[tokio::test]
async fn cancellation_drops_the_in_flight_http_future_without_saving() {
    let temp = tempfile::tempdir().unwrap();
    let settings = generation_settings(temp.path(), 3);
    let dropped = Arc::new(AtomicBool::new(false));
    let client = PendingClient {
        dropped: dropped.clone(),
    };
    let cancellation = CancellationToken::new();
    let cancellation_for_task = cancellation.clone();

    let task = tokio::spawn(async move {
        generate_transcript_report_with_client_progress_and_cancel(
            &settings,
            "sk-or-test",
            generation_request(LlmReportKind::Summary),
            &client,
            &cancellation_for_task,
            |_| Ok(()),
        )
        .await
    });
    tokio::task::yield_now().await;
    cancellation.cancel();

    let error = task.await.unwrap().unwrap_err();

    assert_eq!(error, LlmGenerateError::Cancelled);
    assert!(dropped.load(Ordering::Acquire));
    assert!(!temp.path().join("reports").exists());
}

#[tokio::test]
async fn cancellation_during_completion_event_removes_persisted_report() {
    let temp = tempfile::tempdir().unwrap();
    let settings = generation_settings(temp.path(), 1);
    let client = ScriptedClient::new([
        "complete draft",
        r#"{"success":true,"feedback":"Complete."}"#,
    ]);
    let cancellation = CancellationToken::new();
    let cancellation_from_progress = cancellation.clone();

    let error = generate_transcript_report_with_client_progress_and_cancel(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::Summary),
        &client,
        &cancellation,
        move |event| {
            if event.stage == LlmProgressStage::Completed {
                cancellation_from_progress.cancel();
            }
            Ok(())
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error, LlmGenerateError::Cancelled);
    assert!(
        list_llm_report_history(temp.path().to_str().unwrap())
            .unwrap()
            .is_empty()
    );
}

fn generation_request(kind: LlmReportKind) -> LlmGenerateRequest {
    LlmGenerateRequest {
        kind,
        transcripts: vec![transcript(
            "/tmp/WakeNote/20260713/100000.txt",
            "2026-07-13T10:00:00+09:00",
            "summarize me",
        )],
        run_id: Some("report-run-1".to_string()),
    }
}

#[tokio::test]
async fn generation_stops_after_the_first_successful_quality_evaluation() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 30);
    let client = ScriptedClient::new([
        "first complete draft",
        r#"{"success":true,"feedback":"All requested sections are complete and grounded."}"#,
    ]);
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_events = progress.clone();

    let result = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::Summary),
        &client,
        move |event| progress_events.lock().expect("lock").push(event),
    )
    .await
    .unwrap();

    assert_eq!(result.content, "first complete draft");
    assert_eq!(result.iterations_used, 1);
    assert_eq!(result.max_iterations, 30);
    assert!(result.success_criteria_met);
    assert_eq!(
        result.completion_reason,
        LlmCompletionReason::SuccessCriteriaMet
    );
    assert_eq!(client.prompts.lock().expect("lock").len(), 2);
    assert_eq!(
        progress
            .lock()
            .expect("lock")
            .iter()
            .map(|event| event.stage)
            .collect::<Vec<_>>(),
        vec![
            LlmProgressStage::Preparing,
            LlmProgressStage::Generating,
            LlmProgressStage::Evaluating,
            LlmProgressStage::Saving,
            LlmProgressStage::Completed,
        ]
    );
}

#[tokio::test]
async fn generation_aggregates_usage_and_cost_across_drafting_and_evaluation_calls() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 30);
    let client = ScriptedClient::from_raw([
        openrouter_response_with_usage("complete draft", 100, 40, 0.001),
        openrouter_response_with_usage(
            r#"{"success":true,"feedback":"Complete."}"#,
            80,
            20,
            0.0005,
        ),
    ]);

    let result = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::Summary),
        &client,
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(result.usage.request_count, 2);
    assert_eq!(result.usage.prompt_tokens, Some(180));
    assert_eq!(result.usage.completion_tokens, Some(60));
    assert_eq!(result.usage.total_tokens, Some(240));
    assert_eq!(result.usage.cost, Some(0.0015));
}

#[tokio::test]
async fn generation_refines_with_quality_feedback_and_stops_when_the_second_draft_passes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 5);
    let client = ScriptedClient::new([
        "incomplete draft",
        r#"{"success":false,"feedback":"Add the missing action items."}"#,
        "complete revised draft",
        r#"{"success":true,"feedback":"The missing action items are now included."}"#,
    ]);

    let result = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::DetailedReport),
        &client,
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(result.content, "complete revised draft");
    assert_eq!(result.iterations_used, 2);
    assert!(result.success_criteria_met);
    let prompts = client.prompts.lock().expect("lock");
    assert_eq!(prompts.len(), 4);
    assert!(prompts[2].contains("Add the missing action items."));
    assert!(prompts[2].contains("incomplete draft"));
    drop(prompts);
    let requests = client.requests.lock().expect("lock");
    assert_eq!(requests[0]["messages"][0]["role"], "system");
    assert_eq!(requests[2]["messages"][0]["role"], "system");
    let refinement_payload: serde_json::Value =
        serde_json::from_str(requests[2]["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        refinement_payload["quality_feedback"],
        "Add the missing action items."
    );
    assert_eq!(refinement_payload["current_draft"], "incomplete draft");
    assert!(
        refinement_payload["transcript_data"]
            .as_str()
            .unwrap()
            .contains("summarize me")
    );
}

#[tokio::test]
async fn generation_returns_the_latest_draft_with_an_explicit_incomplete_status_at_the_limit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 2);
    let client = ScriptedClient::new([
        "draft one",
        r#"{"success":false,"feedback":"Missing decisions."}"#,
        "draft two",
        r#"{"success":false,"feedback":"Evidence notes are still incomplete."}"#,
    ]);
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_events = progress.clone();

    let result = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::DetailedReport),
        &client,
        move |event| progress_events.lock().expect("lock").push(event),
    )
    .await
    .unwrap();

    assert_eq!(result.content, "draft two");
    assert_eq!(result.iterations_used, 2);
    assert_eq!(result.max_iterations, 2);
    assert!(!result.success_criteria_met);
    assert_eq!(
        result.completion_reason,
        LlmCompletionReason::MaxIterationsReached
    );
    assert_eq!(
        result.quality_feedback,
        "Evidence notes are still incomplete."
    );
    assert_eq!(client.prompts.lock().expect("lock").len(), 4);
    assert_eq!(
        progress.lock().expect("lock").last().unwrap().message,
        "Maximum iterations reached; saved the latest draft"
    );
}

#[test]
fn quality_evaluation_parser_accepts_a_json_code_fence() {
    let evaluation = parse_quality_evaluation(
        "```json\n{\"success\":false,\"feedback\":\"Add evidence notes.\"}\n```",
    )
    .unwrap();

    assert!(!evaluation.success);
    assert_eq!(evaluation.feedback, "Add evidence notes.");
}

#[tokio::test]
async fn malformed_quality_evaluation_fails_the_active_iteration_without_saving_a_report() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 3);
    let client = ScriptedClient::new(["draft", "not json"]);
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_events = progress.clone();

    let error = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::Summary),
        &client,
        move |event| progress_events.lock().expect("lock").push(event),
    )
    .await
    .unwrap_err();

    assert!(error.contains("Quality evaluator did not return a JSON object"));
    let events = progress.lock().expect("lock");
    let failed = events.last().expect("failed event");
    assert_eq!(failed.stage, LlmProgressStage::Failed);
    assert_eq!(failed.iteration, 1);
    assert!(!tmp.path().join("reports").exists());
}

#[tokio::test]
async fn generation_persists_markdown_report_under_save_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 1);
    let client = ScriptedClient::new(["persist me", r#"{"success":true,"feedback":"Complete."}"#]);

    let result = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::DetailedReport),
        &client,
        |_| {},
    )
    .await
    .unwrap();
    let report_path = result.report_path.expect("report path");

    assert!(report_path.contains("/reports/"));
    assert!(report_path.ends_with("-detailed-report.md"));
    assert_eq!(std::fs::read_to_string(report_path).unwrap(), "persist me");
}

#[tokio::test]
async fn generated_report_is_available_as_history_detail_with_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let settings = generation_settings(tmp.path(), 1);
    let client = ScriptedClient::from_raw([
        openrouter_response_with_usage("persist me", 100, 25, 0.001),
        openrouter_response_with_usage(
            r#"{"success":true,"feedback":"Complete."}"#,
            50,
            10,
            0.0002,
        ),
    ]);

    let result = generate_transcript_report_with_client_and_progress(
        &settings,
        "sk-or-test",
        generation_request(LlmReportKind::DetailedReport),
        &client,
        |_| {},
    )
    .await
    .unwrap();

    let history = list_llm_report_history(tmp.path().to_str().unwrap()).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].report_id, result.report_id);
    assert_eq!(history[0].kind, LlmReportKind::DetailedReport);
    assert_eq!(history[0].selected_count, Some(1));
    assert_eq!(history[0].usage.as_ref().unwrap().total_tokens, Some(185));
    assert!(!history[0].legacy);

    let detail =
        load_llm_report_history_detail(tmp.path().to_str().unwrap(), &result.report_id).unwrap();
    assert_eq!(detail.content, "persist me");
    assert_eq!(detail.item.quality_feedback.as_deref(), Some("Complete."));
}

#[test]
fn history_discovers_legacy_markdown_without_sidecar_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let reports = tmp.path().join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(
        reports.join("20260712-091500-summary.md"),
        "# Earlier summary",
    )
    .unwrap();

    let history = list_llm_report_history(tmp.path().to_str().unwrap()).unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].report_id, "20260712-091500-summary");
    assert_eq!(history[0].kind, LlmReportKind::Summary);
    assert!(history[0].legacy);
    assert!(history[0].usage.is_none());
    let detail =
        load_llm_report_history_detail(tmp.path().to_str().unwrap(), "20260712-091500-summary")
            .unwrap();
    assert_eq!(detail.content, "# Earlier summary");
}

#[test]
fn history_sorts_rfc3339_timestamps_by_instant_across_offsets() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let reports = tmp.path().join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    let older_path = reports.join("20260713-100000-summary.md");
    let newer_path = reports.join("20260713-110000-summary.md");
    std::fs::write(&older_path, "# Older").unwrap();
    std::fs::write(&newer_path, "# Newer").unwrap();

    for (path, created_at) in [
        (&older_path, "2026-01-01T01:30:00+02:00"),
        (&newer_path, "2026-01-01T00:45:00+00:00"),
    ] {
        let report_id = path.file_stem().unwrap().to_string_lossy().to_string();
        let item = LlmReportHistoryItem {
            report_id,
            kind: LlmReportKind::Summary,
            created_at: created_at.to_string(),
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            report_path: path.to_string_lossy().to_string(),
            model: None,
            iterations_used: None,
            max_iterations: None,
            success_criteria_met: None,
            completion_reason: None,
            quality_feedback: None,
            selected_count: None,
            date_range: None,
            usage: None,
            legacy: false,
        };
        std::fs::write(
            path.with_extension("json"),
            serde_json::to_vec_pretty(&item).unwrap(),
        )
        .unwrap();
    }

    let history = list_llm_report_history(tmp.path().to_str().unwrap()).unwrap();

    assert_eq!(history[0].report_id, "20260713-110000-summary");
    assert_eq!(history[1].report_id, "20260713-100000-summary");
}

#[test]
fn report_export_copies_previous_markdown_and_rejects_path_traversal_ids() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let reports = tmp.path().join("reports");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::write(
        reports.join("20260712-091500-summary.md"),
        "# Earlier summary",
    )
    .unwrap();
    let destination = tmp.path().join("downloaded-summary.md");

    let exported = export_llm_report(
        tmp.path().to_str().unwrap(),
        "20260712-091500-summary",
        destination.to_str().unwrap(),
    )
    .unwrap();

    assert_eq!(exported, destination.to_string_lossy());
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "# Earlier summary"
    );
    let error =
        load_llm_report_history_detail(tmp.path().to_str().unwrap(), "../settings").unwrap_err();
    assert!(error.contains("Invalid report id"));
}
