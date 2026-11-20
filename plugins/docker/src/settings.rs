//! `[plugins.docker] options`: which files are image files, what both rules call a secret and which
//! registries they accept, and the image grammar they share.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Error, Result};

/// The `[plugins.docker] options`, every key optional.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Settings {
    /// The file names of image files, compared case-sensitively; a `*` stands for a name that
    /// holds no dot (`Dockerfile.*` takes `Dockerfile.api`, not `Dockerfile.test.ts`).
    pub image_files: Vec<String>,
    /// The secret names: a variable holds a secret when its `_`-separated words, ignoring case,
    /// hold one of these as whole words (`DB_PASSWORD`, `AWS_SECRET_ACCESS_KEY`; not `AUTHOR`).
    pub secret_names: Vec<String>,
    /// The last words that make a name about a secret rather than the secret itself
    /// (`POSTGRES_PASSWORD_FILE`, `TOKEN_TTL`), ignoring case.
    pub secret_metadata_words: Vec<String>,
    /// The registries an image may come from (`docker.io`, `mirror.gcr.io`); empty for any.
    pub registries: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let strings = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        Self {
            image_files: strings(&[
                "Dockerfile",
                "Containerfile",
                "Dockerfile.*",
                "*.Dockerfile",
            ]),
            secret_names: strings(&[
                "PASSWORD",
                "PASSWD",
                "SECRET",
                "TOKEN",
                "PRIVATE_KEY",
                "API_KEY",
                "ACCESS_KEY",
                "CREDENTIAL",
                "CREDENTIALS",
                "AUTH",
            ]),
            secret_metadata_words: strings(&[
                "FILE", "PATH", "DIR", "URL", "ENDPOINT", "NAME", "TYPE", "TTL", "EXPIRY", "LENGTH",
            ]),
            registries: Vec::new(),
        }
    }
}

impl Settings {
    /// The settings of `run`.
    ///
    /// # Errors
    /// When they do not match.
    pub fn read<O>(run: &gate_sdk::Run<'_, O>) -> Result<Self> {
        run.plugin_options().map_err(|error| {
            Error::config(format!(
                "{error}; the docker rules read image-files, secret-names, secret-metadata-words and registries from [plugins.docker] options"
            ))
        })
    }

    /// Whether a file name is an image file: one of `image_files`, and never an ignore file.
    #[must_use]
    pub fn image_file(&self, name: &str) -> bool {
        name.rsplit_once('.')
            .is_none_or(|(_, extension)| extension != "dockerignore")
            && self
                .image_files
                .iter()
                .any(|pattern| file_pattern(pattern, name))
    }

    /// Whether a variable named `name` holds a secret: one of `secret_names` is a run of its
    /// `_`-separated words, and its last word is not one of `secret_metadata_words`.
    #[must_use]
    pub fn secret(&self, name: &str) -> bool {
        let upper = name.to_ascii_uppercase();
        let words: Vec<&str> = upper.split('_').collect();
        let about = words.last().is_some_and(|last| {
            self.secret_metadata_words
                .iter()
                .any(|word| word.eq_ignore_ascii_case(last))
        });
        !about
            && self.secret_names.iter().any(|secret| {
                let secret = secret.to_ascii_uppercase();
                let parts: Vec<&str> = secret.split('_').collect();
                !parts.is_empty()
                    && words
                        .windows(parts.len())
                        .any(|run| run == parts.as_slice())
            })
    }

    /// Why `image` is not acceptable: not pinned by digest, or from a registry `registries` does
    /// not list; `None` when it is.
    #[must_use]
    pub fn unpinned(&self, image: &str) -> Option<String> {
        if !pinned(image) {
            return Some(format!("`{image}` is not pinned by its sha256 digest"));
        }
        let registry = registry(image);
        if !self.registries.is_empty() && !self.registries.contains(&registry) {
            return Some(format!(
                "`{image}` comes from {registry}, which `registries` does not list"
            ));
        }
        None
    }
}

/// Whether `name` matches `pattern`, where a `*` stands for a non-empty name without a dot.
fn file_pattern(pattern: &str, name: &str) -> bool {
    let Some((before, after)) = pattern.split_once('*') else {
        return pattern == name;
    };
    name.len() > before.len() + after.len()
        && name.starts_with(before)
        && name.ends_with(after)
        && !name[before.len()..name.len() - after.len()].contains(['.', '/'])
}

/// Whether `image` is `<name>@sha256:<64 lowercase hex>`.
#[must_use]
pub fn pinned(image: &str) -> bool {
    let Some((name, digest)) = image.split_once("@sha256:") else {
        return false;
    };
    !name.is_empty()
        && !name.contains(char::is_whitespace)
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The registry of an image reference: its first path part when that names a host (it holds a
/// `.` or a `:`, or is `localhost`), `docker.io` otherwise.
#[must_use]
pub fn registry(image: &str) -> String {
    match image.split_once('/') {
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            first.to_owned()
        }
        _ => "docker.io".to_owned(),
    }
}
