//! The `ci` plugin: GitHub Actions workflows whose actions are pinned by a full commit SHA
//! (`ci/actions-pinned`) and whose `workflow_run` triggers name workflows that exist
//! (`ci/workflow-run-target`).
//!
//! Built for `wasm32-wasip2` it is the component `ci.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod actions_pinned;
mod workflow_run_target;
mod workflows;

pub use self::actions_pinned::ActionsPinned;
pub use self::workflow_run_target::WorkflowRunTarget;
pub use self::workflows::Options;

gate_sdk::plugin! {
    name: "ci",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [ActionsPinned, WorkflowRunTarget],
    generators: [],
    presets: [],
}
