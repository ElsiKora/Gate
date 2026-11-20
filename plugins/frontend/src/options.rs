//! The `[plugins.frontend] options` table: the pack, which names the project's framework.
//!
//! ```toml
//! [plugins.frontend]
//! source = "ElsiKora/Gate:frontend@0.3.0"
//! options = { pack = "nextjs" }
//! ```

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{CallState, Need, Run};

/// What every rule needs from the plugin's options: the pack.
pub const PACK: Need = Need {
    key: "pack",
    when: None,
    why: "the standard differs by framework where the framework requires it: write pack = \"react-vite\", \"nextjs\" or \"react-library\"",
};

/// The `[plugins.frontend] options` table.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct PluginOptions {
    /// The framework of the project: `react-vite`, `nextjs` or `react-library`. Every rule needs it.
    pub pack: Option<Pack>,
}

/// The framework of a project, which decides the cells of the standard a framework forces.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Pack {
    /// A React app built with Vite and routed by `TanStack` Router file routes.
    ReactVite,
    /// A Next.js app on the App Router.
    Nextjs,
    /// A React library: a shared layer published on its own.
    ReactLibrary,
}

impl Pack {
    /// The pack of the run's `[plugins.frontend]` entry.
    ///
    /// # Errors
    /// When the table has a key the plugin does not know or a value of the wrong type, or no
    /// `pack`; the error says what to write.
    pub fn read<O, S: CallState>(run: &Run<'_, O, S>) -> gate_sdk::Result<Self> {
        run.plugin_options::<PluginOptions>()?.pack.ok_or_else(|| {
            gate_sdk::Error::config(format!(
                "[plugins.frontend] options has no pack; {}",
                PACK.why
            ))
        })
    }

    /// The pack as the options write it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ReactVite => "react-vite",
            Self::Nextjs => "nextjs",
            Self::ReactLibrary => "react-library",
        }
    }

    /// Whether the pack is a library: a shared layer without app layers, routes or environment.
    #[must_use]
    pub fn library(self) -> bool {
        self == Self::ReactLibrary
    }
}
