//! `typescript/parse` run natively through the SDK against synthetic trees: one file that parses
//! and one that does not per extension class, the non-UTF-8 case, the extension filter and its
//! errors.

use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, Finding, GitHost, Host, Location, RuleRun, Span, TreeHost};

/// A fixture tree: tracked paths and their bytes, no git.
struct Fixture {
    files: BTreeMap<String, Vec<u8>>,
}

impl Fixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
                .collect(),
        }
    }
}

impl TreeHost for Fixture {
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

impl Host for Fixture {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

fn request(options: &str) -> CheckRequest {
    request_with(options, "{}")
}

fn request_with(options: &str, languages: &str) -> CheckRequest {
    CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: "parse".to_owned(),
            options: options.to_owned(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: "{}".to_owned(),
        languages: languages.to_owned(),
        norms: Vec::new(),
    }
}

fn check(files: &[(&str, &[u8])], options: &str) -> gate_sdk::Result<Vec<Finding>> {
    gate_sdk::check(&typescript::PLUGIN, &Fixture::new(files), &request(options))
}

/// The path and line of each finding.
fn located(findings: &[Finding]) -> Vec<(String, Option<u32>)> {
    findings
        .iter()
        .map(|finding| {
            let Some(Location::File(file)) = &finding.location else {
                panic!("a finding names its file: {finding:?}");
            };
            (file.path.clone(), file.span.as_ref().map(|span| span.line))
        })
        .collect()
}

const BROKEN: &[u8] = b"export const a = 1;\nconst = ;\n";

/// One file that parses and one that does not for each extension class.
const CLASSES: &[(&str, &[u8])] = &[
    ("lib/broken.cjs", b"module.exports = { a: 1 };\nconst = ;\n"),
    ("lib/broken.js", BROKEN),
    ("lib/broken.mjs", BROKEN),
    ("lib/good.cjs", b"module.exports = { a: 1 };\n"),
    ("lib/good.js", b"export const a = 1;\n"),
    ("lib/good.mjs", b"export const a = 1;\n"),
    ("src/broken.cts", BROKEN),
    ("src/broken.d.ts", b"declare const = ;\n"),
    ("src/broken.mts", BROKEN),
    ("src/broken.ts", BROKEN),
    ("src/good.cts", b"export const a: number = 1;\n"),
    ("src/good.d.ts", b"declare const a: number;\n"),
    ("src/good.mts", b"export const a: number = 1;\n"),
    ("src/good.ts", b"export const a: number = 1;\n"),
    ("ui/broken.jsx", b"export const v = <div>;\n"),
    ("ui/broken.tsx", b"export const v = <div>;\n"),
    ("ui/good.jsx", b"export const v = <div />;\n"),
    ("ui/good.tsx", b"export const v: unknown = <div />;\n"),
];

#[test]
fn every_file_that_cannot_be_parsed_is_reported_once_at_its_first_error() {
    let findings = check(CLASSES, "{}").expect("runs");

    assert_eq!(
        located(&findings),
        [
            ("lib/broken.cjs".to_owned(), Some(2)),
            ("lib/broken.js".to_owned(), Some(2)),
            ("lib/broken.mjs".to_owned(), Some(2)),
            ("src/broken.cts".to_owned(), Some(2)),
            ("src/broken.d.ts".to_owned(), Some(1)),
            ("src/broken.mts".to_owned(), Some(2)),
            ("src/broken.ts".to_owned(), Some(2)),
            ("ui/broken.jsx".to_owned(), Some(1)),
            ("ui/broken.tsx".to_owned(), Some(1)),
        ]
    );
    let first = &findings[0];
    assert_eq!(
        match &first.location {
            Some(Location::File(file)) => file.span,
            _ => None,
        },
        Some(Span {
            line: 2,
            column: 7,
            end_line: 2,
            end_column: 7
        })
    );
    assert!(
        first
            .help
            .as_deref()
            .is_some_and(|help| help.contains("fix the syntax error"))
    );
    assert_eq!(first.rule, "parse");
    assert_eq!(first.aspect, "");
    assert_eq!(first.target.as_deref(), Some("lib/broken.cjs"));
    assert!(
        first.message.starts_with(
            "lib/broken.cjs cannot be parsed, so the checks that read its syntax tree skip it: "
        ),
        "{}",
        first.message
    );
}

#[test]
fn a_tree_that_parses_has_no_finding() {
    let good: Vec<(&str, &[u8])> = CLASSES
        .iter()
        .copied()
        .filter(|(path, _)| !path.contains("broken"))
        .collect();

    assert_eq!(check(&good, "{}").expect("runs"), []);
}

#[test]
fn a_file_that_is_not_utf8_is_reported_without_a_line() {
    let findings = check(
        &[
            ("media/clip.js", &[0xff, 0xfe, 0x00]),
            ("media/clip.ts", &[0x47, 0x40, 0xff, 0xfe, 0x00]),
        ],
        "{}",
    )
    .expect("runs");

    assert_eq!(
        findings
            .iter()
            .map(|finding| (&finding.location, finding.message.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                &Some(Location::file("media/clip.js")),
                "media/clip.js cannot be parsed, so the checks that read its syntax tree skip it: the file is not UTF-8 text"
            ),
            (
                &Some(Location::file("media/clip.ts")),
                "media/clip.ts cannot be parsed, so the checks that read its syntax tree skip it: the file is not UTF-8 text"
            ),
        ]
    );
}

#[test]
fn only_lowercase_source_extensions_are_files_of_the_rule() {
    let files: &[(&str, &[u8])] = &[
        ("src/Upper.TS", BROKEN),
        ("src/notes.md", BROKEN),
        ("src/data.json", BROKEN),
        ("src/noextension", BROKEN),
    ];

    assert_eq!(check(files, "{}").expect("runs"), []);
}

#[test]
fn the_extensions_option_narrows_the_files() {
    let findings = check(CLASSES, r#"{ "extensions": ["tsx", "mts"] }"#).expect("runs");

    assert_eq!(
        located(&findings),
        [
            ("src/broken.mts".to_owned(), Some(2)),
            ("ui/broken.tsx".to_owned(), Some(1)),
        ]
    );
}

#[test]
fn an_extension_oxc_does_not_parse_is_an_error() {
    let error = check(CLASSES, r#"{ "extensions": ["vue"] }"#).expect_err("fails");

    assert!(error.message().contains("`vue`"), "{error}");
}

#[test]
fn an_unknown_option_is_an_error() {
    let error = check(CLASSES, r#"{ "extension": ["ts"] }"#).expect_err("fails");

    assert!(error.message().contains("extension"), "{error}");
}

#[test]
fn the_plugin_declares_its_rules_for_the_typescript_language() {
    let info = gate_sdk::describe(&typescript::PLUGIN);
    let rule = info.rules.first().expect("the parse rule");

    assert_eq!((info.name.as_str(), info.rules.len()), ("typescript", 3));
    assert_eq!(rule.name, "parse");
    assert_eq!(rule.languages, ["typescript"]);
    assert!(
        rule.options_schema.contains("extensions"),
        "{}",
        rule.options_schema
    );
}

#[test]
fn an_empty_file_parses() {
    assert_eq!(
        check(&[("src/empty.ts", b""), ("lib/empty.cjs", b"")], "{}").expect("runs"),
        []
    );
}

#[test]
fn a_mistake_in_the_language_settings_is_an_error() {
    let host = Fixture::new(CLASSES);
    let accepted = gate_sdk::check(
        &typescript::PLUGIN,
        &host,
        &request_with("{}", r#"{ "typescript": { "skip-folders": ["out"] } }"#),
    );
    let error = gate_sdk::check(
        &typescript::PLUGIN,
        &host,
        &request_with("{}", r#"{ "typescript": { "skip_folders": ["out"] } }"#),
    )
    .expect_err("fails");

    assert_eq!(accepted.expect("runs").len(), 9);
    assert!(
        error.message().contains("[languages.typescript]"),
        "{error}"
    );
}
