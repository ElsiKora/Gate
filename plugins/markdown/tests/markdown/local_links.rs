use crate::fixture::{Fixture, lines};

const TARGETS: &[(&str, &str)] = &[
    ("README.md", "# Project\n"),
    ("docs/guide/setup.md", "# Setup\n"),
    ("docs/images/flow.svg", "<svg/>\n"),
    ("docs/space name.md", "# Space\n"),
    ("src/main.rs", "fn main() {}\n"),
];

fn tree(page: &str) -> Fixture {
    let mut files = TARGETS.to_vec();
    files.push(("docs/index.md", page));
    Fixture::new(&files)
}

#[test]
fn links_to_tracked_files_directories_anchors_and_other_sites_pass() {
    let page = "\
---
title: [ignored](missing-in-front-matter.md)
---
# Index

[Setup](guide/setup.md) and [again](./guide/setup.md#install) and [root](../README.md?plain=1).
![Flow](images/flow.svg) in [the guide folder](guide/) and [the code](../src).
[Here](#index), [space](space%20name.md), [site](https://example.org/x.md), <https://example.org>.
[mail](mailto:someone@mail.example), <someone@mail.example>, [any](examples/*.md), [abs](/docs/absent.md), [up](..).

[setup]: guide/setup.md
[site]: https://example.org
";
    let findings = tree(page).check("local-links", "{}").expect("runs");
    assert_eq!(lines(&findings), Vec::<String>::new());
}

#[test]
fn every_kind_of_missing_destination_is_reported_on_its_line() {
    let page = "\
# Index

[Gone](guide/gone.md) and ![Gone too](images/gone.png)
[Outside](../../outside.md)

[ref]: guide/old.md
[ref]: guide/older.md
> [quoted]: quoted.md
";
    let findings = tree(page).check("local-links", "{}").expect("runs");
    assert_eq!(
        lines(&findings),
        [
            "docs/index.md:3 docs/index.md links to missing file: guide/gone.md",
            "docs/index.md:3 docs/index.md links to missing file: images/gone.png",
            "docs/index.md:4 docs/index.md links to missing file: ../../outside.md",
            "docs/index.md:6 docs/index.md links to missing file: guide/old.md",
            "docs/index.md:7 docs/index.md links to missing file: guide/older.md",
            "docs/index.md:8 docs/index.md links to missing file: quoted.md",
        ]
    );
    let first = &findings[0];
    assert_eq!(first.rule, "local-links");
    assert_eq!(first.aspect, "");
    assert_eq!(first.target.as_deref(), Some("docs/index.md:guide/gone.md"));
    assert!(first.help.is_some());
}

#[test]
fn a_link_in_code_in_a_comment_or_in_front_matter_is_not_read() {
    let page = "---\nsee: [x](gone.md)\n---\n`[x](gone.md)`\n\n```md\n[x](gone.md)\n```\n\n<!-- [x](gone.md) -->\n";
    let findings = tree(page).check("local-links", "{}").expect("runs");
    assert_eq!(lines(&findings), Vec::<String>::new());
}

#[test]
fn roots_and_extra_files_choose_the_pages() {
    let fixture = Fixture::new(&[
        ("docs/a.md", "[x](gone.md)\n"),
        ("handbook/b.md", "[x](gone.md)\n"),
        ("handbook/notes.txt", "[x](gone.md)\n"),
        ("deploy/README.md", "[x](gone.md)\n"),
    ]);

    let default = fixture.check("local-links", "{}").expect("runs");
    assert_eq!(
        lines(&default),
        ["docs/a.md:1 docs/a.md links to missing file: gone.md"]
    );

    let chosen = fixture
        .check(
            "local-links",
            r#"{"roots": ["handbook"], "extra-files": ["deploy/README.md", "not/tracked.md"]}"#,
        )
        .expect("runs");
    assert_eq!(
        lines(&chosen),
        [
            "deploy/README.md:1 deploy/README.md links to missing file: gone.md",
            "handbook/b.md:1 handbook/b.md links to missing file: gone.md",
        ]
    );
}

#[test]
fn a_page_that_is_not_utf8_stops_the_run_naming_it() {
    let fixture = Fixture::new(&[]).with_bytes("docs/bad.md", b"[x](a.md)\xff\n");
    let error = fixture.check("local-links", "{}").expect_err("not UTF-8");
    assert!(
        error.message().contains("docs/bad.md is not UTF-8 text"),
        "{error}"
    );
}

#[test]
fn an_unknown_option_is_a_configuration_error() {
    let error = Fixture::new(&[])
        .check("local-links", r#"{"root": ["docs"]}"#)
        .expect_err("unknown key");
    assert!(error.message().contains("unknown field `root`"), "{error}");
}
