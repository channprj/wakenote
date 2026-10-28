//! macOS menu bar icon repositioning.
//!
//! `tray-icon` covers the status button with a `TaoTrayTarget` view that
//! implements its own mouse handling. That view receives the click before
//! `NSStatusBarButton`, so Command-drag never starts and the icon cannot be
//! moved. The status item is also created without an autosave name, so AppKit
//! has nowhere to keep a position the user does manage to set.
//!
//! Command-drag is forwarded by letting the status button win hit testing while
//! Command is held. The position is stored under a stable autosave name, including
//! after the icon is hidden and created again.
//!
//! The optional level readout beside the icon uses monospaced digits, so a
//! changing level does not shift the status items to its left. When it stands
//! in for the status dot, it is drawn in the dot's color.

use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};
use std::thread;

use crate::log_error;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{ffi, sel};
use objc2_app_kit::{
    NSApplication, NSColor, NSEventModifierFlags, NSEventType, NSFont, NSFontAttributeName,
    NSFontWeightRegular, NSForegroundColorAttributeName,
};
use objc2_foundation::{MainThreadMarker, NSAttributedString, NSDictionary, NSPoint, NSString};
use tauri::Runtime;
use tauri::tray::TrayIcon;

const MENU_BAR_ICON_AUTOSAVE_NAME: &str = "com.chann.wakenote.menu-bar-icon";
const TRAY_TARGET_CLASS: &std::ffi::CStr = c"TaoTrayTarget";
const VISIBILITY_UNKNOWN: u8 = 0;
const VISIBILITY_SHOWN: u8 = 1;
const VISIBILITY_HIDDEN: u8 = 2;

static ORIGINAL_HIT_TEST: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static VISIBILITY: AtomicU8 = AtomicU8::new(VISIBILITY_UNKNOWN);

type HitTestImp = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, NSPoint) -> *mut AnyObject;

pub(crate) fn overlay_should_yield_hit_testing(
    event_type: NSEventType,
    flags: NSEventModifierFlags,
) -> bool {
    flags.contains(NSEventModifierFlags::Command)
        && matches!(
            event_type,
            NSEventType::LeftMouseDown | NSEventType::LeftMouseDragged | NSEventType::LeftMouseUp
        )
}

pub(crate) fn should_persist_menu_bar_position(was_visible: Option<bool>, visible: bool) -> bool {
    visible && was_visible != Some(true)
}

fn visibility_code(visible: bool) -> u8 {
    if visible {
        VISIBILITY_SHOWN
    } else {
        VISIBILITY_HIDDEN
    }
}

fn was_visible(code: u8) -> Option<bool> {
    match code {
        VISIBILITY_SHOWN => Some(true),
        VISIBILITY_HIDDEN => Some(false),
        _ => None,
    }
}

/// Installs the Command-drag override. Safe to call once, after the tray exists.
pub fn enable_menu_bar_icon_repositioning() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if let Err(error) = install_command_drag_override() {
            log_error!("[tray] menu bar icon cannot be repositioned: {error}");
        }
    });
}

fn install_command_drag_override() -> Result<(), String> {
    let class = AnyClass::get(TRAY_TARGET_CLASS)
        .ok_or_else(|| format!("{} is not registered", TRAY_TARGET_CLASS.to_string_lossy()))?;
    let superclass = class
        .superclass()
        .ok_or_else(|| "tray target has no superclass".to_string())?;
    let superclass_hit_test = superclass
        .instance_method(sel!(hitTest:))
        .ok_or_else(|| "NSView hitTest: is missing".to_string())?;
    let encoding = unsafe { ffi::method_getTypeEncoding(superclass_hit_test) };
    if encoding.is_null() {
        return Err("hitTest: has no type encoding".to_string());
    }

    let replacement: Imp = unsafe { std::mem::transmute(command_drag_hit_test as HitTestImp) };
    let own_methods = class.instance_methods();
    if let Some(method) = own_methods
        .iter()
        .find(|method| method.name() == sel!(hitTest:))
    {
        ORIGINAL_HIT_TEST.store(method.implementation() as *mut (), Ordering::Release);
        unsafe { method.set_implementation(replacement) };
        return Ok(());
    }

    ORIGINAL_HIT_TEST.store(
        superclass_hit_test.implementation() as *mut (),
        Ordering::Release,
    );
    let added = unsafe {
        ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel!(hitTest:),
            replacement,
            encoding,
        )
    };
    if added.as_bool() {
        Ok(())
    } else {
        Err("could not override tray target hit testing".to_string())
    }
}

unsafe extern "C-unwind" fn command_drag_hit_test(
    this: *mut AnyObject,
    selector: Sel,
    point: NSPoint,
) -> *mut AnyObject {
    if current_event_should_yield() {
        return std::ptr::null_mut();
    }
    let original = ORIGINAL_HIT_TEST.load(Ordering::Acquire);
    if original.is_null() {
        return std::ptr::null_mut();
    }
    let original: HitTestImp = unsafe { std::mem::transmute(original) };
    unsafe { original(this, selector, point) }
}

fn current_event_should_yield() -> bool {
    let Some(marker) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(marker)
        .currentEvent()
        .is_some_and(|event| {
            overlay_should_yield_hit_testing(event.r#type(), event.modifierFlags())
        })
}

/// Gives AppKit a stable name so a Command-drag position survives relaunch.
///
/// Hiding the icon removes the status item. The replacement needs the same name
/// before the user can drag it again.
pub fn remember_menu_bar_icon_position<R: Runtime>(tray: &TrayIcon<R>, visible: bool) {
    let previous = VISIBILITY.swap(visibility_code(visible), Ordering::AcqRel);
    if !should_persist_menu_bar_position(was_visible(previous), visible) {
        return;
    }

    let tray = tray.clone();
    if thread::Builder::new()
        .name("wakenote-menu-bar-icon".to_string())
        .spawn(move || persist_menu_bar_icon_position(tray))
        .is_err()
    {
        log_error!("[tray] could not start menu bar icon position restore");
    }
}

fn persist_menu_bar_icon_position<R: Runtime>(tray: TrayIcon<R>) {
    let persisted = tray.with_inner_tray_icon(|icon| {
        let Some(item) = icon.ns_status_item() else {
            return false;
        };
        let name = NSString::from_str(MENU_BAR_ICON_AUTOSAVE_NAME);
        if item.autosaveName().isEqualToString(&name) {
            return true;
        }
        item.setAutosaveName(Some(&name));
        true
    });
    if let Err(error) = persisted {
        log_error!("[tray] could not persist menu bar icon position: {error}");
    }
}

fn level_font() -> Retained<NSFont> {
    let size = NSFont::menuBarFontOfSize(0.0).pointSize();
    let weight = unsafe { NSFontWeightRegular };
    NSFont::monospacedDigitSystemFontOfSize_weight(size, weight)
}

/// Shows `title` beside the menu bar icon, or the icon alone for `None`.
/// `color` draws the title in an sRGB color; `None` keeps the label color.
pub fn set_menu_bar_icon_title<R: Runtime>(
    tray: &TrayIcon<R>,
    title: Option<&str>,
    color: Option<[u8; 3]>,
) {
    if title.is_some() {
        // Style before the title is set: setting it resizes the click target.
        let styled = tray.with_inner_tray_icon(|icon| {
            let (Some(item), Some(marker)) = (icon.ns_status_item(), MainThreadMarker::new())
            else {
                return;
            };
            if let Some(button) = item.button(marker) {
                button.setFont(Some(&level_font()));
            }
        });
        if let Err(error) = styled {
            log_error!("[tray] could not style the menu bar level: {error}");
        }
    }
    if let Err(error) = tray.set_title(title) {
        log_error!("[tray] could not show the menu bar level: {error}");
    }
    let (Some(title), Some([red, green, blue])) = (title, color) else {
        return;
    };
    // The plain title above keeps tray-icon's state and click target in sync;
    // the attributed title only adds the color, so its width is unchanged.
    let title = title.to_owned();
    let colored = tray.with_inner_tray_icon(move |icon| {
        let (Some(item), Some(marker)) = (icon.ns_status_item(), MainThreadMarker::new()) else {
            return;
        };
        let Some(button) = item.button(marker) else {
            return;
        };
        let font = level_font();
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            f64::from(red) / 255.0,
            f64::from(green) / 255.0,
            f64::from(blue) / 255.0,
            1.0,
        );
        let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
        let values: [&AnyObject; 2] = [&font, &color];
        let attributes = NSDictionary::<NSString, AnyObject>::from_slices(&keys, &values);
        let text = NSString::from_str(&title);
        // SAFETY: both attributes carry the value types AppKit expects for them.
        let attributed = unsafe { NSAttributedString::new_with_attributes(&text, &attributes) };
        button.setAttributedTitle(&attributed);
    });
    if let Err(error) = colored {
        log_error!("[tray] could not color the menu bar level: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        VISIBILITY_HIDDEN, VISIBILITY_SHOWN, VISIBILITY_UNKNOWN, overlay_should_yield_hit_testing,
        should_persist_menu_bar_position, was_visible,
    };
    use objc2_app_kit::{NSEventModifierFlags, NSEventType};

    #[test]
    fn command_drag_yields_the_overlay_without_swallowing_normal_clicks() {
        let command = NSEventModifierFlags::Command;
        assert!(overlay_should_yield_hit_testing(
            NSEventType::LeftMouseDown,
            command
        ));
        assert!(overlay_should_yield_hit_testing(
            NSEventType::LeftMouseDragged,
            command
        ));
        assert!(overlay_should_yield_hit_testing(
            NSEventType::LeftMouseUp,
            command
        ));
        assert!(!overlay_should_yield_hit_testing(
            NSEventType::LeftMouseDown,
            NSEventModifierFlags::empty()
        ));
        assert!(!overlay_should_yield_hit_testing(
            NSEventType::RightMouseDown,
            command
        ));
        assert!(!overlay_should_yield_hit_testing(
            NSEventType::LeftMouseDown,
            NSEventModifierFlags::Control
        ));
        assert!(overlay_should_yield_hit_testing(
            NSEventType::LeftMouseDown,
            NSEventModifierFlags::Command | NSEventModifierFlags::Shift
        ));
    }

    #[test]
    fn position_is_saved_when_the_icon_appears_and_not_on_every_refresh() {
        assert!(should_persist_menu_bar_position(
            was_visible(VISIBILITY_UNKNOWN),
            true
        ));
        assert!(!should_persist_menu_bar_position(
            was_visible(VISIBILITY_SHOWN),
            true
        ));
        assert!(!should_persist_menu_bar_position(
            was_visible(VISIBILITY_SHOWN),
            false
        ));
        assert!(should_persist_menu_bar_position(
            was_visible(VISIBILITY_HIDDEN),
            true
        ));
        assert!(!should_persist_menu_bar_position(
            was_visible(VISIBILITY_HIDDEN),
            false
        ));
    }
}
