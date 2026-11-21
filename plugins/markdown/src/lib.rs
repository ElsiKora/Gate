//! The `markdown` plugin: documentation pages whose local links resolve
//! (`markdown/local-links`), that carry their front matter (`markdown/frontmatter`) and the
//! level-two sections their path requires (`markdown/required-sections`).
//!
//! Built for `wasm32-wasip2` it is the component `markdown.wasm`; natively it is a library the
//! tests call through [`PLUGIN`].

mod frontmatter;
mod local_links;
mod pages;
mod required_sections;
mod text;

pub use self::frontmatter::{Frontmatter, Options as FrontmatterOptions};
pub use self::local_links::{LocalLinks, Options as LocalLinksOptions};
pub use self::required_sections::{Options as RequiredSectionsOptions, RequiredSections, Sections};

/// `markdown/recommended`: every rule a preset may turn on, at error, for a repository that follows
/// the standard. It leaves out `required-sections`, whose `sections` only the project can give.
const RECOMMENDED: &str = r#"{
    "frontmatter": { "level": "error" },
    "local-links": { "level": "error" }
}"#;

/// `markdown/adoption`: the rules of `markdown/recommended` at warning, for a repository on its way
/// to the standard.
const ADOPTION: &str = r#"{
    "frontmatter": { "level": "warning" },
    "local-links": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "markdown",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Frontmatter, LocalLinks, RequiredSections],
    generators: [],
    presets: [
        gate_sdk::Preset { name: "recommended", rules: RECOMMENDED },
        gate_sdk::Preset { name: "adoption", rules: ADOPTION },
    ],
}
