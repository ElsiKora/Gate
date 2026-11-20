//! `frontend/client-boundary`: in Next.js, `"use client"` marks components of `ui` and `model`
//! segments and the app layer's own modules, never a route file.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::{Project, has_directive};
use crate::options::{PACK, Pack};
use crate::rules::route_files::next_route_file;
use crate::rules::{Found, report};
use crate::standard::{folder_of, place};

/// The rule.
#[derive(Debug, Default)]
pub struct ClientBoundary;

/// The directive that makes a module a client component.
const USE_CLIENT: &str = "use client";

impl Rule<TypeScriptCall> for ClientBoundary {
    type Options = NoOptions;

    const NAME: &'static str = "client-boundary";
    const TITLE: &'static str =
        "Route files stay server components; \"use client\" marks ui and model modules";
    const ASPECTS: &'static [&'static str] = &["route-file", "placement"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project)?;
            report(run, found);
            Ok(())
        })
    }
}

fn check(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();
    if project.pack != Pack::Nextjs {
        return Ok(found);
    }

    for path in project.code() {
        let Some(file) = project.parse(path)? else {
            continue;
        };
        if !has_directive(&file, USE_CLIENT) {
            continue;
        }

        if next_route_file(path) {
            found.push(Found::at(
                "route-file",
                path,
                format!("{path} is a route file marked \"use client\"; route files stay server components"),
                "move the interactive part into a ui segment marked \"use client\" and render it from the route file",
            ));
            continue;
        }

        let placed = folder_of(path) == "src/app"
            || place(Pack::Nextjs, path)
                .is_some_and(|place| matches!(place.segment, Some("ui" | "model")));
        if !placed {
            found.push(Found::at(
                "placement",
                path,
                format!("{path} is marked \"use client\" outside a ui or model segment and src/app/"),
                "keep the client boundary at a component of a ui segment; helpers and hooks need no directive",
            ));
        }
    }

    Ok(found)
}
