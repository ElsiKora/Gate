//! `nestjs/module-registry`: every concrete native Nest module below the source root has one row
//! of the module registry, a Markdown table, whose chapter maps the module in its source map
//! section.
//!
//! A native module is a class, neither abstract nor `declare`, in a tracked `.ts` file (not
//! `.d.ts`) below `layout.source-root` outside `[languages.typescript] skip-folders`, decorated with a call of
//! `module-decorator` of `module-package` (through aliases, namespaces, element access, optional
//! calls, parentheses and assertions). Every finding is located in the registry. A file without a
//! syntax tree is skipped: `typescript/parse` reports it. Every registry row whose owner lies below
//! the source root also links one Markdown chapter below `documentation-root` (`owner-chapter`).

mod chapters;
mod registry;

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::ast::{CallExpression, ChainElement, Class, Expression};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::value::unwrap;
use gate_typescript::{Allocator, SourceFile, TypeScript};

use crate::layout::PluginOptions;
use crate::shared::{project, tree};

use self::chapters::Chapters;
use self::registry::Registry;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/module-registry`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The registry page.
    pub registry: String,
    /// The folder the registry's chapter links resolve from; a chapter outside it is not read
    /// (`registry-chapter`, `registry-coverage`).
    pub chapter_root: String,
    /// The folder the one chapter link of a row must land in (`owner-chapter`).
    pub documentation_root: String,
    /// The index of the chapters, which is no chapter of a module.
    pub chapter_index: String,
    /// The header cells of the registry table: module path, chapter and ownership.
    #[schemars(length(min = 3, max = 3))]
    pub table_header: Vec<String>,
    /// The heading line of a chapter's source map section.
    pub source_map_heading: String,
    /// The package of the module decorator. Absent means `layout.nest-common-package`.
    pub module_package: Option<String>,
    /// The module decorator the package exports.
    pub module_decorator: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            registry: "docs/modules/module-registry.md".to_owned(),
            chapter_root: "docs/modules".to_owned(),
            documentation_root: "docs".to_owned(),
            chapter_index: "docs/modules/README.md".to_owned(),
            table_header: vec![
                "Module Path".to_owned(),
                "Chapter".to_owned(),
                "Ownership".to_owned(),
            ],
            source_map_heading: "## Source Map".to_owned(),
            module_package: None,
            module_decorator: "Module".to_owned(),
        }
    }
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "module-registry";
    const TITLE: &'static str =
        "Every native Nest module has one registry row whose chapter maps it";
    const ASPECTS: &'static [&'static str] = &[
        "owner-chapter",
        "registry-chapter",
        "registry-coverage",
        "registry-shape",
    ];
    const LANGUAGES: &'static [&'static str] = &[];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            use crate::shared::paths::Form::{File, Folder};
            let o = run.options();
            crate::shared::paths::check("registry", &o.registry, File)?;
            crate::shared::paths::check("chapter-root", &o.chapter_root, Folder)?;
            crate::shared::paths::check("documentation-root", &o.documentation_root, Folder)?;
            crate::shared::paths::check("chapter-index", &o.chapter_index, File)?;
        }

        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();

        if options.table_header.len() != 3 {
            return Err(gate_sdk::Error::config(format!(
                "the option table-header has {} cells; give exactly three: the module path, chapter and ownership headers",
                options.table_header.len()
            )));
        }

        let package = super::package::resolve(
            "module-package",
            options.module_package.as_deref(),
            &layout.nest_common_package,
        )?;
        let decorator = vec![options.module_decorator.clone()];
        let header = options.table_header.clone();
        let documentation_root = options.documentation_root.clone();
        let registry = Registry::new(options, &layout.source_root);
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut modules = BTreeSet::new();
        let paths: Vec<&str> = tree::typescript_files(
            typescript.source(),
            &layout.source_root,
            typescript.settings(),
        )
        .filter(|path| !path.ends_with(".d.ts"))
        .collect();

        for path in paths {
            if let Some(file) = project::parsed(&typescript, path)?
                && declares_native_module(&file, &package, &decorator)
            {
                modules.insert(path.to_owned());
            }
        }

        let tree = run.tree();
        let text = tree.text(&registry.path);
        let findings = registry.findings(text.as_deref(), &modules, |page| tree.text(page));
        let chapters = Chapters {
            registry: &registry.path,
            header: &header,
            source_root: &layout.source_root,
            documentation_root: &documentation_root,
        };
        let unlinked = text
            .as_deref()
            .map(|text| chapters.missing(text))
            .unwrap_or_default();

        for finding in findings {
            let help = finding.aspect_help();
            let location = match finding.line {
                Some(line) => Location::line(&registry.path, line),
                None => Location::file(&registry.path),
            };

            run.report()
                .finding(finding.aspect, location, finding.message)
                .help(help)
                .key(finding.key);
        }

        for (line, key, message) in unlinked {
            run.report()
                .finding(
                    "owner-chapter",
                    Location::line(&registry.path, line),
                    message,
                )
                .help(format!(
                    "link the row's chapter cell to exactly one Markdown chapter below {documentation_root}/"
                ))
                .key(key);
        }

        Ok(())
    }
}

/// Whether a class that is neither abstract nor ambient carries a decorator call whose callee is
/// `decorator` of `package`, after parentheses, type assertions and non-null assertions.
fn declares_native_module(file: &SourceFile<'_>, package: &str, decorator: &[String]) -> bool {
    let Some(bindings) = ImportBindings::new(file, package, false) else {
        return false;
    };
    let mut finder = ModuleFinder {
        bindings: &bindings,
        decorator,
        found: false,
    };
    finder.visit_program(file.program());
    finder.found
}

/// Looks for a native module class.
struct ModuleFinder<'b, 's, 'a> {
    bindings: &'b ImportBindings<'s, 'a>,
    decorator: &'b [String],
    found: bool,
}

impl<'a> Visit<'a> for ModuleFinder<'_, '_, 'a> {
    fn visit_class(&mut self, class: &Class<'a>) {
        if !class.r#abstract
            && !class.declare
            && class.decorators.iter().any(|decorator| {
                decorator_call(&decorator.expression).is_some_and(|call| {
                    self.bindings.resolve(&call.callee).as_deref() == Some(self.decorator)
                })
            })
        {
            self.found = true;
        }

        if !self.found {
            walk::walk_class(self, class);
        }
    }
}

/// The call a decorator makes, after parentheses, type assertions and non-null assertions; an
/// optional call such as `Module?.({})` is a call expression for the TypeScript compiler, where oxc
/// wraps it in a chain expression.
fn decorator_call<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e CallExpression<'a>> {
    match unwrap(expression) {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            ChainElement::TSNonNullExpression(inner) => decorator_call(&inner.expression),
            _ => None,
        },
        _ => None,
    }
}
