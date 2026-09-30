//! Native updater IPC and restart protection for active WakeNote work.
use crate::{
    AutoTranscriptionState, BackendState, DictationStage, DictationState, LiveCaptureState,
    LlmRunState, ManualMeetingRecordingState, MeetingState, SystemCaptureState,
    kick_transcription_worker_if_needed, mark_intentional_quit,
    note_source_meeting_microphone_stopped, start_live_capture_runtime, stop_live_capture_runtime,
    update_tray_presentation,
};
use serde::Serialize;
use std::sync::{Arc, Mutex, atomic::Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use wakenote::commands::AppStatus;
use wakenote::live_webhook::WebhookService;
use wakenote::settings::live_capture_should_start_on_launch;
use wakenote::updater::{
    self, CaptureSuspension, RESTART_GATE, RestartDrain, UpdateInfo, UpdateProgress,
};

/// Upper bound for finishing in-flight work after Capture input stops. Each
/// microphone's flushed realtime chunk may wait up to 75 s for its result.
const UPDATE_DRAIN_TIMEOUT: Duration = Duration::from_secs(180);
const UPDATE_DRAIN_POLL: Duration = Duration::from_millis(250);
/// How long an otherwise idle update waits for live-transcription webhooks
/// still being sent. An unreachable endpoint must not hold the update.
const UPDATE_WEBHOOK_GRACE: Duration = Duration::from_secs(20);
pub(crate) const LIVE_INPUT_CHANGED_EVENT: &str = "live-input-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateReadiness {
    /// Work the user must finish first; the install button stays disabled.
    blocker: Option<String>,
    /// Installing pauses Capture input, which is running now.
    stops_capture: bool,
    /// Launch auto-start will reopen Capture input after the relaunch.
    resumes_capture: bool,
}

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
        .then(|| "Capture input is still stopping.".into())
}

fn idle_check_unavailable<T>(_: T) -> String {
    "Could not verify whether WakeNote is idle.".into()
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

/// Sessions the user owns and long-running work. WakeNote never stops these on
/// the user's behalf, so the update waits until they end.
fn update_hard_blocker(app: &AppHandle) -> Result<Option<String>, String> {
    if RESTART_GATE.has_long_activity()? {
        return Ok(Some(
            "Wait for the current download, export or recording save to finish before installing."
                .into(),
        ));
    }
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
        DictationStage::Recording
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
    if app
        .state::<BackendState>()
        .lock()
        .map_err(idle_check_unavailable)?
        .is_transcribing_imported_audio()
    {
        return Ok(Some(
            "Wait for the imported audio to finish transcribing before installing.".into(),
        ));
    }
    Ok(None)
}

/// Everything that must be idle before the process exits. Checked while the
/// restart gate is closed, so nothing new can start once it passes.
fn update_runtime_blocker(app: &AppHandle) -> Result<Option<String>, String> {
    if let Some(reason) = update_hard_blocker(app)? {
        return Ok(Some(reason));
    }
    // A poisoned/missing runtime is an error, never evidence that work is idle.
    let backend_state = app.state::<BackendState>();
    let backend = backend_state.lock().map_err(idle_check_unavailable)?;
    if let Some(reason) = capture_update_blocker(&backend) {
        return Ok(Some(reason));
    }
    if backend.app_status().queue.running_count > 0
        || backend.has_pending_unresumable_transcription_jobs()
        || app
            .state::<AutoTranscriptionState>()
            .load(Ordering::Acquire)
    {
        return Ok(Some(
            "Wait for transcription to finish before installing.".into(),
        ));
    }
    drop(backend);
    if matches!(
        app.state::<DictationState>()
            .lock()
            .map_err(|_| "Could not check dictation.")?
            .stage(),
        DictationStage::Transcribing
    ) {
        return Ok(Some(
            "Wait for dictation to finish before installing.".into(),
        ));
    }
    Ok(None)
}

#[tauri::command]
pub(crate) fn update_install_readiness(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<UpdateReadiness, String> {
    require_update_window(&window)?;
    let blocker = update_hard_blocker(&app)?;
    let backend_state = app.state::<BackendState>();
    let backend = backend_state.lock().map_err(idle_check_unavailable)?;
    let stops_capture = backend.app_status().live_input_active;
    Ok(UpdateReadiness {
        blocker,
        stops_capture,
        resumes_capture: stops_capture && live_capture_should_start_on_launch(&backend.settings()),
    })
}

#[tauri::command]
pub(crate) fn open_update_release(
    window: tauri::WebviewWindow,
    version: Option<String>,
) -> Result<(), String> {
    require_update_window(&window)?;
    updater::open_release(version.as_deref())
}

fn emit_progress(window: &tauri::WebviewWindow, phase: &'static str) {
    let _ = window.emit(
        "app-update-progress",
        UpdateProgress {
            phase,
            downloaded: 0,
            total: 0,
        },
    );
}

fn live_input_active(app: &AppHandle) -> Result<bool, String> {
    app.state::<BackendState>()
        .lock()
        .map(|backend| backend.app_status().live_input_active)
        .map_err(idle_check_unavailable)
}

fn refresh_capture_presentation(app: &AppHandle, status: &AppStatus) {
    if let Ok(settings) = app
        .state::<BackendState>()
        .lock()
        .map(|backend| backend.settings())
    {
        update_tray_presentation(app, &settings, status);
    }
    let _ = app.emit(LIVE_INPUT_CHANGED_EVENT, ());
}

/// Same sequence as the Capture Stop button: the chunk being recorded is
/// flushed to disk and queued. Must not run while holding BackendState.
fn stop_live_capture_for_update(app: &AppHandle) -> Result<(), String> {
    let backend_state = app.state::<BackendState>();
    let live_state = app.state::<LiveCaptureState>();
    let status = stop_live_capture_runtime(app, backend_state.inner(), live_state.inner())?;
    note_source_meeting_microphone_stopped(app, backend_state.inner());
    // Transcribe the flushed chunk before exit: realtime-model jobs cannot
    // resume after a relaunch.
    kick_transcription_worker_if_needed(
        app.clone(),
        backend_state.inner().clone(),
        app.state::<AutoTranscriptionState>().inner().clone(),
    );
    refresh_capture_presentation(app, &status);
    Ok(())
}

/// Undo an abandoned update's pause: reopen Capture input if the update
/// stopped it (or refused a start meanwhile) and pick up pending jobs the
/// drain left queued. Call only after the restart gate is released.
fn abandon_update(app: &AppHandle, suspension: CaptureSuspension<'static>, capture_stopped: bool) {
    let restore = suspension.release(capture_stopped);
    let app = app.clone();
    std::thread::spawn(move || {
        let backend_state = app.state::<BackendState>();
        let transcription_state = app.state::<AutoTranscriptionState>();
        if let Some(restore) = restore {
            // No-op when recording was paused or disabled in the meantime.
            match start_live_capture_runtime(
                &app,
                backend_state.inner(),
                app.state::<LiveCaptureState>().inner(),
                transcription_state.inner().clone(),
            ) {
                Ok(status) => refresh_capture_presentation(&app, &status),
                Err(error) => {
                    eprintln!(
                        "[updater] could not restart Capture input after a cancelled update: {error}"
                    );
                    let _ = app.emit(LIVE_INPUT_CHANGED_EVENT, ());
                }
            }
            drop(restore);
        }
        kick_transcription_worker_if_needed(
            app.clone(),
            backend_state.inner().clone(),
            transcription_state.inner().clone(),
        );
    });
}

struct AppDrain<'a> {
    app: &'a AppHandle,
    window: &'a tauri::WebviewWindow,
    /// When the drain gives up; the webhook wait must end before it.
    deadline: Instant,
    webhook_wait_started: Mutex<Option<Instant>>,
}

impl AppDrain<'_> {
    /// Finished chunks hand their webhook to an async sender that exit would
    /// cancel. Give it a bounded chance to deliver.
    fn webhook_blocker(&self) -> Option<String> {
        let service = self.app.try_state::<Arc<WebhookService>>()?;
        let stats = service.stats();
        let mut started = self.webhook_wait_started.lock().ok()?;
        if stats.pending + stats.in_flight == 0 {
            *started = None;
            return None;
        }
        let since = *started.get_or_insert_with(Instant::now);
        (since.elapsed() < UPDATE_WEBHOOK_GRACE
            && Instant::now() + 2 * UPDATE_DRAIN_POLL < self.deadline)
            .then(|| "Wait for live transcription webhooks to finish sending.".into())
    }
}

impl RestartDrain for AppDrain<'_> {
    fn hard_blocker(&self) -> Result<Option<String>, String> {
        update_hard_blocker(self.app)
    }
    fn runtime_blocker(&self) -> Result<Option<String>, String> {
        let reason = update_runtime_blocker(self.app)?.or_else(|| self.webhook_blocker());
        if reason.is_some() {
            // A flushed chunk can land just after the worker decided to exit,
            // or come from a slot stop that never kicked it. While capture is
            // suspended this only starts jobs that cannot resume after relaunch.
            let backend_state = self.app.state::<BackendState>();
            kick_transcription_worker_if_needed(
                self.app.clone(),
                backend_state.inner().clone(),
                self.app.state::<AutoTranscriptionState>().inner().clone(),
            );
        }
        Ok(reason)
    }
    fn capture_active(&self) -> Result<bool, String> {
        live_input_active(self.app)
    }
    fn stop_capture(&self) -> impl Future<Output = Result<(), String>> + Send {
        let app = self.app.clone();
        async move {
            tauri::async_runtime::spawn_blocking(move || stop_live_capture_for_update(&app))
                .await
                .map_err(|_| "Could not pause Capture input for the update.".to_string())?
        }
    }
    fn progress(&self, phase: &'static str) {
        emit_progress(self.window, phase);
    }
}

#[tauri::command]
pub(crate) async fn download_and_install_update(
    app: AppHandle,
    window: tauri::WebviewWindow,
    expected_version: String,
) -> Result<(), String> {
    require_update_window(&window)?;
    let _download = updater::DownloadGuard::acquire()?;
    if let Some(reason) = update_hard_blocker(&app)? {
        return Err(reason);
    }
    let current = app.package_info().version.to_string();
    let progress_window = window.clone();
    // Capture keeps recording while the update downloads and verifies.
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        updater::prepare(&current, &expected_version, |progress| {
            let _ = progress_window.emit("app-update-progress", progress);
        })
    })
    .await
    .map_err(|_| "Could not prepare the update.".to_string())??;
    // From here nothing may reopen Capture input, so its final chunk can be
    // flushed and transcribed before exit. Pending jobs that can resume stay
    // queued on disk for the relaunch.
    let suspension = RESTART_GATE.suspend_capture()?;
    let mut capture_stopped = false;
    let drained = updater::drain_for_restart(
        &RESTART_GATE,
        &AppDrain {
            app: &app,
            window: &window,
            deadline: Instant::now() + UPDATE_DRAIN_TIMEOUT,
            webhook_wait_started: Mutex::new(None),
        },
        UPDATE_DRAIN_TIMEOUT,
        UPDATE_DRAIN_POLL,
        &mut capture_stopped,
    )
    .await;
    let gate = match drained {
        Ok(gate) => gate,
        Err(error) => {
            abandon_update(&app, suspension, capture_stopped);
            return Err(error);
        }
    };
    if let Err(error) = prepared.launch() {
        drop(gate);
        abandon_update(&app, suspension, capture_stopped);
        return Err(error);
    }
    emit_progress(&window, "restarting");
    mark_intentional_quit(&app, "app update");
    // The gates stay closed until the process terminates (exit is asynchronous).
    std::mem::forget(gate);
    std::mem::forget(suspension);
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::capture_update_blocker;
    use wakenote::{commands::AppBackend, settings::SettingsPatch};

    #[test]
    fn stopping_capture_clears_the_final_update_check_without_changing_recording_preferences() {
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
