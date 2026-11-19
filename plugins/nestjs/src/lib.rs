//! The `nestjs` plugin: the source conventions of a `NestJS` service built on `TypeORM` and a CRUD
//! automator. Its rules read the service's TypeScript through `gate-typescript` and its layout
//! through the `[plugins.nestjs] options` block ([`layout`]).
//!
//! Built for `wasm32-wasip2` it is the component `nestjs.wasm`; natively it is a library the native
//! tests call through [`PLUGIN`] and [`canonical`].

pub mod canonical;
pub mod layout;
pub mod rules;
pub mod shared;

use self::rules::semantics::semantics_rules;
use self::rules::structure::structure_rules;

/// Declares the plugin with the rules of both lists.
macro_rules! declare {
    ([$($rule:ty),* $(,)?]) => {
        gate_sdk::plugin! {
            name: "nestjs",
            version: env!("CARGO_PKG_VERSION"),
            options: crate::layout::PluginOptions,
            rules: [$($rule),*],
            generators: [],
            presets: [],
        }
    };
}

structure_rules! { semantics_rules, declare, [] }
