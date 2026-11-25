//! Markdown styling for the source editor.
//!
//! iced's text editor renders plain text with per-span formats, not blocks.
//! So this is syntax highlighting, not a WYSIWYG rendering: headings, code,
//! links and emphasis are styled in place, but the Markdown markers stay
//! visible and the font size does not change.

use std::ops::Range;

use iced::advanced::text::highlighter::{Format, Highlighter};
use iced::font::{Style, Weight};
use iced::{Color, Font, Theme};

/// What a span of the line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Highlight {
    Heading,
    Bold,
    Italic,
    Code,
    Link,
    Url,
    Marker,
    Quote,
    ListMarker,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MarkdownHighlighter {
    current_line: usize,
}

impl Highlighter for MarkdownHighlighter {
    type Settings = ();
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(_settings: &Self::Settings) -> Self {
        Self::default()
    }

    fn update(&mut self, _new_settings: &Self::Settings) {}

    fn change_line(&mut self, line: usize) {
        self.current_line = line;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.current_line += 1;
        highlight(line).into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

/// Turns a highlight into the format the editor applies.
///
/// Colors come from the active theme, so the source stays readable whichever
/// theme the application uses.
pub fn format(highlight: &Highlight, theme: &Theme) -> Format<Font> {
    let palette = theme.extended_palette();
    let bold = Font {
        weight: Weight::Bold,
        ..Font::DEFAULT
    };
    let muted = Color {
        a: 0.6,
        ..palette.background.base.text
    };

    match highlight {
        Highlight::Heading => Format {
            color: Some(palette.primary.base.color),
            font: Some(bold),
        },
        Highlight::Bold => Format {
            color: None,
            font: Some(bold),
        },
        Highlight::Italic => Format {
            color: None,
            font: Some(Font {
                style: Style::Italic,
                ..Font::DEFAULT
            }),
        },
        Highlight::Code => Format {
            color: Some(palette.danger.base.color),
            font: Some(Font::MONOSPACE),
        },
        Highlight::Link => Format {
            color: Some(palette.primary.base.color),
            font: None,
        },
        Highlight::Url => Format {
            color: Some(muted),
            font: None,
        },
        Highlight::Marker => Format {
            color: Some(muted),
            font: None,
        },
        Highlight::Quote => Format {
            color: Some(muted),
            font: None,
        },
        Highlight::ListMarker => Format {
            color: Some(palette.primary.base.color),
            font: Some(bold),
        },
    }
}

/// The styled ranges of one line.
fn highlight(line: &str) -> Vec<(Range<usize>, Highlight)> {
    let mut spans = Vec::new();

    if let Some((marker, heading)) = heading(line) {
        spans.push((marker, Highlight::Marker));
        spans.push((heading, Highlight::Heading));
        return spans;
    }

    if let Some(rest) = line.strip_prefix('>') {
        spans.push((0..1, Highlight::Marker));
        spans.push((1..1 + rest.len(), Highlight::Quote));
        return spans;
    }

    let indent = line.len() - line.trim_start().len();
    if let Some(len) = list_marker(&line[indent..]) {
        spans.push((indent..indent + len, Highlight::ListMarker));
    }

    scan_inline(line, &mut spans);
    spans
}

/// `# Title` becomes (0..1 marker, 1..len heading).
fn heading(line: &str) -> Option<(Range<usize>, Range<usize>)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = &line[indent..];
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();

    let valid = (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ');
    valid.then(|| (indent..indent + hashes, indent + hashes..line.len()))
}

/// `- `, `* ` and `+ ` start a list item.
fn list_marker(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let bullet = matches!(bytes.first(), Some(b'-' | b'*' | b'+'));

    (bullet && bytes.get(1) == Some(&b' ')).then_some(2)
}

fn scan_inline(line: &str, spans: &mut Vec<(Range<usize>, Highlight)>) {
    let bytes = line.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'`'
            && let Some(end) = line[index + 1..].find('`')
        {
            let end = index + 1 + end;
            spans.push((index..end + 1, Highlight::Code));
            index = end + 1;
            continue;
        }

        if line[index..].starts_with("**")
            && let Some(end) = line[index + 2..].find("**")
        {
            let end = index + 2 + end;
            spans.push((index..index + 2, Highlight::Marker));
            spans.push((index + 2..end, Highlight::Bold));
            spans.push((end..end + 2, Highlight::Marker));
            index = end + 2;
            continue;
        }

        if matches!(bytes[index], b'*' | b'_')
            && let Some(end) = line[index + 1..].find(bytes[index] as char)
        {
            let end = index + 1 + end;
            spans.push((index..index + 1, Highlight::Marker));
            spans.push((index + 1..end, Highlight::Italic));
            spans.push((end..end + 1, Highlight::Marker));
            index = end + 1;
            continue;
        }

        if bytes[index] == b'['
            && let Some((text_end, end)) = link_bounds(line, index)
        {
            spans.push((index..text_end + 1, Highlight::Link));
            spans.push((text_end + 1..end + 1, Highlight::Url));
            index = end + 1;
            continue;
        }

        index += 1;
    }
}

/// Bounds of `[text](url)` starting at `start`: (end of text, end of url).
fn link_bounds(line: &str, start: usize) -> Option<(usize, usize)> {
    let close_text = start + line[start + 1..].find("](")? + 1;
    let close_url = close_text + 2 + line[close_text + 2..].find(')')?;

    Some((close_text, close_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(line: &str) -> Vec<(Range<usize>, Highlight)> {
        highlight(line)
    }

    #[test]
    fn a_heading_gets_a_marker_and_a_heading_span() {
        assert_eq!(
            spans("# Title"),
            [(0..1, Highlight::Marker), (1..7, Highlight::Heading)]
        );
    }

    #[test]
    fn a_hashtag_is_not_a_heading() {
        assert!(spans("#hashtag").is_empty());
    }

    #[test]
    fn bold_markers_are_dimmed() {
        assert_eq!(
            spans("**bold**"),
            [
                (0..2, Highlight::Marker),
                (2..6, Highlight::Bold),
                (6..8, Highlight::Marker),
            ]
        );
    }

    #[test]
    fn italic_markers_are_dimmed() {
        assert_eq!(
            spans("*it*"),
            [
                (0..1, Highlight::Marker),
                (1..3, Highlight::Italic),
                (3..4, Highlight::Marker),
            ]
        );
    }

    #[test]
    fn inline_code_is_monospaced() {
        assert_eq!(spans("`code`"), [(0..6, Highlight::Code)]);
    }

    #[test]
    fn a_link_separates_text_and_url() {
        assert_eq!(
            spans("[a](b)"),
            [(0..3, Highlight::Link), (3..6, Highlight::Url)]
        );
    }

    #[test]
    fn a_list_marker_is_styled() {
        assert_eq!(spans("- item")[0], (0..2, Highlight::ListMarker));
    }

    #[test]
    fn a_quote_is_dimmed() {
        assert_eq!(
            spans("> note"),
            [(0..1, Highlight::Marker), (1..6, Highlight::Quote)]
        );
    }

    #[test]
    fn plain_text_has_no_highlight() {
        assert!(spans("just words").is_empty());
    }

    #[test]
    fn colors_come_from_the_theme() {
        let heading = format(&Highlight::Heading, &Theme::Dark);
        let bold = format(&Highlight::Bold, &Theme::Dark);

        assert!(heading.color.is_some(), "headings are colored");
        assert!(bold.color.is_none(), "bold keeps the normal text color");
    }
}
