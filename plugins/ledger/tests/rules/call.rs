//! The rules of one call share the call state: each ledger file is read from the host once, and
//! each rule finds in the shared call what it finds alone.

use std::cell::RefCell;
use std::collections::BTreeMap;

use gate_sdk::{CheckRequest, GitHost, Host, RuleRun, TreeHost};

use crate::{Files, IMPLEMENTATION, PARITY, findings, ledger};

/// A fixture tree that counts how often each path is read.
struct Counted {
    files: Files,
    reads: RefCell<BTreeMap<String, usize>>,
}

impl TreeHost for Counted {
    fn list(&self, prefix: &str) -> Vec<String> {
        self.files.list(prefix)
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        *self.reads.borrow_mut().entry(path.to_owned()).or_default() += 1;
        self.files.read(path)
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

const RULES: [&str; 3] = ["blockers", "records", "record-status"];

#[test]
fn the_rules_of_one_call_read_each_ledger_file_once_and_find_what_they_find_alone() {
    // A planned record blocked by a record that is not closed: a finding to share.
    let files = ledger().edit(
        IMPLEMENTATION,
        "  - Blocked by: none\n  - Reuses",
        "  - Blocked by: `I-ALPHA-ONE-002`\n  - Reuses",
    );
    let host = Counted {
        files: files.clone(),
        reads: RefCell::new(BTreeMap::new()),
    };
    let request = CheckRequest {
        project: String::new(),
        rules: RULES
            .iter()
            .map(|name| RuleRun {
                name: (*name).to_owned(),
                options: String::new(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: PARITY.to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
        config_file: String::new(),
    };
    let together: Vec<(String, String)> = gate_sdk::check(&ledger::PLUGIN, &host, &request)
        .expect("the call runs")
        .into_iter()
        .map(|finding| (finding.rule, finding.message))
        .collect();

    let reads = host.reads.into_inner();
    assert!(!reads.is_empty());
    assert!(
        reads.values().all(|count| *count == 1),
        "a file read more than once: {reads:?}"
    );

    let alone: Vec<(String, String)> = RULES
        .iter()
        .flat_map(|rule| {
            findings(rule, "", PARITY, &files)
                .into_iter()
                .map(|(_, _, _, message)| ((*rule).to_owned(), message))
        })
        .collect();
    assert!(!alone.is_empty(), "the fixture has a finding");
    assert_eq!(together, alone);
}
