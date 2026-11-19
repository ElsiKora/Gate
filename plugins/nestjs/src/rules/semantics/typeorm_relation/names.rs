//! Relation names as the JavaScript check read them: camel-case words, the role qualifier shared
//! by several relations to one target, the target's resource name, and foreign key names.

use super::{Case, Spelling, Vocabulary};

/// Whether `role` is a mutable selection or state role.
pub(super) fn mutable_role(role: &str, vocabulary: &Vocabulary) -> bool {
    vocabulary.mutable_roles.iter().any(|known| known == role)
}

/// Whether `name` matches `^(?:FK|fk)_[0-9a-f]{27}$`, the name `TypeORM` derives from a hash.
fn hashed(name: &str) -> bool {
    name.strip_prefix("FK_")
        .or_else(|| name.strip_prefix("fk_"))
        .is_some_and(|hash| {
            hash.len() == 27
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
}

/// Whether `name` matches `^<prefix>[a-z][a-z0-9]*(?:_[a-z0-9]+)+$`: the prefix, then two or more
/// words of lower-case ASCII letters and digits joined by `_`, the first starting with a letter.
fn semantic(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    let words: Vec<&str> = rest.split('_').collect();
    let word = |word: &str| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    };

    words.len() >= 2
        && words[0]
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && words.iter().all(|part| word(part))
}

/// The class name without `Entity`, its first letter lowered; `None` for a
/// name that does not end with `Entity` or is nothing else.
pub(super) fn resource(class: &str) -> Option<String> {
    let stem = class
        .strip_suffix("Entity")
        .filter(|stem| !stem.is_empty())?;
    let mut characters = stem.chars();
    let first = characters.next()?;

    Some(format!("{}{}", first.to_lowercase(), characters.as_str()))
}

/// `value` starts with `prefix` and continues, if at all, with an ASCII
/// capital or digit.
pub(super) fn camel_prefix(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|next| next.is_ascii_uppercase() || next.is_ascii_digit())
    })
}

/// The camel case words of a name: the matches of `/[A-Z]?[a-z]+|[A-Z]+(?=[A-Z]|$)|\d+/gu`; the whole
/// value when nothing matches.
pub(super) fn words(value: &str) -> Vec<String> {
    let characters: Vec<char> = value.chars().collect();
    let mut found = Vec::new();
    let mut start = 0;

    while start < characters.len() {
        match word_at(&characters, start) {
            Some(end) => {
                found.push(characters[start..end].iter().collect());
                start = end;
            }
            None => start += 1,
        }
    }

    if found.is_empty() {
        vec![value.to_owned()]
    } else {
        found
    }
}

/// The end of the first alternative that matches at `start`, as the regular expression tries them.
fn word_at(characters: &[char], start: usize) -> Option<usize> {
    let lower = |index: usize| characters.get(index).is_some_and(char::is_ascii_lowercase);
    let upper = |index: usize| characters.get(index).is_some_and(char::is_ascii_uppercase);
    let digit = |index: usize| characters.get(index).is_some_and(char::is_ascii_digit);
    let run = |from: usize, accepts: &dyn Fn(usize) -> bool| {
        let mut end = from;

        while accepts(end) {
            end += 1;
        }

        end
    };

    // `[A-Z]?[a-z]+`: the optional capital is tried first.
    if upper(start) && lower(start + 1) {
        return Some(run(start + 1, &lower));
    }

    if lower(start) {
        return Some(run(start, &lower));
    }

    // `[A-Z]+(?=[A-Z]|$)`: the longest run of capitals followed by a capital or the end.
    if upper(start) {
        let longest = run(start, &upper);

        for end in (start + 1..=longest).rev() {
            if end == characters.len() || upper(end) {
                return Some(end);
            }
        }
    }

    // `\d+`.
    if digit(start) {
        return Some(run(start, &digit));
    }

    None
}

/// The words of `member` before the suffix all relations to its
/// target share, ignoring case; `None` when no suffix or the whole name is shared.
pub(super) fn qualifier(member: &str, relations: &[String]) -> Option<String> {
    let member_words = words(member);
    let relation_words: Vec<Vec<String>> = relations.iter().map(|name| words(name)).collect();
    let mut shared = 0;

    while shared < member_words.len() {
        let word = member_words[member_words.len() - shared - 1].to_lowercase();

        if !relation_words.iter().all(|words| {
            words.len() > shared && words[words.len() - shared - 1].to_lowercase() == word
        }) {
            break;
        }

        shared += 1;
    }

    (shared != 0 && shared != member_words.len())
        .then(|| member_words[..member_words.len() - shared].concat())
}

/// Not a `TypeORM` hash, the spelling's semantic pattern, and
/// shorter than the byte limit.
pub(super) fn foreign_key(
    name: &str,
    spelling: Spelling<'_>,
    vocabulary: &Vocabulary,
) -> Vec<String> {
    let mut errors = Vec::new();
    let quoted = super::quote(name);
    let (prefix, source) = match spelling.case {
        Case::Upper => ("FK_", "/^FK_[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/u"),
        Case::Lower => ("fk_", "/^fk_[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/u"),
    };

    if hashed(name) {
        errors.push(format!(
            "foreign key name {quoted} is a TypeORM hash, not a semantic name"
        ));
    } else if !semantic(name, prefix) {
        errors.push(format!(
            "foreign key name {quoted} must match {source} in {} mode",
            spelling.label
        ));
    }

    if name.len() >= vocabulary.max_foreign_key_bytes {
        errors.push(format!(
            "foreign key name {quoted} must be shorter than {} UTF-8 bytes",
            vocabulary.max_foreign_key_bytes
        ));
    }

    errors
}
