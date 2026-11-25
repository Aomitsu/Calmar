//! Finding notes and folders in the vault, and resolving links between notes.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use crate::config::VaultConfig;
use crate::error::{Result, VaultError};
use crate::markdown;
use crate::note::{Link, Note};
use crate::path::{RelPath, to_display};

use super::Vault;

/// Everything found under the notes directory.
#[derive(Debug, Default)]
struct Scan {
    notes: Vec<String>,
    folders: Vec<String>,
}

impl Vault {
    /// Every note, sorted by path.
    ///
    /// Only files under `notes_dir` whose extension is listed in the config
    /// are returned. Directories named in `ignored_dirs` are skipped, as are
    /// `.git`, `.calmar`, and the attachments directory.
    pub fn list(&self) -> Result<Vec<Note>> {
        let scan = self.scan()?;
        let known: HashSet<&str> = scan.notes.iter().map(String::as_str).collect();

        let mut notes = Vec::with_capacity(scan.notes.len());
        for path in &scan.notes {
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

    /// Folders under `notes_dir`, including `notes_dir` itself and its
    /// ancestors, so the result can be displayed as a tree.
    ///
    /// Empty folders are included: they exist on disk even though Git does not
    /// track them.
    pub fn folders(&self) -> Result<Vec<String>> {
        Ok(self.scan()?.folders)
    }

    /// Vault-relative paths of every note, sorted.
    pub(crate) fn note_paths(&self) -> Result<Vec<String>> {
        Ok(self.scan()?.notes)
    }

    fn scan(&self) -> Result<Scan> {
        let attachments =
            crate::path::config_dir(self.root(), &self.config.attachments_dir, "attachments_dir")?;
        let attachments = attachments
            .strip_prefix(self.root())
            .map(to_display)
            .unwrap_or_default();

        let mut scan = Scan::default();
        walk(
            self.root(),
            &self.config.notes_dir,
            &self.config,
            &attachments,
            &mut scan,
        )?;

        // `walk` starts inside `notes_dir`, so the directory itself has to be
        // added back when it is not the vault root.
        let notes_dir = to_display(&self.config.notes_dir);
        if !notes_dir.is_empty() && notes_dir != "." {
            scan.folders.push(notes_dir);
        }

        add_ancestors(&mut scan.folders);
        scan.notes.sort();
        scan.notes.dedup();
        scan.folders.sort();
        scan.folders.dedup();
        Ok(scan)
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
    config: &VaultConfig,
    attachments: &str,
    out: &mut Scan,
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
            out.folders.push(to_display(&path));
            walk(root, &path, config, attachments, out)?;
        } else if file_type.is_file() && is_note(&name, config) {
            out.notes.push(to_display(&path));
        }
    }

    Ok(())
}

/// Adds missing parents, so `a/b/c` always comes with `a` and `a/b`.
fn add_ancestors(folders: &mut Vec<String>) {
    let mut extra = Vec::new();

    for folder in folders.iter() {
        let mut current = folder.as_str();
        while let Some((parent, _)) = current.rsplit_once('/') {
            extra.push(parent.to_owned());
            current = parent;
        }
    }

    folders.extend(extra);
}

fn is_ignored(name: &OsStr, relative: &Path, config: &VaultConfig) -> bool {
    let name = name.to_string_lossy();
    if name == ".git" || name == ".calmar" {
        return true;
    }

    let relative = to_display(relative);
    config.ignored_dirs.iter().any(|ignored| {
        ignored.eq_ignore_ascii_case(&name) || relative.eq_ignore_ascii_case(ignored)
    })
}

fn is_note(name: &OsStr, config: &VaultConfig) -> bool {
    let Some(extension) = Path::new(name).extension() else {
        return false;
    };
    let extension = format!(".{}", extension.to_string_lossy().to_ascii_lowercase());

    config
        .extensions
        .iter()
        .any(|configured| configured.to_ascii_lowercase() == extension)
}
