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
