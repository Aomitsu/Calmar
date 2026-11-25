//! The application window: choose a vault, then edit it.
//!
//! The chooser is a single card: two actions and the recent vaults list, in
//! the spirit of KeePassXC's welcome screen. The chooser and the vault screen
//! are two states of one struct, so switching stays testable without a window.

use std::path::{Path, PathBuf};

use calmar::Vault;
use iced::widget::tooltip::Position;
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, pick_list, row, stack, text, tooltip,
};
use iced::{Alignment, Element, Length, Task, Theme};
use iced_fonts::lucide;

use crate::recent::RecentVaults;
use crate::settings::{Settings, THEME_NAMES, theme_from_name};
use crate::ui::{backdrop_style, card_style};
use crate::workspace::{self, Workspace};

#[derive(Debug)]
pub struct App {
    recents: RecentVaults,
    settings: Settings,
    settings_path: PathBuf,
    theme: Theme,
    screen: Screen,
    error: Option<String>,
    settings_open: bool,
}

#[derive(Debug, Default)]
enum Screen {
    #[default]
    Chooser,
    Vault(Box<Workspace>),
}

#[derive(Debug, Clone)]
pub enum Message {
    NewVault,
    OpenVault,
    Open(PathBuf),
    Workspace(workspace::Message),
    OpenSettings,
    CloseSettings,
    ToggleClickToEdit,
    ThemeSelected(String),
    Key(iced::keyboard::Key, iced::keyboard::Modifiers),
}

impl Default for App {
    /// Loading the recent vaults and settings is disk IO, fine once at start.
    fn default() -> Self {
        let (settings, settings_path) = Settings::load();
        Self::new(RecentVaults::load(), settings, settings_path)
    }
}

impl App {
    pub fn new(recents: RecentVaults, settings: Settings, settings_path: PathBuf) -> Self {
        let theme = theme_from_name(&settings.theme);

        Self {
            recents,
            settings,
            settings_path,
            theme,
            screen: Screen::default(),
            error: None,
            settings_open: false,
        }
    }

    /// Builds the application, opening `vault` when one is given on the
    /// command line.
    pub fn boot(
        recents: RecentVaults,
        settings: Settings,
        settings_path: PathBuf,
        vault: Option<PathBuf>,
    ) -> Self {
        let mut app = Self::new(recents, settings, settings_path);

        if let Some(path) = vault {
            app.open(&path, true);
        }

        app
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NewVault => self.pick_vault(true),
            Message::OpenVault => self.pick_vault(false),
            Message::Open(path) => self.open(&path, false),
            Message::Workspace(message) => return self.workspace_message(message),
            Message::OpenSettings => self.settings_open = true,
            Message::CloseSettings => self.settings_open = false,
            Message::ToggleClickToEdit => self.toggle_click_to_edit(),
            Message::ThemeSelected(name) => self.select_theme(name),
            Message::Key(key, modifiers) => {
                if let Some(message) = shortcut(&key, modifiers, self.settings_open) {
                    return self.update(message);
                }
            }
        }

        Task::none()
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        // The subscription must not capture state, so the decision is made in
        // `update`, where `settings_open` is available.
        iced::keyboard::listen().filter_map(|event| match event {
            iced::keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::Key(key, modifiers))
            }
            _ => None,
        })
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base = column![self.screen_view(), self.status_bar()].height(Length::Fill);

        if !self.settings_open {
            return base.into();
        }

        // Modal settings: a dimmed backdrop that closes on click, and the card
        // above it. `opaque` keeps clicks on the card from reaching the
        // backdrop.
        let backdrop = opaque(
            mouse_area(
                container(Space::new().width(Length::Fill).height(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(backdrop_style),
            )
            .on_press(Message::CloseSettings),
        );

        let panel = container(opaque(
            container(self.settings_card())
                .padding(24)
                .max_width(440.0)
                .style(card_style),
        ))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24);

        stack([base.into(), backdrop, panel.into()]).into()
    }

    fn settings_card(&self) -> Element<'_, Message> {
        let theme_options: Vec<String> = THEME_NAMES.iter().map(|name| name.to_string()).collect();
        let label = if self.settings.click_to_edit {
            "On"
        } else {
            "Off"
        };
        let style = if self.settings.click_to_edit {
            button::primary
        } else {
            button::secondary
        };

        column![
            text("Settings").size(18),
            row![
                column![
                    text("Theme").size(13),
                    text("Applies to the editor, preview, and syntax colors.").size(11),
                ]
                .spacing(2)
                .width(Length::Fill),
                pick_list(
                    theme_options,
                    Some(self.settings.theme.clone()),
                    Message::ThemeSelected,
                ),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            row![
                column![
                    text("Click rendered note to edit").size(13),
                    text("Clicking the preview returns to the source editor.").size(11),
                ]
                .spacing(2)
                .width(Length::Fill),
                button(text(label).size(12))
                    .style(style)
                    .padding([6, 16])
                    .on_press(Message::ToggleClickToEdit),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text(format!("Stored in {}", self.settings_path.display())).size(10),
            row![
                Space::new().width(Length::Fill),
                button("Close").on_press(Message::CloseSettings),
            ],
        ]
        .spacing(16)
        .into()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let vault = match &self.screen {
            Screen::Vault(workspace) => workspace.root().display().to_string(),
            Screen::Chooser => "No vault open".to_owned(),
        };

        row![
            text(vault).size(11),
            Space::new().width(Length::Fill),
            tooltip(
                button(lucide::settings().size(12))
                    .style(button::text)
                    .padding(2)
                    .on_press(Message::OpenSettings),
                text("Settings").size(11),
                Position::Top,
            ),
        ]
        .spacing(6)
        .padding([2, 8])
        .into()
    }

    fn screen_view(&self) -> Element<'_, Message> {
        match &self.screen {
            Screen::Chooser => self.welcome(),
            Screen::Vault(workspace) => workspace.view().map(Message::Workspace),
        }
    }

    fn welcome(&self) -> Element<'_, Message> {
        let mut card = column![text("Calmar").size(20), text("Recent vaults").size(11)].spacing(10);

        if self.recents.entries().is_empty() {
            card = card.push(text("No recent vault.").size(12));
        } else {
            for path in self.recents.entries() {
                card = card.push(self.recent_row(path));
            }
        }

        if let Some(error) = &self.error {
            card = card.push(text(error).size(12));
        }

        card = card.push(
            row![
                action_button(lucide::file_plus(), "New vault", Message::NewVault),
                action_button(lucide::folder_open(), "Open vault", Message::OpenVault),
            ]
            .spacing(8)
            .width(Length::Fill),
        );

        let panel = container(card)
            .padding(24)
            .max_width(520.0)
            .width(Length::Fill)
            .style(card_style);

        container(panel)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .padding(24)
            .into()
    }

    fn recent_row(&self, path: &Path) -> Element<'_, Message> {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        let content = row![
            column![
                text(name).size(13),
                text(path.display().to_string()).size(10),
            ]
            .spacing(1)
            .width(Length::Fill),
        ]
        .padding(6);

        button(content)
            .width(Length::Fill)
            .padding(2)
            .style(button::text)
            .on_press(Message::Open(path.to_path_buf()))
            .into()
    }

    fn pick_vault(&mut self, create: bool) {
        let title = if create {
            "New vault: choose an empty folder"
        } else {
            "Open vault: choose a folder"
        };

        if let Some(folder) = rfd::FileDialog::new().set_title(title).pick_folder() {
            self.open(&folder, create);
        }
    }

    /// Opens an existing vault, or creates one when `create` is true.
    fn open(&mut self, path: &Path, create: bool) {
        if create && !is_creatable(path) {
            self.error = Some(format!(
                "{} is not empty. Choose an empty folder, or open it instead.",
                path.display()
            ));
            return;
        }

        let result = if create {
            Vault::create(path)
        } else {
            Vault::open(path)
        };

        match result {
            Ok(vault) => {
                // `root()` is canonical, so the same vault always gets the same
                // entry, even through a relative path or a symlink.
                self.recents.remember(vault.root().to_path_buf());

                let mut workspace = Workspace::new(vault);
                workspace.set_click_to_edit(self.settings.click_to_edit);
                workspace.set_theme(self.theme.clone());

                self.screen = Screen::Vault(Box::new(workspace));
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Forwards a vault message, except `Close`, which changes screens.
    fn workspace_message(&mut self, message: workspace::Message) -> Task<Message> {
        if matches!(message, workspace::Message::Close) {
            self.screen = Screen::Chooser;
            self.error = None;
            // A vault may have been deleted while it was open.
            self.recents.prune();
            return Task::none();
        }

        match &mut self.screen {
            Screen::Vault(workspace) => workspace.update(message).map(Message::Workspace),
            Screen::Chooser => Task::none(),
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn select_theme(&mut self, name: String) {
        self.settings.theme = name;
        self.settings.save_to(&self.settings_path);
        self.theme = theme_from_name(&self.settings.theme);

        if let Screen::Vault(workspace) = &mut self.screen {
            workspace.set_theme(self.theme.clone());
        }
    }

    fn toggle_click_to_edit(&mut self) {
        self.settings.click_to_edit = !self.settings.click_to_edit;
        self.settings.save_to(&self.settings_path);

        if let Screen::Vault(workspace) = &mut self.screen {
            workspace.set_click_to_edit(self.settings.click_to_edit);
        }
    }
}

/// Creates a vault only in an empty folder, or in one that already is a vault.
fn is_creatable(path: &Path) -> bool {
    if path.join(".git").exists() {
        return true;
    }

    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        // A missing folder is fine: `Vault::create` will make it.
        Err(_) => true,
    }
}

fn action_button<'a>(
    icon: impl Into<Element<'a, Message>>,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    let icon: Element<'a, Message> = icon.into();

    button(
        row![icon, text(label).size(13)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([9, 12])
    .style(button::secondary)
    .on_press(message)
    .into()
}

/// Keyboard shortcuts that work everywhere in the window.
fn shortcut(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    settings_open: bool,
) -> Option<Message> {
    use iced::keyboard::key::Named;

    match key {
        // The modal closes first; whatever is behind it keeps its state.
        iced::keyboard::Key::Named(Named::Escape) if settings_open => Some(Message::CloseSettings),
        iced::keyboard::Key::Named(Named::Escape) => {
            Some(Message::Workspace(workspace::Message::Escape))
        }
        iced::keyboard::Key::Character(character)
            if !settings_open && modifiers.command() && character.eq_ignore_ascii_case("s") =>
        {
            Some(Message::Workspace(workspace::Message::Save))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The runtime would execute the returned task; tests only check state.
    fn send(app: &mut App, message: Message) {
        let _ = app.update(message);
    }

    fn app_with_recent_file() -> (tempfile::TempDir, App) {
        let directory = tempdir().unwrap();
        let recents = RecentVaults::load_from(directory.path().join("recent-vaults"));
        let settings_path = directory.path().join("settings.toml");

        (
            directory,
            App::new(recents, Settings::default(), settings_path),
        )
    }

    /// Creates a vault through the same code path as the welcome button.
    fn create_vault(app: &mut App, directory: &tempfile::TempDir) -> PathBuf {
        let vault = directory.path().join("vault");
        app.open(&vault, true);
        vault
    }

    #[test]
    fn both_screens_can_build_their_widgets() {
        let (directory, mut app) = app_with_recent_file();
        let _ = app.view();

        create_vault(&mut app, &directory);
        let _ = app.view();
    }

    #[test]
    fn a_vault_given_at_startup_is_opened_directly() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("vault");
        let recents = RecentVaults::load_from(directory.path().join("recent-vaults"));
        let settings_path = directory.path().join("settings.toml");

        let app = App::boot(
            recents,
            Settings::default(),
            settings_path,
            Some(path.clone()),
        );

        match &app.screen {
            Screen::Vault(workspace) => assert_eq!(workspace.root(), path.as_path()),
            Screen::Chooser => panic!("expected the vault screen"),
        }
    }

    #[test]
    fn creating_a_vault_opens_it_and_remembers_it() {
        let (directory, mut app) = app_with_recent_file();

        let vault = create_vault(&mut app, &directory);

        match &app.screen {
            Screen::Vault(workspace) => assert_eq!(workspace.root(), vault.as_path()),
            Screen::Chooser => panic!("expected the vault screen"),
        }
        assert_eq!(app.recents.entries(), [vault]);
        assert!(app.error.is_none());
    }

    #[test]
    fn opening_a_missing_vault_reports_an_error() {
        let (directory, mut app) = app_with_recent_file();

        send(&mut app, Message::Open(directory.path().join("missing")));

        assert!(matches!(app.screen, Screen::Chooser));
        assert!(app.error.is_some(), "the user must see why it failed");
    }

    #[test]
    fn a_non_empty_folder_is_not_initialised_as_a_vault() {
        let (directory, mut app) = app_with_recent_file();
        let folder = directory.path().join("not-a-vault");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("keep.txt"), "precious").unwrap();

        app.open(&folder, true);

        assert!(matches!(app.screen, Screen::Chooser));
        assert!(app.error.is_some());
        assert!(!folder.join(".git").exists());
        assert_eq!(
            std::fs::read_to_string(folder.join("keep.txt")).unwrap(),
            "precious"
        );
    }

    #[test]
    fn closing_the_vault_returns_to_the_chooser() {
        let (directory, mut app) = app_with_recent_file();
        create_vault(&mut app, &directory);

        send(&mut app, Message::Workspace(workspace::Message::Close));

        assert!(matches!(app.screen, Screen::Chooser));
    }

    #[test]
    fn a_previous_vault_can_be_opened_from_the_list() {
        let (directory, mut app) = app_with_recent_file();
        let vault = create_vault(&mut app, &directory);
        send(&mut app, Message::Workspace(workspace::Message::Close));

        // The list is read back from disk, not from memory.
        let recents = RecentVaults::load_from(directory.path().join("recent-vaults"));
        let mut app = App::new(
            recents,
            Settings::default(),
            directory.path().join("settings.toml"),
        );
        assert_eq!(app.recents.entries(), std::slice::from_ref(&vault));

        send(&mut app, Message::Open(vault.clone()));

        match &app.screen {
            Screen::Vault(open) => assert_eq!(open.root(), vault.as_path()),
            Screen::Chooser => panic!("expected the vault screen"),
        }
    }

    #[test]
    fn the_gear_opens_and_closes_the_settings_panel() {
        let (_directory, mut app) = app_with_recent_file();
        assert!(!app.settings_open);

        send(&mut app, Message::OpenSettings);
        assert!(app.settings_open);

        // The panel is rendered above either screen.
        let _ = app.view();

        send(&mut app, Message::CloseSettings);
        assert!(!app.settings_open);
    }

    #[test]
    fn the_settings_panel_shows_the_click_setting() {
        let (directory, mut app) = app_with_recent_file();
        create_vault(&mut app, &directory);
        send(&mut app, Message::OpenSettings);

        send(&mut app, Message::ToggleClickToEdit);

        assert!(!app.settings.click_to_edit);
        assert!(!Settings::load_from(&directory.path().join("settings.toml")).click_to_edit);

        // Still open after a toggle.
        assert!(app.settings_open);
    }

    #[test]
    fn selecting_a_theme_is_saved_and_applied() {
        let (directory, mut app) = app_with_recent_file();
        assert!(matches!(app.theme(), iced::Theme::Dark));

        send(&mut app, Message::ThemeSelected("Dracula".into()));

        assert!(matches!(app.theme(), iced::Theme::Dracula));
        assert_eq!(
            Settings::load_from(&directory.path().join("settings.toml")).theme,
            "Dracula"
        );
    }

    #[test]
    fn the_click_to_edit_setting_is_toggled_and_saved() {
        let (directory, mut app) = app_with_recent_file();
        assert!(app.settings.click_to_edit);

        send(&mut app, Message::ToggleClickToEdit);

        assert!(!app.settings.click_to_edit);
        assert!(!Settings::load_from(&directory.path().join("settings.toml")).click_to_edit);
    }

    #[test]
    fn escape_cancels_creation_and_ctrl_s_saves() {
        use iced::keyboard::{Key, Modifiers, key::Named};

        assert!(matches!(
            shortcut(&Key::Named(Named::Escape), Modifiers::default(), false),
            Some(Message::Workspace(workspace::Message::Escape))
        ));
        assert!(matches!(
            shortcut(&Key::Character("s".into()), Modifiers::COMMAND, false),
            Some(Message::Workspace(workspace::Message::Save))
        ));
        assert!(shortcut(&Key::Character("s".into()), Modifiers::default(), false).is_none());

        // With the settings modal open, Escape closes it and Ctrl+S is inert.
        assert!(matches!(
            shortcut(&Key::Named(Named::Escape), Modifiers::default(), true),
            Some(Message::CloseSettings)
        ));
        assert!(shortcut(&Key::Character("s".into()), Modifiers::COMMAND, true).is_none());
    }
}
