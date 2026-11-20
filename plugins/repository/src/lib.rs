//! The `repository` plugin: what the tracked tree holds, whatever the language. No secret is
//! tracked (`repository/tracked-secrets`) and no large file is, unless the project needs it
//! (`repository/large-files`). Both rules run once per repository and see every tracked path.
//!
//! Built for `wasm32-wasip2` it is the component `repository.wasm`; natively it is a library the
//! tests call through [`PLUGIN`].

mod large_files;
mod tracked_secrets;

pub use self::large_files::{Extra, LargeFiles, Options as LargeFilesOptions};
pub use self::tracked_secrets::{Options as TrackedSecretsOptions, TrackedSecrets};

/// `repository/recommended`: both rules at `error`, a large file at `warning`.
const RECOMMENDED: &str = r#"{
    "tracked-secrets": { "level": "error" },
    "large-files": { "level": "error", "levels": { "large": "warning" } }
}"#;

/// `repository/adoption`: both rules at `warning`, for a repository on its way to `recommended`.
const ADOPTION: &str = r#"{
    "tracked-secrets": { "level": "warning" },
    "large-files": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "repository",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [TrackedSecrets, LargeFiles],
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
