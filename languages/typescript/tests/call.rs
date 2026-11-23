//! `TypeScriptCall`: the rules of one call read each file from the host once, whether a rule
//! parses it, reads its bytes through the call source or through the call's sources, in any order.

use std::cell::RefCell;
use std::collections::BTreeMap;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{CheckRequest, GitHost, Host, Location, Result, Rule, RuleRun, Run, TreeHost};
use gate_typescript::call::TypeScriptCall;

/// Options that take no key.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
struct NoOptions {}

/// Parses every source file and reports the ones that parse.
#[derive(Default)]
struct Parse;

impl Rule<TypeScriptCall> for Parse {
    type Options = NoOptions;
    const NAME: &'static str = "parse";
    const TITLE: &'static str = "Parses every source file";

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> Result<()> {
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            for path in ["src/a.ts", "src/b.ts", "src/c.ts"] {
                if typescript.parse(path)?.is_some() {
                    run.report()
                        .finding("", Location::file(path), "parsed".to_owned());
                }
            }
            Ok(())
        })
    }
}

/// Reads every source file's bytes, through the call source and the call's sources, and builds
/// the import bindings of the source root.
#[derive(Default)]
struct Read;

impl Rule<TypeScriptCall> for Read {
    type Options = NoOptions;
    const NAME: &'static str = "read";
    const TITLE: &'static str = "Reads every source file";

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> Result<()> {
        run.with_call(|run, shared| {
            for path in ["src/a.ts", "src/b.ts", "src/c.ts"] {
                let through_source = shared.source().bytes(path)?.map(|bytes| bytes.len());
                let through_sources = shared.bytes(path).map(|bytes| bytes.len());
                assert_eq!(through_source, through_sources, "{path}");
                run.report().finding(
                    "",
                    Location::file(path),
                    format!("{} bytes", through_source.unwrap_or_default()),
                );
            }
            let contexts = shared.contexts("src")?;
            run.report().finding(
                "",
                Location::file("src"),
                format!("{} contexts", contexts.iter().count()),
            );
            Ok(())
        })
    }
}

gate_sdk::plugin! {
    name: "probe",
    version: "0.0.0",
    options: NoOptions,
    call: TypeScriptCall,
    rules: [Parse, Read],
    generators: [],
    presets: [],
}

/// A tree that counts how often each path is read.
struct Counted {
    files: BTreeMap<&'static str, &'static [u8]>,
    reads: RefCell<BTreeMap<String, usize>>,
}

impl TreeHost for Counted {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .map(|path| (*path).to_owned())
            .collect()
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        *self.reads.borrow_mut().entry(path.to_owned()).or_default() += 1;
        self.files.get(path).map(|bytes| bytes.to_vec())
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Counted {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// One call of `rules`, in order: the findings and the reads of each path.
fn call(rules: &[&str]) -> (Vec<String>, BTreeMap<String, usize>) {
    let host = Counted {
        files: BTreeMap::from([
            (
                "src/a.ts",
                b"import { b } from './b';\nexport const a = b;\n".as_slice(),
            ),
            ("src/b.ts", b"export const b = 1;\n".as_slice()),
            // Not UTF-8: the view cannot parse it, and its bytes are still the file's.
            ("src/c.ts", b"export const c = '\xff';\n".as_slice()),
        ]),
        reads: RefCell::new(BTreeMap::new()),
    };
    let request = CheckRequest {
        project: String::new(),
        rules: rules
            .iter()
            .map(|name| RuleRun {
                name: (*name).to_owned(),
                options: "{}".to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: "{}".to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
        config_file: String::new(),
    };
    let findings = gate_sdk::check(&PLUGIN, &host, &request)
        .expect("the call runs")
        .into_iter()
        .map(|finding| {
            let path = match finding.location {
                Some(Location::File(file)) => file.path,
                other => format!("{other:?}"),
            };
            format!("{} {path}: {}", finding.rule, finding.message)
        })
        .collect();
    (findings, host.reads.into_inner())
}

#[test]
fn a_file_parsed_by_one_rule_and_read_by_another_is_read_from_the_host_once() {
    for order in [["parse", "read"], ["read", "parse"]] {
        let (findings, reads) = call(&order);
        assert_eq!(
            reads.values().copied().collect::<Vec<_>>(),
            [1, 1, 1],
            "{order:?}: {reads:?}"
        );
        let (alone, _) = call(&order[..1]);
        assert!(
            alone.iter().all(|finding| findings.contains(finding)),
            "{order:?}: {findings:?} lacks {alone:?}"
        );
        assert!(
            findings.contains(&"read src/c.ts: 22 bytes".to_owned()),
            "{findings:?}"
        );
    }
}
