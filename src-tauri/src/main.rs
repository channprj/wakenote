use std::sync::Mutex;

use sagwan::commands::{AppBackend, AppStatus, MicrophoneDevice};
use sagwan::models::ModelDescriptor;
use sagwan::queue::QueueSnapshot;
use sagwan::settings::{AppSettings, SettingsPatch};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State};

type BackendState = Mutex<AppBackend>;

#[tauri::command]
fn get_settings(state: State<'_, BackendState>) -> Result<AppSettings, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.settings())
}

#[tauri::command]
fn update_settings(
    state: State<'_, BackendState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.update_settings(patch))
}

#[tauri::command]
fn app_status(state: State<'_, BackendState>) -> Result<AppStatus, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.app_status())
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
    audio_path: String,
    model_id: Option<String>,
) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    Ok(backend.enqueue_audio_file(audio_path, model_id))
}

#[tauri::command]
fn enqueue_backlog(
    state: State<'_, BackendState>,
    save_root: String,
) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend
        .enqueue_backlog(save_root)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_job(state: State<'_, BackendState>, id: u64) -> Result<QueueSnapshot, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.retry_job(id)
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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let backend = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| AppBackend::load_from_dir(dir).ok())
                .unwrap_or_default();
            app.manage(Mutex::new(backend));
            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            update_settings,
            app_status,
            list_microphones,
            list_models,
            verify_model,
            delete_model,
            queue_snapshot,
            enqueue_audio_file,
            enqueue_backlog,
            retry_job,
            skip_job,
            cancel_current_transcription
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Sagwan");
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
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

    let icon = Image::new(&[0, 71, 171, 255], 1, 1);
    TrayIconBuilder::with_id("sagwan")
        .tooltip("Sagwan")
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
    }
}
