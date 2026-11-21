//! `node/tsconfig-base`: each install root that uses `TypeScript` has one base, and the base
//! carries every standard flag and the common module resolution. It is checked here, once per
//! install, whether or not a Gate project owns it; `node/tsconfig` checks that each project's
//! configurations reach it.

use gate_sdk::{NoOptions, Result, Rule, Run, Scope, Tree};

use crate::json::Node;
use crate::repo::{self, file_name, join, root_of};
use crate::settings::Settings;
use crate::standard::STRICT_FLAGS;
use crate::tsconfig::options;

#[derive(Debug, Default)]
pub(crate) struct TsconfigBase;

impl Rule for TsconfigBase {
    type Options = NoOptions;
    const NAME: &'static str = "tsconfig-base";
    const TITLE: &'static str = "Each install root's TypeScript base carries every standard flag";
    const ASPECTS: &'static [&'static str] = &["missing", "flags"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, NoOptions>) -> Result<()> {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let roots = settings.install_roots();
        let configs = configurations(&tree);
        for root in &roots {
            if !configs
                .iter()
                .any(|path| root_of(path, &roots) == root.as_str())
            {
                continue;
            }
            let path = join(root, &settings.base);
            match repo::read_json(&tree, &path)? {
                Some(base) => flags(run, &path, &base),
                None => crate::report(
                    run.report(),
                    "missing",
                    (&path, None),
                    file_name(&path),
                    format!(
                        "{path} is not tracked; its install root has tsconfig files, and they \
                         need a base to extend"
                    ),
                    "add the base with the standard flags; every tsconfig of the install extends it",
                ),
            }
        }
        Ok(())
    }
}

/// Every tracked `tsconfig*.json` outside `node_modules`.
fn configurations(tree: &Tree<'_>) -> Vec<String> {
    tree.paths()
        .into_iter()
        .filter(|path| {
            let name = file_name(path);
            name.starts_with("tsconfig") && repo::is_json(name)
        })
        .filter(|path| !path.split('/').any(|segment| segment == "node_modules"))
        .collect()
}

/// `flags`: the base sets every standard flag and the common resolution. A flag written with another
/// value is reported at its line, one finding each, so a base that turns one flag off on purpose
/// can hold an exception for that flag alone. The flags it leaves out are reported together in one
/// finding at the file, which lists every line to add.
fn flags(run: &mut Run<'_, NoOptions>, path: &str, base: &Node) {
    let set = options(base);
    let value = |flag: &str| set.iter().find(|member| member.key == flag);
    let mut absent: Vec<(&str, &str)> = Vec::new();
    for flag in STRICT_FLAGS {
        let Some(member) = value(flag) else {
            absent.push((flag, "true"));
            continue;
        };
        if member.value.as_bool() != Some(true) {
            crate::report(
                run.report(),
                "flags",
                (path, Some(member.line)),
                flag,
                format!("the base {path} does not set {flag} to true"),
                "set it to true; the base carries every standard flag",
            );
        }
    }
    for flag in ["module", "moduleResolution"] {
        let Some(member) = value(flag) else {
            absent.push((flag, "\"NodeNext\""));
            continue;
        };
        if !member
            .value
            .as_str()
            .is_some_and(|text| text.eq_ignore_ascii_case("NodeNext"))
        {
            crate::report(
                run.report(),
                "flags",
                (path, Some(member.line)),
                flag,
                format!("the base {path} does not set {flag} to NodeNext"),
                "set it to NodeNext; a frontend overrides it with Bundler in its own tsconfig.json",
            );
        }
    }
    if absent.is_empty() {
        return;
    }
    let names: Vec<&str> = absent.iter().map(|(flag, _)| *flag).collect();
    let lines: Vec<String> = absent
        .iter()
        .map(|(flag, wanted)| format!("\"{flag}\": {wanted}"))
        .collect();
    let message = match names.as_slice() {
        [only] => format!("the base {path} leaves out the standard setting {only}"),
        all => format!(
            "the base {path} leaves out {} standard settings: {}",
            all.len(),
            all.join(", ")
        ),
    };
    crate::report(
        run.report(),
        "flags",
        (path, None),
        "compilerOptions",
        message,
        &format!(
            "add to its compilerOptions: {}; the base carries every standard flag and the common resolution",
            lines.join(", ")
        ),
    );
}
