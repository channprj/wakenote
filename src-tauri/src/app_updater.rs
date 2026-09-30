//! Native updater IPC and restart protection for active WakeNote work.
use crate::{
    AutoTranscriptionState, BackendState, DictationStage, DictationState, LlmRunState,
    ManualMeetingRecordingState, MeetingState, SystemCaptureState, mark_intentional_quit,
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use wakenote::updater::{self, RESTART_GATE, UpdateInfo};

fn require_update_window(window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Updates are only available in the main window.".into())
    }
}

fn capture_update_blocker(backend: &wakenote::commands::AppBackend) -> Option<String> {
    backend
        .app_status()
        .live_input_active
        .then(|| "Stop Capture input before installing the update.".into())
}

#[tauri::command]
pub(crate) async fn check_for_update(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<UpdateInfo, String> {
    require_update_window(&window)?;
    let current = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || updater::check(&current))
        .await
        .map_err(|_| "The update check could not finish.".to_string())?
}

fn update_runtime_blocker(app: &AppHandle) -> Result<Option<String>, String> {
    // A poisoned/missing runtime is an error, never evidence that work is idle.
    let unavailable = |_| "Could not verify whether WakeNote is idle.".to_string();
    let backend_state = app.state::<BackendState>();
    let backend = backend_state.lock().map_err(unavailable)?;
    if let Some(reason) = capture_update_blocker(&backend) {
        return Ok(Some(reason));
    }
    if backend.app_status().queue.running_count > 0
        || app
            .state::<AutoTranscriptionState>()
            .load(Ordering::Acquire)
    {
        return Ok(Some(
            "Wait for transcription to finish before installing.".into(),
        ));
    }
    drop(backend);
    if app
        .state::<SystemCaptureState>()
        .lock()
        .map_err(|_| "Could not check system recording.")?
        .is_some()
        || app
            .state::<ManualMeetingRecordingState>()
            .lock()
            .map_err(|_| "Could not check meeting recording.")?
            .session
            .is_some()
    {
        return Ok(Some("Stop the meeting recording before installing.".into()));
    }
    if matches!(
        app.state::<DictationState>()
            .lock()
            .map_err(|_| "Could not check dictation.")?
            .stage(),
        DictationStage::Recording | DictationStage::Transcribing
    ) {
        return Ok(Some(
            "Wait for dictation to finish before installing.".into(),
        ));
    }
    let meeting_state = app.state::<MeetingState>();
    let meeting = meeting_state
        .lock()
        .map_err(|_| "Could not check meeting transcription.")?;
    if meeting.current.is_some() || !meeting.queued.is_empty() {
        return Ok(Some(
            "Wait for meeting transcription to finish before installing.".into(),
        ));
    }
    drop(meeting);
    if app
        .state::<LlmRunState>()
        .lock()
        .map_err(|_| "Could not check report generation.")?
        .active()
        .is_some()
    {
        return Ok(Some(
            "Wait for report generation to finish before installing.".into(),
        ));
    }
    Ok(None)
}

#[tauri::command]
pub(crate) fn update_install_blocker(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<Option<String>, String> {
    require_update_window(&window)?;
    if RESTART_GATE.is_busy()? {
        return Ok(Some(
            "Wait for the current operation to finish before installing.".into(),
        ));
    }
    update_runtime_blocker(&app)
}

#[tauri::command]
pub(crate) fn open_update_release(
    window: tauri::WebviewWindow,
    version: Option<String>,
) -> Result<(), String> {
    require_update_window(&window)?;
    updater::open_release(version.as_deref())
}

#[tauri::command]
pub(crate) async fn download_and_install_update(
    app: AppHandle,
    window: tauri::WebviewWindow,
    expected_version: String,
) -> Result<(), String> {
    require_update_window(&window)?;
    let _download = updater::DownloadGuard::acquire()?;
    {
        let _gate = RESTART_GATE.restart()?;
        if let Some(reason) = update_runtime_blocker(&app)? {
            return Err(reason);
        }
    }
    let current = app.package_info().version.to_string();
    let progress_window = window.clone();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        updater::prepare(&current, &expected_version, |progress| {
            let _ = progress_window.emit("app-update-progress", progress);
        })
    })
    .await
    .map_err(|_| "Could not prepare the update.".to_string())??;
    // A recording/shortcut/worker may have started during the download. The
    // restart gate prevents new starts between this final check and shutdown.
    let gate = RESTART_GATE.restart()?;
    if let Some(reason) = update_runtime_blocker(&app)? {
        return Err(reason);
    }
    prepared.launch()?;
    let _ = window.emit(
        "app-update-progress",
        updater::UpdateProgress {
            phase: "restarting",
            downloaded: 0,
            total: 0,
        },
    );
    mark_intentional_quit(&app, "app update");
    // The gate stays closed until the process terminates (exit is asynchronous).
    std::mem::forget(gate);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::capture_update_blocker;
    use wakenote::{commands::AppBackend, settings::SettingsPatch};

    #[test]
    fn stopping_capture_allows_update_without_changing_recording_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let mut backend = AppBackend::default();
        backend.update_settings(SettingsPatch {
            save_root: Some(directory.path().to_string_lossy().into_owned()),
            recording_enabled: Some(true),
            pause_all: Some(false),
            ..Default::default()
        });
        backend.start_capture_session_for_test(16_000).unwrap();
        assert!(capture_update_blocker(&backend).is_some());

        backend.stop_capture_session().unwrap();
        assert!(backend.live_capture_should_run());
        assert!(capture_update_blocker(&backend).is_none());
    }
}
