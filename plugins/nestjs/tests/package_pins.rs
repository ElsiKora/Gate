//! `nestjs/package-pins` against synthetic manifests and lockfiles.

mod support;

use support::{check, error, row};

const RULE: &str = "package-pins";
const OPTIONS: &str = r#"{ "artifacts": [{
    "package": "@scope/errors",
    "version": "1.2.3",
    "resolved": "https://registry.example.test/@scope/errors/-/errors-1.2.3.tgz",
    "integrity": "sha512-abc"
}] }"#;
const MANIFEST: &str = r#"{ "dependencies": { "@scope/errors": "1.2.3" } }"#;
const LOCKFILE: &str = r#"{ "packages": {
    "": { "dependencies": { "@scope/errors": "1.2.3" } },
    "node_modules/@scope/errors": {
        "version": "1.2.3",
        "resolved": "https://registry.example.test/@scope/errors/-/errors-1.2.3.tgz",
        "integrity": "sha512-abc"
    }
} }"#;

fn files<'a>(manifest: &'a str, lockfile: &'a str) -> Vec<(&'static str, &'a str)> {
    vec![("package.json", manifest), ("package-lock.json", lockfile)]
}

#[test]
fn an_exact_pin_passes() {
    assert_eq!(check(RULE, OPTIONS, &files(MANIFEST, LOCKFILE)), []);
}

#[test]
fn a_range_in_the_manifest_or_the_lock_root_is_reported() {
    let manifest = MANIFEST.replace("\"1.2.3\"", "\"^1.2.3\"");
    let lockfile = LOCKFILE.replacen(
        "\"@scope/errors\": \"1.2.3\"",
        "\"@scope/errors\": \"~1.2.3\"",
        1,
    );

    assert_eq!(
        check(RULE, OPTIONS, &files(&manifest, &lockfile)),
        [
            row(
                "",
                "package.json",
                None,
                "@scope/errors must be exact-pinned to 1.2.3 in package.json"
            ),
            row(
                "",
                "package-lock.json",
                None,
                "@scope/errors must be exact-pinned to 1.2.3 in package-lock.json"
            ),
        ]
    );
}

#[test]
fn a_second_resolution_or_an_alias_is_reported() {
    let nested = LOCKFILE.replace(
        "\"node_modules/@scope/errors\": {",
        "\"node_modules/other/node_modules/@scope/errors\": { \"version\": \"1.0.0\" },\n\
         \"node_modules/@scope/errors\": {",
    );
    let resolution = row(
        "",
        "package-lock.json",
        None,
        "@scope/errors must have exactly one root npm registry resolution",
    );
    assert_eq!(
        check(RULE, OPTIONS, &files(MANIFEST, &nested)),
        std::slice::from_ref(&resolution)
    );

    let aliased = MANIFEST.replace(
        "\"1.2.3\" }",
        "\"1.2.3\", \"errors-copy\": \"npm:@scope/errors@1.2.3\" }",
    );
    assert_eq!(
        check(RULE, OPTIONS, &files(&aliased, LOCKFILE)),
        [resolution]
    );
}

#[test]
fn a_linked_or_different_artifact_is_reported() {
    let message = row(
        "",
        "package-lock.json",
        None,
        "@scope/errors lock artifact must be the immutable 1.2.3 npm registry tarball with exact \
         integrity and no link/alias",
    );
    let linked = LOCKFILE.replace(
        "\"integrity\": \"sha512-abc\"",
        "\"integrity\": \"sha512-abc\", \"link\": true",
    );
    assert_eq!(
        check(RULE, OPTIONS, &files(MANIFEST, &linked)),
        std::slice::from_ref(&message)
    );

    let other = LOCKFILE.replace("sha512-abc", "sha512-xyz");
    assert_eq!(check(RULE, OPTIONS, &files(MANIFEST, &other)), [message]);
}

#[test]
fn a_missing_file_is_reported_even_without_artifacts() {
    let message =
        "package.json and package-lock.json are required for exact safe-error package pins";

    assert_eq!(
        check(RULE, "{}", &[("package.json", MANIFEST)]),
        [row("", "package-lock.json", None, message)]
    );
    assert_eq!(
        check(RULE, "{}", &[("package-lock.json", LOCKFILE)]),
        [row("", "package.json", None, message)]
    );
}

#[test]
fn the_file_options_move_the_manifest_and_lockfile() {
    let options = OPTIONS.replace(
        "\"artifacts\"",
        "\"manifest\": \"app/package.json\", \"lockfile\": \"app/package-lock.json\", \"artifacts\"",
    );
    let manifest = MANIFEST.replace("\"1.2.3\"", "\"^1.2.3\"");

    assert_eq!(
        check(
            RULE,
            &options,
            &[
                ("app/package.json", &manifest),
                ("app/package-lock.json", LOCKFILE)
            ]
        ),
        [row(
            "",
            "app/package.json",
            None,
            "@scope/errors must be exact-pinned to 1.2.3 in app/package.json"
        )]
    );
}

#[test]
fn invalid_json_and_unknown_options_are_errors() {
    let message = error(RULE, OPTIONS, &files("{", LOCKFILE));
    assert!(
        message.contains("package.json is not valid JSON"),
        "{message}"
    );

    let message = error(RULE, r#"{ "artifact": [] }"#, &files(MANIFEST, LOCKFILE));
    assert!(message.contains("artifact"), "{message}");
}

#[test]
fn a_path_option_that_is_not_normalized_is_an_error_naming_the_rule() {
    let message = error(
        RULE,
        r#"{ "lockfile": "./package-lock.json" }"#,
        &files(MANIFEST, LOCKFILE),
    );

    assert!(message.contains(RULE), "{message}");
    assert!(
        message.contains(r#"option `lockfile` is "./package-lock.json""#)
            && message.contains(r#"write "package-lock.json""#),
        "{message}"
    );
}
