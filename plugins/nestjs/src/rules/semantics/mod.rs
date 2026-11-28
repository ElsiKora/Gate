//! The semantic rules.

pub mod api_readiness;
mod code_files;
pub mod custom_routes;
pub mod entity_descriptions;
pub mod entity_table_name;
pub mod eslint_directives;
mod folders;
pub mod import_aliases;
pub mod kind_layout;
pub mod migrations;
pub mod module_names;
pub mod module_registry;
pub mod native_functions;
pub mod native_repository;
pub mod owner_boundaries;
pub mod owner_roots;
mod package;
pub mod prefix_nesting;
pub mod repeated_segment;
pub mod typeorm_lock_owner;
pub mod typeorm_relation;
pub mod wrappers;

/// Passes the rules collected so far, followed by the semantic rules, on to `$next!(..)`. Each
/// rule adds one `$crate::rules::semantics::<module>::<Rule>,` line, in rule-name order.
macro_rules! semantics_rules {
    ($next:ident, [$($rule:ty),* $(,)?]) => {
        $next! {
            [
                $($rule,)*
                $crate::rules::semantics::api_readiness::Rule,
                $crate::rules::semantics::custom_routes::Rule,
                $crate::rules::semantics::entity_descriptions::Rule,
                $crate::rules::semantics::entity_table_name::Rule,
                $crate::rules::semantics::eslint_directives::Rule,
                $crate::rules::semantics::import_aliases::Rule,
                $crate::rules::semantics::kind_layout::Rule,
                $crate::rules::semantics::migrations::Rule,
                $crate::rules::semantics::module_names::Rule,
                $crate::rules::semantics::module_registry::Rule,
                $crate::rules::semantics::native_functions::Rule,
                $crate::rules::semantics::native_repository::Rule,
                $crate::rules::semantics::owner_boundaries::Rule,
                $crate::rules::semantics::owner_roots::Rule,
                $crate::rules::semantics::prefix_nesting::Rule,
                $crate::rules::semantics::repeated_segment::Rule,
                $crate::rules::semantics::typeorm_lock_owner::Rule,
                $crate::rules::semantics::typeorm_relation::Rule,
                $crate::rules::semantics::wrappers::Rule,
            ]
        }
    };
}

pub(crate) use semantics_rules;
