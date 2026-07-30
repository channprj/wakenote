use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

use crate::commands::TrayState;
use crate::settings::{DictationBubblePosition, FloatingOverlayPosition};

pub const OVERLAY_LABEL: &str = "overlay";
pub const OVERLAY_EVENT: &str = "overlay-state";
pub const OVERLAY_LEVEL_EVENT: &str = "overlay-level";
pub const OVERLAY_WAVEFORM_BAR_COUNT: usize = 11;
pub const DICTATION_OVERLAY_EVENT: &str = "dictation-overlay-state";
pub const DICTATION_OVERLAY_LEVEL_EVENT: &str = "dictation-overlay-level";

const OVERLAY_MAX_WIDTH_LOGICAL: f64 = 720.0;
const OVERLAY_MIN_WIDTH_LOGICAL: f64 = 280.0;
const OVERLAY_HEIGHT_LOGICAL: f64 = 104.0;
const DICTATION_OVERLAY_WIDTH_LOGICAL: f64 = 248.0;
const DICTATION_OVERLAY_HEIGHT_LOGICAL: f64 = 76.0;
const DICTATION_OVERLAY_HORIZONTAL_OFFSET_LOGICAL: f64 = 18.0;
const DICTATION_OVERLAY_TOP_OFFSET_LOGICAL: f64 = 32.0;
const DICTATION_OVERLAY_BOTTOM_OFFSET_LOGICAL: f64 = 18.0;
const OVERLAY_SCREEN_MARGIN_LOGICAL: f64 = 24.0;
const OVERLAY_CAPTION_HORIZONTAL_PADDING_LOGICAL: f64 = 36.0;
const OVERLAY_CAPTION_VERTICAL_PADDING_LOGICAL: f64 = 28.0;
const OVERLAY_CAPTION_BORDER_LOGICAL: f64 = 2.0;
const OVERLAY_CAPTION_VERTICAL_WINDOW_INSET_LOGICAL: f64 = 16.0;
const OVERLAY_CAPTION_LINE_HEIGHT_RATIO: f64 = 1.25;

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(WakeNoteOverlayPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true,
            hides_on_deactivate: false
        }
    })
}

#[cfg(target_os = "macos")]
const TOP_OFFSET_LOGICAL: f64 = 46.0;
#[cfg(not(target_os = "macos"))]
const TOP_OFFSET_LOGICAL: f64 = 8.0;

#[cfg(target_os = "macos")]
const BOTTOM_OFFSET_LOGICAL: f64 = 46.0;
#[cfg(not(target_os = "macos"))]
const BOTTOM_OFFSET_LOGICAL: f64 = 8.0;

#[cfg(target_os = "macos")]
fn overlay_collection_behavior() -> tauri_nspanel::CollectionBehavior {
    tauri_nspanel::CollectionBehavior::new()
        .move_to_active_space()
        .full_screen_auxiliary()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayState {
    Hidden,
    Recording,
    Transcribing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DictationOverlayState {
    Hidden,
    Recording,
    Transcribing,
    Error,
}

pub fn overlay_state_for_tray_state(tray_state: TrayState) -> OverlayState {
    match tray_state {
        TrayState::Recording => OverlayState::Recording,
        _ => OverlayState::Hidden,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayAnchor {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorRect {
    pub origin_physical: (i32, i32),
    pub size_physical: (u32, u32),
    pub scale_factor: f64,
}

#[derive(Serialize, Clone)]
struct OverlayStatePayload {
    state: OverlayState,
    position: FloatingOverlayPosition,
}

#[derive(Serialize, Clone)]
struct OverlayLevelPayload {
    levels: Vec<f32>,
}

#[derive(Serialize, Clone)]
struct DictationOverlayPayload {
    state: DictationOverlayState,
    message: Option<String>,
}

#[cfg(target_os = "macos")]
pub fn create_overlay_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(OVERLAY_LABEL).is_some() {
        return Ok(());
    }

    use tauri::{Size, WebviewUrl};
    use tauri_nspanel::{PanelBuilder, PanelLevel};

    let panel = PanelBuilder::<_, WakeNoteOverlayPanel>::new(app, OVERLAY_LABEL)
        .url(WebviewUrl::App("overlay.html".into()))
        .title("WakeNote Overlay")
        .level(PanelLevel::Status)
        .size(Size::Logical(LogicalSize::new(
            OVERLAY_MAX_WIDTH_LOGICAL,
            OVERLAY_HEIGHT_LOGICAL,
        )))
        .has_shadow(false)
        .transparent(true)
        .no_activate(true)
        .corner_radius(0.0)
        .collection_behavior(overlay_collection_behavior())
        .ignores_mouse_events(true)
        .with_window(|window| {
            window
                .resizable(false)
                .decorations(false)
                .visible(false)
                .focused(false)
                .accept_first_mouse(true)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .maximizable(false)
                .minimizable(false)
                .closable(false)
        })
        .build()?;

    panel.hide();

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn create_overlay_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(OVERLAY_LABEL).is_some() {
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        tauri::WebviewUrl::App("overlay.html".into()),
    )
    .title("WakeNote Overlay")
    .inner_size(OVERLAY_MAX_WIDTH_LOGICAL, OVERLAY_HEIGHT_LOGICAL)
    .resizable(false)
    .decorations(false)
    .visible(false)
    .focused(false)
    .accept_first_mouse(true)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .build()
    .and_then(|window| {
        window.set_ignore_cursor_events(true)?;
        Ok(window)
    })?;

    Ok(())
}

pub fn show_overlay(
    app: &AppHandle,
    state: OverlayState,
    position: FloatingOverlayPosition,
) -> tauri::Result<()> {
    if matches!(position, FloatingOverlayPosition::Off) || matches!(state, OverlayState::Hidden) {
        return hide_overlay(app);
    }

    if app.get_webview_window(OVERLAY_LABEL).is_none() {
        create_overlay_window(app)?;
    }
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return Ok(());
    };

    if let Some(rect) = monitor_with_cursor(&window) {
        let anchor = match position {
            FloatingOverlayPosition::Top => OverlayAnchor::Top,
            FloatingOverlayPosition::Bottom => OverlayAnchor::Bottom,
            FloatingOverlayPosition::Off => return hide_overlay(app),
        };
        let overlay_size = overlay_size_for_monitor(rect);
        let logical = calculate_position(rect, anchor, overlay_size);
        window.set_size(LogicalSize::new(overlay_size.0, overlay_size.1))?;
        window.set_position(logical)?;
    }

    window.show()?;
    let payload = OverlayStatePayload { state, position };
    let _ = window.emit(OVERLAY_EVENT, payload.clone());
    let _ = app.emit(OVERLAY_EVENT, payload);
    Ok(())
}

pub fn show_caption_overlay(
    app: &AppHandle,
    position: FloatingOverlayPosition,
    text: &str,
    font_size_px: u32,
) -> tauri::Result<()> {
    if matches!(position, FloatingOverlayPosition::Off) || text.trim().is_empty() {
        return hide_overlay(app);
    }

    if app.get_webview_window(OVERLAY_LABEL).is_none() {
        create_overlay_window(app)?;
    }
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return Ok(());
    };

    if let Some(rect) = monitor_with_cursor(&window) {
        let anchor = match position {
            FloatingOverlayPosition::Top => OverlayAnchor::Top,
            FloatingOverlayPosition::Bottom => OverlayAnchor::Bottom,
            FloatingOverlayPosition::Off => return hide_overlay(app),
        };
        let overlay_size = caption_overlay_size_for_monitor(rect, text, font_size_px);
        let logical = calculate_position(rect, anchor, overlay_size);
        window.set_size(LogicalSize::new(overlay_size.0, overlay_size.1))?;
        window.set_position(logical)?;
    }

    window.show()?;
    let payload = OverlayStatePayload {
        state: OverlayState::Recording,
        position,
    };
    let _ = window.emit(OVERLAY_EVENT, payload.clone());
    let _ = app.emit(OVERLAY_EVENT, payload);
    Ok(())
}

pub fn emit_waveform_levels(app: &AppHandle, levels: Vec<f32>) {
    let payload = OverlayLevelPayload { levels };
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.emit(OVERLAY_LEVEL_EVENT, payload.clone());
    }
    let _ = app.emit(OVERLAY_LEVEL_EVENT, payload);
}

pub fn show_dictation_overlay(
    app: &AppHandle,
    state: DictationOverlayState,
    message: Option<String>,
    position: DictationBubblePosition,
) -> tauri::Result<()> {
    if state == DictationOverlayState::Hidden {
        return hide_dictation_overlay_state(app);
    }
    if app.get_webview_window(OVERLAY_LABEL).is_none() {
        create_overlay_window(app)?;
    }
    let Some(window) = app.get_webview_window(OVERLAY_LABEL) else {
        return Ok(());
    };

    if let Some(rect) = monitor_with_cursor(&window) {
        let overlay_size = dictation_overlay_size_for_monitor(rect);
        let logical = calculate_dictation_position(rect, position, overlay_size);
        window.set_size(LogicalSize::new(overlay_size.0, overlay_size.1))?;
        window.set_position(logical)?;
    }

    window.show()?;
    let payload = DictationOverlayPayload { state, message };
    let _ = window.emit(DICTATION_OVERLAY_EVENT, payload.clone());
    let _ = app.emit(DICTATION_OVERLAY_EVENT, payload);
    Ok(())
}

pub fn emit_dictation_waveform_levels(app: &AppHandle, levels: Vec<f32>) {
    let payload = OverlayLevelPayload { levels };
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.emit(DICTATION_OVERLAY_LEVEL_EVENT, payload.clone());
    }
    let _ = app.emit(DICTATION_OVERLAY_LEVEL_EVENT, payload);
}

pub fn hide_dictation_overlay_state(app: &AppHandle) -> tauri::Result<()> {
    let payload = DictationOverlayPayload {
        state: DictationOverlayState::Hidden,
        message: None,
    };
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.emit(DICTATION_OVERLAY_EVENT, payload.clone());
    }
    let _ = app.emit(DICTATION_OVERLAY_EVENT, payload);
    Ok(())
}

pub fn show_overlay_on_main_thread(
    app: &AppHandle,
    state: OverlayState,
    position: FloatingOverlayPosition,
    context: &'static str,
) -> tauri::Result<()> {
    let app_for_task = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = show_overlay(&app_for_task, state, position) {
            eprintln!("[overlay] {context} failed: {error}");
        }
    })
}

pub fn hide_overlay(app: &AppHandle) -> tauri::Result<()> {
    let payload = OverlayStatePayload {
        state: OverlayState::Hidden,
        position: FloatingOverlayPosition::Off,
    };
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.emit(OVERLAY_EVENT, payload.clone());
        window.hide()?;
    }
    let _ = app.emit(OVERLAY_EVENT, payload);
    Ok(())
}

pub fn hide_overlay_on_main_thread(app: &AppHandle, context: &'static str) -> tauri::Result<()> {
    let app_for_task = app.clone();
    app.run_on_main_thread(move || {
        if let Err(error) = hide_overlay(&app_for_task) {
            eprintln!("[overlay] {context} failed: {error}");
        }
    })
}

fn monitor_with_cursor<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Option<MonitorRect> {
    let monitors = window.available_monitors().ok()?;
    let cursor = window.cursor_position().ok();

    let chosen = cursor.and_then(|pos| {
        monitors.iter().find(|monitor| {
            let origin = monitor.position();
            let size = monitor.size();
            let x = pos.x;
            let y = pos.y;
            x >= origin.x as f64
                && x < (origin.x as f64 + size.width as f64)
                && y >= origin.y as f64
                && y < (origin.y as f64 + size.height as f64)
        })
    });

    let monitor = chosen
        .cloned()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| monitors.into_iter().next())?;

    let position = monitor.position();
    let size = monitor.size();
    Some(MonitorRect {
        origin_physical: (position.x, position.y),
        size_physical: (size.width, size.height),
        scale_factor: monitor.scale_factor(),
    })
}

pub(crate) fn calculate_position(
    monitor: MonitorRect,
    anchor: OverlayAnchor,
    size_logical: (f64, f64),
) -> LogicalPosition<f64> {
    let (overlay_w, overlay_h) = size_logical;
    let scale = if monitor.scale_factor > 0.0 {
        monitor.scale_factor
    } else {
        1.0
    };

    let monitor_origin_logical_x = monitor.origin_physical.0 as f64 / scale;
    let monitor_origin_logical_y = monitor.origin_physical.1 as f64 / scale;
    let monitor_logical_w = monitor.size_physical.0 as f64 / scale;
    let monitor_logical_h = monitor.size_physical.1 as f64 / scale;

    let logical_x = monitor_origin_logical_x + (monitor_logical_w - overlay_w) / 2.0;
    let logical_y = match anchor {
        OverlayAnchor::Top => monitor_origin_logical_y + TOP_OFFSET_LOGICAL,
        OverlayAnchor::Bottom => {
            monitor_origin_logical_y + monitor_logical_h - overlay_h - BOTTOM_OFFSET_LOGICAL
        }
    };

    LogicalPosition::new(logical_x, logical_y)
}

pub(crate) fn calculate_dictation_position(
    monitor: MonitorRect,
    position: DictationBubblePosition,
    size_logical: (f64, f64),
) -> LogicalPosition<f64> {
    let (overlay_w, overlay_h) = size_logical;
    let scale = if monitor.scale_factor > 0.0 {
        monitor.scale_factor
    } else {
        1.0
    };
    let origin_x = monitor.origin_physical.0 as f64 / scale;
    let origin_y = monitor.origin_physical.1 as f64 / scale;
    let monitor_w = monitor.size_physical.0 as f64 / scale;
    let monitor_h = monitor.size_physical.1 as f64 / scale;

    let x = match position {
        DictationBubblePosition::TopLeft | DictationBubblePosition::BottomLeft => {
            origin_x + DICTATION_OVERLAY_HORIZONTAL_OFFSET_LOGICAL
        }
        DictationBubblePosition::TopCenter | DictationBubblePosition::BottomCenter => {
            origin_x + (monitor_w - overlay_w) / 2.0
        }
        DictationBubblePosition::TopRight | DictationBubblePosition::BottomRight => {
            origin_x + monitor_w - overlay_w - DICTATION_OVERLAY_HORIZONTAL_OFFSET_LOGICAL
        }
    };
    let y = match position {
        DictationBubblePosition::TopLeft
        | DictationBubblePosition::TopCenter
        | DictationBubblePosition::TopRight => origin_y + DICTATION_OVERLAY_TOP_OFFSET_LOGICAL,
        DictationBubblePosition::BottomLeft
        | DictationBubblePosition::BottomCenter
        | DictationBubblePosition::BottomRight => {
            origin_y + monitor_h - overlay_h - DICTATION_OVERLAY_BOTTOM_OFFSET_LOGICAL
        }
    };

    LogicalPosition::new(x, y)
}

fn overlay_size_for_monitor(monitor: MonitorRect) -> (f64, f64) {
    let scale = if monitor.scale_factor > 0.0 {
        monitor.scale_factor
    } else {
        1.0
    };
    let monitor_logical_w = monitor.size_physical.0 as f64 / scale;
    let available_w = (monitor_logical_w - OVERLAY_SCREEN_MARGIN_LOGICAL * 2.0).max(1.0);
    let width = available_w
        .min(OVERLAY_MAX_WIDTH_LOGICAL)
        .max(OVERLAY_MIN_WIDTH_LOGICAL.min(available_w));
    (width, OVERLAY_HEIGHT_LOGICAL)
}

fn dictation_overlay_size_for_monitor(_monitor: MonitorRect) -> (f64, f64) {
    (
        DICTATION_OVERLAY_WIDTH_LOGICAL,
        DICTATION_OVERLAY_HEIGHT_LOGICAL,
    )
}

pub(crate) fn caption_overlay_size_for_monitor(
    monitor: MonitorRect,
    text: &str,
    font_size_px: u32,
) -> (f64, f64) {
    let (width, _) = overlay_size_for_monitor(monitor);
    let scale = if monitor.scale_factor > 0.0 {
        monitor.scale_factor
    } else {
        1.0
    };
    let monitor_logical_h = monitor.size_physical.1 as f64 / scale;
    let font_size = (font_size_px as f64).clamp(18.0, 48.0);
    let text_width = (width - OVERLAY_CAPTION_HORIZONTAL_PADDING_LOGICAL).max(font_size * 8.0);
    let average_char_width = font_size * 0.56;
    let chars_per_line = (text_width / average_char_width).floor().max(8.0);
    let weighted_chars = text
        .chars()
        .map(|ch| if ch.is_ascii() { 0.58 } else { 1.0 })
        .sum::<f64>()
        .max(1.0);
    let lines = (weighted_chars / chars_per_line).ceil().max(1.0);
    let text_height = lines * font_size * OVERLAY_CAPTION_LINE_HEIGHT_RATIO;
    let max_height =
        (monitor_logical_h - OVERLAY_SCREEN_MARGIN_LOGICAL * 2.0).max(OVERLAY_HEIGHT_LOGICAL);
    let height = (text_height
        + OVERLAY_CAPTION_VERTICAL_PADDING_LOGICAL
        + OVERLAY_CAPTION_BORDER_LOGICAL
        + OVERLAY_CAPTION_VERTICAL_WINDOW_INSET_LOGICAL)
        .ceil()
        .max(OVERLAY_HEIGHT_LOGICAL)
        .min(max_height);
    (width, height)
}

pub fn waveform_levels_from_samples(samples: &[f32], count: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0.0; count];
    }

    (0..count)
        .map(|index| {
            let start = index * samples.len() / count;
            let end = ((index + 1) * samples.len() / count).min(samples.len());
            if start >= end {
                return 0.0;
            }

            let mut sum_squares = 0.0_f32;
            let mut valid_count = 0usize;
            for sample in &samples[start..end] {
                if sample.is_finite() {
                    let clamped = sample.clamp(-1.0, 1.0);
                    sum_squares += clamped * clamped;
                    valid_count += 1;
                }
            }
            if valid_count == 0 || sum_squares <= f32::EPSILON {
                return 0.0;
            }

            let rms = (sum_squares / valid_count as f32).sqrt().max(0.000_001);
            let dbfs = 20.0 * rms.log10();
            ((dbfs + 60.0) / 60.0).clamp(0.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(origin: (i32, i32), size: (u32, u32), scale: f64) -> MonitorRect {
        MonitorRect {
            origin_physical: origin,
            size_physical: size,
            scale_factor: scale,
        }
    }

    #[test]
    fn top_anchor_centers_horizontally_and_offsets_vertically() {
        let monitor = rect((0, 0), (1920, 1080), 1.0);
        let pos = calculate_position(
            monitor,
            OverlayAnchor::Top,
            overlay_size_for_monitor(monitor),
        );
        assert_eq!(pos.x, ((1920.0 - OVERLAY_MAX_WIDTH_LOGICAL) / 2.0));
        assert_eq!(pos.y, TOP_OFFSET_LOGICAL);
    }

    #[test]
    fn bottom_anchor_mirrors_top_offset() {
        let monitor = rect((0, 0), (1920, 1080), 1.0);
        let pos = calculate_position(
            monitor,
            OverlayAnchor::Bottom,
            overlay_size_for_monitor(monitor),
        );
        assert_eq!(pos.x, ((1920.0 - OVERLAY_MAX_WIDTH_LOGICAL) / 2.0));
        assert_eq!(
            pos.y,
            1080.0 - OVERLAY_HEIGHT_LOGICAL - BOTTOM_OFFSET_LOGICAL
        );
    }

    #[test]
    fn secondary_monitor_with_negative_origin_produces_absolute_coords() {
        let monitor = rect((-1920, 0), (1920, 1080), 1.0);
        let pos = calculate_position(
            monitor,
            OverlayAnchor::Top,
            overlay_size_for_monitor(monitor),
        );
        assert_eq!(
            pos.x,
            -1920.0 + ((1920.0 - OVERLAY_MAX_WIDTH_LOGICAL) / 2.0)
        );
        assert_eq!(pos.y, TOP_OFFSET_LOGICAL);
    }

    #[test]
    fn high_dpi_monitor_returns_logical_coords() {
        let monitor = rect((0, 0), (3840, 2160), 2.0);
        let pos = calculate_position(
            monitor,
            OverlayAnchor::Top,
            overlay_size_for_monitor(monitor),
        );
        // Logical width = 1920, logical x = (1920 - 720) / 2 = 600.
        assert_eq!(pos.x, 600.0);
        assert_eq!(pos.y, TOP_OFFSET_LOGICAL);
    }

    #[test]
    fn overlay_size_shrinks_to_current_monitor_width() {
        let monitor = rect((0, 0), (640, 480), 1.0);
        let size = overlay_size_for_monitor(monitor);

        assert!(size.0 <= 592.0);
        assert_eq!(size.1, OVERLAY_HEIGHT_LOGICAL);
    }

    #[test]
    fn dictation_overlay_is_compact_at_each_configured_edge() {
        let monitor = rect((0, 0), (1920, 1080), 1.0);
        let size = dictation_overlay_size_for_monitor(monitor);
        assert_eq!(size, (248.0, 76.0));

        for (position, expected) in [
            (DictationBubblePosition::TopLeft, (18.0, 32.0)),
            (DictationBubblePosition::TopCenter, (836.0, 32.0)),
            (DictationBubblePosition::TopRight, (1654.0, 32.0)),
            (DictationBubblePosition::BottomLeft, (18.0, 986.0)),
            (DictationBubblePosition::BottomCenter, (836.0, 986.0)),
            (DictationBubblePosition::BottomRight, (1654.0, 986.0)),
        ] {
            let actual = calculate_dictation_position(monitor, position, size);
            assert_eq!((actual.x, actual.y), expected);
        }
    }

    #[test]
    fn dictation_overlay_position_uses_logical_secondary_monitor_coordinates() {
        let monitor = rect((-3840, 0), (3840, 2160), 2.0);
        let size = dictation_overlay_size_for_monitor(monitor);
        let position =
            calculate_dictation_position(monitor, DictationBubblePosition::TopRight, size);

        assert_eq!(position.x, -266.0);
        assert_eq!(position.y, 32.0);
    }

    #[test]
    fn caption_overlay_height_grows_for_long_text() {
        let monitor = rect((0, 0), (1920, 1080), 1.0);
        let short = caption_overlay_size_for_monitor(monitor, "짧은 자막", 24);
        let long = caption_overlay_size_for_monitor(
            monitor,
            "긴 자막은 두 줄에서 잘리면 안 됩니다. 실시간 전사 문장이 길어져도 전체 텍스트가 보이도록 오버레이 높이를 텍스트 길이에 맞춰 확장해야 합니다. 사용자가 회의 중 빠르게 말하면 partial transcript가 길어질 수 있으므로 말줄임표 없이 모두 표시해야 합니다.",
            24,
        );

        assert_eq!(short.1, OVERLAY_HEIGHT_LOGICAL);
        assert!(long.1 > short.1);
        assert!(long.1 <= 1080.0 - OVERLAY_SCREEN_MARGIN_LOGICAL * 2.0);
    }

    #[test]
    fn caption_overlay_height_keeps_breathing_room_for_four_lines() {
        let monitor = rect((0, 0), (1920, 1080), 1.0);
        let four_line_caption = "a".repeat(280);
        let size = caption_overlay_size_for_monitor(monitor, &four_line_caption, 24);

        assert!(size.1 >= 166.0, "caption height was {}", size.1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_overlay_moves_to_active_space_instead_of_joining_all_spaces() {
        use tauri_nspanel::CollectionBehavior;

        assert_eq!(
            overlay_collection_behavior(),
            CollectionBehavior::new()
                .move_to_active_space()
                .full_screen_auxiliary()
        );
        assert_ne!(
            overlay_collection_behavior(),
            CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
                .stationary()
        );
    }

    #[test]
    fn overlay_stays_hidden_until_audio_is_actively_recording() {
        use crate::commands::TrayState;

        assert_eq!(
            overlay_state_for_tray_state(TrayState::Listening),
            OverlayState::Hidden
        );
        assert_eq!(
            overlay_state_for_tray_state(TrayState::Recording),
            OverlayState::Recording
        );
        assert_eq!(
            overlay_state_for_tray_state(TrayState::Transcribing),
            OverlayState::Hidden
        );
    }

    #[test]
    fn waveform_levels_are_zero_for_empty_or_silent_samples() {
        assert_eq!(waveform_levels_from_samples(&[], 3), vec![0.0, 0.0, 0.0]);
        assert_eq!(
            waveform_levels_from_samples(&[0.0; 30], 3),
            vec![0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn waveform_levels_are_bounded_and_follow_bucket_energy() {
        let samples = (0..90).map(|index| index as f32 / 89.0).collect::<Vec<_>>();
        let levels = waveform_levels_from_samples(&samples, OVERLAY_WAVEFORM_BAR_COUNT);

        assert_eq!(levels.len(), OVERLAY_WAVEFORM_BAR_COUNT);
        assert!(levels.iter().all(|level| (0.0..=1.0).contains(level)));
        assert!(levels[0] < levels[OVERLAY_WAVEFORM_BAR_COUNT - 1]);
    }

    #[test]
    fn waveform_levels_handle_full_scale_and_non_finite_samples() {
        let full_scale = waveform_levels_from_samples(&[1.0; 90], OVERLAY_WAVEFORM_BAR_COUNT);
        assert!(
            full_scale
                .iter()
                .all(|level| (*level - 1.0).abs() < f32::EPSILON)
        );

        let invalid = waveform_levels_from_samples(&[f32::NAN, f32::INFINITY], 2);
        assert_eq!(invalid, vec![0.0, 0.0]);
    }
}
