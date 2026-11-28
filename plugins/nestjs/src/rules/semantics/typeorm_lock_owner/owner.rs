//! The owner file and its calls: the one exported function, and whether a call's callee is that
//! function, bound through the file's scopes to an import and followed through the module
//! resolver and re-exports.

use std::collections::HashSet;

use gate_typescript::SourceFile;
use gate_typescript::owners::{SourceContext, export_name, identifier_name, top_level};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    CallExpression, Declaration, ExportDefaultDeclarationKind, Expression, IdentifierReference,
    ImportOrExportKind, Statement, VariableDeclarationKind,
};

use super::Context;

/// The owner's exported function.
#[derive(Clone, Debug)]
pub(crate) struct Function {
    /// The name it is exported under (`default` for a default export).
    pub export: String,
    /// The name messages print: its own name, or the export name.
    pub name: String,
}

/// What one value export is.
enum Export {
    Function(Function),
    Other,
}

/// The one function the owner file exports, when its only value export is a function declaration
/// or a `const` bound to an arrow or function expression. Type exports do not count.
pub(crate) fn function(source: &SourceContext<'_>) -> Option<Function> {
    let mut exports = Vec::new();

    for statement in top_level(&source.file) {
        match statement {
            Statement::ExportDeclaration(export) => match &export.declaration {
                // An overload signature or a `declare function` has no body and is no export of
                // its own: the implementation is.
                Declaration::FunctionDeclaration(function) if function.body.is_none() => {}
                Declaration::FunctionDeclaration(function) => {
                    let name = function.id.as_ref()?.name.to_string();
                    exports.push(Export::Function(Function {
                        export: name.clone(),
                        name,
                    }));
                }
                Declaration::VariableDeclaration(list) => {
                    for declarator in &list.declarations {
                        let name = identifier_name(declarator).map(str::to_owned);
                        let callable = list.kind == VariableDeclarationKind::Const
                            && declarator.init.as_ref().is_some_and(callable);

                        exports.push(match (name, callable) {
                            (Some(name), true) => Export::Function(Function {
                                export: name.clone(),
                                name,
                            }),
                            _ => Export::Other,
                        });
                    }
                }
                Declaration::TSInterfaceDeclaration(_) | Declaration::TSTypeAliasDeclaration(_) => {
                }
                _ => exports.push(Export::Other),
            },
            Statement::ExportNamedDeclaration(export)
                if export.export_kind != ImportOrExportKind::Type =>
            {
                for specifier in &export.specifiers {
                    if specifier.export_kind == ImportOrExportKind::Type {
                        continue;
                    }

                    let local = export_name(&specifier.local);
                    exports.push(if local_function(&source.file, &local) {
                        Export::Function(Function {
                            export: export_name(&specifier.exported),
                            name: local,
                        })
                    } else {
                        Export::Other
                    });
                }
            }
            Statement::ExportFromDeclaration(export)
                if export.export_kind != ImportOrExportKind::Type =>
            {
                exports.extend(
                    export
                        .specifiers
                        .iter()
                        .filter(|specifier| specifier.export_kind != ImportOrExportKind::Type)
                        .map(|_| Export::Other),
                );
            }
            Statement::ExportAllDeclaration(export)
                if export.export_kind != ImportOrExportKind::Type =>
            {
                exports.push(Export::Other);
            }
            Statement::ExportDefaultDeclaration(export) => {
                exports.extend(default_export(&source.file, &export.declaration));
            }
            Statement::TSExportAssignment(_) => exports.push(Export::Other),
            _ => {}
        }
    }

    match exports.as_slice() {
        [Export::Function(function)] => Some(function.clone()),
        _ => None,
    }
}

/// What a default export is; `None` for an interface, which is a type, and for an overload
/// signature.
fn default_export(
    file: &SourceFile<'_>,
    kind: &ExportDefaultDeclarationKind<'_>,
) -> Option<Export> {
    let function = |name: String| {
        Export::Function(Function {
            export: "default".to_owned(),
            name,
        })
    };

    Some(match kind {
        ExportDefaultDeclarationKind::FunctionDeclaration(declaration)
            if declaration.body.is_none() =>
        {
            return None;
        }
        ExportDefaultDeclarationKind::FunctionDeclaration(declaration) => function(
            declaration
                .id
                .as_ref()
                .map_or_else(|| "default".to_owned(), |id| id.name.to_string()),
        ),
        ExportDefaultDeclarationKind::ArrowFunctionExpression(_)
        | ExportDefaultDeclarationKind::FunctionExpression(_) => function("default".to_owned()),
        ExportDefaultDeclarationKind::Identifier(identifier)
            if local_function(file, &identifier.name) =>
        {
            function(identifier.name.to_string())
        }
        ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => return None,
        _ => Export::Other,
    })
}

/// Whether an initializer is an arrow or function expression.
fn callable(expression: &Expression<'_>) -> bool {
    matches!(
        gate_typescript::value::unwrap(expression),
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
    )
}

/// Whether `name` is a top-level function declaration or a `const` bound to a function.
fn local_function(file: &SourceFile<'_>, name: &str) -> bool {
    top_level(file).any(|statement| {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => &export.declaration,
            Statement::FunctionDeclaration(function) => {
                return function.id.as_ref().is_some_and(|id| id.name == name);
            }
            Statement::VariableDeclaration(list) => {
                return list.kind == VariableDeclarationKind::Const
                    && list.declarations.iter().any(|declarator| {
                        identifier_name(declarator) == Some(name)
                            && declarator.init.as_ref().is_some_and(callable)
                    });
            }
            _ => return false,
        };

        match declaration {
            Declaration::FunctionDeclaration(function) => {
                function.id.as_ref().is_some_and(|id| id.name == name)
            }
            Declaration::VariableDeclaration(list) => {
                list.kind == VariableDeclarationKind::Const
                    && list.declarations.iter().any(|declarator| {
                        identifier_name(declarator) == Some(name)
                            && declarator.init.as_ref().is_some_and(callable)
                    })
            }
            _ => false,
        }
    })
}

/// The import an identifier is bound to through the file's scopes: the resolved file and the
/// imported name. A local declaration, parameter or shadowing binding is no import.
pub(crate) fn imported(
    context: &Context<'_, '_>,
    file: &SourceFile<'_>,
    identifier: &IdentifierReference<'_>,
    namespace: bool,
) -> Option<(String, String)> {
    let (module, name) = import(file, identifier, namespace)?;
    let target = context
        .typescript
        .resolve(file.path(), &module)
        .ok()
        .flatten()?;

    Some((target, name))
}

/// The import an identifier is bound to through the file's scopes: the module specifier as
/// written and the imported name (empty for a namespace import, which `namespace` asks for).
pub(crate) fn import(
    file: &SourceFile<'_>,
    identifier: &IdentifierReference<'_>,
    namespace: bool,
) -> Option<(String, String)> {
    let semantic = file.semantic();
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let symbol = scoping
        .get_reference(identifier.reference_id.get()?)
        .symbol_id()?;
    let declarations: Vec<_> = scoping.symbol_declarations(symbol).collect();
    let [declaration] = declarations.as_slice() else {
        return None;
    };
    let AstKind::ImportDeclaration(import) = nodes.parent_kind(*declaration) else {
        return None;
    };
    let name = match nodes.kind(*declaration) {
        AstKind::ImportSpecifier(specifier) if !namespace => export_name(&specifier.imported),
        AstKind::ImportDefaultSpecifier(_) if !namespace => "default".to_owned(),
        AstKind::ImportNamespaceSpecifier(_) if namespace => String::new(),
        _ => return None,
    };

    Some((import.source.value.to_string(), name))
}

/// Whether a call's callee is the owner's function: an identifier bound to an import of it, or a
/// member of a namespace import that exports it.
pub(crate) fn is_call(
    context: &Context<'_, '_>,
    file: &SourceFile<'_>,
    call: &CallExpression<'_>,
) -> bool {
    match gate_typescript::value::unwrap(&call.callee) {
        Expression::Identifier(identifier) => imported(context, file, identifier, false)
            .is_some_and(|(target, name)| exports(context, &target, &name, &mut HashSet::new())),
        Expression::StaticMemberExpression(member) => match &member.object {
            Expression::Identifier(namespace) => imported(context, file, namespace, true)
                .is_some_and(|(target, _)| {
                    exports(context, &target, &member.property.name, &mut HashSet::new())
                }),
            _ => false,
        },
        _ => false,
    }
}

/// Whether the export `name` of the file at `path` is the owner's function, followed through named
/// re-exports, exported imports and star exports.
fn exports(
    context: &Context<'_, '_>,
    path: &str,
    name: &str,
    visited: &mut HashSet<(String, String)>,
) -> bool {
    if path == context.owner {
        return name == context.function.export;
    }

    let key = (path.to_owned(), name.to_owned());

    if let Some(known) = context.exports.borrow().get(&key) {
        return *known;
    }

    // Only the first call of a chase sees every way on; a later one may be cut short by a cycle,
    // so only its yes is kept.
    let first = visited.is_empty();

    if !visited.insert(key.clone()) {
        return false;
    }

    let answer = chase(context, path, name, visited);

    if answer || first {
        context.exports.borrow_mut().insert(key, answer);
    }

    answer
}

/// The re-exports of the file at `path` that could make its export `name` the owner's function.
fn chase(
    context: &Context<'_, '_>,
    path: &str,
    name: &str,
    visited: &mut HashSet<(String, String)>,
) -> bool {
    let Ok(Some(file)) = context.typescript.parse(path) else {
        return false;
    };
    let resolve = |specifier: &str| context.typescript.resolve(path, specifier).ok().flatten();
    let mut stars = Vec::new();

    for statement in top_level(&file) {
        match statement {
            Statement::ExportFromDeclaration(export)
                if export.export_kind != ImportOrExportKind::Type =>
            {
                for specifier in &export.specifiers {
                    if specifier.export_kind != ImportOrExportKind::Type
                        && export_name(&specifier.exported) == name
                    {
                        return resolve(&export.source.value).is_some_and(|target| {
                            exports(context, &target, &export_name(&specifier.local), visited)
                        });
                    }
                }
            }
            Statement::ExportNamedDeclaration(export)
                if export.export_kind != ImportOrExportKind::Type =>
            {
                for specifier in &export.specifiers {
                    if specifier.export_kind != ImportOrExportKind::Type
                        && export_name(&specifier.exported) == name
                    {
                        let local = export_name(&specifier.local);

                        return import_of(&file, &local).is_some_and(|(module, imported)| {
                            resolve(&module)
                                .is_some_and(|target| exports(context, &target, &imported, visited))
                        });
                    }
                }
            }
            Statement::ExportAllDeclaration(export)
                if export.exported.is_none() && export.export_kind != ImportOrExportKind::Type =>
            {
                stars.push(export.source.value.to_string());
            }
            _ => {}
        }
    }

    stars
        .iter()
        .any(|star| resolve(star).is_some_and(|target| exports(context, &target, name, visited)))
}

/// The module and imported name of a value import whose local name is `local`.
fn import_of(file: &SourceFile<'_>, local: &str) -> Option<(String, String)> {
    top_level(file).find_map(|statement| {
        let Statement::ImportDeclaration(import) = statement else {
            return None;
        };

        if import.import_kind == ImportOrExportKind::Type {
            return None;
        }

        import.specifiers.iter().flatten().find_map(|specifier| {
            use gate_typescript::oxc_ast::ast::ImportDeclarationSpecifier as Specifier;

            match specifier {
                Specifier::ImportSpecifier(named) if named.local.name == local => Some((
                    import.source.value.to_string(),
                    export_name(&named.imported),
                )),
                Specifier::ImportDefaultSpecifier(default) if default.local.name == local => {
                    Some((import.source.value.to_string(), "default".to_owned()))
                }
                _ => None,
            }
        })
    })
}
