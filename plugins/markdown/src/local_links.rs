//! `markdown/local-links`: every local link destination of a documentation page names a tracked
//! file or directory.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};
use pulldown_cmark::{BrokenLink, CowStr, Event, LinkType, Parser, Tag};

use crate::pages::{default_roots, pages, strict_text};
use crate::text::{Lines, without_front_matter};

/// The rule.
#[derive(Debug, Default)]
pub struct LocalLinks;

/// The options of `markdown/local-links`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The directories, relative to the project, whose tracked `.md` files are checked; an empty
    /// string is the whole project.
    pub roots: Vec<String>,
    /// Further tracked files to check, whatever their extension; a file that is not tracked is
    /// skipped.
    pub extra_files: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            roots: default_roots(),
            extra_files: Vec::new(),
        }
    }
}

impl Rule for LocalLinks {
    type Options = Options;

    const NAME: &'static str = "local-links";
    const TITLE: &'static str =
        "Local link destinations of documentation pages name a tracked file or directory";

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let tree = run.tree();
        let tracked: BTreeSet<String> = tree.paths().into_iter().collect();
        let pages = pages(&tree, &run.options().roots, &run.options().extra_files);

        for page in pages {
            let Some(text) = strict_text(&tree, &page)? else {
                continue;
            };
            for destination in destinations(&text) {
                let Some(target) = local_target(&destination.url) else {
                    continue;
                };
                let exists = resolve(&page, &target).is_some_and(|resolved| {
                    resolved.is_empty()
                        || tracked.contains(&resolved)
                        || is_directory(&tracked, &resolved)
                });
                if !exists {
                    run.report()
                        .finding(
                            "",
                            Location::line(&page, destination.line),
                            format!("{page} links to missing file: {}", destination.url),
                        )
                        .key(destination.url.as_str())
                        .target(format!("{page}:{}", destination.url))
                        .help(
                            "point the link at a tracked file or directory, relative to the page; \
                             if the target moved, follow it, and if it is gone, remove the link",
                        );
                }
            }
        }

        Ok(())
    }
}

/// Whether some tracked path lies below `directory`.
fn is_directory(tracked: &BTreeSet<String>, directory: &str) -> bool {
    let prefix = format!("{directory}/");
    tracked
        .range(prefix.clone()..)
        .next()
        .is_some_and(|path| path.starts_with(&prefix))
}

/// The destination of a link, an image or a link reference definition, and its line.
#[derive(Debug, PartialEq, Eq)]
struct Destination {
    url: String,
    line: usize,
}

/// The Markdown the documentation is written in, as prettier reads it: GFM tables and
/// strikethrough, footnotes, math and wiki links. None of the last three has a destination.
fn syntax() -> pulldown_cmark::Options {
    pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_FOOTNOTES
        | pulldown_cmark::Options::ENABLE_MATH
        | pulldown_cmark::Options::ENABLE_WIKILINKS
}

/// Every destination of `text`: inline links and images and autolinks in document order, then
/// each link reference definition by line. Front matter is not read. An e-mail autolink
/// (`<someone@mail.example>`) is a `mailto:` link, never a file, so it is left out.
fn destinations(text: &str) -> Vec<Destination> {
    let lines = Lines::new(text);
    let source = without_front_matter(text);
    // A reference to an undefined label is still a link, with no destination, as remark reads it.
    let undefined = |_: BrokenLink<'_>| Some((CowStr::Borrowed(""), CowStr::Borrowed("")));
    let mut events = Parser::new_with_broken_link_callback(&source, syntax(), Some(undefined))
        .into_offset_iter();
    let mut found = Vec::new();
    let mut coverage = Coverage::default();

    for (event, range) in events.by_ref() {
        if let Event::Start(
            Tag::Link {
                link_type,
                dest_url,
                ..
            }
            | Tag::Image {
                link_type,
                dest_url,
                ..
            },
        ) = &event
            && matches!(link_type, LinkType::Inline | LinkType::Autolink)
        {
            found.push(Destination {
                url: dest_url.to_string(),
                line: lines.line(range.start),
            });
        }
        coverage.record(&event, range);
    }

    // The parser keeps the first definition of each label; the uncovered gaps hold the rest. A
    // line holds at most one definition, so the line is the key.
    let mut definitions: BTreeMap<usize, String> = events
        .reference_definitions()
        .iter()
        .map(|(_, definition)| {
            (
                lines.line(definition.span.start),
                definition.dest.to_string(),
            )
        })
        .collect();
    for (offset, url) in coverage.definitions(&source) {
        definitions.entry(lines.line(offset)).or_insert(url);
    }
    found.extend(
        definitions
            .into_iter()
            .map(|(line, url)| Destination { url, line }),
    );
    found
}

/// The source ranges the parser's events cover. A link reference definition yields no event, so
/// every definition lies in a gap between the events of its block, including a repeated label the
/// parser drops after its first definition.
#[derive(Debug, Default)]
struct Coverage {
    open: Vec<(Range<usize>, Vec<Range<usize>>)>,
    top: Vec<Range<usize>>,
    gaps: Vec<Range<usize>>,
}

impl Coverage {
    fn record(&mut self, event: &Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(_) => self.open.push((range, Vec::new())),
            Event::End(_) => {
                if let Some((block, mut children)) = self.open.pop() {
                    self.gaps.extend(uncovered(&block, &mut children));
                    self.children().push(block);
                }
            }
            _ => self.children().push(range),
        }
    }

    fn children(&mut self) -> &mut Vec<Range<usize>> {
        match self.open.last_mut() {
            Some((_, children)) => children,
            None => &mut self.top,
        }
    }

    /// The offset and destination of every definition in the gaps, in source order.
    fn definitions(mut self, source: &str) -> Vec<(usize, String)> {
        let mut top = std::mem::take(&mut self.top);
        self.gaps.extend(uncovered(&(0..source.len()), &mut top));
        self.gaps.sort_by_key(|gap| gap.start);
        let mut found = Vec::new();

        for gap in &self.gaps {
            let Some(part) = source.get(gap.clone()).filter(|part| part.contains("]:")) else {
                continue;
            };
            let mut buffer = part.as_bytes().to_vec();
            // Each parse yields the first definition of each label; blanking them shows repeats.
            while let Ok(current) = std::str::from_utf8(&buffer) {
                let mut spans: Vec<(Range<usize>, String)> = Parser::new_ext(current, syntax())
                    .reference_definitions()
                    .iter()
                    .map(|(_, definition)| (definition.span.clone(), definition.dest.to_string()))
                    .collect();
                if spans.is_empty() {
                    break;
                }
                spans.sort_by_key(|(span, _)| span.start);
                for (span, url) in spans {
                    found.push((gap.start + span.start, url));
                    for byte in buffer.get_mut(span).into_iter().flatten() {
                        if *byte != b'\n' {
                            *byte = b' ';
                        }
                    }
                }
            }
        }

        found
    }
}

/// The parts of `block` none of `children` covers.
fn uncovered(block: &Range<usize>, children: &mut [Range<usize>]) -> Vec<Range<usize>> {
    children.sort_by_key(|child| child.start);
    let mut gaps = Vec::new();
    let mut cursor = block.start;
    for child in children.iter() {
        if child.start > cursor {
            gaps.push(cursor..child.start);
        }
        cursor = cursor.max(child.end);
    }
    if block.end > cursor {
        gaps.push(cursor..block.end);
    }
    gaps
}

/// The decoded file part of a relative destination; `None` for an empty, absolute, fragment-only,
/// scheme or wildcard destination.
fn local_target(url: &str) -> Option<String> {
    let scheme = url
        .split_once(':')
        .is_some_and(|(scheme, _)| is_scheme(scheme));
    if url.is_empty() || url.starts_with(['/', '#']) || scheme {
        return None;
    }
    let file = url.split(['?', '#']).next().unwrap_or_default();
    if file.is_empty() || file.contains('*') {
        return None;
    }
    Some(percent_decoded(file).unwrap_or_else(|| file.to_owned()))
}

/// A URI scheme: a letter, then letters, digits, `+`, `.` or `-`.
fn is_scheme(candidate: &str) -> bool {
    let mut characters = candidate.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters
            .all(|character| character.is_ascii_alphanumeric() || "+.-".contains(character))
}

/// `%XX` escapes decoded as UTF-8; `None` when an escape or the result is malformed.
fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let digits = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            if !digits.bytes().all(|digit| digit.is_ascii_hexdigit()) {
                return None;
            }
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// The project path of `target` relative to the directory of `page`; `None` when it leaves the
/// project. The project root is the empty path.
fn resolve(page: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = page.split('/').collect();
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
    use super::{Destination, destinations, local_target, resolve};

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
        assert_eq!(local_target("#part"), None);
        assert_eq!(local_target("/abs.md"), None);
        assert_eq!(local_target("1x:y.md").as_deref(), Some("1x:y.md"));
    }

    #[test]
    fn resolution_stays_inside_the_project() {
        assert_eq!(
            resolve("docs/a/b.md", "../c.md").as_deref(),
            Some("docs/c.md")
        );
        assert_eq!(resolve("docs/b.md", "../../c.md"), None);
        assert_eq!(resolve("docs/b.md", "..").as_deref(), Some(""));
        assert_eq!(
            resolve("docs/b.md", "./x//y.md").as_deref(),
            Some("docs/x/y.md")
        );
    }

    #[test]
    fn repeated_reference_labels_are_each_read() {
        let found = destinations("[a]: one.md\n[a]: two.md\n\n> [b]: three.md\n");
        assert_eq!(
            found,
            [
                Destination {
                    url: "one.md".to_owned(),
                    line: 1
                },
                Destination {
                    url: "two.md".to_owned(),
                    line: 2
                },
                Destination {
                    url: "three.md".to_owned(),
                    line: 4
                },
            ]
        );
    }
}
