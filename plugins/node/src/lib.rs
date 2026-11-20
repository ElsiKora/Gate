//! The `node` plugin: one Node toolchain for every project, by kind. A repository has one package
//! manager (`node/package-manager`), one Node version (`node/node-version`), one version per
//! dependency (`node/dependency-versions`), one Prettier configuration (`node/formatter`) and one
//! ESLint configuration (`node/linter`); each project's `package.json` (`node/package`), scripts
//! (`node/scripts`) and `tsconfig` files (`node/tsconfig`) follow the standard of its kind
//! ([`settings::Kind`]).
//!
//! Built for `wasm32-wasip2` it is the component `node.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod dependency_versions;
mod formatter;
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
mod yaml;

use gate_sdk::{Location, Report};

use self::dependency_versions::DependencyVersions;
use self::formatter::Formatter;
use self::linter::Linter;
use self::node_version::NodeVersion;
use self::package::Package;
use self::package_manager::PackageManager;
use self::scripts::Scripts;
use self::tsconfig::Tsconfig;

gate_sdk::plugin! {
    name: "node",
    version: env!("CARGO_PKG_VERSION"),
    options: crate::settings::Settings,
    rules: [
        PackageManager,
        NodeVersion,
        DependencyVersions,
        Formatter,
        Linter,
        Package,
        Scripts,
        Tsconfig,
    ],
    generators: [],
    presets: [],
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
