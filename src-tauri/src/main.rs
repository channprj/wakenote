use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

use sagwan::commands::{
    AppBackend, AppStatus, MainWindowCloseAction, MicrophoneDevice, main_window_close_action,
    reveal_save_folder_request, tray_menu_presentation, tray_presentation_for_state,
    tray_runtime_presentation,
};
use sagwan::live_capture::{AudioInputConfig, CpalAudioInput, LiveCaptureRuntime};
use sagwan::models::{ModelDescriptor, ModelStore};
use sagwan::queue::QueueSnapshot;
use sagwan::settings::{
    AppSettings, LaunchAtLoginAction, LiveCaptureRuntimeAction, SettingsPatch,
    launch_at_login_action_for_patch, live_capture_runtime_action_for_patch,
};
use sagwan::transcription::{TranscriptionJobOutcome, TranscriptionWorker, WhisperTranscriber};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State, Wry};

type BackendState = Arc<Mutex<AppBackend>>;
type LiveCaptureState = Mutex<LiveCaptureRuntime<CpalAudioInput>>;
type AutoTranscriptionState = Arc<AtomicBool>;

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
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let (launch_at_login_action, live_capture_action) = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        (
            launch_at_login_action_for_patch(&settings, &patch),
            live_capture_runtime_action_for_patch(&settings, &patch),
        )
    };
    apply_launch_at_login_action(&app, launch_at_login_action)?;

    let settings = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.update_settings(patch)
    };
    apply_live_capture_runtime_action(state.inner(), live_state.inner(), live_capture_action)?;
    kick_transcription_worker_if_needed(state.inner().clone(), transcription_state.inner().clone());
    Ok(settings)
}

#[tauri::command]
fn app_status(app: AppHandle, state: State<'_, BackendState>) -> Result<AppStatus, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    let settings = backend.settings();
    let status = backend.app_status();
    update_tray_presentation(&app, &settings, &status);
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
    let store = ModelStore::new(model_directory);
    let registry = store
        .load_model_registry()
        .map_err(|error| error.to_string())?;
    let model = registry
        .get(&model_id)
        .ok_or_else(|| format!("unknown model {model_id}"))?;
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
    backend_state: &BackendState,
    live_state: &LiveCaptureState,
    action: LiveCaptureRuntimeAction,
) -> Result<(), String> {
    match action {
        LiveCaptureRuntimeAction::Stop => {
            live_state.lock().map_err(|error| error.to_string())?.stop();
            backend_state
                .lock()
                .map_err(|error| error.to_string())?
                .stop_capture_session()?;
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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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

    let _ =
        apply_live_capture_runtime_action(state.inner(), live_state.inner(), live_capture_action);
    if let Ok(backend) = state.lock() {
        update_tray_presentation(app, &backend.settings(), &backend.app_status());
    }
    kick_transcription_worker_if_needed(state.inner().clone(), transcription_state.inner().clone());
}
