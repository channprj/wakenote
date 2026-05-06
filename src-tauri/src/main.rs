use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

use sagwan::commands::{AppBackend, AppStatus, MicrophoneDevice, tray_presentation_for_state};
use sagwan::live_capture::{AudioInputConfig, CpalAudioInput, LiveCaptureRuntime};
use sagwan::models::{ModelDescriptor, ModelStore, default_model_registry};
use sagwan::queue::QueueSnapshot;
use sagwan::settings::{AppSettings, SettingsPatch};
use sagwan::transcription::{TranscriptionJobOutcome, TranscriptionWorker, WhisperTranscriber};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};

type BackendState = Arc<Mutex<AppBackend>>;
type LiveCaptureState = Mutex<LiveCaptureRuntime<CpalAudioInput>>;
type AutoTranscriptionState = Arc<AtomicBool>;

#[tauri::command]
fn get_settings(state: State<'_, BackendState>) -> Result<AppSettings, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.settings())
}

#[tauri::command]
fn update_settings(
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let settings = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.update_settings(patch)
    };
    kick_transcription_worker_if_needed(state.inner().clone(), transcription_state.inner().clone());
    Ok(settings)
}

#[tauri::command]
fn app_status(app: AppHandle, state: State<'_, BackendState>) -> Result<AppStatus, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    let status = backend.app_status();
    update_tray_presentation(&app, &status);
    Ok(status)
}

#[tauri::command]
fn list_microphones(state: State<'_, BackendState>) -> Result<Vec<MicrophoneDevice>, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.list_microphones())
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
    let model_directory = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        backend.settings().model_directory
    };
    let registry = default_model_registry();
    let model = registry
        .get(&model_id)
        .ok_or_else(|| format!("unknown model {model_id}"))?;
    let store = ModelStore::new(model_directory);
    store
        .download_model(model)
        .map_err(|error| error.to_string())?;

    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.model_registry())
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
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    audio_path: String,
    model_id: Option<String>,
) -> Result<QueueSnapshot, String> {
    let snapshot = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.enqueue_audio_file(audio_path, model_id)
    };
    kick_transcription_worker_if_needed(state.inner().clone(), transcription_state.inner().clone());
    Ok(snapshot)
}

#[tauri::command]
fn enqueue_backlog(
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
    kick_transcription_worker_if_needed(state.inner().clone(), transcription_state.inner().clone());
    Ok(snapshot)
}

#[tauri::command]
fn retry_job(
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    id: u64,
) -> Result<QueueSnapshot, String> {
    let snapshot = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.retry_job(id)?
    };
    kick_transcription_worker_if_needed(state.inner().clone(), transcription_state.inner().clone());
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
fn process_next_transcription(state: State<'_, BackendState>) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.process_next_transcription()
}

#[tauri::command]
fn start_live_capture(
    backend_state: State<'_, BackendState>,
    live_state: State<'_, LiveCaptureState>,
    transcription_state: State<'_, AutoTranscriptionState>,
) -> Result<AppStatus, String> {
    let mut live_capture = live_state.lock().map_err(|error| error.to_string())?;
    if live_capture.is_running() {
        let backend = backend_state.lock().map_err(|error| error.to_string())?;
        return Ok(backend.app_status());
    }

    let (device_id, sample_rate) = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        if settings.pause_all || !settings.recording_enabled {
            return Ok(backend.app_status());
        }
        let resolved = CpalAudioInput::resolve_device(&settings.selected_microphone)
            .map_err(|error| error.to_string())?;
        backend.start_capture_session_with_device(
            resolved.sample_rate,
            chrono::Utc::now(),
            resolved.device_id.clone(),
            resolved.device_name,
            resolved.used_fallback_device,
        )?;
        (resolved.device_id, resolved.sample_rate)
    };

    let backend_arc = backend_state.inner().clone();
    let callback_backend = backend_arc.clone();
    let callback_transcription = transcription_state.inner().clone();
    let start_result = live_capture.start(
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
                kick_transcription_worker(callback_backend.clone(), callback_transcription.clone());
            }
        },
    );

    if let Err(error) = start_result {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        let _ = backend.stop_capture_session();
        return Err(error.to_string());
    }

    let backend = backend_state.lock().map_err(|error| error.to_string())?;
    Ok(backend.app_status())
}

#[tauri::command]
fn stop_live_capture(
    backend_state: State<'_, BackendState>,
    live_state: State<'_, LiveCaptureState>,
    transcription_state: State<'_, AutoTranscriptionState>,
) -> Result<AppStatus, String> {
    live_state.lock().map_err(|error| error.to_string())?.stop();
    let status = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        backend.stop_capture_session()?
    };
    kick_transcription_worker_if_needed(
        backend_state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(status)
}

fn kick_transcription_worker_if_needed(
    backend_state: BackendState,
    transcription_state: AutoTranscriptionState,
) {
    let should_process = backend_state
        .lock()
        .map(|backend| backend.should_process_transcriptions())
        .unwrap_or(false);
    if should_process {
        kick_transcription_worker(backend_state, transcription_state);
    }
}

fn kick_transcription_worker(
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
        loop {
            let Some(started) = (match backend_state.lock() {
                Ok(mut backend) => backend.start_next_transcription_job(),
                Err(_) => break,
            }) else {
                break;
            };

            let worker = TranscriptionWorker::new(WhisperTranscriber::new(started.model_directory));
            let outcome = worker
                .process_started_job(&started.job)
                .unwrap_or_else(|error| {
                    TranscriptionJobOutcome::failed(started.job.id, error.to_string())
                });

            match backend_state.lock() {
                Ok(mut backend) => {
                    let _ = backend.finish_transcription_job(outcome);
                }
                Err(_) => break,
            }
        }

        transcription_state.store(false, Ordering::Release);
        kick_transcription_worker_if_needed(backend_state, transcription_state);
    });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let backend = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| AppBackend::load_from_dir(dir).ok())
                .unwrap_or_default();
            let backend_state = Arc::new(Mutex::new(backend));
            let transcription_state = Arc::new(AtomicBool::new(false));
            app.manage(backend_state.clone());
            app.manage(transcription_state.clone());
            app.manage(Mutex::new(LiveCaptureRuntime::new(CpalAudioInput)));
            let initial_status = backend_state
                .lock()
                .ok()
                .map(|backend| backend.app_status());
            setup_tray(app, initial_status.as_ref())?;
            kick_transcription_worker_if_needed(backend_state, transcription_state);
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
            process_next_transcription,
            start_live_capture,
            stop_live_capture
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Sagwan");
}

fn setup_tray(app: &mut tauri::App, initial_status: Option<&AppStatus>) -> tauri::Result<()> {
    let recording = CheckMenuItem::with_id(
        app,
        "toggle-recording",
        "Recording On",
        true,
        true,
        None::<&str>,
    )?;
    let transcription = CheckMenuItem::with_id(
        app,
        "toggle-transcription",
        "Transcription On",
        true,
        true,
        None::<&str>,
    )?;
    let active_model = MenuItem::with_id(
        app,
        "active-model",
        "Model: whisper-medium",
        false,
        None::<&str>,
    )?;
    let active_mic = MenuItem::with_id(
        app,
        "active-microphone",
        "Microphone: System Default",
        false,
        None::<&str>,
    )?;
    let threshold =
        MenuItem::with_id(app, "threshold", "Threshold: -45 dBFS", false, None::<&str>)?;
    let reveal = MenuItem::with_id(
        app,
        "reveal-save-folder",
        "Reveal Save Folder",
        true,
        None::<&str>,
    )?;
    let open = MenuItem::with_id(app, "open-settings", "Open Settings", true, None::<&str>)?;
    let pause_all =
        CheckMenuItem::with_id(app, "pause-all", "Pause All", true, false, None::<&str>)?;
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

    let presentation = initial_status
        .map(|status| tray_presentation_for_state(status.tray_state))
        .unwrap_or_else(|| tray_presentation_for_state(sagwan::commands::TrayState::Listening));
    let icon = Image::new_owned(presentation.rgba.to_vec(), 1, 1);
    TrayIconBuilder::with_id("sagwan")
        .tooltip(presentation.tooltip)
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            handle_tray_menu(app, event.id().as_ref());
        })
        .build(app)?;

    Ok(())
}

fn update_tray_presentation(app: &tauri::AppHandle, status: &AppStatus) {
    let Some(tray) = app.tray_by_id("sagwan") else {
        return;
    };
    let presentation = tray_presentation_for_state(status.tray_state);
    let _ = tray.set_icon(Some(Image::new_owned(presentation.rgba.to_vec(), 1, 1)));
    let _ = tray.set_tooltip(Some(presentation.tooltip));
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
        "cancel-current-operation" => {
            let state = app.state::<BackendState>();
            if let Ok(mut backend) = state.lock() {
                let _ = backend.cancel_current_transcription();
                update_tray_presentation(app, &backend.app_status());
            }
        }
        "open-settings" => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn patch_from_tray(app: &tauri::AppHandle, patch: impl FnOnce(AppSettings) -> SettingsPatch) {
    let state = app.state::<BackendState>();
    if let Ok(mut backend) = state.lock() {
        let current = backend.settings();
        backend.update_settings(patch(current));
        update_tray_presentation(app, &backend.app_status());
    }
}
