//! JavaScript string operations of the reconstruction: budgets, concatenation, `replace` and the
//! advisory-evidence patterns.

use super::super::sql::{Text, is_space, units};
use super::{Fail, Outcome};

pub(super) const CANDIDATE_LIMIT: usize = 64;
pub(super) const CODE_UNIT_LIMIT: usize = 65_536;
pub(super) const DEPTH_LIMIT: usize = 64;
pub(super) const UTF8_LIMIT: usize = 65_536;
pub(super) const WORK_LIMIT: u32 = 32_768;
pub(super) const INTERPOLATION: &str = "${dynamic}";

/// The UTF-8 length of a JavaScript string, an unpaired surrogate as three bytes.
pub(super) fn utf8_length(text: &[u16]) -> usize {
    char::decode_utf16(text.iter().copied())
        .map(|character| character.map_or(3, char::len_utf8))
        .sum()
}

pub(super) fn exceeds(text: &[u16]) -> bool {
    text.len() > CODE_UNIT_LIMIT || utf8_length(text) > UTF8_LIMIT
}

pub(super) fn validate(text: Text) -> Outcome<Text> {
    if exceeds(&text) {
        return Err(Fail::new("Static expression text budget was exceeded."));
    }

    Ok(text)
}

pub(super) fn concatenate(parts: &[&[u16]]) -> Outcome<Text> {
    let length: usize = parts.iter().map(|part| part.len()).sum();
    let bytes: usize = parts.iter().map(|part| utf8_length(part)).sum();

    if length > CODE_UNIT_LIMIT || bytes > UTF8_LIMIT {
        return Err(Fail::new("Static expression text budget was exceeded."));
    }

    Ok(parts.concat())
}

pub(super) fn consume(budget: &mut u32, depth: usize) -> Outcome<()> {
    *budget += 1;

    if depth > DEPTH_LIMIT || *budget > WORK_LIMIT {
        return Err(Fail::resource());
    }

    Ok(())
}

impl Candidates {
    pub(super) fn add(&mut self, candidate: Text, budget: &mut u32, depth: usize) -> Outcome<()> {
        consume(budget, depth)?;
        let candidate = validate(candidate)?;

        if !self.0.contains(&candidate) {
            self.0.push(candidate);
        }

        if self.0.len() > CANDIDATE_LIMIT {
            return Err(Fail::new(
                "Static expression candidate budget was exceeded.",
            ));
        }

        Ok(())
    }
}

/// A candidate set that keeps its insertion order.
#[derive(Default)]
pub(super) struct Candidates(pub(super) Vec<Text>);

pub(super) fn concatenate_candidates(
    left: &[Text],
    right: &[Text],
    budget: &mut u32,
    depth: usize,
) -> Outcome<Vec<Text>> {
    let mut candidates = Candidates::default();

    for first in left {
        for second in right {
            candidates.add(concatenate(&[first, second])?, budget, depth)?;
        }
    }

    Ok(candidates.0)
}

pub(super) fn validate_replacement(
    receiver: &[u16],
    replacement: &[u16],
    all: bool,
) -> Outcome<()> {
    let count = if all { receiver.len() + 1 } else { 1 };
    let length = receiver.len() + count * replacement.len();
    let bytes = utf8_length(receiver) + count * utf8_length(replacement);

    if length > CODE_UNIT_LIMIT || bytes > UTF8_LIMIT {
        return Err(Fail::new(
            "Static expression replacement budget was exceeded.",
        ));
    }

    Ok(())
}

pub(super) fn index_of(text: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if from > text.len() {
        return None;
    }

    let last = text.len().checked_sub(needle.len())?;

    (from..=last).find(|&start| text[start..start + needle.len()] == *needle)
}

/// The replacement text of one match, with JavaScript's `$$`, `$&`, `` $` `` and `$'` patterns of
/// a string search.
pub(super) fn substitution(
    receiver: &[u16],
    matched: &[u16],
    position: usize,
    replacement: &[u16],
) -> Text {
    let mut result = Text::new();
    let mut index = 0;

    while index < replacement.len() {
        let unit = replacement[index];
        let next = replacement.get(index + 1).copied();

        if unit == u16::from(b'$') {
            match next.and_then(|next| u8::try_from(next).ok()) {
                Some(b'$') => {
                    result.push(unit);
                    index += 2;
                    continue;
                }
                Some(b'&') => {
                    result.extend_from_slice(matched);
                    index += 2;
                    continue;
                }
                Some(b'`') => {
                    result.extend_from_slice(&receiver[..position]);
                    index += 2;
                    continue;
                }
                Some(b'\'') => {
                    let tail = (position + matched.len()).min(receiver.len());
                    result.extend_from_slice(&receiver[tail..]);
                    index += 2;
                    continue;
                }
                _ => {}
            }
        }

        result.push(unit);
        index += 1;
    }

    result
}

/// `receiver.replace(search, replacement)` or `replaceAll` with a string search.
pub(super) fn replace(receiver: &[u16], search: &[u16], replacement: &[u16], all: bool) -> Text {
    let mut positions = Vec::new();
    let mut position = index_of(receiver, search, 0);

    while let Some(found) = position {
        positions.push(found);

        if !all {
            break;
        }

        position = index_of(receiver, search, found + search.len().max(1));
    }

    let mut result = Text::new();
    let mut end = 0;

    for found in positions {
        result.extend_from_slice(&receiver[end..found]);
        result.extend(substitution(receiver, search, found, replacement));
        end = found + search.len();
    }

    result.extend_from_slice(&receiver[end.min(receiver.len())..]);
    result
}

/// Folds the case of a code unit as a JavaScript `/iu` pattern of ASCII letters compares it.
pub(super) fn fold(unit: u16) -> u16 {
    match unit {
        0x41..=0x5a => unit | 0x20,
        0x017f => u16::from(b's'),
        0x212a => u16::from(b'k'),
        _ => unit,
    }
}

pub(super) fn contains_folded<U: Copy + Into<u16>>(text: &[U], needle: &str) -> bool {
    let needle = needle.as_bytes();

    text.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(&unit, &expected)| fold(unit.into()) == u16::from(expected))
    })
}

/// Whether a text holds `bit\s*\(\s*64\s*\)`, ignoring case.
pub(super) fn contains_bit_64<U: Copy + Into<u16>>(text: &[U]) -> bool {
    let skip = |mut index: usize| {
        while text.get(index).is_some_and(|&unit| is_space(unit.into())) {
            index += 1;
        }
        index
    };
    let at = |index: usize, unit: u8| {
        text.get(index)
            .is_some_and(|&found| fold(found.into()) == u16::from(unit))
    };

    (0..text.len()).any(|start| {
        if !(at(start, b'b') && at(start + 1, b'i') && at(start + 2, b't')) {
            return false;
        }

        let open = skip(start + 3);

        if !at(open, b'(') {
            return false;
        }

        let digits = skip(open + 1);

        if !(at(digits, b'6') && at(digits + 1, b'4')) {
            return false;
        }

        at(skip(digits + 2), b')')
    })
}

/// `/pg_|advisory_|hashtext|bit\s*\(\s*64\s*\)/iu`, an advisory fragment.
pub(super) fn is_fragment<U: Copy + Into<u16>>(text: &[U]) -> bool {
    contains_folded(text, "pg_") || is_declaration_fragment(text)
}

/// The advisory fragment without a bare `pg_`, which any `pg_catalog` SQL holds: the evidence of a
/// function or class declaration a sink argument names.
pub(super) fn is_declaration_fragment<U: Copy + Into<u16>>(text: &[U]) -> bool {
    contains_folded(text, "advisory_") || contains_folded(text, "hashtext") || contains_bit_64(text)
}

/// `/pg_(?:try_)?advisory_|hashtext|bit\s*\(\s*64\s*\)/iu`, advisory evidence.
pub(super) fn is_advisory<U: Copy + Into<u16>>(text: &[U]) -> bool {
    contains_folded(text, "pg_advisory_")
        || contains_folded(text, "pg_try_advisory_")
        || contains_folded(text, "hashtext")
        || contains_bit_64(text)
}

/// Whether `wide` holds for a source text read as its UTF-16 code units, as JavaScript reads it;
/// the bytes of an ASCII text are its code units, so `narrow` reads them without encoding.
pub(super) fn in_source(source: &str, wide: fn(&[u16]) -> bool, narrow: fn(&[u8]) -> bool) -> bool {
    if source.is_ascii() {
        narrow(source.as_bytes())
    } else {
        wide(&units(source))
    }
}

pub(super) fn joined(parts: &[Text], delimiter: &[u16]) -> Outcome<Text> {
    let mut all: Vec<&[u16]> = Vec::new();

    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            all.push(delimiter);
        }
        all.push(part);
    }

    concatenate(&all)
}
