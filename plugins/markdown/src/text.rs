//! Text helpers shared by the rules: line numbers, front matter and fenced blocks blanked in
//! place, and the white space of JavaScript's `\s`.

/// Maps byte offsets to one-based line numbers.
#[derive(Debug)]
pub(crate) struct Lines {
    starts: Vec<usize>,
}

impl Lines {
    pub(crate) fn new(text: &str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(offset, _)| offset + 1))
            .collect();
        Self { starts }
    }

    /// The line of the byte at `offset`.
    pub(crate) fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset)
    }
}

/// Replaces every byte of `range` but newlines with a space, so offsets and lines stay put.
fn blank(bytes: &mut [u8]) {
    for byte in bytes {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

/// `text` with its front matter blanked. Front matter, as prettier reads it, opens with `---` or
/// `+++` at the start of the document (after a byte order mark and blank lines), runs to the first
/// later line that starts with the same delimiter (for YAML, also `...`), and ends after that
/// delimiter's three characters.
pub(crate) fn without_front_matter(text: &str) -> String {
    let bom = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let start = bom
        + text[bom..]
            .split_inclusive('\n')
            .take_while(|line| line.ends_with('\n') && line.trim().is_empty())
            .map(str::len)
            .sum::<usize>();
    let rest = &text[start..];
    let Some(delimiter) = ["---", "+++"]
        .into_iter()
        .find(|mark| rest.starts_with(mark))
    else {
        return text.to_owned();
    };
    let Some(first_line_end) = rest.find('\n') else {
        return text.to_owned();
    };
    let info = rest[delimiter.len()..first_line_end].trim();
    let after = &rest[first_line_end..];
    let yaml = delimiter == "---" && (info.is_empty() || info == "yaml");
    let Some(closing) = after
        .find(&format!("\n{delimiter}"))
        .or_else(|| if yaml { after.find("\n...") } else { None })
    else {
        return text.to_owned();
    };
    let end = start + first_line_end + closing + 1 + delimiter.len();
    let mut bytes = text.as_bytes().to_vec();
    blank(&mut bytes[start..end]);
    String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned())
}

/// The marker byte and length of a fence line: up to three spaces, then a run of backticks or
/// tildes; `None` for any other line.
fn fence_run(line: &str) -> Option<(u8, usize, &str)> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let marker = *rest.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = rest.bytes().take_while(|byte| *byte == marker).count();
    Some((marker, length, &rest[length..]))
}

/// `text` with fenced code blocks and HTML comments replaced by spaces of the same byte length,
/// line by line, so line numbers stay those of the original. A fence opens with three or more
/// backticks or tildes and closes at a line of at least as many of the same character followed
/// only by spaces or tabs; a comment runs from `<!--` to the next `-->`, across lines.
pub(crate) fn blank_fences_and_comments(text: &str) -> String {
    let mut fence: Option<(u8, usize)> = None;
    let mut in_comment = false;
    let mut lines = Vec::new();

    for original in text.split('\n') {
        if let Some((marker, length)) = fence {
            if let Some((closing, run, after)) = fence_run(original)
                && closing == marker
                && run >= length
                && after.bytes().all(|byte| byte == b' ' || byte == b'\t')
            {
                fence = None;
            }
            lines.push(" ".repeat(original.len()));
            continue;
        }

        let mut bytes = original.as_bytes().to_vec();
        let mut offset = 0;
        while offset < bytes.len() {
            let start = if in_comment {
                offset
            } else if let Some(found) = find(&bytes[offset..], b"<!--") {
                offset + found
            } else {
                break;
            };
            let search = if in_comment { start } else { start + 4 };
            let end = bytes
                .get(search..)
                .and_then(|rest| find(rest, b"-->"))
                .map(|found| search + found + 3);
            let stop = end.unwrap_or(bytes.len());
            blank(&mut bytes[start..stop]);
            in_comment = end.is_none();
            offset = stop;
        }
        let line = String::from_utf8(bytes).unwrap_or_else(|_| original.to_owned());

        if let Some((marker, length, _)) = fence_run(&line)
            && length >= 3
        {
            fence = Some((marker, length));
            lines.push(" ".repeat(line.len()));
            continue;
        }
        lines.push(line);
    }

    lines.join("\n")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Whether `character` is white space for JavaScript's `\s`.
pub(crate) fn is_js_space(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Whether `line` holds a JavaScript line terminator other than `\n`.
pub(crate) fn has_line_terminator(line: &str) -> bool {
    line.contains(['\r', '\u{2028}', '\u{2029}'])
}

#[cfg(test)]
mod tests {
    use super::{Lines, blank_fences_and_comments, without_front_matter};

    #[test]
    fn blanking_keeps_offsets_and_closes_only_a_matching_fence() {
        let text = "a\n````md\n```\nx\n````\nb <!-- c\nd --> e\n";
        let blanked = blank_fences_and_comments(text);
        assert_eq!(blanked.len(), text.len());
        assert_eq!(blanked, "a\n      \n   \n \n    \nb       \n      e\n");
    }

    #[test]
    fn front_matter_is_blanked_in_place() {
        let text = "\n---\ntitle: [x](a.md)\n---\n[y](b.md)\n";
        let blanked = without_front_matter(text);
        assert_eq!(blanked.len(), text.len());
        assert_eq!(blanked, "\n   \n                \n   \n[y](b.md)\n");
        assert_eq!(without_front_matter("--- \nopen\n"), "--- \nopen\n");
        assert_eq!(
            without_front_matter("---\na: 1\n...\nbody\n"),
            "   \n    \n   \nbody\n"
        );
        assert_eq!(
            without_front_matter("+++\na = 1\n...\n"),
            "+++\na = 1\n...\n"
        );
    }

    #[test]
    fn lines_are_one_based() {
        let lines = Lines::new("a\nb\n");
        assert_eq!([lines.line(0), lines.line(2), lines.line(4)], [1, 2, 3]);
    }
}
