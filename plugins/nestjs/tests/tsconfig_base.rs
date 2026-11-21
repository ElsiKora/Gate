//! A project tsconfig that extends the install root's base, the layout `node/tsconfig` wants of a
//! project in a monorepo: the base is read through the tree when the plugin is granted it, and is
//! one finding with the grant to add when it is not, never an environment error.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// The tracked files of the project at `apps/api`, as its call lists and reads them: a granted
/// path outside the project with `..`.
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

const BASE: (&str, &str) = (
    "../../tsconfig.base.json",
    r#"{ "compilerOptions": { "strict": true, "baseUrl": ".", "paths": { "@api/*": ["apps/api/src/*"] } } }"#,
);

/// A service whose barrels are complete, and whose tsconfig extends the base.
const SERVICE: &[(&str, &str)] = &[
    (
        "tsconfig.json",
        r#"{ "extends": "../../tsconfig.base.json", "include": ["src"] }"#,
    ),
    ("src/modules/index.ts", "export * from \"./order\";\n"),
    (
        "src/modules/order/index.ts",
        "export { OrderModule } from \"./order.module\";\n",
    ),
    (
        "src/modules/order/order.module.ts",
        "export class OrderModule {}\n",
    ),
    (
        "src/runtime/api/main.ts",
        "export async function start(): Promise<void> {}\n",
    ),
];

fn run(files: &[(&str, &str)]) -> gate_sdk::Result<Vec<Finding>> {
    gate_sdk::check(
        &nestjs::PLUGIN,
        &Project {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
                .collect(),
        },
        &CheckRequest {
            project: "apps/api".to_owned(),
            rules: vec![RuleRun {
                name: "barrel-exports".to_owned(),
                options: r#"{ "private-entrypoints": ["src/runtime/api/main.ts"] }"#.to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: "{}".to_owned(),
            languages: "{}".to_owned(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
            config_file: String::new(),
        },
    )
}

/// Aspect, path and target of each finding.
fn rows(findings: &[Finding]) -> Vec<(String, String, Option<String>)> {
    findings
        .iter()
        .map(|finding| {
            let path = match &finding.location {
                Some(Location::File(file)) => file.path.clone(),
                _ => String::new(),
            };
            (finding.aspect.clone(), path, finding.target.clone())
        })
        .collect()
}

#[test]
fn a_service_whose_tsconfig_extends_a_granted_base_above_the_project_passes() {
    let files: Vec<_> = SERVICE.iter().copied().chain([BASE]).collect();

    assert_eq!(rows(&run(&files).expect("the rule runs")), []);
}

#[test]
fn a_base_the_plugin_cannot_read_is_a_finding_with_the_grant_to_add() {
    let findings = run(SERVICE).expect("the rule runs without an environment error");

    assert_eq!(
        rows(&findings),
        [(
            String::new(),
            "tsconfig.json".to_owned(),
            Some("tsconfig.json:../../tsconfig.base.json".to_owned())
        )]
    );
    let help = findings[0].help.as_deref().unwrap_or_default();
    assert!(help.contains(r#"reads = ["tsconfig.base.json"]"#), "{help}");
}
