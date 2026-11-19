//! `nestjs/owner-boundaries`: each source file keeps the boundaries of its architecture owner.
//!
//! Every tracked `.ts` file below `layout.source-root`, declaration files included and no folder
//! skipped, belongs to the first `owner-roots` entry that equals its path or contains it:
//!
//! - `owner-barrel`: an `index.ts` re-exports (`export *`, `export * as`, a non-empty
//!   `export { ... } from`) a module of another owner;
//! - `controller-owner`: a class decorated with one of `controller-decorators` outside the
//!   `api-owners`;
//! - `api-plane-route`: a controller route whose literal start is one of `plane-routes` or
//!   `retired-routes`, or a controller below `internal-owner` whose routes do not all start with
//!   `internal-base`;
//! - `nested-resource`: a `<kebab-name><flat-key-controller-suffix>` file name, a path holding
//!   `/<flattened-resource>`, or a route below `internal-owner` that does;
//! - `domain-module-forwarding`: a `module.ts` outside `domain-owner` whose module decorator exports
//!   a `...Module` of the domain owner;
//! - `entity-manager-exposure`: a public method of a `<domain-owner>/**/<domain-service-file>` with a
//!   parameter whose type holds the native entity manager.
//!
//! Specifiers name owners through `layout.path-aliases` or as relative paths. A file without a
//! syntax tree is skipped: `typescript/parse` reports it.

mod facts;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::oxc_ast::ast::Statement;
use gate_typescript::{Allocator, SourceFile, TypeScript};

use self::facts::{Facts, InternalBase, Reader};
use crate::layout::{Layout, PluginOptions};
use crate::shared::modules::normalize;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/owner-boundaries`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The owner roots; a file belongs to the first that equals its path or contains it.
    pub owner_roots: Vec<String>,
    /// The owner roots that may declare controllers.
    pub api_owners: Vec<String>,
    /// The owner whose modules alone may export its modules, and whose services are checked for
    /// the entity manager.
    pub domain_owner: String,
    /// The folder whose controllers compose their routes from the internal base.
    pub internal_owner: String,
    /// Route segments a controller route may not start with literally.
    pub plane_routes: Vec<String>,
    /// Retired route segments a controller route may not start with literally.
    #[serde(default)]
    pub retired_routes: Vec<String>,
    /// The constant every internal route starts with.
    pub internal_base: Base,
    /// Child resources (`parent/child`) that must stay nested below their owning resource.
    #[serde(default)]
    pub flattened_resources: Vec<String>,
    /// The file name suffix of a flat key controller (`-key.controller.ts`); absent, it is not
    /// checked.
    #[serde(default)]
    pub flat_key_controller_suffix: Option<String>,
    /// The class decorators that declare a controller. Absent means `Controller` of
    /// `layout.nest-common-package` and `ApiController` of `layout.automator-package`.
    #[serde(default)]
    pub controller_decorators: Option<Vec<Import>>,
    /// The class decorator that declares a module; its package also exports `forwardRef`, and a
    /// controller decorator of its package takes a route as its first argument. Absent means
    /// `Module` of `layout.nest-common-package`.
    #[serde(default)]
    pub module_decorator: Option<Import>,
    /// The native entity manager type. Absent means `EntityManager` of `layout.typeorm-package`.
    #[serde(default)]
    pub entity_manager: Option<Import>,
    /// The file name of a domain service.
    #[serde(default = "domain_service_file")]
    pub domain_service_file: String,
}

/// The internal API base: an export path of one of `modules`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Base {
    /// The module specifiers that export the base.
    pub modules: Vec<String>,
    /// The export path of the base (`["INTERNAL_API_CONSTANT", "BASE_PATH"]`).
    pub export: Vec<String>,
}

/// An export of a package.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Import {
    /// The package.
    pub package: String,
    /// The export's name.
    pub name: String,
}

/// An import of `name` from `package`.
fn import(package: &str, name: &str) -> Import {
    Import {
        package: package.to_owned(),
        name: name.to_owned(),
    }
}

impl Decorators {
    /// The configured imports, the layout's packages where an option is absent.
    ///
    /// # Errors
    /// When a configured import names an empty package or name.
    fn new(options: &Options, layout: &Layout) -> gate_sdk::Result<Self> {
        let controllers = options.controller_decorators.clone().unwrap_or_else(|| {
            vec![
                import(&layout.nest_common_package, "Controller"),
                import(&layout.automator_package, "ApiController"),
            ]
        });
        let module = options
            .module_decorator
            .clone()
            .unwrap_or_else(|| import(&layout.nest_common_package, "Module"));
        let entity_manager = options
            .entity_manager
            .clone()
            .unwrap_or_else(|| import(&layout.typeorm_package, "EntityManager"));

        for (option, value) in controllers
            .iter()
            .map(|value| ("controller-decorators", value))
            .chain([
                ("module-decorator", &module),
                ("entity-manager", &entity_manager),
            ])
        {
            if value.package.is_empty() || value.name.is_empty() {
                return Err(gate_sdk::Error::new(format!(
                    "the option `{option}` has an empty package or name; name both, or leave the option out to use the layout's packages"
                )));
            }
        }

        Ok(Self {
            controllers,
            module,
            entity_manager,
        })
    }
}

/// The default domain service file name.
fn domain_service_file() -> String {
    "service.ts".to_owned()
}

/// The decorators and types the facts reader recognises.
#[derive(Debug)]
pub struct Decorators {
    controllers: Vec<Import>,
    module: Import,
    entity_manager: Import,
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "owner-boundaries";
    const TITLE: &'static str = "Source files keep the boundaries of their architecture owner";
    const ASPECTS: &'static [&'static str] = &[
        "api-plane-route",
        "controller-owner",
        "domain-module-forwarding",
        "entity-manager-exposure",
        "nested-resource",
        "owner-barrel",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            use crate::shared::paths::Form::Folder;
            let o = run.options();
            crate::shared::paths::check_all("owner-roots", &o.owner_roots, Folder)?;
            crate::shared::paths::check_all("api-owners", &o.api_owners, Folder)?;
            crate::shared::paths::check("domain-owner", &o.domain_owner, Folder)?;
            crate::shared::paths::check("internal-owner", &o.internal_owner, Folder)?;
        }

        let layout = PluginOptions::read(run)?.layout;
        let options = run.options().clone();
        let decorators = Decorators::new(&options, &layout)?;
        let boundaries = Boundaries {
            options: &options,
            layout: &layout,
            entity_manager: &decorators.entity_manager.name,
        };
        let base = InternalBase {
            modules: &options.internal_base.modules,
            export: &options.internal_base.export,
        };
        let service = format!("/{}", options.domain_service_file);
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut findings = Vec::new();

        for path in tree::typescript_files(&source, &layout.source_root, &tree::UNSKIPPED) {
            let Some(file) = project::parsed(&typescript, path)? else {
                continue;
            };
            let owner = boundaries.owner(path);

            if path.ends_with("/index.ts") {
                for specifier in exported_specifiers(&file) {
                    let target = boundaries.dependency_owner(path, &specifier);

                    if let (Some(owner), Some(target)) = (owner, target)
                        && owner != target
                    {
                        findings.push(Finding::new(
                            "owner-barrel",
                            path,
                            format!("{path} re-exports {target} source across its owner barrel"),
                            "re-export only the barrel's own owner",
                        ));
                    }
                }
            }

            let facts = Reader::new(&file, &base, &decorators)
                .facts(below(path, &options.domain_owner) && path.ends_with(&service));

            boundaries.file(path, owner, &facts, &mut findings);
        }

        for finding in findings {
            run.report()
                .finding(
                    finding.aspect,
                    Location::file(finding.path),
                    finding.message,
                )
                .help(finding.help);
        }

        Ok(())
    }
}

/// One finding before it is reported.
struct Finding {
    aspect: &'static str,
    path: String,
    message: String,
    help: &'static str,
}

impl Finding {
    fn new(aspect: &'static str, path: &str, message: String, help: &'static str) -> Self {
        Self {
            aspect,
            path: path.to_owned(),
            message,
            help,
        }
    }
}

/// The owners, routes and internal API base of the source boundaries.
struct Boundaries<'r> {
    options: &'r Options,
    layout: &'r Layout,
    /// The name of the entity manager type, for the message.
    entity_manager: &'r str,
}

impl Boundaries<'_> {
    /// The owner root a project path lies in.
    fn owner(&self, path: &str) -> Option<&str> {
        self.options
            .owner_roots
            .iter()
            .find(|root| path == root.as_str() || below(path, root))
            .map(String::as_str)
    }

    /// The owner root a specifier of `file` names: a relative path, or an alias read as the path
    /// it maps to.
    fn dependency_owner(&self, file: &str, specifier: &str) -> Option<&str> {
        // An absolute specifier names no source owner.
        if specifier.starts_with('/') {
            return None;
        }

        let target = if specifier.starts_with('.') {
            let folder = file.rsplit_once('/').map_or(".", |(folder, _)| folder);
            normalize(&format!("{folder}/{specifier}"))
        } else {
            normalize(
                &self
                    .layout
                    .resolve_alias(specifier)
                    .unwrap_or_else(|| specifier.to_owned()),
            )
        };

        self.owner(&target)
    }

    /// Whether a route or path contains `/<resource>` of a flattened child resource, followed by
    /// `/` or its end.
    fn flattens(&self, text: &str) -> bool {
        self.options.flattened_resources.iter().any(|resource| {
            let needle = format!("/{resource}");

            text.match_indices(&needle).any(|(start, _)| {
                let rest = &text[start + needle.len()..];
                rest.is_empty() || rest.starts_with('/')
            })
        })
    }

    /// The findings of one file's owner, facts and path.
    fn file(&self, path: &str, owner: Option<&str>, facts: &Facts, findings: &mut Vec<Finding>) {
        let options = self.options;

        for parameter in &facts.managers {
            findings.push(Finding::new(
                "entity-manager-exposure",
                path,
                format!(
                    "{path} exposes native {} in public method {parameter}",
                    self.entity_manager
                ),
                "keep the entity manager out of public method signatures",
            ));
        }

        if path.ends_with("/module.ts") && owner != Some(options.domain_owner.as_str()) {
            for (specifier, name) in &facts.forwarded {
                if self.dependency_owner(path, specifier) == Some(options.domain_owner.as_str()) {
                    findings.push(Finding::new(
                        "domain-module-forwarding",
                        path,
                        format!("{path} forwards domain module {name} across an owner boundary"),
                        "import the domain module where it is used instead of exporting it",
                    ));
                }
            }
        }

        if !facts.controllers.is_empty()
            && !owner.is_some_and(|owner| options.api_owners.iter().any(|api| api == owner))
        {
            findings.push(Finding::new(
                "controller-owner",
                path,
                format!("{path} declares a controller outside an API owner boundary"),
                "move the controller into an API owner",
            ));
        }

        self.routes(path, facts, findings);

        let name = path.rsplit_once('/').map_or(path, |(_, name)| name);

        if let Some(suffix) = &options.flat_key_controller_suffix
            && name.strip_suffix(suffix.as_str()).is_some_and(|stem| {
                !stem.is_empty()
                    && stem
                        .chars()
                        .all(|character| matches!(character, 'a'..='z' | '0'..='9' | '-'))
            })
        {
            findings.push(Finding::new(
                "nested-resource",
                path,
                format!(
                    "{path} uses a flat key controller filename; \
                     nest key controllers under an owning key folder"
                ),
                "nest the key controller under an owning key folder",
            ));
        }

        if self.flattens(path) {
            findings.push(Finding::new(
                "nested-resource",
                path,
                format!("{path} flattens a child resource; nest it under its owning resource"),
                "nest the child resource under its owning resource",
            ));
        }

        if below(path, &options.internal_owner)
            && facts.route_shapes.iter().any(|route| self.flattens(route))
        {
            findings.push(Finding::new(
                "nested-resource",
                path,
                format!(
                    "{path} declares a flat child-resource route; nest it under its owning resource"
                ),
                "nest the child resource route under its owning resource",
            ));
        }
    }

    /// The API plane route findings of one file's controllers.
    fn routes(&self, path: &str, facts: &Facts, findings: &mut Vec<Finding>) {
        let options = self.options;
        let internal = below(path, &options.internal_owner);
        let prefixes = || {
            facts
                .controllers
                .iter()
                .flat_map(|controller| controller.literal_prefixes.iter())
        };

        if prefixes().any(|prefix| starts_with_segment(prefix, &options.plane_routes)) {
            findings.push(Finding::new(
                "api-plane-route",
                path,
                format!(
                    "{path} hardcodes an API plane route prefix instead of using plane constants"
                ),
                "compose the route from the plane constants",
            ));
        }

        if internal
            && facts
                .controllers
                .iter()
                .any(|controller| !controller.uses_internal_base)
        {
            findings.push(Finding::new(
                "api-plane-route",
                path,
                format!(
                    "{path} must compose its route from {}",
                    options.internal_base.export.join(".")
                ),
                "start every route with the internal base",
            ));
        }

        if prefixes().any(|prefix| starts_with_segment(prefix, &options.retired_routes)) {
            findings.push(Finding::new(
                "api-plane-route",
                path,
                format!("{path} contains a stale hardcoded API owner route"),
                "remove the retired route prefix",
            ));
        }
    }
}

/// Whether `path` lies below the folder `owner`.
fn below(path: &str, owner: &str) -> bool {
    path.strip_prefix(owner)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// Whether a route starts with one of `segments`, as a whole segment.
fn starts_with_segment(route: &str, segments: &[String]) -> bool {
    segments
        .iter()
        .any(|segment| route == segment || below(route, segment))
}

/// The module specifiers a barrel re-exports, each once: every `export *`, `export * as` and
/// non-empty `export { ... } from`, a relative specifier without its script extension.
fn exported_specifiers(file: &SourceFile<'_>) -> Vec<String> {
    let mut specifiers: Vec<String> = Vec::new();

    for statement in &file.program().body {
        let source = match statement {
            Statement::ExportAllDeclaration(export) => &export.source,
            Statement::ExportFromDeclaration(export) if !export.specifiers.is_empty() => {
                &export.source
            }
            _ => continue,
        };
        let value = source.value.as_str();
        let specifier = if value.starts_with('.') {
            strip_script_extension(value)
        } else {
            value
        };

        if !specifiers.iter().any(|known| known == specifier) {
            specifiers.push(specifier.to_owned());
        }
    }

    specifiers
}

/// `specifier` without a final `.js`, `.ts`, `.cjs`, `.cts`, `.mjs` or `.mts`.
fn strip_script_extension(specifier: &str) -> &str {
    for extension in [".js", ".ts", ".cjs", ".cts", ".mjs", ".mts"] {
        if let Some(stem) = specifier.strip_suffix(extension) {
            return stem;
        }
    }

    specifier
}
