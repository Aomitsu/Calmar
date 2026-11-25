//! Error type shared by the whole crate.
//!
//! `git2` never appears here: the git layer converts its errors into
//! [`VaultError::Git`], so callers do not depend on the git binding.

use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, VaultError>;

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("vault not found at {path}")]
    NotFound { path: PathBuf },

    #[error("{path} is not a directory")]
    NotADirectory { path: PathBuf },

    #[error("no vault at {path}")]
    NotAVault { path: PathBuf },

    #[error("{path} is not a file")]
    NotAFile { path: PathBuf },

    #[error("{path} already exists")]
    AlreadyExists { path: PathBuf },

    /// A path that would escape the vault, or that is not usable as a note path.
    #[error("invalid path {path}: {reason}")]
    InvalidPath { path: PathBuf, reason: String },

    #[error("missing directory {path}")]
    MissingDirectory { path: PathBuf },

    #[error("can't access {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid config at {path}: {message}")]
    Config { path: PathBuf, message: String },

    #[error("git error: {message}")]
    Git { message: String },
}

impl VaultError {
    pub(crate) fn git(err: git2::Error) -> Self {
        Self::Git {
            message: err.message().to_owned(),
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
