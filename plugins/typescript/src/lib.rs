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

/// `typescript/recommended`: every rule at error, for a repository that follows the standard.
const RECOMMENDED: &str = r#"{
    "parse": { "level": "error" },
    "barrel-imports": { "level": "error" },
    "string-enums": { "level": "error" }
}"#;

/// `typescript/adoption`: the rules of `typescript/recommended` at warning, for a repository on its
/// way to the standard.
const ADOPTION: &str = r#"{
    "parse": { "level": "warning" },
    "barrel-imports": { "level": "warning" },
    "string-enums": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "typescript",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    call: gate_typescript::call::TypeScriptCall,
    rules: [Parse, BarrelImports, StringEnums],
    generators: [],
    presets: [
        gate_sdk::Preset { name: "recommended", rules: RECOMMENDED },
        gate_sdk::Preset { name: "adoption", rules: ADOPTION },
    ],
}
