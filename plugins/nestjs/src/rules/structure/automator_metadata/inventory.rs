//! The entity-property, DTO-property and route inventories of the modules, each entry with the
//! contract it has and the one it must have.

use std::collections::{BTreeMap, BTreeSet};

use gate_typescript::oxc_ast::ast::{
    Class, ClassElement, Decorator, Expression, MethodDefinitionKind, ObjectPropertyKind,
};
use gate_typescript::oxc_span::{GetSpan, Span};

use super::policy::Policy;
use super::source::{
    Semantic, Source, as_object, call, decorator_call, first_argument, is_false, object_property,
    property_name, unwrap, variable_object,
};
use crate::shared::syntax::top_level_classes;

const DESCRIBED_EXPLICIT: &str = "DESCRIBED_EXPLICIT";
const DESCRIBED_IMPLICIT: &str = "DESCRIBED_IMPLICIT";
const MISSING: &str = "MISSING";
const API_METHOD: &str = "API_METHOD";
const API_ROUTE_CUSTOM: &str = "API_ROUTE_CUSTOM";

/// One inventory entry: its key, where it is, what it has and, unless it is a generated route,
/// what it must have.
#[derive(Debug)]
pub struct Entry {
    pub key: String,
    pub path: String,
    pub line: u32,
    pub actual: Semantic,
    pub expected: Option<Semantic>,
}

/// A controller class decorator that no inventory classifies.
#[derive(Debug)]
pub struct Unclassified {
    pub path: String,
    pub line: u32,
    pub key: String,
    pub message: String,
}

fn object(entries: Vec<(&str, Semantic)>) -> Semantic {
    Semantic::Obj(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn count(value: usize) -> Semantic {
    Semantic::Num(f64::from(u32::try_from(value).unwrap_or(u32::MAX)))
}

fn owner(kind: &str) -> Semantic {
    object(vec![("kind", Semantic::Str(kind.to_owned()))])
}

/// The Automator property decorators other than `ApiPropertyDescribe` on the properties of an
/// entity class (`is_entity_class`), which describes every field with `ApiPropertyDescribe` alone
/// (`entity-property-decorator`).
pub fn specialized_entity_decorators(
    source: &Source<'_, '_>,
    bases: &BTreeSet<String>,
    found: &mut Vec<Unclassified>,
) {
    for class in top_level_classes(&source.file.program().body) {
        let class_name = class
            .id
            .as_ref()
            .map_or("<anonymous>", |id| id.name.as_str());

        if !is_entity_class(source, class, bases) {
            continue;
        }

        for element in &class.body.body {
            let (decorators, key, computed) = match element {
                ClassElement::PropertyDefinition(property) => {
                    (&property.decorators, &property.key, property.computed)
                }
                ClassElement::AccessorProperty(property) => {
                    (&property.decorators, &property.key, property.computed)
                }
                _ => continue,
            };
            let property = property_name(key, computed).unwrap_or_default();

            for decorator in decorators {
                let Some(imported) =
                    decorator_call(decorator).and_then(|call| source.imported(&call.callee))
                else {
                    continue;
                };

                if imported.module == source.policy.automator_package
                    && imported.name.starts_with("ApiProperty")
                    && imported.name != "ApiPropertyDescribe"
                {
                    found.push(Unclassified {
                        path: source.path.to_owned(),
                        line: source.line(decorator.span),
                        key: format!("{class_name}.{property}@{}", imported.name),
                        message: format!(
                            "{}.{property} uses @{} in an entity class; describe entity fields with @ApiPropertyDescribe only",
                            class_name, imported.name
                        ),
                    });
                }
            }
        }
    }
}

/// The persisted properties of every entity class (`is_entity_class`) of a file.
pub fn entities(
    source: &Source<'_, '_>,
    policy: &Policy,
    bases: &BTreeSet<String>,
    entries: &mut Vec<Entry>,
) {
    for class in top_level_classes(&source.file.program().body) {
        let Some(class_name) = class.id.as_ref().map(|id| id.name.to_string()) else {
            continue;
        };

        if !is_entity_class(source, class, bases) {
            continue;
        }

        for element in &class.body.body {
            let (decorators, key, computed, span) = match element {
                ClassElement::PropertyDefinition(property) => (
                    &property.decorators,
                    &property.key,
                    property.computed,
                    property.span,
                ),
                ClassElement::AccessorProperty(property) => (
                    &property.decorators,
                    &property.key,
                    property.computed,
                    property.span,
                ),
                _ => continue,
            };
            let Some(property) = property_name(key, computed) else {
                continue;
            };

            if !decorators.iter().any(|decorator| {
                decorator_call(decorator)
                    .and_then(|call| source.imported(&call.callee))
                    .is_some_and(|imported| {
                        imported.module == policy.typeorm_package
                            && policy.persisted_decorators.contains(&imported.name)
                    })
            }) {
                continue;
            }

            let key = format!("{class_name}.{property}");
            let describes: Vec<&Decorator<'_>> = decorators
                .iter()
                .filter(|decorator| {
                    decorator_call(decorator).is_some_and(|call| {
                        source.expression_name(&call.callee).as_deref()
                            == Some("ApiPropertyDescribe")
                    })
                })
                .collect();
            let metadata = describes
                .first()
                .and_then(|decorator| decorator_call(decorator))
                .and_then(first_argument)
                .and_then(|argument| match argument {
                    Expression::ObjectExpression(object) => Some(&**object),
                    _ => None,
                });
            let type_property = object_property(metadata, "type");
            let state = if describes.is_empty() {
                MISSING
            } else if type_property.is_some() {
                DESCRIBED_EXPLICIT
            } else {
                DESCRIBED_IMPLICIT
            };
            let auto_dto = source.semantic(object_property(metadata, "isAutoDtoEnabled"));
            let hidden_type = policy.hidden.get(&key).cloned();
            let must_be_hidden = hidden_type.is_some() || policy.buffer.contains(&key);
            let mut actual = vec![
                ("describeCount", count(describes.len())),
                ("state", Semantic::Str(state.to_owned())),
            ];
            let mut expected = vec![
                ("describeCount", count(1)),
                ("state", Semantic::Str(DESCRIBED_EXPLICIT.to_owned())),
            ];

            if must_be_hidden {
                actual.push((
                    "isAutoDtoEnabled",
                    Semantic::Bool(auto_dto != Semantic::Bool(false)),
                ));
                expected.push(("isAutoDtoEnabled", Semantic::Bool(false)));
            }

            if let Some(hidden_type) = hidden_type {
                actual.push(("type", source.semantic(type_property)));
                expected.push(("type", Semantic::Str(hidden_type)));
            }

            entries.push(Entry {
                key,
                path: source.path.to_owned(),
                line: source.line(span),
                actual: object(actual),
                expected: Some(object(expected)),
            });
        }
    }
}

/// Whether a file is a domain entity file, below the domain plane and ending with the entity file
/// suffix, whose `@Entity` and `@ChildEntity` classes the rule reads.
pub fn is_entity_file(policy: &Policy, path: &str) -> bool {
    path.starts_with(&policy.domain_prefix) && path.ends_with(policy.entity_file_suffix.as_str())
}

/// Whether a class decorator's callee names `Entity` or `ChildEntity`, by name as for any entity
/// decorator.
pub fn is_entity_decorated<'a>(source: &Source<'_, 'a>, class: &Class<'a>) -> bool {
    class.decorators.iter().any(|decorator| {
        decorator_call(decorator).is_some_and(|call| {
            matches!(
                source.expression_name(&call.callee).as_deref(),
                Some("Entity" | "ChildEntity")
            )
        })
    })
}

/// Whether a class is read as an entity class: an `@Entity` or `@ChildEntity` class of a domain
/// entity file, or one of `bases`, the classes of the file that such a class extends, directly or
/// through other bases.
fn is_entity_class<'a>(
    source: &Source<'_, 'a>,
    class: &Class<'a>,
    bases: &BTreeSet<String>,
) -> bool {
    (is_entity_file(source.policy, source.path) && is_entity_decorated(source, class))
        || class
            .id
            .as_ref()
            .is_some_and(|id| bases.contains(id.name.as_str()))
}

/// The properties of every `*DTO` class of a file below a `dto` folder.
pub fn dtos<'a>(source: &Source<'_, 'a>, policy: &Policy, entries: &mut Vec<Entry>) {
    if !source.path.contains(policy.dto_folder.as_str()) {
        return;
    }

    for class in top_level_classes(&source.file.program().body) {
        let Some(class_name) = class
            .id
            .as_ref()
            .map(|id| id.name.to_string())
            .filter(|name| name.ends_with(policy.dto_suffix.as_str()))
        else {
            continue;
        };

        for element in &class.body.body {
            let (decorators, key, computed, span) = match element {
                ClassElement::PropertyDefinition(property) => (
                    &property.decorators,
                    &property.key,
                    property.computed,
                    property.span,
                ),
                ClassElement::AccessorProperty(property) => (
                    &property.decorators,
                    &property.key,
                    property.computed,
                    property.span,
                ),
                _ => continue,
            };
            let Some(property) = property_name(key, computed) else {
                continue;
            };
            let key = format!("{class_name}.{property}");
            let imported = |decorator: &Decorator<'a>| {
                decorator_call(decorator).and_then(|call| source.imported(&call.callee))
            };
            let automator = decorators
                .iter()
                .filter(|decorator| {
                    imported(decorator).is_some_and(|imported| {
                        imported.module == source.policy.automator_package
                            && imported.name.starts_with("ApiProperty")
                    })
                })
                .count();
            let validators = decorators
                .iter()
                .filter(|decorator| {
                    imported(decorator)
                        .is_some_and(|imported| imported.module == policy.validator_package)
                })
                .count();
            let custom = policy.decorators.get(&key).map_or(0, |name| {
                decorators
                    .iter()
                    .filter(|decorator| {
                        decorator_call(decorator).is_some_and(|call| {
                            source.expression_name(&call.callee).as_deref() == Some(name.as_str())
                        })
                    })
                    .count()
            });

            entries.push(Entry {
                key,
                path: source.path.to_owned(),
                line: source.line(span),
                actual: object(vec![
                    ("decoratorCount", count(automator + custom)),
                    ("duplicateValidatorCount", count(validators)),
                ]),
                expected: Some(object(vec![
                    ("decoratorCount", count(1)),
                    ("duplicateValidatorCount", count(0)),
                ])),
            });
        }
    }
}

/// JavaScript's default sort of decorator names, `null` sorting as its text.
fn sorted(mut names: Vec<Option<String>>) -> Vec<Option<String>> {
    names.sort_by(|left, right| {
        let text = |name: &Option<String>| -> Vec<u16> {
            name.as_deref().unwrap_or("null").encode_utf16().collect()
        };
        text(left).cmp(&text(right))
    });
    names
}

fn names(names: &[Option<String>]) -> Semantic {
    Semantic::Arr(
        names
            .iter()
            .map(|name| name.clone().map_or(Semantic::Null, Semantic::Str))
            .collect(),
    )
}

/// The generated routes and custom handlers of every class of a `controller.ts` file, and its
/// unclassified class decorators.
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the class keeps the generated routes and the custom handlers in the order the controller declares them"
)]
pub fn routes<'a>(
    source: &Source<'_, 'a>,
    policy: &Policy,
    entries: &mut Vec<Entry>,
    unclassified: &mut Vec<Unclassified>,
) {
    if !source
        .path
        .ends_with(policy.controller_file_suffix.as_str())
    {
        return;
    }

    let body = &source.file.program().body;

    for class in top_level_classes(body) {
        let Some(class_name) = class.id.as_ref().map(|id| id.name.to_string()) else {
            continue;
        };
        let calls: Vec<_> = class.decorators.iter().filter_map(decorator_call).collect();

        for call in &calls {
            let name = source
                .expression_name(&call.callee)
                .unwrap_or_else(|| source.text(call.callee.span()).to_owned());

            if !name.is_empty() && !policy.controller_class_decorators.contains(&name) {
                unclassified.push(Unclassified {
                    path: source.path.to_owned(),
                    line: source.line(call.span),
                    key: format!("{class_name}@{name}"),
                    message: format!(
                        "{} controller {class_name} has unclassified class decorator {name}",
                        source.path
                    ),
                });
            }
        }

        let api_controller = calls
            .iter()
            .find(|call| source.expression_name(&call.callee).as_deref() == Some("ApiController"));

        if let Some(api_controller) = api_controller {
            let config = match first_argument(api_controller) {
                Some(Expression::Identifier(identifier)) => {
                    variable_object(body, identifier.name.as_str())
                }
                Some(Expression::ObjectExpression(object)) => Some(&**object),
                _ => None,
            };
            let routes = object_property(config, "routes").map(unwrap);
            let generated = |key: &str, span: Span| Entry {
                key: format!("{class_name}.{key}"),
                path: source.path.to_owned(),
                line: source.line(span),
                actual: object(vec![
                    ("ownerLocalExceptions", Semantic::Arr(Vec::new())),
                    ("owners", Semantic::Arr(vec![owner(API_METHOD)])),
                ]),
                expected: None,
            };

            match routes {
                Some(Expression::ObjectExpression(routes)) => {
                    let mut objects = BTreeMap::new();

                    for property in &routes.properties {
                        if let ObjectPropertyKind::ObjectProperty(property) = property
                            && !property.shorthand
                            && !property.method
                            && let Some(route_type) =
                                property_name(&property.key, property.computed)
                            && let Some(route) = as_object(&property.value)
                        {
                            objects.insert(route_type, route);
                        }
                    }

                    for route_type in &policy.generated_routes {
                        let route = objects.get(route_type).copied();

                        if let Some(route) = route {
                            let generation =
                                object_property(Some(route), "generation").and_then(as_object);
                            let disabled = generation
                                .and_then(|generation| {
                                    object_property(Some(generation), "isEnabled")
                                })
                                .is_some_and(is_false);

                            if disabled {
                                continue;
                            }
                        }

                        entries.push(generated(
                            route_type,
                            route.map_or(api_controller.span, |route| route.span),
                        ));
                    }
                }
                Some(expression)
                    if let Some(helper) = &policy.routes_helper
                        && let Some(routes) = call(expression)
                        && source.expression_name(&routes.callee).as_deref()
                            == Some(helper.name.as_str()) =>
                {
                    let update_disabled = first_argument(routes)
                        .and_then(|options| match options {
                            Expression::ObjectExpression(options) => Some(&**options),
                            _ => None,
                        })
                        .and_then(|options| object_property(Some(options), &helper.update_flag))
                        .is_some_and(is_false);

                    for route_type in &policy.generated_routes {
                        if update_disabled
                            && matches!(route_type.as_str(), "PARTIAL_UPDATE" | "UPDATE")
                        {
                            continue;
                        }

                        entries.push(generated(route_type, routes.span));
                    }
                }
                _ => {}
            }
        }

        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };

            if method.kind != MethodDefinitionKind::Method {
                continue;
            }

            let Some(method_name) = property_name(&method.key, method.computed) else {
                continue;
            };
            let decorator_names: Vec<Option<String>> = method
                .decorators
                .iter()
                .filter_map(decorator_call)
                .map(|call| source.expression_name(&call.callee))
                .collect();
            let api_methods = decorator_names
                .iter()
                .filter(|name| name.as_deref() == Some("ApiMethod"))
                .count();
            let custom_routes = decorator_names
                .iter()
                .filter(|name| name.as_deref() == Some("ApiRouteCustom"))
                .count();
            let owner_local: Vec<Option<String>> = decorator_names
                .iter()
                .filter(|name| !matches!(name.as_deref(), Some("ApiMethod" | "ApiRouteCustom")))
                .cloned()
                .collect();
            let key = format!("{class_name}.{method_name}");
            let documentation = policy.documentation.get(&key);
            let mut exceptions: Vec<Option<String>> = owner_local
                .iter()
                .filter(|name| {
                    name.as_deref().is_some_and(|name| {
                        policy
                            .always_allowed_route_decorators
                            .iter()
                            .any(|allowed| allowed == name)
                            || documentation
                                .is_some_and(|names| names.iter().any(|allowed| allowed == name))
                    })
                })
                .cloned()
                .collect();

            if policy.health_check_route.as_deref() == Some(key.as_str()) {
                exceptions.push(Some(policy.health_check_decorator.clone()));
            }

            let mut owners = vec![owner(API_METHOD); api_methods];
            owners.extend(vec![owner(API_ROUTE_CUSTOM); custom_routes]);
            let mut actual = vec![
                ("ownerLocalExceptions", names(&sorted(owner_local))),
                ("owners", Semantic::Arr(owners)),
            ];
            let mut expected = vec![
                ("ownerLocalExceptions", names(&sorted(exceptions))),
                ("owners", Semantic::Arr(vec![owner(API_ROUTE_CUSTOM)])),
            ];

            if let Some(expected_parameters) = policy.parameters.get(&key) {
                // The compiler lists a `this` parameter, which cannot be decorated, and a rest
                // parameter among the parameters.
                let decorated = |decorators: &[Decorator<'a>]| {
                    names(
                        &decorators
                            .iter()
                            .filter_map(decorator_call)
                            .map(|call| source.expression_name(&call.callee))
                            .collect::<Vec<_>>(),
                    )
                };
                let mut parameters: Vec<Semantic> = method
                    .value
                    .this_param
                    .iter()
                    .map(|_| Semantic::Arr(Vec::new()))
                    .collect();
                parameters.extend(
                    method
                        .value
                        .params
                        .items
                        .iter()
                        .map(|parameter| decorated(&parameter.decorators)),
                );
                parameters.extend(
                    method
                        .value
                        .params
                        .rest
                        .iter()
                        .map(|rest| decorated(&rest.decorators)),
                );
                actual.push(("parameterDecorators", Semantic::Arr(parameters)));
                expected.push(("parameterDecorators", expected_parameters.clone()));
            }

            entries.push(Entry {
                key,
                path: source.path.to_owned(),
                line: source.line(method.span),
                actual: object(actual),
                expected: Some(object(expected)),
            });
        }
    }
}
