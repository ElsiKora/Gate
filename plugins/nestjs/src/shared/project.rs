//! The project's TypeScript sources as the rules of one call share them ([`Nest`]): the call state
//! of `gate-typescript` ([`gate_typescript::call`]), which every TypeScript rule plugin shares.

pub use gate_typescript::call::{Owned, Sources, TypeScriptCall as Nest, parsed, settings};
