//! Turns the flat list of notes and folders into an indented tree.
//!
//! The vault exposes two flat lists. The window needs one ordered list of
//! rows, with a depth for indentation, so this is where the two are joined.

use std::collections::BTreeMap;

/// One row of the note tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Folder { path: String, depth: usize },
    Note { path: String, depth: usize },
}

impl Entry {
    pub fn path(&self) -> &str {
        match self {
            Entry::Folder { path, .. } | Entry::Note { path, .. } => path,
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Entry::Folder { depth, .. } | Entry::Note { depth, .. } => *depth,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, Entry::Folder { .. })
    }

    /// Last component, the part shown in the list.
    pub fn name(&self) -> &str {
        self.path().rsplit('/').next().unwrap_or(self.path())
    }
}

/// Folders first, then notes, at every level.
pub fn build(folders: Vec<String>, notes: Vec<String>) -> Vec<Entry> {
    let mut children: BTreeMap<String, Group> = BTreeMap::new();

    for folder in folders {
        children
            .entry(parent_of(&folder))
            .or_default()
            .folders
            .push(folder);
    }

    for note in notes {
        children
            .entry(parent_of(&note))
            .or_default()
            .notes
            .push(note);
    }

    let mut entries = Vec::new();
    push_children("", 0, &children, &mut entries);
    entries
}

#[derive(Default)]
struct Group {
    folders: Vec<String>,
    notes: Vec<String>,
}

fn push_children(
    parent: &str,
    depth: usize,
    children: &BTreeMap<String, Group>,
    out: &mut Vec<Entry>,
) {
    let Some(group) = children.get(parent) else {
        return;
    };

    for folder in &group.folders {
        out.push(Entry::Folder {
            path: folder.clone(),
            depth,
        });
        push_children(folder, depth + 1, children, out);
    }

    for note in &group.notes {
        out.push(Entry::Note {
            path: note.clone(),
            depth,
        });
    }
}

/// `a/b.md` belongs to `a`; `a` belongs to the root.
fn parent_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str, depth: usize) -> Entry {
        Entry::Folder {
            path: path.to_owned(),
            depth,
        }
    }

    fn note(path: &str, depth: usize) -> Entry {
        Entry::Note {
            path: path.to_owned(),
            depth,
        }
    }

    #[test]
    fn nests_notes_and_folders() {
        let entries = build(
            vec!["projets".into(), "projets/calmar".into()],
            vec!["index.md".into(), "projets/calmar/notes.md".into()],
        );

        assert_eq!(
            entries,
            [
                folder("projets", 0),
                folder("projets/calmar", 1),
                note("projets/calmar/notes.md", 2),
                note("index.md", 0),
            ]
        );
    }

    #[test]
    fn folders_come_before_notes_in_the_same_folder() {
        let entries = build(vec!["b".into()], vec!["a.md".into()]);

        assert_eq!(entries, [folder("b", 0), note("a.md", 0)]);
    }

    #[test]
    fn a_note_without_a_folder_is_shown_at_the_root() {
        let entries = build(Vec::new(), vec!["orphan.md".into()]);

        assert_eq!(entries, [note("orphan.md", 0)]);
    }
}
