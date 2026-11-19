//! The namespace forms `nestjs/string-enums` does not resolve, reported instead of answered.
//!
//! The binder follows TypeScript's namespace merging for the common forms. Beyond them the
//! compiler and the rule could disagree, so these forms fail closed with an `unsupported-namespace`
//! finding that names the construct: an external module declaration (`declare module "m"`), a
//! global augmentation, a namespace declared outside the file's top level and namespace bodies, a
//! name declared with different kinds (class, function, enum, variable, import alias or namespace)
//! in different blocks of one merged namespace, a namespace merged with a `const enum`, a variable,
//! an import alias or a default export, and, outside the test root, a `keyof typeof X.Y` alias with
//! a prefix `X` that is a value rather than a namespace, enum or import alias: the compiler follows
//! the type of such a value, which the rule does not know. A dotted `namespace A.B.C {}` counts as a
//! declaration of each of `A`, `B` and `C`.

use std::collections::BTreeMap;

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, Statement, TSNamespaceDeclaration,
    TSNamespaceDeclarationBody,
};
use gate_typescript::oxc_span::GetSpan;

use crate::shared::report::Findings;

/// What a name of a namespace scope declares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Class,
    Function,
    Enum,
    ConstEnum,
    Variable,
    ImportAlias,
    DefaultExport,
    Namespace,
}

impl Kind {
    fn text(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Function => "function",
            Self::Enum => "enum",
            Self::ConstEnum => "const enum",
            Self::Variable => "variable",
            Self::ImportAlias => "import alias",
            Self::DefaultExport => "default export",
            Self::Namespace => "namespace",
        }
    }

    fn article(self) -> &'static str {
        match self {
            Self::Enum | Self::ImportAlias => "an",
            _ => "a",
        }
    }
}

/// One declaration of a name: the block that holds it, its kind and its start.
struct Member {
    block: u32,
    kind: Kind,
    start: u32,
}

/// The declarations of each `(namespace path, name)` of a file, the file's own scope with the
/// empty path.
type Members = BTreeMap<(String, String), Vec<Member>>;

fn record(members: &mut Members, path: &str, name: &str, member: Member) {
    members
        .entry((path.to_owned(), name.to_owned()))
        .or_default()
        .push(member);
}

/// Records a namespace and every segment of a dotted name, then the members of its body. The
/// declaration itself is the one block that holds its inner segments.
fn namespace(
    declaration: &TSNamespaceDeclaration<'_>,
    block: u32,
    start: u32,
    path: &str,
    members: &mut Members,
) {
    let name = declaration.id.name.as_str();
    record(
        members,
        path,
        name,
        Member {
            block,
            kind: Kind::Namespace,
            start,
        },
    );
    let inner = format!("{path}.{name}");

    match &declaration.body {
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(nested) => {
            namespace(
                nested,
                declaration.span.start,
                nested.span.start,
                &inner,
                members,
            );
        }
        TSNamespaceDeclarationBody::TSModuleBlock(body) => {
            collect(&body.body, body.span.start, &inner, members);
        }
    }
}

/// Records the members of one block of the namespace at `path`, and walks its namespaces.
fn collect(statements: &[Statement<'_>], block: u32, path: &str, members: &mut Members) {
    for statement in statements {
        let start = statement.span().start;
        let declaration = match statement {
            Statement::ExportDeclaration(export) => &export.declaration,
            Statement::ExportDefaultDeclaration(export) => {
                let name = match &export.declaration {
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => class.id.as_ref(),
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        function.id.as_ref()
                    }
                    _ => None,
                };

                if let Some(id) = name {
                    let member = Member {
                        block,
                        kind: Kind::DefaultExport,
                        start,
                    };
                    record(members, path, &id.name, member);
                }

                continue;
            }
            _ => match statement.as_declaration() {
                Some(declaration) => declaration,
                None => continue,
            },
        };
        let (name, kind) = match declaration {
            Declaration::ClassDeclaration(class) => {
                let Some(id) = &class.id else { continue };
                (id.name.as_str(), Kind::Class)
            }
            Declaration::FunctionDeclaration(function) => {
                let Some(id) = &function.id else { continue };
                (id.name.as_str(), Kind::Function)
            }
            Declaration::TSEnumDeclaration(declaration) => (
                declaration.id.name.as_str(),
                if declaration.r#const {
                    Kind::ConstEnum
                } else {
                    Kind::Enum
                },
            ),
            Declaration::VariableDeclaration(list) => {
                for declarator in &list.declarations {
                    if let BindingPattern::BindingIdentifier(identifier) = &declarator.id {
                        let member = Member {
                            block,
                            kind: Kind::Variable,
                            start,
                        };
                        record(members, path, &identifier.name, member);
                    }
                }

                continue;
            }
            Declaration::TSImportEqualsDeclaration(alias) => {
                (alias.id.name.as_str(), Kind::ImportAlias)
            }
            Declaration::TSNamespaceDeclaration(declaration) => {
                namespace(declaration, block, start, path, members);
                continue;
            }
            _ => continue,
        };

        record(members, path, name, Member { block, kind, start });
    }
}

/// The first declaration of a name that merges with another in a way the binder does not resolve,
/// with the construct it forms.
fn unsupported(qualified: &str, declarations: &[Member]) -> Option<(u32, String)> {
    for (index, later) in declarations.iter().enumerate() {
        for earlier in &declarations[..index] {
            if earlier.block != later.block && earlier.kind != later.kind {
                return Some((
                    later.start,
                    format!(
                        "{qualified} declared as {} {} and {} {} in separate namespace blocks",
                        earlier.kind.article(),
                        earlier.kind.text(),
                        later.kind.article(),
                        later.kind.text()
                    ),
                ));
            }
        }
    }

    let namespace = declarations
        .iter()
        .find(|member| member.kind == Kind::Namespace)?;
    let other = declarations.iter().find(|member| {
        matches!(
            member.kind,
            Kind::ConstEnum | Kind::Variable | Kind::ImportAlias | Kind::DefaultExport
        )
    })?;

    Some((
        namespace.start.max(other.start),
        format!(
            "the {} {qualified} merged with a namespace",
            other.kind.text()
        ),
    ))
}

/// The constructs of a file that the rule does not resolve, by start.
fn constructs(file: &SourceFile<'_>) -> Vec<(u32, String)> {
    let mut found = Vec::new();
    let nodes = file.semantic().nodes();

    for node in nodes.iter() {
        match node.kind() {
            AstKind::TSExternalModuleDeclaration(module) => found.push((
                module.span.start,
                format!("the external module declaration \"{}\"", module.id.value),
            )),
            AstKind::TSGlobalDeclaration(global) => {
                found.push((global.span.start, "a global augmentation".to_owned()));
            }
            AstKind::TSNamespaceDeclaration(declaration)
                if !matches!(
                    nodes.parent_kind(node.id()),
                    AstKind::Program(_)
                        | AstKind::TSModuleBlock(_)
                        | AstKind::ExportDeclaration(_)
                        | AstKind::TSNamespaceDeclaration(_)
                ) =>
            {
                found.push((
                    declaration.span.start,
                    format!(
                        "the namespace {} outside a namespace body",
                        declaration.id.name
                    ),
                ));
            }
            _ => {}
        }
    }

    let program = file.program();
    let mut members = Members::new();
    collect(&program.body, program.span.start, "", &mut members);

    for ((path, name), declarations) in &members {
        let qualified = format!("{path}.{name}");
        found.extend(unsupported(qualified.trim_start_matches('.'), declarations));
    }

    found
}

/// Reports one construct the rule does not resolve, starting at `start`.
pub(super) fn report_construct(
    file: &SourceFile<'_>,
    path: &str,
    start: u32,
    construct: &str,
    findings: &mut Findings,
) {
    findings.push(
        "unsupported-namespace",
        path,
        Some(file.line(start)),
        format!(
            "uses {construct}, a namespace form nestjs/string-enums does not resolve; remove it"
        ),
    );
}

/// Reports every namespace construct of a file that the rule does not resolve.
pub(super) fn report(file: &SourceFile<'_>, path: &str, findings: &mut Findings) {
    for (start, construct) in constructs(file) {
        report_construct(file, path, start, &construct, findings);
    }
}
