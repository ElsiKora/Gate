//! `nestjs/api-readiness`: the API role becomes ready only after its readiness owner and its
//! listener, in that order.
//!
//! The readiness owner files must be tracked, and the entrypoint function of the startup file must
//! await `<application>.<get>(<readiness-provider>).<assert-ready>(...)`, then await
//! `<application>.<listen>(...)`, then call `<lifecycle>.<mark-ready>()`, exactly once each and in
//! that order, in one unconditionally executed block of the entrypoint or of one local `const`
//! async operation that the entrypoint awaits exactly once after declaring it. A referenced
//! block-bodied arrow with a lifecycle call that is the direct initializer (no parentheses, `as`,
//! `satisfies` or other assertions) of a declarator in an unconditional top-level statement, but
//! not such an operation, fails the rule. A startup file without a syntax tree is skipped:
//! `typescript/parse` reports it.

use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, ArrowFunctionBody, ArrowFunctionExpression, BindingPattern, CallExpression,
    ChainElement, Class, Declaration, ExportDefaultDeclarationKind, Expression, Function,
    ObjectProperty, PropertyKind, Statement, VariableDeclarationKind,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::oxc_semantic::AstNodes;
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::oxc_syntax::scope::ScopeFlags;

use crate::shared::project;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/api-readiness`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The files of the readiness owner, each of which must be tracked.
    pub readiness_files: Vec<String>,
    /// The file that declares the entrypoint.
    pub startup_file: String,
    /// The startup function.
    pub entrypoint: String,
    /// The application variable the readiness provider and the listener are read from.
    pub application: String,
    /// The readiness provider class the application resolves.
    pub readiness_provider: String,
    /// The lifecycle variable that marks the role ready.
    pub lifecycle: String,
    /// The method names of the lifecycle calls.
    pub calls: Calls,
    /// The name of the role the messages print.
    pub role_name: String,
}

/// The method names of the lifecycle calls.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Calls {
    /// The application method that resolves the readiness provider.
    pub get: String,
    /// The readiness provider method that asserts readiness.
    pub assert_ready: String,
    /// The application method that starts the listener.
    pub listen: String,
    /// The lifecycle method that marks the role ready.
    pub mark_ready: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            readiness_files: [
                "src/runtime/api/readiness/index.ts",
                "src/runtime/api/readiness/module.ts",
                "src/runtime/api/readiness/service.ts",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            startup_file: "src/runtime/api/function/start.function.ts".to_owned(),
            entrypoint: "runtimeApiStart".to_owned(),
            application: "roleApplication".to_owned(),
            readiness_provider: "RuntimeApiReadinessService".to_owned(),
            lifecycle: "roleLifecycle".to_owned(),
            calls: Calls::default(),
            role_name: "Runtime API".to_owned(),
        }
    }
}

impl Default for Calls {
    fn default() -> Self {
        Self {
            get: "get".to_owned(),
            assert_ready: "assertReady".to_owned(),
            listen: "listen".to_owned(),
            mark_ready: "markReady".to_owned(),
        }
    }
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "api-readiness";
    const TITLE: &'static str =
        "The API role becomes ready after its readiness owner and its listener";
    const ASPECTS: &'static [&'static str] = &["readiness-owner", "startup-order"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
        {
            use crate::shared::paths::Form::File;
            let o = run.options();
            crate::shared::paths::check_all("readiness-files", &o.readiness_files, File)?;
            crate::shared::paths::check("startup-file", &o.startup_file, File)?;
            gate_typescript::check_source_option("startup-file", &o.startup_file)?;
        }

        let role = run.options().role_name.clone();
        let tree = run.tree();
        let missing: Vec<String> = run
            .options()
            .readiness_files
            .iter()
            .filter(|path| !tree.contains(path))
            .cloned()
            .collect();

        for path in missing {
            run.report()
                .finding(
                    "readiness-owner",
                    Location::file(&path),
                    format!("{path} is required for the {role} readiness owner"),
                )
                .help("add the readiness owner file")
                .key("missing");
        }

        let start = run.options().startup_file.clone();
        let entrypoint = run.options().entrypoint.clone();
        let names = Names::new(run.options());
        let typescript = shared.typescript()?;

        let Some(file) = typescript.parse(&start)? else {
            run.report()
                .finding(
                    "startup-order",
                    Location::file(&start),
                    format!("{start} is required for {role} lifecycle governance"),
                )
                .help("add the startup file")
                .key("missing-file");
            return Ok(());
        };

        // A startup file without a syntax tree is reported once, by `typescript/parse`.
        if file.fatal() {
            return Ok(());
        }

        let Some((offset, body)) = entry(&file, &entrypoint) else {
            run.report()
                .finding(
                    "startup-order",
                    Location::file(&start),
                    format!("{start} must declare {entrypoint} with an executable body"),
                )
                .help(format!("declare the function {entrypoint} with a body"))
                .key("missing-entrypoint");
            return Ok(());
        };

        if !names.ordered(&file, body) {
            let line = usize::try_from(file.line(offset)).unwrap_or(usize::MAX);
            let Names {
                application,
                provider,
                lifecycle,
                get,
                assert_ready,
                listen,
                mark_ready,
            } = &names;

            run.report()
                .finding(
                    "startup-order",
                    Location::line(&start, line),
                    format!(
                        "{start} {entrypoint} must await {application}.{get}({provider}).{assert_ready}, then await {application}.{listen}, then call {lifecycle}.{mark_ready} in that order in the entrypoint or one locally bound, once-awaited startup operation"
                    ),
                )
                .help(format!(
                    "await {assert_ready}, then await {listen}, then call {mark_ready}, once each and unconditionally"
                ))
                .key("order");
        }

        Ok(())
        })
    }
}

/// The start offset and body of the first top-level function declaration named `name`; the body
/// is `None` for an overload or `declare` signature.
fn entry<'f, 'a>(file: &'f SourceFile<'a>, name: &str) -> Option<(u32, &'f [Statement<'a>])> {
    file.program()
        .body
        .iter()
        .find_map(|statement| {
            let (start, function) = match statement {
                Statement::FunctionDeclaration(function) => {
                    (function.span.start, function.as_ref())
                }
                Statement::ExportDeclaration(export) => match &export.declaration {
                    Declaration::FunctionDeclaration(function) => {
                        (export.span.start, function.as_ref())
                    }
                    _ => return None,
                },
                Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        (export.span.start, function.as_ref())
                    }
                    _ => return None,
                },
                _ => return None,
            };

            (function.id.as_ref().map(|id| id.name.as_str()) == Some(name))
                .then_some((start, function))
        })
        .and_then(|(start, function): (u32, &Function<'a>)| {
            function
                .body
                .as_ref()
                .map(|body| (start, body.statements.as_slice()))
        })
}

/// How a statement leaves its block unconditionally.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Exit<'s> {
    Return,
    Throw,
    Break(Option<&'s str>),
}

/// How a statement leaves its block unconditionally, if it does.
fn exit<'s>(statement: &'s Statement<'_>) -> Option<Exit<'s>> {
    match statement {
        Statement::ReturnStatement(_) => Some(Exit::Return),
        Statement::ThrowStatement(_) => Some(Exit::Throw),
        Statement::BreakStatement(statement) => Some(Exit::Break(
            statement.label.as_ref().map(|label| label.name.as_str()),
        )),
        Statement::LabeledStatement(labeled) => match exit(&labeled.body) {
            Some(Exit::Break(Some(label))) if label == labeled.label.name.as_str() => None,
            other => other,
        },
        Statement::BlockStatement(block) => block.body.iter().find_map(exit),
        Statement::TryStatement(statement) => {
            if let Some(exit) = statement
                .finalizer
                .as_ref()
                .and_then(|finalizer| finalizer.body.iter().find_map(exit))
            {
                return Some(exit);
            }

            let tried = statement.block.body.iter().find_map(exit);

            match (tried, &statement.handler) {
                (Some(Exit::Throw), Some(handler)) => handler.body.body.iter().find_map(exit),
                _ => tried,
            }
        }
        _ => None,
    }
}

/// The statements of a block up to and including the first one that leaves it unconditionally.
fn unconditional<'s, 'a>(block: &'s [Statement<'a>]) -> Vec<&'s Statement<'a>> {
    let mut statements = Vec::new();

    for statement in block {
        statements.push(statement);

        if exit(statement).is_some() {
            break;
        }
    }

    statements
}

/// A block and the nested blocks and `try` blocks it always enters.
fn unconditional_blocks<'s, 'a>(block: &'s [Statement<'a>]) -> Vec<&'s [Statement<'a>]> {
    let mut blocks = vec![block];

    for statement in unconditional(block) {
        match statement {
            Statement::BlockStatement(inner) => blocks.extend(unconditional_blocks(&inner.body)),
            Statement::TryStatement(inner) => {
                blocks.extend(unconditional_blocks(&inner.block.body));
            }
            _ => {}
        }
    }

    blocks
}

/// One lifecycle call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Lifecycle {
    AssertReady,
    Listen,
    MarkReady,
}

/// The object and property of a static member expression.
fn member<'e, 'a>(expression: &'e Expression<'a>) -> Option<(&'e Expression<'a>, &'e str)> {
    match expression {
        Expression::StaticMemberExpression(member) => {
            Some((&member.object, member.property.name.as_str()))
        }
        _ => None,
    }
}

/// Whether the expression is the identifier `name`.
fn named(expression: &Expression<'_>, name: &str) -> bool {
    matches!(expression, Expression::Identifier(identifier) if identifier.name == name)
}

/// The awaited call of `await call(...);`.
fn awaited<'s, 'a>(statement: &'s Statement<'a>) -> Option<&'s CallExpression<'a>> {
    let Statement::ExpressionStatement(statement) = statement else {
        return None;
    };
    let Expression::AwaitExpression(awaited) = &statement.expression else {
        return None;
    };

    call(&awaited.argument)
}

/// The call an expression is, also inside an optional chain: TypeScript keeps `a?.b()` a call.
fn call<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e CallExpression<'a>> {
    match expression {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// The names the lifecycle calls are made of.
struct Names {
    application: String,
    provider: String,
    lifecycle: String,
    get: String,
    assert_ready: String,
    listen: String,
    mark_ready: String,
}

impl Names {
    fn new(options: &Options) -> Self {
        Self {
            application: options.application.clone(),
            provider: options.readiness_provider.clone(),
            lifecycle: options.lifecycle.clone(),
            get: options.calls.get.clone(),
            assert_ready: options.calls.assert_ready.clone(),
            listen: options.calls.listen.clone(),
            mark_ready: options.calls.mark_ready.clone(),
        }
    }

    /// `<application>.<get>(<provider>).<assert-ready>(...)`.
    fn readiness(&self, call: &CallExpression<'_>) -> bool {
        let Some((Expression::CallExpression(provider), method)) = member(&call.callee) else {
            return false;
        };
        let Some((object, get)) = member(&provider.callee) else {
            return false;
        };

        method == self.assert_ready
            && get == self.get
            && named(object, &self.application)
            && matches!(
                provider.arguments.as_slice(),
                [Argument::Identifier(identifier)] if identifier.name == self.provider.as_str()
            )
    }

    /// `<application>.<listen>(...)`.
    fn listener(&self, call: &CallExpression<'_>) -> bool {
        matches!(member(&call.callee), Some((object, listen)) if listen == self.listen && named(object, &self.application))
    }

    /// `<lifecycle>.<mark-ready>()`.
    fn mark_ready(&self, call: &CallExpression<'_>) -> bool {
        call.arguments.is_empty()
            && matches!(member(&call.callee), Some((object, mark)) if mark == self.mark_ready && named(object, &self.lifecycle))
    }

    /// Every lifecycle call of a block outside nested functions and classes; a readiness call is
    /// not searched further.
    fn lifecycle_calls(&self, block: &[Statement<'_>]) -> Vec<Lifecycle> {
        let mut calls = CallFinder {
            names: self,
            found: Vec::new(),
        };

        for statement in block {
            calls.visit_statement(statement);
        }

        calls.found
    }

    /// Whether the entrypoint runs the three lifecycle calls once each, in order, in one
    /// unconditional block.
    fn ordered<'a>(&self, file: &SourceFile<'a>, body: &[Statement<'a>]) -> bool {
        let Some(scopes) = self.execution_scopes(file, body) else {
            return false;
        };
        let total: usize = scopes
            .iter()
            .map(|scope| self.lifecycle_calls(scope).len())
            .sum();
        let groups: Vec<Vec<Lifecycle>> = scopes
            .iter()
            .flat_map(|scope| unconditional_blocks(scope))
            .map(|block| {
                unconditional(block)
                    .into_iter()
                    .filter_map(|statement| self.statement_call(statement))
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
            .collect();

        total == 3
            && groups
                == [vec![
                    Lifecycle::AssertReady,
                    Lifecycle::Listen,
                    Lifecycle::MarkReady,
                ]]
    }

    /// The lifecycle call a statement makes: an awaited readiness or listener call, or a plain
    /// ready mark.
    fn statement_call(&self, statement: &Statement<'_>) -> Option<Lifecycle> {
        if let Some(call) = awaited(statement) {
            if self.readiness(call) {
                return Some(Lifecycle::AssertReady);
            }

            if self.listener(call) {
                return Some(Lifecycle::Listen);
            }
        }

        match statement {
            Statement::ExpressionStatement(statement) => call(&statement.expression)
                .filter(|call| self.mark_ready(call))
                .map(|_| Lifecycle::MarkReady),
            _ => None,
        }
    }

    /// The entrypoint body and the body of every local startup operation; `None` when an
    /// operation with lifecycle calls is not a `const` async arrow without parameters that is
    /// declared before the one unconditional statement that awaits it.
    fn execution_scopes<'s, 'a>(
        &self,
        file: &SourceFile<'a>,
        body: &'s [Statement<'a>],
    ) -> Option<Vec<&'s [Statement<'a>]>> {
        let mut scopes = vec![body];
        let invocations: HashSet<Span> = unconditional_blocks(body)
            .into_iter()
            .flat_map(unconditional)
            .map(GetSpan::span)
            .collect();
        let body_span = Span::new(
            body.first().map_or(0, |statement| statement.span().start),
            body.last().map_or(0, |statement| statement.span().end),
        );
        let semantic = file.semantic();
        let scoping = semantic.scoping();
        let nodes = semantic.nodes();

        for statement in unconditional(body) {
            let Statement::VariableDeclaration(list) = statement else {
                continue;
            };

            for declarator in &list.declarations {
                let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
                    continue;
                };
                let Some(Expression::ArrowFunctionExpression(operation)) = &declarator.init else {
                    continue;
                };
                let ArrowFunctionBody::FunctionBody(operation_body) = &operation.body else {
                    continue;
                };

                if self.lifecycle_calls(&operation_body.statements).is_empty() {
                    continue;
                }

                let symbol = binding.symbol_id.get()?;
                let mut references: Vec<_> = scoping
                    .get_resolved_references(symbol)
                    // oxc resolves a `typeof` reference in a type to a type reference.
                    .filter(|reference| !reference.flags().is_type())
                    .map(|reference| nodes.get_node(reference.node_id()))
                    .filter(|node| {
                        let span = node.span();
                        span.start >= body_span.start
                            && span.end <= body_span.end
                            && !heritage_or_instantiation(nodes, node.id())
                    })
                    .collect();

                if references.is_empty() {
                    continue;
                }

                references.sort_by_key(|node| node.span().start);
                let reference = references[0];
                let call = match nodes.parent_kind(reference.id()) {
                    AstKind::CallExpression(call) if call.callee.span() == reference.span() => call,
                    _ => return None,
                };
                let mut awaited = nodes.parent_id(nodes.parent_id(reference.id()));

                // `await startup?.()` wraps the call in an optional chain.
                if matches!(nodes.kind(awaited), AstKind::ChainExpression(_)) {
                    awaited = nodes.parent_id(awaited);
                }

                let AstKind::AwaitExpression(_) = nodes.kind(awaited) else {
                    return None;
                };
                let AstKind::ExpressionStatement(invocation) = nodes.parent_kind(awaited) else {
                    return None;
                };

                if list.kind != VariableDeclarationKind::Const
                    || !operation.r#async
                    || !operation.params.items.is_empty()
                    || operation.params.rest.is_some()
                    || references.len() != 1
                    || !call.arguments.is_empty()
                    || invocation.expression.span() != nodes.kind(awaited).span()
                    || !invocations.contains(&invocation.span)
                    || declarator.span.end > call.span.start
                {
                    return None;
                }

                scopes.push(&operation_body.statements);
            }
        }

        Some(scopes)
    }
}

/// Collects the lifecycle calls of a block outside nested functions and classes.
struct CallFinder<'n> {
    names: &'n Names,
    found: Vec<Lifecycle>,
}

impl<'a> Visit<'a> for CallFinder<'_> {
    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_arrow_function_expression(&mut self, _: &ArrowFunctionExpression<'a>) {}

    fn visit_class(&mut self, _: &Class<'a>) {}

    // TypeScript skips an object method or accessor as a whole, its computed name included.
    fn visit_object_property(&mut self, property: &ObjectProperty<'a>) {
        if !property.method && property.kind == PropertyKind::Init {
            walk::walk_object_property(self, property);
        }
    }

    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if self.names.readiness(call) {
            self.found.push(Lifecycle::AssertReady);
            return;
        }

        if self.names.listener(call) {
            self.found.push(Lifecycle::Listen);
        } else if self.names.mark_ready(call) {
            self.found.push(Lifecycle::MarkReady);
        }

        walk::walk_call_expression(self, call);
    }
}

/// Whether a reference lies in a class heritage expression or an instantiation expression such
/// as `startup<number>`: the TypeScript compiler reads both as a type node, so neither counts as a
/// reference.
fn heritage_or_instantiation(nodes: &AstNodes<'_>, node: NodeId) -> bool {
    let span = nodes.get_node(node).span();

    nodes
        .ancestor_ids(node)
        .any(|ancestor| match nodes.kind(ancestor) {
            AstKind::TSInstantiationExpression(_) => true,
            AstKind::Class(class) => class.heritage.as_ref().is_some_and(|heritage| {
                let heritage = heritage.expression.span();
                heritage.start <= span.start && span.end <= heritage.end
            }),
            _ => false,
        })
}
