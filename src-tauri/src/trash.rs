use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
pub fn move_to_trash(path: &Path) -> Result<PathBuf, String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSFileManager, NSURL};

    autoreleasepool(|_| {
        let source_url = NSURL::from_file_path(path)
            .ok_or_else(|| format!("invalid file path: {}", path.display()))?;
        let mut resulting_url = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&source_url, Some(&mut resulting_url))
            .map_err(|error| error.to_string())?;
        resulting_url
            .and_then(|url| url.to_file_path())
            .ok_or_else(|| "macOS did not return the trashed file location".to_string())
    })
}

#[cfg(not(target_os = "macos"))]
pub fn move_to_trash(_path: &Path) -> Result<PathBuf, String> {
    Err("moving Activity audio to the OS Trash is only supported on macOS".to_string())
}
