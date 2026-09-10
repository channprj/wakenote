use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

#[derive(Clone)]
struct Request {
    headers: String,
    body: Vec<u8>,
    at: Instant,
}

#[derive(Clone, Copy)]
enum Response {
    Status(u16),
    StallHeaders,
    StallBody,
    Disconnect,
}

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: CancellationToken,
}

impl Server {
    async fn start(responses: Vec<Response>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/private-path?token=endpoint-secret",
            listener.local_addr().unwrap()
        );
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = requests.clone();
        let stop = CancellationToken::new();
        let cancelled = stop.clone();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = tokio::select! {
                    _ = cancelled.cancelled() => break,
                    result = listener.accept() => result.unwrap(),
                };
                let received = received.clone();
                let responses = responses.clone();
                let cancelled = cancelled.clone();
                tokio::spawn(async move {
                    let work = async {
                        let mut bytes = Vec::new();
                        let header_end = loop {
                            let mut part = [0; 4096];
                            let n = socket.read(&mut part).await.unwrap_or(0);
                            if n == 0 {
                                return;
                            }
                            bytes.extend_from_slice(&part[..n]);
                            if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                                break end + 4;
                            }
                        };
                        let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                        let length: usize = headers
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        let received_len = bytes.len();
                        bytes.resize(header_end + length, 0);
                        if received_len < bytes.len()
                            && socket.read_exact(&mut bytes[received_len..]).await.is_err()
                        {
                            return;
                        }
                        let index = {
                            let mut received = received.lock().unwrap();
                            let index = received.len();
                            received.push(Request {
                                headers,
                                body: bytes[header_end..].to_vec(),
                                at: Instant::now(),
                            });
                            index
                        };
                        let response = responses[index.min(responses.len() - 1)];
                        let status = match response {
                            Response::Status(status) => status,
                            Response::StallBody => 200,
                            Response::StallHeaders => {
                                let _ = socket.read(&mut [0]).await;
                                return;
                            }
                            Response::Disconnect => return,
                        };
                        let body = b"response-body-secret";
                        let length = if matches!(response, Response::StallBody) {
                            1_000_000
                        } else if status == 204 {
                            0
                        } else {
                            body.len()
                        };
                        let headers = format!(
                            "HTTP/1.1 {status} Test\r\nContent-Length: {length}\r\nLocation: /redirect-target\r\nConnection: close\r\n\r\n"
                        );
                        let _ = socket.write_all(headers.as_bytes()).await;
                        if length != 0 {
                            let _ = socket.write_all(body).await;
                        }
                        if matches!(response, Response::StallBody) {
                            let _ = socket.read(&mut [0]).await;
                        }
                    };
                    tokio::select! { _ = cancelled.cancelled() => {}, _ = work => {} }
                });
            }
        });
        Self {
            url,
            requests,
            stop,
        }
    }

    fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

fn metadata(chunk_id: u64) -> Arc<ChunkMetadata> {
    let mut metadata: ChunkMetadata =
        serde_json::from_str(include_str!("../../tests/fixtures/live-transcript.json")).unwrap();
    metadata.live_capture_chunk_id = Some(chunk_id);
    Arc::new(metadata)
}

fn execution(text: &str) -> TranscriptionExecution {
    TranscriptionExecution {
        text: text.into(),
        speaker_turns: Vec::new(),
        requested_model_id: "whisper-medium".into(),
        effective_model_id: "whisper-small".into(),
        fallback_from_model_id: Some("whisper-medium".into()),
        usage: None,
        issue: None,
    }
}

fn settings(url: &str) -> LiveTranscriptionWebhookSettings {
    LiveTranscriptionWebhookSettings {
        enabled: true,
        endpoint_url: url.into(),
        payload_format: WebhookPayloadFormat::TextOnly,
    }
}

fn fast_policy() -> Policy {
    Policy {
        timeout: Duration::from_millis(150),
        retry_delays: [
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(40),
        ],
        ..Policy::default()
    }
}

fn start(
    config: LiveTranscriptionWebhookSettings,
    policy: Policy,
) -> (WebhookService, tempfile::TempDir, JoinHandle<()>) {
    let logs = tempfile::tempdir().unwrap();
    let service = WebhookService::unstarted(config, logs.path().into(), policy);
    let task = tokio::spawn(run(service.inner.clone()));
    (service, logs, task)
}

fn submit(service: &WebhookService, id: u64, text: &str) {
    LiveWebhookTranscript::new(metadata(id)).finalize(service, &execution(text));
}

async fn until(mut predicate: impl FnMut() -> bool) {
    timeout(Duration::from_secs(4), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("condition met before timeout");
}

fn header<'a>(request: &'a Request, name: &str) -> &'a str {
    request
        .headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
        .unwrap()
}

#[tokio::test]
async fn defaults_and_disabled_results_never_send_or_replay() {
    let server = Server::start(vec![Response::Status(200)]).await;
    let (service, logs, _) = start(LiveTranscriptionWebhookSettings::default(), fast_policy());
    let old = LiveWebhookTranscript::new(metadata(1));
    old.finalize(&service, &execution("old"));
    service.configure(settings(&server.url), logs.path().into());
    old.finalize(&service, &execution("old"));
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(server.requests().is_empty());
    assert_eq!(service.stats().pending, 0);
    submit(&service, 2, "new");
    until(|| service.stats().succeeded == 1).await;
    assert_eq!(server.requests()[0].body, b"new");
}

#[tokio::test]
async fn text_preserves_unicode_and_only_deduplicates_the_same_final_result() {
    let server = Server::start(vec![Response::Status(202)]).await;
    let (service, _logs, _) = start(settings(&server.url), fast_policy());
    let text = "  오늘 회의 🙂\n다음 줄입니다.\n";
    let first = LiveWebhookTranscript::new(metadata(1));
    first.finalize(&service, &execution(text));
    first.finalize(&service, &execution("a later correction"));
    submit(&service, 2, text);
    submit(&service, 3, " \n\t");
    until(|| service.stats().succeeded == 2).await;
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert!(
            request
                .headers
                .starts_with("POST /private-path?token=endpoint-secret HTTP/1.1")
        );
        assert_eq!(request.body, text.as_bytes());
        assert_eq!(header(request, "content-type"), "text/plain; charset=utf-8");
        assert_eq!(header(request, "x-transcription-attempt"), "1");
    }
    assert_ne!(
        header(&requests[0], "x-transcription-event-id"),
        header(&requests[1], "x-transcription-event-id")
    );
}

#[tokio::test]
async fn json_reuses_archival_schema_and_contains_only_this_transcript() {
    use crate::recorder::{RecordedChunk, TranscriptionSidecar};
    let server = Server::start(vec![Response::Status(200)]).await;
    let mut config = settings(&server.url);
    config.payload_format = WebhookPayloadFormat::Json;
    let (service, logs, _) = start(config, fast_policy());
    let input = metadata(1);
    let text = "새로운 문장입니다.\n🙂";
    let chunk = RecordedChunk::from_audio_path(logs.path().join("sample.wav"));
    std::fs::write(&chunk.metadata_path, input.json_bytes().unwrap()).unwrap();
    let execution = execution(text);
    TranscriptionSidecar::write_success_with_provenance(
        &chunk,
        text,
        &execution.requested_model_id,
        &execution.effective_model_id,
        execution.fallback_from_model_id.as_deref(),
    )
    .unwrap();
    LiveWebhookTranscript::new(input).finalize(&service, &execution);
    until(|| service.stats().succeeded == 1).await;
    let requests = server.requests();
    assert_eq!(
        header(&requests[0], "content-type"),
        "application/json; charset=utf-8"
    );
    let mut body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let archive: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&chunk.metadata_path).unwrap()).unwrap();
    chrono::DateTime::parse_from_rfc3339(body["transcribed_at"].as_str().unwrap()).unwrap();
    body["transcribed_at"] = archive["transcribed_at"].clone();
    assert_eq!(body, archive);
    assert_eq!(body["duration_ms"], 2000);
    assert_eq!(body["transcript_text"], text);
    assert_eq!(body["started_at"], "2026-09-11T01:02:03Z");
    assert!(
        body.get("language").is_none(),
        "do not invent fields absent from the archival schema"
    );
}

#[tokio::test]
async fn all_success_statuses_stop_without_reading_response_body() {
    for response in [
        Response::Status(200),
        Response::Status(201),
        Response::Status(202),
        Response::Status(204),
        Response::Status(299),
        Response::StallBody,
    ] {
        let server = Server::start(vec![response]).await;
        let (service, _logs, _) = start(settings(&server.url), fast_policy());
        submit(&service, 1, "success");
        until(|| service.stats().succeeded == 1).await;
        assert_eq!(service.stats().retries, 0);
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn json_retries_keep_the_original_snapshot_when_unrelated_settings_change() {
    let server = Server::start(vec![Response::Status(500), Response::Status(204)]).await;
    let mut config = settings(&server.url);
    config.payload_format = WebhookPayloadFormat::Json;
    let policy = Policy {
        retry_delays: [Duration::from_millis(100); 3],
        ..fast_policy()
    };
    let (service, logs, _) = start(config.clone(), policy);
    let transcript = LiveWebhookTranscript::new(metadata(1));
    transcript.finalize(&service, &execution("original"));
    until(|| service.stats().retries == 1).await;
    service.configure(config, logs.path().join("another-log-root"));
    transcript.finalize(
        &service,
        &execution("later correction must not replace the snapshot"),
    );
    until(|| service.stats().succeeded == 1).await;
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body, requests[1].body);
    assert_eq!(
        header(&requests[0], "x-transcription-event-id"),
        header(&requests[1], "x-transcription-event-id")
    );
    assert_eq!(header(&requests[1], "x-transcription-attempt"), "2");
    assert_eq!(service.stats().cancelled, 0);
}

#[tokio::test]
async fn non_2xx_and_network_failures_try_exactly_four_times_without_redirects() {
    for response in [
        Response::Status(301),
        Response::Status(307),
        Response::Status(400),
        Response::Status(401),
        Response::Status(429),
        Response::Status(500),
        Response::Disconnect,
        Response::StallHeaders,
    ] {
        let server = Server::start(vec![response]).await;
        let (service, _logs, _) = start(settings(&server.url), fast_policy());
        submit(&service, 1, "unchanged 🙂\nbody");
        until(|| service.stats().failed == 1).await;
        let requests = server.requests();
        assert_eq!(requests.len(), 4);
        assert_eq!(service.stats().retries, 3);
        for (index, request) in requests.iter().enumerate() {
            assert_eq!(request.body, requests[0].body);
            assert_eq!(
                header(request, "x-transcription-event-id"),
                header(&requests[0], "x-transcription-event-id")
            );
            assert_eq!(
                header(request, "x-transcription-attempt"),
                (index + 1).to_string()
            );
            assert!(
                request
                    .headers
                    .starts_with("POST /private-path?token=endpoint-secret ")
            );
        }
    }
}

#[tokio::test]
async fn connection_refused_is_bounded_and_does_not_expose_the_url_in_logs() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://user:password@{}/private-path?token=endpoint-secret",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let (service, logs, _) = start(settings(&endpoint), fast_policy());
    submit(&service, 1, "private-transcript-body");
    until(|| service.stats().failed == 1).await;
    assert_eq!(service.stats().retries, 3);
    let log_path =
        crate::debug_log::debug_log_path_for(logs.path(), chrono::Local::now().date_naive());
    until(|| {
        std::fs::read_to_string(&log_path)
            .unwrap_or_default()
            .contains("failed_connect")
    })
    .await;
    let log = std::fs::read_to_string(log_path).unwrap();
    for secret in [
        "private-transcript-body",
        "private-path",
        "endpoint-secret",
        "password",
        "http://",
    ] {
        assert!(!log.contains(secret), "log leaked {secret}");
    }
    assert!(log.contains("attempt=4"));
    assert!(log.contains("elapsed_ms="));
    assert!(log.contains("retries=3"));
}

#[tokio::test]
async fn retry_success_stops_and_backoff_releases_the_slot() {
    let server = Server::start(vec![Response::Status(500), Response::Status(200)]).await;
    let policy = Policy {
        max_requests: 1,
        retry_delays: [Duration::from_millis(150); 3],
        ..fast_policy()
    };
    let (service, _logs, _) = start(settings(&server.url), policy);
    submit(&service, 1, "first");
    until(|| service.stats().retries == 1).await;
    assert_eq!(service.stats().in_flight, 0);
    submit(&service, 2, "second");
    submit(&service, 3, "third");
    until(|| service.stats().succeeded == 3).await;
    let bodies: Vec<_> = server
        .requests()
        .into_iter()
        .map(|r| String::from_utf8(r.body).unwrap())
        .collect();
    assert_eq!(bodies, ["first", "second", "third", "first"]);
    assert_eq!(service.stats().retries, 1);
    tokio::time::sleep(Duration::from_millis(180)).await;
    assert_eq!(server.requests().len(), 4);
}

#[tokio::test]
async fn capacity_includes_waiting_active_and_retrying_events_without_blocking() {
    let server = Server::start(vec![Response::StallHeaders]).await;
    let logs = tempfile::tempdir().unwrap();
    let service =
        WebhookService::unstarted(settings(&server.url), logs.path().into(), Policy::default());
    for id in 0..=1000 {
        submit(&service, id, "bounded");
    }
    assert_eq!(service.stats().pending, 1000);
    assert_eq!(service.stats().dropped, 1);
    let task = tokio::spawn(run(service.inner.clone()));
    until(|| server.requests().len() == 3).await;
    assert_eq!(service.stats().in_flight, 3);
    assert_eq!(service.stats().pending, 1000);
    submit(&service, 1001, "also dropped");
    assert_eq!(service.stats().dropped, 2);
    service.shutdown();
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(service.stats().cancelled, 1000);
    assert_eq!(service.stats().pending, 0);
    assert_eq!(service.stats().retries, 0);
    assert_eq!(service.stats().failed, 0);
}

#[tokio::test]
async fn changing_url_mode_or_disabling_cancels_old_work_instead_of_retargeting_it() {
    for change in ["url", "mode", "disable"] {
        let old = Server::start(vec![Response::StallHeaders]).await;
        let new = Server::start(vec![Response::Status(200)]).await;
        let (service, logs, _) = start(settings(&old.url), Policy::default());
        for id in 0..6 {
            submit(&service, id, "old setting");
        }
        until(|| old.requests().len() == 3).await;
        let mut config = settings(&old.url);
        match change {
            "url" => config.endpoint_url = new.url.clone(),
            "mode" => config.payload_format = WebhookPayloadFormat::Json,
            _ => config.enabled = false,
        }
        service.configure(config, logs.path().into());
        until(|| service.stats().cancelled == 6).await;
        assert_eq!(service.stats().failed, 0);
        assert_eq!(service.stats().retries, 0);
        assert!(new.requests().is_empty());
        service.configure(settings(&new.url), logs.path().into());
        submit(&service, 10, "new setting");
        until(|| service.stats().succeeded == 1).await;
        assert_eq!(new.requests()[0].body, b"new setting");
        assert_eq!(old.requests().len(), 3);
    }
}

#[tokio::test]
async fn cancel_during_backoff_and_restart_do_not_replay() {
    let server = Server::start(vec![Response::Status(500)]).await;
    let (service, logs, task) = start(settings(&server.url), Policy::default());
    submit(&service, 1, "old");
    until(|| service.stats().retries == 1).await;
    service.configure(
        LiveTranscriptionWebhookSettings::default(),
        logs.path().into(),
    );
    assert_eq!(service.stats().cancelled, 1);
    assert_eq!(service.stats().pending, 0);
    drop(service);
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    let (restarted, _logs, _) = start(settings(&server.url), fast_policy());
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(server.requests().len(), 1);
    assert_eq!(restarted.stats().pending, 0);
}

#[tokio::test]
async fn production_backoff_is_one_two_four_seconds_after_each_failure() {
    let server = Server::start(vec![Response::Status(503)]).await;
    let (service, _logs, _) = start(settings(&server.url), Policy::default());
    submit(&service, 1, "backoff");
    timeout(Duration::from_secs(12), async {
        while service.stats().failed == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for (pair, delay) in requests.windows(2).zip(RETRY_DELAYS) {
        assert!(pair[1].at - pair[0].at >= delay);
    }
}

#[tokio::test]
async fn production_timeout_fires_at_fifteen_seconds_without_waiting_for_session_end() {
    let server = Server::start(vec![Response::StallHeaders]).await;
    let (service, _logs, task) = start(settings(&server.url), Policy::default());
    let started = Instant::now();
    submit(&service, 1, "session remains open");
    until(|| server.requests().len() == 1).await;
    timeout(Duration::from_secs(20), async {
        while service.stats().retries == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(15));
    assert!(started.elapsed() < Duration::from_secs(20));
    service.shutdown();
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn tls_handshake_and_stalled_upload_are_covered_by_each_attempt_deadline() {
    for tls in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "{}://{}",
            if tls { "https" } else { "http" },
            listener.local_addr().unwrap()
        );
        let sockets = Arc::new(Mutex::new(Vec::new()));
        let accepted = sockets.clone();
        let server = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                accepted.lock().unwrap().push(socket);
            }
        });
        let (service, _logs, _) = start(settings(&url), fast_policy());
        let text = if tls {
            "handshake".into()
        } else {
            "x".repeat(32 * 1024 * 1024)
        };
        submit(&service, 1, &text);
        until(|| service.stats().failed == 1).await;
        assert_eq!(service.stats().retries, 3);
        assert_eq!(sockets.lock().unwrap().len(), 4);
        server.abort();
    }
}

#[tokio::test]
async fn captured_audio_keeps_processing_and_saving_while_delivery_stalls_even_after_stop() {
    use crate::commands::AppBackend;
    use crate::settings::{AudioFormat, SettingsPatch};
    use crate::transcription::{Transcriber, TranscriptionError, TranscriptionRequest};
    #[derive(Clone)]
    struct SyntheticTranscriber;
    impl Transcriber for SyntheticTranscriber {
        fn transcribe(&self, _: TranscriptionRequest<'_>) -> Result<String, TranscriptionError> {
            Ok("확정 전사입니다. 🙂\nSecond line.".into())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let models = root.path().join("models");
    std::fs::create_dir(&models).unwrap();
    std::fs::write(models.join("whisper-medium.bin"), b"unused synthetic model").unwrap();
    let server = Server::start(vec![Response::StallHeaders]).await;
    let mut config = settings(&server.url);
    config.payload_format = WebhookPayloadFormat::Json;
    let (service, _logs, task) = start(config.clone(), Policy::default());
    let service = Arc::new(service);
    let mut backend = AppBackend::default();
    backend
        .try_update_settings(SettingsPatch {
            live_transcription_webhook: Some(config),
            save_root: Some(root.path().to_string_lossy().into()),
            model_directory: Some(models.to_string_lossy().into()),
            selected_model: Some("whisper-medium".into()),
            audio_format: Some(AudioFormat::Wav),
            attack_ms: Some(100),
            release_ms: Some(200),
            min_chunk_ms: Some(600),
            max_chunk_ms: Some(10_000),
            ..Default::default()
        })
        .unwrap();
    backend.set_live_webhook(service.clone());
    backend.start_capture_session_for_test(10).unwrap();
    for _ in 0..110 {
        backend.process_audio_samples_for_test(&[0.8], 100).unwrap();
    }
    assert!(
        server.requests().is_empty(),
        "capture/partial/commit events are not finalized text"
    );
    let first = backend.queue_snapshot().jobs[0].audio_path.clone();
    std::fs::remove_file(first.with_extension("json")).unwrap();
    std::fs::create_dir(first.with_extension("json")).unwrap();
    backend
        .process_next_transcription_with(SyntheticTranscriber)
        .unwrap();
    until(|| server.requests().len() == 1).await;
    assert!(backend.app_status().live_input_active);
    assert_eq!(
        backend.queue_snapshot().failed_count,
        1,
        "the independent sidecar save failure remains visible"
    );
    let payload: serde_json::Value = serde_json::from_slice(&server.requests()[0].body).unwrap();
    assert_eq!(
        payload["transcript_text"],
        "확정 전사입니다. 🙂\nSecond line."
    );
    for _ in 0..3 {
        backend.process_audio_samples_for_test(&[0.8], 100).unwrap();
    }
    backend.stop_capture_session().unwrap();
    backend
        .process_pending_transcriptions_with(SyntheticTranscriber)
        .unwrap();
    until(|| server.requests().len() >= 2).await;
    assert!(
        backend
            .queue_snapshot()
            .jobs
            .iter()
            .any(|job| job.status == crate::queue::QueueJobStatus::Completed)
    );
    assert!(!backend.app_status().live_input_active);
    assert_eq!(
        service.stats().cancelled,
        0,
        "capture stop must not cancel the delivery queue"
    );
    assert!(service.stats().pending >= 2);
    service.shutdown();
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
}
