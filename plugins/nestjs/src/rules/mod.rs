//! The plugin's rules, in two groups that each keep their own list: the structure rules (where
//! files live, what they export and load, how packages are pinned) and the semantic rules (what
//! classes, routes, entities and wrappers declare).

pub mod semantics;
pub mod structure;
