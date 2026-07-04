use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{
    AboutMetadata, CheckMenuItem, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem, Submenu,
    WINDOW_SUBMENU_ID,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State, Wry};
use wakenote::audio::{MicHealthAction, list_input_devices};
use wakenote::audio_analysis::AudioWaveform;
use wakenote::commands::{
    AppBackend, AppStatus, LiveEventHandler, LiveTranscriptEvent, MainWindowCloseAction,
    MicrophoneDevice, RecentTranscript, StartedTranscriptionJob, TrayMenuPresentation,
    TrayRuntimePresentation, TrayState, UploadedAudio, main_window_close_action,
    microphone_devices_from_input_devices, open_containing_folder_request, pinned_device_mismatch,
    recorded_at_for_audio_path, reveal_save_folder_request, tray_icon_image_for_presentation,
    tray_menu_presentation, tray_presentation_for_state, tray_runtime_presentation,
    validate_audio_playback_file, with_live_runtime_warning,
};
use wakenote::debug_log::append_debug_log;
use wakenote::live_capture::{
    AudioFrame, AudioInputBackend, AudioInputConfig, AudioStreamHandle, CpalAudioInput,
    LiveCaptureError, LiveCaptureRuntime, ResolvedCpalInputDevice,
};
use wakenote::live_transcription::{
    LivePartialEvent, LivePartialRequest, LiveTranscriptionService,
};
use wakenote::meeting::{MeetingDetail, MeetingEvent, MeetingSummary};
use wakenote::models::{ModelDescriptor, ModelStore};
use wakenote::overlay::{self, OverlayState};
use wakenote::permissions::{self, AppPermissions};
use wakenote::queue::QueueSnapshot;
use wakenote::recorder::ChunkMetadata;
use wakenote::settings::{
    AppSettings, FloatingOverlayPosition, LaunchAtLoginAction, LiveCaptureRuntimeAction,
    SettingsPatch, TrayClickAction, expand_user_path, launch_at_login_action_for_patch,
    live_capture_runtime_action_for_patch, live_capture_should_start_on_launch,
    resolve_auto_prompt,
};
use wakenote::source_watcher::{
    DetectedSource, SOURCE_MISSING_GRACE_POLLS, SourceTransition,
    compute_source_transition_with_missing_grace, should_auto_capture_source,
    should_defer_source_end_for_recent_audio,
};
use wakenote::sources::source_definitions;
use wakenote::system_audio::{PIPELINE_SAMPLE_RATE, SystemAudioInput, enumerate_windows};
use wakenote::transcription::{
    RuntimeTranscriber, TranscriptionJobOutcome, TranscriptionJobStatus, TranscriptionWorker,
    TranscriptionWorkerOptions, model_supports_live_partials,
};

type BackendState = Arc<Mutex<AppBackend>>;
type LiveCaptureState = Mutex<LiveCaptureRuntime<CpalAudioInput>>;
type AutoTranscriptionState = Arc<AtomicBool>;
type LiveTranscriberState = Arc<Mutex<Option<Arc<LiveTranscriptionService>>>>;
/// Open system-audio stream handle (dropping it stops capture). Held alongside
/// the mic runtime so the two capture paths are independent. Managed as an
/// `Arc<Mutex<…>>` so the watcher thread and IPC commands share one handle.
type SystemCaptureState = Arc<Mutex<Option<Box<dyn AudioStreamHandle>>>>;
type SourceCaptureLifecycleState = Arc<Mutex<SourceCaptureLifecycle>>;
/// The recognized source currently detected on screen, threaded across polls by
/// the watcher and read by the capture commands.
type DetectedSourceState = Arc<Mutex<Option<DetectedSource>>>;
/// Source ids the user paused manually while the source remains detected.
type SourceCapturePauseState = Arc<Mutex<HashSet<String>>>;
/// Tracks the single in-flight long-form meeting job and its cancel flag.
/// Only one meeting transcribes at a time (one shared GPU context).
type MeetingState = Arc<Mutex<MeetingRuntime>>;
type TrayPresentationCache = Mutex<Option<TrayPresentationSnapshot>>;

#[derive(Default)]
struct MeetingRuntime {
    /// Id of the meeting currently being processed, if any.
    current: Option<String>,
    /// Cancel flag for the current job, checked between segments.
    cancel: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone)]
struct TrayPresentationUpdate {
    settings: AppSettings,
    status: AppStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrayPresentationSnapshot {
    runtime: TrayRuntimePresentation,
    menu: TrayMenuPresentation,
    show_menu_on_left_click: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceCaptureLifecycle {
    Idle,
    Starting {
        source_id: String,
        attempt: u8,
    },
    Running {
        source_id: String,
    },
    Stopping {
        source_id: String,
    },
    Failed {
        source_id: String,
        failed_at: Instant,
        attempts: u8,
    },
}

impl Default for SourceCaptureLifecycle {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceCaptureStartDecision {
    Start { attempt: u8 },
    AlreadyStarting,
    AlreadyRunning,
    Stopping,
    BackingOff { retry_after: Duration },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceCaptureAttemptOutcome {
    Started,
    AlreadyCapturing,
    Deferred,
}

#[derive(Debug, Clone, PartialEq)]
struct SourceCaptureAttemptResult {
    status: AppStatus,
    outcome: SourceCaptureAttemptOutcome,
}

trait SourceCaptureStreamStarter {
    fn start_source_capture_stream(
        &mut self,
        source: &DetectedSource,
        sample_rate: u32,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError>;
}

struct SystemAudioStreamStarter;

impl SourceCaptureStreamStarter for SystemAudioStreamStarter {
    fn start_source_capture_stream(
        &mut self,
        source: &DetectedSource,
        sample_rate: u32,
        on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
        let mut input = SystemAudioInput::new();
        input.set_target_app(source.pid, source.app_name.clone());
        input.start(
            AudioInputConfig {
                device_id: source.source_id.clone(),
                sample_rate: Some(sample_rate),
                label_hint: None,
            },
            on_frame,
        )
    }
}

const EVENT_LIVE_STARTED: &str = "live-transcript-started";
const EVENT_LIVE_PARTIAL: &str = "live-transcript-partial";
const EVENT_LIVE_COMMITTED: &str = "live-transcript-committed";
const EVENT_LIVE_FINAL: &str = "live-transcript-final";
const EVENT_LIVE_FAILED: &str = "live-transcript-failed";
const EVENT_SOURCE_DETECTED: &str = "source-detected";
const EVENT_SOURCE_ENDED: &str = "source-ended";
const EVENT_SOURCE_CAPTURE_STARTED: &str = "source-capture-started";
const EVENT_SOURCE_CAPTURE_STOPPED: &str = "source-capture-stopped";
const EVENT_SOURCE_CAPTURE_ERROR: &str = "source-capture-error";
const EVENT_MEETING_PROGRESS: &str = "meeting-progress";
const EVENT_MEETING_SEGMENT: &str = "meeting-segment-committed";
const EVENT_MEETING_FINISHED: &str = "meeting-finished";
/// How often the watcher re-enumerates windows while the feature is enabled.
const SOURCE_WATCH_INTERVAL: Duration = Duration::from_secs(5);
const MAIN_WINDOW_LABEL: &str = "main";
const MAIN_WINDOW_TITLE: &str = "WakeNote";
const CLOSE_SETTINGS_WINDOW_MENU_ID: &str = "close-settings-window";
const MAIN_WINDOW_WIDTH: f64 = 1040.0;
const MAIN_WINDOW_HEIGHT: f64 = 922.0;
const MAIN_WINDOW_MIN_WIDTH: f64 = 980.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 640.0;
/// How long we wait for cpal's `host.input_devices()` enumeration to
/// finish before giving up with "audio device lookup did not finish
/// within N seconds". cpal blocks on CoreAudio on macOS, which can
/// take several seconds the first time after a sleep/wake cycle or
/// when many Bluetooth devices are advertising. 2s was too aggressive
/// — bumped to 5s.
const AUDIO_DEVICE_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCH_AUTO_START_RETRY_DELAY_SECS: [u64; 6] = [2, 5, 10, 20, 30, 60];
const OVERLAY_LEVEL_EMIT_INTERVAL: Duration = Duration::from_millis(50);
const MAX_PARALLEL_TRANSCRIPTIONS: usize = 1;
const MIC_RECOVERY_TICK_INTERVAL: Duration = Duration::from_millis(500);
const SOURCE_CAPTURE_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
];
const SOURCE_CAPTURE_AUDIO_LIVENESS_HOLD: Duration = Duration::from_secs(20);
/// Sleep inserted between a watchdog-driven `stop` and the immediately
/// following `start`. macOS CoreAudio occasionally retains wedged state
/// when a device is reopened the instant after it's released; a brief
/// pause lets the driver drop its handles and avoids re-opening into the
/// same wedged stream.
const MIC_RECOVERY_SETTLING_DELAY: Duration = Duration::from_millis(200);
/// After this many consecutive watchdog escalations without a single
/// non-silent frame, surface a UI warning so the user knows the watchdog
/// is fighting (and that pressing Refresh or changing priority may help).
const MIC_WEDGE_WARNING_THRESHOLD: u32 = 4;
const DEFAULT_RECENT_TRANSCRIPT_LIMIT: usize = 50;
const MAX_RECENT_TRANSCRIPT_LIMIT: usize = 5_000;
#[cfg(test)]
const TRAY_MENU_ORDER: &[&str] = &[
    "open-settings",
    "separator",
    "toggle-recording",
    "toggle-transcription",
    "active-model",
    "active-microphone",
    "threshold",
    "separator",
    "reveal-save-folder",
    "pause-all",
    "separator",
    "cancel-current-operation",
    "separator",
    "quit",
];

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
    recorded_at: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct LiveFailedPayload {
    chunk_id: Option<u64>,
    audio_path: String,
    recorded_at: String,
    error: String,
}

/// Emitted on `source-detected` / `source-ended`, and reused inside the capture
/// status snapshot to describe the recognized source on screen.
#[derive(Debug, Clone, Serialize)]
struct SourcePayload {
    source_id: String,
    label: String,
    app_name: String,
}

impl From<&DetectedSource> for SourcePayload {
    fn from(source: &DetectedSource) -> Self {
        Self {
            source_id: source.source_id.clone(),
            label: source.label.clone(),
            app_name: source.app_name.clone(),
        }
    }
}

fn recording_source_label(source: &DetectedSource) -> String {
    if source.source_id.starts_with("custom-source-") {
        source.label.clone()
    } else {
        source.source_id.clone()
    }
}

fn source_capture_scope(_source: &DetectedSource) -> &'static str {
    "target-app"
}

/// Emitted on `source-capture-started` / `source-capture-stopped`.
#[derive(Debug, Clone, Serialize)]
struct SourceCapturePayload {
    source_id: String,
    label: String,
}

/// Emitted on `source-capture-error`.
#[derive(Debug, Clone, Serialize)]
struct SourceCaptureErrorPayload {
    source_id: String,
    error: String,
}

/// One recognized source returned by `list_recognized_sources`, with its
/// effective auto-prompt preference resolved against current settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RecognizedSourceInfo {
    id: String,
    label: String,
    description: String,
    auto_prompt: bool,
    title_patterns: Vec<String>,
    custom: bool,
}

/// Snapshot returned by `source_capture_status`.
#[derive(Debug, Clone, Serialize)]
struct SourceCaptureStatus {
    detected: Option<SourcePayload>,
    capturing: bool,
}

/// Map the built-in recognized sources to `RecognizedSourceInfo`, resolving each
/// source's effective auto-prompt against the given settings. Pure so it can be
/// unit-tested without a backend.
fn recognized_source_infos(settings: &AppSettings) -> Vec<RecognizedSourceInfo> {
    source_definitions(settings)
        .into_iter()
        .map(|source| RecognizedSourceInfo {
            auto_prompt: resolve_auto_prompt(settings, &source.id),
            id: source.id,
            label: source.label,
            description: source.description,
            title_patterns: source.title_patterns,
            custom: source.custom,
        })
        .collect()
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
    system_capture_state: State<'_, SystemCaptureState>,
    source_capture_lifecycle_state: State<'_, SourceCaptureLifecycleState>,
    detected_source_state: State<'_, DetectedSourceState>,
    meeting_state: State<'_, MeetingState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let (
        launch_at_login_action,
        live_capture_action,
        prior_position,
        previous_model_directory,
        previous_show_dock_icon,
    ) = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        (
            launch_at_login_action_for_patch(&settings, &patch),
            live_capture_runtime_action_for_patch(&settings, &patch),
            settings.floating_overlay_position,
            settings.model_directory.clone(),
            settings.show_dock_icon,
        )
    };
    apply_launch_at_login_action(&app, launch_at_login_action)?;

    let (settings, live_event_handler, live_events) = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        let settings = backend.update_settings(patch);
        let (handler, events) = live_events_for_dispatch(&mut backend);
        (settings, handler, events)
    };
    if settings.model_directory != previous_model_directory {
        if let Ok(slot) = live_transcriber_state.lock() {
            if let Some(service) = slot.as_ref() {
                service.update_model_directory(&settings.model_directory);
            }
        }
    }
    if settings.show_dock_icon != previous_show_dock_icon {
        apply_dock_icon_visibility(&app, settings.show_dock_icon)
            .map_err(|error| error.to_string())?;
    }
    if settings_window_should_open_on_launch(&settings) {
        ensure_main_window_visible(&app, settings.show_dock_icon)
            .map_err(|error| error.to_string())?;
    }
    dispatch_live_events(live_event_handler, live_events);
    apply_live_capture_runtime_action(
        &app,
        state.inner(),
        live_state.inner(),
        transcription_state.inner().clone(),
        live_capture_action,
    )?;
    apply_system_capture_settings_action(
        &app,
        &settings,
        state.inner(),
        system_capture_state.inner(),
        source_capture_lifecycle_state.inner(),
        detected_source_state.inner(),
        meeting_state.inner(),
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
    let overlay_state = app
        .try_state::<BackendState>()
        .and_then(|state| {
            state.lock().ok().map(|backend| {
                overlay::overlay_state_for_tray_state(backend.app_status().tray_state)
            })
        })
        .unwrap_or(OverlayState::Hidden);
    let result = if matches!(
        settings.floating_overlay_position,
        FloatingOverlayPosition::Off
    ) {
        overlay::hide_overlay_on_main_thread(app, "position change hide")
    } else if !matches!(overlay_state, OverlayState::Hidden) {
        overlay::show_overlay_on_main_thread(
            app,
            overlay_state,
            settings.floating_overlay_position,
            "position change show overlay",
        )
    } else {
        overlay::hide_overlay_on_main_thread(app, "position change hide inactive")
    };
    if let Err(error) = result {
        eprintln!("[overlay] position change failed: {error}");
    }
}

/// What to do with an active system-audio capture session after a settings
/// change. `None` means no session is open (nothing to do).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemCaptureSettingsAction {
    /// Feature/recording disabled or paused: tear the session down.
    Stop,
    /// Keep capturing; propagate the new settings to the live controller.
    Sync,
    None,
}

/// Pure decision for `apply_system_capture_settings_action`. The session must be
/// stopped when the feature is disabled, recording is off, or pause-all is on
/// (§7/§2 pause semantics); otherwise the live settings are synced so the
/// running capture never operates on a frozen snapshot.
fn system_capture_settings_action(
    settings: &AppSettings,
    capturing: bool,
) -> SystemCaptureSettingsAction {
    if !capturing {
        return SystemCaptureSettingsAction::None;
    }
    if !settings.system_audio_enabled || !settings.recording_enabled || settings.pause_all {
        SystemCaptureSettingsAction::Stop
    } else {
        SystemCaptureSettingsAction::Sync
    }
}

fn source_capture_retry_delay(attempts: u8) -> Duration {
    let index = attempts.saturating_sub(1) as usize;
    SOURCE_CAPTURE_RETRY_DELAYS[index.min(SOURCE_CAPTURE_RETRY_DELAYS.len() - 1)]
}

fn source_capture_start_decision(
    lifecycle: &SourceCaptureLifecycle,
    source_id: &str,
    now: Instant,
) -> SourceCaptureStartDecision {
    match lifecycle {
        SourceCaptureLifecycle::Idle => SourceCaptureStartDecision::Start { attempt: 1 },
        SourceCaptureLifecycle::Starting { .. } => SourceCaptureStartDecision::AlreadyStarting,
        SourceCaptureLifecycle::Running { .. } => SourceCaptureStartDecision::AlreadyRunning,
        SourceCaptureLifecycle::Stopping { .. } => SourceCaptureStartDecision::Stopping,
        SourceCaptureLifecycle::Failed {
            source_id: failed_source_id,
            failed_at,
            attempts,
        } if failed_source_id == source_id => {
            let delay = source_capture_retry_delay(*attempts);
            let retry_at = *failed_at + delay;
            match retry_at.checked_duration_since(now) {
                Some(retry_after) if !retry_after.is_zero() => {
                    SourceCaptureStartDecision::BackingOff { retry_after }
                }
                _ => SourceCaptureStartDecision::Start {
                    attempt: attempts.saturating_add(1).max(1),
                },
            }
        }
        SourceCaptureLifecycle::Failed { .. } => SourceCaptureStartDecision::Start { attempt: 1 },
    }
}

fn source_capture_mark_starting(
    lifecycle: &mut SourceCaptureLifecycle,
    source_id: &str,
    attempt: u8,
) {
    *lifecycle = SourceCaptureLifecycle::Starting {
        source_id: source_id.to_string(),
        attempt: attempt.max(1),
    };
}

fn source_capture_mark_running(lifecycle: &mut SourceCaptureLifecycle, source_id: &str) {
    *lifecycle = SourceCaptureLifecycle::Running {
        source_id: source_id.to_string(),
    };
}

fn source_capture_mark_stopping(lifecycle: &mut SourceCaptureLifecycle, source_id: &str) {
    *lifecycle = SourceCaptureLifecycle::Stopping {
        source_id: source_id.to_string(),
    };
}

fn source_capture_mark_failed(
    lifecycle: &mut SourceCaptureLifecycle,
    source_id: &str,
    failed_at: Instant,
) {
    let attempts = match lifecycle {
        SourceCaptureLifecycle::Starting {
            source_id: active_source,
            attempt,
        } if active_source == source_id => *attempt,
        SourceCaptureLifecycle::Failed {
            source_id: failed_source,
            attempts,
            ..
        } if failed_source == source_id => *attempts,
        _ => 1,
    };
    *lifecycle = SourceCaptureLifecycle::Failed {
        source_id: source_id.to_string(),
        failed_at,
        attempts: attempts.max(1),
    };
}

fn source_capture_mark_idle(lifecycle: &mut SourceCaptureLifecycle) {
    *lifecycle = SourceCaptureLifecycle::Idle;
}

fn source_capture_failure_diagnostic(
    source_id: &str,
    attempt: u8,
    screen_recording_status: permissions::PermissionGrantStatus,
    error: &str,
) -> String {
    format!(
        "[source-capture] error source_id={} attempt={} screen_recording_status={:?} next_retry_ms={} error={}",
        source_id,
        attempt.max(1),
        screen_recording_status,
        source_capture_retry_delay(attempt.max(1)).as_millis(),
        error
    )
}

/// Reconcile an active system-audio capture session with newly applied settings.
/// If the feature was disabled, recording turned off, or pause-all turned on,
/// tear the session down the same way `stop_source_capture` does (drop the
/// stream handle, flush/stop the backend session, clear detection, emit
/// `source-capture-stopped`). Otherwise propagate the new settings to the live
/// system `CaptureController` so threshold/chunk-timing changes take effect
/// mid-capture without restarting the stream.
fn apply_system_capture_settings_action(
    app: &AppHandle,
    settings: &AppSettings,
    backend_state: &BackendState,
    system_capture_state: &SystemCaptureState,
    source_capture_lifecycle_state: &SourceCaptureLifecycleState,
    detected_source_state: &DetectedSourceState,
    meeting_state: &MeetingState,
) -> Result<(), String> {
    let capturing = system_capture_state
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false);
    match system_capture_settings_action(settings, capturing) {
        SystemCaptureSettingsAction::Stop => {
            stop_source_capture_runtime(
                app,
                backend_state,
                system_capture_state,
                source_capture_lifecycle_state,
                detected_source_state,
                meeting_state,
            )?;
        }
        SystemCaptureSettingsAction::Sync => {
            let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
            backend.sync_system_capture_settings();
        }
        SystemCaptureSettingsAction::None => {}
    }
    Ok(())
}

fn live_events_for_dispatch(
    backend: &mut AppBackend,
) -> (Option<LiveEventHandler>, Vec<LiveTranscriptEvent>) {
    let handler = backend.live_event_handler();
    let events = backend.drain_live_events();
    (handler, events)
}

fn dispatch_live_events(handler: Option<LiveEventHandler>, events: Vec<LiveTranscriptEvent>) {
    if events.is_empty() {
        return;
    }

    let Some(handler) = handler else {
        eprintln!("[wakenote] WARN: live events generated before handler was set");
        return;
    };

    for event in events {
        handler(event);
    }
}

fn overlay_level_emit_due(last_emit: &Mutex<Instant>) -> bool {
    let now = Instant::now();
    let Ok(mut last_emit_at) = last_emit.lock() else {
        return false;
    };
    if now.duration_since(*last_emit_at) < OVERLAY_LEVEL_EMIT_INTERVAL {
        return false;
    }
    *last_emit_at = now;
    true
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
    label_hint: Option<String>,
) -> Result<ResolvedCpalInputDevice, LiveCaptureError> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(CpalAudioInput::resolve_device(
            &device_id,
            label_hint.as_deref(),
        ));
    });

    match receiver.recv_timeout(AUDIO_DEVICE_RESOLVE_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(LiveCaptureError::Cpal(format!(
            "audio device lookup did not finish within {} seconds",
            AUDIO_DEVICE_RESOLVE_TIMEOUT.as_secs()
        ))),
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
    let (settings, status) = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        (
            backend.settings(),
            with_live_runtime_warning(backend.app_status(), dropped_frames, stream_error),
        )
    };
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
        // sherpa-onnx models download a .tar.bz2 and extract in place; whisper
        // models download a single .bin. Both paths record an Error status into
        // the store on failure (surfaced via the registry snapshot); log here so
        // a failed background download is observable in the app log too.
        let result = if model.provider_runtime == "sherpa-onnx" {
            store.download_and_extract_sherpa_model(&model)
        } else {
            store.download_model(&model)
        };
        if let Err(error) = result {
            eprintln!("[models] download {} failed: {error}", model.id);
        }
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
fn permission_snapshot() -> AppPermissions {
    permissions::permission_snapshot()
}

#[tauri::command]
async fn request_microphone_permission() -> Result<AppPermissions, String> {
    tauri::async_runtime::spawn_blocking(permissions::request_microphone_permission)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_microphone_permission_settings() -> Result<(), String> {
    permissions::open_microphone_permission_settings()
}

#[tauri::command]
async fn request_screen_recording_permission() -> Result<AppPermissions, String> {
    tauri::async_runtime::spawn_blocking(permissions::request_screen_recording_permission)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_screen_recording_settings() -> Result<(), String> {
    permissions::open_screen_recording_permission_settings()
}

#[tauri::command]
async fn recent_transcripts(
    state: State<'_, BackendState>,
    limit: Option<usize>,
) -> Result<Vec<RecentTranscript>, String> {
    let save_root = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        expand_user_path(backend.settings().save_root)
    };
    let limit = normalize_recent_transcript_limit(limit);

    tauri::async_runtime::spawn_blocking(move || {
        wakenote::commands::recent_transcripts_from_save_root(&save_root, limit)
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn transcript_days(
    state: State<'_, BackendState>,
) -> Result<Vec<wakenote::commands::TranscriptDay>, String> {
    let save_root = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        expand_user_path(backend.settings().save_root)
    };

    tauri::async_runtime::spawn_blocking(move || {
        wakenote::commands::transcript_days_from_save_root(&save_root)
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn transcripts_for_day(
    state: State<'_, BackendState>,
    day: String,
    download: bool,
) -> Result<Vec<RecentTranscript>, String> {
    let save_root = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        expand_user_path(backend.settings().save_root)
    };

    tauri::async_runtime::spawn_blocking(move || {
        wakenote::commands::transcripts_for_day_from_save_root(&save_root, &day, download)
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn rebuild_transcript_day_index(
    state: State<'_, BackendState>,
    day: String,
    download: bool,
) -> Result<Vec<RecentTranscript>, String> {
    let save_root = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        expand_user_path(backend.settings().save_root)
    };

    tauri::async_runtime::spawn_blocking(move || {
        wakenote::commands::rebuild_transcript_day_index_from_save_root(&save_root, &day, download)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn allow_audio_playback(app: AppHandle, audio_path: String) -> Result<(), String> {
    let path = PathBuf::from(audio_path);
    let validated_path = path.clone();
    tauri::async_runtime::spawn_blocking(move || validate_audio_playback_file(&validated_path))
        .await
        .map_err(|error| error.to_string())??;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn upload_audio_file(
    state: State<'_, BackendState>,
    source_path: String,
) -> Result<UploadedAudio, String> {
    let mut backend = state.lock().map_err(|error| error.to_string())?;
    backend.upload_audio_file(source_path, chrono::Local::now())
}

#[tauri::command]
async fn analyze_audio_waveform(
    audio_path: String,
    bucket_count: Option<usize>,
) -> Result<AudioWaveform, String> {
    let path = PathBuf::from(audio_path);
    validate_audio_playback_file(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        wakenote::audio_analysis::analyze_audio_waveform(&path, bucket_count)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
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
fn regenerate_transcript(
    app: AppHandle,
    state: State<'_, BackendState>,
    transcription_state: State<'_, AutoTranscriptionState>,
    audio_path: String,
    model_id: Option<String>,
) -> Result<QueueSnapshot, String> {
    let snapshot = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.regenerate_transcript(audio_path, model_id)?
    };
    kick_transcription_worker_if_needed(
        app,
        state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(snapshot)
}

#[tauri::command]
fn open_transcript_folder(path: String) -> Result<(), String> {
    let request = open_containing_folder_request(PathBuf::from(path))?;
    Command::new(request.program)
        .arg(request.path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
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
    let (settings, status) = {
        let mut backend = state.lock().map_err(|error| error.to_string())?;
        backend.cancel_current_operation()?;
        (backend.settings(), backend.app_status())
    };
    update_tray_presentation(&app, &settings, &status);
    Ok(())
}

#[tauri::command]
fn reveal_save_folder(state: State<'_, BackendState>) -> Result<(), String> {
    let request = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        reveal_save_folder_request(&backend.settings())
    };
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

// --- Long-form meeting transcription -------------------------------------

/// Build the sink that forwards meeting-pipeline events to the frontend.
fn meeting_event_emitter(app: AppHandle) -> wakenote::meeting::MeetingEventCallback {
    Arc::new(move |event: MeetingEvent| match event {
        MeetingEvent::Progress(payload) => {
            let _ = app.emit(EVENT_MEETING_PROGRESS, payload);
        }
        MeetingEvent::SegmentCommitted(payload) => {
            let _ = app.emit(EVENT_MEETING_SEGMENT, payload);
        }
        MeetingEvent::Finished(payload) => {
            let _ = app.emit(EVENT_MEETING_FINISHED, payload);
        }
    })
}

fn meeting_save_root(state: &State<'_, BackendState>) -> Result<PathBuf, String> {
    let backend = state.lock().map_err(|error| error.to_string())?;
    Ok(expand_user_path(backend.settings().save_root))
}

/// Spawn the worker thread for one meeting, registering its cancel flag. Errors
/// if another meeting is already processing (single shared GPU context).
fn spawn_meeting_job(
    app: AppHandle,
    backend_state: BackendState,
    meeting_state: MeetingState,
    id: String,
) -> Result<(), String> {
    let (save_root, model_directory, suppress_low_confidence) = {
        let backend = backend_state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        (
            expand_user_path(&settings.save_root),
            expand_user_path(&settings.model_directory),
            settings.suppress_low_confidence_transcripts,
        )
    };

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut runtime = meeting_state.lock().map_err(|error| error.to_string())?;
        if let Some(current) = runtime.current.as_ref() {
            return Err(format!(
                "Another meeting ({current}) is being processed. Try again after it finishes."
            ));
        }
        runtime.current = Some(id.clone());
        runtime.cancel = Some(cancel.clone());
    }

    let emit = meeting_event_emitter(app);
    let meeting_state_for_thread = meeting_state.clone();
    thread::spawn(move || {
        if let Err(error) = wakenote::meeting::run_meeting_job(
            &save_root,
            &model_directory,
            &id,
            suppress_low_confidence,
            cancel,
            emit,
        ) {
            eprintln!("[wakenote] meeting job {id} error: {error}");
        }
        if let Ok(mut runtime) = meeting_state_for_thread.lock() {
            if runtime.current.as_deref() == Some(id.as_str()) {
                runtime.current = None;
                runtime.cancel = None;
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn list_meetings(state: State<'_, BackendState>) -> Result<Vec<MeetingSummary>, String> {
    Ok(wakenote::meeting::list_meetings(&meeting_save_root(&state)?))
}

#[tauri::command]
fn import_and_start_meeting(
    app: AppHandle,
    state: State<'_, BackendState>,
    meeting_state: State<'_, MeetingState>,
    source_path: String,
) -> Result<MeetingSummary, String> {
    // Refuse before copying the file if another meeting is already running, so
    // we never leave an orphaned record that can't start (one GPU context).
    {
        let runtime = meeting_state.lock().map_err(|error| error.to_string())?;
        if let Some(current) = runtime.current.as_ref() {
            return Err(format!(
                "Another meeting ({current}) is being processed. Try again after it finishes."
            ));
        }
    }
    let (save_root, model_id, language) = {
        let backend = state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        (
            expand_user_path(&settings.save_root),
            settings.selected_model.clone(),
            settings.transcription_language,
        )
    };
    let record = wakenote::meeting::import_meeting(
        &save_root,
        Path::new(&source_path),
        &model_id,
        language,
        env!("CARGO_PKG_VERSION"),
        chrono::Local::now(),
    )?;
    spawn_meeting_job(
        app,
        state.inner().clone(),
        meeting_state.inner().clone(),
        record.id.clone(),
    )?;
    Ok(record.summary())
}

#[tauri::command]
fn meeting_detail(state: State<'_, BackendState>, id: String) -> Result<MeetingDetail, String> {
    wakenote::meeting::meeting_detail(&meeting_save_root(&state)?, &id)
}

#[tauri::command]
fn cancel_meeting(meeting_state: State<'_, MeetingState>, id: String) -> Result<(), String> {
    let runtime = meeting_state.lock().map_err(|error| error.to_string())?;
    if runtime.current.as_deref() == Some(id.as_str()) {
        if let Some(cancel) = runtime.cancel.as_ref() {
            cancel.store(true, Ordering::Release);
        }
    }
    Ok(())
}

#[tauri::command]
fn resume_meeting(
    app: AppHandle,
    state: State<'_, BackendState>,
    meeting_state: State<'_, MeetingState>,
    id: String,
) -> Result<MeetingSummary, String> {
    let save_root = meeting_save_root(&state)?;
    spawn_meeting_job(
        app,
        state.inner().clone(),
        meeting_state.inner().clone(),
        id.clone(),
    )?;
    let detail = wakenote::meeting::meeting_detail(&save_root, &id)?;
    Ok(detail.record.summary())
}

#[tauri::command]
fn delete_meeting(
    state: State<'_, BackendState>,
    meeting_state: State<'_, MeetingState>,
    id: String,
) -> Result<(), String> {
    {
        let runtime = meeting_state.lock().map_err(|error| error.to_string())?;
        if runtime.current.as_deref() == Some(id.as_str()) {
            return Err("Cannot delete a meeting that is being processed. Cancel it first.".to_string());
        }
    }
    wakenote::meeting::delete_meeting(&meeting_save_root(&state)?, &id)
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
        let (handler, events) = {
            let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
            backend.stop_capture_session()?;
            live_events_for_dispatch(&mut backend)
        };
        dispatch_live_events(handler, events);
    }

    let settings = {
        let backend = backend_state.lock().map_err(|error| error.to_string())?;
        let settings = backend.settings();
        if settings.pause_all || !settings.recording_enabled {
            return Ok(backend.app_status());
        }
        settings
    };

    // Recovery override (set by the watchdog when falling back to default after
    // a wedged pinned stream) takes precedence over the persisted selection for
    // this single start. The override is one-shot — `take_microphone_recovery_override`
    // clears it so subsequent restarts honour the user's pinned device again.
    let recovery_override = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        backend.take_microphone_recovery_override()
    };
    let recovery_override_active = recovery_override.is_some();
    let (requested_device_id, requested_label_hint) = match recovery_override {
        Some(override_data) => (override_data.device_id, override_data.label_hint),
        None => (
            settings.selected_microphone.clone(),
            Some(settings.selected_microphone_label.clone()).filter(|label| !label.is_empty()),
        ),
    };
    let resolved = match resolve_capture_device_with_timeout(
        requested_device_id.clone(),
        requested_label_hint.clone(),
    ) {
        Ok(resolved) => resolved,
        Err(error) => {
            let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
            return Ok(backend.capture_start_failed(format!("Microphone unavailable: {error}")));
        }
    };
    // True if the device we actually opened diverges from the user's pinned
    // selection — either because the watchdog overrode it or because cpal
    // genuinely could not match (label differs). A stable-id drift with the
    // same label is NOT a divergence; the resolver found the same physical
    // device after cpal re-enumeration.
    let used_fallback_device = recovery_override_active
        || pinned_device_mismatch(
            &settings.selected_microphone,
            &settings.selected_microphone_label,
            &resolved.device_id,
            &resolved.device_name,
            resolved.used_fallback_device,
        );

    let (device_id, sample_rate, handler, events) = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        // Persist the resolved stable id back into settings when we matched
        // a pinned device by label — keeps the priority list and pinned id
        // pointing at the current cpal index instead of drifting forever.
        backend.reconcile_resolved_device_id(
            &requested_device_id,
            &resolved.device_id,
            &resolved.device_name,
        );
        backend.start_capture_session_with_device(
            resolved.sample_rate,
            chrono::Utc::now(),
            resolved.device_id.clone(),
            resolved.device_name,
            used_fallback_device,
        )?;
        let (handler, events) = live_events_for_dispatch(&mut backend);
        (resolved.device_id, resolved.sample_rate, handler, events)
    };
    dispatch_live_events(handler, events);

    let backend_arc = Arc::clone(backend_state);
    let callback_backend = backend_arc.clone();
    let callback_transcription = transcription_state.clone();
    let callback_app = app.clone();
    let callback_overlay_level_throttle = Arc::new(Mutex::new(
        Instant::now()
            .checked_sub(Duration::from_millis(100))
            .unwrap_or_else(Instant::now),
    ));
    let start_result = live_state.lock().map_err(|error| error.to_string())?.start(
        AudioInputConfig {
            device_id,
            sample_rate: Some(sample_rate),
            label_hint: requested_label_hint,
        },
        move |frame| {
            let waveform_levels = overlay::waveform_levels_from_samples(
                &frame.samples,
                overlay::OVERLAY_WAVEFORM_BAR_COUNT,
            );
            let (should_kick, handler, events, emit_waveform, tray_status) =
                if let Ok(mut backend) = callback_backend.lock() {
                    let status = backend.process_audio_frame(frame);
                    let emit_waveform = status
                        .as_ref()
                        .map(|status| matches!(status.tray_state, TrayState::Recording))
                        .unwrap_or(false);
                    let should_kick = status
                        .map(|_| backend.should_process_transcriptions())
                        .unwrap_or(false);
                    let tray_status = Some((backend.settings(), backend.app_status()));
                    let (handler, events) = live_events_for_dispatch(&mut backend);
                    (should_kick, handler, events, emit_waveform, tray_status)
                } else {
                    (false, None, Vec::new(), false, None)
                };
            dispatch_live_events(handler, events);
            if let Some((settings, status)) = tray_status {
                update_tray_presentation(&callback_app, &settings, &status);
            }
            if emit_waveform && overlay_level_emit_due(&callback_overlay_level_throttle) {
                overlay::emit_waveform_levels(&callback_app, waveform_levels);
            }
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

    // Warm the live-transcription model now that capture is live. whisper
    // (esp. medium, 1.5 GB) takes 5-15 s to load; preloading here means the
    // first utterance produces captions immediately instead of being dropped
    // while the model loads. Best-effort — a failure just falls back to the
    // original lazy load on the first partial request.
    if let (Some(transcriber_state), Some(model_id)) = (
        app.try_state::<LiveTranscriberState>(),
        backend_state
            .lock()
            .ok()
            .map(|backend| backend.settings().selected_model),
    ) {
        if let Ok(slot) = transcriber_state.lock() {
            if let Some(service) = slot.as_ref() {
                service.preload(model_id);
            }
        }
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
    let (status, handler, events) = {
        let mut backend = backend_state.lock().map_err(|error| error.to_string())?;
        let status = backend.stop_capture_session()?;
        let (handler, events) = live_events_for_dispatch(&mut backend);
        (status, handler, events)
    };
    dispatch_live_events(handler, events);
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

#[tauri::command]
fn list_recognized_sources(backend_state: State<'_, BackendState>) -> Vec<RecognizedSourceInfo> {
    let settings = backend_state
        .lock()
        .map(|backend| backend.settings())
        .unwrap_or_default();
    recognized_source_infos(&settings)
}

#[tauri::command]
fn source_capture_status(
    system_capture_state: State<'_, SystemCaptureState>,
    detected_source_state: State<'_, DetectedSourceState>,
) -> SourceCaptureStatus {
    let detected = detected_source_state
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(SourcePayload::from));
    let capturing = system_capture_state
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false);
    SourceCaptureStatus {
        detected,
        capturing,
    }
}

#[tauri::command]
fn start_source_capture(
    source_id: String,
    app: AppHandle,
    backend_state: State<'_, BackendState>,
    system_capture_state: State<'_, SystemCaptureState>,
    source_capture_lifecycle_state: State<'_, SourceCaptureLifecycleState>,
    detected_source_state: State<'_, DetectedSourceState>,
    source_capture_pause_state: State<'_, SourceCapturePauseState>,
    transcription_state: State<'_, AutoTranscriptionState>,
) -> Result<AppStatus, String> {
    if let Ok(mut paused) = source_capture_pause_state.lock() {
        paused.remove(&source_id);
    }
    start_source_capture_runtime(
        &app,
        &source_id,
        backend_state.inner(),
        system_capture_state.inner(),
        source_capture_lifecycle_state.inner(),
        detected_source_state.inner(),
        transcription_state.inner().clone(),
    )
}

/// Start system-audio capture for a detected source. The source must currently
/// be detected (its pid is the ScreenCaptureKit target) and its id must match.
fn attempt_source_capture_start(
    source: &DetectedSource,
    backend_state: &BackendState,
    system_capture_state: &SystemCaptureState,
    source_capture_lifecycle_state: &SourceCaptureLifecycleState,
    on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
    starter: &mut impl SourceCaptureStreamStarter,
    now: Instant,
    screen_recording_status: permissions::PermissionGrantStatus,
) -> Result<SourceCaptureAttemptResult, String> {
    let stream_running = system_capture_state
        .lock()
        .map(|slot| slot.is_some())
        .unwrap_or(false);
    if stream_running {
        if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
            source_capture_mark_running(&mut lifecycle, &source.source_id);
        }
        let backend = backend_state.lock().map_err(|e| e.to_string())?;
        return Ok(SourceCaptureAttemptResult {
            status: backend.app_status(),
            outcome: SourceCaptureAttemptOutcome::AlreadyCapturing,
        });
    }

    let start_attempt = {
        let mut lifecycle = source_capture_lifecycle_state
            .lock()
            .map_err(|e| e.to_string())?;
        match source_capture_start_decision(&lifecycle, &source.source_id, now) {
            SourceCaptureStartDecision::Start { attempt } => {
                source_capture_mark_starting(&mut lifecycle, &source.source_id, attempt);
                attempt
            }
            SourceCaptureStartDecision::AlreadyStarting
            | SourceCaptureStartDecision::AlreadyRunning
            | SourceCaptureStartDecision::Stopping => {
                let backend = backend_state.lock().map_err(|e| e.to_string())?;
                return Ok(SourceCaptureAttemptResult {
                    status: backend.app_status(),
                    outcome: SourceCaptureAttemptOutcome::Deferred,
                });
            }
            SourceCaptureStartDecision::BackingOff { retry_after } => {
                if let Ok(backend) = backend_state.lock() {
                    append_runtime_debug_log(
                        &backend.settings(),
                        format!(
                            "[source-capture] backoff source_id={} retry_after_ms={}",
                            source.source_id,
                            retry_after.as_millis()
                        ),
                    );
                    return Ok(SourceCaptureAttemptResult {
                        status: backend.app_status(),
                        outcome: SourceCaptureAttemptOutcome::Deferred,
                    });
                }
                return Err("source capture state is unavailable".to_string());
            }
        }
    };

    let sample_rate = system_audio_sample_rate();
    let source_label = recording_source_label(source);
    {
        let mut backend = backend_state.lock().map_err(|e| e.to_string())?;
        append_runtime_debug_log(
            &backend.settings(),
            format!(
                "[source-capture] start source_id={} label={} recording_label={} app={} pid={} sample_rate={} attempt={}",
                source.source_id,
                source.label,
                source_label,
                source.app_name,
                source.pid,
                sample_rate,
                start_attempt
            ),
        );
        if let Err(error) = backend.start_system_capture_session(
            sample_rate,
            chrono::Utc::now(),
            source.app_name.clone(),
            source.source_id.clone(),
            source_label,
        ) {
            if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
                source_capture_mark_failed(&mut lifecycle, &source.source_id, now);
            }
            let diagnostic = source_capture_failure_diagnostic(
                &source.source_id,
                start_attempt,
                screen_recording_status,
                &error,
            );
            append_runtime_debug_log(
                &backend.settings(),
                format!(
                    "{} label={} app={} pid={}",
                    diagnostic, source.label, source.app_name, source.pid
                ),
            );
            return Err(error);
        }
    }

    let capture_scope = source_capture_scope(source);
    if let Ok(backend) = backend_state.lock() {
        append_runtime_debug_log(
            &backend.settings(),
            format!(
                "[source-capture] input_scope source_id={} scope={} app={} pid={}",
                source.source_id, capture_scope, source.app_name, source.pid
            ),
        );
    }

    let stream = match starter.start_source_capture_stream(source, sample_rate, on_frame) {
        Ok(stream) => stream,
        Err(error) => {
            let message = format!("system-audio capture failed: {error}");
            let diagnostic = source_capture_failure_diagnostic(
                &source.source_id,
                start_attempt,
                screen_recording_status,
                &message,
            );
            // Roll back the capture session we opened above.
            if let Ok(mut backend) = backend_state.lock() {
                append_runtime_debug_log(
                    &backend.settings(),
                    format!(
                        "{} label={} app={} pid={}",
                        diagnostic, source.label, source.app_name, source.pid
                    ),
                );
                let _ = backend.stop_system_capture_session();
            }
            if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
                source_capture_mark_failed(&mut lifecycle, &source.source_id, now);
            }
            return Err(message);
        }
    };

    *system_capture_state.lock().map_err(|e| e.to_string())? = Some(stream);
    if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
        source_capture_mark_running(&mut lifecycle, &source.source_id);
    }

    let backend = backend_state.lock().map_err(|e| e.to_string())?;
    Ok(SourceCaptureAttemptResult {
        status: backend.app_status(),
        outcome: SourceCaptureAttemptOutcome::Started,
    })
}

fn start_source_capture_runtime(
    app: &AppHandle,
    source_id: &str,
    backend_state: &BackendState,
    system_capture_state: &SystemCaptureState,
    source_capture_lifecycle_state: &SourceCaptureLifecycleState,
    detected_source_state: &DetectedSourceState,
    transcription_state: AutoTranscriptionState,
) -> Result<AppStatus, String> {
    let source = {
        let slot = detected_source_state.lock().map_err(|e| e.to_string())?;
        match slot.as_ref() {
            Some(source) if source.source_id == source_id => source.clone(),
            _ => {
                let error = format!("source '{source_id}' is not currently detected");
                emit_source_capture_error(app, source_id, &error);
                return Err(error);
            }
        }
    };

    let callback_backend = Arc::clone(backend_state);
    let callback_app = app.clone();
    let callback_transcription = transcription_state.clone();
    let on_frame = move |frame: AudioFrame| {
        let (should_kick, handler, events, tray_status) =
            if let Ok(mut backend) = callback_backend.lock() {
                let status = backend.process_system_audio_frame(frame);
                let should_kick = status
                    .map(|_| backend.should_process_transcriptions())
                    .unwrap_or(false);
                let tray_status = Some((backend.settings(), backend.app_status()));
                let (handler, events) = live_events_for_dispatch(&mut backend);
                (should_kick, handler, events, tray_status)
            } else {
                (false, None, Vec::new(), None)
            };
        dispatch_live_events(handler, events);
        if let Some((settings, status)) = tray_status {
            update_tray_presentation(&callback_app, &settings, &status);
        }
        if should_kick {
            kick_transcription_worker(
                callback_app.clone(),
                callback_backend.clone(),
                callback_transcription.clone(),
            );
        }
    };

    let mut starter = SystemAudioStreamStarter;
    let screen_recording_status = permissions::permission_snapshot().screen_recording.status;
    let result = match attempt_source_capture_start(
        &source,
        backend_state,
        system_capture_state,
        source_capture_lifecycle_state,
        Arc::new(on_frame),
        &mut starter,
        Instant::now(),
        screen_recording_status,
    ) {
        Ok(result) => result,
        Err(error) => {
            emit_source_capture_error(app, source_id, &error);
            return Err(error);
        }
    };

    if result.outcome == SourceCaptureAttemptOutcome::Started {
        let _ = app.emit(
            EVENT_SOURCE_CAPTURE_STARTED,
            SourceCapturePayload {
                source_id: source.source_id.clone(),
                label: source.label.clone(),
            },
        );

        let settings = backend_state
            .lock()
            .map_err(|e| e.to_string())?
            .settings();
        update_tray_presentation(app, &settings, &result.status);
    }
    Ok(result.status)
}

#[tauri::command]
fn stop_source_capture(
    app: AppHandle,
    backend_state: State<'_, BackendState>,
    system_capture_state: State<'_, SystemCaptureState>,
    source_capture_lifecycle_state: State<'_, SourceCaptureLifecycleState>,
    detected_source_state: State<'_, DetectedSourceState>,
    meeting_state: State<'_, MeetingState>,
    source_capture_pause_state: State<'_, SourceCapturePauseState>,
    transcription_state: State<'_, AutoTranscriptionState>,
) -> Result<AppStatus, String> {
    if let Some(source_id) = detected_source_state
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|source| source.source_id.clone()))
    {
        if let Ok(mut paused) = source_capture_pause_state.lock() {
            paused.insert(source_id);
        }
    }
    let status = stop_source_capture_runtime(
        &app,
        backend_state.inner(),
        system_capture_state.inner(),
        source_capture_lifecycle_state.inner(),
        detected_source_state.inner(),
        meeting_state.inner(),
    )?;
    kick_transcription_worker_if_needed(
        app,
        backend_state.inner().clone(),
        transcription_state.inner().clone(),
    );
    Ok(status)
}

fn stop_source_capture_runtime(
    app: &AppHandle,
    backend_state: &BackendState,
    system_capture_state: &SystemCaptureState,
    source_capture_lifecycle_state: &SourceCaptureLifecycleState,
    detected_source_state: &DetectedSourceState,
    meeting_state: &MeetingState,
) -> Result<AppStatus, String> {
    let stopped_source_id = detected_source_state
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|source| source.source_id.clone()));
    if let Some(source_id) = stopped_source_id.as_deref() {
        if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
            source_capture_mark_stopping(&mut lifecycle, source_id);
        }
    }
    // Dropping the handle stops the ScreenCaptureKit stream.
    *system_capture_state.lock().map_err(|e| e.to_string())? = None;
    let (settings, status, meeting_job_actions) = {
        let mut backend = backend_state.lock().map_err(|e| e.to_string())?;
        append_runtime_debug_log(&backend.settings(), "[source-capture] stop");
        let status = backend.stop_system_capture_session()?;
        let meeting_job_actions = drain_finished_system_meeting_job_actions(&mut backend);
        (backend.settings(), status, meeting_job_actions)
    };
    start_finished_system_meeting_jobs(
        app,
        backend_state,
        meeting_state,
        meeting_job_actions,
        &settings,
    );
    update_tray_presentation(app, &settings, &status);
    let payload = detected_source_state
        .lock()
        .ok()
        .and_then(|slot| {
            slot.as_ref().map(|source| SourceCapturePayload {
                source_id: source.source_id.clone(),
                label: source.label.clone(),
            })
        })
        .unwrap_or(SourceCapturePayload {
            source_id: String::new(),
            label: String::new(),
        });
    let _ = app.emit(EVENT_SOURCE_CAPTURE_STOPPED, payload);
    if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
        source_capture_mark_idle(&mut lifecycle);
    }
    Ok(status)
}

fn emit_source_capture_error(app: &AppHandle, source_id: &str, error: &str) {
    let _ = app.emit(
        EVENT_SOURCE_CAPTURE_ERROR,
        SourceCaptureErrorPayload {
            source_id: source_id.to_string(),
            error: error.to_string(),
        },
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FinishedSystemMeetingJobAction {
    Start(String),
    Pending { id: String, reason: &'static str },
}

fn drain_finished_system_meeting_job_actions(
    backend: &mut AppBackend,
) -> Vec<FinishedSystemMeetingJobAction> {
    let ids = backend.take_finished_system_meeting_ids();
    let settings = backend.settings();
    ids.into_iter()
        .map(|id| {
            if !settings.transcription_enabled {
                FinishedSystemMeetingJobAction::Pending {
                    id,
                    reason: "transcription_disabled",
                }
            } else if !selected_meeting_model_is_ready(&settings) {
                FinishedSystemMeetingJobAction::Pending {
                    id,
                    reason: "model_not_ready",
                }
            } else {
                FinishedSystemMeetingJobAction::Start(id)
            }
        })
        .collect()
}

fn selected_meeting_model_is_ready(settings: &AppSettings) -> bool {
    expand_user_path(&settings.model_directory)
        .join(format!("{}.bin", settings.selected_model))
        .is_file()
}

fn start_finished_system_meeting_jobs(
    app: &AppHandle,
    backend_state: &BackendState,
    meeting_state: &MeetingState,
    actions: Vec<FinishedSystemMeetingJobAction>,
    settings: &AppSettings,
) {
    for action in actions {
        let id = match action {
            FinishedSystemMeetingJobAction::Start(id) => id,
            FinishedSystemMeetingJobAction::Pending { id, reason } => {
                append_runtime_debug_log(
                    settings,
                    format!(
                        "[meeting-capture] state=pending id={} reason={}",
                        id, reason
                    ),
                );
                continue;
            }
        };
        append_runtime_debug_log(
            settings,
            format!("[meeting-capture] state=queued id={}", id),
        );
        if let Err(error) = spawn_meeting_job(
            app.clone(),
            backend_state.clone(),
            meeting_state.clone(),
            id.clone(),
        ) {
            append_runtime_debug_log(
                settings,
                format!(
                    "[meeting-capture] state=pending id={} reason=worker_unavailable error={}",
                    id, error
                ),
            );
        } else {
            append_runtime_debug_log(
                settings,
                format!("[meeting-capture] state=transcribing id={}", id),
            );
        }
    }
}

/// ScreenCaptureKit is asked to deliver audio at the transcription pipeline rate
/// so no resampling is needed; mirror that here for the capture session.
fn system_audio_sample_rate() -> u32 {
    PIPELINE_SAMPLE_RATE
}

fn append_runtime_debug_log(settings: &AppSettings, message: impl AsRef<str>) {
    append_debug_log(expand_user_path(&settings.save_root), message);
}

fn source_watcher_should_enumerate_windows(
    settings: &AppSettings,
    screen_recording_status: permissions::PermissionGrantStatus,
) -> bool {
    settings.system_audio_enabled
        && matches!(
            screen_recording_status,
            permissions::PermissionGrantStatus::Granted
        )
}

/// Background watcher: while `system_audio_enabled`, poll the on-screen windows
/// every [`SOURCE_WATCH_INTERVAL`] and react to source transitions. Cheap when
/// the feature is off (it just re-reads the flag and sleeps). Notifications are
/// posted at most once per source per app session via an in-memory snooze set.
fn spawn_source_watcher(
    app: AppHandle,
    backend_state: BackendState,
    system_capture_state: SystemCaptureState,
    source_capture_lifecycle_state: SourceCaptureLifecycleState,
    detected_source_state: DetectedSourceState,
    meeting_state: MeetingState,
    source_capture_pause_state: SourceCapturePauseState,
    transcription_state: AutoTranscriptionState,
) {
    thread::spawn(move || {
        let mut missing_source_polls = 0;
        loop {
            thread::sleep(SOURCE_WATCH_INTERVAL);

            let settings = match backend_state.lock() {
                Ok(backend) => backend.settings(),
                Err(_) => continue,
            };
            if !settings.system_audio_enabled {
                continue;
            }
            let screen_recording_status =
                permissions::permission_snapshot().screen_recording.status;
            if !source_watcher_should_enumerate_windows(&settings, screen_recording_status) {
                continue;
            }

            let windows = enumerate_windows();
            let source_defs = source_definitions(&settings);
            let previous = match detected_source_state.lock() {
                Ok(slot) => slot.clone(),
                Err(_) => continue,
            };
            let (transition, next_missing_source_polls) =
                compute_source_transition_with_missing_grace(
                    previous.as_ref(),
                    &windows,
                    &source_defs,
                    missing_source_polls,
                    SOURCE_MISSING_GRACE_POLLS,
                );
            missing_source_polls = next_missing_source_polls;
            match transition {
                SourceTransition::Detected(source) => {
                    if let Ok(mut slot) = detected_source_state.lock() {
                        *slot = Some(source.clone());
                    }
                    append_runtime_debug_log(
                        &settings,
                        format!(
                            "[source-watch] detected source_id={} label={} app={} pid={}",
                            source.source_id, source.label, source.app_name, source.pid
                        ),
                    );
                    let _ = app.emit(EVENT_SOURCE_DETECTED, SourcePayload::from(&source));

                    let already_capturing = system_capture_state
                        .lock()
                        .map(|slot| slot.is_some())
                        .unwrap_or(false);
                    let auto_capture = resolve_auto_prompt(&settings, &source.source_id);
                    let paused_this_session = source_capture_pause_state
                        .lock()
                        .map(|paused| paused.contains(&source.source_id))
                        .unwrap_or(false);
                    if should_auto_capture_source(
                        settings.system_audio_enabled,
                        auto_capture,
                        already_capturing,
                        paused_this_session,
                    ) {
                        if let Err(error) = start_source_capture_runtime(
                            &app,
                            &source.source_id,
                            &backend_state,
                            &system_capture_state,
                            &source_capture_lifecycle_state,
                            &detected_source_state,
                            transcription_state.clone(),
                        ) {
                            eprintln!("[source-watch] auto-capture failed: {error}");
                        }
                    }
                }
                SourceTransition::Ended(source) => {
                    let capturing = system_capture_state
                        .lock()
                        .map(|slot| slot.is_some())
                        .unwrap_or(false);
                    let recent_audio = backend_state
                        .lock()
                        .map(|backend| {
                            backend.has_recent_system_audio_frame(
                                Instant::now(),
                                SOURCE_CAPTURE_AUDIO_LIVENESS_HOLD,
                            )
                        })
                        .unwrap_or(false);
                    let ended_transition = SourceTransition::Ended(source.clone());
                    if should_defer_source_end_for_recent_audio(
                        &ended_transition,
                        capturing,
                        recent_audio,
                    ) {
                        missing_source_polls = SOURCE_MISSING_GRACE_POLLS.saturating_sub(1);
                        append_runtime_debug_log(
                            &settings,
                            format!(
                                "[source-watch] hold-ended source_id={} label={} app={} pid={} audio_liveness_window_ms={}",
                                source.source_id,
                                source.label,
                                source.app_name,
                                source.pid,
                                SOURCE_CAPTURE_AUDIO_LIVENESS_HOLD.as_millis()
                            ),
                        );
                        continue;
                    }

                    append_runtime_debug_log(
                        &settings,
                        format!(
                            "[source-watch] ended source_id={} label={} app={} pid={}",
                            source.source_id, source.label, source.app_name, source.pid
                        ),
                    );
                    if let Ok(mut paused) = source_capture_pause_state.lock() {
                        paused.remove(&source.source_id);
                    }
                    let _ = app.emit(EVENT_SOURCE_ENDED, SourcePayload::from(&source));

                    if capturing {
                        if let Err(error) = stop_source_capture_runtime(
                            &app,
                            &backend_state,
                            &system_capture_state,
                            &source_capture_lifecycle_state,
                            &detected_source_state,
                            &meeting_state,
                        ) {
                            eprintln!("[source-watch] auto-stop failed: {error}");
                        }
                    } else if let Ok(mut lifecycle) = source_capture_lifecycle_state.lock() {
                        source_capture_mark_idle(&mut lifecycle);
                    }
                    if let Ok(mut slot) = detected_source_state.lock() {
                        *slot = None;
                    }
                }
                SourceTransition::Unchanged => {
                    let source = detected_source_state
                        .lock()
                        .ok()
                        .and_then(|slot| slot.clone());
                    let Some(source) = source else {
                        continue;
                    };
                    let already_capturing = system_capture_state
                        .lock()
                        .map(|slot| slot.is_some())
                        .unwrap_or(false);
                    let auto_capture = resolve_auto_prompt(&settings, &source.source_id);
                    let paused_this_session = source_capture_pause_state
                        .lock()
                        .map(|paused| paused.contains(&source.source_id))
                        .unwrap_or(false);
                    if should_auto_capture_source(
                        settings.system_audio_enabled,
                        auto_capture,
                        already_capturing,
                        paused_this_session,
                    ) {
                        if let Err(error) = start_source_capture_runtime(
                            &app,
                            &source.source_id,
                            &backend_state,
                            &system_capture_state,
                            &source_capture_lifecycle_state,
                            &detected_source_state,
                            transcription_state.clone(),
                        ) {
                            eprintln!("[source-watch] auto-capture retry failed: {error}");
                        }
                    }
                }
            }
        }
    });
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
    eprintln!("[wakenote] wire_live_transcription: model_dir={model_directory}");

    let app_for_partial = app_handle.clone();
    let on_partial: Arc<dyn Fn(LivePartialEvent) + Send + Sync> = Arc::new(
        move |event| match event {
            LivePartialEvent::Text(result) => {
                eprintln!(
                    "[wakenote] live partial -> FE chunk_id={} text='{}'",
                    result.chunk_id, result.text
                );
                if let Err(error) = app_for_partial.emit(
                    EVENT_LIVE_PARTIAL,
                    LivePartialPayload {
                        chunk_id: result.chunk_id,
                        text: result.text,
                    },
                ) {
                    eprintln!("[wakenote] WARN failed to emit partial: {error}");
                }
            }
            LivePartialEvent::ModelMissing { chunk_id, model_id } => {
                eprintln!(
                    "[wakenote] live partial: model missing chunk_id={chunk_id} model={model_id}"
                );
                let error = format!(
                    "Live transcription model {model_id} is not installed. Open Models tab to download it.",
                );
                if let Err(emit_error) = app_for_partial.emit(
                    EVENT_LIVE_FAILED,
                    LiveFailedPayload {
                        chunk_id: Some(chunk_id),
                        audio_path: String::new(),
                        recorded_at: chrono::Utc::now().to_rfc3339(),
                        error,
                    },
                ) {
                    eprintln!("[wakenote] WARN failed to emit live-failed: {emit_error}");
                }
            }
            LivePartialEvent::EngineError { chunk_id, message } => {
                eprintln!(
                    "[wakenote] live partial: engine error chunk_id={chunk_id} message={message}"
                );
                if let Err(emit_error) = app_for_partial.emit(
                    EVENT_LIVE_FAILED,
                    LiveFailedPayload {
                        chunk_id: Some(chunk_id),
                        audio_path: String::new(),
                        recorded_at: chrono::Utc::now().to_rfc3339(),
                        error: format!("Live partial decode failed: {message}"),
                    },
                ) {
                    eprintln!("[wakenote] WARN failed to emit live-failed: {emit_error}");
                }
            }
        },
    );

    let service = Arc::new(LiveTranscriptionService::new(&model_directory, on_partial));
    if let Ok(mut slot) = live_transcriber_state.lock() {
        *slot = Some(service.clone());
    }

    let app_for_handler = app_handle.clone();
    let model_directory_for_handler = model_directory.clone();
    let service_for_handler = service.clone();
    let handler: wakenote::commands::LiveEventHandler = Arc::new(move |event| match event {
        LiveTranscriptEvent::Started {
            chunk_id,
            started_at,
            overlay_position,
        } => {
            eprintln!("[wakenote] handler: emit started chunk_id={chunk_id}");
            if let Err(error) = overlay::show_overlay_on_main_thread(
                &app_for_handler,
                OverlayState::Recording,
                overlay_position,
                "show recording on voice",
            ) {
                eprintln!("[overlay] show recording on voice failed: {error}");
            }
            if let Err(error) = app_for_handler.emit(
                EVENT_LIVE_STARTED,
                LiveStartedPayload {
                    chunk_id,
                    started_at: started_at.to_rfc3339(),
                },
            ) {
                eprintln!("[wakenote] WARN emit started failed: {error}");
            }
        }
        LiveTranscriptEvent::SamplesReady {
            chunk_id,
            model_id,
            language,
            suppress_low_confidence_transcripts,
            sample_rate,
            samples,
        } => {
            if !model_supports_live_partials(&model_directory_for_handler, &model_id) {
                eprintln!("[wakenote] handler: skip live partial for non-whisper model={model_id}");
                return;
            }
            eprintln!(
                "[wakenote] handler: submit live partial chunk_id={chunk_id} model={model_id} samples={} rate={sample_rate}",
                samples.len()
            );
            service_for_handler.submit(LivePartialRequest {
                chunk_id,
                model_id,
                language,
                suppress_low_confidence_transcripts,
                sample_rate,
                samples,
            });
        }
        LiveTranscriptEvent::Committed {
            chunk_id,
            audio_path,
            overlay_position: _,
            will_transcribe: _,
        } => {
            eprintln!(
                "[wakenote] handler: emit committed chunk_id={chunk_id} path={}",
                audio_path.display()
            );
            if let Err(error) = app_for_handler.emit(
                EVENT_LIVE_COMMITTED,
                LiveCommittedPayload {
                    chunk_id,
                    audio_path: audio_path.to_string_lossy().to_string(),
                },
            ) {
                eprintln!("[wakenote] WARN emit committed failed: {error}");
            }
            if let Err(error) =
                overlay::hide_overlay_on_main_thread(&app_for_handler, "hide after commit")
            {
                eprintln!("[overlay] commit hide failed: {error}");
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
        eprintln!(
            "[wakenote] queue worker thread started max_parallel={MAX_PARALLEL_TRANSCRIPTIONS}"
        );
        let (outcome_tx, outcome_rx) = mpsc::channel();
        let mut active_jobs = 0usize;

        loop {
            let started_jobs = match backend_state.lock() {
                Ok(mut backend) => {
                    backend.start_transcription_jobs_up_to(MAX_PARALLEL_TRANSCRIPTIONS)
                }
                Err(_) => break,
            };
            if started_jobs.is_empty() && active_jobs == 0 {
                eprintln!("[wakenote] queue worker: no pending jobs, exiting loop");
                break;
            }

            for started in started_jobs {
                spawn_transcription_job(started, outcome_tx.clone());
                active_jobs = active_jobs.saturating_add(1);
            }

            if active_jobs == 0 {
                continue;
            }

            let Ok((audio_path, outcome)) = outcome_rx.recv() else {
                eprintln!("[wakenote] queue worker: outcome channel closed");
                break;
            };
            eprintln!(
                "[wakenote] queue worker: job id={} outcome={:?}",
                outcome.id, outcome.status
            );

            emit_outcome_to_frontend(&app, &backend_state, &audio_path, &outcome);

            match backend_state.lock() {
                Ok(mut backend) => {
                    let _ = backend.finish_transcription_job(outcome);
                }
                Err(_) => break,
            }
            active_jobs = active_jobs.saturating_sub(1);
        }

        transcription_state.store(false, Ordering::Release);

        kick_transcription_worker_if_needed(app, backend_state, transcription_state);
    });
}

fn spawn_transcription_job(
    started: StartedTranscriptionJob,
    outcome_tx: mpsc::Sender<(PathBuf, TranscriptionJobOutcome)>,
) {
    thread::spawn(move || {
        let audio_path = started.job.audio_path.clone();
        eprintln!(
            "[wakenote] queue worker: processing job id={} path={} model={}",
            started.job.id,
            audio_path.display(),
            started.job.model_id
        );

        let worker = TranscriptionWorker::with_options(
            RuntimeTranscriber::new(started.model_directory),
            TranscriptionWorkerOptions {
                language: started.language,
                suppress_low_confidence_transcripts: started.suppress_low_confidence_transcripts,
            },
        );
        let outcome = worker
            .process_started_job(&started.job)
            .unwrap_or_else(|error| {
                eprintln!("[wakenote] queue worker: process_started_job error: {error}");
                TranscriptionJobOutcome::failed(started.job.id, error.to_string())
            });

        let _ = outcome_tx.send((audio_path, outcome));
    });
}

fn emit_outcome_to_frontend(
    app: &AppHandle,
    backend_state: &BackendState,
    audio_path: &Path,
    outcome: &TranscriptionJobOutcome,
) {
    let (chunk_id, auto_input_enabled, settings_for_log) = backend_state
        .lock()
        .ok()
        .map(|backend| {
            (
                backend
                    .chunk_id_for_audio_path(audio_path)
                    .or_else(|| chunk_id_from_metadata(audio_path)),
                backend.settings().auto_transcript_input_enabled,
                backend.settings(),
            )
        })
        .unwrap_or_else(|| {
            (
                chunk_id_from_metadata(audio_path),
                false,
                AppSettings::default(),
            )
        });
    let audio_path_str = audio_path.to_string_lossy().to_string();
    let recorded_at = recorded_at_for_audio_path(audio_path);

    match &outcome.status {
        TranscriptionJobStatus::Completed => {
            let transcript_path = audio_path.with_extension("txt");
            let text = std::fs::read_to_string(&transcript_path)
                .map(|content| content.trim_end().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                eprintln!(
                    "[wakenote] emit_outcome_to_frontend: empty sidecar at {}",
                    transcript_path.display()
                );
                return;
            }
            eprintln!(
                "[wakenote] emit final chunk_id={:?} path={} text_len={}",
                chunk_id,
                audio_path_str,
                text.len()
            );
            if wakenote::text_input::auto_transcript_input_should_type(
                auto_input_enabled,
                chunk_id,
                &text,
            ) {
                let text_for_input = wakenote::text_input::auto_transcript_input_text(
                    &text,
                    settings_for_log.auto_transcript_input_trailing_space,
                );
                thread::spawn(move || {
                    if let Some(text_for_input) = text_for_input {
                        if let Err(error) =
                            wakenote::text_input::type_text_into_focused_cursor(&text_for_input)
                        {
                            append_runtime_debug_log(
                                &settings_for_log,
                                format!("[auto-input] failed to type transcript: {error}"),
                            );
                        }
                    }
                });
            }
            if let Err(error) = app.emit(
                EVENT_LIVE_FINAL,
                LiveFinalPayload {
                    chunk_id,
                    audio_path: audio_path_str,
                    recorded_at,
                    text,
                },
            ) {
                eprintln!("[wakenote] WARN emit final failed: {error}");
            }
        }
        TranscriptionJobStatus::Failed(error) => {
            eprintln!(
                "[wakenote] emit failed chunk_id={:?} path={} error={}",
                chunk_id, audio_path_str, error
            );
            if let Err(emit_error) = app.emit(
                EVENT_LIVE_FAILED,
                LiveFailedPayload {
                    chunk_id,
                    audio_path: audio_path_str,
                    recorded_at,
                    error: error.clone(),
                },
            ) {
                eprintln!("[wakenote] WARN emit failed event failed: {emit_error}");
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

/// Spawn the microphone-input recovery watchdog. The watchdog runs for the
/// lifetime of the process: every [`MIC_RECOVERY_TICK_INTERVAL`] it polls the
/// backend for a [`MicHealthAction`] and orchestrates a capture restart (or
/// fallback to the system default device) when the stream is wedged.
fn spawn_mic_recovery_watchdog(
    app: AppHandle,
    backend_state: BackendState,
    transcription_state: AutoTranscriptionState,
) {
    thread::spawn(move || {
        loop {
            thread::sleep(MIC_RECOVERY_TICK_INTERVAL);
            let (live_running, runtime_error) = match app.try_state::<LiveCaptureState>() {
                Some(state) => match state.lock() {
                    Ok(live) => (live.is_running(), live.runtime_error()),
                    Err(_) => continue,
                },
                None => continue,
            };
            // Decide whether this tick should do anything at all. We act when
            // the stream is running (normal heartbeat/silence watching) OR
            // when no stream is running but the user still wants recording
            // AND the backend is mid-recovery from a previous start failure.
            let (should_evaluate, recovery_pending) = match backend_state.lock() {
                Ok(backend) => (
                    backend.live_capture_should_run() || live_running,
                    backend.mic_recovery_pending(),
                ),
                Err(_) => continue,
            };
            if !live_running && !recovery_pending {
                continue;
            }
            if !should_evaluate {
                continue;
            }
            // If cpal explicitly told us the stream broke, latch that as a
            // pending recovery so the same tick fires an action immediately —
            // no need to wait for heartbeat/silence thresholds.
            if let Some(error) = runtime_error {
                if let Ok(mut backend) = backend_state.lock() {
                    backend.notify_stream_error(format!("audio stream error: {error}"));
                }
            }
            let action = match backend_state.lock() {
                Ok(mut backend) => backend.evaluate_microphone_health(),
                Err(_) => continue,
            };
            // Surface a UI warning once the watchdog has been escalating
            // without recovery long enough that the user is likely to
            // benefit from intervention (Refresh / change priority).
            // Static text — the warning key on the frontend hashes the
            // content, and we want it stable across ticks so a dismiss
            // sticks until conditions change.
            if let Ok(mut backend) = backend_state.lock() {
                let escalations = backend.mic_escalations_since_first_frame();
                if escalations >= MIC_WEDGE_WARNING_THRESHOLD {
                    backend.set_microphone_warning(
                        "Microphone has not produced audio after several recovery attempts. \
                         Try Refresh or change the microphone priority.",
                    );
                }
            }
            let Some(action) = action else {
                continue;
            };
            apply_mic_recovery_action(&app, &backend_state, &transcription_state, action);
        }
    });
}

fn apply_mic_recovery_action(
    app: &AppHandle,
    backend_state: &BackendState,
    transcription_state: &AutoTranscriptionState,
    action: MicHealthAction,
) {
    let live_state = match app.try_state::<LiveCaptureState>() {
        Some(state) => state,
        None => return,
    };
    match action {
        MicHealthAction::SwitchTo { device_id, reason } => {
            eprintln!("[mic-watchdog] switching to {device_id}: {reason}");
            if let Ok(mut backend) = backend_state.lock() {
                backend.set_microphone_recovery_override(&device_id);
            }
            if let Err(error) = stop_live_capture_runtime(app, backend_state, live_state.inner()) {
                eprintln!("[mic-watchdog] stop before switch failed: {error}");
            }
            // CoreAudio frequently retains wedged state for ~tens of ms
            // after a device is released. A brief settling delay before
            // reopening avoids handing us back the same wedged stream.
            thread::sleep(MIC_RECOVERY_SETTLING_DELAY);
            if let Err(error) = start_live_capture_runtime(
                app,
                backend_state,
                live_state.inner(),
                transcription_state.clone(),
            ) {
                eprintln!("[mic-watchdog] switch start failed: {error}");
                if let Ok(mut backend) = backend_state.lock() {
                    backend.set_microphone_warning(format!(
                        "Microphone recovery failed: {error}. Try Refresh or change priority."
                    ));
                }
            }
        }
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

fn launch_auto_start_retry_delay_secs(settings: &AppSettings) -> &'static [u64] {
    if live_capture_should_start_on_launch(settings) {
        &LAUNCH_AUTO_START_RETRY_DELAY_SECS
    } else {
        &[]
    }
}

fn normalize_recent_transcript_limit(limit: Option<usize>) -> usize {
    limit
        .unwrap_or(DEFAULT_RECENT_TRANSCRIPT_LIMIT)
        .min(MAX_RECENT_TRANSCRIPT_LIMIT)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DockIconRuntimeMode {
    Visible,
    Hidden,
}

fn dock_icon_runtime_mode(settings: &AppSettings) -> DockIconRuntimeMode {
    if settings.show_dock_icon {
        DockIconRuntimeMode::Visible
    } else {
        DockIconRuntimeMode::Hidden
    }
}

fn settings_window_should_open_on_launch(settings: &AppSettings) -> bool {
    !settings.show_tray_icon
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppMenuAction {
    HideSettingsWindow,
    Noop,
}

fn app_menu_action(menu_id: &str) -> AppMenuAction {
    match menu_id {
        CLOSE_SETTINGS_WINDOW_MENU_ID => AppMenuAction::HideSettingsWindow,
        _ => AppMenuAction::Noop,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsWindowRequest {
    Show { show_dock_icon: bool },
}

fn settings_window_request_for_reopen(settings: Option<&AppSettings>) -> SettingsWindowRequest {
    SettingsWindowRequest::Show {
        show_dock_icon: settings
            .map(|settings| settings.show_dock_icon)
            .unwrap_or(true),
    }
}

fn settings_window_request_from_app(app: &AppHandle) -> SettingsWindowRequest {
    let settings = app
        .try_state::<BackendState>()
        .and_then(|state| state.lock().ok().map(|backend| backend.settings()));
    settings_window_request_for_reopen(settings.as_ref())
}

fn handle_app_menu_event(app: &AppHandle, menu_id: &str) {
    match app_menu_action(menu_id) {
        AppMenuAction::HideSettingsWindow => {
            if let Err(error) = hide_settings_window(app) {
                eprintln!("[window] failed to close settings window: {error}");
            }
        }
        AppMenuAction::Noop => {}
    }
}

fn hide_settings_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        if main_window_close_action(window.label()) == MainWindowCloseAction::HideToTray {
            window.hide()?;
        }
    }
    Ok(())
}

fn schedule_settings_window_for_reopen(app: &AppHandle, reason: &'static str) {
    let app_handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Err(error) = open_settings_window_for_reopen(&app_handle) {
            eprintln!("[window] failed to open settings window on {reason}: {error}");
        }
    }) {
        eprintln!("[window] failed to schedule settings window on {reason}: {error}");
    }
}

fn open_settings_window_for_reopen(app: &AppHandle) -> tauri::Result<()> {
    match settings_window_request_from_app(app) {
        SettingsWindowRequest::Show { show_dock_icon } => {
            ensure_main_window_visible(app, show_dock_icon)
        }
    }
}

fn apply_dock_icon_visibility(app: &AppHandle, show_dock_icon: bool) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mode = if show_dock_icon {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        app.set_activation_policy(mode)?;
        app.set_dock_visibility(show_dock_icon)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, show_dock_icon);
    }

    Ok(())
}

fn ensure_main_window_visible(app: &AppHandle, show_dock_icon: bool) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        apply_dock_icon_visibility(app, show_dock_icon)?;
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

fn build_app_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let pkg_info = app.package_info();
    let config = app.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config
            .bundle
            .publisher
            .clone()
            .map(|publisher| vec![publisher]),
        ..Default::default()
    };
    let close_window = MenuItem::with_id(
        app,
        CLOSE_SETTINGS_WINDOW_MENU_ID,
        "Close Window",
        true,
        Some("CmdOrCtrl+W"),
    )?;
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &close_window,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, None, Some(about_metadata))?,
        ],
    )?;

    Menu::with_items(
        app,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app, None, Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?,
            &file_menu,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(app, None)?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )
}

fn main() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            schedule_settings_window_for_reopen(app, "single-instance launch");
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    builder
        .menu(build_app_menu)
        .on_menu_event(|app, event| {
            handle_app_menu_event(app, event.id().as_ref());
        })
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
            let initial_settings_for_runtime = backend.settings();
            // Reconcile any meeting left "processing" by a previous run (its
            // worker thread died with the app) so the UI can offer Resume.
            wakenote::meeting::reconcile_interrupted(&expand_user_path(
                &initial_settings_for_runtime.save_root,
            ));
            let initial_dock_mode = dock_icon_runtime_mode(&initial_settings_for_runtime);
            let show_dock_icon = initial_dock_mode == DockIconRuntimeMode::Visible;
            ensure_main_window_visible(app.handle(), show_dock_icon)?;
            let backend_state = Arc::new(Mutex::new(backend));
            let transcription_state = Arc::new(AtomicBool::new(false));
            let live_transcriber_state: LiveTranscriberState = Arc::new(Mutex::new(None));
            let system_capture_state: SystemCaptureState = Arc::new(Mutex::new(None));
            let source_capture_lifecycle_state: SourceCaptureLifecycleState =
                Arc::new(Mutex::new(SourceCaptureLifecycle::Idle));
            let detected_source_state: DetectedSourceState = Arc::new(Mutex::new(None));
            let source_capture_pause_state: SourceCapturePauseState =
                Arc::new(Mutex::new(HashSet::new()));
            let meeting_state: MeetingState = Arc::new(Mutex::new(MeetingRuntime::default()));
            app.manage(backend_state.clone());
            app.manage(transcription_state.clone());
            app.manage(Mutex::new(LiveCaptureRuntime::new(CpalAudioInput)));
            app.manage(live_transcriber_state.clone());
            app.manage(system_capture_state.clone());
            app.manage(source_capture_lifecycle_state.clone());
            app.manage(detected_source_state.clone());
            app.manage(source_capture_pause_state.clone());
            app.manage(meeting_state.clone());

            spawn_source_watcher(
                app.handle().clone(),
                backend_state.clone(),
                system_capture_state,
                source_capture_lifecycle_state,
                detected_source_state,
                meeting_state,
                source_capture_pause_state,
                transcription_state.clone(),
            );

            wire_live_transcription(
                app.handle().clone(),
                backend_state.clone(),
                live_transcriber_state.clone(),
            );
            spawn_mic_recovery_watchdog(
                app.handle().clone(),
                backend_state.clone(),
                transcription_state.clone(),
            );
            if let Err(error) = overlay::create_overlay_window(app.handle()) {
                eprintln!("[overlay] initial hidden overlay creation failed: {error}");
            }
            let initial = backend_state
                .lock()
                .ok()
                .map(|backend| (backend.settings(), backend.app_status()));
            let (initial_settings, initial_status) = match initial {
                Some((settings, status)) => (Some(settings), Some(status)),
                None => (None, None),
            };
            app.manage(Mutex::new(
                initial_settings
                    .as_ref()
                    .zip(initial_status.as_ref())
                    .map(|(settings, status)| tray_presentation_snapshot(settings, status)),
            ));
            let tray_menu_items =
                setup_tray(app, initial_settings.as_ref(), initial_status.as_ref())?;
            if let Some(settings) = initial_settings.as_ref() {
                let custom_sources = settings
                    .custom_sources
                    .iter()
                    .map(|source| format!("{}:{}", source.id, source.label))
                    .collect::<Vec<_>>()
                    .join(",");
                append_runtime_debug_log(
                    settings,
                    format!(
                        "[app] launched version={} system_audio_enabled={} custom_sources=[{}]",
                        env!("CARGO_PKG_VERSION"),
                        settings.system_audio_enabled,
                        custom_sources
                    ),
                );
                let _ = apply_launch_at_login_preference(app.handle(), settings.launch_at_login);
            }
            app.manage(tray_menu_items);
            let app_handle_for_initial = app.handle().clone();
            let launch_auto_start_delays = initial_settings
                .as_ref()
                .map(launch_auto_start_retry_delay_secs)
                .unwrap_or(&[])
                .to_vec();
            if !launch_auto_start_delays.is_empty() {
                let app_handle_for_capture = app.handle().clone();
                let backend_state_for_capture = backend_state.clone();
                let transcription_state_for_capture = transcription_state.clone();
                thread::spawn(move || {
                    for delay_secs in launch_auto_start_delays {
                        thread::sleep(Duration::from_secs(delay_secs));
                        let should_start = backend_state_for_capture
                            .lock()
                            .map(|backend| {
                                live_capture_should_start_on_launch(&backend.settings())
                            })
                            .unwrap_or(false);
                        if !should_start {
                            eprintln!(
                                "[capture] launch auto-start stopped because settings no longer allow live input"
                            );
                            break;
                        }

                        let live_state = app_handle_for_capture.state::<LiveCaptureState>();
                        match start_live_capture_runtime(
                            &app_handle_for_capture,
                            &backend_state_for_capture,
                            live_state.inner(),
                            transcription_state_for_capture.clone(),
                        ) {
                            Ok(status) => {
                                let settings = backend_state_for_capture
                                    .lock()
                                    .map(|backend| backend.settings())
                                    .ok();
                                if let Some(settings) = settings {
                                    update_tray_presentation(
                                        &app_handle_for_capture,
                                        &settings,
                                        &status,
                                    );
                                }
                                if status.live_input_active {
                                    eprintln!("[capture] launch auto-start succeeded");
                                    break;
                                }
                                eprintln!(
                                    "[capture] launch auto-start did not activate input; retrying if attempts remain"
                                );
                            }
                            Err(error) => {
                                eprintln!(
                                    "[capture] launch auto-start attempt failed: {error}"
                                );
                            }
                        }
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
            permission_snapshot,
            request_microphone_permission,
            open_microphone_permission_settings,
            request_screen_recording_permission,
            open_screen_recording_settings,
            recent_transcripts,
            transcript_days,
            transcripts_for_day,
            rebuild_transcript_day_index,
            allow_audio_playback,
            upload_audio_file,
            analyze_audio_waveform,
            enqueue_audio_file,
            enqueue_backlog,
            retry_job,
            regenerate_transcript,
            open_transcript_folder,
            skip_job,
            cancel_current_transcription,
            cancel_current_operation,
            reveal_save_folder,
            process_next_transcription,
            start_live_capture,
            stop_live_capture,
            list_recognized_sources,
            source_capture_status,
            start_source_capture,
            stop_source_capture,
            list_meetings,
            import_and_start_meeting,
            meeting_detail,
            cancel_meeting,
            resume_meeting,
            delete_meeting
        ])
        .build(tauri::generate_context!())
        .expect("failed to build WakeNote")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            match event {
                tauri::RunEvent::Reopen { .. } => {
                    schedule_settings_window_for_reopen(app, "macOS reopen");
                }
                // whisper.cpp's GGML Metal backend aborts inside its
                // static destructor (`ggml_metal_rsets_free` -> `ggml_abort`)
                // when the process tears down via libc `exit()` ->
                // `__cxa_finalize`. The Metal device is already gone by then,
                // so the cleanup trips an internal assertion and SIGABRTs on
                // every quit that ran a transcription. Bypass the C++ static
                // destructors entirely with `_exit`: the kernel reclaims the
                // GPU resources anyway, and all durable state (settings,
                // transcription queue) is already persisted on write.
                tauri::RunEvent::Exit => {
                    unsafe { libc::_exit(0) };
                }
                _ => {}
            }

            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
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
    let separator_four = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &separator_one,
            &recording,
            &transcription,
            &active_model,
            &active_mic,
            &threshold,
            &separator_two,
            &reveal,
            &pause_all,
            &separator_three,
            &cancel,
            &separator_four,
            &quit,
        ],
    )?;

    let presentation = initial_settings
        .zip(initial_status)
        .map(|(settings, status)| tray_runtime_presentation(settings, status))
        .unwrap_or_else(|| wakenote::commands::TrayRuntimePresentation {
            icon: tray_presentation_for_state(wakenote::commands::TrayState::Listening),
            visible: true,
        });
    let icon_image = tray_icon_image_for_presentation(presentation.icon);
    let icon = Image::new_owned(icon_image.rgba, icon_image.width, icon_image.height);
    let tray = TrayIconBuilder::with_id("wakenote")
        .tooltip(presentation.icon.tooltip)
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .show_menu_on_left_click(
            initial_settings
                .map(tray_show_menu_on_left_click)
                .unwrap_or(false),
        )
        .on_menu_event(|app, event| {
            handle_tray_menu(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            handle_tray_icon_event(tray.app_handle(), event);
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

fn tray_presentation_update_payload(
    settings: &AppSettings,
    status: &AppStatus,
) -> TrayPresentationUpdate {
    TrayPresentationUpdate {
        settings: settings.clone(),
        status: status.clone(),
    }
}

fn tray_presentation_snapshot(
    settings: &AppSettings,
    status: &AppStatus,
) -> TrayPresentationSnapshot {
    TrayPresentationSnapshot {
        runtime: tray_runtime_presentation(settings, status),
        menu: tray_menu_presentation(settings, status),
        show_menu_on_left_click: tray_show_menu_on_left_click(settings),
    }
}

fn next_tray_presentation_update(
    cache: &mut Option<TrayPresentationSnapshot>,
    settings: &AppSettings,
    status: &AppStatus,
) -> Option<TrayPresentationUpdate> {
    let snapshot = tray_presentation_snapshot(settings, status);
    if cache.as_ref() == Some(&snapshot) {
        return None;
    }

    *cache = Some(snapshot);
    Some(tray_presentation_update_payload(settings, status))
}

fn update_tray_presentation(app: &tauri::AppHandle, settings: &AppSettings, status: &AppStatus) {
    let app_handle = app.clone();
    let payload = if let Some(cache) = app.try_state::<TrayPresentationCache>() {
        match cache.lock() {
            Ok(mut cache) => next_tray_presentation_update(&mut cache, settings, status),
            Err(_) => Some(tray_presentation_update_payload(settings, status)),
        }
    } else {
        Some(tray_presentation_update_payload(settings, status))
    };
    let Some(payload) = payload else {
        return;
    };
    let log_settings = payload.settings.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        apply_tray_presentation(&app_handle, &payload.settings, &payload.status);
    }) {
        append_runtime_debug_log(
            &log_settings,
            format!("[tray] failed to schedule presentation update: {error}"),
        );
        eprintln!("[tray] failed to schedule presentation update: {error}");
    }
}

fn apply_tray_presentation(app: &tauri::AppHandle, settings: &AppSettings, status: &AppStatus) {
    let Some(tray) = app.tray_by_id("wakenote") else {
        return;
    };
    let presentation = tray_runtime_presentation(settings, status);
    let icon_image = tray_icon_image_for_presentation(presentation.icon);
    let _ = tray.set_icon(Some(Image::new_owned(
        icon_image.rgba,
        icon_image.width,
        icon_image.height,
    )));
    let _ = tray.set_tooltip(Some(presentation.icon.tooltip));
    let _ = tray.set_visible(presentation.visible);
    let _ = tray.set_show_menu_on_left_click(tray_show_menu_on_left_click(settings));

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

#[cfg(test)]
fn tray_menu_order() -> &'static [&'static str] {
    TRAY_MENU_ORDER
}

fn tray_show_menu_on_left_click(settings: &AppSettings) -> bool {
    settings.tray_left_click_action == TrayClickAction::OpenMenu
}

fn tray_click_action_for_button(
    settings: &AppSettings,
    button: MouseButton,
) -> Option<TrayClickAction> {
    match button {
        MouseButton::Left => Some(settings.tray_left_click_action),
        MouseButton::Right => Some(settings.tray_right_click_action()),
        MouseButton::Middle => None,
    }
}

fn handle_tray_icon_event(app: &tauri::AppHandle, event: TrayIconEvent) {
    let TrayIconEvent::Click {
        button,
        button_state,
        ..
    } = event
    else {
        return;
    };
    if button_state != MouseButtonState::Up {
        return;
    }

    let action = app
        .try_state::<BackendState>()
        .and_then(|state| {
            state
                .lock()
                .ok()
                .and_then(|backend| tray_click_action_for_button(&backend.settings(), button))
        })
        .unwrap_or(TrayClickAction::OpenMenu);

    if action == TrayClickAction::TogglePause {
        patch_from_tray(app, |settings| SettingsPatch {
            pause_all: Some(!settings.pause_all),
            ..SettingsPatch::default()
        });
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
            let request = state
                .lock()
                .map(|backend| reveal_save_folder_request(&backend.settings()))
                .ok();
            if let Some(request) = request {
                let _ = Command::new(request.program).arg(request.path).spawn();
            }
        }
        "cancel-current-operation" => {
            let state = app.state::<BackendState>();
            let presentation = if let Ok(mut backend) = state.lock() {
                let _ = backend.cancel_current_operation();
                Some((backend.settings(), backend.app_status()))
            } else {
                None
            };
            if let Some((settings, status)) = presentation {
                update_tray_presentation(app, &settings, &status);
            }
        }
        "open-settings" => {
            let show_dock_icon = app
                .try_state::<BackendState>()
                .and_then(|state| {
                    state
                        .lock()
                        .ok()
                        .map(|backend| backend.settings().show_dock_icon)
                })
                .unwrap_or(true);
            if let Err(error) = ensure_main_window_visible(app, show_dock_icon) {
                eprintln!("[window] failed to open settings window: {error}");
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
    let (live_capture_action, handler, events) = if let Ok(mut backend) = state.lock() {
        let current = backend.settings();
        let patch = patch(current.clone());
        let live_capture_action = live_capture_runtime_action_for_patch(&current, &patch);
        backend.update_settings(patch);
        let (handler, events) = live_events_for_dispatch(&mut backend);
        (live_capture_action, handler, events)
    } else {
        return;
    };
    dispatch_live_events(handler, events);

    let _ = apply_live_capture_runtime_action(
        app,
        state.inner(),
        live_state.inner(),
        transcription_state.inner().clone(),
        live_capture_action,
    );
    let presentation = state
        .lock()
        .map(|backend| (backend.settings(), backend.app_status()))
        .ok();
    if let Some((settings, status)) = presentation {
        update_tray_presentation(app, &settings, &status);
    }
    kick_transcription_worker_if_needed(
        app.clone(),
        state.inner().clone(),
        transcription_state.inner().clone(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use wakenote::settings::SourceAutoPromptEntry;

    #[test]
    fn recognized_source_infos_use_default_auto_prompt() {
        let settings = AppSettings::default();
        let infos = recognized_source_infos(&settings);
        assert_eq!(infos.len(), 3);
        let meet = infos.iter().find(|i| i.id == "meet").expect("meet listed");
        let zoom = infos.iter().find(|i| i.id == "zoom").expect("zoom listed");
        let youtube = infos
            .iter()
            .find(|i| i.id == "youtube")
            .expect("youtube listed");
        assert_eq!(meet.label, "Google Meet");
        assert_eq!(
            meet.description,
            "Google Meet or browser tabs with Meet in the title"
        );
        assert!(meet.auto_prompt);
        assert_eq!(zoom.label, "Zoom");
        assert!(zoom.auto_prompt);
        assert_eq!(zoom.title_patterns, vec!["zoom", "zoom meeting"]);
        assert!(youtube.auto_prompt);
        assert_eq!(youtube.title_patterns, vec!["- youtube", "youtube"]);
        assert!(!youtube.custom);
    }

    #[test]
    fn recognized_source_infos_honor_user_override() {
        let mut settings = AppSettings::default();
        settings.source_auto_prompt = vec![SourceAutoPromptEntry {
            source_id: "youtube".into(),
            auto_prompt: true,
        }];
        let infos = recognized_source_infos(&settings);
        let youtube = infos
            .iter()
            .find(|i| i.id == "youtube")
            .expect("youtube listed");
        assert!(youtube.auto_prompt);
    }

    #[test]
    fn recognized_source_infos_include_custom_sources() {
        let mut settings = AppSettings::default();
        settings.custom_sources = vec![wakenote::settings::CustomSourceEntry {
            id: "spotify".into(),
            label: "Spotify".into(),
            title_patterns: vec!["Spotify".into()],
            auto_prompt: true,
        }];

        let infos = recognized_source_infos(&settings);
        let spotify = infos
            .iter()
            .find(|i| i.id == "spotify")
            .expect("spotify listed");

        assert_eq!(spotify.label, "Spotify");
        assert!(spotify.custom);
        assert_eq!(spotify.title_patterns, vec!["Spotify"]);
        assert!(spotify.auto_prompt);
    }

    #[test]
    fn recording_source_label_uses_custom_label_and_builtin_id() {
        let custom = DetectedSource {
            source_id: "custom-source-2".into(),
            label: "Spotify".into(),
            app_name: "Spotify".into(),
            pid: 42,
        };
        let builtin = DetectedSource {
            source_id: "youtube".into(),
            label: "YouTube".into(),
            app_name: "Google Chrome".into(),
            pid: 43,
        };

        assert_eq!(recording_source_label(&custom), "Spotify");
        assert_eq!(recording_source_label(&builtin), "youtube");
    }

    #[test]
    fn browser_sources_use_target_app_capture_scope() {
        let youtube = DetectedSource {
            source_id: "youtube".into(),
            label: "YouTube".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let meet = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 43,
        };
        let zoom = DetectedSource {
            source_id: "zoom".into(),
            label: "Zoom".into(),
            app_name: "zoom.us".into(),
            pid: 45,
        };
        let spotify = DetectedSource {
            source_id: "custom-source-2".into(),
            label: "Spotify".into(),
            app_name: "Spotify".into(),
            pid: 44,
        };

        assert_eq!(source_capture_scope(&youtube), "target-app");
        assert_eq!(source_capture_scope(&meet), "target-app");
        assert_eq!(source_capture_scope(&zoom), "target-app");
        assert_eq!(source_capture_scope(&spotify), "target-app");
    }

    #[test]
    fn system_capture_action_is_none_without_active_session() {
        let settings = AppSettings::default();
        assert_eq!(
            system_capture_settings_action(&settings, false),
            SystemCaptureSettingsAction::None
        );
    }

    #[test]
    fn system_capture_action_syncs_while_enabled_and_recording() {
        let mut settings = AppSettings::default();
        settings.system_audio_enabled = true;
        assert!(settings.recording_enabled);
        assert!(!settings.pause_all);
        assert_eq!(
            system_capture_settings_action(&settings, true),
            SystemCaptureSettingsAction::Sync
        );
    }

    #[test]
    fn system_capture_action_stops_when_feature_disabled() {
        let mut settings = AppSettings::default();
        settings.system_audio_enabled = false;
        assert_eq!(
            system_capture_settings_action(&settings, true),
            SystemCaptureSettingsAction::Stop
        );
    }

    #[test]
    fn system_capture_action_stops_when_recording_off_or_paused() {
        let mut recording_off = AppSettings::default();
        recording_off.recording_enabled = false;
        assert_eq!(
            system_capture_settings_action(&recording_off, true),
            SystemCaptureSettingsAction::Stop
        );

        let mut paused = AppSettings::default();
        paused.pause_all = true;
        assert_eq!(
            system_capture_settings_action(&paused, true),
            SystemCaptureSettingsAction::Stop
        );
    }

    #[test]
    fn source_watcher_enumerates_windows_only_after_screen_recording_grant() {
        let mut settings = AppSettings::default();
        settings.system_audio_enabled = true;

        assert!(!source_watcher_should_enumerate_windows(
            &settings,
            permissions::PermissionGrantStatus::NotDetermined
        ));
        assert!(!source_watcher_should_enumerate_windows(
            &settings,
            permissions::PermissionGrantStatus::Denied
        ));
        assert!(source_watcher_should_enumerate_windows(
            &settings,
            permissions::PermissionGrantStatus::Granted
        ));

        settings.system_audio_enabled = false;
        assert!(!source_watcher_should_enumerate_windows(
            &settings,
            permissions::PermissionGrantStatus::Granted
        ));
    }

    #[test]
    fn source_capture_start_decision_blocks_duplicate_starts() {
        let now = Instant::now();
        let starting = SourceCaptureLifecycle::Starting {
            source_id: "meet".into(),
            attempt: 1,
        };
        let running = SourceCaptureLifecycle::Running {
            source_id: "meet".into(),
        };

        assert_eq!(
            source_capture_start_decision(&starting, "meet", now),
            SourceCaptureStartDecision::AlreadyStarting
        );
        assert_eq!(
            source_capture_start_decision(&running, "meet", now),
            SourceCaptureStartDecision::AlreadyRunning
        );
    }

    #[test]
    fn source_capture_start_decision_backs_off_after_failed_start() {
        let now = Instant::now();
        let failed = SourceCaptureLifecycle::Failed {
            source_id: "meet".into(),
            failed_at: now,
            attempts: 1,
        };

        assert_eq!(
            source_capture_start_decision(&failed, "meet", now + Duration::from_secs(1)),
            SourceCaptureStartDecision::BackingOff {
                retry_after: Duration::from_secs(1),
            }
        );
        assert_eq!(
            source_capture_start_decision(&failed, "meet", now + Duration::from_secs(2)),
            SourceCaptureStartDecision::Start { attempt: 2 }
        );
    }

    #[test]
    fn source_capture_retry_delay_steps_then_caps() {
        assert_eq!(source_capture_retry_delay(1), Duration::from_secs(2));
        assert_eq!(source_capture_retry_delay(2), Duration::from_secs(5));
        assert_eq!(source_capture_retry_delay(3), Duration::from_secs(10));
        assert_eq!(source_capture_retry_delay(8), Duration::from_secs(10));
    }

    #[test]
    fn source_capture_lifecycle_preserves_failed_attempt_count() {
        let now = Instant::now();
        let mut lifecycle = SourceCaptureLifecycle::Idle;

        source_capture_mark_starting(&mut lifecycle, "meet", 1);
        assert_eq!(
            lifecycle,
            SourceCaptureLifecycle::Starting {
                source_id: "meet".into(),
                attempt: 1,
            }
        );

        source_capture_mark_failed(&mut lifecycle, "meet", now);
        assert_eq!(
            lifecycle,
            SourceCaptureLifecycle::Failed {
                source_id: "meet".into(),
                failed_at: now,
                attempts: 1,
            }
        );

        source_capture_mark_starting(&mut lifecycle, "meet", 2);
        source_capture_mark_failed(&mut lifecycle, "meet", now + Duration::from_secs(3));
        assert_eq!(
            source_capture_start_decision(&lifecycle, "meet", now + Duration::from_secs(7)),
            SourceCaptureStartDecision::BackingOff {
                retry_after: Duration::from_secs(1),
            }
        );
    }

    #[test]
    fn source_capture_failure_diagnostic_includes_permission_and_retry_context() {
        let message = source_capture_failure_diagnostic(
            "meet",
            2,
            permissions::PermissionGrantStatus::Granted,
            "system-audio capture failed: cpal error: timed out querying shareable content elapsed_ms=10000",
        );

        assert_eq!(
            message,
            "[source-capture] error source_id=meet attempt=2 screen_recording_status=Granted next_retry_ms=5000 error=system-audio capture failed: cpal error: timed out querying shareable content elapsed_ms=10000"
        );
    }

    struct FakeSystemAudioStream;

    impl AudioStreamHandle for FakeSystemAudioStream {}

    struct FakeSourceCaptureStarter {
        calls: usize,
        outcomes: std::collections::VecDeque<Result<(), LiveCaptureError>>,
    }

    impl FakeSourceCaptureStarter {
        fn new(outcomes: Vec<Result<(), LiveCaptureError>>) -> Self {
            Self {
                calls: 0,
                outcomes: outcomes.into(),
            }
        }
    }

    impl SourceCaptureStreamStarter for FakeSourceCaptureStarter {
        fn start_source_capture_stream(
            &mut self,
            source: &DetectedSource,
            sample_rate: u32,
            _on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync>,
        ) -> Result<Box<dyn AudioStreamHandle>, LiveCaptureError> {
            self.calls += 1;
            assert_eq!(source.source_id, "meet");
            assert_eq!(source.app_name, "Google Chrome");
            assert_eq!(sample_rate, PIPELINE_SAMPLE_RATE);
            match self.outcomes.pop_front().expect("fake outcome") {
                Ok(()) => Ok(Box::new(FakeSystemAudioStream)),
                Err(error) => Err(error),
            }
        }
    }

    #[test]
    fn source_capture_start_failure_backoff_retry_preserves_existing_queue() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let queued_audio = tmp.path().join("existing.wav");
        std::fs::write(&queued_audio, b"existing audio").expect("queued audio");

        let mut backend = AppBackend::default();
        backend.update_settings(SettingsPatch {
            save_root: Some(tmp.path().to_string_lossy().to_string()),
            audio_format: Some(wakenote::settings::AudioFormat::Wav),
            transcription_enabled: Some(true),
            ..SettingsPatch::default()
        });
        let queued_before = backend.enqueue_audio_file(&queued_audio, None);

        let backend_state: BackendState = Arc::new(Mutex::new(backend));
        let system_capture_state: SystemCaptureState = Arc::new(Mutex::new(None));
        let lifecycle_state: SourceCaptureLifecycleState =
            Arc::new(Mutex::new(SourceCaptureLifecycle::Idle));
        let source = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let on_frame: Arc<dyn Fn(AudioFrame) + Send + Sync> = Arc::new(|_| {});
        let now = Instant::now();
        let mut starter = FakeSourceCaptureStarter::new(vec![
            Err(LiveCaptureError::Cpal(
                "timed out starting system-audio capture elapsed_ms=10000".into(),
            )),
            Ok(()),
        ]);

        let error = attempt_source_capture_start(
            &source,
            &backend_state,
            &system_capture_state,
            &lifecycle_state,
            on_frame.clone(),
            &mut starter,
            now,
            permissions::PermissionGrantStatus::Granted,
        )
        .expect_err("first ScreenCaptureKit start fails");

        assert!(error.contains("system-audio capture failed"));
        assert_eq!(starter.calls, 1);
        assert!(system_capture_state.lock().expect("stream state").is_none());
        {
            let backend = backend_state.lock().expect("backend");
            assert!(!backend.is_system_capturing());
            assert_eq!(backend.queue_snapshot(), queued_before);
        }
        assert!(wakenote::meeting::list_meetings(tmp.path()).is_empty());
        assert!(matches!(
            &*lifecycle_state.lock().expect("lifecycle"),
            SourceCaptureLifecycle::Failed { source_id, attempts, .. }
                if source_id == "meet" && *attempts == 1
        ));

        let deferred = attempt_source_capture_start(
            &source,
            &backend_state,
            &system_capture_state,
            &lifecycle_state,
            on_frame.clone(),
            &mut starter,
            now + Duration::from_secs(1),
            permissions::PermissionGrantStatus::Granted,
        )
        .expect("retry inside backoff is deferred");

        assert_eq!(deferred.outcome, SourceCaptureAttemptOutcome::Deferred);
        assert_eq!(starter.calls, 1);
        assert!(system_capture_state.lock().expect("stream state").is_none());
        assert_eq!(
            backend_state.lock().expect("backend").queue_snapshot(),
            queued_before
        );

        let started = attempt_source_capture_start(
            &source,
            &backend_state,
            &system_capture_state,
            &lifecycle_state,
            on_frame,
            &mut starter,
            now + Duration::from_secs(2),
            permissions::PermissionGrantStatus::Granted,
        )
        .expect("retry after backoff starts capture");

        assert_eq!(started.outcome, SourceCaptureAttemptOutcome::Started);
        assert_eq!(starter.calls, 2);
        assert!(system_capture_state.lock().expect("stream state").is_some());
        {
            let backend = backend_state.lock().expect("backend");
            assert!(backend.is_system_capturing());
            assert_eq!(backend.queue_snapshot(), queued_before);
        }
        assert!(matches!(
            &*lifecycle_state.lock().expect("lifecycle"),
            SourceCaptureLifecycle::Running { source_id } if source_id == "meet"
        ));
    }

    #[test]
    fn finalized_meet_capture_ids_are_collected_once_for_auto_meeting_jobs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let model_dir = tmp.path().join("models");
        std::fs::create_dir_all(&model_dir).expect("model dir");
        std::fs::write(model_dir.join("whisper-medium.bin"), b"fake model").expect("model file");
        let mut backend = AppBackend::default();
        backend.update_settings(SettingsPatch {
            save_root: Some(tmp.path().to_string_lossy().to_string()),
            model_directory: Some(model_dir.to_string_lossy().to_string()),
            transcription_enabled: Some(true),
            ..SettingsPatch::default()
        });
        let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();

        backend
            .start_system_capture_session(
                10,
                base_time,
                "Google Chrome".into(),
                "meet".into(),
                "meet".into(),
            )
            .expect("start meet system capture");
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.5],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(100),
            })
            .expect("meet audio frame");
        backend
            .stop_system_capture_session()
            .expect("stop meet system capture");

        let meetings = wakenote::meeting::list_meetings(tmp.path());
        let actions = drain_finished_system_meeting_job_actions(&mut backend);

        assert_eq!(
            actions,
            vec![FinishedSystemMeetingJobAction::Start(
                meetings[0].id.clone()
            )]
        );
        assert!(drain_finished_system_meeting_job_actions(&mut backend).is_empty());
    }

    #[test]
    fn finalized_meet_capture_ids_wait_when_meeting_model_is_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut backend = AppBackend::default();
        backend.update_settings(SettingsPatch {
            save_root: Some(tmp.path().to_string_lossy().to_string()),
            model_directory: Some(tmp.path().join("models").to_string_lossy().to_string()),
            transcription_enabled: Some(true),
            ..SettingsPatch::default()
        });
        let base_time = chrono::Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();

        backend
            .start_system_capture_session(
                10,
                base_time,
                "Google Chrome".into(),
                "meet".into(),
                "meet".into(),
            )
            .expect("start meet system capture");
        backend
            .process_system_audio_frame(AudioFrame {
                samples: vec![0.5],
                duration_ms: 100,
                captured_at: base_time + chrono::Duration::milliseconds(100),
            })
            .expect("meet audio frame");
        backend
            .stop_system_capture_session()
            .expect("stop meet system capture");

        let meetings = wakenote::meeting::list_meetings(tmp.path());
        let actions = drain_finished_system_meeting_job_actions(&mut backend);

        assert_eq!(
            actions,
            vec![FinishedSystemMeetingJobAction::Pending {
                id: meetings[0].id.clone(),
                reason: "model_not_ready",
            }]
        );
        assert_eq!(meetings.len(), 1);
        assert_eq!(
            meetings[0].status,
            wakenote::meeting::MeetingStatus::Pending
        );
    }

    #[test]
    fn tray_presentation_update_payload_is_send_static_for_main_thread_dispatch() {
        fn assert_send_static<T: Send + 'static>(_: &T) {}

        let tmp = tempfile::tempdir().expect("tempdir");
        let backend = AppBackend::load_from_dir(tmp.path()).expect("backend");
        let settings = backend.settings();
        let status = backend.app_status();
        let payload = tray_presentation_update_payload(&settings, &status);

        assert_send_static(&payload);
        assert_eq!(payload.settings, settings);
        assert_eq!(payload.status, status);
    }

    #[test]
    fn unchanged_tray_presentation_is_not_dispatched_again() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let backend = AppBackend::load_from_dir(tmp.path()).expect("backend");
        let settings = backend.settings();
        let status = backend.app_status();
        let mut cache = None;

        assert!(next_tray_presentation_update(&mut cache, &settings, &status).is_some());
        assert!(next_tray_presentation_update(&mut cache, &settings, &status).is_none());
    }

    #[test]
    fn tray_menu_order_starts_with_open_settings_then_separator() {
        assert_eq!(
            tray_menu_order(),
            &[
                "open-settings",
                "separator",
                "toggle-recording",
                "toggle-transcription",
                "active-model",
                "active-microphone",
                "threshold",
                "separator",
                "reveal-save-folder",
                "pause-all",
                "separator",
                "cancel-current-operation",
                "separator",
                "quit",
            ]
        );
    }

    #[test]
    fn tray_left_click_menu_flag_follows_click_action_setting() {
        let mut settings = AppSettings::default();
        assert!(tray_show_menu_on_left_click(&settings));

        settings.tray_left_click_action = wakenote::settings::TrayClickAction::TogglePause;
        assert!(!tray_show_menu_on_left_click(&settings));
    }

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
    fn main_window_default_size_matches_current_settings_window() {
        assert_eq!(MAIN_WINDOW_WIDTH as u32, 1040);
        assert_eq!(MAIN_WINDOW_HEIGHT as u32, 922);
    }

    #[test]
    fn dock_icon_runtime_mode_follows_setting() {
        let mut settings = AppSettings::default();
        assert_eq!(
            dock_icon_runtime_mode(&settings),
            DockIconRuntimeMode::Visible
        );

        settings.show_dock_icon = false;
        assert_eq!(
            dock_icon_runtime_mode(&settings),
            DockIconRuntimeMode::Hidden
        );
    }

    #[test]
    fn hidden_menu_bar_icon_requires_settings_window_on_launch() {
        let mut settings = AppSettings::default();
        assert!(!settings_window_should_open_on_launch(&settings));

        settings.show_tray_icon = false;
        assert!(settings_window_should_open_on_launch(&settings));
    }

    #[test]
    fn close_window_shortcut_hides_settings_window() {
        assert_eq!(
            app_menu_action(CLOSE_SETTINGS_WINDOW_MENU_ID),
            AppMenuAction::HideSettingsWindow
        );
        assert_eq!(app_menu_action("open-settings"), AppMenuAction::Noop);
    }

    #[test]
    fn reopen_request_always_shows_settings_window() {
        let mut settings = AppSettings::default();
        assert_eq!(
            settings_window_request_for_reopen(Some(&settings)),
            SettingsWindowRequest::Show {
                show_dock_icon: true
            }
        );

        settings.show_dock_icon = false;
        assert_eq!(
            settings_window_request_for_reopen(Some(&settings)),
            SettingsWindowRequest::Show {
                show_dock_icon: false
            }
        );
        assert_eq!(
            settings_window_request_for_reopen(None),
            SettingsWindowRequest::Show {
                show_dock_icon: true
            }
        );
    }

    #[test]
    fn launch_auto_start_uses_retry_delays_only_when_enabled() {
        let settings = AppSettings::default();

        assert_eq!(
            launch_auto_start_retry_delay_secs(&settings),
            &[2, 5, 10, 20, 30, 60]
        );

        assert!(
            launch_auto_start_retry_delay_secs(&AppSettings {
                start_live_input_on_launch: false,
                ..AppSettings::default()
            })
            .is_empty()
        );
        assert!(
            launch_auto_start_retry_delay_secs(&AppSettings {
                recording_enabled: false,
                ..AppSettings::default()
            })
            .is_empty()
        );
        assert!(
            launch_auto_start_retry_delay_secs(&AppSettings {
                pause_all: true,
                ..AppSettings::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn recent_transcript_limit_defaults_and_clamps_for_runtime_command() {
        assert_eq!(
            normalize_recent_transcript_limit(None),
            DEFAULT_RECENT_TRANSCRIPT_LIMIT
        );
        assert_eq!(normalize_recent_transcript_limit(Some(0)), 0);
        assert_eq!(normalize_recent_transcript_limit(Some(12)), 12);
        assert_eq!(
            normalize_recent_transcript_limit(Some(MAX_RECENT_TRANSCRIPT_LIMIT + 1)),
            MAX_RECENT_TRANSCRIPT_LIMIT
        );
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
