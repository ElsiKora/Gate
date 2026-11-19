use crate::fixture::{Fixture, lines};

const FULL: &str =
    "---\ntitle: Page\ndescription: Purpose\ncategory: guide\nstatus: current\n---\n# Page\n";

fn message(path: &str) -> String {
    format!("{path} {path} must have title, description, category, and status frontmatter")
}

#[test]
fn a_page_with_every_key_passes_and_one_without_is_reported() {
    let fixture = Fixture::new(&[
        ("docs/good.md", FULL),
        ("docs/bare.md", "# Bare\n"),
        ("docs/partial.md", "---\ntitle: Page\n---\n"),
        (
            "docs/twice.md",
            &FULL.replace("status: current", "status: a\nstatus: b"),
        ),
        ("docs/empty.md", &FULL.replace("status: current", "status:")),
    ]);
    let findings = fixture.check("frontmatter", "{}").expect("runs");
    assert_eq!(
        lines(&findings),
        [
            message("docs/bare.md"),
            message("docs/empty.md"),
            message("docs/partial.md"),
            message("docs/twice.md"),
        ]
    );
    assert_eq!(findings[0].target.as_deref(), Some("docs/bare.md"));
}

#[test]
fn record_bodies_need_keys_only_once_they_open_a_block_and_their_readmes_always() {
    let fixture = Fixture::new(&[
        ("docs/adr/0001-choice.md", "# Choice\n- Status: accepted\n"),
        ("docs/ledger/items/one.md", "---\ntitle: Partial\n---\n"),
        ("docs/ledger/README.md", "# Ledger\n"),
        (
            "docs/agents/skills/review/SKILL.md",
            "---\nname: review\n---\n",
        ),
    ]);
    let findings = fixture.check("frontmatter", "{}").expect("runs");
    assert_eq!(
        lines(&findings),
        [
            message("docs/ledger/README.md"),
            message("docs/ledger/items/one.md")
        ]
    );
}

#[test]
fn the_options_set_the_keys_the_pages_and_the_exemptions() {
    let fixture = Fixture::new(&[
        ("manual/a.md", "---\ntitle: A\n---\n"),
        ("manual/b.md", "# B\n"),
        ("manual/drafts/c.md", "# C\n"),
        ("manual/records/d.md", "# D\n"),
        ("docs/e.md", "# E\n"),
    ]);
    let findings = fixture
        .check(
            "frontmatter",
            r#"{"roots": ["manual"], "required-keys": ["title", "owner"],
                "exclude": ["manual/drafts/"], "record-directories": ["manual/records/"]}"#,
        )
        .expect("runs");
    assert_eq!(
        lines(&findings),
        [
            "manual/a.md manual/a.md must have title and owner frontmatter",
            "manual/b.md manual/b.md must have title and owner frontmatter",
        ]
    );
}

#[test]
fn a_page_that_is_not_utf8_is_read_with_replacement_characters() {
    let fixture = Fixture::new(&[]).with_bytes("docs/latin.md", b"# Caf\xe9\n");
    let findings = fixture.check("frontmatter", "{}").expect("runs");
    assert_eq!(lines(&findings), [message("docs/latin.md")]);
}

#[test]
fn no_required_key_is_a_configuration_error() {
    let error = Fixture::new(&[])
        .check("frontmatter", r#"{"required-keys": []}"#)
        .expect_err("empty");
    assert!(
        error.message().contains("required-keys is empty"),
        "{error}"
    );
}
