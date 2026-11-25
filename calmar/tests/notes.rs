//! Notes as a collection, links between them, and renames.

use calmar::{Vault, VaultError};
use tempfile::tempdir;

#[test]
fn list_finds_notes_recursively_and_skips_noise() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("index.md", "# Index\n").unwrap();
    vault.write_file("projets/calmar.md", "# Calmar\n").unwrap();
    vault.write_file("assets/image.md", "attachment\n").unwrap();
    vault.write_file(".calmar/hidden.md", "config\n").unwrap();
    vault.write_file("notes.txt", "not a note\n").unwrap();

    let paths: Vec<String> = vault
        .list()
        .unwrap()
        .into_iter()
        .map(|note| note.path)
        .collect();

    assert_eq!(paths, ["index.md", "projets/calmar.md"]);
}

#[test]
fn list_only_looks_under_notes_dir() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("vault");
    let mut vault = Vault::create(&path).unwrap();
    vault.write_file("notes/a.md", "a").unwrap();
    vault.write_file("b.md", "b").unwrap();

    std::fs::write(path.join(".calmar/config.toml"), "notes_dir = \"notes\"\n").unwrap();
    let vault = Vault::open(&path).unwrap();

    let paths: Vec<String> = vault
        .list()
        .unwrap()
        .into_iter()
        .map(|note| note.path)
        .collect();

    assert_eq!(paths, ["notes/a.md"]);
}

#[test]
fn links_are_resolved_and_broken_ones_reported() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();
    vault.write_file("projets/calmar.md", "# Calmar\n").unwrap();
    vault
        .write_file(
            "index.md",
            "[ok](projets/calmar.md) [broken](nope.md) [web](https://example.org) \
             `[code](inside.md)` ![img](assets/x.png)\n",
        )
        .unwrap();

    let note = vault.read_note("index.md").unwrap();
    let links: Vec<(&str, Option<&str>)> = note
        .links
        .iter()
        .map(|link| (link.target.as_str(), link.note.as_deref()))
        .collect();

    assert_eq!(
        links,
        [
            ("projets/calmar.md", Some("projets/calmar.md")),
            ("nope.md", None),
            ("https://example.org", None),
        ]
    );
}

#[test]
fn backlinks_list_the_notes_pointing_to_a_note() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("journal/2026.md", "# 2026\n").unwrap();
    vault
        .write_file("index.md", "[journal](journal/2026.md)\n")
        .unwrap();
    vault
        .write_file("projets/calmar.md", "[journal](../journal/2026.md#todo)\n")
        .unwrap();

    assert_eq!(
        vault.backlinks("journal/2026.md").unwrap(),
        ["index.md", "projets/calmar.md"]
    );
    assert!(vault.backlinks("journal/absent.md").unwrap().is_empty());
}

#[test]
fn rename_rewrites_links_in_other_notes() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("a.md", "[b](b.md)\n").unwrap();
    vault.write_file("b.md", "# B\n").unwrap();

    vault
        .rename("b.md", "deep/b.md")
        .unwrap()
        .expect("rename must commit");

    assert_eq!(vault.read("a.md").unwrap(), "[b](deep/b.md)\n");
    assert_eq!(vault.backlinks("deep/b.md").unwrap(), ["a.md"]);
    assert!(vault.root().join("deep/b.md").is_file());
    assert!(!vault.root().join("b.md").exists());
}

#[test]
fn rename_rewrites_relative_links_inside_the_moved_note() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("sub/target.md", "# Target\n").unwrap();
    vault
        .write_file("move.md", "[target](sub/target.md)\n")
        .unwrap();

    vault.rename("move.md", "deep/move.md").unwrap();

    assert_eq!(
        vault.read("deep/move.md").unwrap(),
        "[target](../sub/target.md)\n"
    );
}

#[test]
fn rename_refuses_to_overwrite_an_existing_file() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("a.md", "a\n").unwrap();
    vault.write_file("b.md", "b\n").unwrap();

    let error = vault.rename("a.md", "b.md").unwrap_err();
    assert!(matches!(error, VaultError::AlreadyExists { .. }));
    assert_eq!(vault.read("a.md").unwrap(), "a\n");
    assert_eq!(vault.read("b.md").unwrap(), "b\n");
}

#[test]
fn pending_changes_are_detected_and_can_be_committed() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("vault");
    let mut vault = Vault::create(&path).unwrap();
    assert!(!vault.has_pending_changes().unwrap());

    // Edited outside Calmar, the way a text editor or a sync tool would.
    std::fs::write(path.join("outside.md"), "# Outside\n").unwrap();
    assert!(vault.has_pending_changes().unwrap());

    let commit = vault
        .commit("External edit")
        .unwrap()
        .expect("must commit pending work");
    assert_eq!(commit.to_hex().len(), 40);
    assert!(!vault.has_pending_changes().unwrap());
}

#[test]
fn folders_lists_the_directories_of_the_vault() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.create_dir("projets/calmar").unwrap();
    vault.write_file("projets/calmar/notes.md", "n").unwrap();
    vault.write_file("index.md", "i").unwrap();

    assert_eq!(vault.folders().unwrap(), ["projets", "projets/calmar"]);
}

#[test]
fn folders_include_notes_dir_and_its_parents() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("vault");
    Vault::create(&path).unwrap();

    std::fs::write(
        path.join(".calmar/config.toml"),
        "notes_dir = \"a/notes\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(path.join("a/notes/sub")).unwrap();

    let vault = Vault::open(&path).unwrap();

    assert_eq!(vault.folders().unwrap(), ["a", "a/notes", "a/notes/sub"]);
}

#[test]
fn renaming_a_folder_moves_notes_and_rewrites_links() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("projets/calmar.md", "# Calmar\n").unwrap();
    vault
        .write_file("index.md", "[c](projets/calmar.md)\n")
        .unwrap();

    vault.rename_dir("projets", "archive").unwrap();

    assert!(vault.root().join("archive/calmar.md").is_file());
    assert!(!vault.root().join("projets").exists());
    assert_eq!(vault.read("index.md").unwrap(), "[c](archive/calmar.md)\n");
    assert_eq!(vault.backlinks("archive/calmar.md").unwrap(), ["index.md"]);
}

#[test]
fn renaming_a_folder_recomputes_links_that_leave_it() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("index.md", "i").unwrap();
    vault
        .write_file("a/b/note.md", "[root](../../index.md)\n")
        .unwrap();

    vault.rename_dir("a", "deep/a").unwrap();

    assert_eq!(
        vault.read("deep/a/b/note.md").unwrap(),
        "[root](../../../index.md)\n"
    );
}

#[test]
fn a_folder_cannot_be_moved_inside_itself() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();
    vault.create_dir("a").unwrap();

    assert!(matches!(
        vault.rename_dir("a", "a/b"),
        Err(VaultError::InvalidPath { .. })
    ));
}

#[test]
fn renaming_a_folder_refuses_to_overwrite() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();
    vault.create_dir("a").unwrap();
    vault.create_dir("b").unwrap();

    assert!(matches!(
        vault.rename_dir("a", "b"),
        Err(VaultError::AlreadyExists { .. })
    ));
}
