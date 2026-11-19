//! The module specifiers one file loads, and which of them it loads as a runtime value.

use oxc_ast::ast::{
    Argument, CallExpression, ExportAllDeclaration, ExportFromDeclaration, ImportDeclaration,
    ImportDeclarationSpecifier, ImportExpression, ImportOrExportKind, TSImportEqualsDeclaration,
    TSImportType, TSModuleReference,
};
use oxc_ast_visit::{Visit, walk};

use super::SourceFile;
use super::binding::is_require;
use super::value::string_like;

/// Every module specifier a file loads, each once in the order of its first load, and those it
/// loads as a runtime value.
#[derive(Debug, Default)]
pub struct ModuleLoads {
    /// Every specifier, as a type or as a runtime value.
    pub all: Vec<String>,
    /// The specifiers loaded as a runtime value.
    pub runtime: Vec<String>,
}

impl ModuleLoads {
    fn add(&mut self, specifier: &str, runtime: bool) {
        if !self.all.iter().any(|known| known == specifier) {
            self.all.push(specifier.to_owned());
        }

        if runtime && !self.runtime.iter().any(|known| known == specifier) {
            self.runtime.push(specifier.to_owned());
        }
    }
}

/// The modules a file loads: through every import and export declaration with a source, a type
/// only when the whole clause or every named element is type-only; through `import x =
/// require(...)`, a type when written `import type`; through an `import("...")` type, always a
/// type; and through a dynamic `import("...")` or a call of a `require` function with exactly one
/// string argument, always a runtime value.
#[must_use]
pub fn module_loads(file: &SourceFile<'_>) -> ModuleLoads {
    // The semantic model resolves the references that the loader resolution reads.
    let _ = file.semantic();
    let mut finder = Finder {
        file,
        loads: ModuleLoads::default(),
    };
    finder.visit_program(file.program());
    finder.loads
}

/// Whether an import declaration loads its module only as a type: `import type`, or a clause
/// whose every element is a type-only named import.
#[must_use]
pub fn is_type_only_import(import: &ImportDeclaration<'_>) -> bool {
    import.import_kind == ImportOrExportKind::Type
        || import.specifiers.as_ref().is_some_and(|specifiers| {
            let named: Vec<_> = specifiers
                .iter()
                .filter_map(|specifier| match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(named) => Some(named),
                    _ => None,
                })
                .collect();

            !named.is_empty()
                && named.len() == specifiers.len()
                && named
                    .iter()
                    .all(|named| named.import_kind == ImportOrExportKind::Type)
        })
}

struct Finder<'f, 'a> {
    file: &'f SourceFile<'a>,
    loads: ModuleLoads,
}

impl<'a> Visit<'a> for Finder<'_, 'a> {
    fn visit_import_declaration(&mut self, import: &ImportDeclaration<'a>) {
        self.loads
            .add(&import.source.value, !is_type_only_import(import));
        walk::walk_import_declaration(self, import);
    }

    fn visit_export_all_declaration(&mut self, export: &ExportAllDeclaration<'a>) {
        self.loads.add(
            &export.source.value,
            export.export_kind != ImportOrExportKind::Type,
        );
        walk::walk_export_all_declaration(self, export);
    }

    fn visit_export_from_declaration(&mut self, export: &ExportFromDeclaration<'a>) {
        let type_only = export.export_kind == ImportOrExportKind::Type
            || (!export.specifiers.is_empty()
                && export
                    .specifiers
                    .iter()
                    .all(|specifier| specifier.export_kind == ImportOrExportKind::Type));
        self.loads.add(&export.source.value, !type_only);
        walk::walk_export_from_declaration(self, export);
    }

    fn visit_ts_import_equals_declaration(&mut self, import: &TSImportEqualsDeclaration<'a>) {
        if let TSModuleReference::ExternalModuleReference(reference) = &import.module_reference {
            self.loads.add(
                &reference.expression.value,
                import.import_kind != ImportOrExportKind::Type,
            );
        }

        walk::walk_ts_import_equals_declaration(self, import);
    }

    fn visit_ts_import_type(&mut self, import: &TSImportType<'a>) {
        self.loads.add(&import.source.value, false);
        walk::walk_ts_import_type(self, import);
    }

    fn visit_import_expression(&mut self, import: &ImportExpression<'a>) {
        if import.options.is_none()
            && import.phase.is_none()
            && let Some(specifier) = string_like(&import.source)
        {
            self.loads.add(&specifier, true);
        }

        walk::walk_import_expression(self, import);
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if let [argument] = call.arguments.as_slice()
            && let Some(specifier) = argument_text(argument)
            && is_require(self.file, &call.callee)
        {
            self.loads.add(&specifier, true);
        }

        walk::walk_call_expression(self, call);
    }
}

fn argument_text(argument: &Argument<'_>) -> Option<String> {
    argument.as_expression().and_then(string_like)
}
