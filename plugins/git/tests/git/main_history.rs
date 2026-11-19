use crate::repo::{Repo, lines, merged, settings, short};

const PACKAGE: &str = r#"{"name": "app", "version": "1.1.0", "private": true}"#;
const LOCK: &str = r#"{"name": "app", "version": "1.1.0", "packages": {"": {"version": "1.1.0"}, "node_modules/x": {"version": "2.0.0"}}}"#;
const RULES: &str = "# Rules\n\n## RULE-API-001 Typed\n\nEnforcement: gate · Status: stable\n";

/// A main line: a root before activation, the activation commit, then whatever the test adds.
fn start() -> (Repo, String) {
    let mut repo = Repo::default();
    let root = repo.commit(
        &[],
        "init",
        &[
            ("package.json", Some(PACKAGE)),
            ("package-lock.json", Some(LOCK)),
            ("CHANGELOG.md", Some("# Changes\n")),
            ("docs/agents/rules/api.md", Some(RULES)),
            ("src/main.ts", Some("export {};\n")),
        ],
    );
    let activation = repo.commit(&[&root], "activate the policy", &[("docs/a.md", Some("a"))]);
    (repo, activation)
}

fn run(repo: &mut Repo, activation: &str, tip: &str) -> Vec<String> {
    repo.set_ref("refs/remotes/origin/main", tip);
    let findings = repo
        .check("main-history", &settings(activation), "{}")
        .expect("runs");
    for finding in &findings {
        assert_eq!(finding.target.as_deref().map(str::len), Some(40));
        assert!(finding.help.is_some());
    }
    lines(&findings)
}

#[test]
fn a_policy_following_history_passes() {
    let (mut repo, activation) = start();
    let work = repo.commit(
        &[&activation],
        "feat: work",
        &[("src/main.ts", Some("export const a = 1;\n"))],
    );
    let merge = repo.commit(
        &[&activation, &work],
        "Merge branch 'feat/new-work'",
        &[("src/main.ts", Some("export const a = 1;\n"))],
    );
    let bump = repo.commit(
        &[&merge],
        "deps",
        &[("package-lock.json", Some(&LOCK.replace("2.0.0", "2.0.1")))],
    );
    let deps = repo.commit(
        &[&merge, &bump],
        "Merge branch 'dependabot/npm/x-2.0.1'",
        &[("package-lock.json", Some(&LOCK.replace("2.0.0", "2.0.1")))],
    );
    let docs = repo.commit(
        &[&deps],
        "docs: guide",
        &[
            ("docs/guide.md", Some("g")),
            ("NOTES.md", Some("n")),
            (
                "docs/agents/rules/api.md",
                Some(&RULES.replace("Typed", "Typed responses")),
            ),
        ],
    );
    let release = repo.commit(
        &[&docs],
        "chore(release): 1.2.0 [skip ci]",
        &[
            ("package.json", Some(&PACKAGE.replace("1.1.0", "1.2.0"))),
            (
                "package-lock.json",
                Some(&LOCK.replace("2.0.0", "2.0.1").replace("1.1.0", "1.2.0")),
            ),
            ("CHANGELOG.md", Some("# Changes\n\n## 1.2.0\n")),
        ],
    );
    let generated = repo.commit(
        &[&release],
        "docs: regenerate",
        &[(".agents/rules.md", Some("r"))],
    );
    repo.set_sync(&generated, &[".agents/rules.md"], &[]);
    assert_eq!(
        run(&mut repo, &activation, &generated),
        Vec::<String>::new()
    );
}

#[test]
fn merges_that_break_the_policy_are_reported() {
    let (mut repo, activation) = start();
    let first_work = repo.commit(&[&activation], "a", &[("src/a.ts", Some("a"))]);
    let named = repo.commit(
        &[&activation, &first_work],
        "Merge branch 'wip'",
        &[("src/a.ts", Some("a"))],
    );
    let second_work = repo.commit(&[&named], "b", &[("src/b.ts", Some("b"))]);
    let subject = repo.commit(
        &[&named, &second_work],
        "Merge pull request #1 from feat/b-work",
        &[("src/b.ts", Some("b"))],
    );
    let third_work = repo.commit(&[&subject], "c", &[("src/c.ts", Some("c"))]);
    let edited = repo.commit(
        &[&subject, &third_work],
        "Merge branch 'feat/c-work'\n\nWhy this merge.",
        &[("src/c.ts", Some("c"))],
    );
    let deps = repo.commit(&[&edited], "deps", &[("src/d.ts", Some("d"))]);
    let dependabot = repo.commit(
        &[&edited, &deps],
        "Merge branch 'dependabot/npm/d'",
        &[("src/d.ts", Some("d"))],
    );
    let sync = repo.commit(
        &[&dependabot, &edited],
        "Merge branch 'feat/sync-main'",
        &[],
    );
    let left = repo.commit(&[&sync], "e", &[("src/e.ts", Some("e"))]);
    let right = repo.commit(&[&sync], "f", &[("src/f.ts", Some("f"))]);
    let octopus = repo.commit(
        &[&sync, &left, &right],
        "Merge branches 'e' and 'f'",
        &[("src/e.ts", Some("e")), ("src/f.ts", Some("f"))],
    );

    assert_eq!(
        run(&mut repo, &activation, &octopus),
        [
            format!(
                "merge commit:{named} {} merges branch wip, which neither matches branch-pattern within branch-length nor starts with dependabot/",
                short(&named)
            ),
            format!(
                "merge commit:{subject} {} has none of the forms of [plugins.git] options forms: subject \"Merge pull request #1 from feat/b-work\", 2 parents",
                short(&subject)
            ),
            format!(
                "merge commit:{edited} {} has an edited merge message body",
                short(&edited)
            ),
            format!(
                "merge commit:{dependabot} {} merges a Dependabot branch that changes src/d.ts",
                short(&dependabot)
            ),
            format!(
                "merge commit:{sync} {} is a sync merge: its second parent is already on main",
                short(&sync)
            ),
            format!(
                "merge commit:{octopus} {} has none of the forms of [plugins.git] options forms: subject \"Merge branches 'e' and 'f'\", 3 parents",
                short(&octopus)
            ),
        ]
    );
}

#[test]
fn a_merge_that_adds_its_own_change_is_reported_and_a_resolved_conflict_is_not() {
    let (mut repo, activation) = start();
    let work = repo.commit(&[&activation], "a", &[("src/a.ts", Some("a"))]);
    let evil = repo.commit(
        &[&activation, &work],
        "Merge branch 'feat/evil-one'",
        &[("src/a.ts", Some("a")), ("src/extra.ts", Some("x"))],
    );
    let main_side = repo.commit(&[&evil], "docs: x", &[("docs/x.md", Some("main"))]);
    let branch_side = repo.commit(&[&evil], "docs: y", &[("docs/x.md", Some("branch"))]);
    let resolved = repo.commit(
        &[&main_side, &branch_side],
        "Merge branch 'docs/resolved'",
        &[("docs/x.md", Some("both"))],
    );
    assert_eq!(
        run(&mut repo, &activation, &resolved),
        [format!(
            "merge commit:{evil} {} differs from git's conflict-free merge of its parents; the merge commit adds its own change",
            short(&evil)
        )]
    );
}

#[test]
fn a_branch_name_outside_its_length_is_reported() {
    let (mut repo, activation) = start();
    let a = repo.commit(&[&activation], "a", &[("src/a.ts", Some("a"))]);
    let merge = repo.commit(
        &[&activation, &a],
        "Merge branch 'fix/a'",
        &[("src/a.ts", Some("a"))],
    );
    assert_eq!(
        run(&mut repo, &activation, &merge),
        [format!(
            "merge commit:{merge} {} merges branch fix/a, which neither matches branch-pattern within branch-length nor starts with dependabot/",
            short(&merge)
        )]
    );
}

#[test]
fn release_commits_change_only_the_release_files_and_their_versions() {
    let (mut repo, activation) = start();
    let extra = repo.commit(
        &[&activation],
        "chore(release): 1.2.0 [skip ci]",
        &[
            ("package.json", Some(&PACKAGE.replace("1.1.0", "1.2.0"))),
            ("src/main.ts", Some("x")),
        ],
    );
    let wrong = repo.commit(
        &[&extra],
        "chore(release): 1.3.0 [skip ci]",
        &[(
            "package.json",
            Some(&PACKAGE.replace("1.1.0", "1.2.9").replace("true", "false")),
        )],
    );
    let lock = repo.commit(
        &[&wrong],
        "chore(release): 1.4.0 [skip ci]",
        &[("package-lock.json", Some(&LOCK.replace("2.0.0", "3.0.0")))],
    );
    let broken = repo.commit(
        &[&lock],
        "chore(release): 1.5.0 [skip ci]",
        &[("package.json", Some("{"))],
    );
    let broken_lock = repo.commit(
        &[&broken],
        "chore(release): 1.6.0 [skip ci]",
        &[("package-lock.json", Some("{"))],
    );
    let dropped_lock = repo.commit(
        &[&broken_lock],
        "chore(release): 1.7.0 [skip ci]",
        &[("package-lock.json", None)],
    );
    assert_eq!(
        run(&mut repo, &activation, &dropped_lock),
        [
            format!(
                "release commit:{extra} release commit {} changes src/main.ts",
                short(&extra)
            ),
            format!(
                "release commit:{wrong} release commit {} sets version Some(\"1.2.9\"), not 1.3.0",
                short(&wrong)
            ),
            format!(
                "release commit:{wrong} release commit {} changes package.json beyond version",
                short(&wrong)
            ),
            format!(
                "release commit:{lock} release commit {} changes package-lock.json beyond its version fields",
                short(&lock)
            ),
            format!(
                "release commit:{broken} release commit {} has an unreadable package.json",
                short(&broken)
            ),
            format!(
                "release commit:{broken_lock} release commit {} has an unreadable package-lock.json",
                short(&broken_lock)
            ),
            format!(
                "release commit:{dropped_lock} release commit {} has an unreadable package-lock.json",
                short(&dropped_lock)
            ),
        ]
    );
}

#[test]
fn direct_commits_change_documentation_and_what_gate_sync_produces_in_sync() {
    let (mut repo, activation) = start();
    let code = repo.commit(
        &[&activation],
        "fix: quick",
        &[("src/main.ts", Some("x")), ("docs/a.md", Some("b"))],
    );
    let nested = repo.commit(
        &[&code],
        "docs: nested",
        &[
            ("docs/deep/x.md", Some("x")),
            ("guide/README.md", Some("x")),
        ],
    );
    let status = repo.commit(
        &[&nested],
        "docs: retire",
        &[(
            "docs/agents/rules/api.md",
            Some(&RULES.replace("Status: stable", "Status: deprecated")),
        )],
    );
    let generated = repo.commit(
        &[&status],
        "docs: regenerate",
        &[
            (".agents/a.md", Some("a")),
            (".agents/b.md", Some("b")),
            (".agents/stale.md", Some("no generator writes this")),
            (".github/workflows/gate.yml", Some("edited by hand")),
        ],
    );
    // `.agents/a.md` is produced and in sync; the others drift: a written file, a stale file
    // under an owned glob, the core's CI file, and a file the commit does not change.
    let drift = [
        ".agents/b.md",
        ".agents/other.md",
        ".agents/stale.md",
        ".github/workflows/gate.yml",
    ];
    let mut produced = vec![".agents/a.md", ".agents/c.md"];
    produced.extend(drift);
    produced.sort_unstable();
    repo.set_sync(&generated, &produced, &drift);
    assert_eq!(
        run(&mut repo, &activation, &generated),
        [
            format!(
                "direct commit:{code} direct commit {} changes code path src/main.ts; code reaches main through a branch merge",
                short(&code)
            ),
            format!(
                "direct commit:{nested} direct commit {} changes code path guide/README.md; code reaches main through a branch merge",
                short(&nested)
            ),
            format!(
                "direct commit:{status} direct commit {} changes rule Enforcement/Status lines in docs/agents/rules/api.md; that is code",
                short(&status)
            ),
            format!(
                "direct commit:{generated} direct commit {} changes .agents/b.md, which differs from gate sync at that commit",
                short(&generated)
            ),
            format!(
                "direct commit:{generated} direct commit {} changes .agents/stale.md, which differs from gate sync at that commit",
                short(&generated)
            ),
            format!(
                "direct commit:{generated} direct commit {} changes .github/workflows/gate.yml, which differs from gate sync at that commit",
                short(&generated)
            ),
        ]
    );
}

#[test]
fn removed_paths_are_judged_against_the_parents_sync_and_documentation_by_its_text_alone() {
    let (mut repo, activation) = start();
    let generated = repo.commit(
        &[&activation],
        "docs: add a rule and a skill",
        &[
            ("docs/agents/rules/old.md", Some("# Old\n")),
            (".agents/old.md", Some("old")),
            (".agents/skill/run.sh", Some("run")),
        ],
    );
    repo.set_sync(&generated, &[".agents/old.md", ".agents/skill/run.sh"], &[]);
    // The source goes, and `gate sync` removes its adapter with it.
    let deleted = repo.commit(
        &[&generated],
        "docs: retire the old rule",
        &[("docs/agents/rules/old.md", None), (".agents/old.md", None)],
    );
    repo.set_sync(&deleted, &[".agents/skill/run.sh"], &[]);
    // A rename of a generated file: without rename detection, a deletion and an addition.
    let renamed = repo.commit(
        &[&deleted],
        "docs: rename the skill script",
        &[
            (".agents/skill/run.sh", None),
            (".agents/skill/start.sh", Some("run")),
        ],
    );
    repo.set_sync(&renamed, &[".agents/skill/start.sh"], &[]);
    // A hand deletion of a file `gate sync` still produces: drift at the commit.
    let hand = repo.commit(
        &[&renamed],
        "docs: drop the skill script",
        &[(".agents/skill/start.sh", None)],
    );
    repo.set_sync(
        &hand,
        &[".agents/skill/start.sh"],
        &[".agents/skill/start.sh"],
    );
    // A file `documentation` matches, edited by hand where `gate sync` writes it: documentation
    // is judged by its text alone, and `gate sync` is not asked for a commit of documentation only.
    let block = repo.commit(
        &[&hand],
        "docs: edit the block by hand",
        &[("AGENTS.md", Some("edited by hand"))],
    );
    repo.set_sync(
        &block,
        &[".agents/skill/start.sh", "AGENTS.md"],
        &[".agents/skill/start.sh", "AGENTS.md"],
    );
    // A hand-written file `gate sync` never produced, deleted.
    let code = repo.commit(&[&block], "chore: drop the entry", &[("src/main.ts", None)]);
    assert_eq!(
        run(&mut repo, &activation, &code),
        [
            format!(
                "direct commit:{hand} direct commit {} changes .agents/skill/start.sh, which differs from gate sync at that commit",
                short(&hand)
            ),
            format!(
                "direct commit:{code} direct commit {} changes code path src/main.ts; code reaches main through a branch merge",
                short(&code)
            ),
        ]
    );
}

#[test]
fn a_direct_commit_with_a_rule_file_that_is_not_utf8_is_reported_not_an_error() {
    let (mut repo, activation) = start();
    let latin = repo.commit(
        &[&activation],
        "docs: latin-1",
        &[("docs/agents/rules/api.md", Some("placeholder"))],
    );
    repo.set_bytes(&latin, "docs/agents/rules/api.md", b"caf\xe9\n");
    assert_eq!(
        run(&mut repo, &activation, &latin),
        [format!(
            "direct commit:{latin} direct commit {} changes docs/agents/rules/api.md, which is not UTF-8 text; its Enforcement/Status lines cannot be compared",
            short(&latin)
        )]
    );
}

#[test]
fn a_push_to_main_classifies_the_pushed_commits_and_a_push_elsewhere_does_not() {
    let (mut repo, activation) = start();
    let code = repo.commit(&[&activation], "fix: quick", &[("src/main.ts", Some("x"))]);
    repo.set_ref("refs/remotes/origin/main", &activation);
    let direct = format!(
        "direct commit:{code} direct commit {} changes code path src/main.ts; code reaches main through a branch merge",
        short(&code)
    );
    let found = |repo: &Repo| {
        lines(
            &repo
                .check("main-history", &settings(&activation), "{}")
                .expect("runs"),
        )
    };

    assert_eq!(found(&repo), Vec::<String>::new());
    repo.set_range(&code, Some("refs/heads/feat/other"));
    assert_eq!(found(&repo), Vec::<String>::new());
    repo.set_range(&code, None);
    assert_eq!(found(&repo), Vec::<String>::new());
    repo.set_range(&code, Some("refs/heads/main"));
    assert_eq!(found(&repo), [direct]);
}

#[test]
fn the_rule_literals_are_options() {
    let (mut repo, activation) = start();
    let a = repo.commit(&[&activation], "a", &[("package.json", Some("{}"))]);
    let merge = repo.commit(
        &[&activation, &a],
        "Merged: bot/a",
        &[("package.json", Some("{}"))],
    );
    let release = repo.commit(
        &[&merge],
        "chore(release): 1.2.0 [skip ci]",
        &[("VERSION", Some("1.2.0"))],
    );
    repo.set_ref("refs/heads/main", &release);
    let forms = r#"{"forms": [
        {"kind": "merge", "parents": 2, "subject": "^Merged: (.+)$"},
        {"kind": "release", "parents": 1}, {"kind": "direct", "parents": 1}]}"#;
    let options = r#"{"dependabot-prefix": "bot/", "release-files": ["VERSION"]}"#;
    assert_eq!(
        repo.check(
            "main-history",
            &merged(&settings(&activation), forms),
            options
        )
        .expect("runs"),
        []
    );
    let error = repo
        .check(
            "main-history",
            &settings(&activation),
            r#"{"merge-subject": "^Merged: (.+)$"}"#,
        )
        .expect_err("the merge subject is a form's");
    assert!(
        error.message().contains("unknown field `merge-subject`"),
        "{error}"
    );
}

/// A subject-based policy: Conventional Commits subjects and `Revert "..."` on main, merges by
/// their `Merge ...` subject, and no check beyond the form.
const SUBJECTS: &str = r#"{"forms": [
    {"kind": "merge", "parents": 2, "subject": "^Merge ", "checks": []},
    {"kind": "direct", "parents": 1, "checks": [],
     "subject": "^((feat|fix|docs|chore|refactor|test)(\\([a-z0-9-]+\\))?!?: .+|Revert \".+\")$"}]}"#;

#[test]
fn a_subject_based_policy_classifies_by_subject_and_parents() {
    let (mut repo, activation) = start();
    let feature = repo.commit(
        &[&activation],
        "feat(api): add the endpoint",
        &[("src/main.ts", Some("export const a = 1;\n"))],
    );
    let work = repo.commit(&[&feature], "wip", &[("src/b.ts", Some("b"))]);
    let merge = repo.commit(
        &[&feature, &work],
        "Merge pull request #3 from team/b-work\n\nAdd b.",
        &[("src/b.ts", Some("b")), ("src/extra.ts", Some("x"))],
    );
    let revert = repo.commit(
        &[&merge],
        "Revert \"feat(api): add the endpoint\"",
        &[("src/main.ts", Some("export {};\n"))],
    );
    let loose = repo.commit(&[&revert], "quick fix", &[("src/c.ts", Some("c"))]);
    let sync = repo.commit(&[&loose, &revert], "Sync with main", &[]);
    let release = repo.commit(
        &[&sync],
        "chore(release): 1.2.0 [skip ci]",
        &[("src/main.ts", Some("x"))],
    );
    repo.set_ref("refs/heads/main", &release);

    // Neither branch-pattern nor documentation nor release-subject: no form reads them. The
    // release commit is a direct commit with a Conventional Commits subject here.
    let settings = format!(r#"{{"main": ["refs/heads/main"], "activation": "{activation}"}}"#);
    let findings = repo
        .check("main-history", &merged(&settings, SUBJECTS), "{}")
        .expect("runs");
    assert_eq!(
        lines(&findings),
        [
            format!(
                "direct commit:{loose} {} has none of the forms of [plugins.git] options forms: subject \"quick fix\", 1 parent",
                short(&loose)
            ),
            format!(
                "merge commit:{sync} {} has none of the forms of [plugins.git] options forms: subject \"Sync with main\", 2 parents",
                short(&sync)
            ),
        ]
    );
    assert!(findings.iter().all(|finding| {
        finding
            .help
            .as_deref()
            .is_some_and(|help| help.contains("forms"))
    }));
}

#[test]
fn each_merge_check_runs_only_when_its_form_lists_it() {
    let (mut repo, activation) = start();
    let work = repo.commit(&[&activation], "a", &[("src/a.ts", Some("a"))]);
    let evil = repo.commit(
        &[&activation, &work],
        "Merge branch 'wip'\n\nWhy.",
        &[("src/a.ts", Some("a")), ("src/extra.ts", Some("x"))],
    );
    let sync = repo.commit(&[&evil, &activation], "Merge branch 'feat/sync'", &[]);
    let tip = repo.commit(&[&sync], "docs: x", &[("src/x.ts", Some("x"))]);
    repo.set_ref("refs/heads/main", &tip);

    let options = |checks: &str| {
        format!(
            r#"{{"forms": [{{"kind": "merge", "parents": 2, "subject": "^Merge branch '([^']+)'$", "checks": {checks}}},
               {{"kind": "direct", "parents": 1, "checks": []}}]}}"#
        )
    };
    let body = format!(
        "merge commit:{evil} {} has an edited merge message body",
        short(&evil)
    );
    let branch = format!(
        "merge commit:{evil} {} merges branch wip, which neither matches branch-pattern within branch-length nor starts with dependabot/",
        short(&evil)
    );
    let tree = format!(
        "merge commit:{evil} {} differs from git's conflict-free merge of its parents; the merge commit adds its own change",
        short(&evil)
    );
    let synced = format!(
        "merge commit:{sync} {} is a sync merge: its second parent is already on main",
        short(&sync)
    );
    for (checks, expected) in [
        ("[]", vec![]),
        (r#"["body"]"#, vec![body.clone()]),
        (r#"["branch"]"#, vec![branch.clone()]),
        (r#"["tree"]"#, vec![tree.clone()]),
        (r#"["sync"]"#, vec![synced.clone()]),
        (
            r#"["body", "branch", "sync", "tree"]"#,
            vec![branch, body, tree, synced],
        ),
    ] {
        let found = repo
            .check(
                "main-history",
                &merged(&settings(&activation), &options(checks)),
                "{}",
            )
            .expect("runs");
        assert_eq!(lines(&found), expected, "{checks}");
    }
}

#[test]
fn the_files_checks_of_release_and_direct_forms_are_optional() {
    let (mut repo, activation) = start();
    let release = repo.commit(
        &[&activation],
        "chore(release): 1.2.0 [skip ci]",
        &[("src/main.ts", Some("x"))],
    );
    let direct = repo.commit(&[&release], "fix: quick", &[("src/main.ts", Some("y"))]);
    repo.set_ref("refs/heads/main", &direct);
    let options = |release: &str, direct: &str| {
        format!(
            r#"{{"forms": [{{"kind": "release", "parents": 1, "checks": {release}}},
               {{"kind": "direct", "parents": 1, "checks": {direct}}}]}}"#
        )
    };
    let released = format!(
        "release commit:{release} release commit {} changes src/main.ts",
        short(&release)
    );
    let code = format!(
        "direct commit:{direct} direct commit {} changes code path src/main.ts; code reaches main through a branch merge",
        short(&direct)
    );
    for (release_checks, direct_checks, expected) in [
        ("[]", "[]", vec![]),
        (r#"["files"]"#, "[]", vec![released.clone()]),
        ("[]", r#"["files"]"#, vec![code.clone()]),
        (r#"["files"]"#, r#"["files"]"#, vec![released, code]),
    ] {
        let found = repo
            .check(
                "main-history",
                &merged(
                    &settings(&activation),
                    &options(release_checks, direct_checks),
                ),
                "{}",
            )
            .expect("runs");
        assert_eq!(lines(&found), expected, "{release_checks} {direct_checks}");
    }
}

#[test]
fn forms_are_tried_in_order_and_an_absent_parent_count_matches_any() {
    let (mut repo, activation) = start();
    let work = repo.commit(&[&activation], "a", &[("src/a.ts", Some("a"))]);
    let merge = repo.commit(
        &[&activation, &work],
        "chore: land a",
        &[("src/a.ts", Some("a"))],
    );
    let direct = repo.commit(&[&merge], "chore: tidy", &[("src/b.ts", Some("b"))]);
    repo.set_ref("refs/heads/main", &direct);
    // The first form takes every `chore:` commit, whatever its parents; the direct form after it
    // would have reported the code change.
    let options = r#"{"forms": [
        {"kind": "merge", "subject": "^chore: ", "checks": ["body"]},
        {"kind": "direct", "parents": 1}]}"#;
    assert_eq!(
        lines(
            &repo
                .check(
                    "main-history",
                    &merged(&settings(&activation), options),
                    "{}"
                )
                .expect("runs")
        ),
        Vec::<String>::new()
    );
}

#[test]
fn incoherent_forms_are_refused_with_what_to_change() {
    let (mut repo, activation) = start();
    repo.set_ref("refs/heads/main", &activation);
    let cases = [
        (r#"{"forms": []}"#, "[plugins.git] options forms is empty"),
        (
            r#"{"forms": [{"kind": "squash"}]}"#,
            "unknown variant `squash`",
        ),
        (
            r#"{"forms": [{"kind": "direct", "checks": ["body"]}]}"#,
            "[plugins.git] options form 1 (direct) has the check body, which a direct form does not run; a direct form runs files",
        ),
        (
            r#"{"forms": [{"kind": "release", "parents": 1, "subject": "^r$"}]}"#,
            "form 1 (release) has a subject",
        ),
        (
            r#"{"forms": [{"kind": "merge", "parents": 2, "subject": "^Merge "}]}"#,
            "form 1 (merge) runs the branch check, which reads the branch name from capture 1",
        ),
        (
            r#"{"forms": [{"kind": "merge", "subject": "^Merge (.+)$", "checks": ["tree"]}]}"#,
            "form 1 (merge) runs the tree check, which compares a merge's two parents; set parents = 2",
        ),
        (
            r#"{"forms": [{"kind": "direct", "parents": 2}]}"#,
            "form 1 (direct) runs the files check, which compares the commit with its one parent; set parents = 1",
        ),
        (
            r#"{"forms": [{"kind": "direct", "parents": 1, "checks": [], "subject": "("}]}"#,
            "[plugins.git] options form 1 (direct) subject \"(\" is not a valid regular expression",
        ),
    ];
    for (options, expected) in cases {
        let error = repo
            .check(
                "main-history",
                &merged(&settings(&activation), options),
                "{}",
            )
            .expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
    }

    let bare = format!(r#"{{"main": ["refs/heads/main"], "activation": "{activation}"}}"#);
    for (options, expected) in [
        (
            r#"{"forms": [{"kind": "release", "parents": 1}]}"#,
            "has no release-subject; [plugins.git] options form 1 (release) matches the release commits by it",
        ),
        (
            r#"{"forms": [{"kind": "direct", "parents": 1}]}"#,
            "has no documentation; the files check of [plugins.git] options form 1 (direct) reads it",
        ),
    ] {
        let error = repo
            .check("main-history", &merged(&bare, options), "{}")
            .expect_err(options);
        assert!(error.message().contains(expected), "{options}: {error}");
    }
}

#[test]
fn a_merge_whose_subject_captures_no_branch_is_reported_by_the_branch_check() {
    let (mut repo, activation) = start();
    let work = repo.commit(&[&activation], "a", &[("src/a.ts", Some("a"))]);
    let named = repo.commit(
        &[&activation, &work],
        "Merge branch 'feat/new-work'",
        &[("src/a.ts", Some("a"))],
    );
    let other = repo.commit(&[&named], "b", &[("src/b.ts", Some("b"))]);
    let request = repo.commit(
        &[&named, &other],
        "Merge pull request #4 from team/b",
        &[("src/b.ts", Some("b"))],
    );
    repo.set_ref("refs/heads/main", &request);
    // Group 1 takes part only in the first alternative.
    let forms = r#"{"forms": [{"kind": "merge", "parents": 2, "checks": ["branch"],
        "subject": "^Merge (?:branch '([^']+)'|pull request .+)$"}]}"#;
    assert_eq!(
        lines(
            &repo
                .check("main-history", &merged(&settings(&activation), forms), "{}")
                .expect("runs")
        ),
        [format!(
            "merge commit:{request} {} subject \"Merge pull request #4 from team/b\" names no branch: capture 1 of its form's subject is empty",
            short(&request)
        )]
    );
}
