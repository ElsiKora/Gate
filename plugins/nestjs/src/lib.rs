//! The `nestjs` plugin: the source conventions of a `NestJS` service built on `TypeORM` and a CRUD
//! automator. Its rules read the service's TypeScript through `gate-typescript` and its layout
//! through the `[plugins.nestjs] options` block ([`layout`]).
//!
//! Built for `wasm32-wasip2` it is the component `nestjs.wasm`; natively it is a library the native
//! tests call through [`PLUGIN`] and [`canonical`].

pub mod canonical;
pub mod layout;
pub mod rules;
pub mod shared;

pub use self::shared::project::Nest;

use self::rules::semantics::semantics_rules;
use self::rules::structure::structure_rules;

/// `nestjs/recommended`: every rule a preset may turn on, at error, for a repository that follows
/// the standard. It leaves out the rules that need the project's own options or inventory, or a
/// subsystem a service may not have; the plugin's section of the plugins guide lists them.
const RECOMMENDED: &str = r#"{
    "adapter-boundary": { "level": "error" },
    "api-readiness": { "level": "error" },
    "automator-metadata": { "level": "error" },
    "barrel-exports": { "level": "error" },
    "constant-files": { "level": "error" },
    "custom-routes": { "level": "error" },
    "declaration-files": { "level": "error" },
    "dto-decorators": { "level": "error" },
    "entity-descriptions": { "level": "error" },
    "entity-table-name": { "level": "error" },
    "enum-values": { "level": "error" },
    "error-logging": { "level": "error" },
    "error-string-shim": { "level": "error" },
    "eslint-directives": { "level": "error" },
    "import-aliases": { "level": "error" },
    "kind-layout": { "level": "error" },
    "migrations": { "level": "error" },
    "module-names": { "level": "error" },
    "module-registry": { "level": "error" },
    "native-functions": { "level": "error" },
    "native-repository": { "level": "error" },
    "prefix-nesting": { "level": "error" },
    "repeated-segment": { "level": "error" },
    "typeorm-relation": { "level": "error" },
    "wrappers": { "level": "error" }
}"#;

/// `nestjs/adoption`: the rules of `nestjs/recommended` at warning, for a repository on its way to
/// the standard.
const ADOPTION: &str = r#"{
    "adapter-boundary": { "level": "warning" },
    "api-readiness": { "level": "warning" },
    "automator-metadata": { "level": "warning" },
    "barrel-exports": { "level": "warning" },
    "constant-files": { "level": "warning" },
    "custom-routes": { "level": "warning" },
    "declaration-files": { "level": "warning" },
    "dto-decorators": { "level": "warning" },
    "entity-descriptions": { "level": "warning" },
    "entity-table-name": { "level": "warning" },
    "enum-values": { "level": "warning" },
    "error-logging": { "level": "warning" },
    "error-string-shim": { "level": "warning" },
    "eslint-directives": { "level": "warning" },
    "import-aliases": { "level": "warning" },
    "kind-layout": { "level": "warning" },
    "migrations": { "level": "warning" },
    "module-names": { "level": "warning" },
    "module-registry": { "level": "warning" },
    "native-functions": { "level": "warning" },
    "native-repository": { "level": "warning" },
    "prefix-nesting": { "level": "warning" },
    "repeated-segment": { "level": "warning" },
    "typeorm-relation": { "level": "warning" },
    "wrappers": { "level": "warning" }
}"#;

/// Declares the plugin with the rules of both lists.
macro_rules! declare {
    ([$($rule:ty),* $(,)?]) => {
        gate_sdk::plugin! {
            name: "nestjs",
            version: env!("CARGO_PKG_VERSION"),
            options: crate::layout::PluginOptions,
            call: crate::Nest,
            rules: [$($rule),*],
            generators: [],
            presets: [
                gate_sdk::Preset { name: "recommended", rules: crate::RECOMMENDED },
                gate_sdk::Preset { name: "adoption", rules: crate::ADOPTION },
            ],
        }
    };
}

structure_rules! { semantics_rules, declare, [] }
