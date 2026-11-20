//! `cdk/file-names`: every TypeScript file below `lib/` is `<name>.<role>.ts` in kebab case, with
//! a role of the standard, in the place of its role.

use gate_sdk::{NoOptions, Rule, Run};

use crate::options::PluginOptions;
use crate::rules::{Found, file_name, folder, name_and_role, report};
use crate::sources::{Cdk, has_extension};
use crate::standard::{ROLE_DIRECTORIES, ROLE_PLACES, ROLES, is_kebab};

/// The rule.
#[derive(Debug, Default)]
pub struct FileNames;

impl Rule<Cdk> for FileNames {
    type Options = NoOptions;

    const NAME: &'static str = "file-names";
    const TITLE: &'static str =
        "Every file below lib/ is <name>.<role>.ts in kebab case, in the place of its role";
    const ASPECTS: &'static [&'static str] = &["case", "role", "role-place", "bare-role"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        let paths = run.tree().paths();

        for path in paths
            .iter()
            .filter(|path| path.starts_with("lib/") && has_extension(path, "ts"))
        {
            if let Some(found) = judge(path) {
                report(run, found);
            }
        }

        Ok(())
    }
}

/// The one finding of `path`, the first that applies in the order case, bare role, role, place.
fn judge(path: &str) -> Option<Found<'_>> {
    let name = file_name(path);
    let directories: Vec<&str> = folder(path).split('/').skip(1).collect();
    let stem = name.strip_suffix(".ts").unwrap_or(name);
    let words: Vec<&str> = stem.split('.').collect();

    if directories.iter().any(|directory| !is_kebab(directory))
        || words.iter().any(|word| !is_kebab(word))
    {
        return Some(found(
            "case",
            path,
            format!("{path} is not kebab case: lowercase words joined by -"),
            "rename the file and its folders to kebab case",
        ));
    }

    if name == "index.ts" {
        return None;
    }

    if let [only] = words.as_slice()
        && ROLES.contains(only)
    {
        return Some(found(
            "bare-role",
            path,
            format!("{path} is named only by its role; the name says what it is"),
            "name the file <name>.<role>.ts, such as create-physical-name.function.ts",
        ));
    }

    let Some((stem_name, role)) = name_and_role(name).filter(|(_, role)| ROLES.contains(role))
    else {
        return Some(found(
            "role",
            path,
            format!(
                "{path} does not end with a role of the standard ({})",
                ROLES.join(", ")
            ),
            "name the file <name>.<role>.ts with the role of what it declares",
        ));
    };

    if ROLES.contains(&stem_name) {
        return Some(found(
            "bare-role",
            path,
            format!("{path} is named only by roles; the name says what it is"),
            "name the file <name>.<role>.ts, such as create-physical-name.function.ts",
        ));
    }

    if let Some((_, place)) = ROLE_PLACES
        .iter()
        .find(|(placed, place)| *placed == role && !path.starts_with(place))
    {
        return Some(found(
            "role-place",
            path,
            format!("{path} is a {role}, and a {role} lives below {place}"),
            "move the file below the directory of its role",
        ));
    }

    if let Some(directory) = directories.last()
        && ROLE_DIRECTORIES.contains(directory)
        && *directory != role
    {
        return Some(found(
            "role-place",
            path,
            format!(
                "{path} is a {role} in the {directory}/ directory, which holds only {directory} files"
            ),
            "give the file the role of its directory, or move it out",
        ));
    }

    None
}

fn found<'p>(
    aspect: &'static str,
    path: &'p str,
    message: String,
    help: &'static str,
) -> Found<'p> {
    Found {
        aspect,
        path,
        line: None,
        key: aspect.to_owned(),
        message,
        help,
    }
}
