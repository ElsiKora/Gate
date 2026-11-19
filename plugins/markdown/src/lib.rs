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

gate_sdk::plugin! {
    name: "markdown",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Frontmatter, LocalLinks, RequiredSections],
    generators: [],
    presets: [],
}
