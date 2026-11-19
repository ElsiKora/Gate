//! `git/branch-name`: the branch a push updates, or else the checked-out branch, has a name the
//! policy allows: a placeholder template, as `@elsikora/git-branch-lint` checks it, or a regular
//! expression.

use std::collections::BTreeMap;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Result, Rule, Run, Scope};
use regex::Regex;

use crate::settings::{Settings, pattern};

/// The rule.
#[derive(Debug, Default)]
pub struct BranchName;

/// The pattern of a free placeholder of `template` (any but `:type` and `:ticket`) when `subject`
/// names none.
pub const DEFAULT_SUBJECT: &str = "[a-z0-9-]+";

/// The pattern of `:ticket`: two or more lower-case letters, a dash and digits (`proj-123`).
pub const TICKET: &str = "[a-z]{2,}-[0-9]+";

/// The pattern of a free placeholder: one for all, or one per placeholder name.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", untagged)]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Subject {
    /// The pattern of every free placeholder.
    All(String),
    /// The pattern of each placeholder it names (`:ticket` included); the others take
    /// [`DEFAULT_SUBJECT`].
    Each(BTreeMap<String, String>),
}

impl Default for Subject {
    fn default() -> Self {
        Self::All(DEFAULT_SUBJECT.to_owned())
    }
}

/// The options of `git/branch-name`. Exactly one of `template` and `pattern` is given; the others
/// default to the reference's own defaults, so `template = ":type/:ticket-:name"` alone is its
/// default policy.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(
    crate = "gate_sdk::schemars",
    extend("oneOf" = [{ "required": ["template"] }, { "required": ["pattern"] }])
)]
pub struct Options {
    /// A placeholder template the whole name matches, such as `":type/:ticket-:name"`: `:type` is
    /// one of `types`, `:ticket` is [`TICKET`], any other placeholder is `subject`; a placeholder
    /// followed by `-` is optional. The template's other characters are regular-expression source,
    /// as in the reference.
    pub template: Option<String>,
    /// A regular expression the branch name matches, searched anywhere in it unless anchored.
    pub pattern: Option<String>,
    /// The values of `:type`.
    pub types: Vec<String>,
    /// The pattern of the free placeholders: one string for all, or a table by placeholder name.
    pub subject: Subject,
    /// Names refused outright in a run a git hook starts (`gate check --hook`).
    pub prohibited: Vec<String>,
    /// Names that pass every check but `prohibited`.
    pub ignore: Vec<String>,
    /// The shortest name, in UTF-16 code units as the reference counts; 0 for no limit.
    pub min_length: usize,
    /// The longest name, in UTF-16 code units as the reference counts; 0 for no limit.
    pub max_length: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            template: None,
            pattern: None,
            types: ["bugfix", "feature", "hotfix", "release", "support"]
                .map(str::to_owned)
                .to_vec(),
            subject: Subject::default(),
            prohibited: ["main", "master", "release"].map(str::to_owned).to_vec(),
            ignore: vec!["dev".to_owned()],
            min_length: 5,
            max_length: 50,
        }
    }
}

impl Rule for BranchName {
    type Options = Options;

    const NAME: &'static str = "branch-name";
    const TITLE: &'static str =
        "The branch a push updates, or the checked-out one, has an allowed name";
    const SCOPE: Scope = Scope::Repository;
    const READS_HISTORY: bool = true;

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let settings = Settings::read(run)?;
        let options = run.options();
        let policy = Policy::new(options)?;

        let git = run.git()?;
        // Under `--push` the branch is the one the push updates, and the finding points at the
        // pushed tip; a push to a tag or another ref updates no branch. Otherwise it is the
        // checked-out branch at HEAD; a detached HEAD is on no branch.
        let pushed = git
            .range()
            .and_then(|range| range.target_ref.map(|target| (target, range.head)));
        let (name, commit) = match pushed {
            Some((target, head)) => {
                let Some(name) = target.strip_prefix("refs/heads/") else {
                    return Ok(());
                };
                (name.to_owned(), Some(head))
            }
            None => match git.branch() {
                Some(name) => (name, git.head()),
                None => return Ok(()),
            },
        };
        // A repository without commits has nothing a finding could point at.
        let Some(commit) = commit else {
            return Ok(());
        };
        let hook = git.hook();
        // Outside a hook, main passes: a plain or CI run checks main out.
        if !hook && settings.is_main(&name) {
            return Ok(());
        }

        if let Some((message, help)) = policy.verdict(&name, hook) {
            run.report()
                .finding("", Location::commit(commit), message)
                .key(name.as_str())
                .target(name)
                .help(help);
        }
        Ok(())
    }
}

/// The options, checked and compiled.
struct Policy<'o> {
    options: &'o Options,
    /// The whole-name expressions of the template's variants, or the one `pattern`.
    matcher: Matcher,
}

enum Matcher {
    Template(Vec<Regex>),
    Pattern(Regex),
}

impl<'o> Policy<'o> {
    fn new(options: &'o Options) -> Result<Self> {
        let (shortest, longest) = (options.min_length, options.max_length);
        if shortest > 0 && longest > 0 && shortest > longest {
            return Err(Error::config(format!(
                "git/branch-name min-length {shortest} is above max-length {longest}; write the \
                 shortest name's length in min-length and the longest one's in max-length"
            )));
        }
        match &options.subject {
            Subject::All(all) => {
                pattern("subject", all)?;
            }
            Subject::Each(each) => {
                for (name, own) in each {
                    pattern(&format!("subject.{name}"), own)?;
                }
            }
        }
        let matcher = match (&options.template, &options.pattern) {
            (Some(template), None) => {
                Matcher::Template(expressions(template, &options.types, &options.subject)?)
            }
            (None, Some(text)) => Matcher::Pattern(pattern("pattern", text)?),
            _ => {
                return Err(Error::config(
                    "git/branch-name takes exactly one of template and pattern; write a template \
                     such as \":type/:ticket-:name\", or a regular expression in pattern",
                ));
            }
        };
        Ok(Self { options, matcher })
    }

    /// The first check `name` fails, in the reference's order (prohibited, ignore, min length,
    /// max length, pattern), as a message and its help; `None` when it passes.
    fn verdict(&self, name: &str, hook: bool) -> Option<(String, String)> {
        let options = self.options;
        let rename = "rename the branch: git branch -m <new-name>, then push it under the new name";
        if hook
            && options
                .prohibited
                .iter()
                .any(|prohibited| prohibited == name)
        {
            return Some((
                format!(
                    "branch {name} is prohibited; prohibited branch names: {}",
                    options.prohibited.join(", ")
                ),
                format!("work on a branch of its own: git switch -c <new-name>; or {rename}"),
            ));
        }
        if options.ignore.iter().any(|ignored| ignored == name) {
            return None;
        }
        // The reference measures JavaScript's `length`: UTF-16 code units.
        let length = name.encode_utf16().count();
        if options.min_length > 0 && length < options.min_length {
            return Some((
                format!(
                    "branch {name} is {length} characters long, shorter than min-length {}",
                    options.min_length
                ),
                rename.to_owned(),
            ));
        }
        if options.max_length > 0 && length > options.max_length {
            return Some((
                format!(
                    "branch {name} is {length} characters long, longer than max-length {}",
                    options.max_length
                ),
                rename.to_owned(),
            ));
        }
        match &self.matcher {
            Matcher::Template(variants) => {
                if variants.iter().any(|variant| variant.is_match(name)) {
                    return None;
                }
                let template = options.template.as_deref().unwrap_or_default();
                Some((
                    format!(
                        "branch {name} does not match the template {template:?}; valid branch \
                         types: {}",
                        options.types.join(", ")
                    ),
                    rename.to_owned(),
                ))
            }
            Matcher::Pattern(allowed) => (!allowed.is_match(name)).then(|| {
                (
                    format!(
                        "branch {name} does not match the pattern {:?}",
                        allowed.as_str()
                    ),
                    rename.to_owned(),
                )
            }),
        }
    }
}

/// The whole-name expressions of `template`: one per combination of its optional placeholders
/// present or left out, as the reference builds them.
fn expressions(template: &str, types: &[String], subject: &Subject) -> Result<Vec<Regex>> {
    let names = placeholders(template);
    let mut variants = vec![template.to_owned()];
    for name in names.iter().filter(|name| optional(template, name)) {
        variants = variants
            .into_iter()
            .flat_map(|variant| {
                let without = without(&variant, name);
                [variant, without]
            })
            .collect();
    }
    let mut seen = Vec::new();
    for variant in variants.iter().map(|variant| delimiters(variant)) {
        if !variant.is_empty() && !seen.contains(&variant) {
            seen.push(variant);
        }
    }
    if seen.is_empty() {
        return Err(Error::config(format!(
            "git/branch-name template {template:?} has nothing to match; write a template such as \
             \":type/:ticket-:name\""
        )));
    }
    seen.iter()
        .map(|variant| {
            let mut source = variant.clone();
            for name in &names {
                let token = format!(":{name}");
                if source.contains(&token) {
                    let part = placeholder_source(name, types, subject);
                    source = source.replace(&token, &format!("({part})"));
                }
            }
            pattern("template", &format!("^{source}$"))
        })
        .collect()
}

/// The placeholder names of `template` in order of first appearance: `:` and a letter, then
/// letters, digits and inner dashes, in any case.
fn placeholders(template: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let bytes = template.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b':' && bytes.get(at + 1).is_some_and(u8::is_ascii_alphabetic) {
            let start = at + 1;
            let mut end = start + 1;
            while bytes
                .get(end)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
            {
                end += 1;
            }
            // The name ends in a letter or digit: trailing dashes belong to the template.
            while bytes[end - 1] == b'-' {
                end -= 1;
            }
            let name = &template[start..end];
            if !names.iter().any(|known| known == name) {
                names.push(name.to_owned());
            }
            at = end;
        } else {
            at += 1;
        }
    }
    names
}

/// Whether the placeholder `name` is optional: the template holds it followed by `-`.
fn optional(template: &str, name: &str) -> bool {
    template.contains(&format!(":{name}-"))
}

/// `template` without the placeholder `name` and the `-` that follows it.
fn without(template: &str, name: &str) -> String {
    template
        .replace(&format!(":{name}-"), "")
        .replace(&format!(":{name}"), "")
}

/// Runs of `/`, `-`, `_` and `.` collapsed to one, and delimiters at both ends trimmed.
fn delimiters(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    for character in text.chars() {
        let repeated = matches!(character, '/' | '-' | '_' | '.') && collapsed.ends_with(character);
        if !repeated {
            collapsed.push(character);
        }
    }
    collapsed
        .trim_matches(|character| matches!(character, '/' | '-' | '_' | '.'))
        .to_owned()
}

/// The pattern a placeholder matches: `:type` one of `types`, a placeholder `subject` names its
/// own, `:ticket` [`TICKET`], any other the one `subject` or [`DEFAULT_SUBJECT`].
fn placeholder_source(name: &str, types: &[String], subject: &Subject) -> String {
    if name == "type" {
        if types.is_empty() {
            return DEFAULT_SUBJECT.to_owned();
        }
        return types
            .iter()
            .map(|branch_type| regex::escape(branch_type))
            .collect::<Vec<_>>()
            .join("|");
    }
    if let Subject::Each(each) = subject
        && let Some(own) = each.get(name).filter(|own| !own.is_empty())
    {
        return own.clone();
    }
    if name == "ticket" {
        return TICKET.to_owned();
    }
    match subject {
        Subject::All(all) => all.clone(),
        Subject::Each(_) => DEFAULT_SUBJECT.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{delimiters, placeholders, without};

    #[test]
    fn placeholders_follow_the_references_grammar() {
        assert_eq!(
            placeholders(":type/:ticket-:name"),
            ["type", "ticket", "name"]
        );
        assert_eq!(placeholders(":Type/:a-b-:c"), ["Type", "a-b", "c"]);
        assert_eq!(placeholders("release/:version--x"), ["version--x"]);
        assert_eq!(placeholders("plain/:1"), Vec::<String>::new());
    }

    #[test]
    fn delimiters_collapse_and_trim_as_the_reference_does() {
        assert_eq!(
            delimiters(without(":type/:ticket-:name", "ticket").as_str()),
            ":type/:name"
        );
        assert_eq!(delimiters("--a//b__c..d-"), "a/b_c.d");
        assert_eq!(delimiters("-/-"), "");
    }
}
