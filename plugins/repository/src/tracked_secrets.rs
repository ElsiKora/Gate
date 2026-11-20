//! `repository/tracked-secrets`: no tracked file is an environment file, a key or credential file,
//! or holds a private key, a token of a published shape or a literal `.npmrc` credential. A
//! finding names the path, the line and the kind; never the value.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Report, Rule, Run, Scope};
use regex::Regex;

/// The token shapes their issuers publish, each a capture group in the order of [`TOKEN_KINDS`].
static TOKENS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:((?:AKIA|ASIA)[0-9A-Z]{16})|(gh[pousr]_[A-Za-z0-9]{36,})|(npm_[A-Za-z0-9]{36})|(xox[abprs]-[A-Za-z0-9-]{10,}))\b",
    )
    .ok()
});

/// What each capture group of [`TOKENS`] is.
const TOKEN_KINDS: [&str; 4] = [
    "an AWS access key id",
    "a GitHub token",
    "an npm token",
    "a Slack token",
];

/// The extensions of key and keystore files.
const KEY_EXTENSIONS: [&str; 7] = ["key", "p12", "pfx", "jks", "keystore", "kdbx", "ppk"];

/// The file names of private keys and credential stores.
const KEY_NAMES: [&str; 6] = [
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    ".netrc",
    ".git-credentials",
];

/// The `.npmrc` keys that hold a credential.
const NPMRC_KEYS: [&str; 3] = ["_authToken", "_auth", "_password"];

/// How many base64 characters a private key block holds at least; a real key holds far more.
const KEY_BODY: usize = 64;

/// How far into a file a NUL byte marks it as binary, as git decides.
const BINARY_PROBE: usize = 8000;

/// The rule.
#[derive(Debug, Default)]
pub struct TrackedSecrets;

/// The options of `repository/tracked-secrets`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The file names of example environment files, which hold placeholders and are tracked.
    pub env_examples: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            env_examples: vec![".env.example".to_owned()],
        }
    }
}

impl Rule for TrackedSecrets {
    type Options = Options;

    const NAME: &'static str = "tracked-secrets";
    const TITLE: &'static str = "No secret is tracked";
    const ASPECTS: &'static [&'static str] =
        &["env-file", "key-file", "private-key", "token", "npmrc"];
    const SCOPE: Scope = Scope::Repository;

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let Some(tokens) = TOKENS.as_ref() else {
            return Err(Error::new(
                "the token pattern of repository/tracked-secrets does not compile; report this as a bug",
            ));
        };
        let examples = run.options().env_examples.clone();
        let tree = run.tree();
        let report = run.report();

        for path in tree.paths() {
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            if env_file(&name) && !examples.contains(&name) {
                by_name(
                    report,
                    &path,
                    "env-file",
                    "an environment file",
                    "track `.env.example` with placeholders, and keep the real file out with .gitignore",
                );
            }
            if key_file(&path, &name) {
                by_name(
                    report,
                    &path,
                    "key-file",
                    "a key or credential file",
                    "keep it in a secret store, and out of the repository with .gitignore",
                );
            }
            let Some(bytes) = tree.bytes(&path) else {
                continue;
            };
            if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let mut found = Found::new(report, &path);
            private_keys(&mut found, &text);
            for (index, line) in text.lines().enumerate() {
                for captures in tokens.captures_iter(line) {
                    let kind = (1..=TOKEN_KINDS.len())
                        .find(|group| captures.get(*group).is_some())
                        .map_or(TOKEN_KINDS[0], |group| TOKEN_KINDS[group - 1]);
                    found.report("token", index + 1, kind);
                }
                if name == ".npmrc" && npmrc_credential(line) {
                    found.report("npmrc", index + 1, "a literal registry credential");
                }
            }
        }

        Ok(())
    }
}

/// Whether a file name is an environment file: `.env`, `.env.<anything>` or `<anything>.env`.
fn env_file(name: &str) -> bool {
    name == ".env"
        || name.starts_with(".env.")
        || name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension == "env")
}

/// Whether a path is a key or credential file by its name.
fn key_file(path: &str, name: &str) -> bool {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    extension.is_some_and(|extension| KEY_EXTENSIONS.contains(&extension))
        || KEY_NAMES.contains(&name)
        || path == ".aws/credentials"
        || path.ends_with("/.aws/credentials")
}

fn by_name(report: &mut Report, path: &str, aspect: &str, kind: &str, help: &str) {
    report
        .finding(
            aspect,
            Location::file(path.to_owned()),
            format!("{path} is {kind}"),
        )
        .target(path.to_owned())
        .help(help.to_owned())
        .key(aspect.to_owned());
}

/// The findings of one file, each keyed by its kind and its ordinal among that kind's.
struct Found<'r> {
    report: &'r mut Report,
    path: &'r str,
    counts: BTreeMap<String, usize>,
}

impl<'r> Found<'r> {
    fn new(report: &'r mut Report, path: &'r str) -> Self {
        Self {
            report,
            path,
            counts: BTreeMap::new(),
        }
    }

    fn report(&mut self, aspect: &str, line: usize, kind: &str) {
        let count = self.counts.entry(kind.to_owned()).or_insert(0);
        *count += 1;
        let path = self.path;
        self.report
            .finding(
                aspect,
                Location::line(path, line),
                format!("{path}:{line} holds {kind}"),
            )
            .target(format!("{path}:{line}"))
            .help("remove it, revoke it with its issuer, and read it from a secret store; a test fixture that holds a fake on purpose takes an exception for this target")
            .key(format!("{kind}#{count}"));
    }
}

/// The private key blocks of `text`: a `BEGIN ... PRIVATE KEY` line, then at least [`KEY_BODY`]
/// base64 characters, then an `END` line.
fn private_keys(found: &mut Found<'_>, text: &str) {
    let mut open: Option<(usize, usize)> = None;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.starts_with("-----BEGIN ") && line.ends_with("PRIVATE KEY-----") {
            open = Some((index + 1, 0));
        } else if line.starts_with("-----END ") {
            if let Some((start, body)) = open.take()
                && body >= KEY_BODY
            {
                found.report("private-key", start, "a private key");
            }
        } else if let Some((_, body)) = open.as_mut() {
            *body += line
                .bytes()
                .filter(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
                .count();
        }
    }
}

/// Whether an `.npmrc` line sets a credential to a literal value.
fn npmrc_credential(line: &str) -> bool {
    let line = line.trim();
    if line.starts_with('#') || line.starts_with(';') {
        return false;
    }
    let Some((key, value)) = line.split_once('=') else {
        return false;
    };
    let key = key.trim();
    let value = value.trim();
    NPMRC_KEYS
        .iter()
        .any(|name| key == *name || key.ends_with(&format!(":{name}")))
        && !value.is_empty()
        && !value.starts_with("${")
}
