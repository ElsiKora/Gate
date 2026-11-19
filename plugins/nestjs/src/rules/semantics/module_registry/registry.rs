//! The registry page as the rule reads it: table rows outside fenced blocks and HTML comments,
//! after the header and its delimiter row, and the source map section of each linked chapter.

use std::collections::{BTreeMap, BTreeSet};

use gate_typescript::javascript::{is_space, trim};

use super::Options;

/// The JavaScript line terminators, which `.` and `^` in a multiline pattern stop at.
const LINE_TERMINATORS: [char; 4] = ['\n', '\r', '\u{2028}', '\u{2029}'];

/// One finding on the registry page.
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    /// The aspect.
    pub aspect: &'static str,
    /// The line of the registry, when the finding has one.
    pub line: Option<usize>,
    /// The message.
    pub message: String,
}

impl Finding {
    fn new(aspect: &'static str, line: Option<usize>, message: String) -> Self {
        Self {
            aspect,
            line,
            message,
        }
    }

    /// A short fix hint for the finding's aspect.
    pub fn aspect_help(&self) -> &'static str {
        match self.aspect {
            "registry-chapter" => {
                "link the module's own chapter and name the module file in its source map section"
            }
            "registry-coverage" => "give every native module exactly one row, and no other row",
            _ => "keep one module table with one well-formed row per module",
        }
    }
}

/// The registry and how its rows and chapters read.
#[derive(Debug)]
pub struct Registry {
    /// The registry page.
    pub path: String,
    /// The chapter folder with a trailing slash.
    chapter_root: String,
    chapter_index: String,
    header: Vec<String>,
    source_map_heading: String,
    /// The source root with a trailing slash; empty for the project root.
    source_prefix: String,
}

impl Registry {
    /// The registry the options and the source root describe.
    pub fn new(options: &Options, source_root: &str) -> Self {
        let folder = |path: &str| {
            let path = path.trim_end_matches('/');

            if path.is_empty() {
                String::new()
            } else {
                format!("{path}/")
            }
        };

        Self {
            path: options.registry.clone(),
            chapter_root: folder(&options.chapter_root),
            chapter_index: options.chapter_index.clone(),
            header: options.table_header.clone(),
            source_map_heading: options.source_map_heading.clone(),
            source_prefix: folder(source_root),
        }
    }

    /// The findings of the registry text against the native modules: table rows outside fenced
    /// blocks and HTML comments, after the header and its delimiter row; a missing registry is the
    /// only finding. `chapter` reads a tracked page.
    pub fn findings(
        &self,
        text: Option<&str>,
        modules: &BTreeSet<String>,
        chapter: impl Fn(&str) -> Option<String>,
    ) -> Vec<Finding> {
        let registry = &self.path;
        let Some(text) = text else {
            return vec![Finding::new(
                "registry-shape",
                None,
                format!("{registry} is missing the module registry"),
            )];
        };
        let lines: Vec<String> = blank_fenced(text).split('\n').map(str::to_owned).collect();
        let mut findings = Vec::new();
        let mut rows: BTreeMap<String, usize> = BTreeMap::new();
        let mut in_table = false;
        let mut has_table = false;
        let mut index = 0;

        while index < lines.len() {
            let raw = &lines[index];
            let number = index + 1;
            index += 1;

            if !is_table_line(raw) {
                in_table = false;
                continue;
            }

            let line = trim(raw);

            if self.is_header(line) {
                in_table = lines.get(number).is_some_and(|next| is_delimiter(next));

                if in_table {
                    has_table = true;
                    index += 1;
                } else {
                    findings.push(Finding::new(
                        "registry-shape",
                        Some(number + 1),
                        format!(
                            "{registry}:{} has a malformed module table delimiter",
                            number + 1
                        ),
                    ));
                }

                continue;
            }

            if !in_table {
                continue;
            }

            findings.extend(self.row(line, number, modules, &mut rows, &chapter));
        }

        findings.extend(self.table_findings(has_table, &rows, modules));
        findings
    }

    /// The findings of one table row: its shape, a duplicate path and its chapter.
    fn row(
        &self,
        line: &str,
        number: usize,
        modules: &BTreeSet<String>,
        rows: &mut BTreeMap<String, usize>,
        chapter: &impl Fn(&str) -> Option<String>,
    ) -> Vec<Finding> {
        let registry = &self.path;
        let cells: Vec<&str> = if line.ends_with('|') {
            line.get(1..line.len() - 1)
                .unwrap_or_default()
                .split('|')
                .map(trim)
                .collect()
        } else {
            Vec::new()
        };
        let path = cells.first().and_then(|cell| self.module_path(cell));
        let Some(path) = path.filter(|path| {
            cells.len() == 3
                && cells.iter().all(|cell| !cell.is_empty())
                && !path.split('/').any(|part| matches!(part, "" | "." | ".."))
        }) else {
            return vec![Finding::new(
                "registry-shape",
                Some(number),
                format!("{registry}:{number} has a malformed module row"),
            )];
        };
        let mut findings = Vec::new();

        if rows.contains_key(path) {
            findings.push(Finding::new(
                "registry-shape",
                Some(number),
                format!("{registry} contains duplicate module path: {path}"),
            ));
        } else {
            rows.insert(path.to_owned(), number);
        }

        let Some(destination) = chapter_link(cells[1]) else {
            findings.push(Finding::new(
                "registry-shape",
                Some(number),
                format!("{registry}:{number} has a malformed module chapter link"),
            ));
            return findings;
        };

        findings.extend(self.chapter_finding(number, path, destination, modules, chapter));
        findings
    }

    /// A registry without its table, a native module without a row, and a row without a native
    /// module.
    fn table_findings(
        &self,
        has_table: bool,
        rows: &BTreeMap<String, usize>,
        modules: &BTreeSet<String>,
    ) -> Vec<Finding> {
        let registry = &self.path;
        let mut findings = Vec::new();

        if !has_table {
            findings.push(Finding::new(
                "registry-shape",
                None,
                format!(
                    "{registry} is missing the {} table",
                    self.header.join(" / ")
                ),
            ));
        }

        for path in modules.iter().filter(|path| !rows.contains_key(*path)) {
            findings.push(Finding::new(
                "registry-coverage",
                None,
                format!("{registry} is missing source module: {path}"),
            ));
        }

        for (path, line) in rows.iter().filter(|(path, _)| !modules.contains(*path)) {
            findings.push(Finding::new(
                "registry-coverage",
                Some(*line),
                format!("{registry} references unknown source module: {path}"),
            ));
        }

        findings
    }

    /// The chapter finding of one row: the chapter index is no chapter, and the chapter of a
    /// native module exists and maps it.
    fn chapter_finding(
        &self,
        number: usize,
        path: &str,
        destination: &str,
        modules: &BTreeSet<String>,
        chapter: &impl Fn(&str) -> Option<String>,
    ) -> Option<Finding> {
        let registry = &self.path;
        let page = normalize(&format!("{}{destination}", self.chapter_root));
        let message = if page == self.chapter_index {
            format!("{registry}:{number} must link a substantive module chapter: {path}")
        } else if page.starts_with(&self.chapter_root) && modules.contains(path) {
            match chapter(&page) {
                None => format!("{registry}:{number} links missing module chapter: {destination}"),
                Some(text) if !self.source_map_contains(&text, path) => {
                    format!("{registry}:{number} chapter does not map source module: {path}")
                }
                Some(_) => return None,
            }
        } else {
            return None;
        };

        Some(Finding::new("registry-chapter", Some(number), message))
    }

    /// Whether the trimmed line is the header row: `|`, the header cells, each trimmed, and `|`.
    fn is_header(&self, line: &str) -> bool {
        line.len() >= 2
            && line.starts_with('|')
            && line.ends_with('|')
            && line[1..line.len() - 1]
                .split('|')
                .map(trim)
                .eq(self.header.iter().map(String::as_str))
    }

    /// The path of a module path cell: a code span of a `.ts` path below the source root without
    /// a backtick, `*`, `|` or white space.
    fn module_path<'c>(&self, cell: &'c str) -> Option<&'c str> {
        let path = cell.strip_prefix('`')?.strip_suffix('`')?;
        let name = path
            .strip_prefix(self.source_prefix.as_str())?
            .strip_suffix(".ts")?;

        (!name.is_empty()
            && !path.contains(|character: char| {
                character == '`' || character == '*' || character == '|' || is_space(character)
            }))
        .then_some(path)
    }

    /// Whether a backticked source entry of the chapter's source map section matches the module
    /// path, `*` standing for any characters but line terminators. The section starts at the
    /// first line that is the heading and white space, and ends before the next line that starts
    /// with the heading's level marker and a space; lines end at every JavaScript line
    /// terminator.
    fn source_map_contains(&self, chapter: &str, module: &str) -> bool {
        let heading = self.source_map_heading.as_str();
        let level = heading.len() - heading.trim_start_matches('#').len();
        let next = format!("{} ", "#".repeat(level));
        let mut starts = vec![0];
        starts.extend(
            chapter
                .char_indices()
                .filter(|(_, character)| LINE_TERMINATORS.contains(character))
                .map(|(offset, character)| offset + character.len_utf8()),
        );
        let line_at = |start: usize| {
            let rest = &chapter[start..];
            &rest[..rest.find(LINE_TERMINATORS).unwrap_or(rest.len())]
        };

        let Some(found) = starts.iter().position(|start| {
            line_at(*start)
                .strip_prefix(heading)
                .is_some_and(|rest| rest.chars().all(is_space))
        }) else {
            return false;
        };
        let section_start = starts[found] + line_at(starts[found]).len();
        let section_end = starts[found + 1..]
            .iter()
            .copied()
            .find(|start| chapter[*start..].starts_with(next.as_str()))
            .unwrap_or(chapter.len());

        self.entries(&chapter[section_start..section_end])
            .into_iter()
            .any(|entry| wildcard_match(entry, module))
    }

    /// The source entries of a section: each `` `<source root>/…` `` code span, read left to right
    /// without overlap, as the pattern `` `(src/[^`]+)` `` finds them.
    fn entries<'t>(&self, text: &'t str) -> Vec<&'t str> {
        let mut entries = Vec::new();
        let mut offset = 0;

        while let Some(found) = text[offset..].find('`') {
            let start = offset + found + 1;
            let rest = &text[start..];

            if let Some(close) = rest.find('`')
                && close > self.source_prefix.len()
                && rest.starts_with(self.source_prefix.as_str())
            {
                entries.push(&rest[..close]);
                offset = start + close + 1;
            } else {
                offset = start;
            }
        }

        entries
    }
}

/// Whether `text` matches `pattern` whole, each `*` standing for any run of characters that are
/// not line terminators.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };

    for part in middle {
        let Some(found) = rest.find(part) else {
            return false;
        };

        if rest[..found].contains(LINE_TERMINATORS) {
            return false;
        }

        rest = &rest[found + part.len()..];
    }

    rest.len() >= last.len()
        && rest.ends_with(last)
        && !rest[..rest.len() - last.len()].contains(LINE_TERMINATORS)
}

/// Whether a line opens with up to three spaces and a `|`.
fn is_table_line(line: &str) -> bool {
    let rest = line.trim_start_matches(' ');
    line.len() - rest.len() <= 3 && rest.starts_with('|')
}

/// Whether the line after the header is its delimiter row: up to three spaces, `|`, and three
/// cells of at least three dashes with optional alignment colons and spaces or tabs around, each
/// closed by `|`, then only spaces or tabs.
fn is_delimiter(line: &str) -> bool {
    let blanks = |text: &str| text.trim_start_matches([' ', '\t']).len();
    let rest = line.trim_start_matches(' ');

    if line.len() - rest.len() > 3 {
        return false;
    }

    let Some(mut rest) = rest.strip_prefix('|') else {
        return false;
    };

    for _ in 0..3 {
        rest = &rest[rest.len() - blanks(rest)..];
        rest = rest.strip_prefix(':').unwrap_or(rest);
        let dashes = rest.len() - rest.trim_start_matches('-').len();

        if dashes < 3 {
            return false;
        }

        rest = &rest[dashes..];
        rest = rest.strip_prefix(':').unwrap_or(rest);
        rest = &rest[rest.len() - blanks(rest)..];

        let Some(next) = rest.strip_prefix('|') else {
            return false;
        };

        rest = next;
    }

    rest.trim_start_matches([' ', '\t']).is_empty()
}

/// The target of a chapter cell `[label](target#fragment)`, without its fragment: a non-empty
/// label without `]`, and a non-empty target without `)` or `#`.
fn chapter_link(cell: &str) -> Option<&str> {
    let label_end = cell.strip_prefix('[')?.find(']')? + 1;

    if label_end < 2 {
        return None;
    }

    let destination = cell[label_end..].strip_prefix("](")?.strip_suffix(')')?;

    if destination.contains(')') {
        return None;
    }

    let target = destination.split('#').next().unwrap_or_default();
    (!target.is_empty()).then_some(target)
}

/// `path.posix.normalize` of a relative path: `.` and empty segments dropped, `..` folded, a
/// leading `..` kept, `.` for nothing, and a trailing slash kept.
fn normalize(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|last| *last != "..") => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }

    let mut normalized = segments.join("/");

    if normalized.is_empty() {
        normalized.push('.');
    }

    if path.ends_with('/') {
        normalized.push('/');
    }

    normalized
}

/// The text with fenced code blocks and HTML comments blanked by spaces of the same byte length,
/// so line numbers stay those of the original text.
fn blank_fenced(content: &str) -> String {
    let mut fence: Option<(u8, usize)> = None;
    let mut in_comment = false;
    let mut lines = Vec::new();

    for original in content.split('\n') {
        if let Some((character, length)) = fence {
            if let Some(closing) = closing_marker(original)
                && closing.as_bytes().first() == Some(&character)
                && closing.len() >= length
            {
                fence = None;
            }

            lines.push(" ".repeat(original.len()));
            continue;
        }

        let mut line = original.to_owned();
        let mut offset = 0;

        while offset < line.len() {
            let start = if in_comment {
                Some(offset)
            } else {
                line[offset..].find("<!--").map(|found| offset + found)
            };
            let Some(start) = start else {
                break;
            };
            let search = if in_comment { start } else { start + 4 };
            let end = line
                .get(search..)
                .and_then(|rest| rest.find("-->"))
                .map(|found| search + found);
            let next = end.map_or(line.len(), |end| end + 3);
            line.replace_range(start..next, &" ".repeat(next - start));
            in_comment = end.is_none();
            offset = next;
        }

        if let Some((character, length)) = opening_marker(&line) {
            fence = Some((character, length));
            lines.push(" ".repeat(line.len()));
            continue;
        }

        lines.push(line);
    }

    lines.join("\n")
}

/// The marker of a closing fence line: up to three spaces, a run of backticks or tildes, and
/// only spaces or tabs after it.
fn closing_marker(line: &str) -> Option<&str> {
    let rest = strip_indent(line)?;
    let character = *rest.as_bytes().first()?;

    if character != b'`' && character != b'~' {
        return None;
    }

    let length = rest.bytes().take_while(|byte| *byte == character).count();
    rest[length..]
        .bytes()
        .all(|byte| byte == b' ' || byte == b'\t')
        .then(|| &rest[..length])
}

/// The marker character and length of an opening fence line: up to three spaces and a run of at
/// least three backticks or tildes.
fn opening_marker(line: &str) -> Option<(u8, usize)> {
    let rest = strip_indent(line)?;
    let character = *rest.as_bytes().first()?;

    if character != b'`' && character != b'~' {
        return None;
    }

    let length = rest.bytes().take_while(|byte| *byte == character).count();
    (length >= 3).then_some((character, length))
}

/// The line without up to three leading spaces; `None` for a deeper indent.
fn strip_indent(line: &str) -> Option<&str> {
    let spaces = line.bytes().take_while(|byte| *byte == b' ').count();
    (spaces <= 3).then(|| &line[spaces..])
}
