//! `git/commit-message`: the message of the commit being made (`gate check --commit-message
//! <file>`, which the generated `commit-msg` hook passes) has the conventional form, as
//! `commitlint` checks it with `@commitlint/config-conventional` and the rule levels of
//! `@elsikora/git-branch-lint`: `<type>(<scope>): <subject>`, then a blank line and a body, then a
//! blank line and a footer.
//!
//! Each check is an aspect named after the `commitlint` rule it ports. The message is read as
//! `commitlint --edit` reads it: comment lines (`#`) and what follows the scissors line are not
//! part of it, merge, revert, fixup, squash and release messages are not checked, and the parts
//! are split as `conventional-commits-parser` splits them.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Result, Rule, Run, Scope};
use regex::Regex;

use crate::settings::pattern;

/// The rule.
#[derive(Debug, Default)]
pub struct CommitMessage;

/// The options of `git/commit-message`; every one defaults to the reference's value. A length is
/// counted in characters; 0 turns a longest-length check off.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The types a header may name (`type-enum`).
    pub types: Vec<String>,
    /// The longest header (`header-max-length`).
    pub header_max_length: usize,
    /// The shortest header (`header-min-length`).
    pub header_min_length: usize,
    /// The longest scope (`scope-max-length`).
    pub scope_max_length: usize,
    /// The longest subject (`subject-max-length`).
    pub subject_max_length: usize,
    /// The shortest subject (`subject-min-length`).
    pub subject_min_length: usize,
    /// The longest body line; a line with a URL is never too long (`body-max-line-length`).
    pub body_max_line_length: usize,
    /// The longest footer line; a line with a URL is never too long (`footer-max-line-length`).
    pub footer_max_line_length: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            types: [
                "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
                "revert", "wip",
            ]
            .map(str::to_owned)
            .to_vec(),
            header_max_length: 100,
            header_min_length: 10,
            scope_max_length: 30,
            subject_max_length: 80,
            subject_min_length: 3,
            body_max_line_length: 100,
            footer_max_line_length: 100,
        }
    }
}

/// The aspects, one per check, in the order findings are reported.
const ASPECTS: &[&str] = &[
    "header-trim",
    "header-max-length",
    "header-min-length",
    "header-case",
    "header-full-stop",
    "type-empty",
    "type-case",
    "type-enum",
    "scope-empty",
    "scope-case",
    "scope-max-length",
    "subject-empty",
    "subject-case",
    "subject-full-stop",
    "subject-min-length",
    "subject-max-length",
    "body-leading-blank",
    "body-max-line-length",
    "body-full-stop",
    "footer-leading-blank",
    "footer-max-line-length",
];

impl Rule for CommitMessage {
    type Options = Options;

    const NAME: &'static str = "commit-message";
    const TITLE: &'static str =
        "The message of the commit being made has the conventional form: type(scope): subject";
    const ASPECTS: &'static [&'static str] = ASPECTS;
    const SCOPE: Scope = Scope::Repository;
    const READS_HISTORY: bool = true;

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let patterns = Patterns::new()?;
        let Some(message) = run.git()?.message() else {
            return Ok(());
        };
        let Some((header, found)) = verdicts(&message, run.options(), &patterns) else {
            return Ok(());
        };
        for (aspect, text, help) in found {
            let finding = run.report().finding(aspect, None::<Location>, text);
            if let Some(header) = &header {
                finding.target(header.clone());
            }
            finding.help(help);
        }
        Ok(())
    }
}

/// The header of `message` and the checks it fails, as aspect, message and help, in [`ASPECTS`]
/// order; `None` for a message that is blank, has no line but comments, or is of a form the
/// reference does not check.
fn verdicts(
    message: &str,
    options: &Options,
    patterns: &Patterns,
) -> Option<(Option<String>, Vec<Failure>)> {
    if trim(message).is_empty() {
        return None;
    }
    // `commitlint --edit` appends a line feed to the file it reads.
    let raw = format!("{message}\n");
    if ignored(&raw, patterns) {
        return None;
    }
    let parsed = Parsed::read(&raw, patterns);
    if parsed.header.is_none() && parsed.body.is_none() && parsed.footer.is_none() {
        return None;
    }
    let found = checks(&parsed, options, patterns);
    Some((parsed.header, found))
}

/// A failed check: its aspect, the message and the help.
type Failure = (&'static str, String, String);

/// The expressions the checks use.
struct Patterns {
    /// `conventional-changelog-conventionalcommits`' header with `!`, tried first.
    breaking: Regex,
    /// Its header.
    header: Regex,
    /// A quoted span, which a case check skips.
    quoted: Regex,
    /// A first character that has a case.
    cased: Regex,
    /// A CI-skip marker a release header may carry.
    skip_brackets: Regex,
    skip_parentheses: Regex,
}

impl Patterns {
    fn new() -> Result<Self> {
        // `.` of JavaScript, which matches no line terminator.
        let any = r"[^\n\r\u{2028}\u{2029}]";
        Ok(Self {
            breaking: pattern(
                "the header pattern",
                &format!(r"^([0-9A-Za-z_]*)(?:\(({any}*)\))?!: ({any}*)$"),
            )?,
            header: pattern(
                "the header pattern",
                &format!(r"^([0-9A-Za-z_]*)(?:\(({any}*)\))?!?: ({any}*)$"),
            )?,
            quoted: pattern(
                "the quoted-span pattern",
                &format!("`{any}*?`|\"{any}*?\"|'{any}*?'"),
            )?,
            cased: pattern("the cased-letter pattern", r"(?i)^[\p{Ll}\p{Lu}\p{Lt}]")?,
            skip_brackets: pattern(
                "the skip pattern",
                r"\[(?:[sS][kK][iI][pP]|[cC][iI])(?:-|[\s&&[^\u{85}]]|\u{FEFF})(?:[cC][iI]|[sS][kK][iI][pP])\]",
            )?,
            skip_parentheses: pattern(
                "the skip pattern",
                r"\((?:[sS][kK][iI][pP]|[cC][iI])(?:-|[\s&&[^\u{85}]]|\u{FEFF})(?:[cC][iI]|[sS][kK][iI][pP])\)",
            )?,
        })
    }
}

/// Whitespace as JavaScript's `\s` and `trim` know it.
fn is_space(character: char) -> bool {
    (character.is_whitespace() && character != '\u{85}') || character == '\u{feff}'
}

/// A word character of JavaScript's `\w` and `\b`.
fn is_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// The lines of `text` as `split(/\r?\n/)` gives them.
fn lines(text: &str) -> Vec<&str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// `text` without the line feeds and carriage returns at either end.
fn trim_new_lines(text: &str) -> &str {
    text.trim_matches(['\r', '\n'])
}

fn length(text: &str) -> usize {
    text.chars().count()
}

/// The message is of a form `commitlint`'s default ignores skip: a merge (`Merge branch`,
/// `Merge pull request`, `Merge <a> into <b>`, `Merge tag`, `Merge remote-tracking branch`,
/// `Merged ... in`, `Merged PR ...: `, `Automatic merge`, `Auto-merged ... into`), a revert or
/// reapply, an `amend!`, `fixup!` or `squash!` commit, or a release whose header is a version.
fn ignored(raw: &str, patterns: &Patterns) -> bool {
    // The `m` patterns of the reference match at the start of any line.
    let anywhere = raw.split(['\n', '\r', '\u{2028}', '\u{2029}']).any(|line| {
        line.starts_with("Merge pull request")
            || line.starts_with("Merge branch ")
            || line.starts_with("Merge tag ")
            || line
                .strip_prefix("Merge ")
                .is_some_and(|rest| rest.contains(" into "))
    });
    let first = raw
        .split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .next()
        .unwrap_or_default();
    let at_start = ["Revert ", "revert ", "Reapply ", "reapply "]
        .iter()
        .any(|prefix| raw.starts_with(prefix))
        || ["amend!", "fixup!", "squash!"]
            .iter()
            .any(|prefix| raw.starts_with(prefix))
        || raw.starts_with("Merge remote-tracking branch")
        || raw.starts_with("Automatic merge")
        || first
            .strip_prefix("Merged ")
            .is_some_and(|rest| rest.contains("in ") || rest.contains("into "))
        || first
            .strip_prefix("Merged PR ")
            .is_some_and(|rest| rest.contains(": "))
        || first
            .strip_prefix("Auto-merged ")
            .is_some_and(|rest| rest.contains(" into "));
    anywhere || at_start || is_release(raw, patterns)
}

/// The first line, without a `chore(...):` prefix and a CI-skip marker, is a version.
fn is_release(raw: &str, patterns: &Patterns) -> bool {
    let first = raw.split('\n').next().unwrap_or_default();
    let mut rest = first;
    if let Some(after) = first.strip_prefix("chore") {
        let scoped = after.strip_prefix('(').and_then(|inner| {
            let close = inner.find(')')?;
            (close > 0).then(|| inner.get(close + 1..)).flatten()
        });
        if let Some(stripped) = scoped
            .and_then(|tail| tail.strip_prefix(':'))
            .or_else(|| after.strip_prefix(':'))
        {
            rest = stripped;
        }
    }
    let rest = patterns.skip_brackets.replacen(rest, 1, "");
    let rest = patterns.skip_parentheses.replacen(&rest, 1, "");
    is_version(trim(&rest))
}

/// A strict semantic version, as `semver.valid` takes it: `1.2.3`, `v1.2.3-rc.1+build`.
fn is_version(text: &str) -> bool {
    const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
    if text.len() > 256 {
        return false;
    }
    let text = text.strip_prefix('v').unwrap_or(text);
    let (text, build) = match text.split_once('+') {
        Some((text, build)) => (text, Some(build)),
        None => (text, None),
    };
    let (main, prerelease) = match text.split_once('-') {
        Some((main, prerelease)) => (main, Some(prerelease)),
        None => (text, None),
    };
    let numeric = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    let identifier = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    };
    let numbers: Vec<&str> = main.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|part| {
            numeric(part)
                && part
                    .parse::<u64>()
                    .is_ok_and(|number| number <= MAX_SAFE_INTEGER)
        })
        && prerelease.is_none_or(|prerelease| {
            prerelease.split('.').all(|part| {
                identifier(part)
                    && (numeric(part) || part.bytes().any(|byte| !byte.is_ascii_digit()))
            })
        })
        && build.is_none_or(|build| build.split('.').all(identifier))
}

/// A message split as `conventional-commits-parser` splits it with the conventional-commits
/// preset. `None` is the parser's `null`; `""` is a part it found empty.
#[derive(Debug, Default)]
struct Parsed {
    /// The message as given, comments included, with the appended line feed.
    raw: String,
    header: Option<String>,
    kind: Option<String>,
    scope: Option<String>,
    subject: Option<String>,
    body: Option<String>,
    footer: Option<String>,
}

const SCISSORS: &str = "# ------------------------ >8 ------------------------";

/// The keywords that make a line with an issue reference a footer line.
const ACTIONS: &[&str] = &[
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

impl Parsed {
    fn read(raw: &str, patterns: &Patterns) -> Self {
        let mut parsed = Self {
            raw: raw.to_owned(),
            ..Self::default()
        };
        let all = lines(trim_new_lines(raw));
        let end = all
            .iter()
            .position(|line| *line == SCISSORS)
            .unwrap_or(all.len());
        let kept: Vec<&str> = all
            .iter()
            .take(end)
            .copied()
            .filter(|line| !line.starts_with('#') && !trim(line).starts_with("gpg:"))
            .collect();
        let mut rest = kept.iter().copied();

        if let Some(header) = rest.next().filter(|header| !header.is_empty()) {
            let captures = patterns
                .breaking
                .captures(header)
                .or_else(|| patterns.header.captures(header));
            if let Some(captures) = captures {
                let group = |index| {
                    captures
                        .get(index)
                        .map(|found| found.as_str().to_owned())
                        .filter(|text| !text.is_empty())
                };
                parsed.kind = group(1);
                parsed.scope = group(2);
                parsed.subject = group(3);
            }
            parsed.header = Some(header.to_owned());
        }

        let remaining: Vec<&str> = rest.collect();
        let mut index = 0;
        let mut is_body = true;
        while index < remaining.len() {
            if parsed.notes(&remaining, &mut index) {
                is_body = false;
            }
            if index < remaining.len() {
                let line = remaining[index];
                let still_body = is_body && !has_references(line);
                if still_body {
                    append(&mut parsed.body, line);
                } else {
                    append(&mut parsed.footer, line);
                }
                index += 1;
                if !still_body {
                    is_body = false;
                }
            }
        }
        for part in [&mut parsed.body, &mut parsed.footer] {
            if let Some(text) = part.as_mut().filter(|text| !text.is_empty()) {
                *text = trim_new_lines(text).to_owned();
            }
        }
        parsed
    }

    /// A `BREAKING CHANGE` note at `index` and the lines that continue it go to the footer.
    fn notes(&mut self, lines: &[&str], index: &mut usize) -> bool {
        if *index >= lines.len() || !is_note(lines[*index]) {
            return false;
        }
        append(&mut self.footer, lines[*index]);
        *index += 1;
        while *index < lines.len() {
            if self.notes(lines, index) {
                return true;
            }
            let line = lines[*index];
            let references = has_references(line);
            append(&mut self.footer, line);
            *index += 1;
            if references {
                break;
            }
        }
        true
    }
}

/// The parser's `appendLine`: an empty part is replaced, not extended.
fn append(part: &mut Option<String>, line: &str) {
    match part {
        Some(text) if !text.is_empty() => {
            text.push('\n');
            text.push_str(line);
        }
        _ => *part = Some(line.to_owned()),
    }
}

/// `^[\s|*]*(BREAKING CHANGE|BREAKING-CHANGE)[:\s]+`, ignoring case.
fn is_note(line: &str) -> bool {
    let rest =
        line.trim_start_matches(|character| is_space(character) || matches!(character, '|' | '*'));
    ["BREAKING CHANGE", "BREAKING-CHANGE"]
        .iter()
        .any(|keyword| {
            rest.get(..keyword.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(keyword))
                && rest[keyword.len()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next == ':' || is_space(next))
        })
}

/// The keyword of [`ACTIONS`] that starts at `at`, by the reference's alternation order, when
/// whitespace follows it; with `spaced` false, any keyword that starts there.
fn action_at(line: &str, at: usize, spaced: bool) -> Option<usize> {
    let tail = line.get(at..)?;
    ACTIONS.iter().find_map(|keyword| {
        let head = tail.get(..keyword.len())?;
        if !head.eq_ignore_ascii_case(keyword) {
            return None;
        }
        let next = tail[keyword.len()..].chars().next();
        (!spaced || next.is_some_and(is_space)).then_some(keyword.len())
    })
}

/// The line holds an issue reference (`#12`), as the parser finds one: in a sentence after an
/// action keyword (`fixes #12`), or anywhere in a line without one; never in a sentence with a
/// URL.
fn has_references(line: &str) -> bool {
    let starts: Vec<usize> = line.char_indices().map(|(at, _)| at).collect();
    let mut sentences = Vec::new();
    let mut at = 0;
    while at < line.len() {
        let Some(keyword) = action_at(line, at, true) else {
            at = starts
                .iter()
                .copied()
                .find(|start| *start > at)
                .unwrap_or(line.len());
            continue;
        };
        let begin = at + keyword;
        let begin = begin
            + line[begin..]
                .char_indices()
                .find(|(_, character)| !is_space(*character))
                .map_or(line.len() - begin, |(offset, _)| offset);
        let end = starts
            .iter()
            .copied()
            .filter(|start| *start >= begin)
            .find(|start| action_at(line, *start, false).is_some())
            .unwrap_or(line.len());
        sentences.push(&line[begin..end]);
        at = end;
    }
    if sentences.is_empty() && !line.is_empty() {
        sentences.push(line);
    }
    sentences
        .iter()
        .any(|sentence| !has_url(sentence) && has_issue(sentence))
}

/// `#` and an issue id, followed by whitespace, `,`, `;`, `)`, `]` or the end.
fn has_issue(sentence: &str) -> bool {
    sentence.match_indices('#').any(|(at, _)| {
        let id = &sentence[at + 1..];
        let length = id
            .find(|character: char| !(is_word(character) || character == '-'))
            .unwrap_or(id.len());
        length > 0
            && id[length..]
                .chars()
                .next()
                .is_none_or(|next| is_space(next) || matches!(next, ',' | ';' | ')' | ']'))
    })
}

/// The parser's URL: `http://` or `https://` after a word boundary, with a word character among
/// the URL characters that follow.
fn has_url(sentence: &str) -> bool {
    sentence.match_indices("http").any(|(at, _)| {
        let before = sentence[..at].chars().next_back();
        if before.is_some_and(is_word) {
            return false;
        }
        let rest = &sentence[at + 4..];
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        rest.strip_prefix("://").is_some_and(|tail| {
            tail.chars()
                .take_while(|character| is_word(*character) || "-@:%+.~#?&/=".contains(*character))
                .any(is_word)
        })
    })
}

/// A line with `http://` or `https://` and more, which no line-length check counts.
fn has_link(line: &str) -> bool {
    line.match_indices("http").any(|(at, _)| {
        let before = line[..at].chars().next_back();
        if before.is_some_and(is_word) {
            return false;
        }
        let rest = &line[at + 4..];
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        rest.strip_prefix("://")
            .and_then(|tail| tail.chars().next())
            .is_some_and(|next| !is_space(next))
    })
}

/// The text is lower case, as `ensureCase` decides it: quoted spans are skipped, and an empty
/// rest or one that starts with a digit passes.
fn is_lower_case(text: &str, patterns: &Patterns) -> bool {
    let unquoted = patterns.quoted.replace_all(text, "");
    let input = trim(&unquoted);
    let lower = input.to_lowercase();
    lower.is_empty()
        || lower.starts_with(|character: char| character.is_ascii_digit())
        || lower == input
}

/// The first line of `text` longer than `longest`, with its number and length.
fn long_line(text: &str, longest: usize) -> Option<(usize, usize)> {
    if longest == 0 {
        return None;
    }
    lines(text)
        .iter()
        .enumerate()
        .map(|(number, line)| (number + 1, length(line), *line))
        .find(|(_, size, line)| *size > longest && !has_link(line))
        .map(|(number, size, _)| (number, size))
}

/// The line before the first line of `part` that equals it in the raw message, as the
/// reference's leading-blank checks find it.
fn line_before<'r>(raw: &'r [&'r str], part: &str) -> Option<&'r str> {
    let first = lines(part).first().copied().unwrap_or_default();
    let offset = raw
        .iter()
        .position(|line| *line == first)
        .map_or(-1, |at| isize::try_from(at).unwrap_or(isize::MAX));
    // JavaScript's `slice(offset - 1)`: a negative start counts from the end.
    let start = offset - 1;
    let count = isize::try_from(raw.len()).unwrap_or(isize::MAX);
    let start = if start < 0 {
        (count + start).max(0)
    } else {
        start
    };
    usize::try_from(start)
        .ok()
        .and_then(|start| raw.get(start))
        .copied()
}

/// The failures of one message: aspect, message and help, in [`ASPECTS`] order.
#[derive(Default)]
struct Failures(Vec<Failure>);

impl Failures {
    fn add(&mut self, aspect: &'static str, message: String, help: &str) {
        self.0.push((aspect, message, help.to_owned()));
    }
}

/// The part, unless it is missing or empty: the reference skips both alike.
fn present(part: Option<&String>) -> Option<&str> {
    part.map(String::as_str).filter(|text| !text.is_empty())
}

/// Every check `parsed` fails, in [`ASPECTS`] order.
fn checks(parsed: &Parsed, options: &Options, patterns: &Patterns) -> Vec<Failure> {
    let mut found = Failures::default();
    let form = format!(
        "write the header as <type>(<scope>): <subject>, in lower case, with a type of {}",
        options.types.join(", ")
    );
    header_checks(
        &mut found,
        parsed.header.as_deref(),
        options,
        patterns,
        &form,
    );
    type_checks(&mut found, parsed, options, patterns, &form);
    scope_checks(&mut found, parsed, options, patterns, &form);
    subject_checks(&mut found, parsed, options, patterns, &form);
    body_checks(&mut found, parsed, options);
    found.0
}

fn header_checks(
    found: &mut Failures,
    header: Option<&str>,
    options: &Options,
    patterns: &Patterns,
    form: &str,
) {
    let Some(header) = header else {
        // The reference's length checks fail a message without a header line.
        let message = || "the message has no header line".to_owned();
        found.add("header-max-length", message(), form);
        found.add("header-min-length", message(), form);
        return;
    };
    let leading = header.starts_with(is_space);
    let trailing = header.ends_with(is_space);
    if leading || trailing {
        let place = match (leading, trailing) {
            (true, true) => "starts and ends",
            (true, false) => "starts",
            _ => "ends",
        };
        found.add(
            "header-trim",
            format!("the header {header:?} {place} with whitespace"),
            "remove the whitespace around the header",
        );
    }
    let size = length(header);
    if options.header_max_length > 0 && size > options.header_max_length {
        found.add(
            "header-max-length",
            format!(
                "the header is {size} characters long, longer than header-max-length {}",
                options.header_max_length
            ),
            "shorten the header; the body can say the rest",
        );
    }
    if size < options.header_min_length {
        found.add(
            "header-min-length",
            format!(
                "the header is {size} characters long, shorter than header-min-length {}",
                options.header_min_length
            ),
            "say in the header what the change does",
        );
    }
    if header.starts_with(|character: char| character.is_ascii_alphabetic())
        && !is_lower_case(header, patterns)
    {
        found.add(
            "header-case",
            format!("the header {header:?} is not lower case"),
            form,
        );
    }
    if header.ends_with('.') {
        found.add(
            "header-full-stop",
            format!("the header {header:?} ends with a full stop"),
            "remove the full stop at the end of the header",
        );
    }
}

fn type_checks(
    found: &mut Failures,
    parsed: &Parsed,
    options: &Options,
    patterns: &Patterns,
    form: &str,
) {
    let header = parsed.header.as_deref().unwrap_or_default();
    let Some(kind) = present(parsed.kind.as_ref()) else {
        found.add(
            "type-empty",
            format!("the header {header:?} names no type"),
            form,
        );
        return;
    };
    if !is_lower_case(kind, patterns) {
        found.add("type-case", format!("type {kind} is not lower case"), form);
    }
    if !options.types.iter().any(|known| known == kind) {
        found.add(
            "type-enum",
            format!(
                "type {kind} is not a commit type; the commit types: {}",
                options.types.join(", ")
            ),
            form,
        );
    }
}

fn scope_checks(
    found: &mut Failures,
    parsed: &Parsed,
    options: &Options,
    patterns: &Patterns,
    form: &str,
) {
    let header = parsed.header.as_deref().unwrap_or_default();
    let Some(scope) = present(parsed.scope.as_ref()) else {
        found.add(
            "scope-empty",
            format!("the header {header:?} names no scope"),
            form,
        );
        return;
    };
    // Each of several scopes (`a/b`, `a\b`, `a, b`) is lower case.
    if !scope
        .split(['/', '\\', ','])
        .all(|segment| is_lower_case(segment, patterns))
    {
        found.add(
            "scope-case",
            format!("scope {scope} is not lower case"),
            form,
        );
    }
    let size = length(scope);
    if options.scope_max_length > 0 && size > options.scope_max_length {
        found.add(
            "scope-max-length",
            format!(
                "scope {scope} is {size} characters long, longer than scope-max-length {}",
                options.scope_max_length
            ),
            "name a shorter scope",
        );
    }
}

fn subject_checks(
    found: &mut Failures,
    parsed: &Parsed,
    options: &Options,
    patterns: &Patterns,
    form: &str,
) {
    let header = parsed.header.as_deref();
    let subject = present(parsed.subject.as_ref());
    match subject {
        None => found.add(
            "subject-empty",
            format!("the header {:?} has no subject", header.unwrap_or_default()),
            form,
        ),
        Some(subject) => {
            if patterns.cased.is_match(subject) && !is_lower_case(subject, patterns) {
                found.add(
                    "subject-case",
                    format!("the subject {subject:?} is not lower case"),
                    form,
                );
            }
        }
    }
    // The reference reads the stop off the header, and an ellipsis is not one.
    if let Some(header) = header
        && header.ends_with('.')
        && !header.ends_with("...")
    {
        found.add(
            "subject-full-stop",
            format!("the subject of {header:?} ends with a full stop"),
            "remove the full stop at the end of the subject",
        );
    }
    let Some(subject) = subject else {
        return;
    };
    let size = length(subject);
    if size < options.subject_min_length {
        found.add(
            "subject-min-length",
            format!(
                "the subject is {size} characters long, shorter than subject-min-length {}",
                options.subject_min_length
            ),
            "say in the subject what the change does",
        );
    }
    if options.subject_max_length > 0 && size > options.subject_max_length {
        found.add(
            "subject-max-length",
            format!(
                "the subject is {size} characters long, longer than subject-max-length {}",
                options.subject_max_length
            ),
            "shorten the subject; the body can say the rest",
        );
    }
}

fn body_checks(found: &mut Failures, parsed: &Parsed, options: &Options) {
    let raw = lines(&parsed.raw);
    if let Some(body) = present(parsed.body.as_ref()) {
        // The reference looks at the second line of the message as given, comments included.
        if raw.get(1).is_none_or(|line| !line.is_empty()) {
            found.add(
                "body-leading-blank",
                "the body does not start after a blank line".to_owned(),
                "leave the line after the header blank",
            );
        }
        if let Some((number, size)) = long_line(body, options.body_max_line_length) {
            found.add(
                "body-max-line-length",
                format!(
                    "body line {number} is {size} characters long, longer than \
                     body-max-line-length {}",
                    options.body_max_line_length
                ),
                "wrap the body's lines",
            );
        }
        if !body.ends_with('.') {
            found.add(
                "body-full-stop",
                "the body does not end with a full stop".to_owned(),
                "end the body with a full stop",
            );
        }
    }
    if let Some(footer) = present(parsed.footer.as_ref()) {
        if line_before(&raw, footer).is_none_or(|line| !line.is_empty()) {
            found.add(
                "footer-leading-blank",
                "the footer does not start after a blank line".to_owned(),
                "leave a blank line before the footer",
            );
        }
        if let Some((number, size)) = long_line(footer, options.footer_max_line_length) {
            found.add(
                "footer-max-line-length",
                format!(
                    "footer line {number} is {size} characters long, longer than \
                     footer-max-line-length {}",
                    options.footer_max_line_length
                ),
                "wrap the footer's lines",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(message: &str) -> Parsed {
        let patterns = Patterns::new().expect("patterns");
        Parsed::read(&format!("{message}\n"), &patterns)
    }

    #[test]
    fn the_parts_split_as_the_parser_splits_them() {
        let one = parsed("feat(core): add x\n\nThe body.\n\nCloses #12");
        assert_eq!(one.kind.as_deref(), Some("feat"));
        assert_eq!(one.scope.as_deref(), Some("core"));
        assert_eq!(one.subject.as_deref(), Some("add x"));
        assert_eq!(one.body.as_deref(), Some("The body."));
        assert_eq!(one.footer.as_deref(), Some("Closes #12"));

        let breaking = parsed("feat(a)!: b): c");
        assert_eq!(breaking.scope.as_deref(), Some("a"));
        assert_eq!(breaking.subject.as_deref(), Some("b): c"));

        let note = parsed("fix(x): y z\n\nBREAKING CHANGE: gone\nstill the note");
        // The blank line after the header opens an empty body, as in the parser.
        assert_eq!(note.body.as_deref(), Some(""));
        assert_eq!(
            note.footer.as_deref(),
            Some("BREAKING CHANGE: gone\nstill the note")
        );

        let comment = parsed("# a comment\n\nfeat(x): late");
        assert_eq!(comment.header, None);
        assert_eq!(comment.body.as_deref(), Some("feat(x): late"));
    }

    #[test]
    fn references_follow_the_parser() {
        assert!(has_references("Closes #12"));
        assert!(has_references("see #12, thanks"));
        assert!(!has_references("fixes the #12x! thing"));
        assert!(!has_references("see https://example.test/#12"));
        assert!(!has_references("see #12 fixes the bug"));
        assert!(has_references("prefix #3"));
    }

    #[test]
    fn versions_are_strict() {
        for good in ["1.2.3", "v1.2.3", "1.0.0-rc.1+build.5", "0.0.0-0"] {
            assert!(is_version(good), "{good}");
        }
        for bad in ["1.2", "01.2.3", "1.2.3-01", "=1.2.3", "1.2.3-", "V1.2.3"] {
            assert!(!is_version(bad), "{bad}");
        }
    }
}
