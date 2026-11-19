//! The `typescript` plugin: the TypeScript language (`.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`,
//! `.mjs`, `.cjs`) and its rule `typescript/parse`.
//!
//! Built for `wasm32-wasip2` it is the component `typescript.wasm`; natively it is a library the
//! tests call through [`PLUGIN`].

mod parse;

pub use self::parse::{Options, Parse};

gate_sdk::plugin! {
    name: "typescript",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Parse],
    generators: [],
    presets: [],
}
