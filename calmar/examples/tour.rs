//! Tour of the `calmar` library, end to end.
//!
//! Run it with a directory, or without one to use a temporary vault:
//!
//! ```sh
//! cargo run -p calmar --example tour -- /tmp/my-vault
//! ```
//!
//! It creates notes, links them, lists them, renames one, reads the history,
//! and commits a change made outside the library. The vault is left on disk so
//! you can inspect it with `git log`.

use calmar::{Note, VERSION, Vault};

fn main() -> calmar::Result<()> {
    let path = match std::env::args().nth(1) {
        Some(argument) => std::path::PathBuf::from(argument),
        None => default_vault(),
    };
    let mut vault = Vault::create(&path)?;

    println!("Calmar {VERSION}");
    println!("vault: {}", vault.root().display());
    println!("notes dir: {}", vault.config().notes_dir.display());

    // Every write is a commit. The methods return `Some(CommitId)` when
    // something changed, and `None` when the content was already identical.
    vault.write_file(
        "index.md",
        "# Index\n\n- [Calmar](projets/calmar.md)\n- [journal](journal/2026.md)\n",
    )?;
    vault.write_file(
        "projets/calmar.md",
        "# Calmar\n\nRetour: [index](../index.md)\n",
    )?;
    vault.write_file(
        "journal/2026.md",
        "# 2026\n\nVoir le [projet](../projets/calmar.md).\n",
    )?;

    // A note is a file plus the links it contains. Unresolved links are kept:
    // a broken link is information, not an error.
    println!("\nnotes:");
    for note in vault.list()? {
        print_note(&note);
    }

    println!("\nbacklinks to projets/calmar.md:");
    for source in vault.backlinks("projets/calmar.md")? {
        println!("  {source}");
    }

    // Renaming rewrites the links pointing at the note, and the relative links
    // inside the moved note. One commit for the whole operation.
    vault.rename("projets/calmar.md", "projets/notes-calmar.md")?;
    println!("\nindex.md after rename:");
    print!("{}", vault.read("index.md")?);

    println!("\nhistory:");
    for commit in vault.history().log(10)? {
        println!("  {} {}", commit.id, commit.summary);
    }

    // Files can change outside Calmar, for instance when edited with vim.
    // `has_pending_changes` tells the app when to commit them.
    write_external(&vault.root().join("external.md"), "# Edited elsewhere\n")?;
    println!("\npending changes: {}", vault.has_pending_changes()?);
    vault.commit("External edit")?;
    println!(
        "pending changes after commit: {}",
        vault.has_pending_changes()?
    );

    vault.delete_file("external.md")?;
    println!(
        "last commit: {}",
        vault
            .history()
            .last()?
            .map(|commit| commit.summary)
            .unwrap_or_default()
    );

    Ok(())
}

/// Prints a note and, for each link, where it resolves to.
fn print_note(note: &Note) {
    let links: Vec<String> = note
        .links
        .iter()
        .map(|link| match &link.note {
            Some(target) => format!("{} -> {target}", link.target),
            None => format!("{} -> unresolved", link.target),
        })
        .collect();

    println!("  {} | {}", note.path, links.join(", "));
}

/// Simulates a file written by another program, keeping the domain error type.
fn write_external(path: &std::path::Path, contents: &str) -> calmar::Result<()> {
    std::fs::write(path, contents).map_err(|source| calmar::VaultError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Unique temporary directory, so two runs do not share a vault.
fn default_vault() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("calmar-example-{}", std::process::id()))
}
