//! The `docs` plugin: one documentation layout for every project, a standard per kind of project
//! ([`Kind`]). The standards look alike: they differ only where a kind's nature demands it, and
//! each difference carries its reason ([`standard`]). The generator `docs/layout` seeds a
//! project's required index pages through `gate sync`; the rule `docs/layout` checks that the
//! standard holds. Both read the kind from the `[plugins.docs]` options ([`Settings`]).
//!
//! Built for `wasm32-wasip2` it is the component `docs.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod lay_down;
mod layout;
mod page;
mod settings;
pub mod standard;

pub use self::lay_down::LayDown;
pub use self::layout::Layout;
pub use self::settings::{Extra, Settings};
pub use self::standard::Kind;

/// The `docs/recommended` preset: every rule of the plugin at error.
const RECOMMENDED: &str = r#"{ "layout": { "level": "error" } }"#;

/// The `docs/adoption` preset: every rule of the plugin at warning, for a project that is moving
/// to its standard.
const ADOPTION: &str = r#"{ "layout": { "level": "warning" } }"#;

gate_sdk::plugin! {
    name: "docs",
    version: env!("CARGO_PKG_VERSION"),
    options: Settings,
    rules: [Layout],
    generators: [LayDown],
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
