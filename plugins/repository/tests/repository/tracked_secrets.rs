//! `repository/tracked-secrets`: by name and by content, each with a passing twin, and never a
//! value in a finding. Every secret-shaped fixture is built at run time, so no line of this file
//! has a shape the rule reports.

use gate_sdk::Finding;

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

/// Each finding's target and key, sorted: what an exception and a baseline match.
fn identities(findings: &[Finding]) -> Vec<String> {
    let mut identities: Vec<String> = findings
        .iter()
        .map(|finding| {
            format!(
                "{} {}",
                finding.target.as_deref().unwrap_or(""),
                finding.key.as_deref().unwrap_or("")
            )
        })
        .collect();
    identities.sort();
    identities
}

/// `count` synthetic base64 lines of 64 characters, different for each `seed`.
fn base64_lines(count: usize, seed: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            (0..64)
                .map(|offset| char::from(BASE64[(seed + index * 7 + offset * 13) % 64]))
                .collect()
        })
        .collect()
}

/// A synthetic armored block of `label` (as in `RSA PRIVATE KEY`) around `body`.
fn armored(label: &str, body: &[String], newline: &str) -> String {
    let mut text = format!("-----BEGIN {label}-----{newline}");
    for line in body {
        text.push_str(line);
        text.push_str(newline);
    }
    format!("{text}-----END {label}-----")
}

/// A synthetic key block: `body` lines of 64 base64 characters between the armor lines, or no
/// `END` line when `end` is false.
fn key_block(body: usize, end: bool) -> (String, Vec<String>) {
    let lines = base64_lines(body, 0);
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
    assert_eq!(found, ["private-key src/fixture.txt:2 private-key"]);
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
    let fine_grained = format!("github_pat_{}_{}", "C3".repeat(11), "d".repeat(59));
    let text = format!("a = \"{aws}\"\nb = {github}\nc: {npm}\nd {slack}\ne={fine_grained}\n");
    let short = format!(
        "AKIA{} ghp_{} github_pat_{}_{}\n",
        "Q".repeat(15),
        "a".repeat(20),
        "C".repeat(22),
        "d".repeat(40)
    );
    let (found, output) = check(&[
        ("config/tokens.txt", text.as_str()),
        ("short.txt", short.as_str()),
    ]);
    assert_eq!(
        found,
        [
            "token config/tokens.txt:1 aws-access-key-id",
            "token config/tokens.txt:2 github-token",
            "token config/tokens.txt:3 npm-token",
            "token config/tokens.txt:4 slack-token",
            "token config/tokens.txt:5 github-token#2"
        ]
    );
    for token in [&aws, &github, &npm, &slack, &fine_grained] {
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
    assert_eq!(found, ["npmrc .npmrc:1 npmrc-credential"]);
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

#[test]
fn a_key_inside_a_line_is_reported_as_in_a_json_string_or_a_string_literal() {
    let body = base64_lines(20, 3);
    let json = format!(
        "{{\n  \"type\": \"service\",\n  \"private_key\": \"{}\\n\"\n}}\n",
        armored("PRIVATE KEY", &body, "\\n")
    );
    let literal = format!(
        "fn fixture() -> &'static str {{\n    \"{}\"\n}}\n",
        armored("EC PRIVATE KEY", &body, "\n")
    );
    let pgp = format!(
        "intro\n{}\n",
        armored("PGP PRIVATE KEY BLOCK", &body, "\n").replacen("-----\n", "-----\n\n", 1)
    );
    let (found, output) = check(&[
        ("config/service.json", json.as_str()),
        ("src/fixture.rs", literal.as_str()),
        ("keys/signing.asc", pgp.as_str()),
    ]);
    assert_eq!(
        found,
        [
            "private-key config/service.json:3 private-key",
            "private-key keys/signing.asc:2 private-key",
            "private-key src/fixture.rs:2 private-key"
        ]
    );
    for line in &body {
        assert!(!output.contains(line.as_str()), "the key is in the output");
    }

    let label = "PRIVATE KEY";
    let mention =
        format!("The header -----BEGIN {label}----- and -----END {label}----- frame a key.\n");
    let public = format!("{}\n", armored("PGP PUBLIC KEY BLOCK", &body, "\n"));
    let (found, _) = check(&[
        ("docs/keys.md", mention.as_str()),
        ("keys/public.asc", public.as_str()),
    ]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_findings_target_and_key_name_the_path_and_the_kind_and_survive_edits_above_it() {
    let first = armored("RSA PRIVATE KEY", &base64_lines(20, 1), "\n");
    let second = armored("OPENSSH PRIVATE KEY", &base64_lines(20, 2), "\n");
    let token = format!("npm_{}", "e5".repeat(18));
    let text = format!("{first}\n{token}\n{second}\n");
    let edited = format!("added\nlines\nabove\n{text}");

    let before = Fixture::new(&[("test/keys.txt", text.as_bytes())])
        .check(RULE, "{}")
        .expect("runs");
    let after = Fixture::new(&[("test/keys.txt", edited.as_bytes())])
        .check(RULE, "{}")
        .expect("runs");
    assert_eq!(
        identities(&before),
        [
            "test/keys.txt:npm-token npm-token",
            "test/keys.txt:private-key private-key",
            "test/keys.txt:private-key#2 private-key#2"
        ]
    );
    assert_eq!(identities(&after), identities(&before));
    assert_eq!(
        rows(&after),
        [
            "private-key test/keys.txt:27 private-key#2",
            "private-key test/keys.txt:4 private-key",
            "token test/keys.txt:26 npm-token"
        ]
    );
}

#[test]
fn an_example_environment_file_named_by_the_option_passes_and_the_default_is_then_reported() {
    let files: [(&str, &[u8]); 3] = [
        (".env.example", b"A=\n"),
        ("apps/a/.env.sample", b"A=\n"),
        (".env.local", b"A=1\n"),
    ];
    let findings = Fixture::new(&files)
        .check(RULE, r#"{"env-examples": [".env.sample"]}"#)
        .expect("runs");
    assert_eq!(
        rows(&findings),
        [
            "env-file .env.example:0 env-file",
            "env-file .env.local:0 env-file"
        ]
    );
}
