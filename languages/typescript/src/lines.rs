//! One-based line numbers of byte offsets, counted as the TypeScript compiler counts them.

/// The start offset of every line of one source text.
#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    /// Indexes `text`. A line ends at a line feed, a carriage return (alone or before a line
    /// feed), U+2028 or U+2029, as `ts.computeLineStarts` ends it. An offset past `u32::MAX`,
    /// which oxc cannot parse either, counts as `u32::MAX`.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let offset = |index: usize| u32::try_from(index).unwrap_or(u32::MAX);
        let bytes = text.as_bytes();
        let mut starts = vec![0];
        let mut index = 0;

        while index < bytes.len() {
            match bytes[index] {
                b'\n' => {
                    starts.push(offset(index + 1));
                    index += 1;
                }
                b'\r' => {
                    let next = if bytes.get(index + 1) == Some(&b'\n') {
                        index + 2
                    } else {
                        index + 1
                    };
                    starts.push(offset(next));
                    index = next;
                }
                // U+2028 and U+2029 are E2 80 A8 and E2 80 A9 in UTF-8.
                0xE2 if bytes.get(index + 1) == Some(&0x80)
                    && matches!(bytes.get(index + 2), Some(0xA8 | 0xA9)) =>
                {
                    starts.push(offset(index + 3));
                    index += 3;
                }
                _ => index += 1,
            }
        }

        Self { starts }
    }

    /// The byte range of one-based `line`, from its start to the start of the next line (or the end
    /// of the text, `length`); `None` past the last line.
    #[must_use]
    pub fn bounds(&self, line: u32, length: u32) -> Option<(u32, u32)> {
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        let start = *self.starts.get(index)?;
        let end = self.starts.get(index + 1).copied().unwrap_or(length);
        Some((start, end))
    }

    /// The one-based line that contains byte `offset`; an offset past the end is on the last line.
    #[must_use]
    pub fn line(&self, offset: u32) -> u32 {
        let line = self.starts.partition_point(|start| *start <= offset);
        u32::try_from(line).unwrap_or(u32::MAX)
    }

    /// The one-based column of a byte offset of `text` (the text this index was built from),
    /// counted in characters from the start of its line.
    #[must_use]
    pub fn column(&self, text: &str, offset: u32) -> u32 {
        let line = self.starts.partition_point(|start| *start <= offset);
        let start = line
            .checked_sub(1)
            .and_then(|index| self.starts.get(index))
            .map_or(0, |start| *start as usize);
        let end = (offset as usize).min(text.len());
        let characters = text
            .get(start..end)
            .map_or(0, |prefix| prefix.chars().count());
        u32::try_from(characters + 1).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::LineIndex;

    #[test]
    fn every_typescript_line_terminator_starts_a_line() {
        let index = LineIndex::new("a\nb\r\nc\rd\u{2028}e\u{2029}f");
        let lines: Vec<u32> = [0, 2, 5, 7, 11, 15]
            .into_iter()
            .map(|offset| index.line(offset))
            .collect();

        assert_eq!(lines, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn a_line_spans_from_its_start_to_the_next_line_start() {
        let index = LineIndex::new("ab\r\ncd");

        assert_eq!(index.bounds(1, 6), Some((0, 4)));
        assert_eq!(index.bounds(2, 6), Some((4, 6)));
        assert_eq!(index.bounds(3, 6), None);
        assert_eq!(index.bounds(0, 6), None);
    }

    #[test]
    fn a_terminator_belongs_to_the_line_it_ends() {
        let index = LineIndex::new("ab\r\ncd");

        assert_eq!([1, 2, 3, 4].map(|offset| index.line(offset)), [1, 1, 1, 2]);
    }

    #[test]
    fn an_offset_past_the_end_is_on_the_last_line() {
        let index = LineIndex::new("a\n");

        assert_eq!(index.line(2), 2);
        assert_eq!(index.line(99), 2);
        assert_eq!(LineIndex::new("").line(0), 1);
    }
}
