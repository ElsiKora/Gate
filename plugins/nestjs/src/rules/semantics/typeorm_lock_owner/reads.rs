//! Reads and their repositories: which call a find-options object is an argument of, the
//! repository that read runs on, query builder chains, and the comparison of a lock's repository
//! argument with the read's, by syntax tree.

use gate_typescript::SourceFile;
use gate_typescript::owners::{
    SourceContext, call_expression, find_class, property_access, unwrap,
};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, CallExpression, ChainElement, Class, ClassElement, Expression, TSType, TSTypeName,
};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::syntax::{constructor, key_name, parameter_type};

use super::Context;
use super::owner;
use super::values::{Binding, binding};

/// How many `const` bindings a builder chain is followed through.
const CHAIN_HOPS: usize = 1;

/// What a read runs on.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Repository<'e, 'a> {
    /// A repository expression `R`.
    Expression(&'e Expression<'a>),
    /// `M.getRepository(E)` of an entity manager `M` and an entity `E`.
    Manager {
        manager: &'e Expression<'a>,
        entity: &'e Expression<'a>,
    },
}

/// A recognised read.
#[derive(Clone, Debug)]
pub(crate) enum Read<'e, 'a> {
    /// A repository or entity-manager read, or a builder's `setFindOptions` (`builder`); `None`
    /// when the repository cannot be named.
    Find {
        repository: Option<Repository<'e, 'a>>,
        builder: bool,
    },
    /// A service read; the service's entity when it is known.
    Service(Option<String>),
    /// A service method called on `this` in a class that is no service: nothing is checked.
    Ignored,
}

/// The read a call is, when the find-options object is its argument at `index`.
pub(crate) fn classify<'e, 'a>(
    context: &Context<'_, 'a>,
    source: &SourceContext<'a>,
    call: &'e CallExpression<'a>,
    index: usize,
) -> Option<Read<'e, 'a>> {
    let (object, name) = property_access(unwrap(&call.callee))?;
    let object = unwrap(object);

    if name == "setFindOptions" {
        return (index == 0).then(|| Read::Find {
            repository: root(&source.file, object, 0),
            builder: true,
        });
    }

    let service = index == 0
        && context
            .service_read_methods
            .iter()
            .any(|method| method == name);

    if service {
        match object {
            Expression::ThisExpression(_) => {
                match enclosing_class(&source.file, call.span)
                    .and_then(|class| service_entity(context, source, class))
                    .map(|base| base.entity)
                {
                    Some(entity) => return Some(Read::Service(entity)),
                    None if !context.read_methods.iter().any(|method| method == name) => {
                        return Some(Read::Ignored);
                    }
                    None => {}
                }
            }
            Expression::StaticMemberExpression(member)
                if matches!(unwrap(&member.object), Expression::ThisExpression(_)) =>
            {
                let entity = enclosing_class(&source.file, call.span)
                    .and_then(|class| member_entity(context, source, class, &member.property.name));

                return Some(Read::Service(entity));
            }
            _ => {}
        }
    }

    if !context.read_methods.iter().any(|method| method == name) {
        return None;
    }

    match index {
        0 => Some(Read::Find {
            repository: Some(Repository::Expression(object)),
            builder: false,
        }),
        1 => Some(Read::Find {
            repository: call
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .map(unwrap)
                .filter(|entity| matches!(entity, Expression::Identifier(_)))
                .map(|entity| Repository::Manager {
                    manager: object,
                    entity,
                }),
            builder: false,
        }),
        _ => None,
    }
}

/// The innermost class whose body holds `span`.
fn enclosing_class<'a>(file: &SourceFile<'a>, span: Span) -> Option<&'a Class<'a>> {
    file.semantic()
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::Class(class)
                if class.span.start <= span.start && span.end <= class.span.end =>
            {
                Some(class)
            }
            _ => None,
        })
        .min_by_key(|class| class.span.end - class.span.start)
}

/// A class that directly extends the service base of the framework package.
struct ServiceBase {
    /// The base's type argument, when it is one identifier.
    entity: Option<String>,
}

/// The service base a class directly extends, if it extends the one of the framework package.
fn service_entity(
    context: &Context<'_, '_>,
    source: &SourceContext<'_>,
    class: &Class<'_>,
) -> Option<ServiceBase> {
    let heritage = class.heritage.as_ref()?;
    let identity = source.identity(&heritage.expression, true)?;

    if identity.module.as_deref() != Some(context.framework_package.as_str())
        || identity.name != context.service_base
    {
        return None;
    }

    Some(ServiceBase {
        entity: heritage.type_arguments.as_ref().and_then(|arguments| {
            match arguments.params.as_slice() {
                [TSType::TSTypeReference(reference)] => match &reference.type_name {
                    TSTypeName::IdentifierReference(name) => Some(name.name.to_string()),
                    _ => None,
                },
                _ => None,
            }
        }),
    })
}

/// The entity of the service a class member holds: a constructor parameter property or a class
/// property whose written type names a class that directly extends the service base.
fn member_entity(
    context: &Context<'_, '_>,
    source: &SourceContext<'_>,
    class: &Class<'_>,
    member: &str,
) -> Option<String> {
    let mut annotation = None;

    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) if constructor(method) => {
                for parameter in &method.value.params.items {
                    let property = parameter.accessibility.is_some()
                        || parameter.readonly
                        || parameter.r#override;

                    if property
                        && parameter
                            .pattern
                            .get_identifier_name()
                            .is_some_and(|name| name.as_str() == member)
                    {
                        annotation = parameter_type(parameter);
                    }
                }
            }
            ClassElement::PropertyDefinition(property)
                if key_name(&property.key, property.computed).as_deref() == Some(member) =>
            {
                annotation = property
                    .type_annotation
                    .as_ref()
                    .map(|annotation| &annotation.type_annotation);
            }
            _ => {}
        }
    }

    let TSType::TSTypeReference(reference) = annotation? else {
        return None;
    };
    let identity = source.type_identity(&reference.type_name)?;
    let sources = context
        .roots
        .iter()
        .find(|sources| sources.get(&source.path).is_some())?;
    let declared = sources.canonical(Some(&identity))?;
    let declared_source = context.source(declared.path.as_deref()?)?;
    let declared_class = find_class(&declared_source.file, &declared.name)?;

    service_entity(context, declared_source, declared_class)?.entity
}

/// The repository of a query builder chain: `R` for `R.createQueryBuilder(alias)` and
/// `M.getRepository(E)` for `M.createQueryBuilder(E, alias)`, through the chain's calls and
/// `CHAIN_HOPS` `const` bindings.
pub(crate) fn root<'e, 'a>(
    file: &SourceFile<'a>,
    expression: &'e Expression<'a>,
    hops: usize,
) -> Option<Repository<'e, 'a>> {
    let expression = unwrap(expression);

    if let Some(call) = call_expression(expression) {
        let (object, name) = property_access(unwrap(&call.callee))?;

        if name != "createQueryBuilder" {
            return root(file, object, hops);
        }

        let first = call.arguments.first().and_then(Argument::as_expression);

        return Some(match first.map(unwrap) {
            Some(entity)
                if !matches!(
                    entity,
                    Expression::StringLiteral(_) | Expression::TemplateLiteral(_)
                ) =>
            {
                Repository::Manager {
                    manager: unwrap(object),
                    entity,
                }
            }
            _ => Repository::Expression(unwrap(object)),
        });
    }

    let Expression::Identifier(identifier) = expression else {
        return None;
    };

    match binding(file, identifier)? {
        Binding::Variable {
            constant: true,
            init: Some(init),
            ..
        } if hops < CHAIN_HOPS => root(file, init, hops + 1),
        _ => None,
    }
}

/// Whether `expression` is a query builder: a chain from `createQueryBuilder`, or an identifier
/// (at the chain's base) whose written type is a query builder type of the `TypeORM` package.
pub(crate) fn is_builder<'a>(
    context: &Context<'_, 'a>,
    file: &SourceFile<'a>,
    expression: &Expression<'a>,
) -> bool {
    if root(file, expression, 0).is_some() {
        return true;
    }

    let mut base = unwrap(expression);

    while let Some(call) = call_expression(base) {
        let Some((object, _)) = property_access(unwrap(&call.callee)) else {
            return false;
        };
        base = unwrap(object);
    }

    let Expression::Identifier(identifier) = base else {
        return false;
    };
    let Some(
        Binding::Variable {
            annotation: Some(annotation),
            ..
        }
        | Binding::Parameter(Some(annotation)),
    ) = binding(file, identifier)
    else {
        return false;
    };
    let TSType::TSTypeReference(reference) = &annotation.type_annotation else {
        return false;
    };
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return false;
    };

    owner::import(file, name, false).is_some_and(|(module, imported)| {
        module == context.typeorm_package && imported.ends_with("QueryBuilder")
    })
}

/// Whether two expressions are the same by syntax tree: identifiers bound to the same symbol,
/// `this`, member accesses, calls with their type arguments and arguments, and literals. An
/// identifier bound by one `const` is also the same as its initializer.
pub(crate) fn same(file: &SourceFile<'_>, left: &Expression<'_>, right: &Expression<'_>) -> bool {
    same_at(file, left, right, 0)
}

fn same_at(
    file: &SourceFile<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    depth: usize,
) -> bool {
    let left = unwrap(left);
    let right = unwrap(right);

    if depth > 8 {
        return false;
    }

    if structural(file, left, right, depth) {
        return true;
    }

    let initializer = |expression: &Expression<'_>| match expression {
        Expression::Identifier(identifier) => match binding(file, identifier) {
            Some(Binding::Variable {
                constant: true,
                init: Some(init),
                ..
            }) => Some(init),
            _ => None,
        },
        _ => None,
    };

    initializer(left).is_some_and(|init| same_at(file, init, right, depth + 1))
        || initializer(right).is_some_and(|init| same_at(file, left, init, depth + 1))
}

fn structural(
    file: &SourceFile<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    depth: usize,
) -> bool {
    let next =
        |left: &Expression<'_>, right: &Expression<'_>| same_at(file, left, right, depth + 1);

    match (chain_free(left), chain_free(right)) {
        (Node::Identifier(left), Node::Identifier(right)) => {
            let symbol = |identifier: &gate_typescript::oxc_ast::ast::IdentifierReference<'_>| {
                identifier.reference_id.get().and_then(|reference| {
                    file.semantic()
                        .scoping()
                        .get_reference(reference)
                        .symbol_id()
                })
            };

            left.name == right.name && symbol(left) == symbol(right)
        }
        (Node::This, Node::This) => true,
        (Node::Member(left_object, left_name), Node::Member(right_object, right_name)) => {
            left_name == right_name && next(left_object, right_object)
        }
        (Node::Computed(left_object, left_key), Node::Computed(right_object, right_key)) => {
            next(left_object, right_object) && next(left_key, right_key)
        }
        (Node::Call(left), Node::Call(right)) => {
            let types = |call: &CallExpression<'_>| {
                call.type_arguments.as_ref().map(|arguments| {
                    arguments
                        .params
                        .iter()
                        .map(|kind| compact(file.text(kind.span())))
                        .collect::<Vec<_>>()
                })
            };

            next(&left.callee, &right.callee)
                && types(left) == types(right)
                && left.arguments.len() == right.arguments.len()
                && left
                    .arguments
                    .iter()
                    .zip(&right.arguments)
                    .all(
                        |(left, right)| match (left.as_expression(), right.as_expression()) {
                            (Some(left), Some(right)) => next(left, right),
                            _ => false,
                        },
                    )
        }
        (Node::Literal(left), Node::Literal(right)) => left == right,
        _ => false,
    }
}

/// One node of an expression as the comparison sees it, an optional chain's element included.
enum Node<'e, 'a> {
    Identifier(&'e gate_typescript::oxc_ast::ast::IdentifierReference<'a>),
    This,
    Member(&'e Expression<'a>, &'e str),
    Computed(&'e Expression<'a>, &'e Expression<'a>),
    Call(&'e CallExpression<'a>),
    Literal(String),
    Other,
}

fn chain_free<'e, 'a>(expression: &'e Expression<'a>) -> Node<'e, 'a> {
    match expression {
        Expression::Identifier(identifier) => Node::Identifier(identifier),
        Expression::ThisExpression(_) => Node::This,
        Expression::StaticMemberExpression(member) => {
            Node::Member(&member.object, member.property.name.as_str())
        }
        Expression::ComputedMemberExpression(member) => {
            Node::Computed(&member.object, &member.expression)
        }
        Expression::CallExpression(call) => Node::Call(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Node::Call(call),
            ChainElement::StaticMemberExpression(member) => {
                Node::Member(&member.object, member.property.name.as_str())
            }
            ChainElement::ComputedMemberExpression(member) => {
                Node::Computed(&member.object, &member.expression)
            }
            _ => Node::Other,
        },
        Expression::StringLiteral(literal) => Node::Literal(format!("s{}", literal.value)),
        Expression::NumericLiteral(literal) => Node::Literal(format!("n{}", literal.value)),
        Expression::BooleanLiteral(literal) => Node::Literal(format!("b{}", literal.value)),
        Expression::NullLiteral(_) => Node::Literal("null".to_owned()),
        _ => Node::Other,
    }
}

/// Whether `argument` is `expected`: the same expression, or for an entity manager's repository
/// `M.getRepository(E)` with the same `M` and `E` (also through one `const`).
pub(crate) fn matches(
    file: &SourceFile<'_>,
    argument: &Expression<'_>,
    expected: Repository<'_, '_>,
) -> bool {
    match expected {
        Repository::Expression(expression) => same(file, argument, expression),
        Repository::Manager { manager, entity } => {
            let get_repository = |expression: &Expression<'_>| {
                call_expression(unwrap(expression)).is_some_and(|call| {
                    property_access(unwrap(&call.callee)).is_some_and(|(object, name)| {
                        name == "getRepository"
                            && same(file, object, manager)
                            && match call.arguments.as_slice() {
                                [only] => only
                                    .as_expression()
                                    .is_some_and(|only| same(file, only, entity)),
                                _ => false,
                            }
                    })
                })
            };

            get_repository(argument)
                || match unwrap(argument) {
                    Expression::Identifier(identifier) => match binding(file, identifier) {
                        Some(Binding::Variable {
                            constant: true,
                            init: Some(init),
                            ..
                        }) => get_repository(init),
                        _ => false,
                    },
                    _ => false,
                }
        }
    }
}

/// The text a message prints for a repository.
pub(crate) fn describe(file: &SourceFile<'_>, repository: Repository<'_, '_>) -> String {
    match repository {
        Repository::Expression(expression) => compact(file.text(expression.span())),
        Repository::Manager { manager, entity } => format!(
            "{}.getRepository({})",
            compact(file.text(manager.span())),
            compact(file.text(entity.span()))
        ),
    }
}

/// Whether `argument` is `this.<step-context-method><E>().getRepository(E)` with one identifier
/// `E` as both the type argument and the argument, and `E` the service's entity when it is known.
pub(crate) fn step_context(
    context: &Context<'_, '_>,
    argument: &Expression<'_>,
    entity: Option<&str>,
) -> bool {
    let Some(call) = call_expression(unwrap(argument)) else {
        return false;
    };
    let Some((object, name)) = property_access(unwrap(&call.callee)) else {
        return false;
    };
    let [only] = call.arguments.as_slice() else {
        return false;
    };
    let Some(Expression::Identifier(argument)) = only.as_expression().map(unwrap) else {
        return false;
    };
    let Some(step) = call_expression(unwrap(object)) else {
        return false;
    };
    let Some((this, method)) = property_access(unwrap(&step.callee)) else {
        return false;
    };
    let type_argument =
        step.type_arguments
            .as_ref()
            .and_then(|arguments| match arguments.params.as_slice() {
                [TSType::TSTypeReference(reference)] => match &reference.type_name {
                    TSTypeName::IdentifierReference(name) => Some(name.name.as_str()),
                    _ => None,
                },
                _ => None,
            });

    name == "getRepository"
        && method == context.step_context_method
        && matches!(unwrap(this), Expression::ThisExpression(_))
        && step.arguments.is_empty()
        && type_argument == Some(argument.name.as_str())
        && entity.is_none_or(|entity| entity == argument.name.as_str())
}

/// Source text on one line: each line trimmed, the lines joined.
pub(crate) fn compact(text: &str) -> String {
    text.lines().map(str::trim).collect()
}
