//! `node/package`: the project's `package.json` against the standard of its kind.

use gate_sdk::{NoOptions, Result, Rule, Run};

use crate::json::Node;
use crate::repo;
use crate::settings::{Kind, Settings};
use crate::standard::{FIELDS, MODULE_TYPE, PUBLISHED, Row, difference};

#[derive(Debug, Default)]
pub(crate) struct Package;

const MANIFEST: &str = "package.json";

impl Rule for Package {
    type Options = NoOptions;
    const NAME: &'static str = "package";
    const TITLE: &'static str = "The project's package.json has the fields of its kind";
    const ASPECTS: &'static [&'static str] = &["missing", "forbidden", "value"];

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let kind = Settings::read(run)?.kind;
        let tree = run.tree();
        let Some(manifest) = repo::read_json(&tree, MANIFEST)? else {
            crate::report(
                run.report(),
                "missing",
                (MANIFEST, None),
                MANIFEST,
                format!("the project has no {MANIFEST}; a {} has one", kind.name()),
                "add a package.json with the fields of the project's kind",
            );
            return Ok(());
        };
        for field in FIELDS {
            require(run, &manifest, field, kind, "every project has it");
        }
        if let Some(member) = manifest.member("type")
            && member.value.as_str() != Some(MODULE_TYPE)
        {
            crate::report(
                run.report(),
                "value",
                (MANIFEST, Some(member.line)),
                "type",
                format!(
                    "{MANIFEST} sets type to {}; every kind is \"{MODULE_TYPE}\"",
                    member.value.render()
                ),
                "write \"type\": \"module\" and use the .cjs extension for a file that must stay CommonJS",
            );
        }
        if manifest
            .get("engines")
            .and_then(|engines| engines.get("node"))
            .is_none()
        {
            require(
                run,
                &manifest,
                "engines.node",
                kind,
                "every project declares the Node it runs on",
            );
        }
        published(run, &manifest, kind);
        if let Some(member) = manifest.member("workspaces") {
            forbid(
                run,
                member.line,
                "workspaces",
                "pnpm-workspace.yaml lists the workspace's packages; package.json does not",
            );
        }
        Ok(())
    }
}

/// `private` and the fields only a published package has.
fn published(run: &mut Run<'_, NoOptions>, manifest: &Node, kind: Kind) {
    let private = manifest.member("private");
    let is_private = private.and_then(|member| member.value.as_bool()) == Some(true);
    if let Some(difference) = difference(kind, Row::Private) {
        if is_private && let Some(member) = private {
            forbid(run, member.line, "private", difference.reason);
        }
        require(run, manifest, "publishConfig", kind, difference.reason);
    } else if !is_private {
        if let Some(member) = private {
            crate::report(
                run.report(),
                "value",
                (MANIFEST, Some(member.line)),
                "private",
                format!(
                    "{MANIFEST} sets private to {}; a {} is private",
                    member.value.render(),
                    kind.name()
                ),
                "write \"private\": true so the package is never published by mistake",
            );
        } else {
            require(
                run,
                manifest,
                "private",
                kind,
                "a project that is not published is private",
            );
        }
    }
    let library = difference(kind, Row::Published);
    for field in PUBLISHED {
        match (library, manifest.member(field)) {
            (Some(difference), None) => require(run, manifest, field, kind, difference.reason),
            (None, Some(member)) => forbid(
                run,
                member.line,
                field,
                "only a library is published; the field describes a package for consumers",
            ),
            _ => {}
        }
    }
}

/// `missing`: `field` (a dotted path for a nested one) is absent.
fn require(run: &mut Run<'_, NoOptions>, manifest: &Node, field: &str, kind: Kind, why: &str) {
    let top = field.split('.').next().unwrap_or(field);
    if field == top && manifest.member(field).is_some() {
        return;
    }
    crate::report(
        run.report(),
        "missing",
        (MANIFEST, None),
        field,
        format!("{MANIFEST} has no {field}; a {} has it: {why}", kind.name()),
        "add the field",
    );
}

/// `forbidden`: `field` is present where the kind has none.
fn forbid(run: &mut Run<'_, NoOptions>, line: usize, field: &str, why: &str) {
    crate::report(
        run.report(),
        "forbidden",
        (MANIFEST, Some(line)),
        field,
        format!("{MANIFEST} has {field}, which this kind has not: {why}"),
        "remove the field",
    );
}
