//! The `cdk` plugin: one standard for every AWS CDK app written in TypeScript. The app is a project
//! of its own, its root the directory of `cdk.json`, and the rules check, without running the app,
//! its layout, file names, declarations, barrels, `cdk.json` and `package.json`, its accounts,
//! regions and configuration, stack names, construct ids, physical names, properties, tags,
//! removal policies, cdk-nag registration and unit tests.
//!
//! Built for `wasm32-wasip2` it is the component `cdk.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

pub mod options;
pub mod rules;
pub mod sources;
pub mod standard;

pub use self::options::{Extension, PluginOptions};
pub use self::sources::Cdk;

use self::rules::app::App;
use self::rules::barrels::Barrels;
use self::rules::construct_ids::ConstructIds;
use self::rules::declarations::Declarations;
use self::rules::environment::Environment;
use self::rules::file_names::FileNames;
use self::rules::layout::Layout;
use self::rules::nag::Nag;
use self::rules::physical_names::PhysicalNames;
use self::rules::properties::Properties;
use self::rules::removal_policy::RemovalPolicy;
use self::rules::stack_names::StackNames;
use self::rules::tags::Tags;
use self::rules::tests::Tests;

/// Every rule at `error`.
const RECOMMENDED: &str = r#"{
    "layout": { "level": "error" },
    "file-names": { "level": "error" },
    "app": { "level": "error" },
    "environment": { "level": "error" },
    "declarations": { "level": "error" },
    "barrels": { "level": "error" },
    "stack-names": { "level": "error" },
    "construct-ids": { "level": "error" },
    "physical-names": { "level": "error" },
    "properties": { "level": "error" },
    "tags": { "level": "error" },
    "removal-policy": { "level": "error" },
    "nag": { "level": "error" },
    "tests": { "level": "error" }
}"#;

/// `recommended`, with the aspects that rename files and classes at `warning`, so an existing app
/// fixes what protects a deploy first and renames in its own commits.
const ADOPTION: &str = r#"{
    "layout": { "level": "error" },
    "file-names": { "level": "warning" },
    "app": { "level": "error" },
    "environment": { "level": "error" },
    "declarations": { "level": "warning" },
    "barrels": { "level": "warning" },
    "stack-names": { "level": "error", "levels": { "topic": "warning" } },
    "construct-ids": { "level": "error" },
    "physical-names": { "level": "error" },
    "properties": { "level": "warning" },
    "tags": { "level": "error" },
    "removal-policy": { "level": "error" },
    "nag": { "level": "error" },
    "tests": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "cdk",
    version: env!("CARGO_PKG_VERSION"),
    options: crate::options::PluginOptions,
    call: crate::sources::Cdk,
    rules: [
        Layout,
        FileNames,
        App,
        Environment,
        Declarations,
        Barrels,
        StackNames,
        ConstructIds,
        PhysicalNames,
        Properties,
        Tags,
        RemovalPolicy,
        Nag,
        Tests,
    ],
    generators: [],
    presets: [
        gate_sdk::Preset {
            name: "recommended",
            rules: RECOMMENDED,
        },
        gate_sdk::Preset {
            name: "adoption",
            rules: ADOPTION,
        },
    ],
}
