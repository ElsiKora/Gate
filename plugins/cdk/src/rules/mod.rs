//! The rules of the plugin, and what they share: how a finding is reported and the syntax
//! helpers the syntax-tree rules use.

pub mod app;
pub mod barrels;
pub mod construct_ids;
pub mod container_health;
pub mod declarations;
pub mod environment;
pub mod file_names;
pub mod layout;
pub mod nag;
pub mod physical_names;
pub mod properties;
pub mod removal_policy;
pub mod stack_names;
pub mod tags;
pub mod tests;

use gate_sdk::{Location, Run};
use gate_typescript::oxc_ast::ast::{Argument, Expression, ImportDeclarationSpecifier, Statement};
use gate_typescript::value::{string_like, unwrap};
use gate_typescript::{SourceFile, TypeScript};

use crate::sources::Cdk;

/// One finding to report.
#[derive(Debug)]
pub struct Found<'p> {
    /// The aspect.
    pub aspect: &'static str,
    /// The file or folder it is about, relative to the project root.
    pub path: &'p str,
    /// Its line, when it is about one.
    pub line: Option<u32>,
    /// The stable identity of what it is about within the path.
    pub key: String,
    /// What is wrong.
    pub message: String,
    /// What to do.
    pub help: &'static str,
}

/// Reports `found` on `run`: at its path (and line), its target the path, with its key and help.
pub fn report<O>(run: &mut Run<'_, O, Cdk>, found: Found<'_>) {
    let location = match found.line {
        Some(line) => Location::line(found.path, usize::try_from(line).unwrap_or(usize::MAX)),
        None => Location::file(found.path),
    };
    run.report()
        .finding(found.aspect, location, found.message)
        .key(found.key)
        .target(found.path)
        .help(found.help);
}

/// The module specifier the local name `local` is imported from in `file`.
#[must_use]
pub fn import_source<'a>(file: &SourceFile<'a>, local: &str) -> Option<&'a str> {
    file.program().body.iter().find_map(|statement| {
        let Statement::ImportDeclaration(import) = statement else {
            return None;
        };
        let specifiers = import.specifiers.as_ref()?;
        specifiers
            .iter()
            .any(|specifier| specifier_local(specifier) == local)
            .then_some(import.source.value.as_str())
    })
}

fn specifier_local<'s>(specifier: &'s ImportDeclarationSpecifier<'_>) -> &'s str {
    match specifier {
        ImportDeclarationSpecifier::ImportSpecifier(named) => named.local.name.as_str(),
        ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => default.local.name.as_str(),
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
            namespace.local.name.as_str()
        }
    }
}

/// Whether the local name `local` of `path` is imported from a file below `folder` (with its
/// trailing `/`).
///
/// # Errors
/// When the tsconfig cannot be read.
pub fn imported_from(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'_>,
    local: &str,
    folder: &str,
) -> gate_sdk::Result<bool> {
    let Some(specifier) = import_source(file, local) else {
        return Ok(false);
    };
    Ok(typescript
        .resolve(path, specifier)?
        .is_some_and(|target| target.starts_with(folder)))
}

/// Whether `expression` is a call of a function of `lib/naming/`: `create(…)` or
/// `naming.create(…)`, the callee imported from a file below that folder.
///
/// # Errors
/// When the tsconfig cannot be read.
pub fn is_naming_call(
    typescript: &TypeScript<'_>,
    path: &str,
    file: &SourceFile<'_>,
    expression: &Expression<'_>,
) -> gate_sdk::Result<bool> {
    let Expression::CallExpression(call) = unwrap(expression) else {
        return Ok(false);
    };
    let local = match unwrap(&call.callee) {
        Expression::Identifier(callee) => callee.name.as_str(),
        Expression::StaticMemberExpression(member) => match unwrap(&member.object) {
            Expression::Identifier(object) => object.name.as_str(),
            _ => return Ok(false),
        },
        _ => return Ok(false),
    };
    imported_from(typescript, path, file, local, "lib/naming/")
}

/// The expression of the argument at `index`, unless it is a spread.
#[must_use]
pub fn argument<'e, 'a>(arguments: &'e [Argument<'a>], index: usize) -> Option<&'e Expression<'a>> {
    arguments.get(index)?.as_expression()
}

/// The text of a string literal, or of a template literal without substitutions.
#[must_use]
pub fn text(expression: &Expression<'_>) -> Option<String> {
    string_like(unwrap(expression))
}

/// The folder of `path`, without its trailing `/`; empty at the root.
#[must_use]
pub fn folder(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The file name of `path`.
#[must_use]
pub fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `name.role.ts` as `("name", "role")`; `None` for any other form.
#[must_use]
pub fn name_and_role(file_name: &str) -> Option<(&str, &str)> {
    let stem = file_name.strip_suffix(".ts")?;
    let (name, role) = stem.split_once('.')?;
    (!role.contains('.')).then_some((name, role))
}
