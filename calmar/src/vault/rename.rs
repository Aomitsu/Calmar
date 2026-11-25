//! Renaming a file, and keeping every link that points at it valid.
//!
//! A rename is not a filesystem operation with a commit on top: renaming a
//! note changes the meaning of every relative link in the vault, including the
//! ones inside the renamed note itself.

use std::fs;
use std::ops::Range;
use std::path::Path;

use crate::error::{Result, VaultError};
use crate::git::CommitId;
use crate::markdown;

use super::Vault;

impl Vault {
    /// Moves or renames a file and rewrites the links affected by the move.
    ///
    /// All rewrites are computed before the filesystem changes, so a failure
    /// leaves the vault as it was. Returns `None` when `from` and `to` are the
    /// same path.
    pub fn rename(
        &mut self,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> Result<Option<CommitId>> {
        let (from_rel, from_absolute) = self.resolve(from.as_ref())?;
        let (to_rel, to_absolute) = self.resolve(to.as_ref())?;
        let from_path = from_rel.display().to_owned();
        let to_path = to_rel.display().to_owned();

        if from_path == to_path {
            return Ok(None);
        }
        if !from_absolute.is_file() {
            return Err(VaultError::NotAFile {
                path: from_absolute,
            });
        }
        if to_absolute.exists() {
            return Err(VaultError::AlreadyExists { path: to_absolute });
        }

        let mut updates: Vec<(String, String)> = Vec::new();
        for note in self.note_paths()? {
            let absolute = self.root().join(&note);
            let source = fs::read_to_string(&absolute).map_err(|e| VaultError::io(&absolute, e))?;

            if let Some(rewritten) = rewrite_links(&source, &note, &from_path, &to_path) {
                updates.push((note, rewritten));
            }
        }

        if let Some(parent) = to_absolute.parent() {
            fs::create_dir_all(parent).map_err(|e| VaultError::io(parent, e))?;
        }
        fs::rename(&from_absolute, &to_absolute).map_err(|e| VaultError::io(&from_absolute, e))?;

        for (note, contents) in updates {
            let destination = if note == from_path {
                to_absolute.clone()
            } else {
                self.root().join(&note)
            };
            crate::fs::write_atomic(&destination, &contents)?;
        }

        self.commit(&format!("Rename {from_path} to {to_path}"))
    }
}

/// Rewrites the links of one note that the move from `from` to `to` affects.
///
/// Returns `None` when no link needs to change, so untouched notes are not
/// rewritten (and do not show up in the commit).
fn rewrite_links(source: &str, note: &str, from: &str, to: &str) -> Option<String> {
    let moved = note == from;
    let note_after = if moved { to } else { note };
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    for raw in markdown::parse(source) {
        let Some(resolved) = markdown::resolve(note, &raw.target) else {
            continue;
        };

        // Two reasons for a link to change: it points at the renamed file, or
        // it lives in the renamed file and is relative to it.
        let affected = resolved.path == from || (moved && !resolved.absolute);
        if !affected {
            continue;
        }

        let target_after = if resolved.path == from {
            to
        } else {
            resolved.path.as_str()
        };
        let text = markdown::format(
            note_after,
            target_after,
            &resolved.fragment,
            resolved.absolute,
        );

        if text != raw.target {
            edits.push((raw.span, text));
        }
    }

    if edits.is_empty() {
        return None;
    }

    let mut rewritten = source.to_owned();
    // Reverse order: replacing a span shifts the offsets that follow it.
    for (span, text) in edits.into_iter().rev() {
        rewritten.replace_range(span, &text);
    }
    Some(rewritten)
}
