use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::commands::RecentTranscript;
use crate::settings::{AppSettings, expand_user_path};

const OPENROUTER_CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmReportKind {
    Summary,
    DetailedReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmGenerateRequest {
    pub kind: LlmReportKind,
    pub transcripts: Vec<RecentTranscript>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmGenerateResponse {
    pub content: String,
    pub iterations_used: u8,
    pub model: String,
    pub report_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmTranscriptContext {
    pub transcripts: String,
    pub date_range: String,
    pub selected_count: usize,
    pub language: String,
}

pub trait OpenRouterClient {
    fn chat(&self, api_key: &str, body: serde_json::Value) -> Result<String, String>;
}

pub struct UreqOpenRouterClient;

impl OpenRouterClient for UreqOpenRouterClient {
    fn chat(&self, api_key: &str, body: serde_json::Value) -> Result<String, String> {
        ureq::post(OPENROUTER_CHAT_COMPLETIONS_URL)
            .set("Authorization", &format!("Bearer {api_key}"))
            .set("Content-Type", "application/json")
            .set("HTTP-Referer", "https://github.com/channprj/sagwan")
            .set("X-Title", "WakeNote")
            .send_string(&body.to_string())
            .map_err(openrouter_error_body)?
            .into_string()
            .map_err(|error| error.to_string())
    }
}

fn openrouter_error_body(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => response
            .into_string()
            .map(|body| format!("OpenRouter request failed with HTTP {code}: {body}"))
            .unwrap_or_else(|_| format!("OpenRouter request failed with HTTP {code}")),
        ureq::Error::Transport(error) => error.to_string(),
    }
}

pub fn render_prompt_template(
    template: &str,
    date_range: &str,
    selected_count: usize,
    language: &str,
    transcripts: &str,
) -> String {
    template
        .replace("{{date_range}}", date_range)
        .replace("{{selected_count}}", &selected_count.to_string())
        .replace("{{language}}", language)
        .replace("{{transcripts}}", transcripts)
}

pub fn format_transcripts_for_llm(entries: &[RecentTranscript]) -> LlmTranscriptContext {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| {
        left.recorded_at
            .cmp(&right.recorded_at)
            .then_with(|| left.transcript_path.cmp(&right.transcript_path))
    });
    let date_range = match (sorted.first(), sorted.last()) {
        (Some(first), Some(last)) if first.recorded_at != last.recorded_at => {
            format!("{} - {}", first.recorded_at, last.recorded_at)
        }
        (Some(first), _) => first.recorded_at.clone(),
        _ => "No transcript timestamps".to_string(),
    };
    let transcripts = sorted
        .iter()
        .map(|entry| {
            let source = match (&entry.source, &entry.source_label) {
                (crate::recorder::ChunkSource::System, Some(label)) => {
                    format!("system:{label}")
                }
                (crate::recorder::ChunkSource::System, None) => "system".to_string(),
                _ => "microphone".to_string(),
            };
            format!(
                "[{}] [{}] {}\n{}",
                entry.recorded_at,
                source,
                entry.transcript_path,
                entry.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    LlmTranscriptContext {
        transcripts,
        date_range,
        selected_count: sorted.len(),
        language: "auto".to_string(),
    }
}

pub fn build_openrouter_request_body(model: &str, prompt: &str) -> serde_json::Value {
    json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": prompt,
            }
        ],
    })
}

pub fn parse_openrouter_chat_content(body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| error.to_string())?;
    value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "OpenRouter response did not include assistant content".to_string())
}

pub fn generate_transcript_report_with_client<C: OpenRouterClient>(
    settings: &AppSettings,
    api_key: &str,
    request: LlmGenerateRequest,
    client: &C,
) -> Result<LlmGenerateResponse, String> {
    if request.transcripts.is_empty() {
        return Err("Select at least one transcript".to_string());
    }
    let context = format_transcripts_for_llm(&request.transcripts);
    let template = match request.kind {
        LlmReportKind::Summary => &settings.llm_summary_prompt_template,
        LlmReportKind::DetailedReport => &settings.llm_report_prompt_template,
    };
    let base_prompt = render_prompt_template(
        template,
        &context.date_range,
        context.selected_count,
        &context.language,
        &context.transcripts,
    );
    let max_iterations = crate::settings::clamp_llm_max_iterations(settings.llm_max_iterations);
    let mut content = String::new();
    for iteration in 1..=max_iterations {
        let prompt = if iteration == 1 {
            base_prompt.clone()
        } else {
            refinement_prompt(&base_prompt, &content, iteration, max_iterations)
        };
        let response = client.chat(
            api_key,
            build_openrouter_request_body(&settings.openrouter_model, &prompt),
        )?;
        content = parse_openrouter_chat_content(&response)?;
    }

    let report_path = persist_report(&settings.save_root, request.kind, &content)?;

    Ok(LlmGenerateResponse {
        content,
        iterations_used: max_iterations,
        model: settings.openrouter_model.clone(),
        report_path: Some(report_path),
    })
}

pub fn generate_transcript_report(
    settings: &AppSettings,
    api_key: &str,
    request: LlmGenerateRequest,
) -> Result<LlmGenerateResponse, String> {
    generate_transcript_report_with_client(settings, api_key, request, &UreqOpenRouterClient)
}

fn refinement_prompt(
    base_prompt: &str,
    current_draft: &str,
    iteration: u8,
    max_iterations: u8,
) -> String {
    format!(
        "Improve the draft below using the original instructions and transcript. \
Keep unsupported claims out. This is refinement iteration {iteration} of {max_iterations}.\n\n\
Original instructions and transcript:\n{base_prompt}\n\nCurrent draft:\n{current_draft}"
    )
}

fn persist_report(save_root: &str, kind: LlmReportKind, content: &str) -> Result<String, String> {
    let reports_dir = expand_user_path(save_root).join("reports");
    std::fs::create_dir_all(&reports_dir).map_err(|error| error.to_string())?;
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let suffix = match kind {
        LlmReportKind::Summary => "summary",
        LlmReportKind::DetailedReport => "detailed-report",
    };
    let mut path = reports_dir.join(format!("{timestamp}-{suffix}.md"));
    let mut counter = 2;
    while path.exists() {
        path = reports_dir.join(format!("{timestamp}-{suffix}-{counter}.md"));
        counter += 1;
    }
    let tmp_path = path.with_extension("md.tmp");
    std::fs::write(&tmp_path, content).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp_path, &path).map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}
