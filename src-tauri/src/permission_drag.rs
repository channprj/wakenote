use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::Deserialize;

const SHELF_GAP: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDragTarget {
    Accessibility,
    ScreenRecording,
}

impl PermissionDragTarget {
    pub const fn settings_url(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Self::ScreenRecording => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenPoint {
    pub x: f64,
    pub y: f64,
}

impl ScreenPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenSize {
    pub width: f64,
    pub height: f64,
}

impl ScreenSize {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDragError {
    AppBundleUnavailable,
}

impl fmt::Display for PermissionDragError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AppBundleUnavailable => formatter.write_str(
                "WakeNote must be running from an installed .app bundle to offer drag authorization",
            ),
        }
    }
}

impl std::error::Error for PermissionDragError {}

pub fn app_bundle_ancestor(executable: &Path) -> Option<PathBuf> {
    executable
        .ancestors()
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        })
        .map(Path::to_path_buf)
}

pub fn validate_app_bundle(path: &Path) -> Result<PathBuf, PermissionDragError> {
    if path.is_dir()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        Ok(path.to_path_buf())
    } else {
        Err(PermissionDragError::AppBundleUnavailable)
    }
}

pub fn resolve_running_app_bundle() -> Result<PathBuf, PermissionDragError> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| app_bundle_ancestor(&executable))
        .and_then(|bundle| validate_app_bundle(&bundle).ok())
        .ok_or(PermissionDragError::AppBundleUnavailable)
}

pub fn shelf_origin(
    system_settings: ScreenRect,
    shelf: ScreenSize,
    visible_frame: ScreenRect,
) -> ScreenPoint {
    let min_x = visible_frame.x;
    let max_x = (visible_frame.x + visible_frame.width - shelf.width).max(min_x);
    let centered_x = system_settings.x + (system_settings.width - shelf.width) / 2.0;

    let min_y = visible_frame.y;
    let max_y = (visible_frame.y + visible_frame.height - shelf.height).max(min_y);
    let below_y = system_settings.y - shelf.height - SHELF_GAP;
    let preferred_y = if below_y >= min_y {
        below_y
    } else {
        system_settings.y + system_settings.height + SHELF_GAP
    };

    ScreenPoint::new(
        centered_x.clamp(min_x, max_x),
        preferred_y.clamp(min_y, max_y),
    )
}

#[cfg(target_os = "macos")]
#[path = "permission_drag/macos.rs"]
mod platform;

#[cfg(target_os = "macos")]
pub use platform::{close_permission_drag_shelf, show_permission_drag_shelf};

#[cfg(not(target_os = "macos"))]
pub fn show_permission_drag_shelf(
    _app: &tauri::AppHandle,
    _target: PermissionDragTarget,
) -> Result<(), String> {
    Err("Permission drag authorization is available only on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn close_permission_drag_shelf(_app: &tauri::AppHandle) {}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn permission_targets_expose_only_supported_settings_urls() {
        assert_eq!(
            PermissionDragTarget::Accessibility.settings_url(),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        );
        assert_eq!(
            PermissionDragTarget::ScreenRecording.settings_url(),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        );
    }

    #[test]
    fn executable_inside_app_resolves_the_bundle_ancestor() {
        let executable = Path::new("/Applications/WakeNote.app/Contents/MacOS/wakenote");
        assert_eq!(
            app_bundle_ancestor(executable),
            Some(PathBuf::from("/Applications/WakeNote.app"))
        );
    }

    #[test]
    fn closest_app_ancestor_wins_for_nested_paths() {
        let executable =
            Path::new("/Applications/Outer.app/Contents/Helpers/Inner.app/Contents/MacOS/helper");
        assert_eq!(
            app_bundle_ancestor(executable),
            Some(PathBuf::from(
                "/Applications/Outer.app/Contents/Helpers/Inner.app"
            ))
        );
    }

    #[test]
    fn ordinary_executable_is_not_a_drag_source() {
        assert_eq!(app_bundle_ancestor(Path::new("/tmp/wakenote")), None);
    }

    #[test]
    fn bundle_validator_accepts_a_directory_and_rejects_files_or_missing_paths() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let bundle = temporary.path().join("WakeNote.app");
        std::fs::create_dir(&bundle).expect("create app bundle directory");
        assert_eq!(validate_app_bundle(&bundle), Ok(bundle.clone()));

        let file = temporary.path().join("NotAnApp.app");
        std::fs::write(&file, b"not a bundle").expect("create ordinary file");
        assert_eq!(
            validate_app_bundle(&file),
            Err(PermissionDragError::AppBundleUnavailable)
        );
        assert_eq!(
            validate_app_bundle(&temporary.path().join("Missing.app")),
            Err(PermissionDragError::AppBundleUnavailable)
        );
    }

    #[test]
    fn shelf_prefers_below_and_centers_on_system_settings() {
        let origin = shelf_origin(
            ScreenRect::new(100.0, 220.0, 900.0, 650.0),
            ScreenSize::new(360.0, 112.0),
            ScreenRect::new(0.0, 0.0, 1100.0, 900.0),
        );
        assert_eq!(origin, ScreenPoint::new(370.0, 96.0));
    }

    #[test]
    fn shelf_moves_above_when_bottom_space_is_insufficient() {
        let origin = shelf_origin(
            ScreenRect::new(100.0, 40.0, 900.0, 650.0),
            ScreenSize::new(360.0, 112.0),
            ScreenRect::new(0.0, 0.0, 1100.0, 900.0),
        );
        assert_eq!(origin, ScreenPoint::new(370.0, 702.0));
    }

    #[test]
    fn shelf_clamps_to_negative_origin_secondary_display() {
        let origin = shelf_origin(
            ScreenRect::new(-1_400.0, 180.0, 1_000.0, 700.0),
            ScreenSize::new(360.0, 112.0),
            ScreenRect::new(-1_440.0, 0.0, 1_440.0, 900.0),
        );
        assert_eq!(origin, ScreenPoint::new(-1_080.0, 56.0));
    }

    #[test]
    fn shelf_clamps_horizontally_to_visible_frame() {
        let origin = shelf_origin(
            ScreenRect::new(980.0, 220.0, 100.0, 650.0),
            ScreenSize::new(360.0, 112.0),
            ScreenRect::new(0.0, 0.0, 1_100.0, 900.0),
        );
        assert_eq!(origin, ScreenPoint::new(740.0, 96.0));
    }
}
