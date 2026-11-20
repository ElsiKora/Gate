//! The `[plugins.nestjs] options` block: where a `NestJS` service keeps its sources, which folders
//! group which kinds of file, and which packages and decorators the rules recognise.
//!
//! Every rule of the plugin reads the same block, so a service that departs from the conventional
//! layout says so once. Every key is optional; the defaults describe the conventional layout.
//! Folders are relative to the project root and written in their normalized form, without `./`,
//! `.` or `..` segments, doubled or trailing `/`; `""` is the project root. Another form stops the
//! run with an error that shows the normalized form.
//!
//! ```toml
//! [plugins.nestjs]
//! source = "…"
//! options = { layout = { source-root = "src", modules-root = "src/modules" } }
//! ```

use std::collections::BTreeMap;

use gate_sdk::Run;
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;

use crate::shared::paths::{self, Form};

/// The `[plugins.nestjs] options` table.
#[derive(Clone, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct PluginOptions {
    /// The layout of the service.
    pub layout: Layout,
}

impl PluginOptions {
    /// The options of the run's `[plugins.nestjs]` entry.
    ///
    /// # Errors
    /// When the table has a key the plugin does not know or a value of the wrong type, or a layout
    /// path is not in its normalized form; the error names the key.
    pub fn read<O>(run: &Run<'_, O, crate::Nest>) -> gate_sdk::Result<Self> {
        let options = run.plugin_options::<Self>()?;
        options.layout.check_paths()?;
        Ok(options)
    }
}

/// Where the service keeps its sources and what its folders mean. Paths are relative to the
/// project root, without a trailing slash. `modules-root`, `infrastructure-folder` and
/// `path-aliases` derive from `source-root` when they are not set.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", from = "LayoutInput")]
#[schemars(crate = "gate_sdk::schemars", from = "LayoutInput")]
pub struct Layout {
    /// The application source root.
    pub source_root: String,
    /// The test root.
    pub test_root: String,
    /// The folder of repository scripts.
    pub scripts_root: String,
    /// The folder that holds one folder per module.
    pub modules_root: String,
    /// The infrastructure owner below the source root.
    pub infrastructure_folder: String,
    /// The top-level owners below the source root, each reached through its alias.
    pub source_owners: Vec<String>,
    /// Import aliases: an alias matches a specifier that equals it or starts with it and a `/`,
    /// and the matched part is replaced by the folder.
    pub path_aliases: BTreeMap<String, String>,
    /// The planes a module divides into.
    pub module_planes: Vec<String>,
    /// The plane that holds a module's domain.
    pub domain_folder: String,
    /// Folders that group one kind of file.
    pub kind_folders: Vec<String>,
    /// Folders that hold only their own kind of file and no other kind folder.
    pub exclusive_kind_folders: Vec<String>,
    /// Folders that hold one role of class (guards, strategies, subscribers).
    pub role_folders: Vec<String>,
    /// Folders that hold declarations rather than a module's own classes.
    pub artifact_folders: Vec<String>,
    /// Folders below the test root whose files are test support rather than scenarios.
    pub test_support_folders: Vec<String>,
    /// File kinds (`*.<kind>.ts`) below the test root that are test support.
    pub test_support_kinds: Vec<String>,
    /// File name suffixes of test scenarios.
    pub test_scenario_suffixes: Vec<String>,
    /// The CRUD automator package.
    pub automator_package: String,
    /// The `NestJS` common package.
    pub nest_common_package: String,
    /// The `NestJS` core package.
    pub nest_core_package: String,
    /// The `OpenAPI` (Swagger) package of `NestJS`.
    pub swagger_package: String,
    /// The `TypeORM` package.
    pub typeorm_package: String,
    /// The validation package.
    pub validator_package: String,
    /// The route kinds the automator generates for a controller.
    pub generated_routes: Vec<String>,
    /// The `TypeORM` decorators that declare an entity.
    pub typeorm_entity_decorators: Vec<String>,
    /// The `TypeORM` decorators that declare a column.
    pub typeorm_column_decorators: Vec<String>,
    /// The `TypeORM` decorators that declare a relation.
    pub typeorm_relation_decorators: Vec<String>,
    /// The other `TypeORM` decorators that declare a persisted property.
    pub typeorm_other_persisted_decorators: Vec<String>,
}

/// The layout table as it is written: the keys that derive from `source-root` are optional.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars", rename = "Layout")]
pub struct LayoutInput {
    /// The application source root.
    pub source_root: String,
    /// The test root.
    pub test_root: String,
    /// The folder of repository scripts.
    pub scripts_root: String,
    /// The folder that holds one folder per module; `<source-root>/modules` when not set.
    pub modules_root: Option<String>,
    /// The infrastructure owner below the source root; `<source-root>/infrastructure` when not set.
    pub infrastructure_folder: Option<String>,
    /// The top-level owners below the source root, each reached through its alias.
    pub source_owners: Vec<String>,
    /// Import aliases: an alias matches a specifier that equals it or starts with it and a `/`,
    /// and the matched part is replaced by the folder. When not set, `@<owner>` names
    /// `<source-root>/<owner>` for each of `source-owners`.
    pub path_aliases: Option<BTreeMap<String, String>>,
    /// The planes a module divides into.
    pub module_planes: Vec<String>,
    /// The plane that holds a module's domain.
    pub domain_folder: String,
    /// Folders that group one kind of file.
    pub kind_folders: Vec<String>,
    /// Folders that hold only their own kind of file and no other kind folder.
    pub exclusive_kind_folders: Vec<String>,
    /// Folders that hold one role of class (guards, strategies, subscribers).
    pub role_folders: Vec<String>,
    /// Folders that hold declarations rather than a module's own classes.
    pub artifact_folders: Vec<String>,
    /// Folders below the test root whose files are test support rather than scenarios.
    pub test_support_folders: Vec<String>,
    /// File kinds (`*.<kind>.ts`) below the test root that are test support.
    pub test_support_kinds: Vec<String>,
    /// File name suffixes of test scenarios.
    pub test_scenario_suffixes: Vec<String>,
    /// The CRUD automator package.
    pub automator_package: String,
    /// The `NestJS` common package.
    pub nest_common_package: String,
    /// The `NestJS` core package.
    pub nest_core_package: String,
    /// The `OpenAPI` (Swagger) package of `NestJS`.
    pub swagger_package: String,
    /// The `TypeORM` package.
    pub typeorm_package: String,
    /// The validation package.
    pub validator_package: String,
    /// The route kinds the automator generates for a controller.
    pub generated_routes: Vec<String>,
    /// The `TypeORM` decorators that declare an entity.
    pub typeorm_entity_decorators: Vec<String>,
    /// The `TypeORM` decorators that declare a column.
    pub typeorm_column_decorators: Vec<String>,
    /// The `TypeORM` decorators that declare a relation.
    pub typeorm_relation_decorators: Vec<String>,
    /// The other `TypeORM` decorators that declare a persisted property.
    pub typeorm_other_persisted_decorators: Vec<String>,
}

/// The default artifact folders.
const ARTIFACT_FOLDERS: [&str; 11] = [
    "constant",
    "dto",
    "enum",
    "function",
    "guard",
    "interface",
    "middleware",
    "strategy",
    "subscriber",
    "type",
    "utility",
];

/// The default exclusive kind folders.
const EXCLUSIVE_KIND_FOLDERS: [&str; 8] = [
    "constant",
    "decorator",
    "dto",
    "enum",
    "function",
    "interface",
    "type",
    "utility",
];

/// The default `TypeORM` column decorators.
const TYPEORM_COLUMN_DECORATORS: [&str; 10] = [
    "Column",
    "CreateDateColumn",
    "DeleteDateColumn",
    "ObjectIdColumn",
    "PrimaryColumn",
    "PrimaryGeneratedColumn",
    "TreeLevelColumn",
    "UpdateDateColumn",
    "VersionColumn",
    "ViewColumn",
];

/// Owned strings of a list of literals.
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

impl Default for LayoutInput {
    fn default() -> Self {
        let source_owners = strings(&[
            "common",
            "config",
            "infrastructure",
            "migration",
            "modules",
            "runtime",
            "shared",
        ]);

        Self {
            source_root: "src".to_owned(),
            test_root: "test".to_owned(),
            scripts_root: "scripts".to_owned(),
            modules_root: None,
            infrastructure_folder: None,
            source_owners,
            path_aliases: None,
            module_planes: strings(&["domain", "integration", "internal", "public"]),
            domain_folder: "domain".to_owned(),
            kind_folders: strings(&[
                "constant",
                "dto",
                "function",
                "interface",
                "type",
                "utility",
            ]),
            exclusive_kind_folders: strings(&EXCLUSIVE_KIND_FOLDERS),
            role_folders: strings(&["guard", "strategy", "subscriber"]),
            artifact_folders: strings(&ARTIFACT_FOLDERS),
            test_support_folders: strings(&["enum", "fixture", "fixtures", "interface", "type"]),
            test_support_kinds: strings(&["enum", "fixture", "interface", "type"]),
            test_scenario_suffixes: strings(&[".test.ts", ".case.ts"]),
            automator_package: "@elsikora/nestjs-crud-automator".to_owned(),
            nest_common_package: "@nestjs/common".to_owned(),
            nest_core_package: "@nestjs/core".to_owned(),
            swagger_package: "@nestjs/swagger".to_owned(),
            typeorm_package: "typeorm".to_owned(),
            validator_package: "class-validator".to_owned(),
            generated_routes: strings(&[
                "CREATE",
                "DELETE",
                "GET",
                "GET_LIST",
                "PARTIAL_UPDATE",
                "UPDATE",
            ]),
            typeorm_entity_decorators: strings(&["Entity", "ChildEntity"]),
            typeorm_column_decorators: strings(&TYPEORM_COLUMN_DECORATORS),
            typeorm_relation_decorators: strings(&[
                "ManyToMany",
                "ManyToOne",
                "OneToMany",
                "OneToOne",
            ]),
            typeorm_other_persisted_decorators: strings(&[
                "TreeChildren",
                "TreeParent",
                "VirtualColumn",
            ]),
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        LayoutInput::default().into()
    }
}

impl From<LayoutInput> for Layout {
    /// The layout with the keys that were not set derived from `source-root`.
    fn from(input: LayoutInput) -> Self {
        let prefix = folder_prefix(&input.source_root);
        let path_aliases = input.path_aliases.unwrap_or_else(|| {
            input
                .source_owners
                .iter()
                .map(|owner| (format!("@{owner}"), format!("{prefix}{owner}")))
                .collect()
        });

        Self {
            modules_root: input
                .modules_root
                .unwrap_or_else(|| format!("{prefix}modules")),
            infrastructure_folder: input
                .infrastructure_folder
                .unwrap_or_else(|| format!("{prefix}infrastructure")),
            path_aliases,
            source_root: input.source_root,
            test_root: input.test_root,
            scripts_root: input.scripts_root,
            source_owners: input.source_owners,
            module_planes: input.module_planes,
            domain_folder: input.domain_folder,
            kind_folders: input.kind_folders,
            exclusive_kind_folders: input.exclusive_kind_folders,
            role_folders: input.role_folders,
            artifact_folders: input.artifact_folders,
            test_support_folders: input.test_support_folders,
            test_support_kinds: input.test_support_kinds,
            test_scenario_suffixes: input.test_scenario_suffixes,
            automator_package: input.automator_package,
            nest_common_package: input.nest_common_package,
            nest_core_package: input.nest_core_package,
            swagger_package: input.swagger_package,
            typeorm_package: input.typeorm_package,
            validator_package: input.validator_package,
            generated_routes: input.generated_routes,
            typeorm_entity_decorators: input.typeorm_entity_decorators,
            typeorm_column_decorators: input.typeorm_column_decorators,
            typeorm_relation_decorators: input.typeorm_relation_decorators,
            typeorm_other_persisted_decorators: input.typeorm_other_persisted_decorators,
        }
    }
}

/// The prefix of the paths below `folder`: empty for the project root (`""`), `<folder>/`
/// otherwise, with any trailing slash of `folder` trimmed first.
#[must_use]
pub fn folder_prefix(folder: &str) -> String {
    let folder = folder.trim_end_matches('/');

    if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    }
}

impl Layout {
    /// Checks that every folder of the layout is a normalized path relative to the project root.
    ///
    /// # Errors
    /// At the first folder that is not, naming its key and the normalized form.
    pub fn check_paths(&self) -> gate_sdk::Result<()> {
        for (key, folder) in [
            ("source-root", &self.source_root),
            ("test-root", &self.test_root),
            ("scripts-root", &self.scripts_root),
            ("modules-root", &self.modules_root),
            ("infrastructure-folder", &self.infrastructure_folder),
        ] {
            paths::check(&format!("layout.{key}"), folder, Form::Folder)?;
        }

        paths::check("layout.domain-folder", &self.domain_folder, Form::Name)?;

        for (key, names) in [
            ("source-owners", &self.source_owners),
            ("module-planes", &self.module_planes),
            ("kind-folders", &self.kind_folders),
            ("exclusive-kind-folders", &self.exclusive_kind_folders),
            ("role-folders", &self.role_folders),
            ("artifact-folders", &self.artifact_folders),
            ("test-support-folders", &self.test_support_folders),
            ("test-support-kinds", &self.test_support_kinds),
        ] {
            paths::check_all(&format!("layout.{key}"), names, Form::Name)?;
        }

        for (alias, folder) in &self.path_aliases {
            paths::check(
                &format!("layout.path-aliases.{alias}"),
                folder,
                Form::Folder,
            )?;
        }

        Ok(())
    }

    /// The prefix of the paths below the source root: empty when the source root is the project
    /// root, `<source-root>/` otherwise.
    #[must_use]
    pub fn source_prefix(&self) -> String {
        folder_prefix(&self.source_root)
    }

    /// The project path an aliased `specifier` names, or `None` when no alias matches. An alias
    /// matches a specifier that equals it or starts with it and a `/`; when two aliases match, the
    /// longer one wins.
    #[must_use]
    pub fn resolve_alias(&self, specifier: &str) -> Option<String> {
        self.path_aliases
            .iter()
            .filter(|(alias, _)| {
                specifier == alias.as_str()
                    || specifier
                        .strip_prefix(alias.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .max_by_key(|(alias, _)| alias.len())
            .map(|(alias, folder)| {
                let rest = specifier[alias.len()..].trim_start_matches('/');

                if rest.is_empty() {
                    folder.clone()
                } else {
                    format!("{}{rest}", folder_prefix(folder))
                }
            })
    }

    /// Whether `owner` is one of the top-level source owners.
    #[must_use]
    pub fn is_source_owner(&self, owner: &str) -> bool {
        self.source_owners.iter().any(|known| known == owner)
    }

    /// The module planes without the domain plane.
    pub fn planes_without_domain(&self) -> impl Iterator<Item = &str> {
        self.module_planes
            .iter()
            .map(String::as_str)
            .filter(|plane| *plane != self.domain_folder)
    }
}
