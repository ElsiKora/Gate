//! `ci/permissions`: a workflow's own `permissions` grant no write scope, every job sets its own,
//! and no `permissions` is the `read-all` or `write-all` shorthand.

use gate_sdk::{Location, Rule, Run, Scope};

use crate::workflows::{Options, Workflow, load};
use gate_sdk::yaml::Node;

/// The rule.
#[derive(Debug, Default)]
pub struct Permissions;

impl Rule for Permissions {
    type Options = Options;

    const NAME: &'static str = "permissions";
    const TITLE: &'static str =
        "Every job sets its own token permissions, and a workflow's own permissions only read";
    const ASPECTS: &'static [&'static str] = &["workflow-write", "job-missing", "shorthand"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let workflows = load(&run.tree(), &run.options().workflows)?;

        for workflow in &workflows {
            if let Some(permissions) = workflow.root.get("permissions") {
                shorthand(run, workflow, permissions, None);
                for (scope, access) in permissions.mapping().unwrap_or_default() {
                    if access.scalar() != Some("write") {
                        continue;
                    }
                    let scope = scope.scalar().unwrap_or("");
                    run.report()
                        .finding(
                            "workflow-write",
                            Location::line(&workflow.path, access.line),
                            format!(
                                "the workflow's own permissions grant `{scope}: write` to every job"
                            ),
                        )
                        .key(scope)
                        .target(workflow.path.clone())
                        .help(
                            "set the workflow's permissions to {} or read scopes, and grant the \
                             write scope in the permissions of the one job that needs it",
                        );
                }
            }
            for (id, job) in workflow.job_entries() {
                let id = id.scalar().unwrap_or("");
                match job.get("permissions") {
                    Some(permissions) => shorthand(run, workflow, permissions, Some(id)),
                    None => {
                        run.report()
                            .finding(
                                "job-missing",
                                Location::line(&workflow.path, job.line),
                                format!("job `{id}` sets no permissions of its own"),
                            )
                            .key(id)
                            .target(format!("{}:{id}", workflow.path))
                            .help(
                                "give the job `permissions:` with the scopes it uses, \
                                 `contents: read` for a job that only reads the repository, or \
                                 {} for one that reads nothing",
                            );
                    }
                }
            }
        }

        Ok(())
    }
}

/// Reports `permissions` when it is the `read-all` or `write-all` shorthand; `job` is the job's id,
/// or none for the workflow's own permissions.
fn shorthand(
    run: &mut Run<'_, Options>,
    workflow: &Workflow,
    permissions: &Node,
    job: Option<&str>,
) {
    let Some(value @ ("read-all" | "write-all")) = permissions.scalar() else {
        return;
    };
    let (whose, target, key) = match job {
        Some(id) => (
            format!("job `{id}`"),
            format!("{}:{id}", workflow.path),
            id.to_owned(),
        ),
        None => (
            "the workflow".to_owned(),
            workflow.path.clone(),
            "workflow".to_owned(),
        ),
    };
    run.report()
        .finding(
            "shorthand",
            Location::line(&workflow.path, permissions.line),
            format!("{whose} grants `{value}`, every scope at once"),
        )
        .key(key)
        .target(target)
        .help("list the scopes the job uses, each with read or write, instead of a shorthand");
}
