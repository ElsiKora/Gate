//! The `typescript` plugin: the TypeScript language (`.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`,
//! `.mjs`, `.cjs`) and its rules: `typescript/parse`, and `typescript/barrel-imports`, which keeps
//! imports from outside a folder with a barrel to that barrel and the names it exports.
//!
//! Built for `wasm32-wasip2` it is the component `typescript.wasm`; natively it is a library the
//! tests call through [`PLUGIN`].

mod barrel_imports;
mod parse;

pub use self::barrel_imports::{BarrelImports, Options as BarrelImportsOptions};
pub use self::parse::{Options, Parse};

gate_sdk::plugin! {
    name: "typescript",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Parse, BarrelImports],
    generators: [],
    presets: [],
}
