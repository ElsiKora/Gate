//! The `ledger` plugin: rules over a Markdown ledger of decisions, ADRs, plan, implementation,
//! verification, follow-up and production blocker records, over the owners and decisions the
//! exceptions name and the decisions the rule entries name (a project's, and the root's when the
//! root is not a project), and the generators that write the counts of a decision index and the
//! review progress of a roadmap into their managed blocks.
//!
//! The shared paths, the parity plan and the status vocabularies are `[plugins.ledger] options`
//! ([`Settings`]); each rule's own options are in its module. Every path is relative to the call's
//! scope: the project root, or the repository root for the repository-scope `root-*` rules. Built
//! for `wasm32-wasip2` the plugin is the component `ledger.wasm`; natively it is a library the
//! tests call through [`PLUGIN`].

mod block;
mod catalog;
pub mod generators;
mod index;
mod input;
mod records;
pub mod rules;
mod settings;
mod text;

pub use self::generators::decision_counts::DecisionCounts;
pub use self::generators::review_progress::ReviewProgress;
pub use self::rules::blockers::Blockers;
pub use self::rules::closing_notes::ClosingNotes;
pub use self::rules::decisions::Decisions;
pub use self::rules::exceptions::{
    ExceptionDecisions, ExceptionOwners, RootExceptionDecisions, RootExceptionOwners,
};
pub use self::rules::manifest::Manifest;
pub use self::rules::records::{RecordStatus, Records};
pub use self::rules::reuses::Reuses;
pub use self::rules::rule_decisions::{RootRuleDecisions, RuleDecisions};
pub use self::settings::{Parity, Settings};

/// `ledger/recommended`: every rule a preset may turn on, at error, for a repository that follows
/// the standard. It leaves out `closing-notes`, `reuses` and `record-status`, which stay off until
/// the project sets `field`, `catalog` and the plugin option `parity`. A root that is not a project
/// takes only its three repository-scope `root-` rules from `[workspace] extends`.
const RECOMMENDED: &str = r#"{
    "blockers": { "level": "error" },
    "decisions": { "level": "error" },
    "exception-decisions": { "level": "error" },
    "exception-owners": { "level": "error" },
    "manifest": { "level": "error" },
    "records": { "level": "error" },
    "root-exception-decisions": { "level": "error" },
    "root-exception-owners": { "level": "error" },
    "root-rule-decisions": { "level": "error" },
    "rule-decisions": { "level": "error" }
}"#;

/// `ledger/adoption`: the rules of `ledger/recommended` at warning, for a repository on its way to
/// the standard.
const ADOPTION: &str = r#"{
    "blockers": { "level": "warning" },
    "decisions": { "level": "warning" },
    "exception-decisions": { "level": "warning" },
    "exception-owners": { "level": "warning" },
    "manifest": { "level": "warning" },
    "records": { "level": "warning" },
    "root-exception-decisions": { "level": "warning" },
    "root-exception-owners": { "level": "warning" },
    "root-rule-decisions": { "level": "warning" },
    "rule-decisions": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "ledger",
    version: env!("CARGO_PKG_VERSION"),
    options: Settings,
    rules: [
        Blockers,
        ClosingNotes,
        Decisions,
        ExceptionDecisions,
        ExceptionOwners,
        Manifest,
        RecordStatus,
        Records,
        Reuses,
        RootExceptionDecisions,
        RootExceptionOwners,
        RootRuleDecisions,
        RuleDecisions,
    ],
    generators: [DecisionCounts, ReviewProgress],
    presets: [
        gate_sdk::Preset { name: "recommended", rules: RECOMMENDED },
        gate_sdk::Preset { name: "adoption", rules: ADOPTION },
    ],
}
