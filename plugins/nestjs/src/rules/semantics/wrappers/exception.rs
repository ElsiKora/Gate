//! Nest exception and plain error constructions, and whether a message is a value of the message
//! factory.

use std::rc::Rc;

use gate_sdk::Result;
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::ast::{
    CallExpression, Expression, NewExpression, ObjectPropertyKind, PropertyKind,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::value::unwrap;

use super::Config;
use super::provenance::{Provenance, Resolution, Value, static_name};
use super::shape::{call, member};

/// The Nest exception a construction names, through any alias, namespace or re-export: an export
/// of one of the exception packages whose name ends with the exception suffix.
pub fn nest_exception<'a>(
    config: &Config,
    provenance: &Provenance<'_, 'a>,
    file: &Rc<SourceFile<'a>>,
    construction: &'a NewExpression<'a>,
) -> Result<Option<String>> {
    Ok(
        match provenance.reference(file, &construction.callee, &[])? {
            Resolution::Found(Value::Package { module, name })
                if config.options.exception_packages.contains(&module)
                    && name.ends_with(config.options.exception_suffix.as_str()) =>
            {
                Some(name)
            }
            _ => None,
        },
    )
}

/// Whether a construction is of a plain error, a built-in error that no local binding shadows.
pub fn plain_error(
    config: &Config,
    file: &SourceFile<'_>,
    construction: &NewExpression<'_>,
) -> bool {
    matches!(
        unwrap(&construction.callee),
        Expression::Identifier(identifier)
            if config.options.plain_errors.iter().any(|plain| plain == identifier.name.as_str())
                && Provenance::symbol(file, identifier).is_none()
    )
}

/// Whether the first argument of a construction is a message of the message factory.
pub fn factory_message<'a>(
    config: &Config,
    provenance: &Provenance<'_, 'a>,
    file: &Rc<SourceFile<'a>>,
    construction: &'a NewExpression<'a>,
) -> Result<bool> {
    match construction
        .arguments
        .first()
        .and_then(|argument| argument.as_expression())
    {
        Some(message) => canonical(config, provenance, file, message, &[], &mut Vec::new()),
        None => Ok(false),
    }
}

/// Whether `expression` read through `properties` is a call of the message factory, kept directly
/// or as a field of a frozen object literal.
fn canonical<'a>(
    config: &Config,
    provenance: &Provenance<'_, 'a>,
    file: &Rc<SourceFile<'a>>,
    expression: &'a Expression<'a>,
    properties: &[String],
    active: &mut Vec<(String, u32, u32)>,
) -> Result<bool> {
    let Resolution::Found(Value::Expression {
        file: found,
        expression: node,
        properties: remaining,
        ..
    }) = provenance.reference(file, expression, properties)?
    else {
        return Ok(false);
    };
    let span = node.span();
    let key = (found.path().to_owned(), span.start, span.end);

    if active.contains(&key) {
        return Ok(false);
    }

    let Some(invocation) = call(node) else {
        return Ok(false);
    };
    active.push(key);
    let result = canonical_call(config, provenance, &found, invocation, &remaining, active);
    active.pop();
    result
}

fn canonical_call<'a>(
    config: &Config,
    provenance: &Provenance<'_, 'a>,
    file: &Rc<SourceFile<'a>>,
    invocation: &'a CallExpression<'a>,
    properties: &[String],
    active: &mut Vec<(String, u32, u32)>,
) -> Result<bool> {
    let Some((property, remaining)) = properties.split_first() else {
        return Ok(matches!(
            provenance.reference(file, &invocation.callee, &[])?,
            Resolution::Found(Value::Package { module, name })
                if module == config.factory_package && name == config.options.message_factory.name
        ));
    };
    // A field of a built-in `Object.freeze` of an object literal.
    let Some((owner, Some(access))) = member(unwrap(&invocation.callee)) else {
        return Ok(false);
    };
    let built_in = matches!(
        unwrap(owner),
        Expression::Identifier(owner) if owner.name == "Object" && Provenance::symbol(file, owner).is_none()
    );
    let ([argument], true, "freeze") = (invocation.arguments.as_slice(), built_in, access.as_str())
    else {
        return Ok(false);
    };
    let Some(Expression::ObjectExpression(literal)) = argument.as_expression().map(unwrap) else {
        return Ok(false);
    };

    for field in literal.properties.iter().rev() {
        let ObjectPropertyKind::ObjectProperty(field) = field else {
            return Ok(false);
        };
        let Some(name) = static_name(&field.key, field.computed) else {
            return Ok(false);
        };

        if name != *property {
            continue;
        }

        if field.method || field.kind != PropertyKind::Init {
            return Ok(false);
        }

        return canonical(config, provenance, file, &field.value, remaining, active);
    }

    Ok(false)
}
