//! End-to-end tests through the public API: a real temporary directory and a
//! real Git repository, no mocks.

use std::path::PathBuf;

use calmar::{Vault, VaultError};
use tempfile::tempdir;

#[test]
fn create_builds_a_working_vault() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("MyVault");

    let vault = Vault::create(&path).unwrap();

    assert!(path.join(".git").is_dir(), "repository missing");
    assert!(path.join(".calmar/config.toml").is_file(), "config missing");
    assert!(path.join(".gitignore").is_file(), "gitignore missing");
    assert!(
        path.join(".gitattributes").is_file(),
        "gitattributes missing"
    );
    assert!(path.join("assets").is_dir(), "attachments dir missing");

    let last = vault.history().last().unwrap().expect("initial commit");
    assert_eq!(last.message, "Initial commit");
    assert_eq!(last.id.to_hex().len(), 40);
}

#[test]
fn create_does_not_commit_pending_work_in_an_existing_vault() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("vault");
    let vault = Vault::create(&path).unwrap();
    assert_eq!(vault.history().log(10).unwrap().len(), 1);

    // Work done outside Calmar: editor, shell, sync tool.
    std::fs::write(path.join("outside.md"), "written by someone else").unwrap();

    let reopened = Vault::create(&path).unwrap();
    assert_eq!(
        reopened.history().log(10).unwrap().len(),
        1,
        "create must not commit work it did not do"
    );
    assert_eq!(
        reopened.history().last().unwrap().unwrap().message,
        "Initial commit"
    );
}

#[test]
fn open_rejects_a_plain_directory() {
    let tmp = tempdir().unwrap();

    match Vault::open(tmp.path()) {
        Err(VaultError::NotAVault { .. }) => {}
        other => panic!("expected NotAVault, got {other:?}"),
    }
}

#[test]
fn write_file_creates_then_updates() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    let created = vault
        .write_file("notes/hello.md", "# Hello\n")
        .unwrap()
        .expect("first write must commit");
    assert_eq!(vault.read("notes/hello.md").unwrap(), "# Hello\n");
    assert_eq!(
        vault.history().last().unwrap().unwrap().message,
        "Add notes/hello.md"
    );

    let updated = vault
        .write_file("notes/hello.md", "# Hello\n\nworld\n")
        .unwrap()
        .expect("second write must commit");
    assert_ne!(created, updated);
    assert_eq!(
        vault.history().last().unwrap().unwrap().message,
        "Update notes/hello.md"
    );
}

#[test]
fn writing_identical_content_creates_no_commit() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("a.md", "same").unwrap();
    let before = vault.history().log(10).unwrap().len();

    assert!(vault.write_file("a.md", "same").unwrap().is_none());
    assert_eq!(vault.history().log(10).unwrap().len(), before);
}

#[test]
fn delete_file_removes_it_from_the_history() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("a.md", "x").unwrap();
    vault
        .delete_file("a.md")
        .unwrap()
        .expect("delete must commit");

    assert!(!vault.root().join("a.md").exists());
    assert_eq!(
        vault.history().last().unwrap().unwrap().message,
        "Delete a.md"
    );

    // The file must also be gone from the committed tree, not only from disk.
    let repo = git2::Repository::open(vault.root()).unwrap();
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    assert!(tree.get_name("a.md").is_none());
}

#[test]
fn directories_can_be_created_and_deleted() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.create_dir("projets/calmar").unwrap();
    assert!(vault.root().join("projets/calmar").is_dir());

    vault.write_file("projets/calmar/notes.md", "n").unwrap();
    vault
        .delete_dir("projets")
        .unwrap()
        .expect("delete must commit");

    assert!(!vault.root().join("projets").exists());
    assert!(
        vault.delete_dir(".").is_err(),
        "the vault root must not be deletable"
    );
}

#[test]
fn paths_outside_the_vault_are_rejected() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    assert!(matches!(
        vault.write_file("../escape.md", "x"),
        Err(VaultError::InvalidPath { .. })
    ));
    assert!(matches!(
        vault.write_file("/tmp/escape.md", "x"),
        Err(VaultError::InvalidPath { .. })
    ));
    assert!(!vault.root().parent().unwrap().join("escape.md").exists());
}

#[test]
fn log_is_newest_first_even_within_the_same_second() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    // Several commits in the same second: TIME alone cannot order them.
    vault.write_file("a.md", "1").unwrap();
    vault.write_file("b.md", "2").unwrap();
    vault.write_file("c.md", "3").unwrap();

    let messages: Vec<String> = vault
        .history()
        .log(10)
        .unwrap()
        .into_iter()
        .map(|commit| commit.message)
        .collect();

    assert_eq!(
        messages,
        ["Add c.md", "Add b.md", "Add a.md", "Initial commit"]
    );
}

#[test]
fn config_is_read_and_validated_on_open() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("vault");
    Vault::create(&path).unwrap();

    std::fs::write(
        path.join(".calmar/config.toml"),
        "attachments_dir = \"media\"\n",
    )
    .unwrap();
    let vault = Vault::open(&path).unwrap();
    assert_eq!(vault.config().attachments_dir, PathBuf::from("media"));

    std::fs::write(
        path.join(".calmar/config.toml"),
        "notes_dir = \"../outside\"\n",
    )
    .unwrap();
    assert!(matches!(Vault::open(&path), Err(VaultError::Config { .. })));
}

#[test]
fn restoring_a_commit_brings_the_vault_back() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("a.md", "v1").unwrap();
    let first = vault.history().last().unwrap().unwrap().id;

    vault.write_file("a.md", "v2").unwrap();
    vault.write_file("b.md", "b").unwrap();

    let restored = vault.restore(first).unwrap().expect("restore must commit");

    assert_eq!(vault.read("a.md").unwrap(), "v1");
    assert!(!vault.root().join("b.md").exists());
    assert_eq!(vault.history().last().unwrap().unwrap().id, restored);
    assert!(
        vault
            .history()
            .last()
            .unwrap()
            .unwrap()
            .summary
            .starts_with("Restore ")
    );
}

#[test]
fn restoring_leaves_untracked_files_alone() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();

    vault.write_file("a.md", "v1").unwrap();
    let first = vault.history().last().unwrap().unwrap().id;
    vault.write_file("a.md", "v2").unwrap();

    std::fs::write(vault.root().join("untracked.md"), "keep me").unwrap();

    vault.restore(first).unwrap();

    assert_eq!(
        std::fs::read_to_string(vault.root().join("untracked.md")).unwrap(),
        "keep me"
    );
}

#[test]
fn restoring_the_current_state_creates_no_commit() {
    let tmp = tempdir().unwrap();
    let mut vault = Vault::create(tmp.path().join("vault")).unwrap();
    vault.write_file("a.md", "a").unwrap();
    let head = vault.history().last().unwrap().unwrap().id;

    assert!(vault.restore(head).unwrap().is_none());
}
