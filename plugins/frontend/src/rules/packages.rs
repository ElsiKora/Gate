//! `frontend/packages`: one tool per job in the `dependencies` of `package.json` (HTTP client,
//! store, router, translation, styling), and the router and translation library the pack names.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::Value;
use gate_sdk::{Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::config::{PACKAGE_JSON, json};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};

/// The rule.
#[derive(Debug, Default)]
pub struct Packages;

/// A job's tools, each a name and the packages that make it up.
pub type Tools = BTreeMap<String, Vec<String>>;

/// The options of `frontend/packages`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// Each job with its tools; a tool is one or more packages. A project depends on at most one
    /// tool of each job. The jobs `router` and `translation` also hold the tools the packs name.
    pub jobs: BTreeMap<String, Tools>,
}

/// The job of the router and the job of the translation library, which the pack decides.
const ROUTER: &str = "router";
const TRANSLATION: &str = "translation";

/// A default tool: its name and its packages.
type Tool = (&'static str, &'static [&'static str]);

/// A default job: its name and its tools.
type Job = (&'static str, &'static [Tool]);

/// The packages another package requires beside it, which count as part of it and not as a tool of
/// their own: a component library's styling engine.
const COMPANIONS: &[(&str, &[&str])] = &[
    ("@mui/material", &["@emotion/react", "@emotion/styled"]),
    ("@mui/joy", &["@emotion/react", "@emotion/styled"]),
    ("@chakra-ui/react", &["@emotion/react", "@emotion/styled"]),
];

/// The default jobs.
const JOBS: &[Job] = &[
    (
        "http-client",
        &[
            ("axios", &["axios"]),
            ("ky", &["ky"]),
            ("ofetch", &["ofetch"]),
            ("cross-fetch", &["cross-fetch"]),
            ("node-fetch", &["node-fetch"]),
            ("isomorphic-fetch", &["isomorphic-fetch"]),
            ("undici", &["undici"]),
            ("graphql-request", &["graphql-request"]),
        ],
    ),
    (
        "store",
        &[
            ("zustand", &["zustand"]),
            ("redux", &["@reduxjs/toolkit", "redux", "react-redux"]),
            ("recoil", &["recoil"]),
            ("jotai", &["jotai"]),
            ("mobx", &["mobx", "mobx-react", "mobx-react-lite"]),
            ("valtio", &["valtio"]),
        ],
    ),
    (
        ROUTER,
        &[
            ("tanstack-router", &["@tanstack/react-router"]),
            ("react-router", &["react-router", "react-router-dom"]),
            ("wouter", &["wouter"]),
            ("next", &["next"]),
        ],
    ),
    (
        TRANSLATION,
        &[
            ("react-i18next", &["react-i18next", "i18next"]),
            ("next-intl", &["next-intl"]),
            ("react-intl", &["react-intl"]),
            ("lingui", &["@lingui/react", "@lingui/core"]),
            ("typesafe-i18n", &["typesafe-i18n"]),
        ],
    ),
    (
        "styling",
        &[
            (
                "tailwind",
                &["tailwindcss", "@tailwindcss/vite", "@tailwindcss/postcss"],
            ),
            ("styled-components", &["styled-components"]),
            ("emotion", &["@emotion/react", "@emotion/styled"]),
            ("sass", &["sass", "sass-embedded"]),
            ("less", &["less"]),
            (
                "vanilla-extract",
                &["@vanilla-extract/css", "@vanilla-extract/vite-plugin"],
            ),
            ("stitches", &["@stitches/react"]),
        ],
    ),
];

impl Default for Options {
    fn default() -> Self {
        Self {
            jobs: JOBS
                .iter()
                .map(|(job, tools)| {
                    let tools = tools
                        .iter()
                        .map(|(tool, packages)| {
                            (
                                (*tool).to_owned(),
                                packages
                                    .iter()
                                    .map(|package| (*package).to_owned())
                                    .collect(),
                            )
                        })
                        .collect();
                    ((*job).to_owned(), tools)
                })
                .collect(),
        }
    }
}

impl Rule<TypeScriptCall> for Packages {
    type Options = Options;

    const NAME: &'static str = "packages";
    const TITLE: &'static str =
        "package.json names one tool per job, and the router and translation library of the pack";
    const ASPECTS: &'static [&'static str] = &["job", "pack"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project, &run.options().jobs)?;
            report(run, found);
            Ok(())
        })
    }
}

/// The tools of `job` the pack allows, when it decides the job: an empty list for a library's
/// router, since a library uses none; `None` for a job the pack leaves to the project.
fn pack_tools(pack: Pack, job: &str) -> Option<&'static [&'static str]> {
    match (job, pack) {
        (ROUTER, Pack::ReactVite) => Some(&["tanstack-router"]),
        (ROUTER, Pack::Nextjs) => Some(&["next"]),
        (ROUTER, Pack::ReactLibrary) => Some(&[]),
        (TRANSLATION, Pack::Nextjs) => Some(&["next-intl"]),
        (TRANSLATION, Pack::ReactVite | Pack::ReactLibrary) => Some(&["react-i18next"]),
        _ => None,
    }
}

fn check(
    project: &Project<'_, '_>,
    jobs: &BTreeMap<String, Tools>,
) -> gate_sdk::Result<Vec<Found>> {
    let Some(manifest) = json(project, PACKAGE_JSON)? else {
        return Ok(Vec::new());
    };
    let sections: &[&str] = if project.pack.library() {
        &["dependencies", "peerDependencies"]
    } else {
        &["dependencies"]
    };
    let mut declared: BTreeSet<&str> = sections
        .iter()
        .filter_map(|section| manifest.get(section).and_then(Value::as_object))
        .flat_map(|section| section.keys().map(String::as_str))
        .collect();
    for (package, companions) in COMPANIONS {
        if declared.contains(package) {
            declared.retain(|name| !companions.contains(name));
        }
    }
    let mut found = Vec::new();

    for (job, tools) in jobs {
        let used: Vec<&str> = tools
            .iter()
            .filter(|(_, packages)| {
                packages
                    .iter()
                    .any(|package| declared.contains(package.as_str()))
            })
            .map(|(tool, _)| tool.as_str())
            .collect();

        if used.len() > 1 {
            found.push(
                Found::at(
                    "job",
                    PACKAGE_JSON,
                    format!(
                        "{PACKAGE_JSON} names {} tools for the job {job}: {}",
                        used.len(),
                        used.join(", ")
                    ),
                    format!("keep one tool for {job} and move its users to it"),
                )
                .named(job),
            );
        }

        let Some(allowed) = pack_tools(project.pack, job) else {
            continue;
        };
        for tool in used.iter().filter(|tool| !allowed.contains(*tool)) {
            let wanted = if allowed.is_empty() {
                format!("the {} pack uses no {job}", project.pack.name())
            } else {
                format!(
                    "the {} pack uses {}",
                    project.pack.name(),
                    allowed.join(" or ")
                )
            };
            found.push(
                Found::at(
                    "pack",
                    PACKAGE_JSON,
                    format!("{PACKAGE_JSON} names {tool} for the job {job}; {wanted}"),
                    format!("move to the {job} of the pack and remove {tool}"),
                )
                .named(tool),
            );
        }
    }

    Ok(found)
}
