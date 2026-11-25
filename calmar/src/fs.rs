//! Small filesystem helpers.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Result, VaultError};

/// Counter for temporary file names, so two saves never collide.
static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

/// Writes to a temporary file in the same directory, then renames it.
///
/// A crash mid-save then leaves the previous version intact instead of a
/// half-written note. The temporary file is ignored by `.gitignore`.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path.parent().ok_or_else(|| VaultError::InvalidPath {
        path: path.to_path_buf(),
        reason: "path has no parent directory".to_owned(),
    })?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "note".to_owned());
    let tmp = parent.join(format!(
        ".{name}.{}.tmp",
        NEXT_TMP.fetch_add(1, Ordering::Relaxed)
    ));

    std::fs::write(&tmp, contents).map_err(|e| VaultError::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        VaultError::io(path, e)
    })
}
