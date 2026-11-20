//! The `ci` plugin: GitHub Actions workflows held to one standard. Actions are pinned by commit
//! with their release in a comment (`ci/actions-pinned`), `workflow_run` names workflows that exist
//! (`ci/workflow-run-target`), every job sets its own token permissions (`ci/permissions`), a
//! workflow has a concurrency group that cancels only pull-request runs (`ci/concurrency`), jobs set
//! a timeout (`ci/timeout`) and a versioned runner (`ci/runner-pinned`), checkouts keep no
//! credentials (`ci/checkout-credentials`), secrets reach only their steps (`ci/secrets-in-steps`),
//! jobs that write outside the checks run in an environment (`ci/environment`), Node is set up one
//! way (`ci/node-setup`), and workflows, jobs and steps are named (`ci/names`). The preset
//! `ci/recommended` turns every rule on.
//!
//! Built for `wasm32-wasip2` it is the component `ci.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod actions_pinned;
mod checkout_credentials;
mod concurrency;
mod environment;
mod names;
mod node_setup;
mod permissions;
mod runner_pinned;
mod secrets_in_steps;
mod timeout;
mod workflow_run_target;
mod workflows;

pub use self::actions_pinned::{ActionsPinned, Options as ActionsPinnedOptions};
pub use self::checkout_credentials::{CheckoutCredentials, Options as CheckoutCredentialsOptions};
pub use self::concurrency::Concurrency;
pub use self::environment::{Environment, Options as EnvironmentOptions};
pub use self::names::Names;
pub use self::node_setup::NodeSetup;
pub use self::permissions::Permissions;
pub use self::runner_pinned::{Options as RunnerPinnedOptions, RunnerPinned};
pub use self::secrets_in_steps::SecretsInSteps;
pub use self::timeout::{Options as TimeoutOptions, Timeout};
pub use self::workflow_run_target::WorkflowRunTarget;
pub use self::workflows::Options;

/// The `ci/recommended` preset: every rule of the plugin at `error`, the same in every repository
/// that takes it.
const RECOMMENDED: &str = r#"{
    "actions-pinned": { "level": "error" },
    "workflow-run-target": { "level": "error" },
    "permissions": { "level": "error" },
    "concurrency": { "level": "error" },
    "timeout": { "level": "error" },
    "runner-pinned": { "level": "error" },
    "checkout-credentials": { "level": "error" },
    "secrets-in-steps": { "level": "error" },
    "environment": { "level": "error" },
    "node-setup": { "level": "error" },
    "names": { "level": "error" }
}"#;

gate_sdk::plugin! {
    name: "ci",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [
        ActionsPinned,
        WorkflowRunTarget,
        Permissions,
        Concurrency,
        Timeout,
        RunnerPinned,
        CheckoutCredentials,
        SecretsInSteps,
        Environment,
        NodeSetup,
        Names,
    ],
    generators: [],
    presets: [gate_sdk::Preset {
        name: "recommended",
        rules: RECOMMENDED,
    }],
}
