//! Syntax helpers rule plugins share: decorator calls, object literals, member names and positions,
//! and the start of a node as the TypeScript compiler counts it (its first decorator, modifier or
//! `export`).

use crate::SourceFile;
use crate::oxc_ast::ast::{
    CallExpression, Class, ClassElement, Decorator, Expression, FormalParameter, MethodDefinition,
    MethodDefinitionKind, ObjectExpression, ObjectPropertyKind, PropertyKey, PropertyKind,
    Statement, TSType,
};
use crate::oxc_span::{GetSpan, Span};

use crate::owners::{Identity, SourceContext, call_expression, class_declaration, unwrap};

/// The start of a top-level class as the compiler counts it: its first decorator or `export`.
pub fn class_start(statement: &Statement<'_>, class: &Class<'_>) -> u32 {
    decorated_start(
        statement.span().start.min(class.span.start),
        &class.decorators,
    )
}

/// The start of a top-level statement as the compiler counts it: a decorated class from its first
/// decorator, also before `export`.
#[must_use]
pub fn statement_start(statement: &Statement<'_>) -> u32 {
    class_declaration(statement).map_or(statement.span().start, |class| {
        class_start(statement, class)
    })
}

/// The identity of a decorator's callee, through assertions and parentheses, and its call.
pub fn decorator_identity<'d, 'a>(
    source: &SourceContext<'_>,
    decorator: &'d Decorator<'a>,
) -> (Option<Identity>, Option<&'d CallExpression<'a>>) {
    let expression = unwrap(&decorator.expression);
    let call = call_expression(expression);
    let callee = call.map_or(expression, |call| &call.callee);

    (source.identity(callee, true), call)
}

/// The call of the first decorator that names `module`'s `name`, or `None` when that decorator is
/// no call or no decorator names it, as `getDecoratorCall` decided.
pub fn decorator_call<'d, 'a>(
    source: &SourceContext<'_>,
    decorators: &'d [Decorator<'a>],
    module: &str,
    name: &str,
) -> Option<&'d CallExpression<'a>> {
    decorators.iter().find_map(|decorator| {
        let (identity, call) = decorator_identity(source, decorator);

        identity
            .is_some_and(|identity| {
                identity.module.as_deref() == Some(module) && identity.name == name
            })
            .then_some(call)
    })?
}

/// The identity a type names: a type reference.
#[must_use]
pub fn type_identity(source: &SourceContext<'_>, kind: Option<&TSType<'_>>) -> Option<Identity> {
    match kind? {
        TSType::TSTypeReference(reference) => source.type_identity(&reference.type_name),
        _ => None,
    }
}

/// The object literal an expression is, directly or as the initializer of a top-level variable.
#[must_use]
pub fn object_literal<'s, 'a>(
    source: &'s SourceContext<'a>,
    expression: &'s Expression<'a>,
) -> Option<&'s ObjectExpression<'a>> {
    match unwrap(expression) {
        Expression::ObjectExpression(object) => Some(object),
        Expression::Identifier(identifier) => {
            let declarator = source.variables.get(identifier.name.as_str())?;

            match unwrap(declarator.init.as_ref()?) {
                Expression::ObjectExpression(object) => Some(object),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The name of an object or class member key as `getPropertyName` read it: an identifier, a
/// private name or a string literal.
#[must_use]
pub fn key_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    if computed {
        return None;
    }

    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::PrivateIdentifier(identifier) => Some(format!("#{}", identifier.name)),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        _ => None,
    }
}

/// The value of the first assignment or shorthand property named `name`.
pub fn object_property<'o, 'a>(
    object: &'o ObjectExpression<'a>,
    name: &str,
) -> Option<&'o Expression<'a>> {
    object.properties.iter().find_map(|item| {
        let ObjectPropertyKind::ObjectProperty(property) = item else {
            return None;
        };

        (!property.method
            && property.kind == PropertyKind::Init
            && key_name(&property.key, property.computed).as_deref() == Some(name))
        .then_some(&property.value)
    })
}

/// The type annotation of a constructor parameter.
pub fn parameter_type<'p, 'a>(parameter: &'p FormalParameter<'a>) -> Option<&'p TSType<'a>> {
    parameter
        .type_annotation
        .as_ref()
        .map(|annotation| &annotation.type_annotation)
}

/// A start widened to the first decorator, as the compiler's start of a decorated node.
pub fn decorated_start(start: u32, decorators: &[Decorator<'_>]) -> u32 {
    decorators
        .first()
        .map_or(start, |decorator| decorator.span.start.min(start))
}

/// Whether TypeScript reads a class method as a constructor. oxc already reads the instance
/// constructors as TypeScript does but keeps every `static` one a method; TypeScript reads a
/// non-generator method named by the `constructor` keyword, or by a `"constructor"` string directly
/// followed by `(`, as a constructor whether it is static or not, and never an accessor.
pub fn constructor(method: &MethodDefinition<'_>) -> bool {
    method.kind == MethodDefinitionKind::Constructor
        || (method.kind == MethodDefinitionKind::Method
            && !method.computed
            && !method.value.generator
            && match &method.key {
                PropertyKey::StaticIdentifier(key) => key.name == "constructor",
                PropertyKey::StringLiteral(key) => {
                    key.value == "constructor"
                        && !method.optional
                        && method.value.type_parameters.is_none()
                }
                _ => false,
            })
}

/// The kind of a class method as TypeScript reads it: a constructor by [`constructor`], otherwise
/// oxc's kind.
pub fn method_kind(method: &MethodDefinition<'_>) -> MethodDefinitionKind {
    if constructor(method) {
        MethodDefinitionKind::Constructor
    } else {
        method.kind
    }
}

/// The decorators, key and start of a class element.
fn parts<'e, 'a>(
    element: &'e ClassElement<'a>,
) -> (
    &'e [Decorator<'a>],
    Option<(&'e PropertyKey<'a>, bool)>,
    u32,
) {
    match element {
        ClassElement::MethodDefinition(method) => (
            &method.decorators,
            (!constructor(method)).then_some((&method.key, method.computed)),
            method.span.start,
        ),
        ClassElement::PropertyDefinition(property) => (
            &property.decorators,
            Some((&property.key, property.computed)),
            property.span.start,
        ),
        ClassElement::AccessorProperty(property) => (
            &property.decorators,
            Some((&property.key, property.computed)),
            property.span.start,
        ),
        ClassElement::StaticBlock(block) => (&[], None, block.span.start),
        ClassElement::TSIndexSignature(signature) => (&[], None, signature.span.start),
    }
}

/// The start of a class element as the compiler counts it.
#[must_use]
pub fn member_start(element: &ClassElement<'_>) -> u32 {
    let (decorators, _, start) = parts(element);
    decorated_start(start, decorators)
}

/// A member's name as `getMemberName` printed it: `constructor`, the text of an identifier,
/// private name or string, the source text of any other name, or `<anonymous>`.
#[must_use]
pub fn member_name(source: &SourceContext<'_>, element: &ClassElement<'_>) -> String {
    if matches!(element, ClassElement::MethodDefinition(method) if constructor(method)) {
        return "constructor".to_owned();
    }

    let Some((key, computed)) = parts(element).1 else {
        return "<anonymous>".to_owned();
    };

    key_name(key, computed).unwrap_or_else(|| {
        let span = key.span();

        if computed {
            computed_key_text(&source.file, span)
        } else {
            source.text(span).to_owned()
        }
    })
}

/// The source text of a computed member key with its brackets, as the compiler's text of the name:
/// from the nearest `[` before the key to the nearest `]` after it that no comment holds.
pub fn computed_key_text(file: &SourceFile<'_>, span: Span) -> String {
    let text = file.source();
    let comments = &file.program().comments;
    let commented = |offset: usize| {
        comments.iter().any(|comment| {
            comment.span.start as usize <= offset && offset < comment.span.end as usize
        })
    };
    let (key_start, key_end) = (span.start as usize, span.end as usize);
    let start = text[..key_start]
        .char_indices()
        .rev()
        .find(|&(offset, character)| character == '[' && !commented(offset))
        .map_or(key_start, |(offset, _)| offset);
    let end = text[key_end..]
        .char_indices()
        .find(|&(offset, character)| character == ']' && !commented(key_end + offset))
        .map_or(key_end, |(offset, _)| key_end + offset + 1);

    text[start..end].to_owned()
}
