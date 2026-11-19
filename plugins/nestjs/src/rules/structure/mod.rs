//! The structure rules.

pub mod adapter_boundary;
pub mod advisory_lock_boundary;
pub mod advisory_lock_manifest;
pub mod asset_storage;
pub mod automator_metadata;
pub mod barrel_exports;
pub mod constant_files;
pub mod controller_owners;
pub mod declaration_files;
pub mod dto_decorators;
pub mod enum_values;
pub mod error_logging;
pub mod field_inventory;
pub mod import_direction;
pub mod notification_egress;
pub mod package_pins;
pub mod raw_config_access;
pub mod runtime_factory;
pub mod string_enums;

/// Passes the rules collected so far, followed by the structure rules, on to `$next!($last, ..)`.
/// Each rule adds one `$crate::rules::structure::<module>::<Rule>,` line, in rule-name order.
macro_rules! structure_rules {
    ($next:ident, $last:ident, [$($rule:ty),* $(,)?]) => {
        $next! {
            $last,
            [
                $($rule,)*
                $crate::rules::structure::adapter_boundary::AdapterBoundary,
                $crate::rules::structure::advisory_lock_boundary::AdvisoryLockBoundary,
                $crate::rules::structure::advisory_lock_manifest::AdvisoryLockManifest,
                $crate::rules::structure::asset_storage::AssetStorage,
                $crate::rules::structure::automator_metadata::AutomatorMetadata,
                $crate::rules::structure::barrel_exports::BarrelExports,
                $crate::rules::structure::constant_files::ConstantFiles,
                $crate::rules::structure::controller_owners::ControllerOwners,
                $crate::rules::structure::declaration_files::DeclarationFiles,
                $crate::rules::structure::dto_decorators::DtoDecorators,
                $crate::rules::structure::enum_values::EnumValues,
                $crate::rules::structure::error_logging::ErrorLogging,
                $crate::rules::structure::field_inventory::FieldInventory,
                $crate::rules::structure::import_direction::ImportDirection,
                $crate::rules::structure::notification_egress::NotificationEgress,
                $crate::rules::structure::package_pins::PackagePins,
                $crate::rules::structure::raw_config_access::RawConfigAccess,
                $crate::rules::structure::runtime_factory::RuntimeFactory,
                $crate::rules::structure::string_enums::StringEnums,
            ]
        }
    };
}

pub(crate) use structure_rules;
