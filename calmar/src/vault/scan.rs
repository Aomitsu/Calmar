//! Finding notes in the vault and resolving the links between them.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use crate::error::{Result, VaultError};
use crate::markdown;
use crate::note::{Link, Note};
use crate::path::{RelPath, to_display};

use super::Vault;

impl Vault {
    /// Every note, sorted by path.
    ///
    /// Only files under `notes_dir` whose extension is listed in the config
    /// are returned. Directories named in `ignored_dirs` are skipped, as are
    /// `.git`, `.calmar`, and the attachments directory.
    pub fn list(&self) -> Result<Vec<Note>> {
        let paths = self.note_paths()?;
        let known: HashSet<&str> = paths.iter().map(String::as_str).collect();

        let mut notes = Vec::with_capacity(paths.len());
        for path in &paths {
            let absolute = self.root().join(path);
            let source = fs::read_to_string(&absolute).map_err(|e| VaultError::io(&absolute, e))?;
            let links = links_in(path, &source, |target| known.contains(target));
            notes.push(Note {
                path: path.clone(),
                links,
            });
        }

        Ok(notes)
    }

    /// Reads one note without scanning the whole vault.
    pub fn read_note(&self, path: impl AsRef<Path>) -> Result<Note> {
        let rel = RelPath::parse(path.as_ref())?;
        let absolute = rel.absolute(self.root());

        if !absolute.is_file() {
            return Err(VaultError::NotAFile { path: absolute });
        }
        let source = fs::read_to_string(&absolute).map_err(|e| VaultError::io(&absolute, e))?;

        Ok(Note {
            path: rel.display().to_owned(),
            links: links_in(rel.display(), &source, |target| {
                self.root().join(target).is_file()
            }),
        })
    }

    /// Paths of the notes that link to `path`.
    pub fn backlinks(&self, path: impl AsRef<Path>) -> Result<Vec<String>> {
        let target = RelPath::parse(path.as_ref())?.display().to_owned();

        Ok(self
            .list()?
            .into_iter()
            .filter(|note| {
                note.path != target
                    && note
                        .links
                        .iter()
                        .any(|link| link.note.as_deref() == Some(target.as_str()))
            })
            .map(|note| note.path)
            .collect())
    }

    /// Vault-relative paths of every note, sorted.
    pub(crate) fn note_paths(&self) -> Result<Vec<String>> {
        let attachments =
            crate::path::config_dir(self.root(), &self.config.attachments_dir, "attachments_dir")?;
        let attachments = attachments
            .strip_prefix(self.root())
            .map(to_display)
            .unwrap_or_default();

        let mut paths = Vec::new();
        walk(
            self.root(),
            &self.config.notes_dir,
            &self.config,
            &attachments,
            &mut paths,
        )?;
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn links_in(note_path: &str, source: &str, is_note: impl Fn(&str) -> bool) -> Vec<Link> {
    markdown::parse(source)
        .into_iter()
        .map(|raw| {
            let note = markdown::resolve(note_path, &raw.target)
                .filter(|resolved| is_note(&resolved.path))
                .map(|resolved| resolved.path);

            Link {
                target: raw.target,
                note,
            }
        })
        .collect()
}

fn walk(
    root: &Path,
    relative: &Path,
    config: &crate::config::VaultConfig,
    attachments: &str,
    out: &mut Vec<String>,
) -> Result<()> {
    let directory = root.join(relative);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        // A configured directory that does not exist is not an error here:
        // `open` already reports it.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(VaultError::io(&directory, error)),
    };

    for entry in entries {
        let entry = entry.map_err(|e| VaultError::io(&directory, e))?;
        let file_type = entry
            .file_type()
            .map_err(|e| VaultError::io(entry.path(), e))?;
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let path = relative.join(&name);

        if file_type.is_dir() {
            if is_ignored(&name, &path, config) || to_display(&path) == attachments {
                continue;
            }
            walk(root, &path, config, attachments, out)?;
        } else if file_type.is_file() && is_note(&name, config) {
            out.push(to_display(&path));
        }
    }

    Ok(())
}

fn is_ignored(name: &OsStr, relative: &Path, config: &crate::config::VaultConfig) -> bool {
    let name = name.to_string_lossy();
    if name == ".git" || name == ".calmar" {
        return true;
    }

    let relative = to_display(relative);
    config.ignored_dirs.iter().any(|ignored| {
        ignored.eq_ignore_ascii_case(&name) || relative.eq_ignore_ascii_case(ignored)
    })
}

fn is_note(name: &OsStr, config: &crate::config::VaultConfig) -> bool {
    let Some(extension) = Path::new(name).extension() else {
        return false;
    };
    let extension = format!(".{}", extension.to_string_lossy().to_ascii_lowercase());

    config
        .extensions
        .iter()
        .any(|configured| configured.to_ascii_lowercase() == extension)
}
