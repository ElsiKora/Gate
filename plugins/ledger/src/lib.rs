//! The `ledger` plugin: rules over a Markdown ledger of decisions, ADRs, plan, implementation,
//! verification, follow-up and production blocker records, over the owners and decisions the
//! project's exceptions name and the decisions its rule entries name, and the generators that
//! write the counts of a decision index and the review progress of a roadmap into their managed
//! blocks.
//!
//! The shared paths, the parity plan and the status vocabularies are `[plugins.ledger] options`
//! ([`Settings`]); each rule's own options are in its module. Every path is relative to the project
//! root. Built for `wasm32-wasip2` the plugin is the component `ledger.wasm`; natively it is a
//! library the tests call through [`PLUGIN`].

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
pub use self::rules::exceptions::{ExceptionDecisions, ExceptionOwners};
pub use self::rules::manifest::Manifest;
pub use self::rules::records::{RecordStatus, Records};
pub use self::rules::reuses::Reuses;
pub use self::rules::rule_decisions::RuleDecisions;
pub use self::settings::{Parity, Settings};

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
        RuleDecisions,
    ],
    generators: [DecisionCounts, ReviewProgress],
    presets: [],
}
