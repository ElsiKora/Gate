//! Member-level controller checks: route handlers only, owned contract prefixes and the custom
//! service boundary.

use std::collections::BTreeMap;

use gate_typescript::oxc_ast::ast::{
    Class, ClassElement, Expression, Function, MethodDefinitionKind, ObjectExpression,
    ObjectPropertyKind, PropertyKind, TSAccessibility, TSType, TSTypeReference,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};

use gate_typescript::owners::{Identity, SourceContext};

use super::contract::Js;
use super::{Names, route_handler, route_type};
use crate::shared::decorators::{
    constructor, decorator_call, key_name, member_name, member_start, object_literal,
};
use crate::shared::report::Findings;

/// `route-handler-only`: every member other than a constructor is a route handler.
pub(super) fn route_handlers(
    source: &SourceContext<'_>,
    class: &Class<'_>,
    names: &Names,
    findings: &mut Findings,
) {
    let class_name = class
        .id
        .as_ref()
        .map_or("<anonymous>", |id| id.name.as_str());

    for element in &class.body.body {
        if let ClassElement::MethodDefinition(method) = element {
            if constructor(method) {
                continue;
            }

            if method.kind == MethodDefinitionKind::Method
                && route_handler(source, &method.decorators, names)
            {
                continue;
            }
        }

        let line = source.file.line(member_start(element));
        findings.push(
            "route-handler-only",
            source.path.as_str(),
            Some(line),
            format!(
                "{}:{line} {class_name}.{} is not a route-decorated controller handler; move helper logic to its semantic owner",
                source.path,
                member_name(source, element)
            ),
        );
    }
}

/// Collects the identities of every type reference in a type.
struct References<'s, 'a> {
    source: &'s SourceContext<'a>,
    found: Vec<Identity>,
}

impl<'a> Visit<'a> for References<'_, 'a> {
    fn visit_ts_type_reference(&mut self, reference: &TSTypeReference<'a>) {
        if let Some(identity) = self.source.type_identity(&reference.type_name) {
            self.found.push(identity);
        }

        walk::walk_ts_type_reference(self, reference);
    }
}

/// The signature types of a public member: parameter and return types and type parameter
/// constraints and defaults of a method, constructor or accessor, or a property's type.
fn signature_types<'e, 'a>(element: &'e ClassElement<'a>) -> Vec<&'e TSType<'a>> {
    let mut types = Vec::new();
    let function: Option<&Function<'a>> = match element {
        ClassElement::MethodDefinition(method) => Some(&method.value),
        _ => None,
    };

    if let Some(function) = function {
        // TypeScript lists a `this` parameter first and a rest parameter last among the
        // parameters; oxc keeps both apart.
        if let Some(annotation) = function
            .this_param
            .as_ref()
            .and_then(|this| this.type_annotation.as_ref())
        {
            types.push(&annotation.type_annotation);
        }

        for parameter in &function.params.items {
            if let Some(annotation) = &parameter.type_annotation {
                types.push(&annotation.type_annotation);
            }
        }

        if let Some(annotation) = function
            .params
            .rest
            .as_ref()
            .and_then(|rest| rest.type_annotation.as_ref())
        {
            types.push(&annotation.type_annotation);
        }

        if let Some(annotation) = &function.return_type {
            types.push(&annotation.type_annotation);
        }

        for parameter in function
            .type_parameters
            .iter()
            .flat_map(|parameters| parameters.params.iter())
        {
            types.extend(parameter.constraint.as_ref());
            types.extend(parameter.default.as_ref());
        }
    } else if let ClassElement::PropertyDefinition(property) = element {
        types.extend(
            property
                .type_annotation
                .as_ref()
                .map(|annotation| &annotation.type_annotation),
        );
    } else if let ClassElement::AccessorProperty(property) = element {
        types.extend(
            property
                .type_annotation
                .as_ref()
                .map(|annotation| &annotation.type_annotation),
        );
    }

    types
}

/// Whether a member is declared `private` or `protected`.
fn hidden(element: &ClassElement<'_>) -> bool {
    let accessibility = match element {
        ClassElement::MethodDefinition(method) => method.accessibility,
        ClassElement::PropertyDefinition(property) => property.accessibility,
        ClassElement::AccessorProperty(property) => property.accessibility,
        _ => None,
    };

    matches!(
        accessibility,
        Some(TSAccessibility::Private | TSAccessibility::Protected)
    )
}

/// `owned-contract-prefix`: a contract type of this owner's directory that a public
/// member exposes carries the semantic owner's prefix.
pub(super) fn owned_contracts<'a>(
    source: &SourceContext<'a>,
    class: &Class<'a>,
    role: &str,
    semantic_owner: Js<'_>,
    findings: &mut Findings,
) {
    // `semanticOwnerName ?? roleName.replace(/(?:Controller|Service)$/u, "")`, used as a string.
    let stem = match semantic_owner {
        Js::Undefined | Js::Null => role
            .strip_suffix("Controller")
            .or_else(|| role.strip_suffix("Service"))
            .unwrap_or(role)
            .to_owned(),
        value => value.text(),
    };
    let directory = format!(
        "{}/",
        source
            .path
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory)
    );

    for element in &class.body.body {
        if hidden(element)
            || matches!(
                element,
                ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_)
            )
        {
            continue;
        }

        let mut references = References {
            source,
            found: Vec::new(),
        };

        for kind in signature_types(element) {
            references.visit_ts_type(kind);
        }

        // Sorted by name, so the findings of one member come in a stable order.
        references
            .found
            .sort_by(|left, right| left.name.cmp(&right.name));

        for identity in &references.found {
            if !identity
                .source
                .as_deref()
                .is_some_and(|path| path.starts_with(&directory))
            {
                continue;
            }

            let name = identity.name.as_str();
            let prefix = if name.ends_with("DTO") {
                stem.clone()
            } else if interface_like(name, 'I') {
                format!("I{stem}")
            } else if interface_like(name, 'T') {
                format!("T{stem}")
            } else {
                continue;
            };

            if !name.starts_with(&prefix) {
                let line = source.file.line(member_start(element));
                findings.push(
                    "owned-contract-prefix",
                    source.path.as_str(),
                    Some(line),
                    format!(
                        "{}:{line} {role}.{} exposes owned contract {name}; expected the {prefix} semantic-owner prefix",
                        source.path,
                        member_name(source, element)
                    ),
                );
            }
        }
    }
}

/// `/^I[A-Z]/` and `/^T[A-Z]/`.
fn interface_like(name: &str, first: char) -> bool {
    let mut characters = name.chars();
    characters.next() == Some(first)
        && characters
            .next()
            .is_some_and(|next| next.is_ascii_uppercase())
}

/// The properties of an object literal: `None` for a spread, method or accessor, a missing or
/// repeated key; with `routes`, the keys are the names of computed `EApiRouteType` members of the
/// automator package `routes` names.
fn object_properties<'o, 'a>(
    source: &SourceContext<'a>,
    object: &'o ObjectExpression<'a>,
    routes: Option<&str>,
) -> Option<BTreeMap<String, &'o Expression<'a>>> {
    let mut result = BTreeMap::new();

    for item in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = item else {
            return None;
        };

        if property.method || property.kind != PropertyKind::Init {
            return None;
        }

        let name = if let Some(automator) = routes {
            if property.computed {
                property
                    .key
                    .as_expression()
                    .and_then(|key| route_type(source, key, automator))
            } else {
                None
            }
        } else {
            key_name(&property.key, property.computed)
        }?;

        // An empty name is falsy, so the whole object is dropped.
        if name.is_empty() || result.insert(name, &property.value).is_some() {
            return None;
        }
    }

    Some(result)
}

/// The properties of the object literal an expression resolves to.
fn properties<'o, 'a>(
    source: &'o SourceContext<'a>,
    expression: Option<&'o Expression<'a>>,
    routes: Option<&str>,
) -> Option<BTreeMap<String, &'o Expression<'a>>> {
    object_properties(source, object_literal(source, expression?)?, routes)
}

/// `custom-service-entity`: a controller whose primary service binds another entity disables every
/// generated route, loads no relations, inherits nothing and has one dependency.
pub(super) fn custom_service_boundary<'s, 'a>(
    source: &'s SourceContext<'a>,
    class: &Class<'a>,
    config: Option<&'s ObjectExpression<'a>>,
    label: &str,
    line: u32,
    names: &Names,
    findings: &mut Findings,
) {
    let mut error = |message: String| {
        findings.push(
            "custom-service-entity",
            source.path.as_str(),
            Some(line),
            message,
        );
    };
    let automator = names.automator.as_str();
    let controller = config.and_then(|config| object_properties(source, config, None));
    let routes = controller.as_ref().and_then(|controller| {
        properties(source, controller.get("routes").copied(), Some(automator))
    });
    let generated = &names.generated_routes;
    let disabled = routes.as_ref().is_some_and(|routes| {
        routes.len() == generated.len()
            && generated.iter().all(|name| {
                let route = properties(source, routes.get(name.as_str()).copied(), None);
                let generation = properties(
                    source,
                    route.as_ref().and_then(|route| route.get("generation").copied()),
                    None,
                );

                generation.is_some_and(|generation| {
                    matches!(generation.get("isEnabled"), Some(Expression::BooleanLiteral(enabled)) if !enabled.value)
                })
            })
    });

    if !disabled {
        error(format!(
            "{label} a distinct primary service entity requires all six generated routes explicitly disabled"
        ));
    }

    if controller
        .as_ref()
        .is_none_or(|controller| controller.contains_key("relations"))
    {
        error(format!(
            "{label} a distinct primary service entity requires explicit metadata without relation loading"
        ));
    }

    if class.heritage.is_some() {
        error(format!(
            "{label} a distinct primary service entity cannot inherit controller behavior"
        ));
    }

    let first = class.body.body.iter().find_map(|element| match element {
        ClassElement::MethodDefinition(method) if constructor(method) => Some(method),
        _ => None,
    });

    // The compiler counts a rest parameter among the parameters.
    if first.is_none_or(|first| {
        first.value.params.items.len() + usize::from(first.value.params.rest.is_some()) != 1
    }) {
        error(format!(
            "{label} a distinct primary service entity requires one service dependency"
        ));
    }

    for element in &class.body.body {
        let ClassElement::MethodDefinition(method) = element else {
            continue;
        };

        if method.kind != MethodDefinitionKind::Method || constructor(method) {
            continue;
        }

        let route = decorator_call(source, &method.decorators, automator, "ApiRouteCustom")
            .and_then(|call| {
                properties(
                    source,
                    call.arguments
                        .first()
                        .and_then(|argument| argument.as_expression()),
                    None,
                )
            });

        if route.is_none_or(|route| route.contains_key("relations")) {
            error(format!(
                "{label} a distinct primary service entity requires explicit ApiRouteCustom metadata without relation loading"
            ));
        }
    }
}
