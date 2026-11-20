//! `repository/tracked-secrets`: no tracked file is an environment file, a key or credential file,
//! or holds a private key, a token of a published shape or a literal `.npmrc` credential. A
//! finding names the path, the line and the kind; never the value. Its key is the kind and a short
//! hash of the matched secret, never the line or an ordinal, so an edit elsewhere keeps it and no
//! other secret can take it.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Location, Report, Rule, Run, Scope};
use regex::Regex;
use sha2::{Digest, Sha256};

/// The token shapes their issuers publish, each a capture group in the order of [`TOKEN_KINDS`].
static TOKENS: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:((?:AKIA|ASIA)[0-9A-Z]{16})|(gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9]{22}_[A-Za-z0-9]{59})|(npm_[A-Za-z0-9]{36})|(xox[abprs]-[A-Za-z0-9-]{10,}))\b",
    )
    .ok()
});

/// The armor line that opens a private key block, anywhere in a line: `-----BEGIN `, words, then
/// `PRIVATE KEY-----` or, for a PGP key, `PRIVATE KEY BLOCK-----`.
static KEY_HEADER: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----").ok());

/// A kind of secret found in a file's text: its name in targets and keys, and its description.
#[derive(Clone, Copy)]
struct Kind {
    slug: &'static str,
    description: &'static str,
}

/// What each capture group of [`TOKENS`] is.
const TOKEN_KINDS: [Kind; 4] = [
    Kind {
        slug: "aws-access-key-id",
        description: "an AWS access key id",
    },
    Kind {
        slug: "github-token",
        description: "a GitHub token",
    },
    Kind {
        slug: "npm-token",
        description: "an npm token",
    },
    Kind {
        slug: "slack-token",
        description: "a Slack token",
    },
];

/// A private key block.
const PRIVATE_KEY: Kind = Kind {
    slug: "private-key",
    description: "a private key",
};

/// A literal `.npmrc` credential.
const NPMRC_CREDENTIAL: Kind = Kind {
    slug: "npmrc-credential",
    description: "a literal registry credential",
};

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

/// How many base64 characters in a row a private key block holds at least, line breaks aside; a real
/// key holds far more.
const KEY_BODY: usize = 64;

/// How many hex digits of the sha256 of a matched secret its key holds: 48 bits, enough to tell the
/// secrets of one file apart, far too few to reveal one.
const KEY_HASH: usize = 12;

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
        let (Some(tokens), Some(header)) = (TOKENS.as_ref(), KEY_HEADER.as_ref()) else {
            return Err(Error::new(
                "a pattern of repository/tracked-secrets does not compile; report this as a bug",
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
            private_keys(&mut found, header, &text);
            for (index, line) in text.lines().enumerate() {
                for captures in tokens.captures_iter(line) {
                    let kind = (1..=TOKEN_KINDS.len())
                        .find(|group| captures.get(*group).is_some())
                        .map_or(TOKEN_KINDS[0], |group| TOKEN_KINDS[group - 1]);
                    found.report("token", index + 1, kind, &captures[0]);
                }
                if name == ".npmrc"
                    && let Some(value) = npmrc_credential(line)
                {
                    found.report("npmrc", index + 1, NPMRC_CREDENTIAL, value);
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

/// The findings of one file, each keyed by its kind and a short hash of the matched secret:
/// `<kind>:<12 hex>`. A secret found twice in one file is reported once.
struct Found<'r> {
    report: &'r mut Report,
    path: &'r str,
    keys: BTreeSet<String>,
}

impl<'r> Found<'r> {
    fn new(report: &'r mut Report, path: &'r str) -> Self {
        Self {
            report,
            path,
            keys: BTreeSet::new(),
        }
    }

    fn report(&mut self, aspect: &str, line: usize, kind: Kind, secret: &str) {
        let key = format!("{}:{}", kind.slug, short_hash(secret));
        if !self.keys.insert(key.clone()) {
            return;
        }
        let path = self.path;
        let description = kind.description;
        self.report
            .finding(
                aspect,
                Location::line(path, line),
                format!("{path}:{line} holds {description}"),
            )
            .target(format!("{path}:{key}"))
            .help("remove it, revoke it with its issuer, and read it from a secret store; a test fixture that holds a fake on purpose takes an exception for this target")
            .key(key);
    }
}

/// The first [`KEY_HASH`] hex digits of the sha256 of `secret`.
fn short_hash(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut hex = String::with_capacity(KEY_HASH);
    for byte in digest.iter().take(KEY_HASH / 2) {
        for nibble in [byte >> 4, byte & 0x0f] {
            hex.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
        }
    }
    hex
}

/// The private key blocks of `text`: a [`KEY_HEADER`] anywhere in a line, then, before the next
/// header, `-----END `, on the following lines or, as in a JSON string with `\\n` escapes, on the
/// same one. Between them, at least [`KEY_BODY`] base64 characters follow one another once line
/// breaks, the indentation around them and `\\n` or `\\r` escapes are taken out; code that names
/// the header and the footer, or prose, holds no such run. A block is reported at its header's
/// line. Each footer is searched for once, so the scan is linear in the text.
fn private_keys(found: &mut Found<'_>, header: &Regex, text: &str) {
    let headers: Vec<(usize, usize)> = header
        .find_iter(text)
        .map(|matched| (matched.start(), matched.end()))
        .collect();
    let mut footer: Option<usize> = None;
    let mut counted = (0, 1);
    for (index, &(start, end)) in headers.iter().enumerate() {
        if footer.is_none_or(|at| at < end) {
            footer = text[end..].find("-----END ").map(|at| end + at);
        }
        let Some(at) = footer else {
            break;
        };
        if headers.get(index + 1).is_some_and(|(next, _)| *next < at) {
            continue;
        }
        let body = joined(&text[end..at]);
        if longest_base64_run(&body) >= KEY_BODY {
            counted.1 += text[counted.0..start].matches('\n').count();
            counted.0 = start;
            found.report("private-key", counted.1, PRIVATE_KEY, &body);
        }
    }
}

/// `body` with its `\\n` and `\\r` escapes taken as line breaks, and its lines trimmed and joined.
fn joined(body: &str) -> String {
    body.replace("\\r", "\n")
        .replace("\\n", "\n")
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .concat()
}

/// The length of the longest run of base64 characters in `text`.
fn longest_base64_run(text: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=') {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// The literal value an `.npmrc` line sets a credential to, if it sets one.
fn npmrc_credential(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.starts_with('#') || line.starts_with(';') {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    (NPMRC_KEYS
        .iter()
        .any(|name| key == *name || key.ends_with(&format!(":{name}")))
        && !value.is_empty()
        && !value.starts_with("${"))
    .then_some(value)
}
