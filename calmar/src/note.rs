//! Notes and the links they contain.

/// A Markdown file found in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Path relative to the vault root, always with `/` separators.
    pub path: String,
    /// Outgoing links, in document order.
    pub links: Vec<Link>,
}

/// A link written in a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Destination as written, for example `projets/calmar.md#todo`.
    pub target: String,
    /// Vault-relative path of the note it points to, when that note exists.
    ///
    /// `None` for a broken link, an external URL, or a link to a non-note
    /// file such as an image.
    pub note: Option<String>,
}
