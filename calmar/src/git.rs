//! The only module that knows about `git2`.
//!
//! Everything is expressed in vault terms ([`CommitId`], [`Commit`]); no
//! `git2` type leaves this module.

use std::path::Path;

use crate::config::Author;
use crate::error::{Result, VaultError};

/// A commit, identified by its SHA-1.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommitId(git2::Oid);

impl CommitId {
    pub fn to_hex(self) -> String {
        self.0.to_string()
    }
}

impl std::fmt::Debug for CommitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl std::fmt::Display for CommitId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A commit as the UI needs it.
#[derive(Debug, Clone)]
pub struct Commit {
    pub id: CommitId,
    /// First line of the message.
    pub summary: String,
    /// Full message, trailing newline removed.
    pub message: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
}

pub struct GitHistory {
    repo: git2::Repository,
    author_name: String,
    author_email: String,
}

impl std::fmt::Debug for GitHistory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHistory")
            .field("repo", &self.repo.path())
            .field(
                "author",
                &format_args!("{} <{}>", self.author_name, self.author_email),
            )
            .finish()
    }
}

impl GitHistory {
    /// Opens an existing repository. Never looks at parent directories: a
    /// vault nested inside another repository must not commit to that one.
    pub fn open(root: &Path, author: Option<&Author>) -> Result<Self> {
        let repo = git2::Repository::open(root).map_err(|_| VaultError::NotAVault {
            path: root.to_path_buf(),
        })?;
        Self::from_repo(repo, root, author)
    }

    /// Creates the repository (no-op if it already exists).
    pub fn init(root: &Path, author: Option<&Author>) -> Result<Self> {
        let repo = git2::Repository::init(root).map_err(VaultError::git)?;
        Self::from_repo(repo, root, author)
    }

    fn from_repo(repo: git2::Repository, root: &Path, author: Option<&Author>) -> Result<Self> {
        // `init` on a directory that is already a repository worktree is fine,
        // but anything else means we did not open what we think we did.
        if repo.workdir() != Some(root) {
            return Err(VaultError::NotAVault {
                path: root.to_path_buf(),
            });
        }

        let (author_name, author_email) = resolve_author(&repo, author);
        Ok(Self {
            repo,
            author_name,
            author_email,
        })
    }

    /// Stages the whole worktree and commits it.
    ///
    /// Returns `None` when the resulting tree is identical to the parent:
    /// saving an unchanged note must not create an empty commit.
    /// `.gitignore` is honoured by libgit2, so ignored files are never staged.
    pub fn commit_all(&mut self, message: &str) -> Result<Option<CommitId>> {
        let mut index = self.repo.index().map_err(VaultError::git)?;
        // Stages everything, deletions included: libgit2's add_all diffs the
        // index against the worktree, so a removed file disappears from the
        // index too. Ignored files are skipped unless FORCE is used.
        index
            .add_all(["."], git2::IndexAddOption::DEFAULT, None)
            .map_err(VaultError::git)?;
        index.write().map_err(VaultError::git)?;

        let tree_id = index.write_tree().map_err(VaultError::git)?;
        let parent = self.parent()?;

        if parent.as_ref().is_some_and(|p| p.tree_id() == tree_id) {
            return Ok(None);
        }

        let signature = self.signature()?;
        let tree = self.repo.find_tree(tree_id).map_err(VaultError::git)?;
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let oid = self
            .repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .map_err(VaultError::git)?;

        Ok(Some(CommitId(oid)))
    }

    /// Most recent commit, or `None` while the repository has no commit yet.
    pub fn last(&self) -> Result<Option<Commit>> {
        match self.parent()? {
            Some(commit) => Ok(Some(info(&commit))),
            None => Ok(None),
        }
    }

    /// True when the worktree differs from HEAD. Ignored files are excluded.
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let mut options = git2::StatusOptions::new();
        options.include_untracked(true).recurse_untracked_dirs(true);

        let statuses = self
            .repo
            .statuses(Some(&mut options))
            .map_err(VaultError::git)?;

        Ok(!statuses.is_empty())
    }

    /// Commits, newest first, at most `max` of them.
    pub fn log(&self, max: usize) -> Result<Vec<Commit>> {
        let mut walk = self.repo.revwalk().map_err(VaultError::git)?;
        if walk.push_head().is_err() {
            // Empty repository: HEAD points to a branch that does not exist yet.
            return Ok(Vec::new());
        }
        // TOPOLOGICAL is required, not decorative: a save can produce several
        // commits within the same second, and TIME alone leaves their order
        // undefined. Topological sorting always shows children before parents.
        walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
            .map_err(VaultError::git)?;

        let mut commits = Vec::new();
        for oid in walk.take(max) {
            let oid = oid.map_err(VaultError::git)?;
            let commit = self.repo.find_commit(oid).map_err(VaultError::git)?;
            commits.push(info(&commit));
        }
        Ok(commits)
    }

    /// Resolved identity, from the vault config or the global git config.
    fn signature(&self) -> Result<git2::Signature<'static>> {
        git2::Signature::now(&self.author_name, &self.author_email).map_err(VaultError::git)
    }

    /// Commit HEAD points to, `None` on an unborn branch.
    fn parent(&self) -> Result<Option<git2::Commit<'_>>> {
        match self.repo.head() {
            Ok(head) => Ok(Some(head.peel_to_commit().map_err(VaultError::git)?)),
            Err(err)
                if matches!(
                    err.code(),
                    git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound
                ) =>
            {
                Ok(None)
            }
            Err(err) => Err(VaultError::git(err)),
        }
    }
}

fn info(commit: &git2::Commit<'_>) -> Commit {
    let summary = commit
        .summary()
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_owned();
    let message = commit.message().unwrap_or_default().trim_end().to_owned();

    Commit {
        id: CommitId(commit.id()),
        summary,
        message,
        time: commit.time().seconds(),
    }
}

/// Vault config first, then the global git identity, then a stable fallback:
/// saving a note must never fail because git is not configured.
fn resolve_author(repo: &git2::Repository, configured: Option<&Author>) -> (String, String) {
    if let Some(author) = configured {
        return (author.name.clone(), author.email.clone());
    }

    match repo.signature() {
        Ok(signature) => {
            let (fallback_name, fallback_email) = default_author();
            (
                signature.name().unwrap_or(&fallback_name).to_owned(),
                signature.email().unwrap_or(&fallback_email).to_owned(),
            )
        }
        Err(_) => default_author(),
    }
}

/// Identity used when neither the vault config nor git provides one.
///
/// The package version is part of the name, so the history shows which
/// release of Calmar wrote a commit.
fn default_author() -> (String, String) {
    (
        format!("Calmar v{}", env!("CARGO_PKG_VERSION")),
        "calmar@localhost".to_owned(),
    )
}
