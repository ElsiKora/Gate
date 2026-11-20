//! The plugin's options, `[plugins.cdk] options`: the extensions a project may declare, each with
//! its job. Everything else is the standard ([`crate::standard`]).

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{CallState, Error, Run};

use crate::standard::{self, LIB_DIRECTORIES, NAG_PACKS, RESERVED, TAGS};

/// The options every rule of the plugin reads.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct PluginOptions {
    /// Directories below `lib/` beyond the standard's, each with its job: kebab case, singular,
    /// neither a standard nor a reserved name.
    pub extra: Vec<Extension>,
    /// Tag keys beyond the standard's, each with its job: Pascal case, not a standard key.
    pub extra_tags: Vec<Extension>,
    /// The region literals allowed in the code.
    pub regions: Vec<String>,
    /// The cdk-nag packs `bin/application.ts` registers.
    pub nag_packs: Vec<String>,
}

/// One declared extension: its name and the job it does.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Extension {
    /// The directory or tag key.
    pub name: String,
    /// One sentence: what the entry holds.
    pub job: String,
}

impl Default for PluginOptions {
    fn default() -> Self {
        Self {
            extra: Vec::new(),
            extra_tags: Vec::new(),
            regions: vec!["us-east-1".to_owned()],
            nag_packs: vec!["AwsSolutionsChecks".to_owned()],
        }
    }
}

impl PluginOptions {
    /// The plugin's options of `run`, checked.
    ///
    /// # Errors
    /// A configuration error that names the key when an extension breaks the standard, a region
    /// is not one, or a pack is not a cdk-nag pack.
    pub fn read<O, S: CallState>(run: &Run<'_, O, S>) -> gate_sdk::Result<Self> {
        let options = run.plugin_options::<Self>()?;
        options.check()?;
        Ok(options)
    }

    /// Checks the options against the standard.
    ///
    /// # Errors
    /// A configuration error naming the key and the value.
    pub fn check(&self) -> gate_sdk::Result<()> {
        for extension in &self.extra {
            let name = extension.name.as_str();
            if !standard::is_kebab(name) {
                return Err(config("extra", name, "is not kebab case"));
            }
            if LIB_DIRECTORIES
                .iter()
                .any(|(standard, _, _)| *standard == name)
            {
                return Err(config(
                    "extra",
                    name,
                    "is a directory of the standard already",
                ));
            }
            if let Some((_, goes)) = RESERVED.iter().find(|(reserved, _)| *reserved == name) {
                return Err(config(
                    "extra",
                    name,
                    &format!("is reserved: its content goes to {goes}"),
                ));
            }
            empty_job("extra", extension)?;
        }

        for extension in &self.extra_tags {
            let name = extension.name.as_str();
            if !standard::is_pascal(name) {
                return Err(config("extra-tags", name, "is not Pascal case"));
            }
            if TAGS.contains(&name) {
                return Err(config(
                    "extra-tags",
                    name,
                    "is a tag of the standard already",
                ));
            }
            empty_job("extra-tags", extension)?;
        }

        for region in &self.regions {
            if !standard::is_region(region) {
                return Err(config("regions", region, "is not an AWS region"));
            }
        }

        if self.nag_packs.is_empty() {
            return Err(Error::config(
                "[plugins.cdk] options: nag-packs is empty; name at least one cdk-nag pack, such \
                 as AwsSolutionsChecks",
            ));
        }
        for pack in &self.nag_packs {
            if !NAG_PACKS.contains(&pack.as_str()) {
                return Err(config(
                    "nag-packs",
                    pack,
                    &format!("is not a cdk-nag pack; use one of {}", NAG_PACKS.join(", ")),
                ));
            }
        }

        Ok(())
    }

    /// Whether `name` is a declared directory below `lib/`.
    #[must_use]
    pub fn declares(&self, name: &str) -> bool {
        self.extra.iter().any(|extension| extension.name == name)
    }

    /// Whether `key` is a tag key of the standard or a declared one.
    #[must_use]
    pub fn allows_tag(&self, key: &str) -> bool {
        TAGS.contains(&key)
            || self
                .extra_tags
                .iter()
                .any(|extension| extension.name == key)
    }
}

fn empty_job(key: &str, extension: &Extension) -> gate_sdk::Result<()> {
    if extension.job.trim().is_empty() {
        return Err(config(
            key,
            &extension.name,
            "has no job; say in one sentence what it holds",
        ));
    }
    Ok(())
}

fn config(key: &str, value: &str, problem: &str) -> Error {
    Error::config(format!(
        "[plugins.cdk] options: {key} names `{value}`, which {problem}"
    ))
}
