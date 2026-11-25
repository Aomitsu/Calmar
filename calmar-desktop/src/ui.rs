//! Styles shared by the welcome screen and the vault panels.

use iced::widget::container;
use iced::{Border, Color, Theme};

/// A card or panel: subtle background, rounded border.
pub fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();

    container::Style {
        background: Some(palette.background.weak.color.into()),
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..container::Style::default()
    }
}

/// Dims the screen behind a modal.
pub fn backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.35).into()),
        ..container::Style::default()
    }
}
