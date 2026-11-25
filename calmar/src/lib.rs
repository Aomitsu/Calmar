//! Calmar: Markdown notes stored as plain files and versioned with Git.
//!
//! A [`Vault`] is a directory: notes are `.md` files inside it, and every
//! change is a commit in the repository at its root.
//!
//! ```no_run
//! use calmar::Vault;
//!
//! # fn main() -> calmar::Result<()> {
//! let mut vault = Vault::create("notes")?;
//! vault.write_file("ideas.md", "# Ideas\n")?;
//! vault.write_file("ideas.md", "# Ideas\n\n- ship it\n")?;
//! vault.delete_file("ideas.md")?;
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod error;
pub mod git;
pub mod note;
pub mod vault;

mod fs;
mod markdown;
mod path;

pub use config::{Author, VaultConfig};
pub use error::{Result, VaultError};
pub use git::{Commit, CommitId, GitHistory};
pub use note::{Link, Note};
pub use vault::Vault;

/// Version of the crate, as written by `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
