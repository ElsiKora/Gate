//! Helpers the rules of both groups share: opening the project's TypeScript sources, the source
//! walks, module loads, syntax queries, option patterns and finding locations.

pub mod decisions;
pub mod decorators;
pub mod js;
pub mod json_lines;
pub mod modules;
pub mod paths;
pub mod pattern;
pub mod project;
pub mod report;
pub mod syntax;
pub mod tree;
