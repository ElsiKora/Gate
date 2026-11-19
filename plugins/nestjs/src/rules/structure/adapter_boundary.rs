//! `nestjs/adapter-boundary`: only infrastructure loads external adapter packages, and inward
//! owners load no infrastructure.

use crate::shared::paths::{self, Form};
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::{Allocator, TypeScript};

use crate::layout::{PluginOptions, folder_prefix};
use crate::shared::modules::{self, source_target};
use crate::shared::project;
use crate::shared::report::Findings;

/// The rule.
#[derive(Debug, Default)]
pub struct AdapterBoundary;

/// The options of `nestjs/adapter-boundary`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// External adapter packages, matched as the specifier or a subpath of it.
    pub external_packages: Vec<String>,
    /// Specifier prefixes of external adapter packages.
    pub external_package_prefixes: Vec<String>,
    /// The path prefix of the infrastructure folder; by default `layout.infrastructure-folder`
    /// and a `/`.
    pub infrastructure_prefix: Option<String>,
    /// Path prefixes of the inward owners, which load no infrastructure.
    pub inward_owner_prefixes: Vec<String>,
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if let Some(prefix) = &self.infrastructure_prefix {
            paths::check("infrastructure-prefix", prefix, Form::Prefix)?;
        }
        paths::check_all(
            "inward-owner-prefixes",
            &self.inward_owner_prefixes,
            Form::Prefix,
        )
    }
}

impl Rule for AdapterBoundary {
    type Options = Options;

    const NAME: &'static str = "adapter-boundary";
    const TITLE: &'static str =
        "Only infrastructure loads external adapters, and inward owners load no infrastructure";
    const ASPECTS: &'static [&'static str] = &["external-package", "inward-infrastructure"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        run.options().check_paths()?;
        let options = run.options();
        let prefix = options
            .infrastructure_prefix
            .clone()
            .unwrap_or_else(|| folder_prefix(&layout.infrastructure_folder));
        // The infrastructure folder itself, and the prefix of the paths below it.
        let folder = prefix.trim_end_matches('/').to_owned();
        let below = folder_prefix(&folder);
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut findings = Findings::default();

        for (path, loads) in modules::loads(&typescript, &layout.source_root, true)? {
            let inward = options
                .inward_owner_prefixes
                .iter()
                .any(|owner| path.starts_with(owner.as_str()));

            for specifier in &loads.all {
                let external = options.external_packages.iter().any(|package| {
                    specifier == package || specifier.starts_with(&format!("{package}/"))
                }) || options
                    .external_package_prefixes
                    .iter()
                    .any(|external| specifier.starts_with(external.as_str()));

                if external && !path.starts_with(prefix.as_str()) {
                    findings.push(
                        "external-package",
                        path,
                        None,
                        format!(
                            "{path} imports external adapter package {specifier} outside infrastructure"
                        ),
                    );
                }

                // A relative path, a path spelled from the project root or an alias, resolved to
                // the folder itself or a path below it.
                let loads_infrastructure = source_target(&layout, path, specifier)
                    .is_some_and(|target| target == folder || target.starts_with(&below));

                if inward && loads_infrastructure {
                    findings.push(
                        "inward-infrastructure",
                        path,
                        None,
                        format!(
                            "{path} imports infrastructure adapter {specifier} into an inward owner"
                        ),
                    );
                }
            }
        }

        findings.report(run, |aspect| {
            if aspect == "external-package" {
                "wrap the package in an infrastructure adapter and load the adapter"
            } else {
                "depend on a contract the inward owner declares, and bind the adapter at the edge"
            }
        });
        Ok(())
    }
}
