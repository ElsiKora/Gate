//! The `git` plugin: the first-parent history of main since an activation commit has the forms
//! of the policy, by default a merge, a release or a direct commit (`git/main-history`); no commit
//! since activation skips CI (`git/no-ci-skip`); the checked-out branch has an allowed name
//! (`git/branch-name`). The rules read history only through the host's git interface.
//!
//! Built for `wasm32-wasip2` it is the component `git.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod branch_name;
mod forms;
mod history;
mod main_history;
mod no_ci_skip;
mod settings;

pub use self::branch_name::{BranchName, Options as BranchNameOptions};
pub use self::forms::{Check, Form, Kind, default_forms};
pub use self::main_history::{MainHistory, Options as MainHistoryOptions};
pub use self::no_ci_skip::{NoCiSkip, Options as NoCiSkipOptions};
pub use self::settings::Settings;

gate_sdk::plugin! {
    name: "git",
    version: env!("CARGO_PKG_VERSION"),
    options: Settings,
    rules: [MainHistory, NoCiSkip, BranchName],
    generators: [],
    presets: [],
}
