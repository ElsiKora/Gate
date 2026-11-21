//! The arguments of `gate check`, defined once: the command line parses them, and the
//! configuration checks every `check` entry of `[hooks]` and of a CI job with the same definition
//! when it loads, so an entry the command would refuse fails there, not in the hook or the job.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, ValueEnum};

use crate::config::RuleKey;
use crate::{PluginOverride, RuleFilter};

/// The arguments of `gate check`.
#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is one command-line flag"
)]
pub struct CheckArgs {
    /// Check the index, and only the projects the staged change touches.
    #[arg(long, conflicts_with_all = ["rev", "push"])]
    pub staged: bool,
    /// Check the tree of a commit.
    #[arg(long, value_name = "REV", conflicts_with = "push")]
    pub rev: Option<String>,
    /// Check only the projects changed since --base (and those that depend on them).
    #[arg(long, requires = "base", conflicts_with = "push")]
    pub changed: bool,
    /// The revision --changed compares with.
    #[arg(long, value_name = "REV", requires = "changed")]
    pub base: Option<String>,
    /// Check a pushed range <base>..<tip> (a base of zeros is a new branch).
    #[arg(long, value_name = "RANGE")]
    pub push: Option<String>,
    /// The remote ref the --push range goes to (the pre-push hook and CI pass it).
    #[arg(long, value_name = "REF", requires = "push", value_parser = parse_target_ref)]
    pub target_ref: Option<String>,
    /// The output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
    /// Replace one pinned plugin with a local build for this run: <name>=<path.wasm>. Repeatable.
    #[arg(long = "plugin", value_name = "NAME=PATH", value_parser = PluginOverride::parse)]
    pub plugins: Vec<PluginOverride>,
    /// Refuse every --plugin override (the generated CI jobs pass it).
    #[arg(long)]
    pub no_overrides: bool,
    /// Print only the projects the change selects (text, json or github-output); no rule runs.
    #[arg(long)]
    pub list_changed_projects: bool,
    /// The commit message file under review (the commit-msg hook passes "$1").
    #[arg(long, value_name = "FILE")]
    pub commit_message: Option<PathBuf>,
    /// A git hook starts this run; the generated hooks pass it, so rules that apply only while a
    /// developer works (a prohibited branch name) know they do.
    #[arg(long)]
    pub hook: bool,
    /// Run only this rule, <plugin>/<rule>, or every rule of a plugin, <plugin>/*. Repeatable.
    #[arg(long = "rule", value_name = "ID", value_parser = RuleFilter::parse)]
    pub rules: Vec<RuleKey>,
    #[command(flatten)]
    pub run: RunArgs,
}

impl CheckArgs {
    /// What the arguments ask that clap cannot refuse on its own: `--target-ref` without `--push`,
    /// `--commit-message` beside a flag that picks a tree, or `--fail-after` not above
    /// `--warn-after`. `None` when they hold together.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        target_ref_without_push(self.target_ref.as_deref(), self.push.as_deref())
            .or_else(|| self.message_beside_a_tree())
            .or_else(|| {
                let (Some(warn), Some(fail)) = (self.run.warn_after, self.run.fail_after) else {
                    return None;
                };
                (fail <= warn).then(|| {
                    format!(
                        "--fail-after ({} s) is not above --warn-after ({} s); a run warns first and fails later, so raise --fail-after or lower --warn-after",
                        fail.as_secs_f64(),
                        warn.as_secs_f64()
                    )
                })
            })
    }

    /// `--commit-message` checks only the message being written; a flag that picks the tree to
    /// check beside it is refused, since the tree is the other hooks' and CI's to check.
    fn message_beside_a_tree(&self) -> Option<String> {
        self.commit_message.as_ref()?;
        [
            (self.staged, "--staged"),
            (self.changed, "--changed"),
            (self.base.is_some(), "--base"),
            (self.push.is_some(), "--push"),
            (self.rev.is_some(), "--rev"),
        ]
        .into_iter()
        .find_map(|(given, flag)| given.then_some(flag))
        .map(|flag| {
            format!(
                "--commit-message checks only the message being written, so it takes no {flag}: the tree is checked by the pre-commit hook (check --staged), the pre-push hook (check --push) and CI; run those checks apart"
            )
        })
    }
}

/// Options passed through to the run.
#[derive(Debug, Args)]
pub struct RunArgs {
    /// Print the time of each phase and plugin call to standard error.
    #[arg(long)]
    pub timings: bool,
    /// Report a run that takes longer than this many seconds, not counting compiling plugins, as a
    /// gate/budget warning.
    #[arg(long, value_name = "SECONDS", value_parser = parse_seconds)]
    pub warn_after: Option<Duration>,
    /// Report a run that takes longer than this many seconds, not counting compiling plugins, as a
    /// gate/budget error; above --warn-after when both are given.
    #[arg(long, value_name = "SECONDS", value_parser = parse_seconds)]
    pub fail_after: Option<Duration>,
    /// Stop at the first error finding.
    #[arg(long)]
    pub fail_fast: bool,
}

/// The format of `gate check`'s report.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    /// Findings as text.
    Text,
    /// Findings as JSON.
    Json,
    /// Findings as SARIF.
    Sarif,
    /// Findings as GitHub workflow annotations.
    Github,
    /// `name=value` lines for `$GITHUB_OUTPUT` (with --list-changed-projects only).
    GithubOutput,
}

/// A `--target-ref` value: a full ref, the form the pre-push hook and the generated CI pass.
///
/// # Errors
/// Fails, saying what is expected, on a value that is not a full ref.
pub fn parse_target_ref(value: &str) -> Result<String, String> {
    match value.strip_prefix("refs/") {
        Some(name) if !name.is_empty() && !name.ends_with('/') => Ok(value.to_owned()),
        _ => Err(format!(
            "{value:?} is not a full ref; pass the full remote ref, such as refs/heads/main"
        )),
    }
}

/// Refuses `--target-ref` without `--push`. clap drops `requires = "push"` when a present
/// argument conflicts with --push (--rev and --staged do), so it is checked here as well. `None`
/// when the two hold together.
#[must_use]
pub fn target_ref_without_push(target_ref: Option<&str>, push: Option<&str>) -> Option<String> {
    (target_ref.is_some() && push.is_none()).then(|| {
        "--target-ref names the remote ref a --push range updates; pass it with --push".to_owned()
    })
}

/// A number of seconds above 0 that a [`Duration`] holds.
fn parse_seconds(value: &str) -> Result<Duration, String> {
    let seconds: f64 = value
        .parse()
        .map_err(|_| format!("{value:?} is not a number of seconds"))?;
    if !(seconds.is_finite() && seconds > 0.0) {
        return Err(format!("{value:?}: give a number of seconds above 0"));
    }
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| format!("{value:?}: too many seconds; give a number a run can last"))
}

/// `gate check` alone, to parse one entry's arguments.
#[derive(Debug, Parser)]
#[command(name = "check", disable_help_flag = true, disable_version_flag = true)]
struct Entry {
    #[command(flatten)]
    arguments: CheckArgs,
}

/// Checks the arguments of a `check` entry (`arguments`, the words after `check`) with the
/// definition `gate check` parses, as the hook or the job will run it: `--push` and
/// `--commit-message`, which an entry writes without a value, get the kind of value the hook or the
/// job passes.
///
/// # Errors
/// What `gate check` would say about the arguments: an unknown option, a value it refuses, options
/// that do not go together, or thresholds out of order.
pub fn check_entry(arguments: &[&str]) -> Result<(), String> {
    let mut words = vec!["check".to_owned()];
    for word in arguments {
        words.push((*word).to_owned());
        match *word {
            "--push" => words.extend([
                "0000000000000000000000000000000000000000..HEAD".to_owned(),
                "--target-ref".to_owned(),
                "refs/heads/main".to_owned(),
            ]),
            "--commit-message" => words.push(".git/COMMIT_EDITMSG".to_owned()),
            _ => {}
        }
    }
    let entry = Entry::try_parse_from(words).map_err(|error| {
        let rendered = error.render().to_string();
        let first = rendered.lines().next().unwrap_or_default();
        first.strip_prefix("error: ").unwrap_or(first).to_owned()
    })?;
    entry.arguments.refusal().map_or(Ok(()), Err)
}
