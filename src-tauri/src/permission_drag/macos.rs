use std::{
    cell::RefCell,
    ffi::c_void,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use core_foundation::{
    base::TCFType,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::{CFString, CFStringRef},
};
use core_graphics::{
    geometry::CGRect,
    window::{
        copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowOwnerName,
    },
};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSEvent, NSFloatingWindowLevel, NSFont, NSImageView, NSPanel,
    NSScreen, NSTextField, NSView, NSWindow, NSWindowButton, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};
use tauri::AppHandle;

use super::{
    PermissionDragTarget, ScreenRect, ScreenSize, resolve_running_app_bundle, shelf_origin,
};
use crate::permissions::{PermissionGrantStatus, permission_snapshot};

const SHELF_WIDTH: f64 = 360.0;
const SHELF_HEIGHT: f64 = 112.0;
const SETTINGS_DISCOVERY_ATTEMPTS: usize = 20;
const SETTINGS_DISCOVERY_INTERVAL: Duration = Duration::from_millis(250);
const MONITOR_INTERVAL: Duration = Duration::from_millis(400);
const MAIN_THREAD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

static ACTIVE_REQUEST_ID: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static SHELF_STATE: RefCell<Option<ShelfState>> = const { RefCell::new(None) };
}

struct ShelfState {
    request_id: u64,
    panel: Retained<NSPanel>,
}

#[derive(Debug)]
struct DragShelfViewIvars {
    bundle_path: Retained<NSString>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[name = "WakeNotePermissionDragShelfView"]
    #[ivars = DragShelfViewIvars]
    struct DragShelfView;

    impl DragShelfView {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            self as *const Self as *mut NSView
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            #[allow(deprecated)]
            let _ = self.dragFile_fromRect_slideBack_event(
                &self.ivars().bundle_path,
                self.bounds(),
                true,
                event,
            );
        }
    }

    unsafe impl NSObjectProtocol for DragShelfView {}
);

impl DragShelfView {
    fn new(
        frame: NSRect,
        bundle_path: Retained<NSString>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let view = Self::alloc(mtm).set_ivars(DragShelfViewIvars { bundle_path });
        unsafe { msg_send![super(view), initWithFrame: frame] }
    }
}

pub fn show_permission_drag_shelf(
    app: &AppHandle,
    target: PermissionDragTarget,
) -> Result<(), String> {
    let request_id = ACTIVE_REQUEST_ID.fetch_add(1, Ordering::AcqRel) + 1;
    schedule_close_replaced_shelf(app, request_id);

    Command::new("/usr/bin/open")
        .arg(target.settings_url())
        .spawn()
        .map_err(|error| format!("Could not open System Settings: {error}"))?;

    // Opening the pane remains useful even when a development executable is
    // not inside an app bundle. Only the drag helper is withheld in that case.
    let bundle_path = resolve_running_app_bundle().map_err(|error| error.to_string())?;
    let app = app.clone();

    thread::spawn(move || monitor_permission_shelf(app, request_id, target, bundle_path));
    Ok(())
}

pub fn close_permission_drag_shelf(app: &AppHandle) {
    let request_id = ACTIVE_REQUEST_ID.fetch_add(1, Ordering::AcqRel) + 1;
    schedule_close_replaced_shelf(app, request_id);
}

fn monitor_permission_shelf(
    app: AppHandle,
    request_id: u64,
    target: PermissionDragTarget,
    bundle_path: PathBuf,
) {
    let mut initial_bounds = None;
    for _ in 0..SETTINGS_DISCOVERY_ATTEMPTS {
        if request_is_stale(request_id) || permission_is_granted(target) {
            return;
        }
        if let Some(bounds) = system_settings_window_bounds() {
            initial_bounds = Some(bounds);
            break;
        }
        thread::sleep(SETTINGS_DISCOVERY_INTERVAL);
    }

    let Some(bounds) = initial_bounds else {
        return;
    };
    if !present_on_main_thread(&app, request_id, bundle_path, bounds) {
        return;
    }

    let mut missing_polls = 0;
    loop {
        thread::sleep(MONITOR_INTERVAL);
        if request_is_stale(request_id) || permission_is_granted(target) {
            schedule_close(&app, Some(request_id));
            return;
        }

        let Some(bounds) = system_settings_window_bounds() else {
            missing_polls += 1;
            if missing_polls >= 3 {
                schedule_close(&app, Some(request_id));
                return;
            }
            continue;
        };
        missing_polls = 0;
        if !reposition_on_main_thread(&app, request_id, bounds) {
            return;
        }
    }
}

fn request_is_stale(request_id: u64) -> bool {
    ACTIVE_REQUEST_ID.load(Ordering::Acquire) != request_id
}

fn permission_is_granted(target: PermissionDragTarget) -> bool {
    let permissions = permission_snapshot();
    let status = match target {
        PermissionDragTarget::Accessibility => permissions.accessibility.status,
        PermissionDragTarget::ScreenRecording => permissions.screen_recording.status,
    };
    status == PermissionGrantStatus::Granted
}

fn present_on_main_thread(
    app: &AppHandle,
    request_id: u64,
    bundle_path: PathBuf,
    system_settings: CGRect,
) -> bool {
    let (sender, receiver) = mpsc::sync_channel(1);
    if app
        .run_on_main_thread(move || {
            if request_is_stale(request_id) {
                let _ = sender.send(false);
                return;
            }
            let Some(mtm) = MainThreadMarker::new() else {
                let _ = sender.send(false);
                return;
            };

            let state = build_shelf(request_id, &bundle_path, system_settings, mtm);
            let shown = state.is_some();
            SHELF_STATE.with(|slot| {
                let previous = std::mem::replace(&mut *slot.borrow_mut(), state);
                if let Some(previous) = previous {
                    previous.panel.orderOut(None);
                }
            });
            let _ = sender.send(shown);
        })
        .is_err()
    {
        return false;
    }

    receiver
        .recv_timeout(MAIN_THREAD_RESPONSE_TIMEOUT)
        .unwrap_or(false)
}

fn reposition_on_main_thread(app: &AppHandle, request_id: u64, system_settings: CGRect) -> bool {
    let (sender, receiver) = mpsc::sync_channel(1);
    if app
        .run_on_main_thread(move || {
            let visible = SHELF_STATE.with(|slot| {
                let mut slot = slot.borrow_mut();
                let Some(state) = slot.as_ref() else {
                    return false;
                };
                if state.request_id != request_id || !state.panel.isVisible() {
                    if let Some(state) = slot.take() {
                        state.panel.orderOut(None);
                    }
                    return false;
                }

                if let Some(mtm) = MainThreadMarker::new() {
                    position_panel(&state.panel, system_settings, mtm);
                }
                true
            });
            let _ = sender.send(visible);
        })
        .is_err()
    {
        return false;
    }

    receiver
        .recv_timeout(MAIN_THREAD_RESPONSE_TIMEOUT)
        .unwrap_or(false)
}

fn schedule_close(app: &AppHandle, request_id: Option<u64>) {
    let _ = app.run_on_main_thread(move || {
        SHELF_STATE.with(|slot| {
            let should_close = slot
                .borrow()
                .as_ref()
                .is_some_and(|state| request_id.is_none_or(|id| state.request_id == id));
            if should_close && let Some(state) = slot.borrow_mut().take() {
                state.panel.orderOut(None);
            }
        });
    });
}

fn schedule_close_replaced_shelf(app: &AppHandle, active_request_id: u64) {
    let _ = app.run_on_main_thread(move || {
        SHELF_STATE.with(|slot| {
            let should_close = slot
                .borrow()
                .as_ref()
                .is_some_and(|state| state.request_id != active_request_id);
            if should_close && let Some(state) = slot.borrow_mut().take() {
                state.panel.orderOut(None);
            }
        });
    });
}

fn build_shelf(
    request_id: u64,
    bundle_path: &Path,
    system_settings: CGRect,
    mtm: MainThreadMarker,
) -> Option<ShelfState> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::UtilityWindow
        | NSWindowStyleMask::NonactivatingPanel;
    let desired_frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(SHELF_WIDTH, SHELF_HEIGHT),
    );
    let content_rect = NSWindow::contentRectForFrameRect_styleMask(desired_frame, style, mtm);
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        content_rect,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setBecomesKeyOnlyIfNeeded(true);
    panel.setHidesOnDeactivate(false);
    panel.setLevel(NSFloatingWindowLevel);
    panel.setTitle(&NSString::from_str("Add WakeNote"));
    if let Some(close_button) = panel.standardWindowButton(NSWindowButton::CloseButton) {
        let close_label = NSString::from_str("Close permission helper");
        unsafe {
            let _: () = msg_send![&*close_button, setAccessibilityLabel: &*close_label];
        }
    }

    let content = panel.contentView()?;
    let path = NSString::from_str(&bundle_path.to_string_lossy());
    let drag_view = DragShelfView::new(content.bounds(), path.clone(), mtm);
    drag_view.setToolTip(Some(&NSString::from_str(
        "Drag WakeNote into the list above",
    )));

    let icon = NSWorkspace::sharedWorkspace().iconForFile(&path);
    icon.setSize(NSSize::new(54.0, 54.0));
    let icon_view = NSImageView::imageViewWithImage(&icon, mtm);
    icon_view.setFrame(NSRect::new(
        NSPoint::new(9.0, 10.0),
        NSSize::new(54.0, 54.0),
    ));
    drag_view.addSubview(&icon_view);

    let heading = NSTextField::labelWithString(&NSString::from_str("WakeNote"), mtm);
    heading.setFrame(NSRect::new(
        NSPoint::new(98.0, 47.0),
        NSSize::new(244.0, 20.0),
    ));
    heading.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    heading.setTextColor(Some(&NSColor::labelColor()));

    let instructions = NSTextField::wrappingLabelWithString(
        &NSString::from_str("Drag WakeNote into the list above"),
        mtm,
    );
    instructions.setFrame(NSRect::new(
        NSPoint::new(98.0, 8.0),
        NSSize::new(244.0, 38.0),
    ));
    instructions.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    instructions.setTextColor(Some(&NSColor::secondaryLabelColor()));

    drag_view.addSubview(&heading);
    drag_view.addSubview(&instructions);
    content.addSubview(&drag_view);
    position_panel(&panel, system_settings, mtm);
    panel.orderFrontRegardless();

    Some(ShelfState { request_id, panel })
}

fn position_panel(panel: &NSPanel, system_settings: CGRect, mtm: MainThreadMarker) {
    let Some((system_settings, visible_frame)) = cocoa_frames(system_settings, mtm) else {
        return;
    };
    let frame = panel.frame();
    let origin = shelf_origin(
        system_settings,
        ScreenSize::new(frame.size.width, frame.size.height),
        visible_frame,
    );
    panel.setFrameOrigin(NSPoint::new(origin.x, origin.y));
}

fn cocoa_frames(
    system_settings: CGRect,
    mtm: MainThreadMarker,
) -> Option<(ScreenRect, ScreenRect)> {
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject()?;
    let primary_frame = primary.frame();
    let system_settings = ScreenRect::new(
        system_settings.origin.x,
        primary_frame.origin.y + primary_frame.size.height
            - system_settings.origin.y
            - system_settings.size.height,
        system_settings.size.width,
        system_settings.size.height,
    );
    let center_x = system_settings.x + system_settings.width / 2.0;
    let center_y = system_settings.y + system_settings.height / 2.0;

    let mut selected = None;
    for index in 0..screens.count() {
        let screen = screens.objectAtIndex(index);
        let frame = screen.frame();
        if center_x >= frame.origin.x
            && center_x <= frame.origin.x + frame.size.width
            && center_y >= frame.origin.y
            && center_y <= frame.origin.y + frame.size.height
        {
            selected = Some(screen.visibleFrame());
            break;
        }
    }
    let visible = selected.unwrap_or_else(|| primary.visibleFrame());
    Some((
        system_settings,
        ScreenRect::new(
            visible.origin.x,
            visible.origin.y,
            visible.size.width,
            visible.size.height,
        ),
    ))
}

fn system_settings_window_bounds() -> Option<CGRect> {
    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )?;
    let mut best = None;
    let mut best_area = 0.0;

    for raw_window in windows.iter() {
        let raw_window = *raw_window;
        if raw_window.is_null() {
            continue;
        }
        let dictionary = unsafe {
            CFDictionary::<*const c_void, *const c_void>::wrap_under_get_rule(
                raw_window as CFDictionaryRef,
            )
        };
        let Some(raw_owner) = dictionary.find(unsafe { kCGWindowOwnerName } as *const c_void)
        else {
            continue;
        };
        let owner = unsafe { CFString::wrap_under_get_rule(*raw_owner as CFStringRef) };
        if !is_system_settings_owner(&owner.to_string()) {
            continue;
        }

        let Some(raw_bounds) = dictionary.find(unsafe { kCGWindowBounds } as *const c_void) else {
            continue;
        };
        let bounds = unsafe {
            CFDictionary::<*const c_void, *const c_void>::wrap_under_get_rule(
                *raw_bounds as CFDictionaryRef,
            )
        };
        let Some(bounds) = CGRect::from_dict_representation(&bounds) else {
            continue;
        };
        let area = bounds.size.width * bounds.size.height;
        if bounds.size.width >= 240.0 && bounds.size.height >= 240.0 && area > best_area {
            best = Some(bounds);
            best_area = area;
        }
    }

    best
}

fn is_system_settings_owner(owner: &str) -> bool {
    matches!(
        owner,
        "System Settings" | "System Preferences" | "시스템 설정" | "시스템 환경설정"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_current_and_legacy_system_settings_process_names() {
        assert!(is_system_settings_owner("System Settings"));
        assert!(is_system_settings_owner("System Preferences"));
        assert!(is_system_settings_owner("시스템 설정"));
        assert!(!is_system_settings_owner("WakeNote"));
    }
}
