//! The syntactic shapes the wrapper rule reads.

use std::collections::HashSet;

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, BindingPattern, CallExpression, ChainElement, Class,
    ClassElement, Decorator, Expression, FormalParameters, ImportDeclarationSpecifier,
    ObjectPropertyKind, PropertyKey, PropertyKind, Statement,
};
use gate_typescript::value::{string_like, unwrap};

use super::Config;
use super::provenance::Provenance;

/// A member access: its object and the name it reads, when the name is literal.
pub fn member<'e, 'a>(
    expression: &'e Expression<'a>,
) -> Option<(&'e Expression<'a>, Option<String>)> {
    match expression {
        Expression::StaticMemberExpression(member) => {
            Some((&member.object, Some(member.property.name.to_string())))
        }
        Expression::ComputedMemberExpression(member) => {
            Some((&member.object, string_like(&member.expression)))
        }
        Expression::PrivateFieldExpression(member) => {
            Some((&member.object, Some(format!("#{}", member.field.name))))
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => {
                Some((&member.object, Some(member.property.name.to_string())))
            }
            ChainElement::ComputedMemberExpression(member) => {
                Some((&member.object, string_like(&member.expression)))
            }
            ChainElement::PrivateFieldExpression(member) => {
                Some((&member.object, Some(format!("#{}", member.field.name))))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Whether a member access reads its name from a computed key that is not a literal.
fn dynamic_key(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::ComputedMemberExpression(member) => string_like(&member.expression).is_none(),
        Expression::ChainExpression(chain) => matches!(
            &chain.expression,
            ChainElement::ComputedMemberExpression(member) if string_like(&member.expression).is_none()
        ),
        _ => false,
    }
}

/// A call, also as the outermost link of an optional chain.
pub fn call<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e CallExpression<'a>> {
    match expression {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// The expression of a member chain that is not a member access.
pub fn root<'e, 'a>(expression: &'e Expression<'a>) -> &'e Expression<'a> {
    let mut current = unwrap(expression);

    while let Some((object, _)) = member(current) {
        current = unwrap(object);
    }

    current
}

/// The property a member chain reads directly from `this`.
fn this_property(expression: &Expression<'_>) -> Option<String> {
    let mut current = unwrap(expression);
    let mut name = None;

    while let Some((object, access)) = member(current) {
        name = access;
        current = unwrap(object);
    }

    if matches!(current, Expression::ThisExpression(_)) {
        name
    } else {
        None
    }
}

/// The call a `return` or expression statement makes, through `await`.
pub fn statement_call<'e, 'a>(statement: &'e Statement<'a>) -> Option<&'e CallExpression<'a>> {
    let expression = match statement {
        Statement::ReturnStatement(statement) => statement.argument.as_ref()?,
        Statement::ExpressionStatement(statement) => &statement.expression,
        _ => return None,
    };

    expression_call(expression)
}

/// The call an expression makes, through `await`.
pub fn expression_call<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e CallExpression<'a>> {
    let mut current = unwrap(expression);

    while let Expression::AwaitExpression(awaited) = current {
        current = unwrap(&awaited.argument);
    }

    call(current)
}

fn pattern_names(pattern: &BindingPattern<'_>, names: &mut HashSet<String>) {
    match pattern {
        BindingPattern::BindingIdentifier(binding) => {
            names.insert(binding.name.to_string());
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                pattern_names(&property.value, names);
            }

            if let Some(rest) = &object.rest {
                pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                pattern_names(element, names);
            }

            if let Some(rest) = &array.rest {
                pattern_names(&rest.argument, names);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => pattern_names(&assignment.left, names),
    }
}

/// The names the parameters bind.
pub fn parameter_names(parameters: &FormalParameters<'_>) -> HashSet<String> {
    let mut names = HashSet::new();

    for parameter in &parameters.items {
        pattern_names(&parameter.pattern, &mut names);
    }

    if let Some(rest) = &parameters.rest {
        pattern_names(&rest.rest.argument, &mut names);
    }

    names
}

fn pattern_has_default(pattern: &BindingPattern<'_>) -> bool {
    match pattern {
        BindingPattern::BindingIdentifier(_) => false,
        BindingPattern::AssignmentPattern(_) => true,
        BindingPattern::ObjectPattern(object) => {
            object
                .properties
                .iter()
                .any(|property| pattern_has_default(&property.value))
                || object
                    .rest
                    .as_ref()
                    .is_some_and(|rest| pattern_has_default(&rest.argument))
        }
        BindingPattern::ArrayPattern(array) => {
            array.elements.iter().flatten().any(pattern_has_default)
                || array
                    .rest
                    .as_ref()
                    .is_some_and(|rest| pattern_has_default(&rest.argument))
        }
    }
}

/// Whether a parameter or a binding inside one has a default value.
pub fn has_defaults(parameters: &FormalParameters<'_>) -> bool {
    parameters
        .items
        .iter()
        .any(|parameter| parameter.initializer.is_some() || pattern_has_default(&parameter.pattern))
        || parameters
            .rest
            .as_ref()
            .is_some_and(|rest| pattern_has_default(&rest.rest.argument))
}

/// The name of a class member as the compiler names a declaration: an identifier, private name
/// or string, not a computed name.
pub fn declaration_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    if computed {
        return None;
    }

    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::PrivateIdentifier(identifier) => Some(format!("#{}", identifier.name)),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::TemplateLiteral(_) => key.as_expression().and_then(string_like),
        _ => None,
    }
}

fn fixed_value(expression: &Expression<'_>) -> bool {
    let current = unwrap(expression);

    match current {
        Expression::Identifier(_)
        | Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,
        Expression::TemplateLiteral(template) => template.expressions.is_empty(),
        Expression::ArrayExpression(array) => array.elements.iter().all(|element| match element {
            ArrayExpressionElement::SpreadElement(spread) => fixed_value(&spread.argument),
            ArrayExpressionElement::Elision(_) => false,
            element => element.as_expression().is_some_and(fixed_value),
        }),
        Expression::ObjectExpression(object) => {
            object.properties.iter().all(|property| match property {
                ObjectPropertyKind::ObjectProperty(property) => {
                    property.shorthand
                        || (!property.method
                            && property.kind == PropertyKind::Init
                            && fixed_value(&property.value))
                }
                ObjectPropertyKind::SpreadProperty(spread) => fixed_value(&spread.argument),
            })
        }
        _ if member(current).is_some() => !matches!(root(current), Expression::ThisExpression(_)),
        _ => false,
    }
}

/// Whether `name` is a `readonly` field of `class` initialized with a fixed value.
fn fixed_field(class: Option<&Class<'_>>, name: Option<String>) -> bool {
    let (Some(class), Some(name)) = (class, name) else {
        return false;
    };
    let field = class.body.body.iter().find_map(|element| match element {
        ClassElement::PropertyDefinition(property)
            if declaration_name(&property.key, property.computed).as_deref() == Some(&name)
                && property.value.is_some() =>
        {
            Some((property.readonly, property.value.as_ref()))
        }
        ClassElement::AccessorProperty(property)
            if declaration_name(&property.key, property.computed).as_deref() == Some(&name)
                && property.value.is_some() =>
        {
            Some((false, property.value.as_ref()))
        }
        _ => None,
    });

    matches!(field, Some((true, Some(value))) if fixed_value(value))
}

fn transparent(
    expression: &Expression<'_>,
    parameters: &HashSet<String>,
    class: Option<&Class<'_>>,
) -> bool {
    let current = unwrap(expression);

    match current {
        Expression::Identifier(identifier) => parameters.contains(identifier.name.as_str()),
        Expression::ArrayExpression(array) => array.elements.iter().all(|element| {
            !matches!(
                element,
                ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_)
            ) && element
                .as_expression()
                .is_some_and(|element| transparent(element, parameters, class))
        }),
        Expression::ObjectExpression(object) => {
            object.properties.iter().all(|property| match property {
                ObjectPropertyKind::ObjectProperty(property) if property.shorthand => {
                    matches!(&property.value, Expression::Identifier(identifier) if parameters.contains(identifier.name.as_str()))
                }
                ObjectPropertyKind::ObjectProperty(property) => {
                    !property.method
                        && property.kind == PropertyKind::Init
                        && transparent(&property.value, parameters, class)
                }
                ObjectPropertyKind::SpreadProperty(_) => false,
            })
        }
        _ if member(current).is_some() => match root(current) {
            Expression::ThisExpression(_) => !fixed_field(class, this_property(current)),
            Expression::Identifier(identifier) => parameters.contains(identifier.name.as_str()),
            _ => false,
        },
        _ => false,
    }
}

fn supported_callee(expression: &Expression<'_>, parameters: &HashSet<String>) -> bool {
    let current = unwrap(expression);

    if let Expression::Identifier(identifier) = current {
        return !parameters.contains(identifier.name.as_str());
    }

    if member(current).is_none() || dynamic_key(current) {
        return false;
    }

    match root(current) {
        Expression::Identifier(identifier) => !parameters.contains(identifier.name.as_str()),
        Expression::Super(_) | Expression::ThisExpression(_) => true,
        _ => false,
    }
}

/// Whether `owner.name` is one of the intrinsic calls, such as `JSON.stringify`.
fn intrinsic_call(config: &Config, owner: &str, name: &str) -> bool {
    config
        .options
        .intrinsic_calls
        .iter()
        .any(|[object, method]| object == owner && method == name)
}

/// An intrinsic call, such as `JSON.stringify`, of a new array or object literal, which builds a
/// value instead of forwarding one; a local binding of the object's name is not the built-in.
fn intrinsic_structured_value(
    config: &Config,
    file: &SourceFile<'_>,
    call: &CallExpression<'_>,
) -> bool {
    let [argument] = call.arguments.as_slice() else {
        return false;
    };
    let Some((owner, Some(name))) = member(unwrap(&call.callee)) else {
        return false;
    };
    let Expression::Identifier(owner) = unwrap(owner) else {
        return false;
    };
    let argument = argument.as_expression().map(unwrap);

    Provenance::symbol(file, owner).is_none()
        && intrinsic_call(config, &owner.name, &name)
        && matches!(
            argument,
            Some(Expression::ArrayExpression(_) | Expression::ObjectExpression(_))
        )
}

/// Whether the call forwards to one of the intrinsic calls by name, built-in or not.
pub fn intrinsic_alias(config: &Config, call: &CallExpression<'_>) -> bool {
    let Some((owner, Some(name))) = member(unwrap(&call.callee)) else {
        return false;
    };

    matches!(
        unwrap(owner),
        Expression::Identifier(owner) if intrinsic_call(config, &owner.name, &name)
    )
}

/// Whether the call only forwards the parameters, unchanged, to another call.
pub fn forwarding_call(
    config: &Config,
    file: &SourceFile<'_>,
    call: &CallExpression<'_>,
    parameters: &FormalParameters<'_>,
    class: Option<&Class<'_>>,
) -> bool {
    let names = parameter_names(parameters);

    !has_defaults(parameters)
        && !intrinsic_structured_value(config, file, call)
        && supported_callee(&call.callee, &names)
        && call.arguments.iter().all(|argument| match argument.as_expression() {
            Some(argument) => transparent(argument, &names, class),
            None => matches!(argument, Argument::SpreadElement(spread) if transparent(&spread.argument, &names, class)),
        })
}

/// Whether `class` adapts a reporter port: one of the reporter adapters names `name` as its method,
/// `path` lies below its path prefix, and the class extends the port class the file imports from
/// its module.
fn reporter_adapter(
    config: &Config,
    file: &SourceFile<'_>,
    path: &str,
    name: &str,
    class: Option<&Class<'_>>,
) -> bool {
    let Some(class) = class else {
        return false;
    };
    let Some(Expression::Identifier(parent)) =
        class.heritage.as_ref().map(|heritage| &heritage.expression)
    else {
        return false;
    };

    config.options.reporter_adapters.iter().any(|adapter| {
        path.starts_with(adapter.path_prefix.as_str())
            && name == adapter.method
            && file
                .program()
                .body
                .iter()
                .filter_map(|statement| match statement {
                    Statement::ImportDeclaration(import)
                        if import.source.value == adapter.module =>
                    {
                        import.specifiers.as_ref()
                    }
                    _ => None,
                })
                .flatten()
                .any(|specifier| match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(specifier) => {
                        super::provenance::export_name(&specifier.imported) == adapter.class
                            && specifier.local.name == parent.name
                    }
                    _ => false,
                })
    })
}

/// Whether a method or field is a framework boundary: decorated, a lifecycle method, a reporter
/// port adapter, or a framework method of its folder.
pub fn framework_boundary(
    config: &Config,
    file: &SourceFile<'_>,
    path: &str,
    name: &str,
    decorators: &[Decorator<'_>],
    class: Option<&Class<'_>>,
) -> bool {
    !decorators.is_empty()
        || config
            .options
            .lifecycle_methods
            .iter()
            .any(|method| method == name)
        || reporter_adapter(config, file, path, name, class)
        || config
            .options
            .framework_methods
            .iter()
            .any(|(segment, methods)| {
                path.contains(segment.as_str()) && methods.iter().any(|method| method == name)
            })
}
