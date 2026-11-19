//! The rules about rule files: registry, ids, enforcement, references, size, owner, CLAUDE.md.

use crate::fixture::{Fixture, always, lines, rule_file};

const RULES: &str = "docs/agents/rules";

fn at(file: &str, rest: &str) -> String {
    format!("{RULES}/{file}{rest}")
}

#[test]
fn registry_reports_each_front_matter_heading_and_metadata_problem() {
    let bad = rule_file(
        "area: API\napplies: globs\nowner: D-ONE-001\n",
        "### RULE-API-001 Deep\n\nEnforcement: gate · Status: stable\n\n\
         ## RULE-WEB-002 Other area\nEnforcement: magic · Status: final · Redirect: RULE-API-001\n\n\
         ## RULE-API-003 No line\n\nText.\n\n\
         ## RULE-API-004 Bad line\nEnforcement: gate\n\n\
         Enforcement: gate · Status: stable\n",
    );
    let fixture = Fixture::new(&[
        (&at("a-good.md", ""), &always("API", 2)),
        (&at("bad.md", ""), &bad),
        (&at("c-none.md", ""), "# No front matter\n"),
        (&at("d-yaml.md", ""), "---\narea: [\n---\n"),
        (
            &at("e-lower.md", ""),
            &rule_file(
                "area: web\napplies: always\nglobs: [x]\nowner: D-ONE-001\n",
                "",
            ),
        ),
        (
            &at("f-other.md", ""),
            &rule_file("area: OPS\napplies: sometimes\n", ""),
        ),
        (&at("g-noapplies.md", ""), &rule_file("area: GEN\n", "")),
        (&at("h-noarea.md", ""), &rule_file("applies: always\n", "")),
        (&at("nested/x.md", ""), "# Not a rule file\n"),
        (&at("notes.txt", ""), "Enforcement: stray\n"),
    ]);
    let found = lines(&fixture.check("rule-registry", "{}").expect("runs"));
    let yaml = at("d-yaml.md:1 frontmatter is not valid YAML: ", "");
    assert!(
        found.iter().any(|line| line.starts_with(&yaml)),
        "{found:?}"
    );
    let found: Vec<String> = found
        .into_iter()
        .filter(|line| !line.starts_with(&yaml))
        .collect();
    assert_eq!(
        found,
        [
            at(
                "bad.md",
                ":2 area API is already declared by docs/agents/rules/a-good.md"
            ),
            at("bad.md", ":3 applies: globs needs a non-empty globs list"),
            at(
                "bad.md",
                ":8 RULE-API-001 is a level-3 heading; rule headings are level 2"
            ),
            at(
                "bad.md",
                ":12 RULE-WEB-002 belongs to area WEB, but the file declares API"
            ),
            at(
                "bad.md",
                ":13 RULE-WEB-002 has unknown enforcement class `magic`"
            ),
            at("bad.md", ":13 RULE-WEB-002 has unknown status `final`"),
            at(
                "bad.md",
                ":13 RULE-WEB-002 has a redirect but status `final`"
            ),
            at(
                "bad.md",
                ":15 RULE-API-003 has no Enforcement/Status line after its heading"
            ),
            at(
                "bad.md",
                ":20 RULE-API-004 metadata line is not `Enforcement: <class>[ <detail>] · Status: <status>[ · Redirect: <id>]`"
            ),
            at("bad.md", ":22 an Enforcement line follows no rule heading"),
            at(
                "c-none.md",
                ":1 a rule file starts with frontmatter that declares area, applies and owner"
            ),
            at(
                "e-lower.md",
                ":2 area web is not 2-8 uppercase letters or digits"
            ),
            at(
                "e-lower.md",
                ":4 an always-applying rule file declares no globs"
            ),
            at(
                "f-other.md",
                ":3 applies is `sometimes`, not `always` or `globs`"
            ),
            at("g-noapplies.md", ":1 frontmatter has no applies"),
            at("h-noarea.md", ":1 frontmatter has no area"),
        ]
    );
}

#[test]
fn ids_are_well_formed_unique_and_numbered_without_gaps() {
    let api = rule_file(
        "area: API\napplies: always\nowner: D-ONE-001\n",
        "## RULE-API-001 One\n\nEnforcement: gate · Status: stable\n\n\
         ## RULE-API-004 Four\n\nEnforcement: gate · Status: removed\n\n\
         ## RULE-API-1 Short\n\nEnforcement: gate · Status: stable\n",
    );
    let fixture = Fixture::new(&[
        (&at("api.md", ""), &api),
        (&at("web.md", ""), &always("WEB", 1)),
        (
            &at("zz.md", ""),
            &rule_file(
                "area: ZZ\n",
                "## RULE-API-001 Again\n\nEnforcement: gate · Status: stable\n",
            ),
        ),
    ]);
    let findings = fixture.check("rule-ids", "{}").expect("runs");
    let gap = |number: &str| {
        at(
            "api.md",
            &format!(
                " RULE-API-{number} is missing; identifiers are never deleted or reused, so keep it with status removed"
            ),
        )
    };
    assert_eq!(
        lines(&findings),
        [
            at("api.md", ":16 RULE-API-1 is not RULE-<AREA>-<NNN>"),
            at(
                "zz.md",
                ":6 RULE-API-001 is already defined in docs/agents/rules/api.md"
            ),
            gap("002"),
            gap("003"),
        ]
    );
    assert_eq!(findings[2].target.as_deref(), Some("RULE-API-002"));
}

const ENFORCED: &str = "\
## RULE-API-001 Gate with detail
Enforcement: gate extra · Status: stable
## RULE-API-002 Gate unmapped
Enforcement: gate · Status: stable
## RULE-API-003 Gate mapped
Enforcement: gate · Status: stable
## RULE-API-004 Eslint short
Enforcement: eslint no-console · Status: stable
## RULE-API-005 Eslint other root
Enforcement: eslint no-console src/**/*.ts cdk · Status: stable
## RULE-API-006 Eslint bad glob
Enforcement: eslint no-console src/[ root · Status: stable
## RULE-API-007 Typecheck untracked
Enforcement: typecheck strict tsconfig.missing.json · Status: stable
## RULE-API-008 Typecheck unset
Enforcement: typecheck noImplicitAny tsconfig.json · Status: stable
## RULE-API-009 Typecheck set
Enforcement: typecheck strict tsconfig.json · Status: stable
## RULE-API-010 Test missing
Enforcement: test tests/missing.test.ts · Status: stable
## RULE-API-011 Test present
Enforcement: test tests/present.test.ts · Status: stable
## RULE-API-012 Review without reason
Enforcement: review · Status: stable
## RULE-API-013 Advisory with reason
Enforcement: advisory -- judgement · Status: stable
## RULE-API-014 Advisory without reason
Enforcement: advisory · Status: stable
## RULE-API-015 Test present again
Enforcement: test tests/present.test.ts · Status: stable
## RULE-API-016 Removed
Enforcement: gate extra · Status: removed
## RULE-API-017 Review mapped
Enforcement: review -- by hand · Status: stable
";

fn enforcement_fixture() -> Fixture {
    Fixture::new(&[
        (
            &at("api.md", ""),
            &rule_file("area: API\napplies: always\nowner: D-ONE-001\n", ENFORCED),
        ),
        (
            "tsconfig.json",
            "{\"compilerOptions\": {\"strict\": true}}\n",
        ),
        ("tests/present.test.ts", "test();\n"),
    ])
}

const NORMS: [&str; 4] = [
    "RULE-API-003",
    "RULE-API-017",
    "RULE-API-099",
    "D-OTHER-001",
];

#[test]
fn enforcement_holds_each_class_to_its_grammar_and_norms_to_gate_rules() {
    let findings = enforcement_fixture()
        .check_with_norms("rule-enforcement", "{}", &NORMS)
        .expect("runs");
    assert_eq!(
        lines(&findings),
        [
            at(
                "api.md",
                ":9 gate enforcement takes no detail; the check is mapped in gate.toml"
            ),
            at(
                "api.md",
                ":11 RULE-API-002 declares gate enforcement, but no check in gate.toml reports it"
            ),
            at(
                "api.md",
                ":15 eslint enforcement is `eslint <rule-id> <files-glob> <root>`"
            ),
            at(
                "api.md",
                ":17 eslint enforcement is `eslint <rule-id> <files-glob> <root>`"
            ),
            at("api.md", ":19 `src/[` is not a valid glob"),
            at("api.md", ":21 tsconfig.missing.json is not tracked"),
            at("api.md", ":23 tsconfig.json does not set noImplicitAny"),
            at(
                "api.md",
                ":27 the owning test tests/missing.test.ts is not tracked"
            ),
            at("api.md", ":31 review enforcement needs `-- <reason>`"),
            at("api.md", ":35 advisory enforcement needs `-- <reason>`"),
            "- a configured rule maps to RULE-API-017, whose enforcement class is review"
                .to_owned(),
            "- a configured rule maps to RULE-API-099, which no rule file defines".to_owned(),
        ]
    );
}

#[test]
fn enforcement_options_widen_eslint_roots() {
    let findings = enforcement_fixture()
        .check_with_norms(
            "rule-enforcement",
            r#"{"eslint-roots": ["root", "cdk"]}"#,
            &NORMS,
        )
        .expect("runs");
    let found = lines(&findings);
    let of = |line: &str| {
        found
            .iter()
            .filter(|found| found.starts_with(&at("api.md", line)))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        of(":15 "),
        [at(
            "api.md",
            ":15 eslint enforcement is `eslint <rule-id> <files-glob> <root|cdk>`"
        )]
    );
    assert_eq!(of(":17 "), Vec::<String>::new());
}

#[test]
fn enforcement_has_no_legacy_class_or_option() {
    let error = enforcement_fixture()
        .check("rule-enforcement", r#"{"legacy": false}"#)
        .expect_err("an unknown option is refused");
    assert!(error.to_string().contains("legacy"), "{error}");
}

#[test]
fn references_resolve_to_live_rules_in_every_markdown_file() {
    let api = rule_file(
        "area: API\napplies: always\nowner: D-ONE-001\n",
        "## RULE-API-001 Live\nEnforcement: gate · Status: stable\n\
         ## RULE-API-002 Gone\nEnforcement: review -- old · Status: removed · Redirect: RULE-API-001\n\
         ## RULE-API-003 Gone without redirect\nEnforcement: review -- old · Status: removed\n\
         ## RULE-API-004 Points nowhere\nEnforcement: review -- old · Status: deprecated · Redirect: RULE-API-404\n",
    );
    let fixture = Fixture::new(&[
        (&at("api.md", ""), &api),
        (
            "guide/notes.md",
            "See `RULE-API-001`, `RULE-API-002` and `RULE-API-003`.\n\n\
             `RULE-NOPE-001` and `RULE-API-01` and RULE-API-005.\n\n```\n`RULE-API-777`\n```\n",
        ),
        ("docs/README.MD", "`RULE-API-555`\n"),
        ("src/code.ts", "// `RULE-API-999`\n"),
    ]);
    assert_eq!(
        lines(&fixture.check("rule-references", "{}").expect("runs")),
        [
            at(
                "api.md",
                ":15 RULE-API-004 redirects to unknown RULE-API-404"
            ),
            "docs/README.MD:1 RULE-API-555 is not defined by any rule file".to_owned(),
            "guide/notes.md:1 RULE-API-002 is removed; use RULE-API-001".to_owned(),
            "guide/notes.md:1 RULE-API-003 is removed".to_owned(),
            "guide/notes.md:3 RULE-NOPE-001 is not defined by any rule file".to_owned(),
        ]
    );
}

#[test]
fn references_survive_markdown_that_is_not_utf8() {
    let api = rule_file(
        "area: API\napplies: always\nowner: D-ONE-001\n",
        "## RULE-API-001 Live\nEnforcement: gate · Status: stable\n",
    );
    let fixture = Fixture::new(&[(&at("api.md", ""), &api)])
        .with_bytes("notes/latin1.md", b"caf\xe9\n")
        .with_bytes(
            "notes/cited.md",
            b"caf\xe9 `RULE-API-555` and `RULE-API-001`\n",
        );
    assert_eq!(
        lines(&fixture.check("rule-references", "{}").expect("runs")),
        ["notes/cited.md:1 RULE-API-555 is not defined by any rule file"]
    );
}

#[test]
fn size_reports_above_max_lines_or_else_above_warn_lines() {
    let sized = |lines: usize| {
        let head = always("API", 1);
        let used = head.lines().count();
        format!("{head}{}", "text\n".repeat(lines - used))
    };
    let fixture = Fixture::new(&[
        (&at("big.md", ""), &sized(501)),
        (&at("mid.md", ""), &sized(201)),
        (&at("ok.md", ""), &sized(200)),
    ]);
    let max = |file: &str, rest: &str| format!("#max-lines {}", at(file, rest));
    let warn = |file: &str, rest: &str| format!("#warn-lines {}", at(file, rest));
    assert_eq!(
        lines(&fixture.check("rule-file-size", "{}").expect("runs")),
        [
            max("big.md", " 501 lines exceed the 500-line limit"),
            warn(
                "mid.md",
                " 201 lines exceed 200; consider splitting the area"
            ),
        ]
    );
    assert_eq!(
        lines(
            &fixture
                .check("rule-file-size", r#"{"max-lines": 300, "warn-lines": 150}"#)
                .expect("runs")
        ),
        [
            max("big.md", " 501 lines exceed the 300-line limit"),
            warn(
                "mid.md",
                " 201 lines exceed 150; consider splitting the area"
            ),
            warn(
                "ok.md",
                " 200 lines exceed 150; consider splitting the area"
            ),
        ]
    );
    let error = fixture
        .check("rule-file-size", r#"{"max-lines": 10, "warn-lines": 20}"#)
        .expect_err("refuses warn above max");
    assert!(
        error
            .to_string()
            .contains("warn-lines 20 is above max-lines 10"),
        "{error}"
    );
}

const INDEX: &str = "\
# Decisions

| Id | Status | Title | File |
| --- | --- | --- | --- |
| `D-ONE-001` | `accepted` | One | `decisions/one.md` |
| `D-TWO-001` | proposed | Two | `decisions/two.md` |
| `D-THREE-001` | accepted | Short |
| `D-ONE-001` | rejected | Duplicate | x |
";

fn owned(owner: Option<&str>) -> String {
    let owner = owner.map_or_else(String::new, |owner| format!("owner: {owner}\n"));
    rule_file(&format!("area: AAA\napplies: always\n{owner}"), "")
}

#[test]
fn owners_are_accepted_decisions_of_the_index() {
    let rules = [
        (at("a.md", ""), owned(Some("D-ONE-001"))),
        (at("b.md", ""), owned(Some("D-TWO-001"))),
        (at("c.md", ""), owned(None)),
        (at("d.md", ""), owned(Some("D-THREE-001"))),
        (at("e.md", ""), "# No front matter\n".to_owned()),
    ];
    let mut files: Vec<(&str, &str)> = rules
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let without_index = Fixture::new(&files);
    files.push(("docs/ledger/decision-index.md", INDEX));
    let fixture = Fixture::new(&files);
    let refused = |file: &str, owner: &str| {
        at(
            file,
            &format!(":4 owner {owner} is not an accepted decision in the decision index"),
        )
    };
    let no_owner = at("c.md", ":1 frontmatter has no owner decision");

    assert_eq!(
        lines(&fixture.check("rule-owner", "{}").expect("runs")),
        [
            refused("b.md", "D-TWO-001"),
            no_owner.clone(),
            refused("d.md", "D-THREE-001")
        ]
    );
    assert_eq!(
        lines(
            &fixture
                .check("rule-owner", r#"{"accepted-status": "proposed"}"#)
                .expect("runs")
        ),
        [
            refused("a.md", "D-ONE-001"),
            no_owner.clone(),
            refused("d.md", "D-THREE-001")
        ]
    );
    assert_eq!(
        lines(&without_index.check("rule-owner", "{}").expect("runs")),
        [
            refused("a.md", "D-ONE-001"),
            refused("b.md", "D-TWO-001"),
            no_owner,
            refused("d.md", "D-THREE-001"),
        ]
    );
    let moved = Fixture::new(&[
        (&at("a.md", ""), &owned(Some("D-ONE-001"))),
        ("records/index.md", INDEX),
    ]);
    assert_eq!(
        lines(
            &moved
                .check("rule-owner", r#"{"decisions": "records/index.md"}"#)
                .expect("runs")
        ),
        Vec::<String>::new()
    );
    let empty = Fixture::new(&[("README.md", "# Project\n")]);
    assert_eq!(empty.check("rule-owner", "{}").expect("runs"), []);
}

#[test]
fn no_claude_md_runs_once_for_the_whole_repository() {
    let info = gate_sdk::describe(&agents::PLUGIN);
    let rule = info
        .rules
        .iter()
        .find(|rule| rule.name == "no-claude-md")
        .expect("declared");
    assert_eq!(rule.scope, gate_sdk::Scope::Repository);
}

#[test]
fn a_tracked_claude_md_is_reported_at_any_depth_and_case() {
    let fixture = Fixture::new(&[
        ("CLAUDE.md", "x\n"),
        ("apps/web/claude.MD", "x\n"),
        ("docs/CLAUDE.md.bak", "x\n"),
        ("docs/gemini.md", "x\n"),
    ]);
    let message = "a tracked CLAUDE.md disables AGENTS.md; use AGENTS.md and docs/agents instead";
    assert_eq!(
        lines(&fixture.check("no-claude-md", "{}").expect("runs")),
        [
            format!("CLAUDE.md {message}"),
            format!("apps/web/claude.MD {message}")
        ]
    );
    assert_eq!(
        lines(
            &fixture
                .check("no-claude-md", r#"{"file-name": "GEMINI.md"}"#)
                .expect("runs")
        ),
        [format!("docs/gemini.md {message}")]
    );
}
