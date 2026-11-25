//! Turning user input into a path inside the vault.
//!
//! Kept out of [`crate::Vault`] because these rules are security-relevant and
//! must be testable without creating a repository.

use std::path::{Component, Path, PathBuf};

use crate::error::{Result, VaultError};

/// A vault-relative path, normalized and guaranteed not to escape the vault.
pub(crate) struct RelPath {
    path: PathBuf,
    display: String,
}

impl RelPath {
    /// Rejects absolute paths, `..`, and empty paths.
    pub(crate) fn parse(input: &Path) -> Result<Self> {
        let mut path = PathBuf::new();

        for component in input.components() {
            match component {
                Component::Normal(part) => path.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(VaultError::InvalidPath {
                        path: input.to_path_buf(),
                        reason: "must be a relative path inside the vault".to_owned(),
                    });
                }
            }
        }

        if path.as_os_str().is_empty() {
            return Err(VaultError::InvalidPath {
                path: input.to_path_buf(),
                reason: "path is empty".to_owned(),
            });
        }

        // Always `/`, on every OS: this string ends up in commit messages.
        let display = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        Ok(Self { path, display })
    }

    pub(crate) fn display(&self) -> &str {
        &self.display
    }

    pub(crate) fn absolute(&self, root: &Path) -> PathBuf {
        root.join(&self.path)
    }
}

/// Formats a vault-relative path with `/` separators.
///
/// `./a/b.md` becomes `a/b.md`. Used for display, commit messages, and
/// comparisons, so the same note always has the same string on every OS.
pub(crate) fn to_display(relative: &Path) -> String {
    relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Absolute, symlink-resolved vault root.
pub(crate) fn canonical_root(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).map_err(|e| VaultError::io(path, e))?;
    dunce::canonicalize(&absolute).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => VaultError::NotFound {
            path: path.to_path_buf(),
        },
        _ => VaultError::io(path, e),
    })
}

/// Joins a directory declared in the config, refusing values that escape.
pub(crate) fn config_dir(root: &Path, relative: &Path, field: &'static str) -> Result<PathBuf> {
    let escapes = relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });

    if escapes {
        return Err(VaultError::Config {
            path: root.join(crate::config::CONFIG_PATH),
            message: format!(
                "{field} must stay inside the vault, got {}",
                relative.display()
            ),
        });
    }

    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Result<RelPath> {
        RelPath::parse(Path::new(input))
    }

    #[test]
    fn accepts_nested_relative_paths() {
        let path = parse("projets/calmar/notes.md").unwrap();
        assert_eq!(path.display(), "projets/calmar/notes.md");
    }

    #[test]
    fn normalizes_current_dir() {
        let path = parse("./projets/./notes.md").unwrap();
        assert_eq!(path.display(), "projets/notes.md");
    }

    #[test]
    fn rejects_traversal() {
        assert!(matches!(
            parse("../escape.md"),
            Err(VaultError::InvalidPath { .. })
        ));
        assert!(matches!(
            parse("a/../../escape.md"),
            Err(VaultError::InvalidPath { .. })
        ));
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(matches!(
            parse("/etc/escape.md"),
            Err(VaultError::InvalidPath { .. })
        ));
    }

    #[test]
    fn rejects_empty_paths() {
        assert!(matches!(parse(""), Err(VaultError::InvalidPath { .. })));
        assert!(matches!(parse("."), Err(VaultError::InvalidPath { .. })));
    }
}
