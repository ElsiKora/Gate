//! `cdk/declarations`: a code file of the project (`bin/`, `lib/`, `test/`, `scripts/`) declares one
//! top-level entity and nothing below the top level; below `lib/` the entity's kind matches the
//! file's role and a class-role file's class is named from its file.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, ClassType, Declaration, ExportDefaultDeclarationKind, Expression, Statement,
};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::value::unwrap;

use crate::options::PluginOptions;
use crate::rules::{Found, file_name, name_and_role, report};
use crate::sources::{Cdk, code_files, parsed};
use crate::standard::{CLASS_ROLES, pascal};

/// The rule.
#[derive(Debug, Default)]
pub struct Declarations;

impl Rule<Cdk> for Declarations {
    type Options = NoOptions;

    const NAME: &'static str = "declarations";
    const TITLE: &'static str = "A code file declares one top-level entity; below lib/ it is of its role's kind, named from the file";
    const ASPECTS: &'static [&'static str] = &["count", "name", "kind", "nested"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            for folder in CODE_FOLDERS {
                for path in code_files(typescript, folder) {
                    if file_name(path) == "index.ts" {
                        continue;
                    }
                    let named = name_and_role(file_name(path));
                    if folder == "lib/" && named.is_none() {
                        continue;
                    }
                    let Some(file) = parsed(typescript, path)? else {
                        continue;
                    };
                    let role = named.filter(|_| folder == "lib/");
                    for found in judge(&file, path, role) {
                        report(run, found);
                    }
                }
            }
            Ok(())
        })
    }
}

/// The kind of a top-level entity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Class,
    Interface,
    TypeAlias,
    Enum,
    Function,
    Variable,
    Default,
}

impl Kind {
    fn noun(self) -> &'static str {
        match self {
            Self::Class => "a class",
            Self::Interface => "an interface",
            Self::TypeAlias => "a type alias",
            Self::Enum => "an enum",
            Self::Function => "a function",
            Self::Variable => "a constant",
            Self::Default => "a default export",
        }
    }
}

/// One top-level entity of a file.
#[derive(Debug)]
struct Entity {
    kind: Kind,
    name: String,
    start: u32,
}

/// The folders whose code the rule reads: the whole app, its tests and its scripts.
const CODE_FOLDERS: [&str; 4] = ["bin/", "lib/", "test/", "scripts/"];

/// The findings of `file`: its count and nested declarations, and, for a file of `lib/` named
/// `<stem>.<role>.ts`, the kind and the name of each entity.
fn judge<'p>(file: &SourceFile<'_>, path: &'p str, named: Option<(&str, &str)>) -> Vec<Found<'p>> {
    let mut found = Vec::new();
    let entities = entities(file);

    for (name, start) in nested(file) {
        found.push(Found {
            aspect: "nested",
            path,
            line: Some(file.line(start)),
            key: name.clone(),
            message: format!("{path} declares {name} below the top level"),
            help: "move the declaration to the top level of a file of its own",
        });
    }

    if entities.len() > 1 {
        let names: Vec<&str> = entities.iter().map(|entity| entity.name.as_str()).collect();
        found.push(Found {
            aspect: "count",
            path,
            line: None,
            key: "count".to_owned(),
            message: format!(
                "{path} declares {} entities ({}); a file declares one",
                entities.len(),
                names.join(", ")
            ),
            help: "move each entity into a file of its own, named for it",
        });
    }

    let Some((stem, role)) = named else {
        return found;
    };
    let class_role = CLASS_ROLES.contains(&role);
    let expected = if class_role {
        Some(Kind::Class)
    } else {
        match role {
            "interface" => Some(Kind::Interface),
            "type" => Some(Kind::TypeAlias),
            "enum" => Some(Kind::Enum),
            "function" => Some(Kind::Function),
            "constant" => Some(Kind::Variable),
            _ => None,
        }
    };

    for entity in &entities {
        let misplaced = match entity.kind {
            Kind::Class => !class_role,
            Kind::Interface => role != "interface",
            Kind::TypeAlias => role != "type",
            Kind::Enum => role != "enum",
            Kind::Function | Kind::Variable | Kind::Default => false,
        };
        if misplaced || expected.is_some_and(|kind| kind != entity.kind) {
            found.push(Found {
                aspect: "kind",
                path,
                line: Some(file.line(entity.start)),
                key: entity.name.clone(),
                message: format!(
                    "{path} is a .{role}.ts file and declares {} {}",
                    entity.kind.noun(),
                    entity.name
                ),
                help: "declare the entity in a file whose role is its kind",
            });
            continue;
        }
        if class_role && entity.kind == Kind::Class {
            let wanted = pascal(stem) + &pascal(role);
            if entity.name != wanted {
                found.push(Found {
                    aspect: "name",
                    path,
                    line: Some(file.line(entity.start)),
                    key: entity.name.clone(),
                    message: format!(
                        "{path} declares the class {}; its file names it {wanted}",
                        entity.name
                    ),
                    help: "name the class from its file: Pascal(<name>) + Pascal(<role>)",
                });
            }
        }
    }

    found
}

/// A top-level entity before overloads are merged: kind, name, start.
type Raw<'a> = (Kind, &'a str, u32);

/// The top-level entities of `file`: every class, interface, type alias and enum, the exported
/// functions (overloads once) and constants, and the default export.
fn entities(file: &SourceFile<'_>) -> Vec<Entity> {
    let mut raw: Vec<Raw<'_>> = Vec::new();

    for statement in &file.program().body {
        match statement {
            Statement::ClassDeclaration(class) => raw.push((
                Kind::Class,
                class
                    .id
                    .as_ref()
                    .map_or("<anonymous>", |id| id.name.as_str()),
                class.span.start,
            )),
            Statement::TSInterfaceDeclaration(interface) => raw.push((
                Kind::Interface,
                interface.id.name.as_str(),
                interface.span.start,
            )),
            Statement::TSTypeAliasDeclaration(alias) => {
                raw.push((Kind::TypeAlias, alias.id.name.as_str(), alias.span.start));
            }
            Statement::TSEnumDeclaration(declaration) => raw.push((
                Kind::Enum,
                declaration.id.name.as_str(),
                declaration.span.start,
            )),
            Statement::ExportDeclaration(export) => {
                exported(&export.declaration, export.span.start, &mut raw);
            }
            Statement::ExportDefaultDeclaration(export) => {
                raw.push(default_export(&export.declaration, export.span.start));
            }
            _ => {}
        }
    }

    let mut entities: Vec<Entity> = Vec::new();
    for (kind, name, start) in raw {
        let overload = kind == Kind::Function
            && entities
                .iter()
                .any(|entity| entity.kind == Kind::Function && entity.name == name);
        if !overload {
            entities.push(Entity {
                kind,
                name: name.to_owned(),
                start,
            });
        }
    }
    entities
}

/// The entities an `export` declaration declares.
fn exported<'a>(declaration: &Declaration<'a>, start: u32, raw: &mut Vec<Raw<'a>>) {
    match declaration {
        Declaration::ClassDeclaration(class) => raw.push((
            Kind::Class,
            class
                .id
                .as_ref()
                .map_or("<anonymous>", |id| id.name.as_str()),
            start,
        )),
        Declaration::TSInterfaceDeclaration(interface) => {
            raw.push((Kind::Interface, interface.id.name.as_str(), start));
        }
        Declaration::TSTypeAliasDeclaration(alias) => {
            raw.push((Kind::TypeAlias, alias.id.name.as_str(), start));
        }
        Declaration::TSEnumDeclaration(declaration) => {
            raw.push((Kind::Enum, declaration.id.name.as_str(), start));
        }
        Declaration::FunctionDeclaration(function) => raw.push((
            Kind::Function,
            function
                .id
                .as_ref()
                .map_or("<anonymous>", |id| id.name.as_str()),
            start,
        )),
        Declaration::VariableDeclaration(variables) => {
            for declarator in &variables.declarations {
                let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
                    continue;
                };
                let function = declarator.init.as_ref().is_some_and(|init| {
                    matches!(
                        unwrap(init),
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    )
                });
                let kind = if function {
                    Kind::Function
                } else {
                    Kind::Variable
                };
                raw.push((kind, binding.name.as_str(), start));
            }
        }
        _ => {}
    }
}

/// The entity an `export default` declaration declares.
fn default_export<'a>(declaration: &ExportDefaultDeclarationKind<'a>, start: u32) -> Raw<'a> {
    match declaration {
        ExportDefaultDeclarationKind::ClassDeclaration(class) => (
            Kind::Class,
            class.id.as_ref().map_or("<default>", |id| id.name.as_str()),
            start,
        ),
        ExportDefaultDeclarationKind::FunctionDeclaration(function) => (
            Kind::Function,
            function
                .id
                .as_ref()
                .map_or("<default>", |id| id.name.as_str()),
            start,
        ),
        ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
            (Kind::Interface, interface.id.name.as_str(), start)
        }
        _ => (Kind::Default, "<default>", start),
    }
}

/// The classes, interfaces, type aliases and enums of `file` below its top level, with where
/// each starts.
fn nested(file: &SourceFile<'_>) -> Vec<(String, u32)> {
    let nodes = file.semantic().nodes();
    let mut found = Vec::new();

    for node in nodes.iter() {
        let (name, start) = match node.kind() {
            AstKind::Class(class) if class.r#type == ClassType::ClassDeclaration => (
                class
                    .id
                    .as_ref()
                    .map_or("<anonymous>", |id| id.name.as_str()),
                class.span.start,
            ),
            AstKind::TSInterfaceDeclaration(interface) => {
                (interface.id.name.as_str(), interface.span.start)
            }
            AstKind::TSTypeAliasDeclaration(alias) => (alias.id.name.as_str(), alias.span.start),
            AstKind::TSEnumDeclaration(declaration) => {
                (declaration.id.name.as_str(), declaration.span.start)
            }
            _ => continue,
        };
        if !top_level(file, node.id()) {
            found.push((name.to_owned(), start));
        }
    }

    found
}

/// Whether the declaration `node` is a statement of the program, exported or not.
fn top_level(file: &SourceFile<'_>, node: NodeId) -> bool {
    let nodes = file.semantic().nodes();
    let mut parent = nodes.parent_id(node);
    if matches!(
        nodes.kind(parent),
        AstKind::ExportDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
    ) {
        parent = nodes.parent_id(parent);
    }
    matches!(nodes.kind(parent), AstKind::Program(_))
}
