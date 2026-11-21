//! `docs/layout`, one case per aspect, each with a passing twin: a complete book of its kind,
//! filled, passes; each change below gives exactly the findings named.

use crate::fixture::{Fixture, keys, kind, messages};

const MARKER: &str = "<!-- gate:seed: replace this page's text, then delete this line -->\n";

/// A complete, filled book of `name`: the seeds of its kind without their marker lines.
fn book(name: &str) -> Fixture {
    let seeded = Fixture::from_output(&Fixture::new(&[]).generate(&kind(name)).expect("seeds"));
    seeded.replaced(MARKER, "")
}

fn check(fixture: &Fixture, options: &str) -> Vec<String> {
    keys(&fixture.check(options).expect("checks"))
}

#[test]
fn a_complete_book_passes_in_every_kind() {
    for name in [
        "service",
        "frontend",
        "infrastructure",
        "library",
        "monorepo-root",
    ] {
        assert_eq!(
            check(&book(name), &kind(name)),
            Vec::<String>::new(),
            "{name}"
        );
    }
}

#[test]
fn each_required_entry_absent_gives_one_missing_finding() {
    let service = kind("service");
    assert_eq!(
        check(&book("service").without("docs/start-here.md").replaced("| [start-here.md](start-here.md) | the first reading path, for a person or an agent |\n", ""), &service),
        ["missing docs/start-here.md start-here.md"]
    );
    let without_index = book("service")
        .without("docs/testing/README.md")
        .with("docs/testing/unit.md", "# Unit\n");
    assert_eq!(
        check(&without_index, &service),
        ["missing docs/testing/README.md testing"],
        "a directory without its index is missing, not entry-index"
    );
    let empty = check(&Fixture::new(&[]), &kind("monorepo-root"));
    assert_eq!(empty.len(), 8, "{empty:?}");
    assert!(
        empty.iter().all(|line| line.starts_with("missing ")),
        "{empty:?}"
    );
}

/// Every required entry of each kind, removed alone from a complete book of that kind, gives exactly
/// its one `missing` finding.
#[test]
fn each_required_entry_of_each_kind_absent_alone_gives_its_one_missing_finding() {
    const COMMON: [(&str, &str); 9] = [
        ("docs/README.md", "README.md"),
        ("docs/start-here.md", "start-here.md"),
        ("docs/architecture/README.md", "architecture"),
        ("docs/modules/README.md", "modules"),
        ("docs/adr/README.md", "adr"),
        ("docs/reference/README.md", "reference"),
        ("docs/guide/README.md", "guide"),
        ("docs/testing/README.md", "testing"),
        ("docs/operations/README.md", "operations"),
    ];
    for name in [
        "service",
        "frontend",
        "infrastructure",
        "library",
        "monorepo-root",
    ] {
        let options = kind(name);
        let mut required: Vec<(&str, &str)> = COMMON
            .iter()
            .copied()
            .filter(|(_, entry)| name != "monorepo-root" || *entry != "modules")
            .collect();
        if name == "library" {
            required.push(("docs/releases/README.md", "releases"));
        }
        assert_eq!(check(&book(name), &options), Vec::<String>::new(), "{name}");
        for (index, entry) in required {
            assert_eq!(
                check(&book(name).without(index), &options),
                [format!("missing {index} {entry}")],
                "{name}: {entry}"
            );
        }
    }
    let pages_only = book("library")
        .without("docs/releases/README.md")
        .with("docs/releases/1.0.0.md", "# 1.0.0\n");
    assert_eq!(
        check(&pages_only, &kind("library")),
        ["missing docs/releases/README.md releases"],
        "required in a library, so missing and not entry-index"
    );
}

#[test]
fn an_unknown_top_level_entry_is_reported_with_where_its_content_goes() {
    let service = kind("service");
    let fixture = book("service")
        .with("docs/concepts/a.md", "# A\n")
        .with("docs/ai/map.md", "# Map\n")
        .with("docs/how-to/x.md", "# X\n")
        .with("docs/glossary.md", "# Glossary\n")
        .with("docs/notes/n.md", "# N\n")
        .with("docs/architecture.md", "# Architecture\n")
        .with("docs/.gitkeep", "");
    let findings = fixture.check(&service).expect("checks");
    assert_eq!(
        keys(&findings),
        [
            "unknown docs/.gitkeep .gitkeep",
            "unknown docs/ai ai",
            "unknown docs/architecture.md architecture.md",
            "unknown docs/concepts concepts",
            "unknown docs/glossary.md glossary.md",
            "unknown docs/how-to how-to",
            "unknown docs/notes notes",
        ]
    );
    let messages = messages(&findings);
    for expected in [
        "docs/concepts/ is reserved: its content goes to docs/architecture/",
        "docs/ai/ is reserved: its content goes to docs/agents/",
        "docs/how-to/ is reserved: its content goes to docs/guide/",
        "docs/glossary.md is reserved: its content goes to docs/reference/glossary.md",
        "docs/architecture.md is not an entry: the entry is docs/architecture/, with its README.md",
    ] {
        assert!(
            messages.iter().any(|message| message == expected),
            "{expected}: {messages:?}"
        );
    }

    let nested = book("service")
        .with("docs/guide/examples/x.json", "{}\n")
        .with("docs/architecture/map.tsv", "a\tb\n");
    assert_eq!(check(&nested, &service), Vec::<String>::new());
}

#[test]
fn a_monorepo_root_has_no_ledger_and_no_modules_and_says_why() {
    let root = kind("monorepo-root");
    let fixture = book("monorepo-root")
        .with("docs/ledger/index.md", "# Ledger\n")
        .with("docs/modules/README.md", "# Modules\n");
    let findings = fixture.check(&root).expect("checks");
    assert_eq!(
        keys(&findings),
        ["unknown docs/ledger ledger", "unknown docs/modules modules"]
    );
    assert!(
        messages(&findings).contains(
            &"docs/ledger/ is not an entry of a monorepo root: work is recorded in the ledger of the member project whose code it changes, and a repository-wide decision is an adr/ record"
                .to_owned()
        ),
        "{findings:?}"
    );
    // Each says what the entries of the kind are and how a project declares one of its own.
    for finding in &findings {
        let help = finding.help.as_deref().unwrap_or_default();
        assert!(
            help.starts_with("the entries of a monorepo root are docs/README.md, ")
                && !help.contains("docs/ledger/")
                && help.ends_with(
                    "declare another in the [plugins.docs] options as extra = [{ name = \"<name>\", job = \"<what it holds>\" }]"
                ),
            "{help}"
        );
    }
}

#[test]
fn delegated_entries_are_their_owners_business() {
    let service = kind("service");
    assert_eq!(check(&book("service"), &service), Vec::<String>::new());
    let with = book("service")
        .with("docs/ledger/index.md", "# Ledger\n")
        .with("docs/agents/rules/a.md", "# A\n")
        .replaced(
            "| [operations/](operations/README.md) | runbooks: deploy, roll back, recover, rotate, release |\n",
            "| [operations/](operations/README.md) | runbooks |\n| [agents/](agents/) | agents |\n| [ledger/](ledger/index.md) | ledger |\n",
        );
    assert_eq!(
        check(&with, &service),
        Vec::<String>::new(),
        "no README.md needed inside a delegated entry"
    );
}

#[test]
fn a_declared_extension_passes_after_the_standard_and_a_missing_one_is_reported() {
    let options =
        r#"{"kind": "service", "extra": [{"name": "playbooks", "job": "Plays for incidents"}]}"#;
    let with = book("service")
        .with("docs/playbooks/README.md", "# Playbooks\n")
        .replaced(
            "| [operations/](operations/README.md) | runbooks: deploy, roll back, recover, rotate, release |\n",
            "| [operations/](operations/README.md) | runbooks |\n| [playbooks/](playbooks/README.md) | plays |\n",
        );
    assert_eq!(check(&with, options), Vec::<String>::new());
    assert_eq!(
        check(&book("service"), options),
        ["extra-missing docs/playbooks playbooks"]
    );
    let first = with.replaced(
        "| [start-here.md](start-here.md)",
        "| [playbooks/](playbooks/README.md) | plays |\n| [start-here.md](start-here.md)",
    );
    assert_eq!(
        check(&first, options),
        ["index docs/README.md order start-here.md"],
        "an extension comes after the standard's entries"
    );
}

#[test]
fn a_declared_extension_that_breaks_the_rules_is_a_configuration_error_naming_the_key() {
    for (name, says) in [
        ("concepts", "is reserved"),
        ("ledger", "is the standard entry docs/ledger/"),
        ("ledger.md", "is the standard entry docs/ledger/"),
        ("modules", "is the standard entry docs/modules/"),
        ("start-here", "is the standard entry docs/start-here.md"),
        ("start-here.md", "is the standard entry docs/start-here.md"),
        ("readme", "is the standard entry docs/README.md"),
        ("readme.md", "is the standard entry docs/README.md"),
        ("releases", "is the standard entry docs/releases/"),
        ("Playbooks", "is not kebab-case"),
    ] {
        let options =
            format!(r#"{{"kind": "monorepo-root", "extra": [{{"name": "{name}", "job": "x"}}]}}"#);
        let error = book("monorepo-root")
            .check(&options)
            .expect_err("a configuration error");
        assert!(error.is_config(), "{name}: {error}");
        assert!(
            error
                .to_string()
                .contains(&format!("extra[0].name {name:?} {says}")),
            "{name}: {error}"
        );
    }
    let error = book("service")
        .check(r#"{"kind": "service", "extra": [{"name": "playbooks", "job": " "}]}"#)
        .expect_err("an empty job");
    assert!(
        error.to_string().contains("extra[0].job is empty"),
        "{error}"
    );
}

#[test]
fn an_optional_or_declared_directory_without_its_index_is_reported() {
    let options = r#"{"kind": "service", "extra": [{"name": "playbooks", "job": "Plays"}]}"#;
    let fixture = book("service")
        .with("docs/research/a.md", "# A\n")
        .with("docs/playbooks/b.md", "# B\n")
        .replaced(
            "| [operations/](operations/README.md) | runbooks: deploy, roll back, recover, rotate, release |\n",
            "| [operations/](operations/README.md) | runbooks |\n| [research/](research/a.md) | research |\n| [playbooks/](playbooks/b.md) | plays |\n",
        );
    assert_eq!(
        check(&fixture, options),
        [
            "entry-index docs/playbooks/README.md playbooks",
            "entry-index docs/research/README.md research"
        ]
    );
}

#[test]
fn the_index_links_every_present_entry_in_the_standard_order() {
    let service = kind("service");
    let unlinked = book("service").with("docs/research/README.md", "# Research\n");
    assert_eq!(
        check(&unlinked, &service),
        ["index docs/README.md link research"]
    );
    let swapped = book("service")
        .replaced("| [adr/](adr/README.md)", "| [ADR-PLACEHOLDER]")
        .replaced(
            "| [guide/](guide/README.md)",
            "| [adr/](adr/README.md) | records |\n| [guide/](guide/README.md)",
        )
        .replaced(
            "| [ADR-PLACEHOLDER] | decision records: the log and one NNNN-<slug>.md per decision |\n",
            "",
        );
    assert_eq!(
        check(&swapped, &service),
        ["index docs/README.md order adr"],
        "adr/ listed after reference/ is out of order"
    );
}

#[test]
fn an_index_has_its_kinds_sections_in_order_and_may_add_more_after() {
    let frontend = kind("frontend");
    let without_routing = book("frontend").replaced(
        "## Routing\n\nThe routes, and how a request reaches a page.\n",
        "",
    );
    let findings = without_routing.check(&frontend).expect("checks");
    assert_eq!(
        keys(&findings),
        ["sections docs/architecture/README.md docs/architecture/README.md#missing"]
    );
    assert_eq!(
        messages(&findings),
        [
            "docs/architecture/README.md has no section \"Routing\"; in a frontend it comes after \"System shape\""
        ]
    );

    assert!(
        findings[0]
            .help
            .as_deref()
            .is_some_and(|help| help.starts_with(
                "write each as a level-2 heading, ## <title>, in this order: System shape, Routing"
            )),
        "{findings:?}"
    );

    // Several absent sections are one finding, which names them in the standard's order.
    let service = kind("service");
    let bare = book("service")
        .replaced(
            "## Mental model\n\nWhat this project is, in a few paragraphs, and the words it uses.\n",
            "",
        )
        .replaced("## First day\n\nWhat to read, run and change first.\n", "");
    let findings = bare.check(&service).expect("checks");
    assert_eq!(
        keys(&findings),
        ["sections docs/start-here.md docs/start-here.md#missing"]
    );
    assert_eq!(
        messages(&findings),
        [
            "docs/start-here.md lacks the sections \"Mental model\", \"First day\"; in a service they are: Mental model, First day"
        ]
    );
    assert_eq!(
        findings[0].help.as_deref(),
        Some("write each as a level-2 heading, ## <title>, in this order: Mental model, First day")
    );

    let reordered = book("service")
        .replaced("## Data\n", "## Runtime-tmp\n")
        .replaced("## Runtime\n", "## Data\n")
        .replaced("## Runtime-tmp\n", "## Runtime\n");
    assert_eq!(
        check(&reordered, &service),
        ["sections docs/architecture/README.md:12 docs/architecture/README.md#order"]
    );

    let more = book("service").with(
        "docs/testing/README.md",
        "# Testing\n\n## Kinds of test\n\nUnit.\n\n## Evidence\n\nLogs.\n\n## Fixtures\n\nSynthetic.\n",
    );
    assert_eq!(check(&more, &service), Vec::<String>::new());

    let root = kind("monorepo-root");
    let no_members = book("monorepo-root").replaced("## Members\n", "## Others\n");
    assert_eq!(
        check(&no_members, &root),
        ["sections docs/README.md docs/README.md#missing"]
    );
}

#[test]
fn a_seeded_page_is_unfilled_until_its_marker_line_is_deleted() {
    let service = kind("service");
    let seeded = Fixture::from_output(&Fixture::new(&[]).generate(&service).expect("seeds"));
    let unfilled = check(&seeded, &service);
    assert_eq!(unfilled.len(), 9, "{unfilled:?}");
    assert!(unfilled.contains(&"unfilled docs/start-here.md:3 docs/start-here.md".to_owned()));
    let one_left = seeded.replaced(MARKER, "").with(
        "docs/guide/README.md",
        &format!("# Guide\n\n{MARKER}\n## Pages\n\nNone.\n"),
    );
    assert_eq!(
        check(&one_left, &service),
        ["unfilled docs/guide/README.md:3 docs/guide/README.md"]
    );
}

#[test]
fn a_listed_delegated_entry_is_linked_by_the_seed_and_must_be_tracked() {
    let options = r#"{"kind": "service", "delegated": ["agents", "ledger"]}"#;
    let tracked = || {
        Fixture::from_output(&Fixture::new(&[]).generate(options).expect("seeds"))
            .replaced(MARKER, "")
            .with("docs/agents/rules/general.md", "# General\n")
            .with("docs/ledger/manifest.json", "{}\n")
    };
    let unlinked = || {
        tracked().replaced(
            "| [ledger/](ledger/) | work records, verification, follow-ups |\n",
            "",
        )
    };
    assert_eq!(check(&tracked(), options), Vec::<String>::new());
    assert_eq!(
        check(&tracked().without("docs/ledger/manifest.json"), options),
        ["delegated-missing docs/ledger ledger"]
    );
    assert_eq!(
        check(&unlinked(), options),
        ["index docs/README.md link ledger"]
    );
    assert_eq!(
        check(&unlinked().without("docs/ledger/manifest.json"), options),
        [
            "delegated-missing docs/ledger ledger",
            "index docs/README.md link ledger"
        ],
        "a listed entry needs its row whether or not it is tracked"
    );
    let unlisted = r#"{"kind": "service", "delegated": ["agents"]}"#;
    assert_eq!(
        check(&unlinked(), unlisted),
        ["index docs/README.md link ledger"],
        "a delegated directory that exists needs its row, listed or not"
    );
}

#[test]
fn a_delegated_list_that_breaks_the_rules_is_a_configuration_error_naming_the_key() {
    for (options, says) in [
        (
            r#"{"kind": "service", "delegated": ["research"]}"#,
            r#"delegated[0] "research" is not a delegated entry; list one of agents, ledger"#,
        ),
        (
            r#"{"kind": "monorepo-root", "delegated": ["agents", "ledger"]}"#,
            r#"delegated[1] "ledger" is not an entry of a monorepo root: "#,
        ),
        (
            r#"{"kind": "service", "delegated": ["agents", "agents"]}"#,
            r#"delegated[1] "agents" is listed twice"#,
        ),
    ] {
        let error = book("service")
            .check(options)
            .expect_err("a configuration error");
        assert!(error.is_config(), "{options}: {error}");
        assert!(error.to_string().contains(says), "{options}: {error}");
    }
}
