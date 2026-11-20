//! The `typescript` plugin: the TypeScript language (`.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`,
//! `.mjs`, `.cjs`) and its rules: `typescript/parse`; `typescript/barrel-imports`, which keeps
//! imports from outside a folder with a barrel to that barrel and the names it exports; and
//! `typescript/string-enums`, which wants an enum wherever string literals name variants.
//!
//! Built for `wasm32-wasip2` it is the component `typescript.wasm`; natively it is a library the
//! tests call through [`PLUGIN`].

mod barrel_imports;
mod findings;
mod parse;
mod string_enums;

pub use self::barrel_imports::{BarrelImports, Options as BarrelImportsOptions};
pub use self::parse::{Options, Parse};
pub use self::string_enums::{Options as StringEnumsOptions, StringEnums};

gate_sdk::plugin! {
    name: "typescript",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Parse, BarrelImports, StringEnums],
    generators: [],
    presets: [],
}
