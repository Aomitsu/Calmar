//! A deliberately small Markdown link parser.
//!
//! Calmar only needs two things from Markdown: finding inline links
//! (`[text](target)`) to build backlinks, and being able to rewrite them when
//! a note is renamed. So this module does exactly that and nothing else.
//!
//! Out of scope, on purpose: reference-style links (`[text][id]`), HTML links,
//! autolinks, and indented code blocks. Fenced code blocks, inline code, and
//! images are skipped, because a link written in a code sample is not a link.

use std::ops::Range;

/// A link found in a document, with the byte range of its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawLink {
    /// Destination as written, without `<...>` and without the title.
    pub target: String,
    /// Byte range of the destination inside the source.
    pub span: Range<usize>,
}

/// A destination, split into the parts Calmar cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved {
    /// Vault-relative path, normalized (`a/./b/../c.md` becomes `a/c.md`).
    pub path: String,
    /// Trailing `#fragment`, or an empty string.
    pub fragment: String,
    /// True when the link was written as `/some/note.md`.
    pub absolute: bool,
}

/// Finds every inline link outside code blocks and code spans.
pub(crate) fn parse(source: &str) -> Vec<RawLink> {
    let mut links = Vec::new();
    let mut offset = 0;
    let mut fenced = false;

    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            offset += line.len();
            continue;
        }

        if !fenced {
            scan_line(line, offset, &mut links);
        }
        offset += line.len();
    }

    links
}

/// Resolves a link destination against the note that contains it.
///
/// Returns `None` for external URLs, pure anchors (`#section`), and paths that
/// would escape the vault.
pub(crate) fn resolve(from_note: &str, raw_target: &str) -> Option<Resolved> {
    let target = raw_target.trim().replace('\\', "/");
    if target.is_empty()
        || target.starts_with('#')
        || target.starts_with("//")
        || target.contains("://")
        || target.starts_with("mailto:")
        || target.starts_with("tel:")
    {
        return None;
    }

    let (path_part, fragment) = match target.find('#') {
        Some(index) => (&target[..index], &target[index..]),
        None => (target.as_str(), ""),
    };

    if path_part.is_empty() {
        return None;
    }

    let absolute = path_part.starts_with('/');
    let joined = if absolute {
        path_part.trim_start_matches('/').to_owned()
    } else {
        match from_note.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{path_part}"),
            None => path_part.to_owned(),
        }
    };

    Some(Resolved {
        path: normalize(&joined)?,
        fragment: fragment.to_owned(),
        absolute,
    })
}

/// Builds the destination text for a link from `from_note` to `target`.
pub(crate) fn format(from_note: &str, target: &str, fragment: &str, absolute: bool) -> String {
    if absolute {
        format!("/{target}{fragment}")
    } else {
        format!("{}{fragment}", relative(from_note, target))
    }
}

/// Path of `target` as seen from the directory of `from_note`.
fn relative(from_note: &str, target: &str) -> String {
    let mut from_dir: Vec<&str> = from_note.split('/').collect();
    from_dir.pop();

    let to: Vec<&str> = target.split('/').collect();
    let common = from_dir
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();

    let mut parts: Vec<&str> = vec![".."; from_dir.len() - common];
    parts.extend(&to[common..]);
    parts.join("/")
}

fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();

    for part in path.split('/') {
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

fn scan_line(line: &str, base: usize, out: &mut Vec<RawLink>) {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = run_len(bytes, i);
                match find_run(bytes, i + run, run) {
                    Some(close) => i = close + run,
                    None => i += run,
                }
            }
            b'[' if i == 0 || bytes[i - 1] != b'!' => match bracket_link(line, i) {
                Some((target, span, next)) => {
                    out.push(RawLink {
                        target,
                        span: (base + span.start)..(base + span.end),
                    });
                    i = next;
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }
}

/// Parses `[text](target)` starting at `open`, the index of `[`.
fn bracket_link(line: &str, open: usize) -> Option<(String, Range<usize>, usize)> {
    let bytes = line.as_bytes();
    let close_text = find_byte(bytes, open + 1, b']')?;
    if bytes.get(close_text + 1) != Some(&b'(') {
        return None;
    }
    let close_target = find_byte(bytes, close_text + 2, b')')?;
    let inner = &line[close_text + 2..close_target];

    let (target, start, end) = destination(inner, close_text + 2)?;
    Some((target, start..end, close_target + 1))
}

/// Extracts the destination from the inside of `(...)`.
///
/// Handles `<path with spaces>` and strips a trailing title
/// (`"title"`, `'title'`, or `(title)`), as CommonMark allows.
fn destination(inner: &str, offset: usize) -> Option<(String, usize, usize)> {
    let leading = inner.len() - inner.trim_start().len();
    let trimmed = &inner[leading..];
    let start = offset + leading;

    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix('<') {
        let close = rest.find('>')?;
        if close == 0 {
            return None;
        }
        return Some((rest[..close].to_owned(), start + 1, start + 1 + close));
    }

    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    if end == 0 {
        return None;
    }

    Some((trimmed[..end].to_owned(), start, start + end))
}

fn run_len(bytes: &[u8], start: usize) -> usize {
    bytes[start..].iter().take_while(|b| **b == b'`').count()
}

fn find_run(bytes: &[u8], from: usize, len: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let run = run_len(bytes, i);
            if run == len {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

fn find_byte(bytes: &[u8], from: usize, byte: u8) -> Option<usize> {
    bytes[from..]
        .iter()
        .position(|b| *b == byte)
        .map(|i| from + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(source: &str) -> Vec<String> {
        parse(source).into_iter().map(|link| link.target).collect()
    }

    #[test]
    fn parses_inline_links() {
        assert_eq!(
            targets("See [Calmar](projets/calmar.md) and [home](index.md)."),
            ["projets/calmar.md", "index.md"]
        );
    }

    #[test]
    fn strips_titles_and_angle_brackets() {
        assert_eq!(
            targets("[a](note.md \"title\") [b](<with space.md>)"),
            ["note.md", "with space.md"]
        );
    }

    #[test]
    fn ignores_images() {
        assert_eq!(
            targets("![schema](assets/schema.png)"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn ignores_fenced_code() {
        let source = "```\n[a](inside.md)\n```\n[out](outside.md)\n";
        assert_eq!(targets(source), ["outside.md"]);
    }

    #[test]
    fn ignores_inline_code() {
        assert_eq!(
            targets("write `[a](not-a-link.md)` then [b](real.md)"),
            ["real.md"]
        );
    }

    #[test]
    fn keeps_the_fragment_in_the_target() {
        assert_eq!(
            targets("[a](note.md#todo)"),
            ["note.md#todo"],
            "the target keeps its fragment so it can be rewritten as written"
        );
    }

    #[test]
    fn resolves_relative_to_the_note_directory() {
        let resolved = resolve("projets/calmar.md", "../index.md#intro").unwrap();
        assert_eq!(resolved.path, "index.md");
        assert_eq!(resolved.fragment, "#intro");
        assert!(!resolved.absolute);
    }

    #[test]
    fn resolves_vault_absolute_links() {
        let resolved = resolve("projets/calmar.md", "/journal/2026.md").unwrap();
        assert_eq!(resolved.path, "journal/2026.md");
        assert!(resolved.absolute);
    }

    #[test]
    fn rejects_external_links_and_anchors() {
        assert!(resolve("a.md", "https://example.org").is_none());
        assert!(resolve("a.md", "#section").is_none());
        assert!(resolve("a.md", "mailto:someone@example.org").is_none());
    }

    #[test]
    fn rejects_paths_escaping_the_vault() {
        assert!(resolve("a.md", "../../etc/passwd").is_none());
    }

    #[test]
    fn builds_relative_links() {
        assert_eq!(
            format("projets/calmar.md", "index.md", "#intro", false),
            "../index.md#intro"
        );
        assert_eq!(
            format("index.md", "projets/calmar.md", "", false),
            "projets/calmar.md"
        );
        assert_eq!(format("a/b/c.md", "a/b/d.md", "", false), "d.md");
        assert_eq!(
            format("index.md", "projets/calmar.md", "", true),
            "/projets/calmar.md"
        );
    }

    #[test]
    fn round_trips_resolution_and_formatting() {
        let resolved = resolve("projets/calmar.md", "../journal/2026.md#todo").unwrap();
        let text = format(
            "projets/calmar.md",
            &resolved.path,
            &resolved.fragment,
            resolved.absolute,
        );
        assert_eq!(text, "../journal/2026.md#todo");
    }
}
