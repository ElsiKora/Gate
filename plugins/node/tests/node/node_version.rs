use crate::fixture::{Fixture, message};

const RULE: &str = "node-version";

fn root(range: &str) -> String {
    format!("{{\n  \"name\": \"r\",\n  \"engines\": {{ \"node\": \"{range}\" }}\n}}\n")
}

fn member(range: &str) -> String {
    format!("{{\n  \"engines\": {{\n    \"node\": \"{range}\"\n  }}\n}}\n")
}

fn run(nvmrc: &str, root_range: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let root = root(root_range);
    let member = member(">=24.11.0  <25");
    let mut files = vec![
        (".nvmrc", nvmrc),
        ("package.json", root.as_str()),
        ("apps/a/package.json", member.as_str()),
    ];
    files.extend_from_slice(extra);
    Fixture::new("monorepo-root", &files).lines(RULE)
}

#[test]
fn one_exact_version_that_every_range_admits_passes() {
    assert_eq!(run("24.11.0\n", ">=24.11.0 <25", &[]), Vec::<String>::new());
}

#[test]
fn the_version_file_holds_one_exact_version() {
    assert_eq!(run("24\n", ">=24.11.0 <25", &[]), ["file .nvmrc .nvmrc"]);
    assert_eq!(run("lts/*\n", ">=24.11.0 <25", &[]), ["file .nvmrc .nvmrc"]);
    let missing = Fixture::new(
        "service",
        &[("package.json", "{\"engines\": {\"node\": \">=24\"}}")],
    )
    .lines(RULE);
    assert_eq!(missing, ["file .nvmrc .nvmrc"]);
    assert_eq!(
        run("24\n", "latest", &[]),
        [
            "file .nvmrc .nvmrc",
            "engines package.json:3 engines.node",
            "differs apps/a/package.json:3 engines.node"
        ]
    );
}

#[test]
fn the_root_range_admits_the_version_and_no_earlier_major() {
    // The member keeps `>=24.11.0 <25`, so a changed root range also differs from it.
    for range in [">=22", ">=25", "latest"] {
        assert_eq!(
            run("24.11.0\n", range, &[]),
            [
                "engines package.json:3 engines.node",
                "differs apps/a/package.json:3 engines.node"
            ],
            "{range}"
        );
    }
    let below = Fixture::new(
        "service",
        &[(".nvmrc", "24.11.0"), ("package.json", &root(">=22"))],
    );
    assert_eq!(
        message(&below, RULE),
        "package.json engines.node \">=22\" admits a Node major below 24, the major in .nvmrc"
    );
    let none =
        Fixture::new("service", &[(".nvmrc", "24.11.0"), ("package.json", "{}")]).lines(RULE);
    assert_eq!(none, ["engines package.json engines.node"]);
}

#[test]
fn another_manifest_with_another_range_differs() {
    let other = member(">=24");
    assert_eq!(
        run(
            "24.11.0\n",
            ">=24.11.0 <25",
            &[("apps/b/package.json", &other)]
        ),
        ["differs apps/b/package.json:3 engines.node"]
    );
}

#[test]
fn a_second_place_for_the_version_is_reported() {
    let extra = [
        (".node-version", "24.11.0\n"),
        (".tool-versions", "nodejs 24.11.0\npython 3.12.0\n"),
        ("apps/a/.nvmrc", "24.11.0\n"),
        ("tools/other/.tool-versions", "python 3.12.0\n"),
        (
            "apps/c/package.json",
            "{\n  \"volta\": {\n    \"node\": \"24.11.0\"\n  }\n}\n",
        ),
    ];
    assert_eq!(
        run("24.11.0\n", ">=24.11.0 <25", &extra),
        [
            "form .node-version .node-version",
            "form .tool-versions .tool-versions",
            "form apps/a/.nvmrc .nvmrc",
            "form apps/c/package.json:3 volta.node"
        ]
    );
}
