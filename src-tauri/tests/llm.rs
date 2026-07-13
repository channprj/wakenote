use std::sync::{Arc, Mutex};

use wakenote::commands::RecentTranscript;
use wakenote::llm::{
    LlmGenerateRequest, LlmReportKind, OpenRouterClient, build_openrouter_request_body,
    format_transcripts_for_llm, generate_transcript_report_with_client,
    parse_openrouter_chat_content, render_prompt_template,
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
fn openrouter_response_parser_extracts_assistant_content() {
    let body = r#"{
      "choices": [
        { "message": { "role": "assistant", "content": "report body" } }
      ]
    }"#;

    assert_eq!(parse_openrouter_chat_content(body).unwrap(), "report body");
}

#[derive(Clone)]
struct RecordingClient {
    prompts: Arc<Mutex<Vec<String>>>,
}

impl OpenRouterClient for RecordingClient {
    fn chat(&self, _api_key: &str, body: serde_json::Value) -> Result<String, String> {
        let prompt = body["messages"][0]["content"]
            .as_str()
            .expect("prompt")
            .to_string();
        self.prompts.lock().expect("lock").push(prompt);
        let count = self.prompts.lock().expect("lock").len();
        Ok(format!(
            r#"{{"choices":[{{"message":{{"content":"draft {count}"}}}}]}}"#
        ))
    }
}

#[test]
fn generation_uses_configured_iterations_up_to_thirty() {
    let mut settings = AppSettings::default();
    settings.apply_patch(SettingsPatch {
        llm_max_iterations: Some(30),
        ..Default::default()
    });
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let client = RecordingClient {
        prompts: prompts.clone(),
    };
    let request = LlmGenerateRequest {
        kind: LlmReportKind::Summary,
        transcripts: vec![transcript(
            "/tmp/WakeNote/20260713/100000.txt",
            "2026-07-13T10:00:00+09:00",
            "summarize me",
        )],
    };

    let result =
        generate_transcript_report_with_client(&settings, "sk-or-test", request, &client).unwrap();

    assert_eq!(result.content, "draft 30");
    assert_eq!(result.iterations_used, 30);
    assert_eq!(prompts.lock().expect("lock").len(), 30);
}

#[test]
fn generation_persists_markdown_report_under_save_root() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut settings = AppSettings::default();
    settings.apply_patch(SettingsPatch {
        save_root: Some(tmp.path().to_string_lossy().to_string()),
        llm_max_iterations: Some(1),
        ..Default::default()
    });
    let client = RecordingClient {
        prompts: Arc::new(Mutex::new(Vec::new())),
    };
    let request = LlmGenerateRequest {
        kind: LlmReportKind::DetailedReport,
        transcripts: vec![transcript(
            "/tmp/WakeNote/20260713/100000.txt",
            "2026-07-13T10:00:00+09:00",
            "persist me",
        )],
    };

    let result =
        generate_transcript_report_with_client(&settings, "sk-or-test", request, &client).unwrap();
    let report_path = result.report_path.expect("report path");

    assert!(report_path.contains("/reports/"));
    assert!(report_path.ends_with("-detailed-report.md"));
    assert_eq!(std::fs::read_to_string(report_path).unwrap(), "draft 1");
}
