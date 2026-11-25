//! Vault configuration, stored at `.calmar/config.toml`.
//!
//! The file is part of the vault, so it is committed with the notes and
//! follows the folder from one machine to another. It is optional: a vault
//! without config simply uses [`VaultConfig::default`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, VaultError};

/// Path of the config file, relative to the vault root.
pub const CONFIG_PATH: &str = ".calmar/config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VaultConfig {
    /// Where notes live, relative to the vault root. `"."` is the root itself.
    ///
    /// Vault file operations always take vault-relative paths; this setting
    /// only tells the (future) scanner where to look.
    pub notes_dir: PathBuf,

    /// Where images and other attachments are stored, relative to the root.
    pub attachments_dir: PathBuf,

    /// Extra directories the scanner must skip. `.git` and `.calmar` are
    /// always skipped and cannot be removed from here.
    pub ignored_dirs: Vec<String>,

    /// File extensions considered as notes. Compared case-insensitively.
    pub extensions: Vec<String>,

    /// Identity used for commits. Falls back to the global git config.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    pub name: String,
    pub email: String,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            attachments_dir: PathBuf::from("assets"),
            ignored_dirs: vec![".git".to_owned(), ".calmar".to_owned()],
            extensions: vec![".md".to_owned()],
            author: None,
        }
    }
}

impl VaultConfig {
    /// Reads the config, falling back to defaults when the file does not exist.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(CONFIG_PATH);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&path, &text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(VaultError::io(path, source)),
        }
    }

    /// Same as [`VaultConfig::load`], but writes the defaults on first use so a
    /// freshly created vault always contains a config file to edit.
    pub fn load_or_create(root: &Path) -> Result<Self> {
        let path = root.join(CONFIG_PATH);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&path, &text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save(root)?;
                Ok(config)
            }
            Err(source) => Err(VaultError::io(path, source)),
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(CONFIG_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VaultError::io(parent, e))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| VaultError::Config {
            path: path.clone(),
            message: e.to_string(),
        })?;
        std::fs::write(&path, text).map_err(|e| VaultError::io(path, e))
    }

    fn parse(path: &Path, text: &str) -> Result<Self> {
        let config: Self = toml::from_str(text).map_err(|e| VaultError::Config {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Rejects values that would silently point outside the vault.
    fn validate(&self) -> Result<()> {
        for dir in &self.ignored_dirs {
            let wrong = dir.is_empty() || dir.starts_with('/') || dir.split('/').any(|c| c == "..");
            if wrong {
                return Err(VaultError::Config {
                    path: PathBuf::from(CONFIG_PATH),
                    message: format!("ignored_dirs contains an invalid entry: {dir:?}"),
                });
            }
        }

        for ext in &self.extensions {
            if !ext.starts_with('.') || ext.len() < 2 {
                return Err(VaultError::Config {
                    path: PathBuf::from(CONFIG_PATH),
                    message: format!("extensions must look like \".md\", got {ext:?}"),
                });
            }
        }

        Ok(())
    }
}
