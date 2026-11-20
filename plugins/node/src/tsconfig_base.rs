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

/// `flags`: the base sets every standard flag and the common resolution.
fn flags(run: &mut Run<'_, NoOptions>, path: &str, base: &Node) {
    let set = options(base);
    let value = |flag: &str| set.iter().find(|member| member.key == flag);
    for flag in STRICT_FLAGS {
        if value(flag).and_then(|member| member.value.as_bool()) != Some(true) {
            crate::report(
                run.report(),
                "flags",
                (path, value(flag).map(|member| member.line)),
                flag,
                format!("the base {path} does not set {flag} to true"),
                "set it to true; the base carries every standard flag",
            );
        }
    }
    for flag in ["module", "moduleResolution"] {
        let found = value(flag);
        if !found
            .and_then(|member| member.value.as_str())
            .is_some_and(|text| text.eq_ignore_ascii_case("NodeNext"))
        {
            crate::report(
                run.report(),
                "flags",
                (path, found.map(|member| member.line)),
                flag,
                format!("the base {path} does not set {flag} to NodeNext"),
                "set it to NodeNext; a frontend overrides it with Bundler in its own tsconfig.json",
            );
        }
    }
}
