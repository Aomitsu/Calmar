//! The vault: a directory of Markdown files backed by a Git repository.
//!
//! Everything that mutates the vault goes through here, so the worktree and
//! the history can never drift apart: a file operation is always followed by a
//! commit describing it.
//!
//! All paths taken by this type are relative to the vault root. They are
//! validated by [`crate::path`] before touching the filesystem.

use std::path::{Path, PathBuf};

use crate::config::VaultConfig;
use crate::error::{Result, VaultError};
use crate::fs::write_atomic;
use crate::git::{CommitId, GitHistory};
use crate::path::{RelPath, canonical_root, config_dir};

mod rename;
mod scan;

/// Default `.gitignore` written on vault creation.
///
/// `*.tmp` also covers the temporary files used by [`Vault::write_file`].
const GITIGNORE: &str = "\
# Calmar
.calmar/cache/
*.tmp
.DS_Store
Thumbs.db
";

/// Default `.gitattributes`: the same bytes on every OS.
const GITATTRIBUTES: &str = "*.md text eol=lf\n";

#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
    config: VaultConfig,
    history: GitHistory,
}

impl Vault {
    /// Creates a vault, or opens it if it already exists.
    ///
    /// This is the only method allowed to create a repository. Missing files
    /// are written with their defaults; existing ones are left untouched.
    pub fn create(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref();
        std::fs::create_dir_all(path).map_err(|e| VaultError::io(path, e))?;
        let root = canonical_root(path)?;

        let config = VaultConfig::load_or_create(&root)?;

        let notes_dir = config_dir(&root, &config.notes_dir, "notes_dir")?;
        std::fs::create_dir_all(&notes_dir).map_err(|e| VaultError::io(&notes_dir, e))?;

        let attachments_dir = config_dir(&root, &config.attachments_dir, "attachments_dir")?;
        std::fs::create_dir_all(&attachments_dir)
            .map_err(|e| VaultError::io(&attachments_dir, e))?;

        // `create` is idempotent, so only a brand new repository gets an
        // initial commit; an existing one may have uncommitted user work.
        let fresh = !root.join(".git").exists();
        let mut history = if fresh {
            GitHistory::init(&root, config.author.as_ref())?
        } else {
            GitHistory::open(&root, config.author.as_ref())?
        };

        for (name, contents) in [(".gitignore", GITIGNORE), (".gitattributes", GITATTRIBUTES)] {
            let path = root.join(name);
            if !path.exists() {
                std::fs::write(&path, contents).map_err(|e| VaultError::io(&path, e))?;
            }
        }

        if fresh {
            history.commit_all("Initial commit")?;
        }

        Ok(Self {
            root,
            config,
            history,
        })
    }

    /// Opens an existing vault. Fails on a plain directory: use
    /// [`Vault::create`] to initialize one.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = canonical_root(root.as_ref())?;

        if !root.is_dir() {
            return Err(VaultError::NotADirectory { path: root });
        }
        // A `.git` directory is a normal repository; a `.git` file is a linked
        // worktree. Both are valid.
        if !root.join(".git").exists() {
            return Err(VaultError::NotAVault { path: root });
        }

        let config = VaultConfig::load(&root)?;

        let notes_dir = config_dir(&root, &config.notes_dir, "notes_dir")?;
        if !notes_dir.is_dir() {
            return Err(VaultError::MissingDirectory { path: notes_dir });
        }

        let history = GitHistory::open(&root, config.author.as_ref())?;

        Ok(Self {
            root,
            config,
            history,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    pub fn history(&self) -> &GitHistory {
        &self.history
    }

    /// Reads a note (or any text file) as UTF-8.
    pub fn read(&self, path: impl AsRef<Path>) -> Result<String> {
        let (_, absolute) = self.resolve(path.as_ref())?;
        if !absolute.is_file() {
            return Err(VaultError::NotAFile { path: absolute });
        }
        std::fs::read_to_string(&absolute).map_err(|e| VaultError::io(&absolute, e))
    }

    /// Creates or overwrites a file and commits the change.
    ///
    /// Parent directories are created as needed. The commit message is
    /// generated from the operation (`Add ...` / `Update ...`).
    /// Returns `None` when the content did not change: no empty commit.
    pub fn write_file(
        &mut self,
        path: impl AsRef<Path>,
        contents: &str,
    ) -> Result<Option<CommitId>> {
        let (rel, absolute) = self.resolve(path.as_ref())?;
        let existed = absolute.is_file();

        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VaultError::io(parent, e))?;
        }
        write_atomic(&absolute, contents)?;

        let action = if existed { "Update" } else { "Add" };
        self.history
            .commit_all(&format!("{action} {}", rel.display()))
    }

    /// Deletes a file and commits the deletion.
    pub fn delete_file(&mut self, path: impl AsRef<Path>) -> Result<Option<CommitId>> {
        let (rel, absolute) = self.resolve(path.as_ref())?;
        if !absolute.is_file() {
            return Err(VaultError::NotAFile { path: absolute });
        }
        std::fs::remove_file(&absolute).map_err(|e| VaultError::io(&absolute, e))?;
        self.history
            .commit_all(&format!("Delete {}", rel.display()))
    }

    /// Creates a directory, including its parents.
    ///
    /// Git does not track empty directories, so nothing is committed here: the
    /// directory shows up in the history as soon as it contains a file.
    pub fn create_dir(&mut self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let (_, absolute) = self.resolve(path.as_ref())?;
        std::fs::create_dir_all(&absolute).map_err(|e| VaultError::io(&absolute, e))?;
        Ok(absolute)
    }

    /// Deletes a directory and everything inside it, then commits.
    ///
    /// The vault root cannot be deleted: an empty path is rejected by
    /// [`RelPath`] before reaching the filesystem.
    pub fn delete_dir(&mut self, path: impl AsRef<Path>) -> Result<Option<CommitId>> {
        let (rel, absolute) = self.resolve(path.as_ref())?;
        if !absolute.is_dir() {
            return Err(VaultError::NotADirectory { path: absolute });
        }
        std::fs::remove_dir_all(&absolute).map_err(|e| VaultError::io(&absolute, e))?;
        self.history
            .commit_all(&format!("Delete {}", rel.display()))
    }

    /// Commits pending changes with a custom message.
    pub fn commit(&mut self, message: &str) -> Result<Option<CommitId>> {
        self.history.commit_all(message)
    }

    /// True when files changed outside Calmar and are not committed yet.
    ///
    /// [`Vault::commit`] puts them into the history. A UI that watches the
    /// filesystem uses this to decide when to do that.
    pub fn has_pending_changes(&self) -> Result<bool> {
        self.history.has_uncommitted_changes()
    }

    /// Validated path and its absolute counterpart.
    fn resolve(&self, path: &Path) -> Result<(RelPath, PathBuf)> {
        let rel = RelPath::parse(path)?;
        let absolute = rel.absolute(&self.root);
        Ok((rel, absolute))
    }
}
