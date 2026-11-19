//! The agent rule-file format the `agents` plugin documents, shared with the plugins that read it
//! (the `git` plugin's `#direct` check compares Enforcement/Status lines across commits).
//!
//! A rule file is a Markdown file with YAML front matter. Each rule is a heading whose first word
//! is its id (`RULE-<AREA>-<nnn>`), followed by the metadata line
//! `Enforcement: <class>[ <detail>] · Status: <status>[ · Redirect: <id>]`.

pub mod markdown;
pub mod rules;
pub mod yaml;

pub use self::rules::{CLASSES, Metadata, Rule, RuleFile, STATUSES, parse_rule_file, rule_id};
