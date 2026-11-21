//! An app tsconfig that extends the install root's base, the layout `node/tsconfig` wants of a
//! project in a monorepo: the base is read through the tree when the plugin is granted it, so its
//! alias resolves, and is one finding with the grant to add when it is not, never an environment
//! error.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, TreeHost};

/// The tracked files of the project at `apps/web`, as its call lists and reads them: a granted
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
    r#"{ "compilerOptions": { "strict": true, "paths": { "@/*": ["./apps/web/src/*"] } } }"#,
);

/// An app whose tsconfig extends the base, which holds the app's one alias; a widget imports a
/// page, a layer above it, through that alias.
const APP: &[(&str, &str)] = &[
    (
        "tsconfig.json",
        r#"{ "extends": "../../tsconfig.base.json", "include": ["app", "src"] }"#,
    ),
    (
        "src/pages/home/index.ts",
        "import { Header } from \"@/widgets/header\";\nexport const Home = Header;\n",
    ),
    (
        "src/widgets/header/index.ts",
        "import { Home } from \"@/pages/home\";\nexport const Header = Home;\n",
    ),
];

fn run(files: &[(&str, &str)]) -> gate_sdk::Result<Vec<Finding>> {
    gate_sdk::check(
        &frontend::PLUGIN,
        &Project {
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
                .collect(),
        },
        &CheckRequest {
            project: "apps/web".to_owned(),
            rules: vec![RuleRun {
                name: "import-direction".to_owned(),
                options: "{}".to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            }],
            plugin_options: r#"{ "pack": "nextjs" }"#.to_owned(),
            languages: "{}".to_owned(),
            norms: Vec::new(),
            rule_decisions: Vec::new(),
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
fn an_app_whose_alias_is_in_a_granted_base_above_the_project_resolves_it() {
    let files: Vec<_> = APP.iter().copied().chain([BASE]).collect();

    // The widget's import of the page resolves through the base's alias, so it is reported.
    let found: Vec<_> = rows(&run(&files).expect("the rule runs"))
        .into_iter()
        .map(|(aspect, path, _)| (aspect, path))
        .collect();
    assert_eq!(
        found,
        [("layer".to_owned(), "src/widgets/header/index.ts".to_owned())]
    );
}

#[test]
fn a_base_the_plugin_cannot_read_is_a_finding_with_the_grant_to_add() {
    let findings = run(APP).expect("the rule runs without an environment error");

    // Without the base the alias resolves to nothing, so only the base is reported.
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
