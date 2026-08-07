use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static TRASH_STAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashedRecordingBundle {
    pub reference_path: PathBuf,
    pub original_paths: Vec<PathBuf>,
    pub trash_path: PathBuf,
}

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
    Err("moving recording bundles to the OS Trash is only supported on macOS".to_string())
}

pub fn recording_bundle_paths(reference_path: &Path) -> Result<Vec<PathBuf>, String> {
    let parent = reference_path
        .parent()
        .ok_or_else(|| "recording reference has no parent directory".to_string())?;
    let file_name = reference_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "recording reference filename is not valid UTF-8".to_string())?;
    let stem = file_name
        .strip_suffix(".error.txt")
        .or_else(|| file_name.rsplit_once('.').map(|(stem, _)| stem))
        .unwrap_or(file_name)
        .trim();
    if stem.is_empty() {
        return Err("recording reference has an empty stem".to_string());
    }

    let stem_path = parent.join(stem);
    Ok(["m4a", "wav", "mp3", "txt", "json", "error.txt"]
        .into_iter()
        .map(|extension| stem_path.with_extension(extension))
        .collect())
}

pub fn move_recording_bundle_to_trash(
    reference_path: &Path,
) -> Result<Option<TrashedRecordingBundle>, String> {
    move_recording_bundle_to_trash_with(reference_path, move_to_trash)
}

pub fn move_recording_bundle_to_trash_with<F>(
    reference_path: &Path,
    mut move_stage_to_trash: F,
) -> Result<Option<TrashedRecordingBundle>, String>
where
    F: FnMut(&Path) -> Result<PathBuf, String>,
{
    let original_paths = recording_bundle_paths(reference_path)?
        .into_iter()
        .filter_map(|path| match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
                Some(Ok(path))
            }
            Ok(_) => Some(Err(format!(
                "recording bundle member is not a file: {}",
                path.display()
            ))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(Err(format!(
                "failed to inspect recording bundle member {}: {error}",
                path.display()
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if original_paths.is_empty() {
        return Ok(None);
    }

    let stage_path = create_bundle_stage(reference_path)?;
    let mut staged = Vec::with_capacity(original_paths.len());
    for original_path in &original_paths {
        let file_name = original_path
            .file_name()
            .ok_or_else(|| format!("bundle member has no filename: {}", original_path.display()))?;
        let staged_path = stage_path.join(file_name);
        if let Err(error) = fs::rename(original_path, &staged_path) {
            let rollback_error = rollback_staged_files(&stage_path, &staged);
            return Err(format_with_rollback(
                format!(
                    "failed to stage recording bundle member {}: {error}",
                    original_path.display()
                ),
                rollback_error,
            ));
        }
        staged.push((original_path.clone(), staged_path));
    }

    match move_stage_to_trash(&stage_path) {
        Ok(trash_path) => Ok(Some(TrashedRecordingBundle {
            reference_path: reference_path.to_path_buf(),
            original_paths,
            trash_path,
        })),
        Err(error) => {
            let rollback_error = rollback_staged_files(&stage_path, &staged);
            Err(format_with_rollback(
                format!("failed to move recording bundle to Trash: {error}"),
                rollback_error,
            ))
        }
    }
}

pub fn restore_recording_bundle(bundle: &TrashedRecordingBundle) -> Result<(), String> {
    let mut errors = Vec::new();
    for original_path in &bundle.original_paths {
        let Some(file_name) = original_path.file_name() else {
            errors.push(format!(
                "bundle member has no filename: {}",
                original_path.display()
            ));
            continue;
        };
        let trashed_path = bundle.trash_path.join(file_name);
        if let Err(error) = fs::rename(&trashed_path, original_path) {
            errors.push(format!(
                "failed to restore {} from Trash: {error}",
                original_path.display()
            ));
        }
    }
    if errors.is_empty() {
        fs::remove_dir(&bundle.trash_path).map_err(|error| {
            format!(
                "restored bundle files but failed to remove empty Trash bundle {}: {error}",
                bundle.trash_path.display()
            )
        })?;
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn create_bundle_stage(reference_path: &Path) -> Result<PathBuf, String> {
    let parent = reference_path
        .parent()
        .ok_or_else(|| "recording reference has no parent directory".to_string())?;
    let stem = reference_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("recording")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    for _ in 0..100 {
        let sequence = TRASH_STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            "WakeNote recording bundle - {stem} - {}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create recording Trash stage {}: {error}",
                    candidate.display()
                ));
            }
        }
    }
    Err("failed to allocate a unique recording Trash stage".to_string())
}

fn rollback_staged_files(stage_path: &Path, staged: &[(PathBuf, PathBuf)]) -> Option<String> {
    let mut errors = Vec::new();
    for (original_path, staged_path) in staged.iter().rev() {
        if let Err(error) = fs::rename(staged_path, original_path) {
            errors.push(format!(
                "failed to restore {}: {error}",
                original_path.display()
            ));
        }
    }
    if errors.is_empty()
        && let Err(error) = fs::remove_dir(stage_path)
    {
        errors.push(format!(
            "failed to remove empty Trash stage {}: {error}",
            stage_path.display()
        ));
    }
    (!errors.is_empty()).then(|| errors.join("; "))
}

fn format_with_rollback(error: String, rollback_error: Option<String>) -> String {
    match rollback_error {
        Some(rollback_error) => format!("{error}; rollback incomplete: {rollback_error}"),
        None => error,
    }
}
