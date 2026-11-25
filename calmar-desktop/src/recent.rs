//! Recently opened vaults, persisted as one path per line.
//!
//! The file lives in `$XDG_CONFIG_HOME/calmar/recent-vaults` (or
//! `~/.config/calmar/recent-vaults`). It is machine state, not vault state, so
//! it does not belong inside a vault.

use std::path::PathBuf;

/// How many vaults are remembered.
const MAX: usize = 8;

#[derive(Debug)]
pub struct RecentVaults {
    file: PathBuf,
    entries: Vec<PathBuf>,
}

impl RecentVaults {
    /// Loads the list from the default location.
    pub fn load() -> Self {
        Self::load_from(crate::config::file("recent-vaults"))
    }

    pub fn load_from(file: PathBuf) -> Self {
        let mut entries = read_entries(&file);
        let before = entries.len();
        entries.retain(|path| is_available(path));

        let recents = Self { file, entries };

        // Vaults that disappeared are removed from the file straight away, so
        // the next start does not have to prune them again.
        if recents.entries.len() != before {
            recents.save_quietly();
        }

        recents
    }

    /// Drops vaults that no longer exist, for the case where one is deleted
    /// while the application is running.
    pub fn prune(&mut self) {
        let before = self.entries.len();
        self.entries.retain(|path| is_available(path));

        if self.entries.len() != before {
            self.save_quietly();
        }
    }

    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    /// Puts `path` first, removes duplicates, and persists the list.
    ///
    /// Failing to save must not prevent opening a vault, so the write error is
    /// reported on stderr instead of being returned.
    pub fn remember(&mut self, path: PathBuf) {
        self.entries.retain(|entry| entry != &path);
        self.entries.insert(0, path);
        self.entries.truncate(MAX);
        self.save_quietly();
    }

    fn save_quietly(&self) {
        if let Err(error) = self.save() {
            eprintln!("calmar: could not save recent vaults: {error}");
        }
    }

    fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let text: String = self
            .entries
            .iter()
            .map(|path| format!("{}\n", path.display()))
            .collect();

        std::fs::write(&self.file, text)
    }
}

fn read_entries(file: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_to_string(file)
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// A vault is only usable when its repository is there.
fn is_available(path: &std::path::Path) -> bool {
    path.join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent-vaults");
        (dir, file)
    }

    fn fake_vault(root: &std::path::Path, name: &str) -> PathBuf {
        let path = root.join(name);
        std::fs::create_dir_all(path.join(".git")).unwrap();
        path
    }

    #[test]
    fn keeps_the_most_recent_first_and_removes_duplicates() {
        let (_dir, file) = temp_file();
        let mut recents = RecentVaults::load_from(file);

        recents.remember(PathBuf::from("/a"));
        recents.remember(PathBuf::from("/b"));
        recents.remember(PathBuf::from("/a"));

        assert_eq!(
            recents.entries(),
            [PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn caps_the_list() {
        let (_dir, file) = temp_file();
        let mut recents = RecentVaults::load_from(file);

        for index in 0..MAX + 4 {
            recents.remember(PathBuf::from(format!("/vault-{index}")));
        }

        assert_eq!(recents.entries().len(), MAX);
        assert_eq!(recents.entries()[0], PathBuf::from("/vault-11"));
    }

    #[test]
    fn round_trips_through_the_file() {
        let (directory, file) = temp_file();
        let first = fake_vault(directory.path(), "first");
        let second = fake_vault(directory.path(), "second");

        let mut recents = RecentVaults::load_from(file.clone());
        recents.remember(first.clone());
        recents.remember(second.clone());

        let reloaded = RecentVaults::load_from(file);

        assert_eq!(reloaded.entries(), [second, first]);
    }

    #[test]
    fn missing_vaults_are_dropped_on_load_and_forgotten() {
        let (directory, file) = temp_file();
        let kept = fake_vault(directory.path(), "kept");
        std::fs::write(
            &file,
            format!(
                "{}\n{}/does-not-exist\n",
                kept.display(),
                directory.path().display()
            ),
        )
        .unwrap();

        let recents = RecentVaults::load_from(file.clone());

        assert_eq!(recents.entries(), [kept]);
        // The file was rewritten, so the stale entry is gone for good.
        assert_eq!(RecentVaults::load_from(file).entries().len(), 1);
    }

    #[test]
    fn a_folder_without_a_repository_is_dropped() {
        let (directory, file) = temp_file();
        let plain = directory.path().join("plain-folder");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(&file, format!("{}\n", plain.display())).unwrap();

        assert!(RecentVaults::load_from(file).entries().is_empty());
    }

    #[test]
    fn prune_drops_a_vault_deleted_while_running() {
        let (directory, file) = temp_file();
        let vault = fake_vault(directory.path(), "gone");
        std::fs::write(&file, format!("{}\n", vault.display())).unwrap();

        let mut recents = RecentVaults::load_from(file);
        assert_eq!(recents.entries().len(), 1);

        std::fs::remove_dir_all(&vault).unwrap();
        recents.prune();

        assert!(recents.entries().is_empty());
    }

    #[test]
    fn missing_file_means_no_vault() {
        let (_dir, file) = temp_file();

        assert!(RecentVaults::load_from(file).entries().is_empty());
    }
}
