//! `ci/names`: every workflow, job and step has a name; a workflow's file is its name in kebab
//! case; job ids are kebab case; no two jobs of a workflow, and no two workflows, share a name.

use std::collections::BTreeMap;

use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{Options, Workflow, is_expression, load, steps};
use gate_sdk::yaml::Node;

/// The rule.
#[derive(Debug, Default)]
pub struct Names;

impl Rule for Names {
    type Options = Options;

    const NAME: &'static str = "names";
    const TITLE: &'static str =
        "Every workflow, job and step has a name, and a workflow's file is its name in kebab case";
    const ASPECTS: &'static [&'static str] =
        &["workflow", "file", "job", "step", "duplicate", "job-id"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;
        let mut first_with_name: BTreeMap<&str, &str> = BTreeMap::new();

        for workflow in &workflows {
            match workflow.root.get("name").and_then(Node::scalar) {
                None => {
                    run.report()
                        .finding(
                            "workflow",
                            Location::line(&workflow.path, 1),
                            "the workflow has no name",
                        )
                        .target(workflow.path.clone())
                        .help(
                            "add a top-level `name:`; GitHub otherwise shows the file's path \
                             wherever the workflow is listed",
                        );
                }
                Some(name) if !is_expression(name) => {
                    file_name(run, workflow, name);
                    if let Some(first) = first_with_name.get(name) {
                        let line = workflow.root.get("name").map_or(1, |node| node.line);
                        run.report()
                            .finding(
                                "duplicate",
                                Location::line(&workflow.path, line),
                                format!("the workflow is named `{name}`, as {first} is"),
                            )
                            .key(name)
                            .target(workflow.path.clone())
                            .help("give each workflow a name of its own");
                    } else {
                        first_with_name.insert(name, &workflow.path);
                    }
                }
                Some(_) => {}
            }
            jobs(run, workflow);
        }

        Ok(())
    }
}

/// Reports a workflow whose file stem is not its name in kebab case.
fn file_name(run: &mut Run<'_, Options>, workflow: &Workflow, name: &str) {
    let file = workflow.path.rsplit('/').next().unwrap_or(&workflow.path);
    let stem = file
        .strip_suffix(".yml")
        .or_else(|| file.strip_suffix(".yaml"))
        .unwrap_or(file);
    let expected = kebab(name);
    if stem == expected {
        return;
    }
    let line = workflow.root.get("name").map_or(1, |node| node.line);
    run.report()
        .finding(
            "file",
            Location::line(&workflow.path, line),
            format!("the workflow `{name}` is in `{file}`; its name in kebab case is `{expected}`"),
        )
        .target(workflow.path.clone())
        .help(
            "rename the file to the workflow's name in kebab case, or the workflow to match its \
             file, so one finds the other",
        );
}

/// Checks the jobs and steps of one workflow.
fn jobs(run: &mut Run<'_, Options>, workflow: &Workflow) {
    let mut first_with_name: BTreeMap<&str, &str> = BTreeMap::new();
    for (id_node, job) in workflow.job_entries() {
        let id = id_node.scalar().unwrap_or("");
        let target = format!("{}:{id}", workflow.path);
        if !is_kebab(id) {
            run.report()
                .finding(
                    "job-id",
                    Location::line(&workflow.path, id_node.line),
                    format!("job id `{id}` is not kebab case"),
                )
                .key(id)
                .target(target.clone())
                .help("write job ids in lowercase words joined by single dashes: `build-image`");
        }
        match job.get("name").and_then(Node::scalar) {
            None => {
                run.report()
                    .finding(
                        "job",
                        Location::line(&workflow.path, job.line),
                        format!("job `{id}` has no name"),
                    )
                    .key(id)
                    .target(target.clone())
                    .help(
                        "add `name:`: it is the check name branch protection requires, so keep it \
                         when the job's steps change",
                    );
            }
            Some(name) if !is_expression(name) => {
                if let Some(first) = first_with_name.get(name) {
                    let line = job.get("name").map_or(job.line, |node| node.line);
                    run.report()
                        .finding(
                            "duplicate",
                            Location::line(&workflow.path, line),
                            format!("job `{id}` is named `{name}`, as job `{first}` is"),
                        )
                        .key(id)
                        .target(target.clone())
                        .help(
                            "give each job of the workflow a name of its own: branch protection \
                             cannot tell two checks of one name apart",
                        );
                } else {
                    first_with_name.insert(name, id);
                }
            }
            Some(_) => {}
        }
        for (index, step) in steps(job).iter().enumerate() {
            if step.get("name").and_then(Node::scalar).is_some() {
                continue;
            }
            run.report()
                .finding(
                    "step",
                    Location::line(&workflow.path, step.line),
                    format!("step {} of job `{id}` has no name", index + 1),
                )
                .key(format!("{id}:{}", index + 1))
                .target(target.clone())
                .help("add `name:` saying what the step does; the run's log lists steps by it");
        }
    }
}

/// `name` in kebab case: its runs of ASCII letters and digits, lowercase, joined by `-`.
fn kebab(name: &str) -> String {
    name.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

/// Whether `id` is lowercase words of letters and digits joined by single dashes.
fn is_kebab(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::{is_kebab, kebab};

    #[test]
    fn kebab_case_of_names_and_ids() {
        assert_eq!(kebab("Release And Publish"), "release-and-publish");
        assert_eq!(kebab("CI / lint (fast)"), "ci-lint-fast");
        assert_eq!(kebab("Reproducible"), "reproducible");
        assert!(is_kebab("build-image"));
        assert!(is_kebab("test2"));
        assert!(!is_kebab("build_image"));
        assert!(!is_kebab("Build"));
        assert!(!is_kebab("a--b"));
        assert!(!is_kebab("-a"));
    }
}
