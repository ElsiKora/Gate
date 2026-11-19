//! `nestjs/custom-routes`: custom HTTP handlers use the automator's custom route decorator, never a
//! raw route decorator of the `NestJS` common package.
//!
//! In a tracked `.ts` file below `layout.source-root` (not `.d.ts`, outside `[languages.typescript] skip-folders`)
//! nothing names a route decorator of the package or of a subpath of it: no named import or
//! `export ... from` of one, no `export * from` the package, and no read of one from a namespace,
//! default or `import = require` binding of the package, as a static or string-keyed member or in an
//! object destructuring. A file without a syntax tree is skipped: `typescript/parse` reports it.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Expression, ImportDeclarationSpecifier, Statement, TSModuleReference,
};
use gate_typescript::{Allocator, SourceFile, TypeScript};

use crate::layout::PluginOptions;
use crate::shared::{project, syntax, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/custom-routes`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The package whose route decorators are raw; its subpaths count too. Absent means
    /// `layout.nest-common-package`.
    pub package: Option<String>,
    /// The route decorators the package exports.
    pub route_decorators: Vec<String>,
    /// The decorator the message recommends instead.
    pub replacement: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            package: None,
            route_decorators: [
                "All",
                "Copy",
                "Delete",
                "Get",
                "Head",
                "Lock",
                "Mkcol",
                "Move",
                "Options",
                "Patch",
                "Post",
                "Propfind",
                "Proppatch",
                "Put",
                "RequestMapping",
                "Search",
                "Sse",
                "Unlock",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            replacement: "@ApiRouteCustom".to_owned(),
        }
    }
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "custom-routes";
    const TITLE: &'static str =
        "Custom HTTP handlers use ApiRouteCustom, not raw Nest route decorators";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();
        let matcher = Matcher {
            package: super::package::resolve(
                "package",
                options.package.as_deref(),
                &layout.nest_common_package,
            )?,
            routes: options.route_decorators.clone(),
        };
        let replacement = options.replacement.clone();
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let paths = syntax::typescript_sources(tree::files(
            &source,
            &layout.source_root,
            typescript.settings(),
        ));

        for path in paths {
            let Some(file) = project::parsed(&typescript, path)? else {
                continue;
            };
            let mut findings = Vec::new();
            let bindings = matcher.statements(&file, &mut findings);

            if !bindings.is_empty() {
                matcher.members(&file, &bindings, &mut findings);
            }

            for (offset, name) in findings {
                let line = usize::try_from(file.line(offset)).unwrap_or(usize::MAX);

                run.report()
                    .finding(
                        "",
                        Location::line(path, line),
                        format!(
                            "{path} uses the raw Nest {name} route decorator; declare custom handlers with {replacement}"
                        ),
                    )
                    .help(format!("declare the handler with {replacement}"));
            }
        }

        Ok(())
    }
}

/// The package and its route decorators.
struct Matcher {
    package: String,
    routes: Vec<String>,
}

impl Matcher {
    /// Whether a module specifier is the package or a subpath of it.
    fn package(&self, specifier: &str) -> bool {
        specifier == self.package
            || specifier
                .strip_prefix(self.package.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    }

    /// Whether `name` is a route decorator.
    fn route(&self, name: &str) -> bool {
        self.routes.iter().any(|route| route == name)
    }

    /// Reads the top-level imports and re-exports of the package: named route decorators are
    /// findings, and the local names of namespace, default and `import = require` bindings are
    /// returned.
    fn statements<'a>(
        &self,
        file: &'a SourceFile<'_>,
        findings: &mut Vec<(u32, String)>,
    ) -> Vec<&'a str> {
        let mut bindings = Vec::new();

        for statement in &file.program().body {
            match statement {
                Statement::ImportDeclaration(import) if self.package(&import.source.value) => {
                    for specifier in import.specifiers.iter().flatten() {
                        match specifier {
                            ImportDeclarationSpecifier::ImportSpecifier(named) => {
                                let name = named.imported.name();

                                if self.route(name.as_str()) {
                                    findings.push((named.span.start, format!("@{name}")));
                                }
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                                bindings.push(namespace.local.name.as_str());
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                                bindings.push(default.local.name.as_str());
                            }
                        }
                    }
                }
                Statement::ExportFromDeclaration(export) if self.package(&export.source.value) => {
                    for specifier in &export.specifiers {
                        let name = specifier.local.name();

                        if self.route(name.as_str()) {
                            findings.push((specifier.span.start, format!("@{name}")));
                        }
                    }
                }
                Statement::ExportAllDeclaration(export) if self.package(&export.source.value) => {
                    findings.push((export.span.start, "re-exported".to_owned()));
                }
                Statement::TSImportEqualsDeclaration(import) => {
                    if let TSModuleReference::ExternalModuleReference(reference) =
                        &import.module_reference
                        && self.package(&reference.expression.value)
                    {
                        bindings.push(import.id.name.as_str());
                    }
                }
                _ => {}
            }
        }

        bindings
    }

    /// Reads route decorators from the package bindings: `nest.Get`, `nest["Get"]` and
    /// `const { Get } = nest`.
    fn members(&self, file: &SourceFile<'_>, bindings: &[&str], findings: &mut Vec<(u32, String)>) {
        let bound = |expression: &Expression<'_>| {
            expression
                .get_identifier_reference()
                .is_some_and(|object| bindings.contains(&object.name.as_str()))
        };

        for node in file.semantic().nodes().iter() {
            match node.kind() {
                AstKind::StaticMemberExpression(member)
                    if bound(&member.object) && self.route(member.property.name.as_str()) =>
                {
                    findings.push((member.span.start, format!("@{}", member.property.name)));
                }
                AstKind::ComputedMemberExpression(member) if bound(&member.object) => {
                    if let Expression::StringLiteral(key) = &member.expression
                        && self.route(key.value.as_str())
                    {
                        findings.push((member.span.start, format!("@{}", key.value)));
                    }
                }
                AstKind::VariableDeclarator(declarator)
                    if declarator.init.as_ref().is_some_and(bound) =>
                {
                    if let BindingPattern::ObjectPattern(object) = &declarator.id {
                        for property in &object.properties {
                            if let Some(name) = property.key.static_name()
                                && self.route(name.as_ref())
                            {
                                findings.push((property.span.start, format!("@{name}")));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
