//! User preferences, stored in `~/.config/calmar/settings.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Clicking the rendered note switches back to the source editor.
    pub click_to_edit: bool,
    /// Display name of the theme, from [`THEME_NAMES`].
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            click_to_edit: true,
            theme: "Dark".to_owned(),
        }
    }
}

/// The themes offered in the settings, as human-readable names.
pub const THEME_NAMES: &[&str] = &[
    "Dark",
    "Light",
    "Dracula",
    "Nord",
    "Gruvbox Dark",
    "Solarized Dark",
    "Catppuccin Mocha",
    "Tokyo Night",
];

/// Maps a display name to the corresponding iced theme. Unknown names fall
/// back to Dark, so a hand-edited settings file cannot break startup.
pub fn theme_from_name(name: &str) -> iced::Theme {
    match name {
        "Light" => iced::Theme::Light,
        "Dracula" => iced::Theme::Dracula,
        "Nord" => iced::Theme::Nord,
        "Gruvbox Dark" => iced::Theme::GruvboxDark,
        "Solarized Dark" => iced::Theme::SolarizedDark,
        "Catppuccin Mocha" => iced::Theme::CatppuccinMocha,
        "Tokyo Night" => iced::Theme::TokyoNight,
        _ => iced::Theme::Dark,
    }
}

impl Settings {
    /// Loads the settings and returns the path they were read from, so changes
    /// can be written back to the same place.
    pub fn load() -> (Self, PathBuf) {
        let path = crate::config::file("settings.toml");
        (Self::load_from(&path), path)
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Failing to save a preference must not interrupt the user.
    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            eprintln!("calmar: could not create settings directory: {error}");
            return;
        }

        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(error) = std::fs::write(path, text) {
                    eprintln!("calmar: could not save settings: {error}");
                }
            }
            Err(error) => eprintln!("calmar: could not serialize settings: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn click_to_edit_is_on_by_default() {
        let directory = tempdir().unwrap();

        let settings = Settings::load_from(&directory.path().join("missing.toml"));

        assert!(settings.click_to_edit);
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.toml");

        Settings {
            click_to_edit: false,
            theme: "Dark".to_owned(),
        }
        .save_to(&path);

        assert!(!Settings::load_from(&path).click_to_edit);
    }

    #[test]
    fn theme_defaults_to_dark_and_falls_back_gracefully() {
        assert!(matches!(theme_from_name("Dark"), iced::Theme::Dark));
        assert!(matches!(theme_from_name("not-a-theme"), iced::Theme::Dark));
        assert!(matches!(theme_from_name("Nord"), iced::Theme::Nord));
        assert_eq!(Settings::default().theme, "Dark");
    }

    #[test]
    fn the_theme_round_trips_through_the_file() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.toml");

        Settings {
            click_to_edit: true,
            theme: "Dracula".to_owned(),
        }
        .save_to(&path);

        assert_eq!(Settings::load_from(&path).theme, "Dracula");
    }

    #[test]
    fn a_broken_file_falls_back_to_defaults() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        std::fs::write(&path, "this is not toml").unwrap();

        assert!(Settings::load_from(&path).click_to_edit);
    }
}
