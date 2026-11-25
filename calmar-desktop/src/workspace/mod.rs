//! The vault screen: an explorer and a Markdown editor.
//!
//! This module holds the state and the operations. The widgets live in
//! [`view`], so the behaviour can be read, and tested, without iced.

mod markdown_highlight;
mod tree;
mod view;

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use calmar::{Commit, CommitId, Vault, VaultError};
use iced::widget::{markdown, pane_grid, text_editor};
use iced::{Task, Theme};

use tree::Entry;

#[derive(Debug, Clone)]
pub enum Message {
    SelectFolder(String),
    ToggleFolder(String),
    SelectNote(String),
    StartNew(NewKind),
    StartRename,
    NameEdited(String),
    SubmitName,
    AskDelete,
    ConfirmDelete,
    CancelOverlay,
    OpenHistory,
    Restore(CommitId),
    DailyNote,
    Editor(text_editor::Action),
    Save,
    Refresh,
    TogglePreview,
    TogglePreviewOnly,
    Edit,
    Escape,
    Link(String),
    Resized(pane_grid::ResizeEvent),
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewKind {
    Note,
    Folder,
}

/// The note or folder the user last clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Folder(String),
    Note(String),
}

impl Selection {
    pub fn path(&self) -> &str {
        match self {
            Selection::Folder(path) | Selection::Note(path) => path,
        }
    }
}

/// A modal panel above the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    Delete(Selection),
    History,
}

/// Share of the width taken by the explorer.
const LIST_RATIO: f32 = 0.25;

/// Share of the remaining width taken by the editor when the preview is open.
const PREVIEW_RATIO: f32 = 0.62;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Pane {
    Explorer,
    Editor,
    Preview,
}

pub struct Workspace {
    vault: Vault,
    tree: Vec<Entry>,
    /// Folders the user collapsed.
    collapsed: BTreeSet<String>,
    /// Folder the next note or folder is created in. Empty means the root.
    selected_folder: String,
    selected_note: Option<String>,
    content: text_editor::Content,
    dirty: bool,
    /// Name being typed when creating or renaming.
    name: String,
    creating: Option<NewKind>,
    renaming: Option<Selection>,
    active: Option<Selection>,
    overlay: Option<Overlay>,
    history: Vec<Commit>,
    status: Option<String>,
    panes: pane_grid::State<Pane>,
    editor_pane: pane_grid::Pane,
    preview_pane: Option<pane_grid::Pane>,
    /// When true, the editor pane shows the rendered note instead of the source.
    preview_only: bool,
    /// When true, clicking the rendered note switches back to the editor.
    click_to_edit: bool,
    theme: Theme,
    preview: markdown::Content,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("root", &self.vault.root())
            .field("selected_note", &self.selected_note)
            .finish()
    }
}

impl Workspace {
    pub fn new(vault: Vault) -> Self {
        let (mut panes, explorer) = pane_grid::State::new(Pane::Explorer);
        let editor = vertical_split(&mut panes, explorer, Pane::Editor, LIST_RATIO);

        let mut workspace = Self {
            selected_folder: base_folder(&vault),
            vault,
            tree: Vec::new(),
            collapsed: BTreeSet::new(),
            selected_note: None,
            content: text_editor::Content::new(),
            dirty: false,
            name: String::new(),
            creating: None,
            renaming: None,
            active: None,
            overlay: None,
            history: Vec::new(),
            status: None,
            panes,
            editor_pane: editor,
            preview_pane: None,
            preview_only: true,
            click_to_edit: true,
            theme: Theme::Dark,
            preview: markdown::Content::new(),
        };

        if let Err(error) = workspace.reload() {
            workspace.status = Some(error.to_string());
        }

        // Open the first note, so the editor and the preview are not empty on
        // arrival.
        if let Some(first) = first_note(&workspace.tree)
            && let Err(error) = workspace.select_note(&first)
        {
            workspace.status = Some(error.to_string());
        }

        workspace
    }

    pub fn root(&self) -> &Path {
        self.vault.root()
    }

    pub fn set_click_to_edit(&mut self, value: bool) {
        self.click_to_edit = value;
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectFolder(path) => {
                self.selected_folder = path.clone();
                self.active = Some(Selection::Folder(path));
                self.status = None;
            }
            Message::ToggleFolder(path) => self.toggle_folder(&path),
            Message::SelectNote(path) => {
                self.attempt(|workspace| workspace.select_note(&path));
                self.active = Some(Selection::Note(path));
            }
            Message::StartNew(kind) => self.start_new(kind),
            Message::StartRename => self.start_rename(),
            Message::NameEdited(name) => self.name = name,
            Message::SubmitName => self.attempt(Self::submit_name),
            Message::AskDelete => self.ask_delete(),
            Message::ConfirmDelete => self.attempt(Self::confirm_delete),
            Message::CancelOverlay => self.overlay = None,
            Message::OpenHistory => self.attempt(Self::open_history),
            Message::Restore(commit) => self.attempt(|workspace| workspace.restore(commit)),
            Message::DailyNote => self.attempt(Self::open_daily_note),
            Message::Editor(action) => {
                self.content.perform(action);
                self.dirty = true;
                self.refresh_preview();
            }
            Message::Save => self.attempt(Self::save),
            Message::Refresh => self.attempt(Self::reload),
            Message::TogglePreview => self.toggle_preview(),
            Message::TogglePreviewOnly => self.toggle_preview_only(),
            Message::Edit => self.edit(),
            Message::Escape => self.escape(),
            Message::Link(uri) => self.follow_link(&uri),
            Message::Resized(event) => self.panes.resize(event.split, event.ratio),
            // The application owns the screen switch and handles this one.
            Message::Close => {}
        }

        Task::none()
    }

    /// Rows actually displayed, with collapsed folders hidden.
    pub(crate) fn visible_tree(&self) -> impl Iterator<Item = &Entry> {
        self.tree
            .iter()
            .filter(|entry| !self.is_hidden(entry.path()))
    }

    pub(crate) fn is_collapsed(&self, path: &str) -> bool {
        self.collapsed.contains(path)
    }

    fn select_note(&mut self, path: &str) -> calmar::Result<()> {
        if self.selected_note.as_deref() == Some(path) {
            return Ok(());
        }

        // Pending edits are saved before leaving a note, never dropped.
        self.save()?;
        self.reload_note(path)?;
        self.status = None;

        Ok(())
    }

    fn reload_note(&mut self, path: &str) -> calmar::Result<()> {
        let source = self.vault.read(path)?;
        self.content = text_editor::Content::with_text(&source);
        self.selected_note = Some(path.to_owned());
        self.dirty = false;
        self.refresh_preview();

        Ok(())
    }

    fn clear_note(&mut self) {
        self.selected_note = None;
        self.content = text_editor::Content::new();
        self.dirty = false;
        self.preview = markdown::Content::new();
    }

    fn save(&mut self) -> calmar::Result<()> {
        let text = self.content.text();

        let Some(path) = self.selected_note.clone() else {
            // Typing before choosing a note: saving gives the draft a name.
            if text.trim().is_empty() {
                return Ok(());
            }

            let path = child_path(&self.selected_folder, &self.untitled_note_name());
            let _ = self.vault.write_file(&path, &text)?;
            self.reload()?;
            self.selected_note = Some(path.clone());
            self.active = Some(Selection::Note(path.clone()));
            self.dirty = false;
            self.status = Some(format!("Saved {path}"));

            return Ok(());
        };

        if !self.dirty {
            return Ok(());
        }

        let _ = self.vault.write_file(&path, &text)?;
        self.dirty = false;
        self.status = Some(format!("Saved {path}"));

        Ok(())
    }

    /// `Sans titre`, `Sans titre2`, and so on, in the selected folder.
    fn untitled_note_name(&self) -> String {
        let folder = self.vault.root().join(&self.selected_folder);
        let mut name = "Sans titre.md".to_owned();
        let mut index = 2;

        while folder.join(&name).exists() {
            name = format!("Sans titre{index}.md");
            index += 1;
        }

        name
    }

    /// Creates or opens `notebook/YYYY/MM/DD.md`.
    fn open_daily_note(&mut self) -> calmar::Result<()> {
        self.save()?;

        let now = jiff::ZonedDateTime::now();
        let path = format!(
            "notebook/{}/{:02}/{:02}.md",
            now.year(),
            now.month(),
            now.day()
        );

        if !self.vault.root().join(&path).is_file() {
            let _ = self.vault.write_file(&path, "")?;
            self.reload()?;
        }

        self.selected_folder = parent_of(&path).to_owned();
        self.active = Some(Selection::Note(path.clone()));
        self.selected_note = None;
        self.select_note(&path)?;

        Ok(())
    }

    fn start_new(&mut self, kind: NewKind) {
        self.creating = Some(kind);
        self.renaming = None;
        self.overlay = None;
        self.name.clear();
        self.status = None;
    }

    fn start_rename(&mut self) {
        let Some(selection) = self.active.clone() else {
            self.status = Some("Select a note or folder first.".to_owned());
            return;
        };

        self.creating = None;
        self.overlay = None;
        self.name = match &selection {
            Selection::Note(path) => path
                .rsplit('/')
                .next()
                .unwrap_or(path)
                .trim_end_matches(".md")
                .to_owned(),
            Selection::Folder(path) => path.rsplit('/').next().unwrap_or(path).to_owned(),
        };
        self.renaming = Some(selection);
    }

    /// On error the field stays open so the name can be fixed.
    fn submit_name(&mut self) -> calmar::Result<()> {
        if self.renaming.is_some() {
            self.submit_rename()
        } else if self.creating.is_some() {
            self.submit_new()
        } else {
            Ok(())
        }
    }

    fn submit_new(&mut self) -> calmar::Result<()> {
        let Some(kind) = self.creating else {
            return Ok(());
        };

        let result = match kind {
            NewKind::Note => {
                let name = if self.name.trim().is_empty() {
                    self.untitled_note_name()
                } else {
                    self.name.trim().to_owned()
                };
                self.create_note(&name)
            }
            NewKind::Folder => {
                let name = self.entered_name("new-folder", "");
                self.create_folder(&name)
            }
        };

        if result.is_ok() {
            self.creating = None;
        }

        result
    }

    fn submit_rename(&mut self) -> calmar::Result<()> {
        let Some(selection) = self.renaming.clone() else {
            return Ok(());
        };

        let old = selection.path().to_owned();
        let parent = parent_of(&old);
        let is_note = matches!(selection, Selection::Note(_));
        let new = if is_note {
            child_path(parent, &note_file_name(&self.name)?)
        } else {
            child_path(parent, &clean_name(&self.name)?)
        };

        if new == old {
            self.cancel_inline();
            return Ok(());
        }

        self.save()?;
        if is_note {
            let _ = self.vault.rename(&old, &new)?;
        } else {
            let _ = self.vault.rename_dir(&old, &new)?;
        }

        self.cancel_inline();
        self.reload()?;

        if is_note {
            self.select_note(&new)?;
            self.active = Some(Selection::Note(new));
        } else {
            self.active = Some(Selection::Folder(new.clone()));
            self.selected_folder = new.clone();

            // The open note may have moved with the folder, and its links may
            // have been rewritten, so it is read again from disk.
            let open = self.selected_note.clone().map(|path| {
                match path.strip_prefix(&format!("{old}/")) {
                    Some(rest) => format!("{new}/{rest}"),
                    None => path,
                }
            });

            if let Some(path) = open.filter(|path| self.vault.root().join(path).is_file()) {
                self.reload_note(&path)?;
            }
        }

        Ok(())
    }

    fn cancel_inline(&mut self) {
        self.creating = None;
        self.renaming = None;
        self.name.clear();
    }

    fn ask_delete(&mut self) {
        match &self.active {
            Some(selection) => self.overlay = Some(Overlay::Delete(selection.clone())),
            None => self.status = Some("Select a note or folder first.".to_owned()),
        }
    }

    fn confirm_delete(&mut self) -> calmar::Result<()> {
        let Some(Overlay::Delete(selection)) = self.overlay.take() else {
            return Ok(());
        };

        self.save()?;

        let removed = match &selection {
            Selection::Note(path) => {
                let _ = self.vault.delete_file(path)?;
                path.clone()
            }
            Selection::Folder(path) => {
                let _ = self.vault.delete_dir(path)?;
                path.clone()
            }
        };

        self.forget_removed(&removed, &selection);
        self.reload()?;

        if self.selected_note.is_none()
            && let Some(first) = first_note(&self.tree)
        {
            self.select_note(&first)?;
        }

        self.status = Some(format!("Deleted {removed}"));
        Ok(())
    }

    /// Drops any state that pointed inside what was just deleted.
    fn forget_removed(&mut self, removed: &str, selection: &Selection) {
        let was_note = matches!(selection, Selection::Note(_));

        if self
            .selected_note
            .as_deref()
            .is_some_and(|note| was_note && note == removed || is_under(note, removed))
        {
            self.clear_note();
        }

        if self.active.as_ref() == Some(selection)
            || self
                .active
                .as_ref()
                .is_some_and(|active| is_under(active.path(), removed))
        {
            self.active = None;
        }

        if !was_note && is_under(&self.selected_folder, removed) {
            self.selected_folder.clear();
        }
    }

    fn open_history(&mut self) -> calmar::Result<()> {
        self.save()?;
        self.history = self.vault.history().log(50)?;
        self.overlay = Some(Overlay::History);
        Ok(())
    }

    fn restore(&mut self, commit: CommitId) -> calmar::Result<()> {
        self.save()?;
        let _ = self.vault.restore(commit)?;
        self.reload()?;
        self.overlay = None;

        let target = self
            .selected_note
            .clone()
            .filter(|path| self.vault.root().join(path).is_file())
            .or_else(|| first_note(&self.tree));

        self.clear_note();
        if let Some(target) = target {
            self.select_note(&target)?;
        }

        self.status = Some(format!("Restored {}", short_hash(commit)));
        Ok(())
    }

    fn entered_name(&self, base: &str, extension: &str) -> String {
        let name = self.name.trim();

        if name.is_empty() {
            self.unique_name(base, extension)
        } else {
            name.to_owned()
        }
    }

    fn create_note(&mut self, name: &str) -> calmar::Result<()> {
        self.save()?;

        let file = note_file_name(name)?;
        let path = child_path(&self.selected_folder, &file);

        let _ = self.vault.write_file(&path, "")?;
        self.reload()?;
        self.select_note(&path)?;
        self.name.clear();

        Ok(())
    }

    fn create_folder(&mut self, name: &str) -> calmar::Result<()> {
        self.save()?;

        let folder = clean_name(name)?;
        let path = child_path(&self.selected_folder, &folder);

        self.vault.create_dir(&path)?;
        self.reload()?;
        self.selected_folder = path;
        self.name.clear();

        Ok(())
    }

    fn reload(&mut self) -> calmar::Result<()> {
        let folders = self.vault.folders()?;
        let notes = self
            .vault
            .list()?
            .into_iter()
            .map(|note| note.path)
            .collect();

        self.tree = tree::build(folders, notes);
        Ok(())
    }

    /// Internal links open the note, external ones go to the browser.
    fn follow_link(&mut self, uri: &str) {
        if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("mailto:") {
            let _ = std::process::Command::new("xdg-open").arg(uri).spawn();
            return;
        }

        let target = uri.split('#').next().unwrap_or(uri);
        let Some(current) = self.selected_note.as_deref() else {
            return;
        };
        let directory = current.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");

        let Some(path) = normalize_link(directory, target) else {
            return;
        };
        if self.vault.root().join(&path).is_file() {
            self.attempt(|workspace| workspace.select_note(&path));
        }
    }

    fn refresh_preview(&mut self) {
        self.preview = markdown::Content::parse(&self.content.text());
    }

    fn toggle_preview_only(&mut self) {
        self.preview_only = !self.preview_only;

        if self.preview_only {
            // Two previews would be redundant, so the side pane is closed.
            if let Some(preview) = self.preview_pane.take() {
                self.panes.close(preview);
            }
        }
    }

    fn edit(&mut self) {
        self.preview_only = false;
    }

    fn escape(&mut self) {
        if self.overlay.is_some() {
            self.overlay = None;
        } else if self.creating.is_some() || self.renaming.is_some() {
            self.cancel_inline();
        } else if self.preview_only {
            self.preview_only = false;
        }
    }

    fn toggle_preview(&mut self) {
        // The side preview has no meaning while the editor pane renders the
        // note in read mode.
        if self.preview_only {
            return;
        }

        if let Some(preview) = self.preview_pane.take() {
            self.panes.close(preview);
            return;
        }

        let preview = vertical_split(
            &mut self.panes,
            self.editor_pane,
            Pane::Preview,
            PREVIEW_RATIO,
        );
        self.preview_pane = Some(preview);
    }

    fn toggle_folder(&mut self, path: &str) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_owned());
        }
    }

    fn is_hidden(&self, path: &str) -> bool {
        let mut current = path;

        while let Some((parent, _)) = current.rsplit_once('/') {
            if self.collapsed.contains(parent) {
                return true;
            }
            current = parent;
        }

        false
    }

    /// `untitled.md`, then `untitled-2.md`, and so on.
    fn unique_name(&self, base: &str, extension: &str) -> String {
        let folder = self.vault.root().join(&self.selected_folder);
        let mut name = format!("{base}{extension}");
        let mut index = 2;

        while folder.join(&name).exists() {
            name = format!("{base}-{index}{extension}");
            index += 1;
        }

        name
    }

    fn attempt(&mut self, operation: impl FnOnce(&mut Self) -> calmar::Result<()>) {
        if let Err(error) = operation(self) {
            self.status = Some(error.to_string());
        }
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

/// True when `path` is `prefix` or lives inside it.
fn is_under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn short_hash(commit: CommitId) -> String {
    commit.to_hex().chars().take(7).collect()
}

fn first_note(tree: &[Entry]) -> Option<String> {
    tree.iter().find_map(|entry| match entry {
        Entry::Note { path, .. } => Some(path.clone()),
        Entry::Folder { .. } => None,
    })
}

fn vertical_split(
    panes: &mut pane_grid::State<Pane>,
    target: pane_grid::Pane,
    content: Pane,
    ratio: f32,
) -> pane_grid::Pane {
    let (pane, split) = panes
        .split(pane_grid::Axis::Vertical, target, content)
        .expect("splitting one pane always yields two");

    panes.resize(split, ratio);
    pane
}

/// Resolves a link target against the folder of the current note.
fn normalize_link(directory: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = if target.starts_with('/') {
        Vec::new()
    } else {
        directory
            .split('/')
            .filter(|part| !part.is_empty())
            .collect()
    };

    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Default folder for new notes: `notes_dir`, unless it is the vault root.
fn base_folder(vault: &Vault) -> String {
    vault
        .config()
        .notes_dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Rejects names that would point somewhere else than a direct child.
fn clean_name(name: &str) -> calmar::Result<String> {
    let name = name.trim();
    let invalid =
        name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == "..";

    if invalid {
        return Err(VaultError::InvalidPath {
            path: PathBuf::from(name),
            reason: "use a simple name, without slashes".to_owned(),
        });
    }

    Ok(name.to_owned())
}

fn note_file_name(name: &str) -> calmar::Result<String> {
    let name = clean_name(name)?;

    if name.to_ascii_lowercase().ends_with(".md") {
        Ok(name)
    } else {
        Ok(format!("{name}.md"))
    }
}

fn child_path(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    struct Fixture {
        workspace: Workspace,
        // Keeps the temporary directory alive for the whole test.
        _directory: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let directory = tempdir().unwrap();
        let root = directory.path().join("vault");

        let mut vault = Vault::create(&root).unwrap();
        vault.write_file("index.md", "# Index\n").unwrap();
        vault.write_file("projets/calmar.md", "# Calmar\n").unwrap();
        vault.create_dir("vides").unwrap();

        Fixture {
            workspace: Workspace::new(Vault::open(&root).unwrap()),
            _directory: directory,
        }
    }

    fn send(workspace: &mut Workspace, message: Message) {
        let _ = workspace.update(message);
    }

    fn visible_paths(workspace: &Workspace) -> Vec<&str> {
        workspace.visible_tree().map(|entry| entry.path()).collect()
    }

    #[test]
    fn the_tree_lists_folders_then_notes() {
        let fixture = fixture();

        assert_eq!(
            fixture.workspace.tree,
            [
                Entry::Folder {
                    path: "projets".into(),
                    depth: 0
                },
                Entry::Note {
                    path: "projets/calmar.md".into(),
                    depth: 1
                },
                Entry::Folder {
                    path: "vides".into(),
                    depth: 0
                },
                Entry::Note {
                    path: "index.md".into(),
                    depth: 0
                },
            ]
        );
    }

    #[test]
    fn opening_a_vault_opens_the_first_note() {
        let fixture = fixture();

        assert_eq!(
            fixture.workspace.selected_note.as_deref(),
            Some("projets/calmar.md")
        );
        assert_eq!(fixture.workspace.content.text(), "# Calmar\n");
        assert!(!fixture.workspace.preview.items().is_empty());
    }

    #[test]
    fn selecting_a_note_loads_its_content() {
        let mut fixture = fixture();

        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        assert_eq!(fixture.workspace.content.text(), "# Index\n");
        assert!(!fixture.workspace.dirty);
    }

    #[test]
    fn saving_writes_the_edited_text_and_commits() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        fixture.workspace.content = text_editor::Content::with_text("# Index\n\nedited\n");
        fixture.workspace.dirty = true;
        send(&mut fixture.workspace, Message::Save);

        assert_eq!(
            fixture.workspace.vault.read("index.md").unwrap(),
            "# Index\n\nedited\n"
        );
        assert!(!fixture.workspace.dirty);
    }

    #[test]
    fn switching_notes_saves_pending_changes() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        fixture.workspace.content = text_editor::Content::with_text("# pending\n");
        fixture.workspace.dirty = true;
        send(
            &mut fixture.workspace,
            Message::SelectNote("projets/calmar.md".into()),
        );

        assert_eq!(
            fixture.workspace.vault.read("index.md").unwrap(),
            "# pending\n"
        );
        assert_eq!(
            fixture.workspace.content.text(),
            fixture.workspace.vault.read("projets/calmar.md").unwrap()
        );
    }

    #[test]
    fn creating_a_note_puts_it_in_the_selected_folder_and_opens_it() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectFolder("projets".into()),
        );

        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));
        fixture.workspace.name = "nouvelle".into();
        send(&mut fixture.workspace, Message::SubmitName);

        let path = fixture.workspace.vault.root().join("projets/nouvelle.md");
        assert!(path.is_file());
        assert_eq!(
            fixture.workspace.selected_note.as_deref(),
            Some("projets/nouvelle.md")
        );
    }

    #[test]
    fn creating_a_folder_selects_it_for_the_next_note() {
        let mut fixture = fixture();

        send(&mut fixture.workspace, Message::StartNew(NewKind::Folder));
        fixture.workspace.name = "journal".into();
        send(&mut fixture.workspace, Message::SubmitName);

        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));
        fixture.workspace.name = "aujourdhui".into();
        send(&mut fixture.workspace, Message::SubmitName);

        let path = fixture.workspace.vault.root().join("journal/aujourdhui.md");
        assert!(path.is_file());
    }

    #[test]
    fn an_empty_name_gets_a_generated_one() {
        let mut fixture = fixture();

        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));
        send(&mut fixture.workspace, Message::SubmitName);

        assert!(
            fixture
                .workspace
                .vault
                .root()
                .join("Sans titre.md")
                .is_file()
        );
    }

    #[test]
    fn saving_a_draft_creates_a_sans_titre_note() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("vault");
        Vault::create(&root).unwrap();

        let mut workspace = Workspace::new(Vault::open(&root).unwrap());
        assert!(workspace.selected_note.is_none());

        workspace.content = text_editor::Content::with_text("# brouillon\n");
        workspace.dirty = true;
        send(&mut workspace, Message::Save);

        assert_eq!(workspace.selected_note.as_deref(), Some("Sans titre.md"));
        assert_eq!(
            workspace.vault.read("Sans titre.md").unwrap(),
            "# brouillon\n"
        );
        assert!(!workspace.dirty);
    }

    #[test]
    fn sans_titre_names_increment() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("vault");
        Vault::create(&root).unwrap();
        let mut workspace = Workspace::new(Vault::open(&root).unwrap());

        for (index, content) in ["first\n", "second\n", "third\n"].iter().enumerate() {
            workspace.selected_note = None;
            workspace.content = text_editor::Content::with_text(content);
            workspace.dirty = true;
            send(&mut workspace, Message::Save);

            let expected = match index {
                0 => "Sans titre.md".to_owned(),
                other => format!("Sans titre{}.md", other + 1),
            };
            assert!(workspace.vault.root().join(&expected).is_file());
        }
    }

    #[test]
    fn the_daily_note_is_created_once_and_selected() {
        let mut fixture = fixture();

        send(&mut fixture.workspace, Message::DailyNote);

        let path = fixture.workspace.selected_note.clone().unwrap();
        let parts: Vec<&str> = path.split('/').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "notebook");
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 2);
        assert!(parts[3].ends_with(".md"));
        assert!(fixture.workspace.vault.root().join(&path).is_file());

        // A second click reuses the same note instead of creating another.
        let before = fixture.workspace.vault.history().log(50).unwrap().len();
        send(&mut fixture.workspace, Message::DailyNote);
        assert_eq!(
            fixture.workspace.selected_note.as_deref(),
            Some(path.as_str())
        );
        assert_eq!(
            fixture.workspace.vault.history().log(50).unwrap().len(),
            before
        );
    }

    #[test]
    fn an_invalid_name_keeps_the_field_open() {
        let mut fixture = fixture();

        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));
        fixture.workspace.name = "a/b".into();
        send(&mut fixture.workspace, Message::SubmitName);

        assert!(fixture.workspace.status.is_some());
        assert_eq!(fixture.workspace.creating, Some(NewKind::Note));
    }

    #[test]
    fn escape_cancels_creation() {
        let mut fixture = fixture();

        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));
        send(&mut fixture.workspace, Message::Escape);

        assert!(fixture.workspace.creating.is_none());
        assert!(fixture.workspace.name.is_empty());
    }

    #[test]
    fn collapsing_a_folder_hides_its_notes() {
        let mut fixture = fixture();
        let before = visible_paths(&fixture.workspace);
        assert!(before.contains(&"projets/calmar.md"));

        send(
            &mut fixture.workspace,
            Message::ToggleFolder("projets".into()),
        );

        let after = visible_paths(&fixture.workspace);
        assert!(!after.contains(&"projets/calmar.md"));
        assert!(after.contains(&"projets"));
    }

    #[test]
    fn dragging_the_divider_resizes_the_two_panes() {
        let mut fixture = fixture();

        let (split, before) = split_of(&fixture.workspace.panes);
        assert!(
            (before - LIST_RATIO).abs() < f32::EPSILON,
            "explorer starts narrow"
        );

        send(
            &mut fixture.workspace,
            Message::Resized(pane_grid::ResizeEvent { split, ratio: 0.5 }),
        );

        let (_, after) = split_of(&fixture.workspace.panes);
        assert!((after - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn following_a_link_opens_the_note_it_points_to() {
        let mut fixture = fixture();

        fixture.workspace.selected_note = Some("index.md".into());
        send(
            &mut fixture.workspace,
            Message::Link("projets/calmar.md#top".into()),
        );

        assert_eq!(
            fixture.workspace.selected_note.as_deref(),
            Some("projets/calmar.md")
        );
    }

    #[test]
    fn link_targets_are_resolved_relative_to_the_current_note() {
        assert_eq!(
            normalize_link("projets", "../index.md"),
            Some("index.md".to_string())
        );
        assert_eq!(
            normalize_link("projets", "calmar.md"),
            Some("projets/calmar.md".to_string())
        );
        assert_eq!(
            normalize_link("a/b", "/root.md"),
            Some("root.md".to_string())
        );
        assert_eq!(normalize_link("", "../../outside.md"), None);
    }

    #[test]
    fn the_preview_follows_the_edited_note() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        assert!(!fixture.workspace.preview.items().is_empty());

        fixture.workspace.content =
            text_editor::Content::with_text("# Title\n\n**bold** and `code`\n");
        fixture.workspace.refresh_preview();

        assert_eq!(fixture.workspace.preview.items().len(), 2);
    }

    #[test]
    fn renaming_a_note_moves_it_and_keeps_the_content() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );
        send(&mut fixture.workspace, Message::StartRename);
        assert_eq!(fixture.workspace.name, "index");

        fixture.workspace.name = "home".into();
        send(&mut fixture.workspace, Message::SubmitName);

        let root = fixture.workspace.vault.root();
        assert!(root.join("home.md").is_file());
        assert!(!root.join("index.md").exists());
        assert_eq!(fixture.workspace.selected_note.as_deref(), Some("home.md"));
        assert_eq!(fixture.workspace.content.text(), "# Index\n");
    }

    #[test]
    fn renaming_a_folder_moves_it_and_follows_the_open_note() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectFolder("projets".into()),
        );
        send(&mut fixture.workspace, Message::StartRename);
        fixture.workspace.name = "archive".into();
        send(&mut fixture.workspace, Message::SubmitName);

        let root = fixture.workspace.vault.root();
        assert!(root.join("archive/calmar.md").is_file());
        assert!(!root.join("projets").exists());
        assert_eq!(fixture.workspace.selected_folder, "archive");
        assert_eq!(
            fixture.workspace.selected_note.as_deref(),
            Some("archive/calmar.md")
        );
        assert_eq!(fixture.workspace.content.text(), "# Calmar\n");
    }

    #[test]
    fn renaming_without_a_selection_reports_it() {
        let mut fixture = fixture();
        assert!(fixture.workspace.active.is_none());

        send(&mut fixture.workspace, Message::StartRename);

        assert!(fixture.workspace.renaming.is_none());
        assert!(fixture.workspace.status.is_some());
    }

    #[test]
    fn deleting_a_note_needs_a_confirmation() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        send(&mut fixture.workspace, Message::AskDelete);
        assert!(matches!(
            fixture.workspace.overlay,
            Some(Overlay::Delete(Selection::Note(ref path))) if path == "index.md"
        ));
        assert!(fixture.workspace.vault.root().join("index.md").is_file());

        send(&mut fixture.workspace, Message::CancelOverlay);
        assert!(fixture.workspace.overlay.is_none());
        assert!(fixture.workspace.vault.root().join("index.md").is_file());

        send(&mut fixture.workspace, Message::AskDelete);
        send(&mut fixture.workspace, Message::ConfirmDelete);

        assert!(fixture.workspace.overlay.is_none());
        assert!(!fixture.workspace.vault.root().join("index.md").exists());
        assert!(
            fixture.workspace.selected_note.is_some(),
            "another note opens"
        );
    }

    #[test]
    fn deleting_a_folder_removes_its_notes() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectFolder("projets".into()),
        );
        send(&mut fixture.workspace, Message::AskDelete);
        send(&mut fixture.workspace, Message::ConfirmDelete);

        assert!(!fixture.workspace.vault.root().join("projets").exists());
        assert!(fixture.workspace.selected_folder.is_empty());
    }

    #[test]
    fn history_lists_commits_and_restores_one() {
        let mut fixture = fixture();
        let target = fixture
            .workspace
            .vault
            .history()
            .log(20)
            .unwrap()
            .into_iter()
            .find(|commit| commit.summary == "Add index.md")
            .unwrap()
            .id;

        fixture
            .workspace
            .vault
            .write_file("index.md", "# Changed\n")
            .unwrap();

        send(&mut fixture.workspace, Message::OpenHistory);
        assert_eq!(fixture.workspace.overlay, Some(Overlay::History));
        assert!(!fixture.workspace.history.is_empty());

        send(&mut fixture.workspace, Message::Restore(target));

        assert!(fixture.workspace.overlay.is_none());
        assert_eq!(
            fixture.workspace.vault.read("index.md").unwrap(),
            "# Index\n"
        );
        assert!(
            fixture
                .workspace
                .status
                .as_deref()
                .is_some_and(|status| status.starts_with("Restored "))
        );
    }

    #[test]
    fn escape_closes_the_overlay_before_the_inline_field() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        send(&mut fixture.workspace, Message::AskDelete);
        send(&mut fixture.workspace, Message::Escape);
        assert!(fixture.workspace.overlay.is_none());

        send(&mut fixture.workspace, Message::StartRename);
        send(&mut fixture.workspace, Message::Escape);
        assert!(fixture.workspace.renaming.is_none());
    }

    #[test]
    fn the_view_can_be_built_with_each_overlay() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        send(&mut fixture.workspace, Message::AskDelete);
        let _ = fixture.workspace.view();

        send(&mut fixture.workspace, Message::CancelOverlay);
        send(&mut fixture.workspace, Message::OpenHistory);
        let _ = fixture.workspace.view();

        send(&mut fixture.workspace, Message::StartRename);
        let _ = fixture.workspace.view();
    }

    #[test]
    fn the_theme_can_be_changed_and_the_view_builds() {
        let mut fixture = fixture();
        assert!(matches!(fixture.workspace.theme, Theme::Dark));

        fixture.workspace.set_theme(Theme::Nord);

        assert!(matches!(fixture.workspace.theme, Theme::Nord));
        let _ = fixture.workspace.view();
    }

    #[test]
    fn clicking_the_rendered_note_returns_to_the_editor() {
        let mut fixture = fixture();
        assert!(fixture.workspace.preview_only);

        send(&mut fixture.workspace, Message::Edit);

        assert!(!fixture.workspace.preview_only);
    }

    #[test]
    fn the_click_behaviour_can_be_disabled() {
        let mut fixture = fixture();
        assert!(fixture.workspace.click_to_edit);

        fixture.workspace.set_click_to_edit(false);

        assert!(!fixture.workspace.click_to_edit);
        // The read mode still renders without the click handler.
        let _ = fixture.workspace.view();
    }

    #[test]
    fn the_vault_opens_in_preview_only_mode() {
        let fixture = fixture();

        assert!(fixture.workspace.preview_only);
        assert!(fixture.workspace.preview_pane.is_none());
        assert_eq!(fixture.workspace.panes.len(), 2);

        // The view in read mode must build too.
        let _ = fixture.workspace.view();
    }

    #[test]
    fn edit_mode_shows_the_source_and_the_side_preview_stays_closed() {
        let mut fixture = fixture();

        send(&mut fixture.workspace, Message::TogglePreviewOnly);

        assert!(!fixture.workspace.preview_only);
        assert!(fixture.workspace.preview_pane.is_none());

        // The side preview is opt-in.
        send(&mut fixture.workspace, Message::TogglePreview);
        assert!(fixture.workspace.preview_pane.is_some());

        // Back to read mode closes it again.
        send(&mut fixture.workspace, Message::TogglePreviewOnly);
        assert!(fixture.workspace.preview_only);
        assert!(fixture.workspace.preview_pane.is_none());
    }

    #[test]
    fn escape_leaves_preview_only_but_cancels_creation_first() {
        let mut fixture = fixture();
        assert!(fixture.workspace.preview_only);

        send(&mut fixture.workspace, Message::Escape);
        assert!(!fixture.workspace.preview_only);

        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));
        send(&mut fixture.workspace, Message::Escape);
        assert!(fixture.workspace.creating.is_none());
    }

    #[test]
    fn the_preview_pane_can_be_closed_and_reopened() {
        let mut fixture = fixture();
        send(&mut fixture.workspace, Message::TogglePreviewOnly);
        assert_eq!(fixture.workspace.panes.len(), 2);

        send(&mut fixture.workspace, Message::TogglePreview);
        assert_eq!(fixture.workspace.panes.len(), 3);
        assert!(fixture.workspace.preview_pane.is_some());

        send(&mut fixture.workspace, Message::TogglePreview);
        assert_eq!(fixture.workspace.panes.len(), 2);
        assert!(fixture.workspace.preview_pane.is_none());
    }

    #[test]
    fn the_side_preview_toggle_is_ignored_in_read_mode() {
        let mut fixture = fixture();
        assert!(fixture.workspace.preview_only);

        send(&mut fixture.workspace, Message::TogglePreview);

        assert!(fixture.workspace.preview_pane.is_none());
        assert_eq!(fixture.workspace.panes.len(), 2);
    }

    #[test]
    fn the_view_can_be_built_with_a_note_open() {
        let mut fixture = fixture();
        send(
            &mut fixture.workspace,
            Message::SelectNote("index.md".into()),
        );

        // Building the widget tree is enough to catch layout panics; rendering
        // needs a window.
        let _ = fixture.workspace.view();
    }

    #[test]
    fn the_view_can_be_built_while_creating_a_note() {
        let mut fixture = fixture();
        send(&mut fixture.workspace, Message::StartNew(NewKind::Note));

        let _ = fixture.workspace.view();
    }

    fn split_of(panes: &pane_grid::State<Pane>) -> (pane_grid::Split, f32) {
        match panes.layout() {
            pane_grid::Node::Split { id, ratio, .. } => (*id, *ratio),
            pane_grid::Node::Pane(_) => panic!("expected the pane grid to be split"),
        }
    }
}
