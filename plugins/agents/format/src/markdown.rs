//! Markdown structure with line numbers: front matter, headings, code spans and fenced lines.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::yaml::{self, Node, Value};

/// Maps byte offsets to one-based line numbers.
#[derive(Debug)]
pub struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    /// The line starts of `text`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
            .collect();
        Self { starts }
    }

    /// The line of the byte at `offset`.
    #[must_use]
    pub fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset)
    }
}

/// The YAML block between the leading `---` lines.
#[derive(Debug)]
pub struct Frontmatter {
    /// The block, its nodes on the lines of the file.
    pub data: Node,
    /// The line of the closing `---`.
    pub end_line: usize,
}

/// The front matter of `text`: it opens with `---\n` and closes at the first line that is exactly
/// `---`; `None` when the text has none.
///
/// # Errors
/// The parser's message when the block is not valid YAML.
pub fn frontmatter(text: &str) -> Result<Option<Frontmatter>, String> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok(None);
    };
    let mut offset = 0;
    for (end_line, line) in (2..).zip(rest.split_inclusive('\n')) {
        if line.trim_end_matches('\n') == "---" {
            let mut data = yaml::parse(&rest[..offset])?;
            shift(&mut data);
            return Ok(Some(Frontmatter { data, end_line }));
        }
        offset += line.len();
    }
    Ok(None)
}

/// Moves every node one line down, past the opening `---`.
fn shift(node: &mut Node) {
    node.line += 1;
    match &mut node.value {
        Value::Sequence(items) => items.iter_mut().for_each(shift),
        Value::Mapping(entries) => entries.iter_mut().for_each(|(key, value)| {
            shift(key);
            shift(value);
        }),
        Value::Null | Value::Scalar(_) => {}
    }
}

/// The text after the front matter block, or all of it without one.
#[must_use]
pub fn body(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end_matches('\n') == "---" {
            return &rest[offset..];
        }
    }
    text
}

/// An inline code span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeSpan {
    pub text: String,
    pub line: usize,
}

/// A heading and its plain text (text and code, without markup).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

fn syntax() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_STRIKETHROUGH
}

/// The inline code spans of `text`, outside fenced and indented code blocks.
#[must_use]
pub fn code_spans(text: &str) -> Vec<CodeSpan> {
    let lines = Lines::new(text);
    Parser::new_ext(text, syntax())
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Code(code) => Some(CodeSpan {
                text: code.into_string(),
                line: lines.line(range.start),
            }),
            _ => None,
        })
        .collect()
}

/// The headings of `text`.
#[must_use]
pub fn headings(text: &str) -> Vec<Heading> {
    let lines = Lines::new(text);
    let mut headings = Vec::new();
    let mut current: Option<Heading> = None;
    for (event, range) in Parser::new_ext(text, syntax()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some(Heading {
                    level: level_number(level),
                    text: String::new(),
                    line: lines.line(range.start),
                });
            }
            Event::End(TagEnd::Heading(_)) => headings.extend(current.take()),
            Event::Text(part) | Event::Code(part) => {
                if let Some(heading) = current.as_mut() {
                    heading.text.push_str(&part);
                }
            }
            _ => {}
        }
    }
    headings
}

fn level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// For each line of `text` (as `str::lines` splits it), whether it lies in a fenced code block,
/// its fence lines included. A fence opens with three or more backticks or tildes after any
/// indentation and closes at a run of at least as many of the same character with nothing but
/// white space after it.
#[must_use]
pub fn fenced_lines(text: &str) -> Vec<bool> {
    let mut inside = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = trimmed
            .chars()
            .next()
            .filter(|first| matches!(first, '`' | '~'));
        let run = marker.map_or(0, |first| {
            trimmed.chars().take_while(|c| *c == first).count()
        });
        match (fence, marker) {
            (None, Some(first)) if run >= 3 => {
                fence = Some((first, run));
                inside.push(true);
            }
            (Some((first, length)), Some(current))
                if current == first && run >= length && trimmed[run..].trim().is_empty() =>
            {
                fence = None;
                inside.push(true);
            }
            (Some(_), _) => inside.push(true),
            (None, _) => inside.push(false),
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::{body, fenced_lines, frontmatter, headings};

    #[test]
    fn front_matter_keeps_file_lines() {
        let text = "---\ntitle: A\nlist:\n  - x\n---\n# A\n";
        let block = frontmatter(text).expect("valid").expect("present");
        assert_eq!(block.end_line, 5);
        assert_eq!(block.data.get("title").map(|node| node.line), Some(2));
        assert_eq!(body(text), "# A\n");
        assert!(frontmatter("---\na: [\n---\n").is_err());
        assert!(frontmatter("# none\n").expect("valid").is_none());
    }

    #[test]
    fn headings_and_fences_are_read_with_lines() {
        let text = "# One `two`\n\n```\n# not\n```\ntext\n";
        let found = headings(text);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].text.as_str(), found[0].line), ("One two", 1));
        assert_eq!(fenced_lines(text), [false, false, true, true, true, false]);
    }
}
