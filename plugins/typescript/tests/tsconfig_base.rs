//! A project tsconfig that extends the install root's base, which the plugin is not granted: the
//! rules of one call share one TypeScript view, so the base is one finding of the call, never an
//! environment error and never one per rule.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, RuleRun, TreeHost};

/// The tracked files of the project at `apps/api`, as its call lists and reads them.
struct Project {
    files: BTreeMap<String, Vec<u8>>,
}

impl TreeHost for Project {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .cloned()
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Project {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// A service whose one import goes through its folder's barrel, and whose tsconfig extends the
/// root's base.
const SERVICE: &[(&str, &str)] = &[
    (
        "tsconfig.json",
        r#"{ "extends": "../../tsconfig.base.json", "include": ["src"] }"#,
    ),
    (
        "src/main.ts",
        "import { OrderModule } from \"./orders\";\nexport const modules = [OrderModule];\n",
    ),
    (
        "src/orders/index.ts",
        "export { OrderModule } from \"./order.module\";\n",
    ),
    (
        "src/orders/order.module.ts",
        "export class OrderModule {}\n",
    ),
];

/// Every rule of the plugin over `files`, as the call of the project at `apps/api` runs them.
fn run(files: &[(&str, &str)]) -> gate_sdk::Result<Vec<Finding>> {
    let request = CheckRequest {
        project: "apps/api".to_owned(),
        rules: ["parse", "barrel-imports", "string-enums"]
            .iter()
            .map(|rule| RuleRun {
                name: (*rule).to_owned(),
                options: String::new(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: "{}".to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
    };
    gate_sdk::check(
        &typescript::PLUGIN,
        &Project {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
                .collect(),
        },
        &request,
    )
}

#[test]
fn a_base_the_plugin_cannot_read_is_one_finding_of_the_call() {
    let findings: Vec<(String, Option<String>)> = run(SERVICE)
        .expect("the rules run without an environment error")
        .into_iter()
        .map(|finding| (finding.rule, finding.target))
        .collect();

    assert_eq!(
        findings,
        [(
            "barrel-imports".to_owned(),
            Some("tsconfig.json:../../tsconfig.base.json".to_owned())
        )]
    );
}
