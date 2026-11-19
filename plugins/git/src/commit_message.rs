//! `git/commit-message`: the message of the commit being made (`gate check --commit-message
//! <file>`, which the generated `commit-msg` hook passes), and the message of each commit a push
//! sends (`gate check --push`: `<base>..<head>`, or the head commit alone for a new branch), have
//! the conventional form, as `commitlint` checks it with `@commitlint/config-conventional` and the
//! rule levels of `@elsikora/git-branch-lint`: `<type>(<scope>): <subject>`, then a blank line and
//! a body, then a blank line and a footer.
//!
//! Each check is an aspect named after the `commitlint` rule it ports. The message is read as
//! `commitlint --edit` reads it: comment lines (`#`) and what follows the scissors line are not
//! part of it, merge, revert, fixup, squash and release messages are not checked, and the parts
//! are split as `conventional-commits-parser` splits them.

use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Git, Result, Rule, Run, Scope};
use regex::Regex;

use crate::history::{Commit, ahead, main_reference};
use crate::javascript::{ANY, translate};
use crate::settings::Settings;

/// The rule.
#[derive(Debug, Default)]
pub struct CommitMessage;

/// The options of `git/commit-message`; every one defaults to the reference's value. A length is
/// counted as the reference counts it, in UTF-16 code units; 0 turns a longest-length check off.
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
    /// The header's regular expression (`headerPattern` of the parser), with the named groups
    /// `type`, `scope` and `subject`. It alone decides the header, a breaking `!` included; unset,
    /// a breaking header (`type(scope)!: subject`) is read with the conventional breaking pattern
    /// first, as the reference reads it, and any other with the conventional pattern.
    pub header_pattern: Option<String>,
    /// Regular expressions of messages that are not checked (`ignores`), besides the default ones.
    pub ignores: Vec<String>,
    /// Whether the reference's default ignores apply (`defaultIgnores`): merges, reverts,
    /// reapplies, `amend!`, `fixup!` and `squash!` commits, and a release whose header is a
    /// version.
    pub default_ignores: bool,
    /// Regular expressions of the trailers the message being written and the pushed commits'
    /// messages may not carry, each searched in every trailer of its trailer block, written
    /// `Token: value`. An ignored message is checked too.
    pub prohibited_trailers: Vec<String>,
    /// Regular expressions of the body lines the message being written and the pushed commits'
    /// messages may not carry, each searched in every line after the header that is neither
    /// blank nor in the trailer block. An ignored message is checked too.
    pub prohibited_body_lines: Vec<String>,
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
            header_pattern: None,
            ignores: Vec::new(),
            default_ignores: true,
            prohibited_trailers: Vec::new(),
            prohibited_body_lines: Vec::new(),
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
    "prohibited-body-lines",
    "prohibited-trailers",
];

impl Rule for CommitMessage {
    type Options = Options;

    const NAME: &'static str = "commit-message";
    const TITLE: &'static str =
        "Each commit message, made or pushed, has the conventional form: type(scope): subject";
    const ASPECTS: &'static [&'static str] = ASPECTS;
    const SCOPE: Scope = Scope::Repository;
    const READS_HISTORY: bool = true;
    const READS_MESSAGE: bool = true;

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let patterns = Patterns::new(run.options())?;
        let git = run.git()?;
        if let Some(message) = git.message()? {
            report(run, &patterns, &message, None);
        }
        for commit in pushed(run, &git)? {
            report(run, &patterns, &commit.message, Some(&commit));
        }
        Ok(())
    }
}

/// The commits a push to a branch adds, oldest first: those reachable from the pushed head and
/// neither from the remote ref's old tip (`base`, absent for a new branch) nor from main, so a
/// merge of main or a rebase onto it judges the branch's own commits only. A new branch's set is
/// `main..head`, the set `git/no-ci-skip` judges. None for a run that is not a push to a
/// `refs/heads/` reference: `--rev`, or a pushed tag.
///
/// # Errors
/// When `[plugins.git] options` cannot be read or none of its main references names a commit.
fn pushed(run: &Run<'_, Options>, git: &Git<'_>) -> Result<Vec<Commit>> {
    let Some(range) = git.range().filter(|range| {
        range
            .target_ref
            .as_deref()
            .is_some_and(|target| target.starts_with("refs/heads/"))
    }) else {
        return Ok(Vec::new());
    };
    let settings = Settings::read(run)?;
    let main = main_reference(git, &settings)?;
    let mut commits = ahead(git, &main, &range.head);
    if let Some(base) = &range.base {
        let sent: HashSet<String> = git
            .log(&format!("{base}..{}", range.head))
            .into_iter()
            .map(|commit| commit.id)
            .collect();
        commits.retain(|commit| sent.contains(&commit.id));
    }
    Ok(commits)
}

/// Reports what `message` fails: the message being written when `commit` is `None`, else the
/// message of the pushed commit `commit`, whose findings are on that commit, name it as their
/// target, are keyed by it and start with its short id.
fn report(run: &mut Run<'_, Options>, patterns: &Patterns, message: &str, commit: Option<&Commit>) {
    let location = commit.map(Commit::location);
    let text = |text: String| match commit {
        Some(commit) => format!("{}: {text}", commit.short()),
        None => text,
    };
    // A pushed commit is reworded in history, or excepted when others already have it.
    let advice = |help: &str| match commit {
        Some(commit) => format!(
            "{help}; reword the commit with git rebase -i {}^ and push the branch again, or, when \
             others already have it, add an exception for its id",
            commit.short()
        ),
        None => help.to_owned(),
    };
    if let Some((header, found)) = verdicts(message, run.options(), patterns) {
        for (aspect, failure, help) in found {
            let finding = run
                .report()
                .finding(aspect, location.clone(), text(failure));
            match (commit, &header) {
                (Some(commit), _) => {
                    finding.target(commit.id.as_str()).key(commit.id.as_str());
                }
                (None, Some(header)) => {
                    finding.target(header.clone()).key(header.as_str());
                }
                (None, None) => {}
            }
            finding.help(advice(&help));
        }
    }
    // A body line and a trailer stay in history whatever the header says, so an ignored message is
    // checked too.
    for line in body_lines(message) {
        let Some(pattern) = patterns
            .prohibited_lines
            .iter()
            .find(|(_, prohibited)| prohibited.is_match(line))
            .map(|(text, _)| text)
        else {
            continue;
        };
        let finding = run.report().finding(
            "prohibited-body-lines",
            location.clone(),
            text(format!(
                "the body line {line:?} is prohibited: it matches prohibited-body-lines {pattern:?}"
            )),
        );
        match commit {
            Some(commit) => finding
                .target(commit.id.as_str())
                .key(format!("{} {line}", commit.id)),
            None => finding.target(line).key(line),
        };
        finding.help(advice("remove the line from the message"));
    }
    for trailer in trailers(message) {
        let Some(pattern) = patterns
            .prohibited
            .iter()
            .find(|(_, prohibited)| prohibited.is_match(&trailer))
            .map(|(text, _)| text)
        else {
            continue;
        };
        let finding = run.report().finding(
            "prohibited-trailers",
            location.clone(),
            text(format!(
                "the trailer {trailer:?} is prohibited: it matches prohibited-trailers {pattern:?}"
            )),
        );
        match commit {
            Some(commit) => finding
                .target(commit.id.as_str())
                .key(format!("{} {trailer}", commit.id)),
            None => finding.target(trailer.clone()).key(trailer.as_str()),
        };
        finding.help(advice("remove the trailer from the message"));
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
    if (options.default_ignores && ignored(&raw, patterns))
        || patterns.ignores.iter().any(|ignore| ignore.is_match(&raw))
    {
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
    /// `conventional-changelog-conventionalcommits`' header with `!`, tried first; none under a
    /// custom `header-pattern`, which decides the header alone.
    breaking: Option<Regex>,
    /// Its header, or `header-pattern`.
    header: Regex,
    /// The messages `ignores` skips.
    ignores: Vec<Regex>,
    /// `prohibited-trailers`, each with the text it was written as.
    prohibited: Vec<(String, Regex)>,
    /// `prohibited-body-lines`, each with the text it was written as.
    prohibited_lines: Vec<(String, Regex)>,
    /// A quoted span, which a case check skips.
    quoted: Regex,
    /// A first character that has a case.
    cased: Regex,
    /// A CI-skip marker a release header may carry.
    skip_brackets: Regex,
    skip_parentheses: Regex,
}

impl Patterns {
    fn new(options: &Options) -> Result<Self> {
        let any = ANY;
        let header = match &options.header_pattern {
            Some(text) => header_pattern(text)?,
            None => builtin(
                "the header pattern",
                &format!(
                    r"^(?<type>[0-9A-Za-z_]*)(?:\((?<scope>{any}*)\))?!?: (?<subject>{any}*)$"
                ),
            )?,
        };
        let ignores = options
            .ignores
            .iter()
            .map(|text| javascript("ignores", text, WRITE_IGNORE))
            .collect::<Result<Vec<_>>>()?;
        let prohibited = refusals(
            "prohibited-trailers",
            &options.prohibited_trailers,
            "trailer",
            WRITE_TRAILER,
        )?;
        let prohibited_lines = refusals(
            "prohibited-body-lines",
            &options.prohibited_body_lines,
            "body line",
            WRITE_BODY_LINE,
        )?;
        let breaking = if options.header_pattern.is_some() {
            None
        } else {
            Some(builtin(
                "the header pattern",
                &format!(r"^(?<type>[0-9A-Za-z_]*)(?:\((?<scope>{any}*)\))?!: (?<subject>{any}*)$"),
            )?)
        };
        Ok(Self {
            breaking,
            header,
            ignores,
            prohibited,
            prohibited_lines,
            quoted: builtin(
                "the quoted-span pattern",
                &format!("`{any}*?`|\"{any}*?\"|'{any}*?'"),
            )?,
            cased: builtin("the cased-letter pattern", r"(?i)^[\p{Ll}\p{Lu}\p{Lt}]")?,
            skip_brackets: builtin(
                "the skip pattern",
                r"\[(?:[sS][kK][iI][pP]|[cC][iI])(?:-|[\s&&[^\u{85}]]|\u{FEFF})(?:[cC][iI]|[sS][kK][iI][pP])\]",
            )?,
            skip_parentheses: builtin(
                "the skip pattern",
                r"\((?:[sS][kK][iI][pP]|[cC][iI])(?:-|[\s&&[^\u{85}]]|\u{FEFF})(?:[cC][iI]|[sS][kK][iI][pP])\)",
            )?,
        })
    }
}

/// What to write in `header-pattern`.
const WRITE_HEADER: &str = "write a JavaScript regular expression without lookaround or \
                            backreferences, with the named groups type, scope and subject, such \
                            as ^(?<type>\\w+)(?:\\((?<scope>[a-z0-9-]+)\\))?!?: (?<subject>.+)$";

/// What to write in `ignores`.
const WRITE_IGNORE: &str = "write a JavaScript regular expression without lookaround or \
                            backreferences that the messages to skip match, such as \"^Merge \"";

/// The patterns of the option `option`, each with the text it was written as. An empty pattern,
/// which would refuse every `what`, and one that does not compile are configuration errors that
/// name the option and say what to `write`.
fn refusals(
    option: &str,
    texts: &[String],
    what: &str,
    write: &str,
) -> Result<Vec<(String, Regex)>> {
    texts
        .iter()
        .map(|text| {
            if text.is_empty() {
                return Err(Error::config(format!(
                    "git/commit-message {option} has an empty pattern, which refuses every \
                     {what}; {write}"
                )));
            }
            javascript(option, text, write).map(|pattern| (text.clone(), pattern))
        })
        .collect()
}

/// What to write in `prohibited-body-lines`.
const WRITE_BODY_LINE: &str = "write a JavaScript regular expression without lookaround or \
                               backreferences that the body lines to refuse match, such as \
                               \"^Generated with \"";

/// What to write in `prohibited-trailers`.
const WRITE_TRAILER: &str = "write a JavaScript regular expression without lookaround or \
                             backreferences that the trailers to refuse match, written Token: \
                             value, such as \"^[Cc]o-[Aa]uthored-[Bb]y: \"";

/// `text`, the value of `option`, read as JavaScript reads it and compiled.
///
/// # Errors
/// When it holds a construct the translation does not read, or does not compile, naming the
/// option, the construct and what to write.
fn javascript(option: &str, text: &str, write: &str) -> Result<Regex> {
    let translated = translate(text).map_err(|construct| {
        Error::config(format!(
            "git/commit-message {option} {text:?} has {construct}, which the rule does not read; \
             {write}"
        ))
    })?;
    Regex::new(&translated).map_err(|error| {
        Error::config(format!(
            "git/commit-message {option} {text:?} is not a valid regular expression; {write}: \
             {error}"
        ))
    })
}

/// A pattern of the rule itself, in the `regex` crate's syntax.
///
/// # Errors
/// When it does not compile, which is a bug of the rule.
fn builtin(name: &str, text: &str) -> Result<Regex> {
    Regex::new(text).map_err(|error| {
        Error::new(format!(
            "{name} of git/commit-message does not compile ({error}); report this as a bug"
        ))
    })
}

/// `header-pattern`, which must compile and name a `type` group.
///
/// # Errors
/// When it does not compile or has no `type` group, naming the option and what to write.
fn header_pattern(text: &str) -> Result<Regex> {
    let header = javascript("header-pattern", text, WRITE_HEADER)?;
    if !header.capture_names().flatten().any(|name| name == "type") {
        return Err(Error::config(format!(
            "git/commit-message header-pattern {text:?} has no group named type; {WRITE_HEADER}"
        )));
    }
    Ok(header)
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

/// The length the reference measures: JavaScript's `length`, in UTF-16 code units, so a character
/// outside the Basic Multilingual Plane (an emoji) counts twice.
fn length(text: &str) -> usize {
    text.encode_utf16().count()
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

/// The line `git commit --verbose` writes above the diff; git stores nothing from it on.
pub(crate) const SCISSORS: &str = "# ------------------------ >8 ------------------------";

/// The message of a `commit-msg` file above the scissors line, trimmed. Comment lines stay: git
/// keeps them in a message given with `-m` or `-F`, so a directive in one may be stored.
pub(crate) fn above_scissors(raw: &str) -> String {
    let kept: Vec<&str> = raw.lines().take_while(|line| *line != SCISSORS).collect();
    kept.join("\n").trim().to_owned()
}

/// The lines git writes itself, one of which lets a paragraph of a quarter trailers be a trailer
/// block.
const GIT_GENERATED: &[&str] = &["Signed-off-by: ", "(cherry picked from commit "];

/// White space as git's `isspace` knows it.
fn is_git_space(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r')
}

fn trim_git(text: &str) -> &str {
    text.trim_matches(is_git_space)
}

/// The offset of the `:` that ends a trailer's token, as git's `find_separator` finds it: a token
/// of ASCII letters, digits and `-`, then optional spaces or tabs.
fn separator(line: &str) -> Option<usize> {
    let mut spaced = false;
    for (at, character) in line.char_indices() {
        if character == ':' {
            return (at >= 1).then_some(at);
        }
        if !spaced && (character.is_ascii_alphanumeric() || character == '-') {
            continue;
        }
        if at > 0 && matches!(character, ' ' | '\t') {
            spaced = true;
            continue;
        }
        return None;
    }
    None
}

/// Where the log message of `lines` ends, as git's `ignored_log_message_bytes` finds it: before
/// the trailing run of comment lines, empty lines and an old-style `Conflicts:` block, whose paths
/// are indented by a tab.
fn log_end(lines: &[&str]) -> usize {
    let mut start = None;
    let mut conflicts = false;
    for (at, line) in lines.iter().enumerate() {
        if line.starts_with('#') || line.is_empty() {
            start.get_or_insert(at);
        } else if *line == "Conflicts:" {
            conflicts = true;
            start.get_or_insert(at);
        } else if !(conflicts && line.starts_with('\t')) {
            start = None;
            conflicts = false;
        }
    }
    start.unwrap_or(lines.len())
}

/// A message as git reads it for its trailers: the lines above the scissors line and before a
/// trailing run of comment lines, empty lines and an old-style `Conflicts:` block, without comment
/// lines and trailing blank lines; and where its trailer block starts, when it has one.
struct Kept<'m> {
    lines: Vec<&'m str>,
    block: Option<usize>,
    /// The indexes in `lines` of the block's lines that are neither a trailer nor a continuation
    /// of one: prose that git's quarter rule lets a block with a line git writes itself hold.
    prose: Vec<usize>,
}

/// `message` as [`Kept`]. The trailer block is the last paragraph, never the first (the title),
/// when every line of it is a trailer or continues one (starts with white space), or when it holds
/// a line of [`GIT_GENERATED`] and at least a quarter of its lines are trailers.
fn kept(message: &str) -> Kept<'_> {
    let above: Vec<&str> = lines(message)
        .into_iter()
        .take_while(|line| *line != SCISSORS)
        .collect();
    let mut kept: Vec<&str> = above[..log_end(&above)]
        .iter()
        .copied()
        .filter(|line| !line.starts_with('#'))
        .collect();
    let blank = |line: &&str| trim_git(line).is_empty();
    let end = kept
        .iter()
        .rposition(|line| !blank(line))
        .map_or(0, |last| last + 1);
    kept.truncate(end);
    // The first paragraph is the title; without a blank line after it there is no other.
    let Some(start) = kept.iter().rposition(blank).map(|at| at + 1) else {
        return Kept {
            lines: kept,
            block: None,
            prose: Vec::new(),
        };
    };

    let mut trailer_lines = 0_usize;
    let mut prose = Vec::new();
    let mut recognized = false;
    let mut after_trailer = false;
    for (at, line) in kept.iter().enumerate().skip(start) {
        if line.starts_with(is_git_space) {
            if !after_trailer {
                prose.push(at);
            }
        } else if GIT_GENERATED.iter().any(|prefix| line.starts_with(prefix)) {
            recognized = true;
            trailer_lines += 1;
            after_trailer = true;
        } else if separator(line).is_some() {
            trailer_lines += 1;
            after_trailer = true;
        } else {
            prose.push(at);
            after_trailer = false;
        }
    }
    let is_block =
        trailer_lines > 0 && (prose.is_empty() || (recognized && trailer_lines * 3 >= prose.len()));
    if !is_block {
        prose.clear();
    }
    Kept {
        lines: kept,
        block: is_block.then_some(start),
        prose,
    }
}

/// The lines `prohibited-body-lines` reads in `message`: each line after the header that is not
/// blank, as [`Kept`] finds them, without the trailers of the trailer block and their
/// continuation lines; the prose a block holds under git's quarter rule is read.
fn body_lines(message: &str) -> Vec<&str> {
    let kept = kept(message);
    let blank = |line: &&str| trim_git(line).is_empty();
    let end = kept.block.unwrap_or(kept.lines.len());
    let mut found: Vec<&str> = kept.lines[..end]
        .iter()
        .copied()
        .skip_while(blank)
        .skip(1)
        .filter(|line| !blank(line))
        .collect();
    found.extend(kept.prose.iter().map(|&at| kept.lines[at]));
    found
}

/// The trailers of `message` as `git interpret-trailers` finds them, each unfolded and written
/// `Token: value`: the lines of its trailer block (see [`kept`]). Comment lines, a trailing
/// old-style `Conflicts:` block and what follows the scissors line are not part of the message.
fn trailers(message: &str) -> Vec<String> {
    let kept = kept(message);
    let Some(start) = kept.block else {
        return Vec::new();
    };
    let block = &kept.lines[start..];

    let mut found: Vec<(&str, String)> = Vec::new();
    let mut after_trailer = false;
    for line in block {
        if line.starts_with(is_git_space) {
            if after_trailer && let Some((_, value)) = found.last_mut() {
                let more = trim_git(line);
                if !value.is_empty() && !more.is_empty() {
                    value.push(' ');
                }
                value.push_str(more);
            }
            continue;
        }
        after_trailer = match separator(line) {
            Some(at) => {
                found.push((trim_git(&line[..at]), trim_git(&line[at + 1..]).to_owned()));
                true
            }
            None => false,
        };
    }
    found
        .into_iter()
        .map(|(token, value)| {
            if value.is_empty() {
                format!("{token}:")
            } else {
                format!("{token}: {value}")
            }
        })
        .collect()
}

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
                .as_ref()
                .and_then(|breaking| breaking.captures(header))
                .or_else(|| patterns.header.captures(header));
            if let Some(captures) = captures {
                let group = |name| {
                    captures
                        .name(name)
                        .map(|found| found.as_str().to_owned())
                        .filter(|text| !text.is_empty())
                };
                parsed.kind = group("type");
                parsed.scope = group("scope");
                parsed.subject = group("subject");
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

/// `body` above the trailer block of the message `raw`, without the white space before it, when
/// the body ends with the block, or with its first lines, as a paragraph of its own: the parser
/// ends the body at a line with an issue reference, such as `Refs: #12`, which may be in the block.
/// `body` itself otherwise.
fn above_trailers<'b>(body: &'b str, raw: &str) -> &'b str {
    let kept = kept(raw);
    let Some(start) = kept.block else {
        return body;
    };
    let block = &kept.lines[start..];
    // Each line of `body`, without its carriage return, with the offset it starts at.
    let mut body_lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in body.split('\n') {
        body_lines.push((offset, line.strip_suffix('\r').unwrap_or(line)));
        offset += line.len() + 1;
    }
    for taken in (1..=block.len()).rev() {
        let Some(split) = body_lines.len().checked_sub(taken) else {
            continue;
        };
        let ends_with_block = body_lines[split..]
            .iter()
            .map(|(_, line)| *line)
            .eq(block[..taken].iter().copied());
        let own_paragraph = split
            .checked_sub(1)
            .is_none_or(|before| trim_git(body_lines[before].1).is_empty());
        if ends_with_block && own_paragraph {
            // The offset of a line's start follows a line feed, so it is a character boundary.
            let (cut, _) = body_lines[split];
            return body[..cut].trim_end_matches(is_git_space);
        }
    }
    body
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
        let above = above_trailers(body, &parsed.raw);
        if !above.is_empty() && !above.ends_with('.') {
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

    #[test]
    fn a_pattern_is_read_as_javascript_reads_it() {
        // `.` matches no line terminator and `\w` is ASCII; `\A` is a letter, not an assertion.
        let header = header_pattern(r"^(?<type>\w+): (?<subject>.+)$").expect("compiles");
        assert!(header.is_match("feat: add x"));
        assert!(!header.is_match("feat: add\u{2028}x"));
        assert!(!header.is_match("f\u{e9}at: add x"));
        let letters = javascript("ignores", r"^\Awip\z", WRITE_IGNORE).expect("compiles");
        assert!(letters.is_match("Awipz"));
        assert!(!letters.is_match("wip"));
        let refused = javascript("ignores", "^(?=wip)", WRITE_IGNORE).expect_err("refused");
        assert!(
            refused
                .to_string()
                .contains("ignores \"^(?=wip)\" has a lookaround"),
            "{refused}"
        );
    }

    fn parsed(message: &str) -> Parsed {
        let patterns = Patterns::new(&Options::default()).expect("patterns");
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
    fn the_trailer_block_is_found_as_git_finds_it() {
        let none = Vec::<String>::new();
        // A trailer block: the last paragraph, unfolded, a token with spaces before its colon.
        assert_eq!(
            trailers(
                "feat: x\n\nThe body.\n\nReviewed-by: A\nCo-authored-by: B\n  and C\nAcked-by : D\n"
            ),
            ["Reviewed-by: A", "Co-authored-by: B and C", "Acked-by: D"]
        );
        // The title is never one, and a paragraph with prose in it is not one.
        assert_eq!(trailers("Co-authored-by: B"), none);
        assert_eq!(trailers("feat: x\nCo-authored-by: B"), none);
        assert_eq!(trailers("feat: x\n\nThe body.\nCo-authored-by: B"), none);
        // A paragraph above the last is not one.
        assert_eq!(trailers("feat: x\n\nCo-authored-by: B\n\nThe body."), none);
        // A line git writes itself lets a quarter of trailers be enough.
        assert_eq!(
            trailers("feat: x\n\nSigned-off-by: A\nnot a trailer\nnor this\nnor that\n"),
            ["Signed-off-by: A"]
        );
        assert_eq!(
            trailers("feat: x\n\nSigned-off-by: A\none\ntwo\nthree\nfour\n"),
            none
        );
        // Comment lines, trailing blank lines and the scissors section are not part of it.
        assert_eq!(
            trailers(&format!(
                "feat: x\n\nCo-authored-by: B\n# a comment\n\n\n{SCISSORS}\n\nOther: E\n"
            )),
            ["Co-authored-by: B"]
        );
        // A trailing old-style Conflicts: block is not part of the message, as in git.
        assert_eq!(
            trailers("feat: x\n\nCo-authored-by: A\n\nConflicts:\n\ta.rs\n"),
            ["Co-authored-by: A"]
        );
        assert_eq!(
            trailers("feat: x\n\nCo-authored-by: A\n\nConflicts:\n\ta.rs\nmore\n"),
            none
        );
        assert_eq!(
            trailers("feat: x\r\n\r\nCo-authored-by: B\r\n"),
            ["Co-authored-by: B"]
        );
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
