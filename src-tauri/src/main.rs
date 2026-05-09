use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use sagwan::audio::list_input_devices;
use sagwan::commands::{
    main_window_close_action, microphone_devices_from_input_devices, reveal_save_folder_request,
    tray_menu_presentation, tray_presentation_for_state, tray_runtime_presentation,
    with_live_runtime_warning, AppBackend, AppStatus, LiveTranscriptEvent, MainWindowCloseAction,
    MicrophoneDevice,
};
use sagwan::live_capture::{
    AudioInputConfig, CpalAudioInput, LiveCaptureError, LiveCaptureRuntime, ResolvedCpalInputDevice,
};
use sagwan::live_transcription::{LivePartialEvent, LivePartialRequest, LiveTranscriptionService};
use sagwan::models::{ModelDescriptor, ModelStore};
use sagwan::overlay::{self, OverlayState};
use sagwan::queue::QueueSnapshot;
use sagwan::recorder::ChunkMetadata;
use sagwan::settings::{
    launch_at_login_action_for_patch, live_capture_runtime_action_for_patch,
    live_capture_should_run, AppSettings, FloatingOverlayPosition, LaunchAtLoginAction,
    LiveCaptureRuntimeAction, SettingsPatch,
};
use sagwan::transcription::{
    TranscriptionJobOutcome, TranscriptionJobStatus, TranscriptionWorker, WhisperTranscriber,
};
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State, Wry};

type BackendState = Arc<Mutex<AppBackend>>;
type LiveCaptureState = Mutex<LiveCaptureRuntime<CpalAudioInput>>;
type AutoTranscriptionState = Arc<AtomicBool>;
type LiveTranscriberState = Arc<Mutex<Option<Arc<LiveTranscriptionService>>>>;

const EVENT_LIVE_STARTED: &str = "live-transcript-started";
const EVENT_LIVE_PARTIAL: &str = "live-transcript-partial";
const EVENT_LIVE_COMMITTED: &str = "live-transcript-committed";
const EVENT_LIVE_FINAL: &str = "live-transcript-final";
const EVENT_LIVE_FAILED: &str = "live-transcript-failed";
const MAIN_WINDOW_LABEL: &str = "main";
const MAIN_WINDOW_TITLE: &str = "Sagwan";
const MAIN_WINDOW_WIDTH: f64 = 1180.0;
const MAIN_WINDOW_HEIGHT: f64 = 760.0;
const MAIN_WINDOW_MIN_WIDTH: f64 = 980.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 640.0;
const AUDIO_DEVICE_RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
struct LiveStartedPayload {
    chunk_id: u64,
    started_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct LivePartialPayload {
    chunk_id: u64,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct LiveCommittedPayload {
    chunk_id: u64,
    audio_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct LiveFinalPayload {
    chunk_id: Option<u64>,
    audio_path: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct LiveFailedPayload {
    chunk_id: Option<u64>,
    audio_path: String,
    error: String,
}

#[derive(Clone)]
struct TrayMenuItems {
    recording: CheckMenuItem<Wry>,
    transcription: CheckMenuItem<Wry>,
    active_model: MenuItem<Wry>,
    active_mic: MenuItem<Wry>,
    threshold: MenuItem<Wry>,
    pause_all: CheckMenuItem<Wry>,
}

#[tauri::command]
fn get_settings(state: State<'_, BackendState>) -> Result<AppSettings, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.settings())
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: State<'_, BackendState>,
    live_state: State<'_, LiveCaptureState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    live_transcriber_state: State<'_, LiveTranscriberState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let (launch_at_login_action, live_capture_action, prior_position, previous_model_directory) = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        (
            launch_at_login_action_for_patch(&settings, &patch),
            live_capture_runtime_action_for_patch(&settings, &patch),
            settings.floating_overlay_position,
            settings.model_directory.clone(),
        )
    };
    apply_launch_at_login_action(&app, launch_at_login_action)?;

    let settings = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.update_settings(patch)
    };
    if settings.model_directory != previous_model_directory {
        if let Ok(slot) = live_transcriber_state.lock() {
            if let Some(service) = slot.as_ref() {
                service.update_model_directory(&settings.model_directory);
            }
        }
    }
    apply_live_capture_runtime_action(
        &app,
        state.inner(),
        live_state.inner(),
        transcription_state.inner().clone(),
        live_capture_action,
    )?;
    kick_transcription_worker_if_needed(
        app.clone(),
        state.inner().clone(),
        transcription_state.inner().clone(),
    );

    if prior_position != settings.floating_overlay_position {
        apply_overlay_position_change(&app, &settings);
    }

    Ok(settings)
}

fn apply_overlay_position_change(app: &AppHandle, settings: &AppSettings) {
    let live_running = app
        .try_state::<LiveCaptureState>()
        .and_then(|state| state.lock().ok().map(|live| live.is_running()))
        .unwrap_or(false);
    let result = if matches!(
        settings.floating_overlay_position,
        FloatingOverlayPosition::Off
    ) {
        overlay::hide_overlay_on_main_thread(app, "position change hide")
    } else if live_running {
        overlay::show_overlay_on_main_thread(
            app,
            OverlayState::Recording,
            settings.floating_overlay_position,
            "position change show recording",
        )
    } else {
        overlay::hide_overlay_on_main_thread(app, "position change hide inactive")
    };
    if let Err(error) = result {
        eprintln!("[overlay] position change failed: {error}");
    }
}

async fn resolve_microphones_for_ui(selected_microphone: String) -> Vec<MicrophoneDevice> {
    let fallback_selected_microphone = selected_microphone.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (sender, receiver) = mpsc::channel();
        let selected_for_worker = selected_microphone.clone();
        thread::spawn(move || {
            let devices =
                microphone_devices_from_input_devices(&selected_for_worker, list_input_devices());
            let _ = sender.send(devices);
        });

        receiver
            .recv_timeout(Duration::from_millis(600))
            .unwrap_or_else(|_| fallback_microphones(&selected_microphone))
    })
    .await
    .unwrap_or_else(|_| fallback_microphones(&fallback_selected_microphone))
}

fn fallback_microphones(selected_microphone: &str) -> Vec<MicrophoneDevice> {
    vec![MicrophoneDevice {
        id: "default".to_string(),
        label: "System Default".to_string(),
        available: true,
        fallback: selected_microphone != "default",
    }]
}

fn resolve_capture_device_with_timeout(
    device_id: String,
) -> Result<ResolvedCpalInputDevice, LiveCaptureError> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(CpalAudioInput::resolve_device(&device_id));
    });

    match receiver.recv_timeout(AUDIO_DEVICE_RESOLVE_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(LiveCaptureError::Cpal(
            "audio device lookup did not finish within 2 seconds".to_string(),
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(LiveCaptureError::Cpal(
            "audio device lookup disconnected".to_string(),
        )),
    }
}

#[tauri::command]
fn app_status(
    app: AppHandle,
    state: State<'_, BackendState>,
    live_state: State<'_, LiveCaptureState>,
) -> Result<AppStatus, String> {
    let (dropped_frames, stream_error) = live_state
        .lock()
        .map(|live_capture| {
            (
                live_capture.dropped_frame_count(),
                live_capture.runtime_error(),
            )
        })
        .unwrap_or((0, None));
    let backend = state.lock().map_err(|error| error.to_string())?;
    let settings = backend.settings();
    let status = with_live_runtime_warning(backend.app_status(), dropped_frames, stream_error);
    update_tray_presentation(&app, &settings, &status);
    Ok(status)
}

#[tauri::command]
async fn list_microphones(state: State<'_, BackendState>) -> Result<Vec<MicrophoneDevice>, String> {
    let selected_microphone = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        backend.settings().selected_microphone
    };
    Ok(resolve_microphones_for_ui(selected_microphone).await)
}

#[tauri::command]
fn list_models(state: State<'_, BackendState>) -> Result<Vec<ModelDescriptor>, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.model_registry())
}

#[tauri::command]
fn verify_model(
    state: State<'_, BackendState>,
    model_id: String,
) -> Result<Vec<ModelDescriptor>, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    backend.verify_model(&model_id)
}

#[tauri::command]
fn download_model(
    state: State<'_, BackendState>,
    model_id: String,
) -> Result<Vec<ModelDescriptor>, String> {
    let prepared = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        backend.prepare_model_download(&model_id)?
    };
    let model_directory = prepared.model_directory.clone();
    let model = prepared.model.clone();
    thread::spawn(move || {
        let store = ModelStore::new(model_directory);
        let _ = store.download_model(&model);
    });

    Ok(prepared.registry)
}

#[tauri::command]
fn cancel_model_download(
    state: State<'_, BackendState>,
    model_id: String,
) -> Result<Vec<ModelDescriptor>, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    backend.cancel_model_download(&model_id)
}

#[tauri::command]
fn delete_model(
    state: State<'_, BackendState>,
    model_id: String,
) -> Result<Vec<ModelDescriptor>, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    backend.delete_model(&model_id)
}

#[tauri::command]
fn queue_snapshot(state: State<'_, BackendState>) -> Result<QueueSnapshot, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.queue_snapshot())
}

#[tauri::command]
fn enqueue_audio_file(
    app: AppHandle,
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    audio_path: String,
    model_id: Option<String>,
) -> Result<QueueSnapshot, String> {
    let snapshot = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.enqueue_audio_file(audio_path, model_id)
    };
    kick_transcription_worker_if_needed(
        app,
        state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(snapshot)
}

#[tauri::command]
fn enqueue_backlog(
    app: AppHandle,
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    save_root: String,
) -> Result<QueueSnapshot, String> {
    let snapshot = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend
            .enqueue_backlog(save_root)
            .map_err(|error| error.to_string())?
    };
    kick_transcription_worker_if_needed(
        app,
        state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(snapshot)
}

#[tauri::command]
fn retry_job(
    app: AppHandle,
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    id: u64,
) -> Result<QueueSnapshot, String> {
    let snapshot = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.retry_job(id)?
    };
    kick_transcription_worker_if_needed(
        app,
        state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(snapshot)
}

#[tauri::command]
fn skip_job(state: State<'_, BackendState>, id: u64) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.skip_job(id)
}

#[tauri::command]
fn cancel_current_transcription(state: State<'_, BackendState>) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.cancel_current_transcription()
}

#[tauri::command]
fn cancel_current_operation(app: AppHandle, state: State<'_, BackendState>) -> Result<(), String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.cancel_current_operation()?;
    update_tray_presentation(&app, &backend.settings(), &backend.app_status());
    Ok(())
}

#[tauri::command]
fn reveal_save_folder(state: State<'_, BackendState>) -> Result<(), String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    let request = reveal_save_folder_request(&backend.settings());
    Command::new(request.program)
        .arg(request.path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn process_next_transcription(state: State<'_, BackendState>) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.process_next_transcription()
}

#[tauri::command]
fn start_live_capture(
    app: AppHandle,
    backend_state: State<'_, BackendState>,
    live_state: State<'_, LiveCaptureState>,
    transcription_state: State<'_, AutoTranscriptionState>,
) -> Result<AppStatus, String> {
    start_live_capture_runtime(
        &app,
        backend_state.inner(),
        live_state.inner(),
        transcription_state.inner().clone(),
    )
}

fn start_live_capture_runtime(
    app: &AppHandle,
    backend_state: &BackendState,
    live_state: &LiveCaptureState,
    transcription_state: AutoTranscriptionState,
) -> Result<AppStatus, String> {
    let (is_running, stream_error, dropped_frames) = {
        let live_capture = live_state.lock().map_err(|error| error.to_string())?;
        (
            live_capture.is_running(),
            live_capture.runtime_error(),
            live_capture.dropped_frame_count(),
        )
    };
    if is_running && stream_error.is_none() {
        let backend = backend_state.lock().map_err(|error| error.to_string())?;
        return Ok(with_live_runtime_warning(
            backend.app_status(),
            dropped_frames,
            None,
        ));
    }
    if stream_error.is_some() {
        live_state.lock().map_err(|error| error.to_string())?.stop();
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        backend.stop_capture_session()?;
    }

    let settings = {
        let backend = backend_state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        if settings.pause_all || !settings.recording_enabled {
            return Ok(backend.app_status());
        }
        settings
    };

    let resolve_device_id = if settings.selected_microphone == "default" {
        settings.selected_microphone.as_str()
    } else {
        "default"
    };
    let resolved = match resolve_capture_device_with_timeout(resolve_device_id.to_string()) {
        Ok(resolved) => resolved,
        Err(error) => {
            let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
            return Ok(backend.capture_start_failed(format!("Microphone unavailable: {error}")));
        }
    };
    let used_fallback_device = settings.selected_microphone != resolved.device_id;

    let (device_id, sample_rate, overlay_position) = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        backend.start_capture_session_with_device(
            resolved.sample_rate,
            chrono::Utc::now(),
            resolved.device_id.clone(),
            resolved.device_name,
            used_fallback_device || resolved.used_fallback_device,
        )?;
        (
            resolved.device_id,
            resolved.sample_rate,
            backend.settings().floating_overlay_position,
        )
    };

    let backend_arc = Arc::clone(backend_state);
    let callback_backend = backend_arc.clone();
    let callback_transcription = transcription_state.clone();
    let callback_app = app.clone();
    let start_result = live_state.lock().map_err(|error| error.to_string())?.start(
        AudioInputConfig {
            device_id,
            sample_rate: Some(sample_rate),
        },
        move |frame| {
            let should_kick = if let Ok(mut backend) = callback_backend.lock() {
                backend
                    .process_audio_frame(frame)
                    .map(|_| backend.should_process_transcriptions())
                    .unwrap_or(false)
            } else {
                false
            };
            if should_kick {
                kick_transcription_worker(
                    callback_app.clone(),
                    callback_backend.clone(),
                    callback_transcription.clone(),
                );
            }
        },
    );

    if let Err(error) = start_result {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        return Ok(backend.capture_start_failed(format!("Microphone capture failed: {error}")));
    }

    if let Err(error) = overlay::show_overlay_on_main_thread(
        app,
        OverlayState::Recording,
        overlay_position,
        "show recording",
    ) {
        eprintln!("[overlay] show recording failed: {error}");
    }

    let (dropped_frames, runtime_error) = live_state
        .lock()
        .map(|live_capture| {
            (
                live_capture.dropped_frame_count(),
                live_capture.runtime_error(),
            )
        })
        .unwrap_or((0, None));
    let backend = backend_state.lock().map_err(|error| error.to_string())?;
    Ok(with_live_runtime_warning(
        backend.app_status(),
        dropped_frames,
        runtime_error,
    ))
}

#[tauri::command]
fn stop_live_capture(
    app: AppHandle,
    backend_state: State<'_, BackendState>,
    live_state: State<'_, LiveCaptureState>,
    transcription_state: State<'_, AutoTranscriptionState>,
) -> Result<AppStatus, String> {
    let status = stop_live_capture_runtime(&app, backend_state.inner(), live_state.inner())?;
    kick_transcription_worker_if_needed(
        app,
        backend_state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(status)
}

fn stop_live_capture_runtime(
    app: &AppHandle,
    backend_state: &BackendState,
    live_state: &LiveCaptureState,
) -> Result<AppStatus, String> {
    live_state.lock().map_err(|error| error.to_string())?.stop();
    let status = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        backend.stop_capture_session()?
    };
    let queue_idle = backend_state
        .lock()
        .map(|backend| {
            let snapshot = backend.queue_snapshot();
            snapshot.running_count == 0 && snapshot.pending_count == 0
        })
        .unwrap_or(true);
    if queue_idle {
        if let Err(error) = overlay::hide_overlay_on_main_thread(app, "hide") {
            eprintln!("[overlay] hide failed: {error}");
        }
    }
    Ok(status)
}

fn kick_transcription_worker_if_needed(
    app: AppHandle,
    backend_state: BackendState,
    transcription_state: AutoTranscriptionState,
) {
    let should_process = backend_state
        .lock()
        .map(|backend| backend.should_process_transcriptions())
        .unwrap_or(false);
    if should_process {
        kick_transcription_worker(app, backend_state, transcription_state);
    }
}

fn wire_live_transcription(
    app_handle: AppHandle,
    backend_state: BackendState,
    live_transcriber_state: LiveTranscriberState,
) {
    let model_directory = backend_state
        .lock()
        .map(|backend| backend.settings().model_directory)
        .unwrap_or_default();
    eprintln!("[sagwan] wire_live_transcription: model_dir={model_directory}");

    let app_for_partial = app_handle.clone();
    let on_partial: Arc<dyn Fn(LivePartialEvent) + Send + Sync> = Arc::new(
        move |event| match event {
            LivePartialEvent::Text(result) => {
                eprintln!(
                    "[sagwan] live partial -> FE chunk_id={} text='{}'",
                    result.chunk_id, result.text
                );
                if let Err(error) = app_for_partial.emit(
                    EVENT_LIVE_PARTIAL,
                    LivePartialPayload {
                        chunk_id: result.chunk_id,
                        text: result.text,
                    },
                ) {
                    eprintln!("[sagwan] WARN failed to emit partial: {error}");
                }
            }
            LivePartialEvent::ModelMissing { chunk_id, model_id } => {
                eprintln!(
                    "[sagwan] live partial: model missing chunk_id={chunk_id} model={model_id}"
                );
                let error = format!(
                    "Live transcription model {model_id} is not installed. Open Models tab to download it.",
                );
                if let Err(emit_error) = app_for_partial.emit(
                    EVENT_LIVE_FAILED,
                    LiveFailedPayload {
                        chunk_id: Some(chunk_id),
                        audio_path: String::new(),
                        error,
                    },
                ) {
                    eprintln!("[sagwan] WARN failed to emit live-failed: {emit_error}");
                }
            }
            LivePartialEvent::EngineError { chunk_id, message } => {
                eprintln!(
                    "[sagwan] live partial: engine error chunk_id={chunk_id} message={message}"
                );
                if let Err(emit_error) = app_for_partial.emit(
                    EVENT_LIVE_FAILED,
                    LiveFailedPayload {
                        chunk_id: Some(chunk_id),
                        audio_path: String::new(),
                        error: format!("Live partial decode failed: {message}"),
                    },
                ) {
                    eprintln!("[sagwan] WARN failed to emit live-failed: {emit_error}");
                }
            }
        },
    );

    let service = Arc::new(LiveTranscriptionService::new(&model_directory, on_partial));
    if let Ok(mut slot) = live_transcriber_state.lock() {
        *slot = Some(service.clone());
    }

    let app_for_handler = app_handle.clone();
    let service_for_handler = service.clone();
    let handler: sagwan::commands::LiveEventHandler = Arc::new(move |event| match event {
        LiveTranscriptEvent::Started {
            chunk_id,
            started_at,
        } => {
            eprintln!("[sagwan] handler: emit started chunk_id={chunk_id}");
            if let Err(error) = app_for_handler.emit(
                EVENT_LIVE_STARTED,
                LiveStartedPayload {
                    chunk_id,
                    started_at: started_at.to_rfc3339(),
                },
            ) {
                eprintln!("[sagwan] WARN emit started failed: {error}");
            }
        }
        LiveTranscriptEvent::SamplesReady {
            chunk_id,
            model_id,
            sample_rate,
            samples,
        } => {
            eprintln!(
                    "[sagwan] handler: submit live partial chunk_id={chunk_id} model={model_id} samples={} rate={sample_rate}",
                    samples.len()
                );
            service_for_handler.submit(LivePartialRequest {
                chunk_id,
                model_id,
                sample_rate,
                samples,
            });
        }
        LiveTranscriptEvent::Committed {
            chunk_id,
            audio_path,
        } => {
            eprintln!(
                "[sagwan] handler: emit committed chunk_id={chunk_id} path={}",
                audio_path.display()
            );
            service_for_handler.cancel_chunk(chunk_id);
            if let Err(error) = app_for_handler.emit(
                EVENT_LIVE_COMMITTED,
                LiveCommittedPayload {
                    chunk_id,
                    audio_path: audio_path.to_string_lossy().to_string(),
                },
            ) {
                eprintln!("[sagwan] WARN emit committed failed: {error}");
            }
        }
    });

    if let Ok(mut backend) = backend_state.lock() {
        backend.set_live_event_handler(handler);
    }
}

fn kick_transcription_worker(
    app: AppHandle,
    backend_state: BackendState,
    transcription_state: AutoTranscriptionState,
) {
    if transcription_state
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    thread::spawn(move || {
        eprintln!("[sagwan] queue worker thread started");
        loop {
            let Some(started) = (match backend_state.lock() {
                Ok(mut backend) => backend.start_next_transcription_job(),
                Err(_) => break,
            }) else {
                eprintln!("[sagwan] queue worker: no pending jobs, exiting loop");
                break;
            };

            let audio_path = started.job.audio_path.clone();
            eprintln!(
                "[sagwan] queue worker: processing job id={} path={} model={}",
                started.job.id,
                audio_path.display(),
                started.job.model_id
            );

            let overlay_position = backend_state
                .lock()
                .map(|backend| backend.settings().floating_overlay_position)
                .unwrap_or(FloatingOverlayPosition::Top);
            if let Err(error) = overlay::show_overlay_on_main_thread(
                &app,
                OverlayState::Transcribing,
                overlay_position,
                "show transcribing",
            ) {
                eprintln!("[overlay] show transcribing failed: {error}");
            }

            let worker = TranscriptionWorker::new(WhisperTranscriber::new(started.model_directory));
            let outcome = worker
                .process_started_job(&started.job)
                .unwrap_or_else(|error| {
                    eprintln!("[sagwan] queue worker: process_started_job error: {error}");
                    TranscriptionJobOutcome::failed(started.job.id, error.to_string())
                });
            eprintln!(
                "[sagwan] queue worker: job id={} outcome={:?}",
                started.job.id, outcome.status
            );

            emit_outcome_to_frontend(&app, &backend_state, &audio_path, &outcome);

            match backend_state.lock() {
                Ok(mut backend) => {
                    let _ = backend.finish_transcription_job(outcome);
                }
                Err(_) => break,
            }
        }

        transcription_state.store(false, Ordering::Release);

        let live_running = app
            .try_state::<LiveCaptureState>()
            .and_then(|state| state.lock().ok().map(|live| live.is_running()))
            .unwrap_or(false);
        let queue_pending = backend_state
            .lock()
            .map(|backend| backend.queue_snapshot().pending_count > 0)
            .unwrap_or(false);
        if !live_running && !queue_pending {
            if let Err(error) =
                overlay::hide_overlay_on_main_thread(&app, "hide after worker drained")
            {
                eprintln!("[overlay] hide after worker drained failed: {error}");
            }
        } else if live_running && !queue_pending {
            let overlay_position = backend_state
                .lock()
                .map(|backend| backend.settings().floating_overlay_position)
                .unwrap_or(FloatingOverlayPosition::Top);
            if let Err(error) = overlay::show_overlay_on_main_thread(
                &app,
                OverlayState::Recording,
                overlay_position,
                "revert to recording",
            ) {
                eprintln!("[overlay] revert to recording failed: {error}");
            }
        }

        kick_transcription_worker_if_needed(app, backend_state, transcription_state);
    });
}

fn emit_outcome_to_frontend(
    app: &AppHandle,
    backend_state: &BackendState,
    audio_path: &Path,
    outcome: &TranscriptionJobOutcome,
) {
    let chunk_id = backend_state
        .lock()
        .ok()
        .and_then(|backend| backend.chunk_id_for_audio_path(audio_path))
        .or_else(|| chunk_id_from_metadata(audio_path));
    let audio_path_str = audio_path.to_string_lossy().to_string();

    match &outcome.status {
        TranscriptionJobStatus::Completed => {
            let transcript_path = audio_path.with_extension("txt");
            let text = std::fs::read_to_string(&transcript_path)
                .map(|content| content.trim_end().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                eprintln!(
                    "[sagwan] emit_outcome_to_frontend: empty sidecar at {}",
                    transcript_path.display()
                );
                return;
            }
            eprintln!(
                "[sagwan] emit final chunk_id={:?} path={} text_len={}",
                chunk_id,
                audio_path_str,
                text.len()
            );
            if let Err(error) = app.emit(
                EVENT_LIVE_FINAL,
                LiveFinalPayload {
                    chunk_id,
                    audio_path: audio_path_str,
                    text,
                },
            ) {
                eprintln!("[sagwan] WARN emit final failed: {error}");
            }
        }
        TranscriptionJobStatus::Failed(error) => {
            eprintln!(
                "[sagwan] emit failed chunk_id={:?} path={} error={}",
                chunk_id, audio_path_str, error
            );
            if let Err(emit_error) = app.emit(
                EVENT_LIVE_FAILED,
                LiveFailedPayload {
                    chunk_id,
                    audio_path: audio_path_str,
                    error: error.clone(),
                },
            ) {
                eprintln!("[sagwan] WARN emit failed event failed: {emit_error}");
            }
        }
    }
}

fn chunk_id_from_metadata(audio_path: &Path) -> Option<u64> {
    let metadata_path = audio_path.with_extension("json");
    let bytes = std::fs::read(&metadata_path).ok()?;
    let metadata: ChunkMetadata = serde_json::from_slice(&bytes).ok()?;
    metadata.live_capture_chunk_id
}

fn apply_launch_at_login_action(
    app: &AppHandle,
    action: LaunchAtLoginAction,
) -> Result<(), String> {
    match action {
        LaunchAtLoginAction::Enable => apply_launch_at_login_preference(app, true),
        LaunchAtLoginAction::Disable => apply_launch_at_login_preference(app, false),
        LaunchAtLoginAction::Unchanged => Ok(()),
    }
}

fn apply_live_capture_runtime_action(
    app: &AppHandle,
    backend_state: &BackendState,
    live_state: &LiveCaptureState,
    transcription_state: AutoTranscriptionState,
    action: LiveCaptureRuntimeAction,
) -> Result<(), String> {
    match action {
        LiveCaptureRuntimeAction::Start => {
            start_live_capture_runtime(app, backend_state, live_state, transcription_state)?;
            Ok(())
        }
        LiveCaptureRuntimeAction::Stop => {
            stop_live_capture_runtime(app, backend_state, live_state)?;
            Ok(())
        }
        LiveCaptureRuntimeAction::Restart => {
            stop_live_capture_runtime(app, backend_state, live_state)?;
            start_live_capture_runtime(app, backend_state, live_state, transcription_state)?;
            Ok(())
        }
        LiveCaptureRuntimeAction::Unchanged => Ok(()),
    }
}

#[cfg(any(target_os = "macos", windows, target_os = "linux"))]
fn apply_launch_at_login_preference(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    let autostart = app.autolaunch();
    if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    }
    .map_err(|error| error.to_string())
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn apply_launch_at_login_preference(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

fn ensure_main_window_visible(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        app.set_activation_policy(tauri::ActivationPolicy::Regular)?;
        app.set_dock_visibility(true)?;
        app.show()?;
    }

    let window = if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window
    } else {
        tauri::WebviewWindowBuilder::new(
            app,
            MAIN_WINDOW_LABEL,
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title(MAIN_WINDOW_TITLE)
        .inner_size(MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)
        .min_inner_size(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT)
        .resizable(true)
        .visible(true)
        .focused(true)
        .build()?
    };

    window.set_min_size(Some(LogicalSize::new(
        MAIN_WINDOW_MIN_WIDTH,
        MAIN_WINDOW_MIN_HEIGHT,
    )))?;
    if window.is_minimized().unwrap_or(false) {
        window.unminimize()?;
    }
    window.show()?;
    let size = window.inner_size()?;
    if main_window_needs_size_restore(size.width, size.height) {
        window.set_size(LogicalSize::new(MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT))?;
        if let Err(error) = window.center() {
            eprintln!("[window] failed to center settings window: {error}");
        }
    }
    if let Err(error) = window.set_focus() {
        eprintln!("[window] failed to focus settings window: {error}");
    }
    Ok(())
}

fn main_window_needs_size_restore(width: u32, height: u32) -> bool {
    width < MAIN_WINDOW_MIN_WIDTH as u32 || height < MAIN_WINDOW_MIN_HEIGHT as u32
}

fn main() {
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if main_window_close_action(window.label()) == MainWindowCloseAction::HideToTray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ))?;

            ensure_main_window_visible(app.handle())?;

            let backend = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| AppBackend::load_from_dir(dir).ok())
                .unwrap_or_default();
            let backend_state = Arc::new(Mutex::new(backend));
            let transcription_state = Arc::new(AtomicBool::new(false));
            let live_transcriber_state: LiveTranscriberState = Arc::new(Mutex::new(None));
            app.manage(backend_state.clone());
            app.manage(transcription_state.clone());
            app.manage(Mutex::new(LiveCaptureRuntime::new(CpalAudioInput)));
            app.manage(live_transcriber_state.clone());

            wire_live_transcription(
                app.handle().clone(),
                backend_state.clone(),
                live_transcriber_state.clone(),
            );
            let initial = backend_state
                .lock()
                .ok()
                .map(|backend| (backend.settings(), backend.app_status()));
            let (initial_settings, initial_status) = match initial {
                Some((settings, status)) => (Some(settings), Some(status)),
                None => (None, None),
            };
            let tray_menu_items =
                setup_tray(app, initial_settings.as_ref(), initial_status.as_ref())?;
            if let Some(settings) = initial_settings.as_ref() {
                let _ = apply_launch_at_login_preference(app.handle(), settings.launch_at_login);
            }
            app.manage(tray_menu_items);
            let app_handle_for_initial = app.handle().clone();
            if initial_settings
                .as_ref()
                .is_some_and(live_capture_should_run)
            {
                let app_handle_for_capture = app.handle().clone();
                let backend_state_for_capture = backend_state.clone();
                let transcription_state_for_capture = transcription_state.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs(2));
                    let live_state = app_handle_for_capture.state::<LiveCaptureState>();
                    match start_live_capture_runtime(
                        &app_handle_for_capture,
                        &backend_state_for_capture,
                        live_state.inner(),
                        transcription_state_for_capture,
                    ) {
                        Ok(status) => {
                            if let Ok(backend) = backend_state_for_capture.lock() {
                                update_tray_presentation(
                                    &app_handle_for_capture,
                                    &backend.settings(),
                                    &status,
                                );
                            }
                        }
                        Err(error) => eprintln!("[capture] initial live capture failed: {error}"),
                    }
                });
            }
            kick_transcription_worker_if_needed(
                app_handle_for_initial,
                backend_state,
                transcription_state,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            app_status,
            list_microphones,
            list_models,
            verify_model,
            download_model,
            cancel_model_download,
            delete_model,
            queue_snapshot,
            enqueue_audio_file,
            enqueue_backlog,
            retry_job,
            skip_job,
            cancel_current_transcription,
            cancel_current_operation,
            reveal_save_folder,
            process_next_transcription,
            start_live_capture,
            stop_live_capture
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Sagwan");
}

fn setup_tray(
    app: &mut tauri::App,
    initial_settings: Option<&AppSettings>,
    initial_status: Option<&AppStatus>,
) -> tauri::Result<TrayMenuItems> {
    let initial_menu = initial_settings
        .zip(initial_status)
        .map(|(settings, status)| tray_menu_presentation(settings, status));
    let recording = CheckMenuItem::with_id(
        app,
        "toggle-recording",
        "Recording On",
        true,
        initial_menu
            .as_ref()
            .map(|menu| menu.recording_checked)
            .unwrap_or(true),
        None::<&str>,
    )?;
    let transcription = CheckMenuItem::with_id(
        app,
        "toggle-transcription",
        "Transcription On",
        true,
        initial_menu
            .as_ref()
            .map(|menu| menu.transcription_checked)
            .unwrap_or(true),
        None::<&str>,
    )?;
    let active_model = MenuItem::with_id(
        app,
        "active-model",
        initial_menu
            .as_ref()
            .map(|menu| menu.active_model_text.as_str())
            .unwrap_or("Model: whisper-medium"),
        false,
        None::<&str>,
    )?;
    let active_mic = MenuItem::with_id(
        app,
        "active-microphone",
        initial_menu
            .as_ref()
            .map(|menu| menu.active_microphone_text.as_str())
            .unwrap_or("Microphone: System Default"),
        false,
        None::<&str>,
    )?;
    let threshold = MenuItem::with_id(
        app,
        "threshold",
        initial_menu
            .as_ref()
            .map(|menu| menu.threshold_text.as_str())
            .unwrap_or("Threshold: -45 dBFS"),
        false,
        None::<&str>,
    )?;
    let reveal = MenuItem::with_id(
        app,
        "reveal-save-folder",
        "Reveal Save Folder",
        true,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, "open-settings", "Open Settings", true, None::<&str>)?;
    let pause_all = CheckMenuItem::with_id(
        app,
        "pause-all",
        "Pause All",
        true,
        initial_menu
            .as_ref()
            .map(|menu| menu.pause_all_checked)
            .unwrap_or(false),
        None::<&str>,
    )?;
    let cancel = MenuItem::with_id(
        app,
        "cancel-current-operation",
        "Cancel Current Operation",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, Some("CmdOrCtrl+Q"))?;
    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let separator_three = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &recording,
            &transcription,
            &active_model,
            &active_mic,
            &threshold,
            &separator_one,
            &reveal,
            &open,
            &pause_all,
            &separator_two,
            &cancel,
            &separator_three,
            &quit,
        ],
    )?;

    let presentation = initial_settings
        .zip(initial_status)
        .map(|(settings, status)| tray_runtime_presentation(settings, status))
        .unwrap_or_else(|| sagwan::commands::TrayRuntimePresentation {
            icon: tray_presentation_for_state(sagwan::commands::TrayState::Listening),
            visible: true,
        });
    let icon = Image::new_owned(presentation.icon.rgba.to_vec(), 1, 1);
    let tray = TrayIconBuilder::with_id("sagwan")
        .tooltip(presentation.icon.tooltip)
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            handle_tray_menu(app, event.id().as_ref());
        })
        .build(app)?;
    tray.set_visible(presentation.visible)?;

    Ok(TrayMenuItems {
        recording,
        transcription,
        active_model,
        active_mic,
        threshold,
        pause_all,
    })
}

fn update_tray_presentation(app: &tauri::AppHandle, settings: &AppSettings, status: &AppStatus) {
    let Some(tray) = app.tray_by_id("sagwan") else {
        return;
    };
    let presentation = tray_runtime_presentation(settings, status);
    let _ = tray.set_icon(Some(Image::new_owned(
        presentation.icon.rgba.to_vec(),
        1,
        1,
    )));
    let _ = tray.set_tooltip(Some(presentation.icon.tooltip));
    let _ = tray.set_visible(presentation.visible);

    if let Some(items) = app.try_state::<TrayMenuItems>() {
        let menu = tray_menu_presentation(settings, status);
        let _ = items.recording.set_checked(menu.recording_checked);
        let _ = items.transcription.set_checked(menu.transcription_checked);
        let _ = items.pause_all.set_checked(menu.pause_all_checked);
        let _ = items.active_model.set_text(menu.active_model_text);
        let _ = items.active_mic.set_text(menu.active_microphone_text);
        let _ = items.threshold.set_text(menu.threshold_text);
    }
}

fn handle_tray_menu(app: &tauri::AppHandle, id: &str) {
    match id {
        "toggle-recording" => patch_from_tray(app, |settings| SettingsPatch {
            recording_enabled: Some(!settings.recording_enabled),
            ..SettingsPatch::default()
        }),
        "toggle-transcription" => patch_from_tray(app, |settings| SettingsPatch {
            transcription_enabled: Some(!settings.transcription_enabled),
            ..SettingsPatch::default()
        }),
        "pause-all" => patch_from_tray(app, |settings| SettingsPatch {
            pause_all: Some(!settings.pause_all),
            ..SettingsPatch::default()
        }),
        "reveal-save-folder" => {
            let state = app.state::<BackendState>();
            if let Ok(backend) = state.lock() {
                let request = reveal_save_folder_request(&backend.settings());
                let _ = Command::new(request.program).arg(request.path).spawn();
            }
        }
        "cancel-current-operation" => {
            let state = app.state::<BackendState>();
            if let Ok(mut backend) = state.lock() {
                let _ = backend.cancel_current_operation();
                update_tray_presentation(app, &backend.settings(), &backend.app_status());
            }
        }
        "open-settings" => {
            if let Err(error) = ensure_main_window_visible(app) {
                eprintln!("[window] failed to open settings window: {error}");
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_window_size_restore_detects_zero_sized_window() {
        assert!(main_window_needs_size_restore(0, 0));
    }

    #[test]
    fn main_window_size_restore_detects_too_small_window() {
        assert!(main_window_needs_size_restore(
            100,
            MAIN_WINDOW_MIN_HEIGHT as u32
        ));
        assert!(main_window_needs_size_restore(
            MAIN_WINDOW_MIN_WIDTH as u32,
            100
        ));
    }

    #[test]
    fn main_window_size_restore_keeps_usable_window_size() {
        assert!(!main_window_needs_size_restore(
            MAIN_WINDOW_MIN_WIDTH as u32,
            MAIN_WINDOW_MIN_HEIGHT as u32,
        ));
    }

    #[test]
    fn fallback_microphones_returns_default_device_without_fallback_for_default_selection() {
        let devices = fallback_microphones("default");

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "default");
        assert!(!devices[0].fallback);
    }

    #[test]
    fn fallback_microphones_marks_default_as_fallback_for_pinned_selection() {
        let devices = fallback_microphones("input-0-external");

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "default");
        assert!(devices[0].fallback);
    }
}

fn patch_from_tray(app: &tauri::AppHandle, patch: impl FnOnce(AppSettings) -> SettingsPatch) {
    let state = app.state::<BackendState>();
    let live_state = app.state::<LiveCaptureState>();
    let transcription_state = app.state::<AutoTranscriptionState>();
    let live_capture_action = if let Ok(mut backend) = state.lock() {
        let current = backend.settings();
        let patch = patch(current.clone());
        let live_capture_action = live_capture_runtime_action_for_patch(&current, &patch);
        backend.update_settings(patch);
        live_capture_action
    } else {
        return;
    };

    let _ = apply_live_capture_runtime_action(
        app,
        state.inner(),
        live_state.inner(),
        transcription_state.inner().clone(),
        live_capture_action,
    );
    if let Ok(backend) = state.lock() {
        update_tray_presentation(app, &backend.settings(), &backend.app_status());
    }
    kick_transcription_worker_if_needed(
        app.clone(),
        state.inner().clone(),
        transcription_state.inner().clone(),
    );
}
