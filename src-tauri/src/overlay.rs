use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition};

use crate::settings::FloatingOverlayPosition;

pub const OVERLAY_LABEL: &str = "overlay";
pub const OVERLAY_EVENT: &str = "overlay-state";

const OVERLAY_WIDTH_LOGICAL: f64 = 172.0;
const OVERLAY_HEIGHT_LOGICAL: f64 = 36.0;

#[cfg(target_os = "macos")]
const TOP_OFFSET_LOGICAL: f64 = 46.0;
#[cfg(not(target_os = "macos"))]
const TOP_OFFSET_LOGICAL: f64 = 8.0;

#[cfg(target_os = "macos")]
const BOTTOM_OFFSET_LOGICAL: f64 = 46.0;
#[cfg(not(target_os = "macos"))]
const BOTTOM_OFFSET_LOGICAL: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayState {
    Hidden,
    Recording,
    Transcribing,
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

pub fn create_overlay_window(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window(OVERLAY_LABEL).is_some() {
        return Ok(());
    }

    let builder = tauri::WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        tauri::WebviewUrl::App("overlay.html".into()),
    )
    .inner_size(OVERLAY_WIDTH_LOGICAL, OVERLAY_HEIGHT_LOGICAL)
    .resizable(false)
    .decorations(false)
    .visible(false)
    .focused(false)
    .accept_first_mouse(true)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true);

    let window = builder.build()?;

    #[cfg(target_os = "macos")]
    apply_panel_behaviour(&window)?;

    #[cfg(not(target_os = "macos"))]
    let _ = window;

    Ok(())
}

#[cfg(target_os = "macos")]
fn apply_panel_behaviour(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use tauri_nspanel::{
        tauri_panel, CollectionBehavior, PanelLevel, WebviewWindowExt,
    };

    tauri_panel! {
        panel!(SagwanOverlayPanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true,
                hides_on_deactivate: false
            }
        })
    }

    let panel = window.to_panel::<SagwanOverlayPanel>()?;

    panel.set_level(PanelLevel::Status.value());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .stationary()
            .value(),
    );
    panel.set_has_shadow(false);
    panel.set_corner_radius(0.0);
    panel.set_floating_panel(true);

    Ok(())
}

pub fn show_overlay(
    app: &AppHandle,
    state: OverlayState,
    position: FloatingOverlayPosition,
) -> tauri::Result<()> {
    if matches!(position, FloatingOverlayPosition::Off) {
        return hide_overlay(app);
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
        let physical = calculate_position(
            rect,
            anchor,
            (OVERLAY_WIDTH_LOGICAL, OVERLAY_HEIGHT_LOGICAL),
        );
        window.set_size(LogicalSize::new(OVERLAY_WIDTH_LOGICAL, OVERLAY_HEIGHT_LOGICAL))?;
        window.set_position(physical)?;
    }

    let payload = OverlayStatePayload { state, position };
    let _ = app.emit(OVERLAY_EVENT, payload);

    window.show()?;
    Ok(())
}

pub fn hide_overlay(app: &AppHandle) -> tauri::Result<()> {
    let payload = OverlayStatePayload {
        state: OverlayState::Hidden,
        position: FloatingOverlayPosition::Off,
    };
    let _ = app.emit(OVERLAY_EVENT, payload);

    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        window.hide()?;
    }
    Ok(())
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
) -> PhysicalPosition<i32> {
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

    let logical = LogicalPosition::new(logical_x, logical_y);
    let physical = logical.to_physical::<i32>(scale);
    physical
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
        let pos = calculate_position(monitor, OverlayAnchor::Top, (172.0, 36.0));
        assert_eq!(pos.x, ((1920 - 172) / 2) as i32);
        assert_eq!(pos.y, TOP_OFFSET_LOGICAL as i32);
    }

    #[test]
    fn bottom_anchor_mirrors_top_offset() {
        let monitor = rect((0, 0), (1920, 1080), 1.0);
        let pos = calculate_position(monitor, OverlayAnchor::Bottom, (172.0, 36.0));
        assert_eq!(pos.x, ((1920 - 172) / 2) as i32);
        assert_eq!(pos.y, (1080.0 - 36.0 - BOTTOM_OFFSET_LOGICAL) as i32);
    }

    #[test]
    fn secondary_monitor_with_negative_origin_produces_absolute_coords() {
        let monitor = rect((-1920, 0), (1920, 1080), 1.0);
        let pos = calculate_position(monitor, OverlayAnchor::Top, (172.0, 36.0));
        assert_eq!(pos.x, -1920 + ((1920 - 172) / 2) as i32);
        assert_eq!(pos.y, TOP_OFFSET_LOGICAL as i32);
    }

    #[test]
    fn high_dpi_monitor_returns_physical_coords() {
        let monitor = rect((0, 0), (3840, 2160), 2.0);
        let pos = calculate_position(monitor, OverlayAnchor::Top, (172.0, 36.0));
        // Logical width = 1920, logical x = (1920 - 172) / 2 = 874, physical = 874 * 2 = 1748
        assert_eq!(pos.x, 1748);
        assert_eq!(pos.y, (TOP_OFFSET_LOGICAL * 2.0) as i32);
    }
}
