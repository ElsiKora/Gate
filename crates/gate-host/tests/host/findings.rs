//! Findings: exceptions, order, report formats and exit codes.

use gate_host::config::{RuleId, RuleRef};
use gate_host::{
    Baseline, Decision, Finding, Findings, Format, Level, Location, Reason, Report, RulesRan,
    ScopedException, Selection, Span,
};

fn finding(
    rule: &str,
    aspect: &str,
    level: Level,
    path: &str,
    line: u32,
    message: &str,
) -> Finding {
    let mut finding = Finding::new(&RuleId::parse(rule).expect("rule id"), level, message);
    aspect.clone_into(&mut finding.aspect);
    finding.location = Some(Location::File {
        path: path.to_owned(),
        span: Some(Span {
            line,
            column: 1,
            end_line: line,
            end_column: 5,
        }),
    });
    finding.project = Some("server".to_owned());
    finding
}

/// Every rule these tests use ran in the projects `server` and `client`.
fn ran_all() -> RulesRan {
    let mut ran = RulesRan::new();
    for project in ["server", "client"] {
        for rule in ["layers/relation", "a/rule", "b/rule", "z/rule"] {
            ran.insert(Some(project), rule);
        }
    }
    ran
}

/// An exception of `apps/server`; `target` is written relative to it.
fn exception(rule: &str, target: &str) -> ScopedException {
    ScopedException {
        file: "apps/server/gate.exceptions.toml".to_owned(),
        line: 12,
        rule: RuleRef::try_from(rule.to_owned()).expect("rule ref"),
        target: target.to_owned(),
        path: Some(format!("apps/server/{target}")),
        reason: "accepted".to_owned(),
        decision: None,
        owner: None,
        project: Some("server".to_owned()),
    }
}

fn report(findings: Vec<Finding>) -> Report {
    Report {
        mode: "tree".to_owned(),
        rules: 2,
        findings,
        selection: Selection {
            ran: vec![Decision {
                project: "server".to_owned(),
                reason: Reason::Full,
            }],
            skipped: Vec::new(),
            repository: true,
        },
        overrides: Vec::new(),
    }
}

#[test]
fn findings_are_ordered_by_path_line_column_rule_aspect_message() {
    let mut sink = Findings::new();
    sink.extend([
        finding("b/rule", "", Level::Error, "src/b.ts", 1, "m"),
        finding("a/rule", "y", Level::Error, "src/a.ts", 2, "m"),
        finding("a/rule", "x", Level::Error, "src/a.ts", 2, "z"),
        finding("a/rule", "x", Level::Error, "src/a.ts", 2, "a"),
        finding("z/rule", "", Level::Warning, "src/a.ts", 1, "m"),
        finding("a/rule", "", Level::Note, "src/a.ts", 10, "m"),
    ]);
    let order: Vec<String> = sink
        .finish(&[], &Baseline::default(), &ran_all())
        .iter()
        .map(|finding| {
            let span = finding
                .location
                .as_ref()
                .and_then(Location::span)
                .expect("span");
            format!(
                "{}:{} {} {}",
                finding.location.as_ref().expect("location").key(),
                span.line,
                finding.rule_with_aspect(),
                finding.message
            )
        })
        .collect();
    assert_eq!(
        order,
        [
            "src/a.ts:1 z/rule m",
            "src/a.ts:2 a/rule#x a",
            "src/a.ts:2 a/rule#x z",
            "src/a.ts:2 a/rule#y m",
            "src/a.ts:10 a/rule m",
            "src/b.ts:1 b/rule m",
        ]
    );
}

#[test]
fn an_exception_drops_the_findings_it_matches() {
    let mut on_target = finding(
        "layers/relation",
        "nullable",
        Level::Error,
        "apps/server/src/user.ts",
        3,
        "nullable",
    );
    on_target.target = Some("src/user.ts:User.parent".to_owned());
    let mut other_target = on_target.clone();
    other_target.target = Some("src/user.ts:User.child".to_owned());
    let other_aspect = finding(
        "layers/relation",
        "eager",
        Level::Error,
        "apps/server/src/user.ts",
        4,
        "eager",
    );

    let mut sink = Findings::new();
    sink.extend([on_target, other_target.clone(), other_aspect.clone()]);
    let kept = sink.finish(
        &[exception(
            "layers/relation#nullable",
            "src/user.ts:User.parent",
        )],
        &Baseline::default(),
        &ran_all(),
    );
    assert_eq!(kept, [other_target, other_aspect]);
}

#[test]
fn an_exception_matches_the_path_of_findings_without_a_target() {
    let mut targeted = finding(
        "layers/relation",
        "a",
        Level::Error,
        "apps/server/x.ts",
        3,
        "targeted",
    );
    targeted.target = Some("src/x.ts:X.a".to_owned());
    let mut sink = Findings::new();
    sink.extend([
        finding(
            "layers/relation",
            "a",
            Level::Error,
            "apps/server/x.ts",
            1,
            "one",
        ),
        finding(
            "layers/relation",
            "b",
            Level::Error,
            "apps/server/x.ts",
            2,
            "two",
        ),
        targeted,
        finding(
            "layers/relation",
            "",
            Level::Error,
            "apps/server/y.ts",
            2,
            "other file",
        ),
    ]);
    let kept = sink.finish(
        &[exception("layers/relation", "x.ts")],
        &Baseline::default(),
        &ran_all(),
    );
    let messages: Vec<&str> = kept
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        ["targeted", "other file"],
        "a finding with a target is matched by its target only"
    );
}

#[test]
fn an_unmatched_exception_is_reported_when_its_rule_ran() {
    let unmatched = exception("layers/relation", "src/gone.ts:Gone.field");
    let kept = Findings::new().finish(
        std::slice::from_ref(&unmatched),
        &Baseline::default(),
        &ran_all(),
    );
    assert_eq!(kept.len(), 1);
    let reported = &kept[0];
    assert_eq!(reported.rule, "gate/exceptions");
    assert_eq!(reported.level, Level::Error);
    assert_eq!(
        reported.message,
        "the exception for layers/relation at src/gone.ts:Gone.field matches no finding"
    );
    let location = reported.location.as_ref().expect("location");
    assert_eq!(location.path(), Some("apps/server/gate.exceptions.toml"));
    assert_eq!(location.span().map(|span| span.line), Some(12));

    let quiet = Findings::new().finish(&[unmatched], &Baseline::default(), &RulesRan::new());
    assert!(
        quiet.is_empty(),
        "an exception whose rule did not run is not due"
    );
}

#[test]
fn exit_code_is_one_only_with_an_error() {
    assert_eq!(report(Vec::new()).exit_code(), 0);
    assert_eq!(
        report(vec![finding("a/r", "", Level::Warning, "x", 1, "w")]).exit_code(),
        0
    );
    assert_eq!(
        report(vec![finding("a/r", "", Level::Error, "x", 1, "e")]).exit_code(),
        1
    );
}

#[test]
fn text_report_lists_findings_then_the_summary() {
    let mut first = finding(
        "layers/import-direction",
        "",
        Level::Error,
        "apps/server/src/a.ts",
        3,
        "imports a layer above",
    );
    first.help = Some("import through the module's barrel".to_owned());
    first.norm = Some("RULE-IMP-001".to_owned());
    let second = finding(
        "git/commit-subject",
        "length",
        Level::Warning,
        "apps/server/src/b.ts",
        1,
        "too long",
    );

    assert_eq!(
        report(vec![first, second])
            .render(Format::Text)
            .expect("render"),
        "error[layers/import-direction] apps/server/src/a.ts:3:1: imports a layer above\n  help: import through the module's barrel\n  norm: RULE-IMP-001\nwarning[git/commit-subject#length] apps/server/src/b.ts:1:1: too long\ngate check (tree): 2 rules, 1 error, 1 warning; projects: ran server; skipped none\n"
    );
}

#[test]
fn json_sarif_and_github_formats() {
    let mut finding = finding(
        "layers/import-direction",
        "deep",
        Level::Error,
        "apps/a,b.ts",
        3,
        "50% wrong\nreally",
    );
    finding.norm = Some("RULE-IMP-001".to_owned());
    let report = report(vec![finding.clone()]);

    let json: serde_json::Value =
        serde_json::from_str(&report.render(Format::Json).expect("render")).expect("json");
    assert_eq!(json["errors"], 1);
    assert_eq!(json["findings"][0]["rule"], "layers/import-direction");
    assert_eq!(json["findings"][0]["fingerprint"], finding.fingerprint());
    assert_eq!(json["projects"]["ran"][0]["project"], "server");

    let sarif: serde_json::Value =
        serde_json::from_str(&report.render(Format::Sarif).expect("render")).expect("sarif");
    assert_eq!(sarif["version"], "2.1.0");
    let result = &sarif["runs"][0]["results"][0];
    assert_eq!(result["ruleId"], "layers/import-direction");
    assert_eq!(result["level"], "error");
    assert_eq!(
        result["locations"][0]["physicalLocation"]["region"]["startLine"],
        3
    );
    assert_eq!(result["properties"]["norm"], "RULE-IMP-001");
    assert_eq!(
        sarif["runs"][0]["tool"]["driver"]["rules"][0]["id"],
        "layers/import-direction"
    );

    let github = report.render(Format::Github).expect("render");
    assert_eq!(
        github.lines().next(),
        Some(
            "::error file=apps/a%2Cb.ts,line=3,col=1,endLine=3,endColumn=5,title=layers/import-direction#deep::50%25 wrong%0Areally%0Anorm: RULE-IMP-001"
        )
    );
    assert!(github.ends_with("projects: ran server; skipped none\n"));
}

#[test]
fn fingerprint_ignores_the_line() {
    let a = finding("a/r", "", Level::Error, "x.ts", 1, "same");
    let b = finding("a/r", "", Level::Error, "x.ts", 40, "same");
    let c = finding("a/r", "", Level::Error, "x.ts", 1, "other");
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_ne!(a.fingerprint(), c.fingerprint());
    assert_eq!(a.fingerprint().len(), 32);
}

#[test]
fn the_baseline_hides_one_finding_per_entry_and_reports_stale_entries() {
    let old = finding("layers/relation", "", Level::Error, "apps/x.ts", 3, "old");
    let twin = finding("layers/relation", "", Level::Error, "apps/x.ts", 9, "old");
    let mut gone = finding("layers/relation", "", Level::Error, "apps/y.ts", 1, "fixed");
    gone.project = Some("client".to_owned());
    let baseline = Baseline::from_findings(&[old.clone(), gone]);
    let moved = finding("layers/relation", "", Level::Error, "apps/x.ts", 30, "old");

    let mut sink = Findings::new();
    sink.extend([moved, twin.clone()]);
    let kept = sink.finish(&[], &baseline, &ran_all());

    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_eq!(
        kept[0], twin,
        "one entry accepts one finding, wherever it moved"
    );
    assert_eq!(kept[1].rule, "gate/baseline");
    assert_eq!(
        kept[1].message,
        "the baseline entry for layers/relation at apps/y.ts matches no finding"
    );
    assert_eq!(
        kept[1].location.as_ref().and_then(Location::path),
        Some("gate/baseline.json")
    );

    let mut sink = Findings::new();
    sink.push(old);
    let mut server_only = RulesRan::new();
    server_only.insert(Some("server"), "layers/relation");
    let quiet = sink.finish(&[], &baseline, &server_only);
    assert!(
        quiet.is_empty(),
        "an entry of a scope that did not run is not due"
    );
}

#[test]
fn pruning_drops_exactly_the_entries_check_reports_stale() {
    // Two equal entries, of which one finding remains; an entry of a fixed finding; an entry of a
    // project whose rule did not run.
    let old = finding("layers/relation", "", Level::Error, "apps/x.ts", 3, "old");
    let mut gone = finding("layers/relation", "", Level::Error, "apps/y.ts", 1, "fixed");
    gone.project = Some("client".to_owned());
    let mut elsewhere = finding("layers/relation", "", Level::Error, "apps/z.ts", 1, "other");
    elsewhere.project = Some("docs".to_owned());
    let baseline = Baseline::from_findings(&[old.clone(), old.clone(), gone, elsewhere]);
    let new = finding("layers/relation", "", Level::Error, "apps/n.ts", 1, "new");

    let pruned = baseline.pruned(&[old.clone(), new.clone()], &ran_all());
    assert!(pruned.rekeyed.is_empty());
    let removed: Vec<&str> = pruned
        .removed
        .iter()
        .map(|entry| entry.message.as_str())
        .collect();
    assert_eq!(removed, ["old", "fixed"]);
    let pruned = pruned.baseline;
    let kept: Vec<&str> = pruned
        .entries()
        .iter()
        .map(|entry| entry.message.as_str())
        .collect();
    assert_eq!(
        kept,
        ["old", "other"],
        "the entry of a rule that did not run stays"
    );

    // What check reports with the pruned baseline: the new finding only, nothing stale.
    let mut sink = Findings::new();
    sink.extend([old, new.clone()]);
    assert_eq!(sink.finish(&[], &pruned, &ran_all()), [new]);
}

#[test]
fn pruning_re_keys_an_entry_written_before_its_finding_had_a_key() {
    let before = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        3,
        "x.ts:3 User.parent",
    );
    let baseline = Baseline::from_findings(&[before]);
    let mut keyed = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        3,
        "x.ts:3 User.parent",
    );
    keyed.key = Some("User.parent".to_owned());
    let mut moved = keyed.clone();
    moved.message = "x.ts:9 User.parent".to_owned();
    let new = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        5,
        "x.ts:5 User.child",
    );

    // Check does not re-key: the old entry is stale and the keyed finding is new.
    let mut sink = Findings::new();
    sink.push(keyed.clone());
    let reported = sink.finish(&[], &baseline, &ran_all());
    assert_eq!(reported.len(), 2, "{reported:?}");

    // Prune re-keys it, adds nothing for the new finding, and the entry then accepts the finding
    // wherever its line moves.
    let pruned = baseline.pruned(&[keyed.clone(), new.clone()], &ran_all());
    assert!(pruned.removed.is_empty());
    assert_eq!(pruned.rekeyed.len(), 1);
    assert_eq!(pruned.rekeyed[0].key.as_deref(), Some("User.parent"));
    assert_eq!(pruned.baseline.len(), 1);
    let mut sink = Findings::new();
    sink.extend([moved, new.clone()]);
    assert_eq!(sink.finish(&[], &pruned.baseline, &ran_all()), [new]);
}

#[test]
fn a_baseline_entry_of_one_check_does_not_accept_another_check_of_the_same_subject() {
    // ledger/reuses-planner's two opposite violations of one record and catalog key, on one line,
    // with the keys the plugin gives them.
    let mut drop = finding(
        "ledger/reuses",
        "planner",
        Level::Error,
        "docs/slices.md",
        12,
        "drop",
    );
    drop.key = Some("I-BASE-002/event-bus#drop".to_owned());
    let mut name = finding(
        "ledger/reuses",
        "planner",
        Level::Error,
        "docs/slices.md",
        12,
        "name",
    );
    name.key = Some("I-BASE-002/event-bus#name".to_owned());
    let baseline = Baseline::from_findings(&[drop]);
    let mut ran = RulesRan::new();
    ran.insert(Some("server"), "ledger/reuses");

    let mut sink = Findings::new();
    sink.push(name.clone());
    let reported = sink.finish(&[], &baseline, &ran);
    assert_eq!(reported.len(), 2, "{reported:?}");
    assert!(
        reported.contains(&name),
        "the other violation is reported: {reported:?}"
    );
    assert!(
        reported
            .iter()
            .any(|finding| finding.rule == "gate/baseline"),
        "and the entry of the fixed one is stale: {reported:?}"
    );
}

#[test]
fn a_key_replaces_the_message_in_the_fingerprint() {
    let mut first = finding("layers/relation", "", Level::Error, "apps/x.ts", 3, "first");
    let mut second = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        9,
        "second",
    );
    assert_ne!(first.fingerprint(), second.fingerprint());
    first.key = Some("User.parent".to_owned());
    second.key = Some("User.parent".to_owned());
    assert_eq!(first.fingerprint(), second.fingerprint());
    let baseline = Baseline::from_findings(&[first]);
    let mut sink = Findings::new();
    sink.push(second);
    assert!(sink.finish(&[], &baseline, &ran_all()).is_empty());
}

#[test]
fn the_baseline_never_holds_the_cores_own_findings() {
    let unmatched = exception("layers/relation", "gone.ts");
    let core = Findings::new().finish(&[unmatched], &Baseline::default(), &ran_all());
    assert_eq!(core.len(), 1);
    assert!(Baseline::from_findings(&core).is_empty());
}

#[test]
fn the_baseline_file_round_trips_and_rejects_hand_edits() {
    use crate::fixture::{ROOT, Repo};

    let baseline = Baseline::from_findings(&[
        finding(
            "layers/relation",
            "eager",
            Level::Error,
            "apps/b.ts",
            1,
            "b",
        ),
        finding("layers/relation", "", Level::Warning, "apps/a.ts", 1, "a"),
    ]);
    let text = baseline.to_json().expect("json");
    assert!(
        text.find("apps/a.ts") < text.find("apps/b.ts"),
        "sorted by path: {text}"
    );

    let repo = Repo::new();
    repo.write("gate.toml", ROOT)
        .write("gate/baseline.json", &text)
        .add();
    let workspace = gate_host::Workspace::open(repo.path(), gate_host::Source::WorkTree)
        .expect("valid baseline");
    assert_eq!(workspace.baseline(), &baseline);

    repo.write(
        "gate/baseline.json",
        &text.replacen("\"a\"", "\"edited\"", 1),
    )
    .add();
    let error = gate_host::Workspace::open(repo.path(), gate_host::Source::WorkTree)
        .expect_err("edited baseline");
    assert_eq!(error.kind(), gate_host::ErrorKind::Config);
    assert!(
        error.message().contains("the file was edited by hand"),
        "{error}"
    );
}

#[test]
fn the_summary_names_each_override_and_its_checksum() {
    let mut report = report(Vec::new());
    report
        .overrides
        .push("layers=build/layers.wasm sha256 00ff".to_owned());
    assert!(
        report
            .summary()
            .ends_with("; override layers=build/layers.wasm sha256 00ff"),
        "{}",
        report.summary()
    );
}

#[test]
fn an_exception_accepts_only_the_findings_of_its_own_scope() {
    let target = "src/user.ts:User.parent";
    let mut server = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/server/src/user.ts",
        3,
        "server",
    );
    server.target = Some(target.to_owned());
    let mut client = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/client/src/user.ts",
        3,
        "client",
    );
    client.target = Some(target.to_owned());
    client.project = Some("client".to_owned());
    let mut repository = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/server/src/user.ts",
        3,
        "repository",
    );
    repository.target = Some(target.to_owned());
    repository.project = None;

    let mut sink = Findings::new();
    sink.extend([server, client, repository]);
    let kept = sink.finish(
        &[exception("layers/relation", target)],
        &Baseline::default(),
        &ran_all(),
    );
    let messages: Vec<&str> = kept
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert_eq!(
        messages,
        ["client", "repository"],
        "an exception of server accepts neither another project's finding with the same target nor a repository-scope one"
    );
}

#[test]
fn an_exception_is_due_only_where_its_rule_ran() {
    let unmatched = exception("layers/relation", "src/gone.ts:Gone.field");
    let mut elsewhere = RulesRan::new();
    elsewhere.insert(Some("client"), "layers/relation");
    elsewhere.insert(Some("server"), "layers/other");
    let quiet = Findings::new().finish(
        std::slice::from_ref(&unmatched),
        &Baseline::default(),
        &elsewhere,
    );
    assert!(
        quiet.is_empty(),
        "the rule ran in another project, and another rule ran in server: not due"
    );

    let mut here = RulesRan::new();
    here.insert(Some("server"), "layers/relation");
    let due = Findings::new().finish(&[unmatched], &Baseline::default(), &here);
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].rule, "gate/exceptions");
}

#[test]
fn a_commit_location_is_its_full_id_in_json_short_in_text_and_matched_by_target() {
    let id = "0123456789abcdef0123456789abcdef01234567";
    let mut on_commit = Finding::new(
        &RuleId::parse("git/commit-subject").expect("rule id"),
        Level::Error,
        "too long",
    );
    on_commit.location = Some(Location::Commit {
        commit: id.to_owned(),
    });
    on_commit.target = Some(id.to_owned());
    on_commit.project = Some("server".to_owned());
    let report = report(vec![on_commit.clone()]);

    let json: serde_json::Value =
        serde_json::from_str(&report.render(Format::Json).expect("render")).expect("json");
    assert_eq!(
        json["findings"][0]["location"],
        serde_json::json!({ "commit": id })
    );
    assert_eq!(json["findings"][0]["target"], id);

    let text = report.render(Format::Text).expect("render");
    assert!(
        text.starts_with("error[git/commit-subject] commit 0123456: too long\n"),
        "{text}"
    );

    let sarif: serde_json::Value =
        serde_json::from_str(&report.render(Format::Sarif).expect("render")).expect("sarif");
    let result = &sarif["runs"][0]["results"][0];
    assert_eq!(result["properties"]["commit"], id);
    assert!(result.get("locations").is_none(), "{result:#}");

    let mut other = on_commit.clone();
    other.location = Some(Location::Commit {
        commit: "f".repeat(40),
    });
    other.target = Some("f".repeat(40));
    let mut sink = Findings::new();
    sink.extend([on_commit, other.clone()]);
    let mut accepted = exception("git/commit-subject", id);
    accepted.path = None;
    let kept = sink.finish(&[accepted], &Baseline::default(), &ran_all());
    assert_eq!(kept, [other]);
}

#[test]
fn a_baseline_entry_accepts_only_the_finding_of_its_own_project() {
    // Two projects whose plugin reads docs/ report the same finding on the same file.
    let mut server = finding("layers/relation", "", Level::Error, "docs/x.md", 1, "cited");
    server.target = None;
    let mut client = server.clone();
    client.project = Some("client".to_owned());
    let baseline = Baseline::from_findings(std::slice::from_ref(&client));
    assert_eq!(server.fingerprint(), client.fingerprint());

    let mut sink = Findings::new();
    sink.extend([server.clone(), client]);
    let kept = sink.finish(&[], &baseline, &ran_all());
    assert_eq!(
        kept,
        [server.clone()],
        "client's entry accepts client's finding only; server's stays"
    );

    let mut sink = Findings::new();
    sink.push(server);
    let kept = sink.finish(&[], &baseline, &ran_all());
    let rules: Vec<&str> = kept.iter().map(|finding| finding.rule.as_str()).collect();
    assert_eq!(
        rules,
        ["layers/relation", "gate/baseline"],
        "server's finding is not taken by client's entry, which is stale"
    );
}

#[test]
fn a_root_file_exception_accepts_a_repository_scope_finding() {
    let mut root = exception("git/commit-subject", "abc1234");
    root.file = "gate.exceptions.toml".to_owned();
    root.path = Some("abc1234".to_owned());
    root.project = None;
    let mut subject = finding("git/commit-subject", "", Level::Error, "", 1, "too long");
    subject.location = None;
    subject.target = Some("abc1234".to_owned());
    subject.project = None;

    let mut ran = RulesRan::new();
    ran.insert(None, "git/commit-subject");
    let mut sink = Findings::new();
    sink.push(subject);
    let kept = sink.finish(std::slice::from_ref(&root), &Baseline::default(), &ran);
    assert!(kept.is_empty(), "{kept:?}");
}

fn filter(keys: &[&str]) -> gate_host::RuleFilter {
    gate_host::RuleFilter::new(
        keys.iter()
            .map(|key| gate_host::RuleFilter::parse(key).expect("key"))
            .collect(),
    )
}

#[test]
fn a_filtered_run_reports_only_its_rules_and_no_unused_exceptions_of_the_others() {
    let relation = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        3,
        "relation",
    );
    let other = finding("a/rule", "", Level::Error, "apps/x.ts", 4, "other");
    let unused = exception("a/rule", "src/gone.ts:Gone.field");

    // The runtime ran every rule; the core still holds the run to --rule.
    let unfiltered = {
        let mut sink = Findings::new();
        sink.extend([relation.clone(), other.clone()]);
        sink.finish(
            std::slice::from_ref(&unused),
            &Baseline::default(),
            &ran_all(),
        )
    };
    let rules: Vec<&str> = unfiltered.iter().map(|found| found.rule.as_str()).collect();
    assert_eq!(rules, ["gate/exceptions", "layers/relation", "a/rule"]);

    let only = filter(&["layers/relation"]);
    let mut ran = ran_all();
    ran.retain_rules(&only);
    let mut sink = Findings::new();
    sink.extend([relation.clone(), other]);
    sink.retain_rules(&only);
    let kept = sink.finish(std::slice::from_ref(&unused), &Baseline::default(), &ran);
    assert_eq!(
        kept,
        [relation],
        "no a/rule finding, and its exception is not due"
    );
}

#[test]
fn a_family_filter_takes_every_rule_of_its_plugin_and_the_core_always_runs() {
    let family = filter(&["layers/*"]);
    assert!(family.allows("layers/relation"));
    assert!(family.allows("layers/barrel-exports"));
    assert!(!family.allows("a/rule"));
    assert!(family.allows("gate/exceptions"));
    assert!(family.allows_plugin("layers"));
    assert!(!family.allows_plugin("a"));
    assert!(filter(&[]).allows("a/rule"));
    assert!(
        gate_host::RuleFilter::parse("layers/relation#nullable")
            .expect_err("aspect")
            .contains("without #<aspect>")
    );
}
