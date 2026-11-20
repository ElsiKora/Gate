//! The `node` plugin: one Node toolchain for every project, by kind. A repository has one package
//! manager (`node/package-manager`), one Node version (`node/node-version`), one version per
//! dependency (`node/dependency-versions`), one Prettier configuration (`node/formatter`) and one
//! `ESLint` configuration (`node/linter`); each project's `package.json` (`node/package`), scripts
//! (`node/scripts`) and `tsconfig` files (`node/tsconfig`) follow the standard of its kind
//! ([`settings::Kind`]), and each install root's `TypeScript` base carries the standard flags
//! (`node/tsconfig-base`).
//!
//! Built for `wasm32-wasip2` it is the component `node.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod dependency_versions;
mod formatter;
mod js;
mod json;
mod linter;
mod node_version;
mod package;
mod package_manager;
mod repo;
mod scripts;
mod semver;
pub mod settings;
mod standard;
mod tsconfig;
mod tsconfig_base;
mod yaml;

use gate_sdk::{Location, Report};

use self::dependency_versions::DependencyVersions;
use self::formatter::Formatter;
use self::linter::Linter;
use self::node_version::NodeVersion;
use self::package::Package;
use self::package_manager::PackageManager;
use self::scripts::Scripts;
use self::settings::Settings;
use self::tsconfig::Tsconfig;
use self::tsconfig_base::TsconfigBase;

/// `node/recommended`: every rule at error, for a repository that follows the standard.
const RECOMMENDED: &str = r#"{
    "package-manager": { "level": "error" },
    "node-version": { "level": "error" },
    "dependency-versions": { "level": "error" },
    "formatter": { "level": "error" },
    "linter": { "level": "error" },
    "package": { "level": "error" },
    "scripts": { "level": "error" },
    "tsconfig": { "level": "error" },
    "tsconfig-base": { "level": "error" }
}"#;

/// `node/adoption`: every rule at warning, for a repository on its way to the standard.
const ADOPTION: &str = r#"{
    "package-manager": { "level": "warning" },
    "node-version": { "level": "warning" },
    "dependency-versions": { "level": "warning" },
    "formatter": { "level": "warning" },
    "linter": { "level": "warning" },
    "package": { "level": "warning" },
    "scripts": { "level": "warning" },
    "tsconfig": { "level": "warning" },
    "tsconfig-base": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "node",
    version: env!("CARGO_PKG_VERSION"),
    options: Settings,
    rules: [
        PackageManager,
        NodeVersion,
        DependencyVersions,
        Formatter,
        Linter,
        Package,
        Scripts,
        Tsconfig,
        TsconfigBase,
    ],
    generators: [],
    presets: [
        gate_sdk::Preset {
            name: "recommended",
            rules: RECOMMENDED,
        },
        gate_sdk::Preset {
            name: "adoption",
            rules: ADOPTION,
        },
    ],
}

/// Reports one finding of `aspect` at `path` (and `line`, when known), keyed by `key` and
/// targeted as `<path>:<key>`, so an exception names one field, script, flag or dependency.
pub(crate) fn report(
    report: &mut Report,
    aspect: &str,
    (path, line): (&str, Option<usize>),
    key: &str,
    message: String,
    help: &str,
) {
    let location = match line {
        Some(line) => Location::line(path, line),
        None => Location::file(path),
    };
    report
        .finding(aspect, location, message)
        .key(key)
        .target(format!("{path}:{key}"))
        .help(help);
}
