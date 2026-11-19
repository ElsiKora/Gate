//! The `owner-chapter` aspect: every registry row whose owner lies below the source root links
//! exactly one local Markdown chapter below the documentation root.
//!
//! The registry is read with a `CommonMark` parser and the syntax a Markdown formatter reads (GFM
//! tables and strikethrough, footnotes, math and wiki links), with its front matter blanked. A
//! registry table is a table whose trimmed header cells are `table-header`. A row's owner is the
//! first top-level code span of its first cell; its links are the destinations of the links in its
//! second cell (an undefined reference counts as a link without a destination, a wiki link does
//! not) and each bare URL in that cell's plain text.

use std::ops::Range;

use pulldown_cmark::{BrokenLink, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};

/// One body row of a registry table.
#[derive(Debug)]
struct Row {
    owner: String,
    /// Destinations of every link in the chapter cell; an unresolved reference is empty.
    links: Vec<String>,
    line: usize,
}

/// The table being read: its header cells, then its body rows.
#[derive(Debug, Default)]
struct Table {
    header: Vec<String>,
    in_head: bool,
    rows: Vec<Row>,
    row: Option<Row>,
    cell: usize,
    /// Inline nesting below the current cell; an owner is a top-level code span of the first cell.
    depth: usize,
    /// Open links in the current cell; their text is never a bare URL.
    links: usize,
}

/// The line of each byte offset.
#[derive(Debug)]
struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(offset, _)| offset + 1));
        Self { starts }
    }

    fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset)
    }
}

/// The layout of the check.
#[derive(Debug)]
pub(super) struct Chapters<'o> {
    /// The registry page.
    pub registry: &'o str,
    /// The header cells of a registry table.
    pub header: &'o [String],
    /// The folder a checked owner lies below.
    pub source_root: &'o str,
    /// The folder a chapter lies below.
    pub documentation_root: &'o str,
}

impl Chapters<'_> {
    /// The line and message of every checked row of `text`, the registry, that does not link one
    /// local chapter.
    pub fn missing(&self, text: &str) -> Vec<(usize, String)> {
        let source = crate::layout::folder_prefix(self.source_root);
        let documentation = crate::layout::folder_prefix(self.documentation_root);
        let mut missing = Vec::new();

        for row in self.rows(text) {
            if !row.owner.starts_with(&source) {
                continue;
            }

            let chapter = match row.links.as_slice() {
                [url] => local_target(url).and_then(|target| resolve(self.registry, &target)),
                _ => None,
            };

            if !chapter
                .is_some_and(|chapter| chapter.starts_with(&documentation) && is_markdown(&chapter))
            {
                missing.push((
                    row.line,
                    format!(
                        "{} {} must link one local documentation chapter",
                        self.registry, row.owner
                    ),
                ));
            }
        }

        missing
    }

    /// The body rows of every registry table of `text`.
    fn rows(&self, text: &str) -> Vec<Row> {
        let lines = Lines::new(text);
        // An undefined reference is still a link, with no destination.
        let callback = |_: BrokenLink<'_>| Some((CowStr::Borrowed(""), CowStr::Borrowed("")));
        let source = without_front_matter(text);
        let parser = Parser::new_with_broken_link_callback(&source, options(), Some(callback));
        let mut table: Option<Table> = None;
        let mut rows = Vec::new();

        for (event, range) in parser.into_offset_iter() {
            self.read(&mut table, &event, &range, &lines, &mut rows);
        }

        rows
    }

    /// Reads one event into the table being read; a finished registry table's rows go to `rows`.
    fn read(
        &self,
        table: &mut Option<Table>,
        event: &Event<'_>,
        range: &Range<usize>,
        lines: &Lines,
        rows: &mut Vec<Row>,
    ) {
        match event {
            Event::Start(Tag::Table(_)) => *table = Some(Table::default()),
            Event::End(TagEnd::Table) => {
                if let Some(finished) = table.take()
                    && finished.header.as_slice() == self.header
                {
                    rows.extend(finished.rows);
                }
            }
            _ => {}
        }

        let Some(current) = table.as_mut() else {
            return;
        };

        match event {
            Event::Start(Tag::TableHead) => current.in_head = true,
            Event::End(TagEnd::TableHead) => current.in_head = false,
            Event::Start(Tag::TableRow) => {
                current.row = Some(Row {
                    owner: String::new(),
                    links: Vec::new(),
                    line: lines.line(range.start),
                });
                current.cell = 0;
            }
            Event::End(TagEnd::TableRow) => current.rows.extend(current.row.take()),
            Event::Start(Tag::TableCell) => {
                current.depth = 0;
                current.links = 0;

                if current.in_head {
                    current.header.push(String::new());
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(header) = current.header.last_mut().filter(|_| current.in_head) {
                    *header = header.trim().to_owned();
                }

                current.cell += 1;
            }
            Event::Start(tag) => {
                if let (
                    Some(row),
                    Tag::Link {
                        link_type,
                        dest_url,
                        ..
                    },
                ) = (current.row.as_mut(), tag)
                    && current.cell == 1
                    && !matches!(link_type, LinkType::WikiLink { .. })
                {
                    row.links.push(dest_url.to_string());
                }

                if matches!(tag, Tag::Link { .. }) {
                    current.links += 1;
                }

                current.depth += 1;
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::Link) {
                    current.links = current.links.saturating_sub(1);
                }

                current.depth = current.depth.saturating_sub(1);
            }
            Event::Text(part) | Event::Code(part) if current.in_head && current.depth == 0 => {
                if let Some(header) = current.header.last_mut() {
                    header.push_str(part);
                }
            }
            Event::Code(code) if current.cell == 0 && current.depth == 0 => {
                if let Some(row) = current.row.as_mut().filter(|row| row.owner.is_empty()) {
                    row.owner = code.to_string();
                }
            }
            // A bare URL in text is a link of its own.
            Event::Text(part) if current.cell == 1 && current.links == 0 => {
                if let Some(row) = current.row.as_mut() {
                    row.links.extend(bare_urls(part));
                }
            }
            _ => {}
        }
    }
}

/// A `.md` file; the extension is case-sensitive like the destinations it is compared with.
fn is_markdown(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "md")
}

/// Bare `http://`, `https://`, `mailto:` and `www.` URLs followed by at least one character that
/// can continue a URL.
fn bare_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();

    for prefix in ["http://", "https://", "mailto:", "www."] {
        for (index, _) in text.match_indices(prefix) {
            let rest = text.get(index + prefix.len()..).unwrap_or_default();

            if rest
                .chars()
                .next()
                .is_some_and(|next| !next.is_whitespace() && next != '<')
            {
                urls.push(prefix.to_owned());
            }
        }
    }

    urls
}

/// The text with its front matter blanked, keeping every offset. Front matter starts with `---` or
/// `+++` at the start of the document (after a byte order mark or blank lines), runs to the first
/// later line that starts with the same delimiter, or for YAML to a line starting with `...`, and
/// ends after the three delimiter characters.
fn without_front_matter(text: &str) -> String {
    let mut start = text
        .strip_prefix('\u{feff}')
        .map_or(0, |_| '\u{feff}'.len_utf8());

    while let Some(line) = text
        .get(start..)
        .and_then(|rest| rest.split_inclusive('\n').next())
    {
        if line.ends_with('\n') && line.trim().is_empty() {
            start += line.len();
        } else {
            break;
        }
    }

    let rest = text.get(start..).unwrap_or_default();
    let Some(delimiter) = ["---", "+++"]
        .into_iter()
        .find(|mark| rest.starts_with(mark))
    else {
        return text.to_owned();
    };
    let Some(opening_end) = rest.find('\n') else {
        return text.to_owned();
    };
    let language = rest
        .get(delimiter.len()..opening_end)
        .unwrap_or_default()
        .trim();
    let after = rest.get(opening_end..).unwrap_or_default();
    let closing = after.find(&format!("\n{delimiter}")).or_else(|| {
        (delimiter == "---" && (language.is_empty() || language == "yaml"))
            .then(|| after.find("\n..."))
            .flatten()
    });
    let Some(closing) = closing else {
        return text.to_owned();
    };
    let end = start + opening_end + closing + 1 + delimiter.len();
    let mut bytes = text.as_bytes().to_vec();

    for byte in bytes.get_mut(start..end).into_iter().flatten() {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }

    String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned())
}

/// GFM tables and strikethrough, footnotes, math and wiki links, none of which is a link
/// destination.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS
}

/// The decoded file part of a relative destination; `None` for empty, absolute, fragment-only,
/// scheme and wildcard destinations.
fn local_target(url: &str) -> Option<String> {
    let has_scheme = url
        .split_once(':')
        .is_some_and(|(scheme, _)| is_scheme(scheme));

    if url.is_empty() || url.starts_with('/') || url.starts_with('#') || has_scheme {
        return None;
    }

    let target = url.split(['?', '#']).next().unwrap_or_default();

    if target.is_empty() || target.contains('*') {
        return None;
    }

    Some(percent_decode(target).unwrap_or_else(|| target.to_owned()))
}

/// A URL scheme: a letter, then letters, digits, `+`, `.` and `-`.
fn is_scheme(candidate: &str) -> bool {
    let mut characters = candidate.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-')
        })
}

/// Decodes `%XX` sequences as UTF-8; `None` when a sequence or the result is malformed.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while let Some(byte) = bytes.get(index) {
        if *byte == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;

            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }

            decoded.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            index += 3;
        } else {
            decoded.push(*byte);
            index += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

/// The project path of `target` relative to the folder of `document`; `None` when it leaves the
/// project. The project root is the empty path.
fn resolve(document: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = document.split('/').collect();
    parts.pop();

    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }

    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{local_target, resolve, without_front_matter};

    #[test]
    fn local_targets_drop_queries_and_decode_percent_escapes() {
        assert_eq!(
            local_target("../file%20name.md?view=1#part").as_deref(),
            Some("../file name.md")
        );
        assert_eq!(local_target("../100%.md").as_deref(), Some("../100%.md"));
        assert_eq!(local_target("a%+5.md").as_deref(), Some("a%+5.md"));
        assert_eq!(local_target("custom+scheme:data"), None);
        assert_eq!(local_target("examples/*.md"), None);
        assert_eq!(local_target("?query"), None);
    }

    #[test]
    fn resolution_stays_inside_the_project() {
        assert_eq!(
            resolve("docs/a/b.md", "../c.md").as_deref(),
            Some("docs/c.md")
        );
        assert_eq!(resolve("docs/b.md", "../../c.md"), None);
        assert_eq!(resolve("docs/b.md", "..").as_deref(), Some(""));
    }

    #[test]
    fn front_matter_is_blanked_keeping_offsets() {
        let text = "---\ntitle: x\n---\n# Book\n";

        assert_eq!(without_front_matter(text), "   \n        \n   \n# Book\n");
        assert_eq!(without_front_matter("# Book\n---\n"), "# Book\n---\n");
    }
}
