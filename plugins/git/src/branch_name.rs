//! `git/branch-name`: the branch a push updates, or else the checked-out branch, has a name the
//! policy allows.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Result, Rule, Run, Scope};

use crate::settings::{Settings, pattern};

/// The rule.
#[derive(Debug, Default)]
pub struct BranchName;

/// The options of `git/branch-name`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// A regular expression the branch name matches, searched anywhere in it unless anchored.
    pub pattern: String,
    /// The shortest branch name, in characters.
    #[serde(default)]
    pub min_length: Option<usize>,
    /// The longest branch name, in characters.
    #[serde(default)]
    pub max_length: Option<usize>,
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
        let allowed = pattern("pattern", &options.pattern)?;
        let (shortest, longest) = (options.min_length, options.max_length);
        if let (Some(shortest), Some(longest)) = (shortest, longest)
            && shortest > longest
        {
            return Err(Error::new(format!(
                "git/branch-name min-length {shortest} is above max-length {longest}; write the \
                 shortest name's length in min-length and the longest one's in max-length"
            )));
        }

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
        // Main is named by the policy itself.
        if settings.is_main(&name) {
            return Ok(());
        }
        // A repository without commits has nothing a finding could point at.
        let Some(commit) = commit else {
            return Ok(());
        };

        let length = name.chars().count();
        let mut found = Vec::new();
        if !allowed.is_match(&name) {
            found.push(format!(
                "branch {name} does not match the pattern {:?}",
                allowed.as_str()
            ));
        }
        if let Some(shortest) = shortest
            && length < shortest
        {
            found.push(format!(
                "branch {name} is {length} characters long, shorter than min-length {shortest}"
            ));
        }
        if let Some(longest) = longest
            && length > longest
        {
            found.push(format!(
                "branch {name} is {length} characters long, longer than max-length {longest}"
            ));
        }

        for message in found {
            run.report()
                .finding("", Location::commit(commit.clone()), message)
                .target(name.clone())
                .help(
                    "rename the branch to a name the policy allows: git branch -m <new-name>, \
                     then push it under the new name",
                );
        }
        Ok(())
    }
}
