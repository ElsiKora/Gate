//! `git/no-ci-skip`: no commit since activation carries a CI-skip directive, except a commit of a
//! release form of `[plugins.git] options forms`.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Need, Rule, Run, Scope};

use crate::commit_message::above_scissors;
use crate::forms::Kind;
use crate::history::{History, excepted_commits, release_version};
use crate::settings::{self, Settings, pattern};

/// The rule.
#[derive(Debug, Default)]
pub struct NoCiSkip;

/// The options of `git/no-ci-skip`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// A regular expression of the skip directives GitHub Actions honours in a message, read as
    /// JavaScript reads one without flags.
    pub directive_pattern: String,
    /// A regular expression of the skip trailer, read as JavaScript reads one without flags.
    pub trailer_pattern: String,
}

impl Default for Options {
    /// The directives and the trailer in any case, the trailer on a line of its own.
    fn default() -> Self {
        Self {
            directive_pattern: format!(
                r"\[(?:{})\]",
                caseless("skip ci|ci skip|no ci|skip actions|actions skip")
            ),
            trailer_pattern: format!(
                r"(?:^|\n){}:\s*{}\s*(?:\n|$)",
                caseless("skip-checks"),
                caseless("true")
            ),
        }
    }
}

/// `text` with each ASCII letter as the class of its two cases, so that a pattern read without
/// flags matches it in either case.
fn caseless(text: &str) -> String {
    let mut spelled = String::new();
    for character in text.chars() {
        if character.is_ascii_alphabetic() {
            spelled.push('[');
            spelled.push(character.to_ascii_lowercase());
            spelled.push(character.to_ascii_uppercase());
            spelled.push(']');
        } else {
            spelled.push(character);
        }
    }
    spelled
}

impl Rule for NoCiSkip {
    type Options = Options;

    const NAME: &'static str = "no-ci-skip";
    const TITLE: &'static str =
        "No commit since activation carries a CI-skip directive except the release commit";
    const SCOPE: Scope = Scope::Repository;
    const READS_HISTORY: bool = true;
    const READS_MESSAGE: bool = true;
    const NEEDS: &'static [Need] = &[settings::ACTIVATION, settings::RELEASE_SUBJECT];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let release = settings.release()?;
        let forms = settings.forms(release.as_ref())?;
        let directive = pattern("directive-pattern", &run.options().directive_pattern)?;
        let trailer = pattern("trailer-pattern", &run.options().trailer_pattern)?;
        // The run judges the message being written, its whole history, and each commit an
        // exception names: one outside that history has no finding.
        let mut judged = Vec::new();
        // The message being written (the commit-msg hook), which no history holds yet.
        if let Some(message) = run.message()? {
            let message = above_scissors(&message);
            let header = message.lines().next().unwrap_or_default().to_owned();
            judged.push(header.clone());
            let release_form = forms.iter().any(|form| form.kind == Kind::Release);
            let exempt = release_form
                && release
                    .as_ref()
                    .is_some_and(|release| release_version(release, &message).is_some());
            if !exempt
                && let Some(found) = directive.find(&message).or_else(|| trailer.find(&message))
            {
                let token = found.as_str().trim().to_owned();
                run.report()
                    .finding(
                        "",
                        None::<Location>,
                        format!(
                            "the commit message carries the forbidden CI-skip directive {token}"
                        ),
                    )
                    .target(header)
                    .help(
                        "reword the message without the directive; a skipped run leaves main \
                         unchecked",
                    );
            }
        }
        let git = run.git()?;
        let history = History::read(&git, &settings, "git/no-ci-skip")?;
        judged.extend(history.commits.iter().map(|commit| commit.id.clone()));
        // An excepted commit outside the history is re-judged with no finding.
        let excepted: Vec<String> = excepted_commits(&git, "git/no-ci-skip", run.exceptions())?
            .into_iter()
            .map(str::to_owned)
            .collect();

        for commit in &history.commits {
            if history.is_release(commit, &forms, release.as_ref()) {
                continue;
            }
            let Some(found) = directive
                .find(&commit.message)
                .or_else(|| trailer.find(&commit.message))
            else {
                continue;
            };
            let token = found.as_str().trim();
            run.report()
                .finding(
                    "",
                    commit.location(),
                    format!(
                        "{} carries the forbidden CI-skip directive {token}",
                        commit.short()
                    ),
                )
                .target(commit.id.clone())
                .help(
                    "reword the commit without the directive (an unpushed commit: git commit \
                     --amend or git rebase -i); a skipped run leaves main unchecked",
                );
        }

        run.judges_only(judged);
        run.rejudges(excepted);
        Ok(())
    }
}
