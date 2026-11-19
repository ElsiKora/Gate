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
        ("modules", "is the standard entry docs/modules/"),
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
        ["sections docs/architecture/README.md docs/architecture/README.md#Routing"]
    );
    assert_eq!(
        messages(&findings),
        [
            "docs/architecture/README.md has no section \"Routing\"; in a frontend it comes after \"System shape\""
        ]
    );

    let service = kind("service");
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
        ["sections docs/README.md docs/README.md#Members"]
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
