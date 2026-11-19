//! `agents/no-claude-md`: no `CLAUDE.md` is tracked anywhere in the repository. A repository-scope
//! rule, so a root file is seen whichever projects the workspace lists.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Result, Rule, Run, Scope};

/// The rule.
#[derive(Debug, Default)]
pub struct NoClaudeMd;

/// The options of `agents/no-claude-md`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The file name that may not be tracked, compared without regard to case.
    pub file_name: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            file_name: "CLAUDE.md".to_owned(),
        }
    }
}

impl Rule for NoClaudeMd {
    type Options = Options;

    const NAME: &'static str = "no-claude-md";
    const TITLE: &'static str = "No CLAUDE.md is tracked anywhere in the repository";
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        let name = run.options().file_name.clone();
        for path in run.tree().paths() {
            let file = path.rsplit('/').next().unwrap_or(&path);
            if !file.eq_ignore_ascii_case(&name) {
                continue;
            }
            run.report()
                .finding(
                    "",
                    Location::file(&path),
                    "a tracked CLAUDE.md disables AGENTS.md; use AGENTS.md and docs/agents instead",
                )
                .target(path.clone())
                .help("move its instructions into AGENTS.md or a rule file, and delete it");
        }
        Ok(())
    }
}
