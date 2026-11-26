//! The ledger rules run natively through the SDK against synthetic trees: each rule's pass, each
//! aspect's fail, the options that change what is reported, and option errors.

mod blockers;
mod call;
mod closing_notes;
mod config_decisions;
mod decisions;
mod exceptions;
mod generators;
mod layout;
mod manifest;
mod presence;
mod presets;
mod record_status;
mod records;
mod reuses;

use std::collections::BTreeMap;

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

    /// Every file under `prefix`, as another project's files appear through `reads`.
    #[must_use]
    pub(crate) fn renamed(self, prefix: &str) -> Self {
        Self {
            files: self
                .files
                .into_iter()
                .map(|(path, text)| (format!("{prefix}{path}"), text))
                .collect(),
        }
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

/// One finding as the tests compare it: aspect, path, line, message.
pub(crate) type Seen = (String, String, Option<u32>, String);

/// Runs `rule` with its options and the plugin options (JSON) over `files`.
pub(crate) fn run(
    rule: &str,
    options: &str,
    plugin_options: &str,
    files: &Files,
) -> gate_sdk::Result<Vec<Seen>> {
    run_in(rule, options, plugin_options, "", files)
}

/// Runs `rule` as [`run`] does, in a call whose own `gate.toml` is `config_file` (empty: none, as
/// in the repository call under a root project).
pub(crate) fn run_in(
    rule: &str,
    options: &str,
    plugin_options: &str,
    config_file: &str,
    files: &Files,
) -> gate_sdk::Result<Vec<Seen>> {
    let request = CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: rule.to_owned(),
            options: options.to_owned(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: plugin_options.to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
        config_file: config_file.to_owned(),
    };
    gate_sdk::check(&ledger::PLUGIN, files, &request).map(|findings| {
        findings
            .into_iter()
            .map(|finding: Finding| {
                let Some(Location::File(location)) = finding.location else {
                    panic!("every ledger finding names a file: {:?}", finding.location);
                };
                (
                    finding.aspect,
                    location.path,
                    location.span.map(|span| span.line),
                    finding.message,
                )
            })
            .collect()
    })
}

/// One finding's identity as baselines see it: aspect, path, line and key.
pub(crate) type Keyed = (String, String, Option<u32>, Option<String>);

/// The findings of a run that must succeed, with their keys.
pub(crate) fn keys(rule: &str, options: &str, plugin_options: &str, files: &Files) -> Vec<Keyed> {
    let request = CheckRequest {
        project: String::new(),
        rules: vec![RuleRun {
            name: rule.to_owned(),
            options: options.to_owned(),
            options_file: None,
            exceptions: Vec::new(),
        }],
        plugin_options: plugin_options.to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
        config_file: String::new(),
    };
    gate_sdk::check(&ledger::PLUGIN, files, &request)
        .expect("the rule runs")
        .into_iter()
        .map(|finding: Finding| {
            let Some(Location::File(location)) = finding.location else {
                panic!("every ledger finding names a file: {:?}", finding.location);
            };
            (
                finding.aspect,
                location.path,
                location.span.map(|span| span.line),
                finding.key,
            )
        })
        .collect()
}

/// The findings of a run that must succeed.
pub(crate) fn findings(
    rule: &str,
    options: &str,
    plugin_options: &str,
    files: &Files,
) -> Vec<Seen> {
    run(rule, options, plugin_options, files).expect("the rule runs")
}

/// The findings of a run that must succeed, in a call whose own `gate.toml` is `config_file`.
pub(crate) fn findings_in(
    rule: &str,
    options: &str,
    plugin_options: &str,
    config_file: &str,
    files: &Files,
) -> Vec<Seen> {
    run_in(rule, options, plugin_options, config_file, files).expect("the rule runs")
}

/// The error of a run that must fail.
pub(crate) fn error(rule: &str, options: &str, plugin_options: &str, files: &Files) -> String {
    run(rule, options, plugin_options, files)
        .expect_err("the run fails")
        .message()
        .to_owned()
}

/// One expected finding.
pub(crate) fn seen(aspect: &str, path: &str, line: Option<u32>, message: &str) -> Seen {
    (aspect.to_owned(), path.to_owned(), line, message.to_owned())
}

/// The `#input` finding of `ledger/<rule>` for the untracked `path` that `option` names, as
/// `[plugins.ledger] options <key>` or `its option <key>`.
pub(crate) fn input(rule: &str, path: &str, option: &str) -> Seen {
    seen(
        "input",
        path,
        None,
        &format!(
            "{path} is not tracked, and ledger/{rule} reads it ({option}); create it, point the option at the file, or turn the input off with a decision: \"ledger/{rule}\" = {{ levels = {{ input = \"off\" }}, decision = \"<record>\" }}"
        ),
    )
}

/// The parity options of the synthetic ledger.
pub(crate) const PARITY: &str = r#"{"parity":{"plan":"docs/ledger/plans/alpha.md","prefix":"ALPHA","label":"Alpha","reference":"Upstream reference","products":["Base","Alpha"],"fields":["Parity evidence"],"active-status":"in progress"}}"#;

pub(crate) const OID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(crate) const OID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

pub(crate) const PLAN: &str = "docs/ledger/plans/alpha.md";
pub(crate) const IMPLEMENTATION: &str = "docs/ledger/implementation-slices.md";
pub(crate) const VERIFICATION: &str = "docs/ledger/verification.md";
pub(crate) const FOLLOW_UPS: &str = "docs/ledger/follow-ups.md";
pub(crate) const BLOCKERS: &str = "docs/ledger/production-blockers.md";

/// A synthetic ledger every record rule passes on with [`PARITY`]: one completed paired record,
/// one planned record, one follow-up and one open production blocker.
pub(crate) fn ledger() -> Files {
    Files::new(&[
        (
            PLAN,
            "# Alpha plan\n\
             \n\
             ### I-ALPHA-ONE-001: First slice\n\
             \n\
             - **Status:** `completed`\n\
             - **Blocked by:** none\n\
             - **Owning repository:** app\n\
             - **Commit:** `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`\n",
        ),
        (
            IMPLEMENTATION,
            "# Implementation\n\
             \n\
             - `I-ALPHA-ONE-001`: First slice\n\
             \x20 - Status: `completed`\n\
             \x20 - Blocked by: none\n\
             \x20 - Verification items: `V-ALPHA-ONE-001`\n\
             \x20 - Commit: `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`\n\
             \x20 - Owning repository: app\n\
             - `I-BASE-002`: Base work\n\
             \x20 - Status: `planned`\n\
             \x20 - Blocked by: none\n\
             \x20 - Reuses: none; this work builds the first parser itself.\n",
        ),
        (
            VERIFICATION,
            "# Verification\n\
             \n\
             - `V-ALPHA-ONE-001`: First slice checks\n\
             \x20 - Status: `completed`\n\
             \x20 - Verification notes: the unit tests pass on the merged branch\n",
        ),
        (
            FOLLOW_UPS,
            "# Follow-ups\n\
             \n\
             - `F-ONE`: Status `done`; the tidy-up landed\n",
        ),
        (
            BLOCKERS,
            "# Production blockers\n\
             \n\
             - `PB-ONE`: Rollout guard\n\
             \x20 - Phase: `LAUNCH`\n\
             \x20 - Status: `open`\n\
             \x20 - Exit evidence: pending\n\
             \x20 - Prerequisite links: `I-BASE-002`\n\
             \x20 - Owner: platform\n\
             \x20 - Ledger links: none\n\
             \x20 - Exit criteria: the guard is live\n",
        ),
    ])
}

#[test]
fn an_option_path_must_be_in_normal_form() {
    for (rule, options, plugin, message) in [
        (
            "blockers",
            "",
            r#"{"implementation":"./docs/ledger/implementation-slices.md"}"#,
            r#"[plugins.ledger] options implementation "./docs/ledger/implementation-slices.md" is not in normal form; write "docs/ledger/implementation-slices.md""#,
        ),
        (
            "records",
            "",
            r#"{"decisions":"../shared/../shared/docs/index.md"}"#,
            r#"[plugins.ledger] options decisions "../shared/../shared/docs/index.md" is not in normal form; write "../shared/docs/index.md""#,
        ),
        (
            "blockers",
            "",
            r#"{"blockers":"/docs/ledger/production-blockers.md"}"#,
            r#"[plugins.ledger] options blockers "/docs/ledger/production-blockers.md" is absolute; write it relative to the project root"#,
        ),
        (
            "manifest",
            r#"{"manifest":"docs//ledger/manifest.json"}"#,
            "",
            r#"ledger/manifest option manifest "docs//ledger/manifest.json" is not in normal form; write "docs/ledger/manifest.json""#,
        ),
        (
            "decisions",
            r#"{"coverage":"docs/adr/../adr/decision-coverage.md"}"#,
            "",
            r#"ledger/decisions option coverage "docs/adr/../adr/decision-coverage.md" is not in normal form; write "docs/adr/decision-coverage.md""#,
        ),
        (
            "exception-owners",
            r#"{"exceptions":"config/./gate.exceptions.toml"}"#,
            "",
            r#"ledger/exception-owners option exceptions "config/./gate.exceptions.toml" is not in normal form; write "config/gate.exceptions.toml""#,
        ),
        (
            "reuses",
            r#"{"catalog":"docs/reference/mechanisms.md/"}"#,
            "",
            r#"ledger/reuses option catalog "docs/reference/mechanisms.md/" is not in normal form; write "docs/reference/mechanisms.md""#,
        ),
    ] {
        assert_eq!(
            error(rule, options, plugin, &ledger()),
            format!("rule {rule}: {message}"),
            "{rule}"
        );
    }
    assert_eq!(
        findings(
            "reuses",
            r#"{"catalog":"docs/reference/mechanisms.md","exclude":["docs/ledger/implementation-slices/a/"]}"#,
            "",
            &ledger()
        )
        .len(),
        2,
        "a trailing / on an excluded directory is accepted; the missing catalog and index are reported"
    );
}

#[test]
fn a_closed_status_must_be_lowercase() {
    assert_eq!(
        error(
            "blockers",
            "",
            r#"{"closed-statuses":["done","Verified"]}"#,
            &ledger()
        ),
        r#"rule blockers: [plugins.ledger] options closed-statuses has "Verified"; statuses are compared in lowercase, so write "verified""#
    );
}

#[test]
fn rule_options_are_checked_even_when_the_ledger_is_not_tracked() {
    let empty = Files::default();
    for (rule, options, key) in [
        (
            "records",
            r#"{"pending-patterns":["("]}"#,
            "pending-patterns",
        ),
        ("record-status", r#"{"allowed":[]}"#, "allowed"),
        (
            "reuses",
            r#"{"catalog":"docs/reference/mechanisms.md","generic-reasons":"("}"#,
            "generic-reasons",
        ),
        (
            "closing-notes",
            r#"{"field":"`DELTA` notes","entry-label-letters":["ab"]}"#,
            "entry-label-letters",
        ),
    ] {
        let message = error(rule, options, "", &empty);
        assert!(message.contains(key), "{rule}: {message}");
    }
}
