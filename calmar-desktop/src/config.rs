//! Where Calmar keeps its machine-local state.
//!
//! Recent vaults and preferences belong to the machine, not to a vault, so
//! they live under `$XDG_CONFIG_HOME/calmar` (or `~/.config/calmar`).

use std::path::PathBuf;

pub fn file(name: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));

    base.join("calmar").join(name)
}
