//! Calmar desktop.
//!
//! One window: choose a vault, then edit its notes.
//!
//! ```sh
//! cargo run -p calmar-desktop                 # show the vault chooser
//! cargo run -p calmar-desktop -- /tmp/notes   # open this vault directly
//! ```

mod app;
mod config;
mod recent;
mod settings;
mod ui;
mod workspace;

use std::path::PathBuf;

use app::App;
use recent::RecentVaults;
use settings::Settings;

fn main() -> iced::Result {
    iced::application(boot, App::update, App::view)
        .subscription(App::subscription)
        .font(iced_fonts::LUCIDE_FONT_BYTES)
        .theme(|app: &App| app.theme())
        .title("Calmar")
        .window_size((1100.0, 720.0))
        .centered()
        .run()
}

fn boot() -> App {
    let vault = std::env::args().nth(1).map(PathBuf::from);
    let (settings, settings_path) = Settings::load();

    App::boot(RecentVaults::load(), settings, settings_path, vault)
}
