//! `[plugins.docker] options`: what both rules call a secret and which registries they accept, and
//! the image grammar they share.

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
    /// A variable whose name contains one of these, ignoring case, holds a secret.
    pub secret_names: Vec<String>,
    /// The registries an image may come from (`docker.io`, `mirror.gcr.io`); empty for any.
    pub registries: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            secret_names: [
                "PASSWORD",
                "PASSWD",
                "SECRET",
                "TOKEN",
                "PRIVATE_KEY",
                "API_KEY",
                "ACCESS_KEY",
                "CREDENTIAL",
                "AUTH",
            ]
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
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
                "{error}; the docker rules read secret-names and registries from [plugins.docker] options"
            ))
        })
    }

    /// Whether a variable named `name` holds a secret.
    #[must_use]
    pub fn secret(&self, name: &str) -> bool {
        let name = name.to_ascii_uppercase();
        self.secret_names
            .iter()
            .any(|part| name.contains(&part.to_ascii_uppercase()))
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
