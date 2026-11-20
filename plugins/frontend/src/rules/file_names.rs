//! `frontend/file-names`: every file and folder name under `src/` is kebab-case, its only dotted
//! parts are `test`, `stories`, `module` and `d`, and a route folder takes its router's syntax.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{DOTTED_PARTS, FileName, VITE_ROUTE_TREE, camel, kebab, within};

/// The rule.
#[derive(Debug, Default)]
pub struct FileNames;

impl Rule<TypeScriptCall> for FileNames {
    type Options = NoOptions;

    const NAME: &'static str = "file-names";
    const TITLE: &'static str =
        "Files and folders under src/ are kebab-case, with route folders in their router's syntax";
    const ASPECTS: &'static [&'static str] = &["file", "folder", "suffix"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project);
            report(run, found);
            Ok(())
        })
    }
}

fn check(project: &Project<'_>) -> Vec<Found> {
    let pack = project.pack;
    let route_folder = pack.route_folder();
    let mut folders = BTreeSet::new();
    let mut found = Vec::new();

    for path in project.sources() {
        let segments: Vec<&str> = path.split('/').collect();
        let (name, folder_names) = segments.split_last().unwrap_or((&"", &[]));

        for depth in 1..folder_names.len() {
            let folder = segments[..=depth].join("/");
            if !folders.insert(folder.clone()) {
                continue;
            }
            let routed = route_folder
                .is_some_and(|routes| within(&folder, routes) && folder.len() > routes.len());
            if !folder_name(pack, folder_names[depth], routed) {
                found.push(Found::at(
                    "folder",
                    &folder,
                    format!("folder {folder}/ is not {}", form(pack, routed)),
                    "rename the folder",
                ));
            }
        }

        if name.starts_with('.') || (path == VITE_ROUTE_TREE && pack == Pack::ReactVite) {
            continue;
        }

        let routed = route_folder.is_some_and(|routes| within(path, routes));
        let file = FileName::new(name);

        if !file_stem(pack, file.stem, routed) {
            found.push(Found::at(
                "file",
                path,
                format!("file {path} is not {}", form(pack, routed)),
                "rename the file in kebab-case; a component file is named after its component, user-card.tsx for UserCard",
            ));
        }

        for part in &file.dotted {
            let allowed = DOTTED_PARTS.contains(part)
                || (routed && pack == Pack::ReactVite && *part == "lazy");
            if !allowed {
                found.push(
                    Found::at(
                        "suffix",
                        path,
                        format!(
                            "file {path} carries the part .{part}; the only dotted parts are .{}",
                            DOTTED_PARTS.join(", .")
                        ),
                        "let the segment say the kind: name.helper.ts is lib/name.ts, name.service.ts is api/name.ts",
                    )
                    .named(part),
                );
            }
        }
    }

    found
}

/// What a name in that place must be, for a message.
fn form(pack: Pack, routed: bool) -> &'static str {
    match (pack, routed) {
        (Pack::ReactVite, true) => {
            "kebab-case or a TanStack Router name ($param, _pathless, (group), __root)"
        }
        (Pack::Nextjs, true) => {
            "kebab-case or a Next.js route name ([param], [...rest], [[...rest]], (group), @slot)"
        }
        _ => "kebab-case",
    }
}

/// Whether a folder name is allowed.
fn folder_name(pack: Pack, name: &str, routed: bool) -> bool {
    kebab(name)
        || (routed
            && match pack {
                Pack::ReactVite => vite_route_name(name),
                Pack::Nextjs => next_route_name(name),
                Pack::ReactLibrary => false,
            })
}

/// Whether a file stem is allowed.
fn file_stem(pack: Pack, stem: &str, routed: bool) -> bool {
    kebab(stem)
        || (routed && pack == Pack::ReactVite && (stem == "__root" || vite_route_name(stem)))
}

/// A TanStack Router path segment: `$param`, `$` (splat), `_pathless`, `(group)`.
fn vite_route_name(name: &str) -> bool {
    if name == "$" {
        return true;
    }
    if let Some(parameter) = name.strip_prefix('$') {
        return camel(parameter);
    }
    if let Some(pathless) = name.strip_prefix('_') {
        return kebab(pathless);
    }
    group(name)
}

/// A Next.js App Router segment: `[param]`, `[...rest]`, `[[...rest]]`, `(group)`, `@slot`, and an
/// intercepting route's `(.)`, `(..)` or `(...)` prefixes before one of those or a kebab name.
fn next_route_name(name: &str) -> bool {
    let mut rest = name;
    while let Some(after) = ["(...)", "(..)", "(.)"]
        .iter()
        .find_map(|prefix| rest.strip_prefix(prefix))
    {
        rest = after;
    }
    if rest.len() < name.len() && kebab(rest) {
        return true;
    }
    if let Some(slot) = rest.strip_prefix('@') {
        return kebab(slot);
    }
    if let Some(inner) = rest
        .strip_prefix("[[...")
        .and_then(|inner| inner.strip_suffix("]]"))
    {
        return camel(inner);
    }
    if let Some(inner) = rest
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
    {
        return camel(inner.strip_prefix("...").unwrap_or(inner));
    }
    group(rest)
}

/// A route group: `(name)` with a kebab name.
fn group(name: &str) -> bool {
    name.strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .is_some_and(kebab)
}
