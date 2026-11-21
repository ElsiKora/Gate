//! The cdk rules run natively through the SDK against synthetic trees: one app that follows the
//! standard passes every rule, and each aspect fails on an edit of it; the options that change
//! what is reported, and option errors.

mod app;
mod code;
mod health;
mod layout;
mod names;
mod options;
mod policy;
mod presets;
mod tsconfig_base;

use std::collections::BTreeMap;

use gate_sdk::serde_json::{self, Value};
use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// A fixture tree: tracked paths and their text, no git.
#[derive(Clone, Debug, Default)]
pub(crate) struct Files {
    files: BTreeMap<String, String>,
}

impl Files {
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
                .collect(),
        }
    }

    /// Adds or replaces a file.
    #[must_use]
    pub(crate) fn with(mut self, path: &str, text: &str) -> Self {
        self.files.insert(path.to_owned(), text.to_owned());
        self
    }

    /// Removes a file.
    #[must_use]
    pub(crate) fn without(mut self, path: &str) -> Self {
        assert!(
            self.files.remove(path).is_some(),
            "{path} is in the fixture"
        );
        self
    }

    /// Moves a file.
    #[must_use]
    pub(crate) fn moved(mut self, from: &str, to: &str) -> Self {
        let text = self.files.remove(from).expect("the file is in the fixture");
        self.files.insert(to.to_owned(), text);
        self
    }

    /// Replaces the one occurrence of `from` in `path`.
    #[must_use]
    pub(crate) fn edit(mut self, path: &str, from: &str, to: &str) -> Self {
        let text = self
            .files
            .get_mut(path)
            .expect("the file is in the fixture");
        assert_eq!(
            text.matches(from).count(),
            1,
            "{from:?} occurs once in {path}"
        );
        *text = text.replacen(from, to, 1);
        self
    }
}

impl TreeHost for Files {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).map(|text| text.as_bytes().to_vec())
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Files {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// One finding as the tests compare it: aspect, path, line and key.
pub(crate) type Seen = (String, String, Option<u32>, String);

/// Every rule of the plugin.
pub(crate) const RULES: [&str; 15] = [
    "layout",
    "file-names",
    "app",
    "environment",
    "declarations",
    "barrels",
    "stack-names",
    "construct-ids",
    "physical-names",
    "properties",
    "tags",
    "removal-policy",
    "nag",
    "container-health",
    "tests",
];

/// Runs `rules` with the plugin options (JSON) over `files`.
pub(crate) fn run(
    rules: &[&str],
    plugin_options: &str,
    files: &Files,
) -> gate_sdk::Result<Vec<Finding>> {
    let request = CheckRequest {
        project: String::new(),
        rules: rules
            .iter()
            .map(|rule| RuleRun {
                name: (*rule).to_owned(),
                options: String::new(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: plugin_options.to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
    };
    gate_sdk::check(&cdk::PLUGIN, files, &request)
}

/// The findings of `rule` over `files`, with the plugin options (JSON), as the tests compare
/// them, sorted.
pub(crate) fn findings(rule: &str, plugin_options: &str, files: &Files) -> Vec<Seen> {
    let mut seen: Vec<Seen> = run(&[rule], plugin_options, files)
        .expect("the rule runs")
        .into_iter()
        .map(|finding| {
            let Some(Location::File(location)) = finding.location else {
                panic!("every cdk finding names a file: {:?}", finding.location);
            };
            (
                finding.aspect,
                location.path,
                location.span.map(|span| span.line),
                finding.key.unwrap_or_default(),
            )
        })
        .collect();
    seen.sort();
    seen
}

/// The messages of `rule`'s findings over `files` with the default options.
pub(crate) fn messages(rule: &str, files: &Files) -> Vec<String> {
    run(&[rule], "{}", files)
        .expect("the rule runs")
        .into_iter()
        .map(|finding| finding.message)
        .collect()
}

/// The findings of `rule` over `files` with the default options.
pub(crate) fn check(rule: &str, files: &Files) -> Vec<Seen> {
    findings(rule, "{}", files)
}

/// The error of a run that must fail.
pub(crate) fn error(rule: &str, plugin_options: &str, files: &Files) -> String {
    run(&[rule], plugin_options, files)
        .expect_err("the run fails")
        .message()
        .to_owned()
}

/// One expected finding.
pub(crate) fn seen(aspect: &str, path: &str, line: Option<u32>, key: &str) -> Seen {
    (aspect.to_owned(), path.to_owned(), line, key.to_owned())
}

pub(crate) const ENTRY: &str = "bin/application.ts";
pub(crate) const STACK: &str = "lib/stack/network/network.stack.ts";
pub(crate) const DEPLOYMENT: &str = "lib/deployment/application.deployment.ts";
pub(crate) const TAGGING: &str = "lib/naming/apply-tags.function.ts";
pub(crate) const PACKAGE: &str = "package.json";
pub(crate) const CDK_JSON: &str = "cdk.json";

/// An app that follows the standard: every rule passes on it.
const APP: &[(&str, &str)] = &[
    (
        "cdk.json",
        r#"{
  "app": "npx ts-node --prefer-ts-exts bin/application.ts",
  "context": {
    "@aws-cdk/core:enableStackNameDuplicates": true
  }
}
"#,
    ),
    (
        "package.json",
        r#"{
  "name": "sample-infrastructure",
  "private": true,
  "scripts": {
    "lint": "eslint .",
    "synth": "cdk synth",
    "test": "jest"
  },
  "dependencies": {
    "aws-cdk-lib": "2.180.0",
    "cdk-nag": "3.0.0",
    "constructs": "10.4.2"
  },
  "devDependencies": {
    "aws-cdk": "catalog:"
  }
}
"#,
    ),
    (
        "tsconfig.json",
        "{ \"compilerOptions\": { \"strict\": true } }\n",
    ),
    (
        "guard/naming.guard",
        "rule names_have_the_project_prefix {}\n",
    ),
    (
        "deploy/staging.config.example.json",
        "{ \"environment\": \"staging\", \"account\": \"<account>\", \"destroyData\": true }\n",
    ),
    (
        "bin/application.ts",
        r#"import { App, Validations } from "aws-cdk-lib";
import { AwsSolutionsChecks } from "cdk-nag";

import { readConfiguration } from "../lib/config";
import { ApplicationDeployment } from "../lib/deployment";

const app = new App();
const configuration = readConfiguration(app.node.tryGetContext("configuration"));
new ApplicationDeployment(app, "Staging", { configuration });
Validations.of(app).addPlugins(new AwsSolutionsChecks(app, { verbose: true }));
"#,
    ),
    (
        "lib/config/index.ts",
        "export * from \"./interface\";\nexport * from \"./read-configuration.function\";\n",
    ),
    (
        "lib/config/interface/index.ts",
        "export * from \"./deployment-configuration.interface\";\n",
    ),
    (
        "lib/config/interface/deployment-configuration.interface.ts",
        "export interface IConfiguration {\n  readonly destroyData: boolean;\n  readonly environment: string;\n}\n",
    ),
    (
        "lib/config/read-configuration.function.ts",
        r#"import type { IConfiguration } from "./interface";

export function readConfiguration(value: unknown): IConfiguration {
  return value as IConfiguration;
}
"#,
    ),
    (
        "lib/naming/index.ts",
        "export * from \"./apply-tags.function\";\nexport * from \"./create-physical-name.function\";\nexport * from \"./create-stack-name.function\";\n",
    ),
    (
        "lib/naming/apply-tags.function.ts",
        r#"import { Tags } from "aws-cdk-lib";
import type { IConstruct } from "constructs";

export function applyTags(scope: IConstruct, environment: string, name: string): void {
  Tags.of(scope).add("Project", "sample");
  Tags.of(scope).add("Environment", environment);
  Tags.of(scope).add("ManagedBy", "cdk");
  Tags.of(scope).add("Application", "sample");
  Tags.of(scope).add("Name", name);
}
"#,
    ),
    (
        "lib/naming/create-physical-name.function.ts",
        "export function createPhysicalName(environment: string, name: string): string {\n  return `sample-${environment}-${name}`;\n}\n",
    ),
    (
        "lib/naming/create-stack-name.function.ts",
        "export function createStackName(environment: string, topic: string): string {\n  return `Sample-${environment}-${topic}`;\n}\n",
    ),
    (
        "lib/deployment/index.ts",
        "export * from \"./application.deployment\";\nexport * from \"./interface\";\n",
    ),
    (
        "lib/deployment/interface/index.ts",
        "export * from \"./application-deployment-properties.interface\";\n",
    ),
    (
        "lib/deployment/interface/application-deployment-properties.interface.ts",
        r#"import type { IConfiguration } from "../../config";

export interface IApplicationDeploymentProperties {
  readonly configuration: IConfiguration;
}
"#,
    ),
    (
        "lib/deployment/application.deployment.ts",
        r#"import { Stage } from "aws-cdk-lib";
import type { Construct } from "constructs";

import { createStackName } from "../naming";
import { NetworkStack } from "../stack/network";
import type { IApplicationDeploymentProperties } from "./interface";

export class ApplicationDeployment extends Stage {
  constructor(scope: Construct, id: string, properties: IApplicationDeploymentProperties) {
    super(scope, id);
    const stackName = createStackName(properties.configuration.environment, "network");
    new NetworkStack(this, stackName, { configuration: properties.configuration, stackName });
  }
}
"#,
    ),
    (
        "lib/stack/network/index.ts",
        "export * from \"./interface\";\nexport * from \"./network.stack\";\n",
    ),
    (
        "lib/stack/network/interface/index.ts",
        "export * from \"./properties.interface\";\n",
    ),
    (
        "lib/stack/network/interface/properties.interface.ts",
        r#"import type { StackProps } from "aws-cdk-lib";

import type { IConfiguration } from "../../../config";

export interface INetworkStackProperties extends StackProps {
  readonly configuration: IConfiguration;
}
"#,
    ),
    (
        "lib/stack/network/network.stack.ts",
        r#"import { RemovalPolicy, Stack } from "aws-cdk-lib";
import { Bucket } from "aws-cdk-lib/aws-s3";
import type { Construct } from "constructs";

import { applyTags, createPhysicalName } from "../../naming";
import type { INetworkStackProperties } from "./interface/properties.interface";

export class NetworkStack extends Stack {
  constructor(scope: Construct, id: string, properties: INetworkStackProperties) {
    super(scope, id, properties);
    const destroy = properties.configuration.destroyData;
    new Bucket(this, "FlowLogs", {
      autoDeleteObjects: destroy,
      bucketName: createPhysicalName(properties.configuration.environment, "flow-logs"),
      removalPolicy: destroy ? RemovalPolicy.DESTROY : RemovalPolicy.RETAIN,
    });
    applyTags(this, properties.configuration.environment, "network");
  }
}
"#,
    ),
    (
        "test/config/read-configuration.function.test.ts",
        "export {};\n",
    ),
    ("test/naming/apply-tags.function.test.ts", "export {};\n"),
    (
        "test/naming/create-physical-name.function.test.ts",
        "export {};\n",
    ),
    (
        "test/naming/create-stack-name.function.test.ts",
        "export {};\n",
    ),
    ("test/stack/network/network.stack.test.ts", "export {};\n"),
];

/// The app that follows the standard.
pub(crate) fn app() -> Files {
    Files::new(APP)
}

#[test]
fn the_standard_app_passes_every_rule() {
    let findings: Vec<String> = run(&RULES, "{}", &app())
        .expect("the rules run")
        .into_iter()
        .map(|finding| format!("{}#{} {}", finding.rule, finding.aspect, finding.message))
        .collect();
    assert_eq!(findings, Vec::<String>::new());
}

/// The rules of a preset of the plugin, as JSON.
fn preset(name: &str) -> Value {
    let info = gate_sdk::describe(&cdk::PLUGIN);
    let preset = info
        .presets
        .iter()
        .find(|preset| preset.name == name)
        .unwrap_or_else(|| panic!("cdk/{name}"));
    serde_json::from_str(&preset.rules).expect("JSON")
}

#[test]
fn both_presets_set_every_rule_at_their_one_level() {
    let mut every = RULES.to_vec();
    every.sort_unstable();
    for (name, level) in [("recommended", "error"), ("adoption", "warning")] {
        let rules = preset(name);
        let mut set: Vec<&str> = rules
            .as_object()
            .expect("rules")
            .keys()
            .map(String::as_str)
            .collect();
        set.sort_unstable();
        assert_eq!(set, every, "cdk/{name}");
        for rule in &RULES {
            assert_eq!(
                rules[*rule],
                serde_json::json!({ "level": level }),
                "cdk/{name} {rule}"
            );
        }
    }
}
