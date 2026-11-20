//! `node/scripts`: one name per job. A bare name checks; `<name>:fix` writes.

use gate_sdk::{NoOptions, Result, Rule, Run};

use crate::json::Member;
use crate::repo::{self, file_name};
use crate::settings::Settings;
use crate::standard::{FORMAT_REASON, FORMAT_SCRIPTS, Row, SCRIPTS, SYNONYMS, difference};

#[derive(Debug, Default)]
pub(crate) struct Scripts;

const MANIFEST: &str = "package.json";

/// npm lifecycle names that are not kebab-case and keep their name.
const LIFECYCLE: [&str; 1] = ["prepublishOnly"];

impl Rule for Scripts {
    type Options = NoOptions;
    const NAME: &'static str = "scripts";
    const TITLE: &'static str = "One script name per job: a bare name checks, <name>:fix writes";
    const ASPECTS: &'static [&'static str] = &["missing", "synonym", "writes", "misplaced", "name"];

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let kind = Settings::read(run)?.kind;
        let tree = run.tree();
        let Some(manifest) = repo::read_json(&tree, MANIFEST)? else {
            return Ok(());
        };
        let paths = tree.paths();
        let member = !paths.iter().any(|path| path == "pnpm-lock.yaml");
        let typescript = paths.iter().any(|path| {
            let name = file_name(path);
            name.starts_with("tsconfig") && name.ends_with(".json")
        });
        let section = manifest.member("scripts");
        let scripts = section
            .map(|section| section.value.members())
            .unwrap_or_default();
        for script in scripts {
            named(run, script, member);
        }
        let mut required: Vec<&str> = SCRIPTS
            .iter()
            .copied()
            .filter(|script| typescript || *script != "typecheck")
            .collect();
        if !member {
            required.extend(FORMAT_SCRIPTS);
        }
        let dev = difference(kind, Row::Dev);
        if dev.is_some() {
            required.push("dev");
        }
        for script in required {
            if scripts.iter().any(|present| present.key == script) {
                continue;
            }
            let why = match (script, dev) {
                ("dev", Some(difference)) => difference.reason,
                ("typecheck", _) => "the project configures TypeScript",
                _ => "every project has it",
            };
            crate::report(
                run.report(),
                "missing",
                (MANIFEST, section.map(|section| section.line)),
                script,
                format!(
                    "{MANIFEST} has no script {script}; a {} has it: {why}",
                    kind.name()
                ),
                "add the script under its standard name",
            );
        }
        Ok(())
    }
}

/// `synonym`, `writes`, `misplaced` and `name`: one script's name and command.
fn named(run: &mut Run<'_, NoOptions>, script: &Member, member: bool) {
    let name = script.key.as_str();
    let at = (MANIFEST, Some(script.line));
    if let Some((_, standard)) = SYNONYMS.iter().find(|(synonym, _)| *synonym == name) {
        crate::report(
            run.report(),
            "synonym",
            at,
            name,
            format!("script {name} does the job of the standard script {standard}"),
            "rename it to the standard name; every project calls a job by one name",
        );
    }
    let command = script.value.as_str().unwrap_or_default();
    if !name.ends_with(":fix") && writes(command) {
        crate::report(
            run.report(),
            "writes",
            at,
            name,
            format!(
                "script {name} rewrites files ({command}), and only a <name>:fix script writes"
            ),
            "make it check only, and put the writing command in <name>:fix",
        );
    }
    if member && FORMAT_SCRIPTS.contains(&name) {
        crate::report(
            run.report(),
            "misplaced",
            at,
            name,
            format!("script {name} is in a workspace member: {FORMAT_REASON}"),
            "remove it; run format and format:fix at the repository root",
        );
    }
    if !LIFECYCLE.contains(&name) && !kebab(name) {
        crate::report(
            run.report(),
            "name",
            at,
            name,
            format!("script {name} is not lower-case kebab-case segments joined by \":\""),
            "rename it, such as test:e2e or build-storybook",
        );
    }
}

/// Whether `command` passes `--fix` or `--write`, the flags that make a tool rewrite files.
fn writes(command: &str) -> bool {
    command.split_whitespace().any(|word| {
        let flag = word.split_once('=').map_or(word, |(flag, _)| flag);
        matches!(flag, "--fix" | "--write")
    })
}

/// Whether `name` is lower-case kebab-case segments joined by `:`.
fn kebab(name: &str) -> bool {
    name.split(':').all(|segment| {
        !segment.is_empty()
            && segment.split('-').all(|word| {
                !word.is_empty()
                    && word.chars().all(|character| {
                        character.is_ascii_lowercase() || character.is_ascii_digit()
                    })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::{kebab, writes};

    #[test]
    fn names_and_writing_flags() {
        for name in [
            "build",
            "test:e2e",
            "build-storybook",
            "start:api",
            "lint:fix",
            "e2e",
        ] {
            assert!(kebab(name), "{name}");
        }
        for name in ["lintFix", "test::x", "Build", "a-", "lint:ts_x"] {
            assert!(!kebab(name), "{name}");
        }
        assert!(writes("eslint --fix ."));
        assert!(writes("prettier --write=true ."));
        assert!(!writes("prettier --check ."));
        assert!(!writes("eslint --fix-type problem"));
    }
}
