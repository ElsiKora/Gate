//! The smallest Gate plugin: one rule, `hello/marker`, that reports every line of a project's
//! files that holds a marker (`TODO` unless the options say otherwise).
//!
//! Build it with `cargo build --release -p gate-plugin-hello --target wasm32-wasip2`; the
//! component is `target/wasm32-wasip2/release/gate_plugin_hello.wasm`.

use gate_sdk::{Location, Rule, Run, schemars};

/// Reports each line that holds the marker.
#[derive(Debug, Default)]
pub struct Marker;

/// The options of `hello/marker`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkerOptions {
    /// The text that marks unfinished work.
    #[serde(default = "default_marker")]
    pub marker: String,
}

fn default_marker() -> String {
    "TODO".to_owned()
}

impl Rule for Marker {
    type Options = MarkerOptions;
    const NAME: &'static str = "marker";
    const TITLE: &'static str = "No file holds the marker of unfinished work";

    fn check(&self, run: &mut Run<'_, MarkerOptions>) -> gate_sdk::Result<()> {
        let marker = run.options().marker.clone();
        if marker.is_empty() {
            return Err(gate_sdk::Error::config(
                "hello/marker: the option marker is empty, so every line would match; set a marker or turn the rule off",
            ));
        }
        let tree = run.tree();
        for path in tree.paths() {
            let Some(text) = tree.text(&path) else {
                continue;
            };
            for (index, line) in text.lines().enumerate() {
                if line.contains(&marker) {
                    let number = index + 1;
                    run.report()
                        .finding(
                            "",
                            Location::line(path.clone(), number),
                            format!("the line holds the marker {marker}"),
                        )
                        .target(format!("{path}:{number}"))
                        .help("finish the work, or move it to the issue tracker and remove the marker");
                }
            }
        }
        Ok(())
    }
}

gate_sdk::plugin! {
    name: "hello",
    version: env!("CARGO_PKG_VERSION"),
    options: gate_sdk::NoOptions,
    rules: [Marker],
    generators: [],
    presets: [],
}
