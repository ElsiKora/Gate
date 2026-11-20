//! `repository/tracked-secrets`: by name and by content, each with a passing twin, and never a
//! value in a finding. Every secret-shaped fixture is built at run time, so no line of this file
//! has a shape the rule reports.

use crate::{Fixture, rows, texts};

const RULE: &str = "tracked-secrets";

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn check(files: &[(&str, &str)]) -> (Vec<String>, String) {
    let bytes: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes()))
        .collect();
    let findings = Fixture::new(&bytes).check(RULE, "{}").expect("runs");
    (rows(&findings), texts(&findings))
}

/// A synthetic key block: `body` lines of 64 base64 characters between the armor lines, or no
/// `END` line when `end` is false.
fn key_block(body: usize, end: bool) -> (String, Vec<String>) {
    let lines: Vec<String> = (0..body)
        .map(|index| {
            (0..64)
                .map(|offset| char::from(BASE64[(index * 7 + offset * 13) % 64]))
                .collect()
        })
        .collect();
    let kind = "PRIVATE";
    let mut text = format!("-----BEGIN {kind} KEY-----\n");
    for line in &lines {
        text.push_str(line);
        text.push('\n');
    }
    if end {
        text = format!("{text}-----END {kind} KEY-----\n");
    }
    (text, lines)
}

#[test]
fn environment_files_are_reported_wherever_they_are_and_the_example_passes() {
    let (rows, _) = check(&[
        (".env", "A=1\n"),
        ("apps/a/.env.local", "A=1\n"),
        ("prod.env", "A=1\n"),
        (".env.example", "A=\n"),
        ("apps/a/.env.example", "A=\n"),
    ]);
    assert_eq!(
        rows,
        [
            "env-file .env:0 env-file",
            "env-file apps/a/.env.local:0 env-file",
            "env-file prod.env:0 env-file"
        ]
    );
}

#[test]
fn key_and_credential_files_are_reported_by_name_and_a_public_key_passes() {
    let (rows, _) = check(&[
        ("deploy.key", "x\n"),
        ("ops/id_ed25519", "x\n"),
        ("ops/id_ed25519.pub", "x\n"),
        (".aws/credentials", "x\n"),
        ("docs/credentials.md", "x\n"),
    ]);
    assert_eq!(
        rows,
        [
            "key-file .aws/credentials:0 key-file",
            "key-file deploy.key:0 key-file",
            "key-file ops/id_ed25519:0 key-file"
        ]
    );
}

#[test]
fn a_private_key_block_is_reported_at_its_begin_line_without_its_text() {
    let (key, lines) = key_block(25, true);
    let text = format!("notes\n{key}");
    let (found, output) = check(&[("src/fixture.txt", text.as_str())]);
    assert_eq!(found, ["private-key src/fixture.txt:2 a private key#1"]);
    for line in &lines {
        assert!(!output.contains(line.as_str()), "the key is in the output");
    }

    let short = key_block(0, true)
        .0
        .replace("-----\n-----END", "-----\nVEVTVA==\n-----END");
    let (open, _) = key_block(25, false);
    let (found, _) = check(&[("short.txt", short.as_str()), ("open.txt", open.as_str())]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn each_published_token_shape_is_reported_and_shorter_strings_pass() {
    let aws = format!("AKIA{}", "Q".repeat(16));
    let github = format!("ghp_{}", "a1".repeat(18));
    let npm = format!("npm_{}", "b2".repeat(18));
    let slack = format!("xoxb-{}", "1234-5678".repeat(2));
    let text = format!("a = \"{aws}\"\nb = {github}\nc: {npm}\nd {slack}\n");
    let short = format!("AKIA{} ghp_{}\n", "Q".repeat(15), "a".repeat(20));
    let (found, output) = check(&[
        ("config/tokens.txt", text.as_str()),
        ("short.txt", short.as_str()),
    ]);
    assert_eq!(
        found,
        [
            "token config/tokens.txt:1 an AWS access key id#1",
            "token config/tokens.txt:2 a GitHub token#1",
            "token config/tokens.txt:3 an npm token#1",
            "token config/tokens.txt:4 a Slack token#1"
        ]
    );
    for token in [&aws, &github, &npm, &slack] {
        assert!(!output.contains(token.as_str()), "a token is in the output");
    }
}

#[test]
fn a_literal_npmrc_credential_is_reported_and_an_interpolated_one_passes() {
    let literal = format!(
        "//registry.example.invalid/:_authToken={}\n",
        "c3".repeat(10)
    );
    let (found, output) = check(&[
        (".npmrc", literal.as_str()),
        (
            "apps/a/.npmrc",
            "//registry.example.invalid/:_authToken=${NPM_TOKEN}\nsave-exact=true\n",
        ),
    ]);
    assert_eq!(found, ["npmrc .npmrc:1 a literal registry credential#1"]);
    assert!(!output.contains(&"c3".repeat(10)), "{output}");
}

#[test]
fn a_binary_file_is_not_read_as_text() {
    let mut bytes = vec![0u8, 1, 2];
    bytes.extend_from_slice(format!("AKIA{}", "Q".repeat(16)).as_bytes());
    let findings = Fixture::new(&[("image.png", bytes.as_slice())])
        .check(RULE, "{}")
        .expect("runs");
    assert_eq!(rows(&findings), Vec::<String>::new());
}
