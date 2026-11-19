//! The `agents` plugin: the agent instructions of a project (`AGENTS.md` and the rule files and
//! skills under `docs/agents`), checked by nine rules, and the files generated from them
//! (`agents/adapters`, `agents/agents-block`).
//!
//! Built for `wasm32-wasip2` it is the component `agents.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

mod adapters;
mod agents_block;
mod block;
mod instruction_references;
mod load;
mod no_claude_md;
mod rule_enforcement;
mod rule_file_size;
mod rule_ids;
mod rule_owner;
mod rule_references;
mod rule_registry;
mod skills;

pub use self::adapters::Adapters;
pub use self::agents_block::AgentsBlock;
pub use self::instruction_references::InstructionReferences;
pub use self::no_claude_md::NoClaudeMd;
pub use self::rule_enforcement::RuleEnforcement;
pub use self::rule_file_size::RuleFileSize;
pub use self::rule_ids::RuleIds;
pub use self::rule_owner::RuleOwner;
pub use self::rule_references::RuleReferences;
pub use self::rule_registry::RuleRegistry;
pub use self::skills::Skills;

gate_sdk::plugin! {
    name: "agents",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [
        InstructionReferences,
        NoClaudeMd,
        RuleEnforcement,
        RuleFileSize,
        RuleIds,
        RuleOwner,
        RuleReferences,
        RuleRegistry,
        Skills,
    ],
    generators: [Adapters, AgentsBlock],
    presets: [],
}
