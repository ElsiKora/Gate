//! The `cdk` plugin: one standard for every AWS CDK app written in TypeScript. The app is a project
//! of its own, its root the directory of `cdk.json`, and the rules check, without running the app,
//! its layout, file names, declarations, barrels, `cdk.json` and `package.json`, its accounts,
//! regions and configuration, stack names, construct ids, physical names, properties, tags,
//! removal policies, container health checks, cdk-nag registration and unit tests.
//!
//! Built for `wasm32-wasip2` it is the component `cdk.wasm`; natively it is a library the tests
//! call through [`PLUGIN`].

pub mod follow;
pub mod options;
pub mod rules;
pub mod sources;
pub mod standard;

pub use self::options::{Extension, PluginOptions};
pub use self::sources::Cdk;

use self::rules::app::App;
use self::rules::barrels::Barrels;
use self::rules::construct_ids::ConstructIds;
use self::rules::container_health::ContainerHealth;
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

/// Every rule at `error`, for an app that follows the standard.
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
    "container-health": { "level": "error" },
    "tests": { "level": "error" }
}"#;

/// The rules of `recommended` at `warning`, for an app on its way to the standard.
const ADOPTION: &str = r#"{
    "layout": { "level": "warning" },
    "file-names": { "level": "warning" },
    "app": { "level": "warning" },
    "environment": { "level": "warning" },
    "declarations": { "level": "warning" },
    "barrels": { "level": "warning" },
    "stack-names": { "level": "warning" },
    "construct-ids": { "level": "warning" },
    "physical-names": { "level": "warning" },
    "properties": { "level": "warning" },
    "tags": { "level": "warning" },
    "removal-policy": { "level": "warning" },
    "nag": { "level": "warning" },
    "container-health": { "level": "warning" },
    "tests": { "level": "warning" }
}"#;

gate_sdk::plugin! {
    name: "cdk",
    version: env!("CARGO_PKG_VERSION"),
    options: PluginOptions,
    call: Cdk,
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
        ContainerHealth,
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
