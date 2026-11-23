//! Findings: exceptions, order, report formats and exit codes.

use gate_host::config::{RuleId, RuleRef};
use gate_host::{
    Baseline, BaselineEntry, Decision, Finding, Findings, Format, Level, Location, Reason, Report,
    RulesRan, ScopedException, Selection, Span,
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
        baselined: 0,
        selection: Selection {
            ran: vec![Decision {
                project: "server".to_owned(),
                reason: Reason::Full,
            }],
            skipped: Vec::new(),
            repository: true,
        },
        overrides: Vec::new(),
        skipped_rules: Vec::new(),
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
fn an_exception_of_a_rule_that_narrowed_its_view_is_due_only_where_it_judged_the_target() {
    let id = "c".repeat(40);
    let unmatched = |target: &str| {
        let mut exception = exception("a/rule", target);
        exception.path = None;
        exception
    };
    let messages = |exceptions: &[ScopedException], ran: &RulesRan| -> Vec<String> {
        Findings::new()
            .finish(exceptions, &Baseline::default(), ran)
            .into_iter()
            .map(|finding| finding.message)
            .collect()
    };
    let exceptions = [unmatched(&id), unmatched("feat/old")];

    // A rule that judged its whole scope: every exception is due, as before.
    assert_eq!(
        messages(&exceptions, &ran_all()),
        [
            format!("the exception for a/rule at {id} matches no finding"),
            "the exception for a/rule at feat/old matches no finding".to_owned(),
        ]
    );

    // It judged the commit only: the other exception is not due.
    let mut ran = ran_all();
    ran.narrow(
        Some("server"),
        "a/rule",
        vec![id.clone(), "feat/new".to_owned()],
    );
    assert_eq!(
        messages(&exceptions, &ran),
        [format!(
            "the exception for a/rule at {id} matches no finding"
        )]
    );

    // It judged nothing: none is due.
    let mut nothing = ran_all();
    nothing.narrow(Some("server"), "a/rule", Vec::new());
    assert_eq!(messages(&exceptions, &nothing), [] as [String; 0]);

    // Another project's narrowing leaves this one's view whole.
    let mut elsewhere = ran_all();
    elsewhere.narrow(Some("client"), "a/rule", Vec::new());
    assert_eq!(messages(&exceptions, &elsewhere).len(), 2);

    // An exception that matches a finding is used, judged or not.
    let mut found = finding("a/rule", "", Level::Error, "apps/server/x.ts", 1, "m");
    found.target = Some("feat/old".to_owned());
    let mut sink = Findings::new();
    sink.push(found);
    let kept = sink.finish(&exceptions[1..], &Baseline::default(), &nothing);
    assert!(kept.is_empty(), "{kept:?}");
}

#[test]
fn a_finding_on_a_target_re_judged_for_its_exceptions_stays_only_when_an_exception_accepts_it() {
    let id = "c".repeat(40);
    let mut excepted = exception("a/rule#header-max-length", &id);
    excepted.path = None;
    let on = |aspect: &str, message: &str| {
        let mut found = finding(
            "a/rule",
            aspect,
            Level::Error,
            "apps/server/x.ts",
            1,
            message,
        );
        found.target = Some(id.clone());
        found
    };
    let messages = |found: Vec<Finding>, baseline: &Baseline, ran: &RulesRan| -> Vec<String> {
        let mut sink = Findings::new();
        sink.extend(found);
        sink.finish(std::slice::from_ref(&excepted), baseline, ran)
            .into_iter()
            .map(|finding| finding.message)
            .collect()
    };
    // The run judged nothing in full and re-judged the excepted commit.
    let mut ran = ran_all();
    ran.narrow(Some("server"), "a/rule", Vec::new());
    ran.rejudge(Some("server"), "a/rule", vec![id.clone()]);

    // The excepted aspect is accepted; the aspect the exception does not cover, failing since an
    // option changed, is dropped: the run does not judge the commit otherwise.
    let both = || {
        vec![
            on("header-max-length", "long"),
            on("prohibited-trailers", "trailer"),
        ]
    };
    assert_eq!(
        messages(both(), &Baseline::default(), &ran),
        [] as [String; 0]
    );
    // Once the excepted aspect passes, the exception is due and reported.
    assert_eq!(
        messages(
            vec![on("prohibited-trailers", "trailer")],
            &Baseline::default(),
            &ran
        ),
        [format!(
            "the exception for a/rule#header-max-length at {id} matches no finding"
        )]
    );
    // A baseline entry keyed by a target only re-judged is not due.
    let mut keyed = on("prohibited-trailers", "trailer");
    keyed.key = Some(id.clone());
    let baseline = Baseline::from_findings(&[keyed]);
    assert_eq!(
        messages(vec![on("header-max-length", "long")], &baseline, &ran),
        [] as [String; 0]
    );

    // A run that judges the commit in full, a push of it, reports the other aspect.
    ran.narrow(Some("server"), "a/rule", vec![id.clone()]);
    assert_eq!(messages(both(), &Baseline::default(), &ran), ["trailer"]);
}

#[test]
fn a_baseline_entry_of_a_rule_that_narrowed_its_view_is_due_only_where_it_judged_its_key() {
    let mut keyed = finding("a/rule", "", Level::Error, "apps/x.ts", 1, "keyed");
    keyed.key = Some("feat/old".to_owned());
    let keyless = finding("a/rule", "", Level::Error, "apps/y.ts", 1, "keyless");
    let baseline = Baseline::from_findings(&[keyed, keyless]);
    let stale = |ran: &RulesRan| -> Vec<String> {
        Findings::new()
            .finish(&[], &baseline, ran)
            .into_iter()
            .map(|finding| finding.message)
            .collect()
    };
    let mut ran = ran_all();
    ran.narrow(Some("server"), "a/rule", vec!["feat/new".to_owned()]);
    assert_eq!(
        stale(&ran),
        ["the baseline entry for a/rule at apps/y.ts matches no finding"],
        "a keyed entry outside the judged targets is not due; one without a key is"
    );
    ran.narrow(Some("server"), "a/rule", vec!["feat/old".to_owned()]);
    assert_eq!(stale(&ran).len(), 2);
    assert_eq!(baseline.pruned(&[], &ran).removed.len(), 2);
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
fn finish_counts_each_finding_a_baseline_entry_accepts_and_nothing_else() {
    let target = "src/user.ts:User.parent";
    let old = finding("layers/relation", "", Level::Error, "apps/x.ts", 3, "old");
    let twin = finding("layers/relation", "", Level::Error, "apps/x.ts", 9, "old");
    let warned = finding(
        "layers/relation",
        "",
        Level::Warning,
        "apps/w.ts",
        2,
        "warned",
    );
    let noted = finding("layers/relation", "", Level::Note, "apps/v.ts", 4, "noted");
    let mut excepted = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/server/src/user.ts",
        3,
        "excepted",
    );
    excepted.target = Some(target.to_owned());
    let mut gone = finding("layers/relation", "", Level::Error, "apps/y.ts", 1, "fixed");
    gone.project = Some("client".to_owned());
    // A core finding, which no entry accepts, and a finding on a commit `a/rule` re-judged only
    // for its exceptions, which is dropped before the baseline sees it.
    let core = finding("gate/budget", "", Level::Warning, "gate.toml", 1, "slow");
    let id = "c".repeat(40);
    let mut rejudged = finding(
        "a/rule",
        "",
        Level::Error,
        "apps/server/x.ts",
        1,
        "re-judged",
    );
    rejudged.target = Some(id.clone());
    // Keyed by the commit, as a commit rule's finding is, so its entry is due only where the rule
    // judged that commit in full.
    rejudged.key = Some(id.clone());
    let baseline = Baseline::from_findings(&[
        old.clone(),
        warned.clone(),
        noted.clone(),
        excepted.clone(),
        gone,
        core.clone(),
        rejudged.clone(),
    ]);
    let mut ran = ran_all();
    ran.narrow(Some("server"), "a/rule", Vec::new());
    ran.rejudge(Some("server"), "a/rule", vec![id]);

    let mut sink = Findings::new();
    sink.extend([old, twin.clone(), warned, noted, excepted, core, rejudged]);
    let finished = sink.finish_counted(&[exception("layers/relation", target)], &baseline, &ran);

    // `old`, `warned` and `noted` are held back; `twin` is past its fingerprint's one entry, the
    // exception takes `excepted` first, the entries of `excepted` and `gone` match nothing, the
    // core finding stays reported, and the re-judged finding is dropped uncounted. Three held and
    // two not, so a count of the findings no entry accepted reads 2, not 3.
    assert_eq!(finished.baselined, 3, "{:?}", finished.findings);
    let mut rules: Vec<&str> = finished
        .findings
        .iter()
        .map(|finding| finding.rule.as_str())
        .collect();
    rules.sort_unstable();
    assert_eq!(
        rules,
        [
            "gate/baseline",
            "gate/baseline",
            "gate/budget",
            "layers/relation"
        ]
    );
    assert!(finished.findings.contains(&twin), "{:?}", finished.findings);

    let nothing = Findings::new().finish_counted(&[], &baseline, &ran_all());
    assert_eq!(nothing.baselined, 0, "{:?}", nothing.findings);
}

#[test]
fn the_summary_counts_the_findings_the_baseline_held_back_when_there_are_any() {
    let mut report = report(Vec::new());
    assert_eq!(
        report.summary(),
        "gate check (tree): 2 rules, 0 errors, 0 warnings; projects: ran server; skipped none"
    );
    report.baselined = 1;
    assert_eq!(
        report.summary(),
        "gate check (tree): 2 rules, 0 errors, 0 warnings, 1 finding held back by gate/baseline.json; projects: ran server; skipped none"
    );
    report.baselined = 3;
    let json: serde_json::Value =
        serde_json::from_str(&report.render(Format::Json).expect("render")).expect("json");
    assert_eq!(json["baselined"], 3);
    let sarif: serde_json::Value =
        serde_json::from_str(&report.render(Format::Sarif).expect("render")).expect("sarif");
    assert_eq!(sarif["runs"][0]["properties"]["baselined"], 3);
    assert_eq!(
        json["summary"],
        "gate check (tree): 2 rules, 0 errors, 0 warnings, 3 findings held back by gate/baseline.json; projects: ran server; skipped none"
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
    assert_eq!(pruned.rekeyed, [] as [BaselineEntry; 0]);
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
    assert_eq!(pruned.removed, [] as [BaselineEntry; 0]);
    assert_eq!(pruned.rekeyed.len(), 1);
    assert_eq!(pruned.rekeyed[0].key.as_deref(), Some("User.parent"));
    assert_eq!(pruned.baseline.len(), 1);
    let mut sink = Findings::new();
    sink.extend([moved, new.clone()]);
    assert_eq!(sink.finish(&[], &pruned.baseline, &ran_all()), [new]);
}

#[test]
fn a_prune_never_re_keys_an_entry_to_a_finding_an_entry_written_meanwhile_accepts() {
    let before = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        3,
        "x.ts:3 User.parent",
    );
    let mut keyed = before.clone();
    keyed.key = Some("User.parent".to_owned());
    // Prune loaded the keyless entry. While its check ran, a baseline write from a tree with both
    // findings kept that entry and added the keyed one; prune's own check reports only the keyed
    // finding.
    let judged = Baseline::from_findings(std::slice::from_ref(&before));
    let current = Baseline::from_findings(&[before, keyed.clone()]);
    let pruned = current.pruned_since(&judged, std::slice::from_ref(&keyed), &ran_all());

    // As a prune run after the write: the keyed entry accepts the finding, so the keyless one is
    // stale, and nothing is re-keyed.
    assert_eq!(pruned.rekeyed, [] as [BaselineEntry; 0]);
    assert_eq!(pruned.removed.len(), 1, "{:?}", pruned.removed);
    assert_eq!(pruned.removed[0].key, None);
    assert_eq!(pruned.baseline.entries(), [BaselineEntry::of(&keyed)]);
    // The pruned baseline accepts the finding once, so a second one is reported. The second is on
    // another line: the fingerprint leaves the line out, so the keyed entry matches it too, while
    // `finish` folds two identical findings into one before the baseline sees them.
    let mut second = keyed.clone();
    second.location = Some(Location::File {
        path: "apps/x.ts".to_owned(),
        span: Some(Span {
            line: 9,
            column: 1,
            end_line: 9,
            end_column: 5,
        }),
    });
    assert_eq!(second.fingerprint(), keyed.fingerprint());
    let mut sink = Findings::new();
    sink.extend([keyed, second.clone()]);
    assert_eq!(sink.finish(&[], &pruned.baseline, &ran_all()), [second]);
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
    assert_eq!(sink.finish(&[], &baseline, &ran_all()), [] as [Finding; 0]);
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

/// A filter of `keys` (`--rule`) that skips `skip` (`--skip-rule`).
fn skipping(keys: &[&str], skip: &[&str]) -> gate_host::RuleFilter {
    filter(keys).skipping(
        skip.iter()
            .map(|key| gate_host::RuleFilter::parse_skip(key).expect("key"))
            .collect(),
    )
}

#[test]
fn a_skip_leaves_its_rules_out_after_the_rule_filter() {
    // Without --rule: every rule but the skipped ones.
    let one = skipping(&[], &["layers/relation"]);
    assert!(!one.allows("layers/relation"));
    assert!(one.allows("layers/barrel-exports"));
    assert!(one.allows("a/rule"));
    assert!(one.allows("gate/exceptions"), "the core always runs");
    assert!(one.allows_plugin("layers"), "layers keeps a rule to run");
    assert!(one.narrowed_to("layers/relation"), "--rule alone allows it");

    let family = skipping(&[], &["layers/*"]);
    assert!(!family.allows("layers/relation"));
    assert!(!family.allows("layers/barrel-exports"));
    assert!(family.allows("a/rule"));
    assert!(!family.allows_plugin("layers"), "no rule of layers runs");
    assert!(family.allows_plugin("a"));

    // With --rule: the skip applies to what --rule leaves.
    let both = skipping(&["layers/*", "a/rule"], &["layers/relation"]);
    assert!(!both.allows("layers/relation"));
    assert!(both.allows("layers/barrel-exports"));
    assert!(both.allows("a/rule"));
    assert!(!both.allows("b/rule"), "--rule leaves b out");
    assert!(both.allows_plugin("layers"));
    assert!(!both.allows_plugin("b"));

    // The same key in both: the skip wins.
    let emptied = skipping(&["layers/relation"], &["layers/*"]);
    assert!(!emptied.allows("layers/relation"));
    assert!(!emptied.allows_plugin("layers"));

    assert!(
        gate_host::RuleFilter::parse_skip("layers/relation#nullable")
            .expect_err("aspect")
            .contains("--skip-rule leaves whole rules out of a run")
    );
}

#[test]
fn a_skipped_rule_s_exceptions_and_baseline_entries_are_not_due() {
    let relation = finding(
        "layers/relation",
        "",
        Level::Error,
        "apps/x.ts",
        3,
        "relation",
    );
    let other = finding("a/rule", "", Level::Error, "apps/x.ts", 4, "other");
    let gone = finding("a/rule", "", Level::Error, "apps/y.ts", 1, "fixed");
    let unused = exception("a/rule", "src/gone.ts:Gone.field");
    let baseline = Baseline::from_findings(&[gone]);

    // Without the skip, the exception is unused and the baseline entry stale.
    let mut sink = Findings::new();
    sink.extend([relation.clone(), other.clone()]);
    let rules: Vec<String> = sink
        .finish(std::slice::from_ref(&unused), &baseline, &ran_all())
        .into_iter()
        .map(|found| found.rule)
        .collect();
    assert_eq!(
        rules,
        [
            "gate/exceptions",
            "layers/relation",
            "a/rule",
            "gate/baseline"
        ]
    );

    // The runtime reported a/rule anyway; the core holds the run to the skip.
    let skip = skipping(&[], &["a/rule"]);
    let mut ran = ran_all();
    ran.retain_rules(&skip);
    let mut sink = Findings::new();
    sink.extend([relation.clone(), other]);
    sink.retain_rules(&skip);
    let kept = sink.finish(std::slice::from_ref(&unused), &baseline, &ran);
    assert_eq!(
        kept,
        [relation],
        "no a/rule finding, and neither its exception nor its baseline entry is due"
    );
}

#[test]
fn the_summary_names_the_rules_a_skip_left_out() {
    let mut report = report(Vec::new());
    report.skipped_rules = vec!["git/main-history".to_owned(), "git/no-ci-skip".to_owned()];
    assert_eq!(
        report.summary(),
        "gate check (tree): 2 rules (skipped git/main-history, git/no-ci-skip), 0 errors, 0 warnings; projects: ran server; skipped none"
    );
    let json: serde_json::Value =
        serde_json::from_str(&report.render(Format::Json).expect("render")).expect("json");
    assert_eq!(
        json["skipped-rules"],
        serde_json::json!(["git/main-history", "git/no-ci-skip"])
    );
    report.skipped_rules.clear();
    let json: serde_json::Value =
        serde_json::from_str(&report.render(Format::Json).expect("render")).expect("json");
    assert_eq!(json["skipped-rules"], serde_json::json!([]));
}
