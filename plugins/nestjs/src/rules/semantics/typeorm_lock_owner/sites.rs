//! The sites of one file and the first aspect each one breaks.
//!
//! A site is a `lock` property of a read's options (or any `lock` whose value is an owner call),
//! a `setLock` or `setOnLocked` call on a query builder, an owner call that is not a `lock`
//! value, or, outside the owner, a `mode` property that folds to a row-lock mode and stands in no
//! lock site. A lock site is checked in this order: `lock-object`, `lock-value`, `receiver`,
//! `row-list`, `builder`.

use std::collections::{BTreeMap, HashSet};

use gate_typescript::SourceFile;
use gate_typescript::owners::{SourceContext, property_access, unwrap};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    CallExpression, Expression, ObjectExpression, ObjectProperty, PropertyKind, UnaryOperator,
};
use gate_typescript::oxc_semantic::AstNodes;
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::syntax::{key_name, object_property};

use super::reads::{self, Read};
use super::{Context, owner, values};

/// One finding before it is reported.
#[derive(Debug)]
pub(crate) struct Pending {
    pub aspect: &'static str,
    pub path: String,
    pub line: Option<u32>,
    /// The site's offset, which orders two findings of one line.
    pub start: u32,
    pub key: String,
    pub message: String,
}

impl Pending {
    /// A finding about a whole file.
    pub fn file(aspect: &'static str, path: &str, message: String) -> Self {
        Self {
            aspect,
            path: path.to_owned(),
            line: None,
            start: 0,
            key: aspect.to_owned(),
            message,
        }
    }
}

/// A site's finding before its key is known.
struct Site {
    node: NodeId,
    start: u32,
    aspect: &'static str,
    message: String,
}

/// The findings of one file.
pub(crate) fn check<'a>(
    context: &Context<'_, 'a>,
    source: &SourceContext<'a>,
) -> gate_sdk::Result<Vec<Pending>> {
    let file = &*source.file;
    let nodes = file.semantic().nodes();
    let path = source.path.as_str();
    let function = context.function.name.as_str();
    let at = |offset: u32| format!("{path}:{}", file.line(offset));
    let mut sites: Vec<Site> = Vec::new();
    // The `lock` properties already read, whose `mode` objects are no site of their own.
    let mut handled: Vec<Span> = Vec::new();
    // The owner calls that are `lock` values.
    let mut lock_values: HashSet<Span> = HashSet::new();

    for node in nodes.iter() {
        let AstKind::ObjectProperty(property) = node.kind() else {
            continue;
        };

        if !named(property, "lock") {
            continue;
        }

        let object = nodes.parent_id(node.id());
        let read = argument(nodes, object)
            .and_then(|(call, index)| reads::classify(context, source, call, index));
        let branches = branches(&property.value);
        let calls: Vec<&CallExpression<'_>> = branches
            .iter()
            .filter_map(|branch| owner_call(context, file, branch))
            .collect();

        if matches!(read, Some(Read::Ignored)) || (read.is_none() && calls.is_empty()) {
            if read.is_some() {
                handled.push(property.span);
                lock_values.extend(calls.iter().map(|call| call.span));
            }
            continue;
        }

        handled.push(property.span);
        lock_values.extend(calls.iter().map(|call| call.span));

        let modes: Vec<Option<Vec<values::Branch>>> = branches
            .iter()
            .map(|branch| match branch {
                Expression::ObjectExpression(object) => {
                    object_property(object, "mode").map(|mode| values::fold(context, file, mode))
                }
                _ => None,
            })
            .collect();
        let row_free = branches.iter().zip(&modes).all(|(branch, mode)| {
            undefined(branch)
                || mode
                    .as_ref()
                    .is_some_and(|mode| values::row_free(context, mode))
        }) && modes.iter().any(Option::is_some);

        if row_free {
            continue;
        }

        let site = |aspect: &'static str, message: String| Site {
            node: node.id(),
            start: property.span.start,
            aspect,
            message,
        };
        let here = at(property.span.start);

        if modes
            .iter()
            .flatten()
            .any(|mode| values::locks_rows(context, mode))
        {
            sites.push(site(
                "lock-object",
                format!(
                    "{here} builds a row lock object outside {}; take the lock from {function}.",
                    context.owner
                ),
            ));
            continue;
        }

        if branches
            .iter()
            .any(|branch| !undefined(branch) && owner_call(context, file, branch).is_none())
        {
            sites.push(site(
                "lock-value",
                format!("{here} sets lock to a value that is not a call of {function}."),
            ));
            continue;
        }

        let Some(read) = read else {
            sites.push(site(
                "receiver",
                format!(
                    "{here} passes lock in options that are not an argument of a read, so the repository it locks cannot be checked."
                ),
            ));
            continue;
        };

        if let Some(message) = receiver(context, file, &read, &calls, &here) {
            sites.push(site("receiver", message));
            continue;
        }

        if let Some(message) = row_list(file, &read, &calls, &here) {
            sites.push(site("row-list", message));
            continue;
        }

        if matches!(read, Read::Find { builder: true, .. }) && !eager_free(nodes, object) {
            sites.push(site(
                "builder",
                format!("{here} passes lock to setFindOptions without loadEagerRelations: false."),
            ));
        }
    }

    for node in nodes.iter() {
        match node.kind() {
            AstKind::CallExpression(call) => {
                if let Some((object, name)) = property_access(unwrap(&call.callee))
                    && matches!(name, "setLock" | "setOnLocked")
                    && reads::is_builder(context, file, object)
                {
                    let start = member_start(&call.callee);
                    sites.push(Site {
                        node: node.id(),
                        start,
                        aspect: "builder",
                        message: format!(
                            "{} calls {name}; take the lock through setFindOptions with {function}.",
                            at(start)
                        ),
                    });
                } else if path != context.owner
                    && !lock_values.contains(&call.span)
                    && owner::is_call(context, file, call)
                {
                    sites.push(Site {
                        node: node.id(),
                        start: call.span.start,
                        aspect: "lock-value",
                        message: format!(
                            "{} calls {function} outside the lock property of a read.",
                            at(call.span.start)
                        ),
                    });
                }
            }
            AstKind::ObjectProperty(property)
                if path != context.owner
                    && named(property, "mode")
                    && !handled.iter().any(|span| {
                        span.start <= property.span.start && property.span.end <= span.end
                    })
                    && values::locks_rows(
                        context,
                        &values::fold(context, file, &property.value),
                    ) =>
            {
                sites.push(Site {
                    node: node.id(),
                    start: property.span.start,
                    aspect: "lock-object",
                    message: format!(
                        "{} builds a row lock object outside {}; take the lock from {function}.",
                        at(property.span.start),
                        context.owner
                    ),
                });
            }
            _ => {}
        }
    }

    sites.sort_by_key(|site| site.start);
    let mut ordinals: BTreeMap<String, usize> = BTreeMap::new();

    Ok(sites
        .into_iter()
        .map(|site| {
            let label = label(nodes, site.node);
            let ordinal = ordinals.entry(label.clone()).or_default();
            *ordinal += 1;

            Pending {
                aspect: site.aspect,
                path: path.to_owned(),
                line: Some(file.line(site.start)),
                start: site.start,
                key: format!("{label}#{ordinal}"),
                message: site.message,
            }
        })
        .collect())
}

/// Whether an object property is a plain `name: value` (or shorthand) property named `name`.
fn named(property: &ObjectProperty<'_>, name: &str) -> bool {
    !property.method
        && property.kind == PropertyKind::Init
        && key_name(&property.key, property.computed).as_deref() == Some(name)
}

/// The start of the property name of a member callee: the line of `.setLock` in a chain.
fn member_start(callee: &Expression<'_>) -> u32 {
    match unwrap(callee) {
        Expression::StaticMemberExpression(member) => member.property.span.start,
        expression => expression.span().start,
    }
}

/// The node an expression's value stands in, through parentheses, assertions and the branches of
/// conditionals.
fn lift(nodes: &AstNodes<'_>, mut id: NodeId) -> NodeId {
    loop {
        let parent = nodes.parent_id(id);

        if parent == id {
            return id;
        }

        let span = nodes.kind(id).span();

        match nodes.kind(parent) {
            AstKind::ParenthesizedExpression(_)
            | AstKind::TSAsExpression(_)
            | AstKind::TSSatisfiesExpression(_)
            | AstKind::TSNonNullExpression(_)
            | AstKind::TSTypeAssertion(_) => id = parent,
            AstKind::ConditionalExpression(conditional) if conditional.test.span() != span => {
                id = parent;
            }
            _ => return id,
        }
    }
}

/// The call whose argument holds the object `object`, directly or spread into an outer object
/// literal, and the argument's index.
fn argument<'a>(nodes: &AstNodes<'a>, object: NodeId) -> Option<(&'a CallExpression<'a>, usize)> {
    let mut id = object;

    loop {
        id = lift(nodes, id);
        let parent = nodes.parent_id(id);

        match nodes.kind(parent) {
            AstKind::SpreadElement(_) => {
                let outer = nodes.parent_id(parent);

                if !matches!(nodes.kind(outer), AstKind::ObjectExpression(_)) {
                    return None;
                }

                id = outer;
            }
            AstKind::CallExpression(call) => {
                let span = nodes.kind(id).span();

                return call
                    .arguments
                    .iter()
                    .position(|argument| argument.span() == span)
                    .map(|index| (call, index));
            }
            _ => return None,
        }
    }
}

/// Whether an object literal on the way from `object` to the read's argument has
/// `loadEagerRelations: false`.
fn eager_free(nodes: &AstNodes<'_>, object: NodeId) -> bool {
    let mut id = object;

    loop {
        if let AstKind::ObjectExpression(object) = nodes.kind(id)
            && eager_flag(object)
        {
            return true;
        }

        let parent = nodes.parent_id(id);

        if parent == id || matches!(nodes.kind(parent), AstKind::CallExpression(_)) {
            return false;
        }

        id = parent;
    }
}

fn eager_flag(object: &ObjectExpression<'_>) -> bool {
    object_property(object, "loadEagerRelations").is_some_and(
        |value| matches!(unwrap(value), Expression::BooleanLiteral(literal) if !literal.value),
    )
}

/// The branches of a value: itself, or the branches of a conditional.
fn branches<'e, 'a>(expression: &'e Expression<'a>) -> Vec<&'e Expression<'a>> {
    match unwrap(expression) {
        Expression::ConditionalExpression(conditional) => {
            let mut all = branches(&conditional.consequent);
            all.extend(branches(&conditional.alternate));
            all
        }
        value => vec![value],
    }
}

/// Whether a value is `undefined` (the global) or `void 0`.
fn undefined(expression: &Expression<'_>) -> bool {
    match unwrap(expression) {
        Expression::Identifier(identifier) => identifier.name == "undefined",
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::Void,
        _ => false,
    }
}

/// The owner call a branch is.
fn owner_call<'e, 'a>(
    context: &Context<'_, '_>,
    file: &SourceFile<'_>,
    expression: &'e Expression<'a>,
) -> Option<&'e CallExpression<'a>> {
    match unwrap(expression) {
        Expression::CallExpression(call) if owner::is_call(context, file, call) => Some(call),
        _ => None,
    }
}

/// The `receiver` finding of the own form: a first argument that is not the read's repository.
fn receiver(
    context: &Context<'_, '_>,
    file: &SourceFile<'_>,
    read: &Read<'_, '_>,
    calls: &[&CallExpression<'_>],
    here: &str,
) -> Option<String> {
    for call in calls {
        let argument = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression());

        if argument
            .is_some_and(|argument| matches!(unwrap(argument), Expression::ArrayExpression(_)))
        {
            continue;
        }

        let text = argument.map_or_else(
            || "no repository".to_owned(),
            |argument| reads::compact(file.text(argument.span())),
        );

        match read {
            Read::Find {
                repository: None, ..
            } => {
                return Some(format!(
                    "{here} locks a read whose repository cannot be named; call the read on a repository, as manager.<read>(<Entity>, …) or on a chain from createQueryBuilder."
                ));
            }
            Read::Find {
                repository: Some(repository),
                ..
            } => {
                if !argument.is_some_and(|argument| reads::matches(file, argument, *repository)) {
                    return Some(format!(
                        "{here} locks through {text}, but the read runs on {}.",
                        reads::describe(file, *repository)
                    ));
                }
            }
            Read::Service(entity) => {
                if !argument.is_some_and(|argument| {
                    reads::step_context(context, argument, entity.as_deref())
                }) {
                    let step = context.step_context_method;

                    return Some(match entity {
                        Some(entity) => format!(
                            "{here} locks a read of a service through {text}; pass this.{step}<{entity}>().getRepository({entity}) with the service's entity {entity}."
                        ),
                        None => format!(
                            "{here} locks a read of a service through {text}; pass this.{step}<Entity>().getRepository(Entity) with one entity as both the type argument and the argument."
                        ),
                    });
                }
            }
            Read::Ignored => {}
        }
    }

    None
}

/// The `row-list` finding of a list of repositories: outside a builder, empty, or repeating one.
fn row_list(
    file: &SourceFile<'_>,
    read: &Read<'_, '_>,
    calls: &[&CallExpression<'_>],
    here: &str,
) -> Option<String> {
    for call in calls {
        let Some(Expression::ArrayExpression(list)) = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
            .map(unwrap)
        else {
            continue;
        };

        if !matches!(read, Read::Find { builder: true, .. }) {
            return Some(format!(
                "{here} passes a list of repositories outside a query builder's setFindOptions; only a builder locks joined rows."
            ));
        }

        let elements: Vec<&Expression<'_>> = list
            .elements
            .iter()
            .filter_map(|element| element.as_expression())
            .collect();

        if list.elements.is_empty() {
            return Some(format!("{here} passes an empty list of locked rows."));
        }

        for (index, element) in elements.iter().enumerate() {
            if elements[..index]
                .iter()
                .any(|earlier| reads::same(file, earlier, element))
            {
                return Some(format!(
                    "{here} names {} more than once in its list of locked rows.",
                    reads::compact(file.text(element.span()))
                ));
            }
        }
    }

    None
}

/// The key label of a site: the innermost named function, method or class property around it, as
/// `Class.member` or `function`; `module` at the top level.
fn label(nodes: &AstNodes<'_>, node: NodeId) -> String {
    let class_name = |id: NodeId| {
        nodes.ancestor_kinds(id).find_map(|kind| match kind {
            AstKind::Class(class) => Some(
                class
                    .id
                    .as_ref()
                    .map_or_else(|| "<anonymous>".to_owned(), |id| id.name.to_string()),
            ),
            _ => None,
        })
    };

    for id in nodes.ancestor_ids(node) {
        match nodes.kind(id) {
            AstKind::MethodDefinition(method) => {
                if let Some(name) = key_name(&method.key, method.computed) {
                    return match class_name(id) {
                        Some(class) => format!("{class}.{name}"),
                        None => name,
                    };
                }
            }
            AstKind::PropertyDefinition(property) => {
                if let Some(name) = key_name(&property.key, property.computed) {
                    return match class_name(id) {
                        Some(class) => format!("{class}.{name}"),
                        None => name,
                    };
                }
            }
            AstKind::Function(function) => {
                if let Some(id) = &function.id {
                    return id.name.to_string();
                }
            }
            AstKind::VariableDeclarator(declarator)
                if declarator.init.as_ref().is_some_and(|init| {
                    matches!(
                        unwrap(init),
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    )
                }) =>
            {
                if let Some(name) = declarator.id.get_identifier_name() {
                    return name.to_string();
                }
            }
            _ => {}
        }
    }

    "module".to_owned()
}
