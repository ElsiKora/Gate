//! The `frontend` plugin: one standard for React apps and libraries. Its rules hold a project to
//! one tree of Feature-Sliced layers under `src/`, kebab-case names, imports that point down through
//! one alias and each slice's public API, one module that talks to the network, one module that
//! reads the environment, every user-visible string through the translation layer, assets and
//! styles in known places, and the alias, router plugin and packages the toolchain files declare.
//! The option `pack` (`react-vite`, `nextjs` or `react-library`) decides the few cells a framework
//! forces ([`options`]).
//!
//! Built for `wasm32-wasip2` it is the component `frontend.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

pub mod code;
pub mod config;
pub mod options;
pub mod rules;
pub mod standard;

use self::rules::api_boundary::ApiBoundary;
use self::rules::asset_placement::AssetPlacement;
use self::rules::client_boundary::ClientBoundary;
use self::rules::colocated_files::ColocatedFiles;
use self::rules::component_names::ComponentNames;
use self::rules::env_access::EnvAccess;
use self::rules::file_names::FileNames;
use self::rules::hook_names::HookNames;
use self::rules::import_alias::ImportAlias;
use self::rules::import_direction::ImportDirection;
use self::rules::import_paths::ImportPaths;
use self::rules::layout::Layout;
use self::rules::literal_text::LiteralText;
use self::rules::messages::Messages;
use self::rules::packages::Packages;
use self::rules::route_config::RouteConfig;
use self::rules::route_files::RouteFiles;
use self::rules::state_placement::StatePlacement;
use self::rules::style_files::StyleFiles;
use self::rules::typescript_sources::TypeScriptSources;

/// The preset a project that meets the standard extends: every rule at `error`.
const RECOMMENDED: &str = r#"{
    "layout": { "level": "error" },
    "file-names": { "level": "error" },
    "typescript-sources": { "level": "error" },
    "import-direction": { "level": "error" },
    "import-paths": { "level": "error" },
    "component-names": { "level": "error" },
    "hook-names": { "level": "error" },
    "state-placement": { "level": "error" },
    "api-boundary": { "level": "error" },
    "env-access": { "level": "error" },
    "literal-text": { "level": "error" },
    "messages": { "level": "error" },
    "asset-placement": { "level": "error" },
    "style-files": { "level": "error" },
    "route-files": { "level": "error" },
    "client-boundary": { "level": "error" },
    "colocated-files": { "level": "error" },
    "import-alias": { "level": "error" },
    "route-config": { "level": "error" },
    "packages": { "level": "error" }
}"#;

/// The preset an existing project extends while it moves to the standard: every rule at `warning`,
/// so it sees every departure without a failing gate.
const ADOPTION: &str = r#"{
    "layout": { "level": "warning" },
    "file-names": { "level": "warning" },
    "typescript-sources": { "level": "warning" },
    "import-direction": { "level": "warning" },
    "import-paths": { "level": "warning" },
    "component-names": { "level": "warning" },
    "hook-names": { "level": "warning" },
    "state-placement": { "level": "warning" },
    "api-boundary": { "level": "warning" },
    "env-access": { "level": "warning" },
    "literal-text": { "level": "warning" },
    "messages": { "level": "warning" },
    "asset-placement": { "level": "warning" },
    "style-files": { "level": "warning" },
    "route-files": { "level": "warning" },
    "client-boundary": { "level": "warning" },
    "colocated-files": { "level": "warning" },
    "import-alias": { "level": "warning" },
    "route-config": { "level": "warning" },
    "packages": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "frontend",
    version: env!("CARGO_PKG_VERSION"),
    options: crate::options::PluginOptions,
    call: gate_typescript::call::TypeScriptCall,
    rules: [
        Layout,
        FileNames,
        TypeScriptSources,
        ImportDirection,
        ImportPaths,
        ComponentNames,
        HookNames,
        StatePlacement,
        ApiBoundary,
        EnvAccess,
        LiteralText,
        Messages,
        AssetPlacement,
        StyleFiles,
        RouteFiles,
        ClientBoundary,
        ColocatedFiles,
        ImportAlias,
        RouteConfig,
        Packages,
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
