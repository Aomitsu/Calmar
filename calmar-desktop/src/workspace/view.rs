//! Widgets for the vault screen.
//!
//! Kept apart from the state in [`super`] so the operations stay readable.

use iced::widget::tooltip::Position;
use iced::widget::{
    Space, button, column, container, markdown, mouse_area, opaque, pane_grid, row, scrollable,
    stack, text, text_editor, text_input, tooltip,
};
use iced::{Alignment, Element, Length, Padding, Theme};
use iced_fonts::lucide;

use super::markdown_highlight::{self, MarkdownHighlighter};
use super::{Message, NewKind, Overlay, Pane, Selection, Workspace};
use crate::ui::{backdrop_style, card_style};
use crate::workspace::tree::Entry;

impl Workspace {
    pub fn view(&self) -> Element<'_, Message> {
        let base: Element<'_, Message> = pane_grid(&self.panes, |_pane, pane, _maximized| {
            pane_grid::Content::new(match pane {
                Pane::Explorer => self.explorer(),
                Pane::Editor if self.preview_only => self.preview(true),
                Pane::Editor => self.editor(),
                Pane::Preview => self.preview(false),
            })
        })
        .on_resize(8, Message::Resized)
        .spacing(4)
        .into();

        let Some(card) = self.overlay_card() else {
            return base;
        };

        // The backdrop closes the modal; `opaque` keeps clicks on the card from
        // reaching it.
        let backdrop = opaque(
            mouse_area(
                container(Space::new().width(Length::Fill).height(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(backdrop_style),
            )
            .on_press(Message::CancelOverlay),
        );

        let panel = container(opaque(
            container(card)
                .padding(24)
                .max_width(560.0)
                .style(card_style),
        ))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24);

        stack([base, backdrop, panel.into()]).into()
    }

    fn overlay_card(&self) -> Option<Element<'_, Message>> {
        match self.overlay.as_ref()? {
            Overlay::Delete(selection) => Some(self.delete_card(selection)),
            Overlay::History => Some(self.history_card()),
        }
    }

    fn delete_card(&self, selection: &Selection) -> Element<'_, Message> {
        let kind = if matches!(selection, Selection::Folder(_)) {
            "folder"
        } else {
            "note"
        };

        column![
            text(format!("Delete this {kind}?")).size(18),
            text(selection.path().to_owned()).size(12),
            row![
                Space::new().width(Length::Fill),
                button("Cancel").on_press(Message::CancelOverlay),
                button("Delete")
                    .style(button::danger)
                    .on_press(Message::ConfirmDelete),
            ]
            .spacing(8),
        ]
        .spacing(14)
        .into()
    }

    fn history_card(&self) -> Element<'_, Message> {
        let mut rows = column![].spacing(4);

        if self.history.is_empty() {
            rows = rows.push(text("No commits yet.").size(12));
        }

        for commit in &self.history {
            let short: String = commit.id.to_hex().chars().take(7).collect();

            rows = rows.push(
                row![
                    text(short).size(11),
                    text(commit.summary.as_str()).size(12).width(Length::Fill),
                    button("Restore")
                        .style(button::text)
                        .padding([2, 8])
                        .on_press(Message::Restore(commit.id)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        column![
            text("History").size(20),
            text("Restoring rewrites the notes to that commit and records a new commit.").size(11),
            scrollable(rows).height(Length::Fixed(360.0)),
            row![
                Space::new().width(Length::Fill),
                button("Close").on_press(Message::CancelOverlay),
            ],
        ]
        .spacing(14)
        .into()
    }

    fn editor(&self) -> Element<'_, Message> {
        let title = self.selected_note.as_deref().unwrap_or("No note selected");
        let dirty = if self.dirty { "*" } else { "" };

        column![
            row![
                text(title).size(13),
                Space::new().width(Length::Fill),
                text(dirty).size(13),
                action(
                    if self.preview_pane.is_some() {
                        lucide::eye_off().size(14)
                    } else {
                        lucide::eye().size(14)
                    },
                    "Toggle preview",
                    Message::TogglePreview,
                ),
                action(
                    lucide::book_open().size(14),
                    "Preview only",
                    Message::TogglePreviewOnly,
                ),
                button(row![lucide::save().size(13), text("Save").size(12)].spacing(4))
                    .style(button::text)
                    .padding(4)
                    .on_press(Message::Save),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .padding([4, 8]),
            text_editor(&self.content)
                .on_action(Message::Editor)
                .highlight_with::<MarkdownHighlighter>((), markdown_highlight::format)
                .height(Length::Fill),
        ]
        .spacing(4)
        .into()
    }

    fn preview(&self, with_edit_button: bool) -> Element<'_, Message> {
        let body = markdown::view(self.preview.items(), self.theme.clone()).map(Message::Link);

        let rendered = container(body).padding([4, 12]);

        // In read mode, a click anywhere on the note returns to the source,
        // unless the user turned that behaviour off.
        let rendered: Element<'_, Message> = if with_edit_button && self.click_to_edit {
            mouse_area(rendered).on_press(Message::Edit).into()
        } else {
            rendered.into()
        };

        if with_edit_button {
            column![
                row![
                    Space::new().width(Length::Fill),
                    action(
                        lucide::pencil().size(13),
                        "Back to editor",
                        Message::TogglePreviewOnly
                    ),
                ]
                .padding([4, 8]),
                scrollable(rendered).height(Length::Fill),
            ]
            .into()
        } else {
            scrollable(rendered).height(Length::Fill).into()
        }
    }

    fn explorer(&self) -> Element<'_, Message> {
        let header = row![
            text("VAULT").size(11),
            Space::new().width(Length::Fill),
            action(
                lucide::file_plus().size(13),
                "New note",
                Message::StartNew(NewKind::Note)
            ),
            action(
                lucide::folder_plus().size(13),
                "New folder",
                Message::StartNew(NewKind::Folder)
            ),
            action(
                lucide::calendar().size(13),
                "Daily note",
                Message::DailyNote
            ),
            action(lucide::pencil().size(13), "Rename", Message::StartRename),
            action(lucide::trash().size(13), "Delete", Message::AskDelete),
            action(lucide::history().size(13), "History", Message::OpenHistory),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .padding([6, 8]);

        let mut rows = column![].spacing(1).padding([2, 0]);

        if let Some(kind) = self.creating {
            rows = rows.push(self.new_entry_row(kind));
        }

        for entry in self.visible_tree() {
            rows = rows.push(self.tree_row(entry));
        }

        column![
            header,
            scrollable(rows).height(Length::Fill),
            row![
                container(text(self.status.as_deref().unwrap_or("")).size(11)).width(Length::Fill),
                action(lucide::refresh_cw().size(12), "Refresh", Message::Refresh),
            ]
            .padding([2, 8])
            .align_y(Alignment::Center),
            button(row![lucide::x().size(12), text("Close vault").size(12)].spacing(4))
                .style(button::text)
                .padding([4, 8])
                .on_press(Message::Close),
        ]
        .into()
    }

    fn tree_row(&self, entry: &Entry) -> Element<'_, Message> {
        if self
            .renaming
            .as_ref()
            .is_some_and(|selection| selection.path() == entry.path())
        {
            return self.rename_row(entry);
        }

        let indent = 4.0 + entry.depth() as f32 * 12.0;

        let chevron: Element<'_, Message> = if entry.is_folder() {
            let icon = if self.is_collapsed(entry.path()) {
                lucide::chevron_right()
            } else {
                lucide::chevron_down()
            };

            button(icon.size(14))
                .style(button::text)
                .padding(0)
                .width(Length::Fixed(18.0))
                .on_press(Message::ToggleFolder(entry.path().to_owned()))
                .into()
        } else {
            Space::new().width(Length::Fixed(18.0)).into()
        };

        let icon: Element<'_, Message> = if entry.is_folder() {
            Space::new().into()
        } else {
            lucide::file_text().size(13).into()
        };

        let selected = if entry.is_folder() {
            self.selected_folder == entry.path()
        } else {
            self.selected_note.as_deref() == Some(entry.path())
        };

        let select = if entry.is_folder() {
            Message::SelectFolder(entry.path().to_owned())
        } else {
            Message::SelectNote(entry.path().to_owned())
        };

        let style: fn(&Theme, button::Status) -> button::Style =
            if selected { selected_row } else { button::text };

        let label = button(
            row![icon, text(entry.name().to_owned()).size(13)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .style(style)
        .padding([3, 4])
        .width(Length::Fill)
        .on_press(select);

        container(row![chevron, label].spacing(2).align_y(Alignment::Center))
            .padding(Padding::ZERO.left(indent))
            .into()
    }

    fn rename_row(&self, entry: &Entry) -> Element<'_, Message> {
        let indent = 4.0 + entry.depth() as f32 * 12.0;
        let icon = if entry.is_folder() {
            lucide::folder()
        } else {
            lucide::file_text()
        };

        row![
            Space::new().width(Length::Fixed(18.0)),
            icon.size(14),
            text_input("name", &self.name)
                .on_input(Message::NameEdited)
                .on_submit(Message::SubmitName)
                .size(13),
        ]
        .spacing(6)
        .padding(Padding::ZERO.left(indent).top(4).bottom(4))
        .align_y(Alignment::Center)
        .into()
    }

    fn new_entry_row(&self, kind: NewKind) -> Element<'_, Message> {
        let icon = match kind {
            NewKind::Note => lucide::file_text(),
            NewKind::Folder => lucide::folder(),
        };

        // Indent the field under the folder it will create into.
        let depth = if self.selected_folder.is_empty() {
            0
        } else {
            self.selected_folder.matches('/').count() + 1
        };
        let indent = 4.0 + depth as f32 * 12.0;

        row![
            Space::new().width(Length::Fixed(18.0)),
            icon.size(14),
            text_input("name", &self.name)
                .on_input(Message::NameEdited)
                .on_submit(Message::SubmitName)
                .size(13),
        ]
        .spacing(6)
        .padding(Padding::ZERO.left(indent).top(4).bottom(4))
        .align_y(Alignment::Center)
        .into()
    }
}

/// An icon-only button with a tooltip, the way an editor toolbar works.
fn action<'a>(
    icon: impl Into<Element<'a, Message>>,
    label: &'static str,
    message: Message,
) -> Element<'a, Message> {
    tooltip(
        button(icon)
            .style(button::text)
            .padding(2)
            .on_press(message),
        text(label).size(11),
        Position::Bottom,
    )
    .into()
}

fn selected_row(theme: &Theme, _status: button::Status) -> button::Style {
    let palette = theme.extended_palette();

    button::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: palette.background.weak.text,
        ..button::Style::default()
    }
}
