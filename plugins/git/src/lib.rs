//! The `git` plugin: the first-parent history of main since an activation commit has the forms
//! of the policy, by default a merge, a release or a direct commit (`git/main-history`); no commit
//! since activation skips CI (`git/no-ci-skip`); the checked-out branch has an allowed name
//! (`git/branch-name`); the messages of the commit being made and of the pushed commits have the
//! conventional form (`git/commit-message`). The rules read history only through the host's git
//! interface.
//!
//! Built for `wasm32-wasip2` it is the component `git.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod branch_name;
mod commit_message;
mod forms;
mod history;
mod javascript;
mod main_history;
mod no_ci_skip;
mod pins;
mod settings;

pub use self::branch_name::{BranchName, Options as BranchNameOptions};
pub use self::commit_message::{CommitMessage, Options as CommitMessageOptions};
pub use self::forms::{Check, Form, Kind, default_forms};
pub use self::main_history::{MainHistory, Options as MainHistoryOptions};
pub use self::no_ci_skip::{NoCiSkip, Options as NoCiSkipOptions};
pub use self::settings::Settings;

/// `git/recommended`: the policies of the `ElsiKora` linters at error, with
/// `@elsikora/git-branch-lint`'s default branch template and commitlint's levels, whose two level-1
/// checks are warnings. It leaves out `main-history` and `no-ci-skip`, which need the plugin
/// options `activation` and `release-subject` that only the repository can give.
const RECOMMENDED: &str = r#"{
    "branch-name": { "level": "error", "options": { "template": ":type/:ticket-:name" } },
    "commit-message": {
        "level": "error",
        "levels": { "header-min-length": "warning", "scope-max-length": "warning" }
    }
}"#;

/// `git/adoption`: the rules of `git/recommended` at warning, with the same branch template, for a
/// repository on its way to the standard.
const ADOPTION: &str = r#"{
    "branch-name": { "level": "warning", "options": { "template": ":type/:ticket-:name" } },
    "commit-message": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "git",
    version: env!("CARGO_PKG_VERSION"),
    options: Settings,
    rules: [MainHistory, NoCiSkip, BranchName, CommitMessage],
    generators: [],
    presets: [
        gate_sdk::Preset { name: "recommended", rules: RECOMMENDED },
        gate_sdk::Preset { name: "adoption", rules: ADOPTION },
    ],
}
