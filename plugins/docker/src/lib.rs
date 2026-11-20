//! The `docker` plugin: image files (`docker/dockerfile`) and compose files (`docker/compose`)
//! that follow one standard: pinned images, no secret written in, nothing published beyond the
//! machine, one name per file. Both rules read the `[plugins.docker]` options ([`Settings`]).
//!
//! Built for `wasm32-wasip2` it is the component `docker.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod compose;
mod dockerfile;
mod instructions;
mod settings;

pub use self::compose::{Compose, Options as ComposeOptions};
pub use self::dockerfile::{Dockerfile, Options as DockerfileOptions};
pub use self::settings::Settings;

/// `docker/recommended`: both rules at `error`.
const RECOMMENDED: &str = r#"{
    "dockerfile": { "level": "error" },
    "compose": { "level": "error" }
}"#;

/// `docker/adoption`: both rules at `warning`, for a project on its way to `recommended`.
const ADOPTION: &str = r#"{
    "dockerfile": { "level": "warning" },
    "compose": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "docker",
    version: env!("CARGO_PKG_VERSION"),
    options: Settings,
    rules: [Dockerfile, Compose],
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
