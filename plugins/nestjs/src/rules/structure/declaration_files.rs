//! `nestjs/declaration-files`: a source, infrastructure-as-code or test-support file owns one
//! exported code entity, named contracts sit at the top level of a file in their kind folder, and a
//! test scenario declares no named contract or independently bound class.

use std::collections::{BTreeMap, HashMap, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::owners::unwrap;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BindingPattern, Class, Declaration,
    ExportDefaultDeclarationKind, Expression, Function, LogicalOperator, Statement,
    TSEnumDeclaration, TSInterfaceDeclaration, TSTypeAliasDeclaration, VariableDeclarator,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::syntax::decorated_start;

use crate::layout::{Layout, PluginOptions, folder_prefix};
use crate::shared::report::location;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct DeclarationFiles;

/// The options of `nestjs/declaration-files`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The folder each kind of named contract belongs in, by the compiler's node name
    /// (`EnumDeclaration`, `InterfaceDeclaration`, `TypeAliasDeclaration`).
    pub contract_kinds: BTreeMap<String, String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            contract_kinds: [
                ("EnumDeclaration", "enum"),
                ("InterfaceDeclaration", "interface"),
                ("TypeAliasDeclaration", "type"),
            ]
            .into_iter()
            .map(|(kind, folder)| (kind.to_owned(), folder.to_owned()))
            .collect(),
        }
    }
}

impl Rule<crate::Nest> for DeclarationFiles {
    type Options = Options;

    const NAME: &'static str = "declaration-files";
    const TITLE: &'static str =
        "Files own one exported entity and named contracts sit in their own kind-folder files";
    const ASPECTS: &'static [&'static str] = &[
        "kind-folder-declaration",
        "nested-declaration",
        "single-owner",
        "support-owner",
        "test-scenario-declaration",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let kinds = run.options().contract_kinds.clone();
            let source = shared.source();
            let typescript = shared.typescript()?;
            let settings = typescript.settings();
            let files = tree::distinct(
                tree::typescript_files(source, &layout.source_root, settings)
                    .chain(
                        tree::typescript_files(source, &layout.test_root, settings).filter(
                            |path| {
                                tree::is_test_support(&layout, path)
                                    || tree::is_test_scenario(&layout, path)
                            },
                        ),
                    )
                    .chain(tree::typescript_files(source, &layout.cdk_root, settings)),
            );

            for path in files {
                if !tree::is_test_scenario(&layout, path) && path.ends_with("/index.ts") {
                    continue;
                }

                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };

                for (aspect, line, key, message) in findings(&file, &layout, &kinds) {
                    run.report()
                        .finding(
                            aspect,
                            location(path, line.and_then(|line| usize::try_from(line).ok())),
                            message,
                        )
                        .help(help(aspect))
                        .key(key);
                }
            }

            Ok(())
        })
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "kind-folder-declaration" => {
            "move the contract to a file named for its kind in its kind folder"
        }
        "nested-declaration" => "move the contract to the top level of its own file",
        "test-scenario-declaration" => "move the contract or class to its own test-support file",
        _ => "give each class, interface, type and enum its own file",
    }
}

type Finding = (&'static str, Option<u32>, String, String);

/// The findings of one file, in report order.
fn findings(
    file: &SourceFile<'_>,
    layout: &Layout,
    kinds: &BTreeMap<String, String>,
) -> Vec<Finding> {
    let path = file.path();
    let mut findings = Vec::new();

    if tree::is_test_scenario(layout, path) {
        for entity in entities(file, true) {
            if [
                "ClassDeclaration",
                "ClassExpression",
                "InterfaceDeclaration",
                "TypeAliasDeclaration",
                "EnumDeclaration",
            ]
            .contains(&entity.kind)
            {
                findings.push((
                    "test-scenario-declaration",
                    Some(entity.line),
                    entity.name.clone(),
                    format!(
                        "{path} declares {} inside a test scenario; named contracts and independently bound classes require separate owning files",
                        entity.name
                    ),
                ));
            }
        }

        return findings;
    }

    let support = path.starts_with(&folder_prefix(&layout.test_root));
    let entities = entities(file, support);
    let factories = if support {
        entities
            .iter()
            .filter(|entity| ["FunctionDeclaration", "FunctionValuedConst"].contains(&entity.kind))
            .count()
    } else {
        0
    };

    if entities.len() - factories.saturating_sub(1) > 1 {
        let descriptions: Vec<String> = entities
            .iter()
            .map(|entity| format!("{} {}", entity.kind, entity.name))
            .collect();
        findings.push((
            if support { "support-owner" } else { "single-owner" },
            None,
            "owners".to_owned(),
            format!(
                "{path} contains multiple code owners: {}. Named classes, interfaces, types and enums require separate owning files regardless of export visibility; cohesive private functions may remain local.",
                descriptions.join(", ")
            ),
        ));
    }

    for entity in &entities {
        if entity.nested {
            findings.push((
                "nested-declaration",
                Some(entity.line),
                entity.name.clone(),
                format!(
                    "{path} declares nested {}; named contracts belong at the top level of their own file",
                    entity.name
                ),
            ));
        }

        let Some(folder) = kinds.get(entity.kind) else {
            continue;
        };

        if !in_kind_folder(path, folder, &layout.artifact_folders) {
            findings.push((
                "kind-folder-declaration",
                Some(entity.line),
                entity.name.clone(),
                format!(
                    "{path} declares {}; {folder} contracts belong in the owning {folder}/ folder with a .{folder}.ts filename",
                    entity.name
                ),
            ));
        }
    }

    findings
}

/// Whether a contract of `folder` kind lies in the file's last `folder` folder, below no other
/// artifact folder, in a file named `*.<folder>.ts` or `<folder>.ts`.
fn in_kind_folder(path: &str, folder: &str, outer: &[String]) -> bool {
    let folders: Vec<&str> = path.split('/').collect();
    let folders = &folders[..folders.len() - 1];
    let Some(index) = folders.iter().rposition(|segment| *segment == folder) else {
        return false;
    };

    !folders[..index]
        .iter()
        .any(|segment| outer.iter().any(|kind| kind == segment) && *segment != folder)
        && (path.ends_with(&format!(".{folder}.ts")) || path.ends_with(&format!("/{folder}.ts")))
}

/// One code entity of a file.
#[derive(Debug)]
struct Entity {
    kind: &'static str,
    line: u32,
    name: String,
    nested: bool,
}

/// The identity of an entity: one node, or one function name, whose overloads count once.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Key {
    Node(usize),
    Function(String),
}

/// A top-level declaration a local name may denote.
#[derive(Clone, Copy)]
enum Local<'a> {
    Class(&'a Class<'a>, u32),
    Function(&'a Function<'a>, u32),
    Interface(&'a TSInterfaceDeclaration<'a>, u32),
    TypeAlias(&'a TSTypeAliasDeclaration<'a>, u32),
    Enum(&'a TSEnumDeclaration<'a>, u32),
    Variable(&'a VariableDeclarator<'a>),
}

fn address<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

/// The code entities of a file in the order they are reported: the exported top-level
/// declarations and function-valued constants by export, the default export, then every class,
/// interface, type alias and enum in source order. In a test file an anonymous class expression
/// used inline (`inline`) is no contract.
fn entities<'a>(file: &SourceFile<'a>, inline: bool) -> Vec<Entity> {
    let mut scan = Scan {
        file,
        entities: Vec::new(),
        keys: HashMap::new(),
    };
    let mut locals: HashMap<String, Local<'a>> = HashMap::new();
    let mut exported: Vec<String> = Vec::new();
    let mut defaults: Vec<&'a Expression<'a>> = Vec::new();

    for statement in &file.program().body {
        let start = statement.span().start;
        let (declaration, is_exported) = match statement {
            Statement::ExportDeclaration(export) => (Some(&export.declaration), true),
            Statement::ExportDefaultDeclaration(default) => {
                let local = match &default.declaration {
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        Some(Local::Class(class, start))
                    }
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        Some(Local::Function(function, start))
                    }
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                        Some(Local::Interface(interface, start))
                    }
                    expression => {
                        if let Some(expression) = expression.as_expression() {
                            defaults.push(expression);
                        }
                        None
                    }
                };

                if let Some(local) = local {
                    let name = local_name(local);
                    locals.insert(name.clone(), local);
                    export(name, &mut exported);
                }
                continue;
            }
            Statement::TSExportAssignment(assignment) => {
                defaults.push(&assignment.expression);
                continue;
            }
            Statement::ExportNamedDeclaration(named) => {
                for specifier in &named.specifiers {
                    export(specifier.local.name().to_string(), &mut exported);
                }
                continue;
            }
            _ => (statement.as_declaration(), false),
        };
        let Some(declaration) = declaration else {
            continue;
        };
        let local = match declaration {
            Declaration::ClassDeclaration(class) => Local::Class(class, start),
            Declaration::FunctionDeclaration(function) => Local::Function(function, start),
            Declaration::TSInterfaceDeclaration(interface) => Local::Interface(interface, start),
            Declaration::TSTypeAliasDeclaration(alias) => Local::TypeAlias(alias, start),
            Declaration::TSEnumDeclaration(declaration) => Local::Enum(declaration, start),
            Declaration::VariableDeclaration(variables) => {
                for declarator in &variables.declarations {
                    let name = binding_name(file.source(), &declarator.id);
                    locals.insert(name.clone(), Local::Variable(declarator));

                    if is_exported {
                        export(name, &mut exported);
                    }
                }
                continue;
            }
            _ => continue,
        };
        let name = local_name(local);
        locals.insert(name.clone(), local);

        if is_exported {
            export(name, &mut exported);
        }
    }

    for name in &exported {
        scan.local(&locals, name, &mut HashSet::new());
    }

    for expression in defaults {
        scan.expression(&locals, expression, "<default>", &mut HashSet::new());
    }

    // The semantic model assigns the node identifiers that the contract walk reads.
    let _ = file.semantic();

    let mut contracts = Contracts {
        scan: &mut scan,
        inline,
    };
    contracts.visit_program(file.program());

    scan.entities
}

/// Adds a name to the exported names, once, in the order of its first export.
fn export(name: String, exported: &mut Vec<String>) {
    if !exported.contains(&name) {
        exported.push(name);
    }
}

fn local_name(local: Local<'_>) -> String {
    match local {
        Local::Class(class, _) => class.id.as_ref().map(|id| id.name.to_string()),
        Local::Function(function, _) => function.id.as_ref().map(|id| id.name.to_string()),
        Local::Interface(interface, _) => Some(interface.id.name.to_string()),
        Local::TypeAlias(alias, _) => Some(alias.id.name.to_string()),
        Local::Enum(declaration, _) => Some(declaration.id.name.to_string()),
        Local::Variable(_) => None,
    }
    .unwrap_or_else(|| "<anonymous>".to_owned())
}

/// The name of a binding: an identifier, or the source text of a destructuring pattern.
fn binding_name(source: &str, binding: &BindingPattern<'_>) -> String {
    match binding {
        BindingPattern::BindingIdentifier(identifier) => identifier.name.to_string(),
        pattern => {
            let span = pattern.span();
            source[span.start as usize..span.end as usize].to_owned()
        }
    }
}

struct Scan<'f, 'a> {
    file: &'f SourceFile<'a>,
    entities: Vec<Entity>,
    keys: HashMap<Key, usize>,
}

impl<'a> Scan<'_, 'a> {
    /// Adds an entity, or replaces the one of the same key in its place.
    fn set(&mut self, key: Key, entity: Entity) {
        if let Some(&index) = self.keys.get(&key) {
            self.entities[index] = entity;
        } else {
            self.keys.insert(key, self.entities.len());
            self.entities.push(entity);
        }
    }

    fn line(&self, start: u32) -> u32 {
        self.file.line(start)
    }

    /// The exported local `name`: its declaration, or the function or class its constant holds.
    fn local(
        &mut self,
        locals: &HashMap<String, Local<'a>>,
        name: &str,
        seen: &mut HashSet<String>,
    ) {
        if !seen.insert(name.to_owned()) {
            return;
        }

        let Some(&local) = locals.get(name) else {
            return;
        };
        let (key, kind, start) = match local {
            Local::Variable(declarator) => {
                if let Some(init) = &declarator.init {
                    self.expression(locals, init, name, seen);
                }
                return;
            }
            Local::Class(class, start) => (
                Key::Node(address(class)),
                "ClassDeclaration",
                decorated_start(start.min(class.span.start), &class.decorators),
            ),
            Local::Function(_, start) => (
                Key::Function(format!("FunctionDeclaration {name}")),
                "FunctionDeclaration",
                start,
            ),
            Local::Interface(interface, start) => {
                (Key::Node(address(interface)), "InterfaceDeclaration", start)
            }
            Local::TypeAlias(alias, start) => {
                (Key::Node(address(alias)), "TypeAliasDeclaration", start)
            }
            Local::Enum(declaration, start) => {
                (Key::Node(address(declaration)), "EnumDeclaration", start)
            }
        };
        let line = self.line(start);
        self.set(
            key,
            Entity {
                kind,
                line,
                name: name.to_owned(),
                nested: false,
            },
        );
    }

    /// An exported value: a function or class it holds, or the local it names.
    fn expression(
        &mut self,
        locals: &HashMap<String, Local<'a>>,
        expression: &'a Expression<'a>,
        name: &str,
        seen: &mut HashSet<String>,
    ) {
        let value = unwrap(expression);
        let (key, start) = match value {
            Expression::Identifier(identifier) => {
                self.local(locals, &identifier.name, seen);
                return;
            }
            Expression::ArrowFunctionExpression(arrow) => (address(&**arrow), arrow.span.start),
            Expression::FunctionExpression(function) => (address(&**function), function.span.start),
            Expression::ClassExpression(class) => (
                address(&**class),
                decorated_start(class.span.start, &class.decorators),
            ),
            _ => return,
        };
        let line = self.line(start);
        self.set(
            Key::Node(key),
            Entity {
                kind: "FunctionValuedConst",
                line,
                name: name.to_owned(),
                nested: false,
            },
        );
    }
}

/// Every class, interface, type alias and enum of the file, in source order.
struct Contracts<'s, 'f, 'a> {
    scan: &'s mut Scan<'f, 'a>,
    inline: bool,
}

impl<'a> Contracts<'_, '_, 'a> {
    fn semantic_parent(&self, node: NodeId) -> (NodeId, AstKind<'a>) {
        let nodes = self.scan.file.semantic().nodes();
        let parent = nodes.parent_id(node);
        (parent, nodes.kind(parent))
    }

    /// The start of a declaration as the compiler counts it: an enclosing `export` or
    /// `export default`, then its first decorator.
    fn declaration_start(&self, node: NodeId, start: u32) -> u32 {
        match self.semantic_parent(node).1 {
            AstKind::ExportDeclaration(export) => start.min(export.span.start),
            AstKind::ExportDefaultDeclaration(export) => start.min(export.span.start),
            _ => start,
        }
    }

    /// Whether a declaration lies outside the top level: its parent, through an `export`, is not
    /// the program.
    fn nested(&self, node: NodeId) -> bool {
        let (parent, kind) = self.semantic_parent(node);
        let kind = match kind {
            AstKind::ExportDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
                self.semantic_parent(parent).1
            }
            kind => kind,
        };

        !matches!(kind, AstKind::Program(_))
    }

    /// The outermost expression that hands a class expression on unchanged: through parentheses
    /// and assertions, a branch of a conditional, either side of `||` or `??`, and the right side
    /// of `&&` or the last operand of a comma.
    fn container(&self, node: NodeId, span: Span) -> (NodeId, Span) {
        let (mut node, mut span) = (node, span);

        loop {
            let (parent, kind) = self.semantic_parent(node);
            let passes = match kind {
                AstKind::ParenthesizedExpression(_)
                | AstKind::TSAsExpression(_)
                | AstKind::TSSatisfiesExpression(_)
                | AstKind::TSTypeAssertion(_)
                | AstKind::TSNonNullExpression(_) => true,
                AstKind::ConditionalExpression(conditional) => {
                    conditional.consequent.span() == span || conditional.alternate.span() == span
                }
                AstKind::LogicalExpression(logical) => {
                    logical.operator != LogicalOperator::And || logical.right.span() == span
                }
                AstKind::SequenceExpression(sequence) => sequence
                    .expressions
                    .last()
                    .is_some_and(|last| last.span() == span),
                _ => false,
            };

            if !passes {
                return (node, span);
            }

            span = kind.span();
            node = parent;
        }
    }

    /// Whether a container is the right side of an assignment to a bare identifier; the compiler
    /// keeps a parenthesized target `(a) = …` a parenthesized expression, which oxc drops.
    fn identifier_assignment(&self, container: NodeId, span: Span) -> bool {
        matches!(self.semantic_parent(container).1, AstKind::AssignmentExpression(assignment)
        if assignment.right.span() == span
            && matches!(&assignment.left, AssignmentTarget::AssignmentTargetIdentifier(target)
                if target.span.start == assignment.span.start)
            && matches!(
                assignment.operator,
                AssignmentOperator::Assign
                    | AssignmentOperator::LogicalAnd
                    | AssignmentOperator::LogicalOr
                    | AssignmentOperator::LogicalNullish
            ))
    }

    /// Whether a class expression is a top-level owner: exported by default, the initializer of a
    /// top-level variable or assigned to an identifier in a top-level expression statement.
    fn top_level_class_expression(&self, container: NodeId, span: Span) -> bool {
        let (parent, kind) = self.semantic_parent(container);

        match kind {
            AstKind::ExportDefaultDeclaration(_) | AstKind::TSExportAssignment(_) => {
                matches!(self.semantic_parent(parent).1, AstKind::Program(_))
            }
            AstKind::VariableDeclarator(_) => {
                let (list, kind) = self.semantic_parent(parent);
                let (statement, owner) = self.semantic_parent(list);
                let owner = match owner {
                    AstKind::ExportDeclaration(_) => self.semantic_parent(statement).1,
                    owner => owner,
                };
                matches!(kind, AstKind::VariableDeclaration(_))
                    && matches!(owner, AstKind::Program(_))
            }
            AstKind::AssignmentExpression(_) if self.identifier_assignment(container, span) => {
                let (statement, kind) = self.semantic_parent(parent);
                matches!(kind, AstKind::ExpressionStatement(_))
                    && matches!(self.semantic_parent(statement).1, AstKind::Program(_))
            }
            _ => false,
        }
    }

    fn contract(&mut self, key: usize, kind: &'static str, start: u32, name: String, nested: bool) {
        let line = self.scan.line(start);
        self.scan.set(
            Key::Node(key),
            Entity {
                kind,
                line,
                name,
                nested,
            },
        );
    }
}

impl<'a> Visit<'a> for Contracts<'_, '_, 'a> {
    fn visit_class(&mut self, class: &Class<'a>) {
        let node = class.node_id.get();
        let name = class
            .id
            .as_ref()
            .map_or_else(|| "<anonymous>".to_owned(), |id| id.name.to_string());

        if class.is_expression() {
            let (container, span) = self.container(node, class.span);
            let parent = self.semantic_parent(container).1;
            let inline = self.inline
                && class.id.is_none()
                && !matches!(
                    parent,
                    AstKind::VariableDeclarator(_)
                        | AstKind::ExportDefaultDeclaration(_)
                        | AstKind::TSExportAssignment(_)
                )
                && !self.identifier_assignment(container, span);

            if !inline {
                let nested = !self.top_level_class_expression(container, span);
                self.contract(
                    address(class),
                    "ClassExpression",
                    decorated_start(class.span.start, &class.decorators),
                    name,
                    nested,
                );
            }
        } else {
            let start = self.declaration_start(node, class.span.start);
            let nested = self.nested(node);
            self.contract(
                address(class),
                "ClassDeclaration",
                decorated_start(start, &class.decorators),
                name,
                nested,
            );
        }

        walk::walk_class(self, class);
    }

    fn visit_ts_interface_declaration(&mut self, interface: &TSInterfaceDeclaration<'a>) {
        let node = interface.node_id.get();
        let start = self.declaration_start(node, interface.span.start);
        let nested = self.nested(node);
        self.contract(
            address(interface),
            "InterfaceDeclaration",
            start,
            interface.id.name.to_string(),
            nested,
        );
        walk::walk_ts_interface_declaration(self, interface);
    }

    fn visit_ts_type_alias_declaration(&mut self, alias: &TSTypeAliasDeclaration<'a>) {
        let node = alias.node_id.get();
        let start = self.declaration_start(node, alias.span.start);
        let nested = self.nested(node);
        self.contract(
            address(alias),
            "TypeAliasDeclaration",
            start,
            alias.id.name.to_string(),
            nested,
        );
        walk::walk_ts_type_alias_declaration(self, alias);
    }

    fn visit_ts_enum_declaration(&mut self, declaration: &TSEnumDeclaration<'a>) {
        let node = declaration.node_id.get();
        let start = self.declaration_start(node, declaration.span.start);
        let nested = self.nested(node);
        self.contract(
            address(declaration),
            "EnumDeclaration",
            start,
            declaration.id.name.to_string(),
            nested,
        );
        walk::walk_ts_enum_declaration(self, declaration);
    }
}
