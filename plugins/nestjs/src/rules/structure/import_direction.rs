//! `nestjs/import-direction`: the files below a source directory load nothing that its dependency
//! contract forbids.

use crate::shared::paths::{self, Form};
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::{Allocator, TypeScript};

use crate::layout::{Layout, PluginOptions, folder_prefix};
use crate::shared::modules::{self, names_file, source_target, stem};
use crate::shared::project;
use crate::shared::report::Findings;

/// The rule.
#[derive(Debug, Default)]
pub struct ImportDirection;

/// The aspect a dependency contract reports under.
#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum ContractAspect {
    /// The domain plane loads no other plane.
    DomainDirection,
    /// Migrations load no feature code.
    MigrationDirection,
    /// A plane loads only the planes below it.
    PlaneDirection,
    /// A plane loads no migration.
    PlaneMigration,
    /// An owner loads only what its contract allows.
    #[default]
    OwnerContract,
    /// Shared code loads no feature owner.
    SharedPurity,
}

impl ContractAspect {
    /// The aspect's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DomainDirection => "domain-direction",
            Self::MigrationDirection => "migration-direction",
            Self::PlaneDirection => "plane-direction",
            Self::PlaneMigration => "plane-migration",
            Self::OwnerContract => "owner-contract",
            Self::SharedPurity => "shared-purity",
        }
    }
}

/// One dependency contract: what the files below `source-directory` may not load.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Contract {
    /// The folder whose files the contract binds, relative to the project root.
    pub source_directory: String,
    /// The aspect its findings carry.
    #[serde(default)]
    pub aspect: ContractAspect,
    /// Source folders and files the files may not load, matched as the path or a path below it.
    pub forbidden_owner_paths: Vec<String>,
    /// Packages the files may not load, matched as the specifier or a subpath of it.
    pub forbidden_packages: Vec<String>,
    /// Specifier prefixes the files may not load.
    #[serde(default)]
    pub forbidden_package_prefixes: Vec<String>,
    /// Source files the files may not load, without or with their script extension.
    #[serde(default)]
    pub forbidden_source_paths: Vec<String>,
    /// Whether only loads of runtime values count, not type-only ones.
    #[serde(default)]
    pub runtime_values_only: bool,
    /// Whether declaration files are bound too.
    #[serde(default = "include_declarations")]
    pub include_declaration_files: bool,
    /// Files the contract does not bind.
    #[serde(default)]
    pub allowed_source_paths: Vec<String>,
    /// Folders whose files the contract does not bind.
    #[serde(default)]
    pub allowed_source_directories: Vec<String>,
    /// Loads one file may make despite the contract.
    #[serde(default)]
    pub allowed_dependencies: Vec<AllowedDependency>,
}

/// Declaration files are bound unless a contract says otherwise.
const fn include_declarations() -> bool {
    true
}

/// A file that may load the listed source files despite its contract.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct AllowedDependency {
    /// The file.
    pub source_path: String,
    /// The source files it may load.
    pub target_paths: Vec<String>,
}

/// The options of `nestjs/import-direction`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The dependency contracts; at least one.
    #[schemars(length(min = 1))]
    pub contracts: Vec<Contract>,
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if self.contracts.is_empty() {
            return Err(gate_sdk::Error::new(
                "option `contracts` lists nothing; list at least one contract, or leave the rule off".to_owned(),
            ));
        }
        for (index, contract) in self.contracts.iter().enumerate() {
            let option = |key: &str| format!("contracts[{index}].{key}");
            paths::check(
                &option("source-directory"),
                &contract.source_directory,
                Form::Folder,
            )?;
            paths::check_all(
                &option("forbidden-owner-paths"),
                &contract.forbidden_owner_paths,
                Form::Folder,
            )?;
            paths::check_all(
                &option("forbidden-source-paths"),
                &contract.forbidden_source_paths,
                Form::File,
            )?;
            paths::check_all(
                &option("allowed-source-paths"),
                &contract.allowed_source_paths,
                Form::File,
            )?;
            paths::check_all(
                &option("allowed-source-directories"),
                &contract.allowed_source_directories,
                Form::Folder,
            )?;
            for (dependency, allowed) in contract.allowed_dependencies.iter().enumerate() {
                let option = option(&format!("allowed-dependencies[{dependency}]"));
                paths::check(
                    &format!("{option}.source-path"),
                    &allowed.source_path,
                    Form::File,
                )?;
                paths::check_all(
                    &format!("{option}.target-paths"),
                    &allowed.target_paths,
                    Form::File,
                )?;
            }
        }
        Ok(())
    }
}

impl Rule for ImportDirection {
    type Options = Options;

    const NAME: &'static str = "import-direction";
    const TITLE: &'static str =
        "Source directories load only what their dependency contracts allow";
    const ASPECTS: &'static [&'static str] = &[
        "domain-direction",
        "migration-direction",
        "owner-contract",
        "plane-direction",
        "plane-migration",
        "shared-purity",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        run.options().check_paths()?;
        let contracts = run.options().contracts.clone();
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut findings = Findings::default();

        for contract in &contracts {
            let directory = &contract.source_directory;

            for (path, loads) in
                modules::loads(&typescript, directory, contract.include_declaration_files)?
            {
                if contract
                    .allowed_source_paths
                    .iter()
                    .any(|allowed| allowed == path)
                    || contract
                        .allowed_source_directories
                        .iter()
                        .any(|allowed| path == allowed || path.starts_with(&folder_prefix(allowed)))
                {
                    continue;
                }

                let specifiers = if contract.runtime_values_only {
                    &loads.runtime
                } else {
                    &loads.all
                };

                for specifier in specifiers {
                    if forbids(&layout, contract, path, specifier) {
                        findings.push(
                            contract.aspect.name(),
                            path,
                            None,
                            format!(
                                "{path} loads {specifier} outside its {directory} dependency contract"
                            ),
                        );
                    }
                }
            }
        }

        findings.report(
            run,
            |_| "load the module through an owner the contract allows, or move the code",
        );
        Ok(())
    }
}

/// Whether a contract forbids `path` to load `specifier`: a forbidden owner or source file it names
/// through a relative path, a source-root path or an alias, or a forbidden package. An allowed
/// dependency wins.
fn forbids(layout: &Layout, contract: &Contract, path: &str, specifier: &str) -> bool {
    let target = source_target(layout, path, specifier);
    let target = target.as_deref().map(stem);

    if let Some(target) = target
        && contract.allowed_dependencies.iter().any(|allowed| {
            allowed.source_path == path
                && allowed
                    .target_paths
                    .iter()
                    .any(|file| names_file(target, file))
        })
    {
        return false;
    }

    target.is_some_and(|target| {
        contract
            .forbidden_owner_paths
            .iter()
            .any(|owner| target == owner || target.starts_with(&format!("{owner}/")))
            || contract
                .forbidden_source_paths
                .iter()
                .any(|file| names_file(target, file))
    }) || contract
        .forbidden_packages
        .iter()
        .any(|package| specifier == package || specifier.starts_with(&format!("{package}/")))
        || contract
            .forbidden_package_prefixes
            .iter()
            .any(|prefix| specifier.starts_with(prefix.as_str()))
}
