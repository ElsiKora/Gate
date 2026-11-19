//! `nestjs/controller-owners`: every controller is a registered semantic owner with exact metadata.
//!
//! The registry is the `controllers` option: one row per controller file. Every top-level class of
//! a source file named `…Controller` or decorated with an automator `ApiController` or a Nest
//! `Controller` call is a controller: its file must hold exactly the registered one, which must
//! carry exactly its metadata, inject its primary service once, keep only route handlers, and name
//! the contracts it owns after its semantic owner.

mod contract;
mod members;

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::Value;
use gate_sdk::{Rule, Run};
use gate_typescript::owners::{
    Identity, Owner, SourceContext, Sources, class_declaration, find_class, property_access,
    top_level, unwrap,
};
use gate_typescript::oxc_ast::ast::{
    Argument, Class, ClassElement, Decorator, Expression, FormalParameter, MethodDefinition,
    Statement, TSType,
};
use gate_typescript::oxc_span::GetSpan;
use regex_automata::meta::Regex;

use self::contract::{Contract, Js, same_name, same_path};
pub use self::contract::{ControllerRow, Nullable};
use crate::layout::{PluginOptions, folder_prefix};
use crate::shared::decorators::{
    class_start, constructor, decorated_start, decorator_call, decorator_identity, object_literal,
    object_property, parameter_type, type_identity,
};
use crate::shared::pattern;
use crate::shared::report::Findings;

/// The options of `nestjs/controller-owners`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The registry: one row per controller file.
    pub controllers: Vec<ControllerRow>,
    /// The module plane, below `layout.modules-root`, that holds `EXTERNAL_NOUN` controllers.
    pub external_noun_plane: String,
    /// The name of the constructor parameter that injects the primary service.
    pub service_member: String,
    /// The route decorators of `layout.nest-common-package`.
    pub nest_route_decorators: Vec<String>,
    /// The names of the automator's route decorators.
    pub automator_route_pattern: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            controllers: Vec::new(),
            external_noun_plane: "public".to_owned(),
            service_member: "service".to_owned(),
            nest_route_decorators: [
                "All", "Delete", "Get", "Head", "Options", "Patch", "Post", "Put", "Search",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            automator_route_pattern: "^ApiRoute[A-Za-z0-9]*$".to_owned(),
        }
    }
}

/// The names the checks match, from the options and the layout.
pub(super) struct Names {
    automator: String,
    nest_common: String,
    nest_routes: Vec<String>,
    automator_route: Regex,
    generated_routes: Vec<String>,
    service_member: String,
}

/// The rule.
#[derive(Debug, Default)]
pub struct ControllerOwners;

impl Rule<crate::Nest> for ControllerOwners {
    type Options = Options;

    const NAME: &'static str = "controller-owners";
    const TITLE: &'static str =
        "Every controller is a registered semantic owner with exact metadata";
    const ASPECTS: &'static [&'static str] = &[
        "api-metadata",
        "custom-service-entity",
        "owned-contract-prefix",
        "registration",
        "registry-contract",
        "route-handler-only",
        "service-owner",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let options = run.options();
            let names = Names {
                automator: layout.automator_package.clone(),
                nest_common: layout.nest_common_package.clone(),
                nest_routes: options.nest_route_decorators.clone(),
                automator_route: pattern::compile(
                    "automator-route-pattern",
                    &options.automator_route_pattern,
                )?,
                generated_routes: layout.generated_routes.clone(),
                service_member: options.service_member.clone(),
            };
            let external = format!(
                "{}{}/",
                folder_prefix(&layout.modules_root),
                options.external_noun_plane
            );
            let contracts = options.controllers.clone();
            let registry = run
                .options_file()
                .map_or_else(|| "gate.toml".to_owned(), str::to_owned);
            let typescript = shared.typescript()?;
            let sources = Sources::new(typescript, &layout.source_root)?;
            let mut findings = Findings::default();

            contract::validate(&contracts, &external, |key, message| {
                findings.push_keyed("registry-contract", registry.as_str(), None, key, message);
            });

            let mut by_path: BTreeMap<&str, Contract<'_>> = BTreeMap::new();

            for fields in &contracts {
                let contract = Contract::new(fields);

                if let Some(path) = contract.path().string() {
                    by_path.insert(path, contract);
                }
            }

            let mut discovered: BTreeSet<&str> = BTreeSet::new();

            for source in sources.iter() {
                let controllers: Vec<(&Statement<'_>, &Class<'_>)> = top_level(&source.file)
                    .filter_map(|statement| Some((statement, class_declaration(statement)?)))
                    .filter(|(_, class)| controller(source, class, &names))
                    .collect();

                if controllers.is_empty() {
                    continue;
                }

                discovered.insert(&source.path);
                let Some(contract) = by_path.get(source.path.as_str()).copied() else {
                    unregistered(&mut findings, source, &controllers);
                    continue;
                };
                let matching: Vec<_> = controllers
                    .iter()
                    .filter(|(_, class)| {
                        class
                            .id
                            .as_ref()
                            .is_some_and(|id| contract.controller_name() == Js::Str(&id.name))
                    })
                    .collect();

                let ([only], 1) = (matching.as_slice(), controllers.len()) else {
                    findings.push_keyed(
                        "registration",
                        source.path.as_str(),
                        None,
                        format!("{}#declared", contract.controller_name().text()),
                        format!(
                            "{} must declare exactly one registered controller {}",
                            source.path,
                            contract.controller_name().text()
                        ),
                    );
                    continue;
                };

                Owned {
                    sources: &sources,
                    contract,
                    names: &names,
                    findings: &mut findings,
                }
                .controller(source, only.0, only.1);
            }

            stale(&contracts, &discovered, &sources, &registry, &mut findings);
            findings.report(run, help);
            Ok(())
        })
    }
}

/// Reports each row whose path names no discovered controller; a registered file that does not
/// parse is reported by `typescript/parse`, not as stale.
/// One `registration` finding per controller class of `source`, a file no registry row names.
fn unregistered(
    findings: &mut Findings,
    source: &SourceContext<'_>,
    controllers: &[(&Statement<'_>, &Class<'_>)],
) {
    for (statement, class) in controllers {
        let line = source.file.line(class_start(statement, class));
        let name = class
            .id
            .as_ref()
            .map_or("<anonymous>", |id| id.name.as_str());
        findings.push_keyed(
            "registration",
            source.path.as_str(),
            Some(line),
            name,
            format!(
                "{}:{line} {name} is an unregistered controller semantic owner",
                source.path
            ),
        );
    }
}

fn stale(
    contracts: &[ControllerRow],
    discovered: &BTreeSet<&str>,
    sources: &Sources<'_>,
    registry: &str,
    findings: &mut Findings,
) {
    for fields in contracts {
        let contract = Contract::new(fields);
        let path = contract.path();

        if !path
            .string()
            .is_some_and(|path| discovered.contains(path) || sources.unparsed(path))
        {
            findings.push_keyed(
                "registration",
                registry,
                None,
                format!("{}#stale", path.text()),
                format!(
                    "{} is a stale semantic-owner registry row for {}",
                    path.text(),
                    contract.controller_name().text()
                ),
            );
        }
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "registration" | "registry-contract" => {
            "register each controller once in the controllers option, and remove rows of removed controllers"
        }
        "route-handler-only" => "move helper logic to the controller's semantic owner",
        "owned-contract-prefix" => "name the owned contract after its semantic owner",
        "service-owner" => "inject the registered primary service once, bound to its entity",
        "custom-service-entity" => {
            "disable the generated routes and load no relations in a controller whose service binds another entity"
        }
        _ => "make the controller's metadata match its registry row",
    }
}

/// The line of a top-level class as the compiler starts it: its first decorator or `export`.
fn class_line(source: &SourceContext<'_>, class: &Class<'_>) -> u32 {
    let statement_start = top_level(&source.file)
        .find(|statement| {
            class_declaration(statement).is_some_and(|candidate| std::ptr::eq(candidate, class))
        })
        .map_or(class.span.start, |statement| statement.span().start);

    source.file.line(decorated_start(
        statement_start.min(class.span.start),
        &class.decorators,
    ))
}

/// Whether a top-level class is a controller.
fn controller(source: &SourceContext<'_>, class: &Class<'_>, names: &Names) -> bool {
    class
        .id
        .as_ref()
        .is_some_and(|id| id.name.ends_with("Controller"))
        || decorator_call(source, &class.decorators, &names.automator, "ApiController").is_some()
        || decorator_call(source, &class.decorators, &names.nest_common, "Controller").is_some()
}

/// Whether a method carries an automator route or Nest route decorator, called or not.
pub(super) fn route_handler(
    source: &SourceContext<'_>,
    decorators: &[Decorator<'_>],
    names: &Names,
) -> bool {
    decorators.iter().any(|decorator| {
        decorator_identity(source, decorator)
            .0
            .is_some_and(|identity| {
                let module = identity.module.as_deref();

                if module == Some(names.automator.as_str()) {
                    names.automator_route.is_match(identity.name.as_str())
                } else if module == Some(names.nest_common.as_str()) {
                    names.nest_routes.contains(&identity.name)
                } else {
                    false
                }
            })
    })
}

/// The checks of one registered controller and its primary service.
struct Owned<'v, 's, 'a, 'n, 'f> {
    sources: &'s Sources<'a>,
    contract: Contract<'v>,
    names: &'n Names,
    findings: &'f mut Findings,
}

impl<'a> Owned<'_, '_, 'a, '_, '_> {
    #[expect(
        clippy::too_many_lines,
        reason = "the checks run in one order that each early return keeps"
    )]
    fn controller(
        &mut self,
        source: &SourceContext<'a>,
        statement: &Statement<'a>,
        class: &Class<'a>,
    ) {
        let contract = self.contract;
        let names = self.names;
        let api_controller =
            decorator_call(source, &class.decorators, &names.automator, "ApiController");
        let nest_controller =
            decorator_call(source, &class.decorators, &names.nest_common, "Controller");
        let line = source.file.line(class_start(statement, class));
        let name = contract.controller_name().text();
        let label = format!("{}:{line} {name}", source.path);
        let path = source.path.clone();
        let service_entity_name = contract.service_entity_name();
        let service_entity_path = contract.service_entity_path();
        let distinct = service_entity_name != contract.entity_name()
            || service_entity_path != contract.entity_path();
        let mut error = |aspect: &'static str, check: &str, message: String| {
            self.findings.push_keyed(
                aspect,
                path.as_str(),
                Some(line),
                format!("{name}#{check}"),
                message,
            );
        };

        if contract.plain() {
            if nest_controller.is_none() || api_controller.is_some() {
                error(
                    "api-metadata",
                    "plain",
                    format!("{label} must be one exact plain Nest controller"),
                );
            }
        } else if api_controller.is_none() || nest_controller.is_some() {
            error(
                "api-metadata",
                "api-controller",
                format!("{label} must be one exact ApiController"),
            );
        }

        let constructors: Vec<_> = class
            .body
            .body
            .iter()
            .filter_map(|element| match element {
                ClassElement::MethodDefinition(method) if constructor(method) => {
                    Some(method.as_ref())
                }
                _ => None,
            })
            .collect();
        let service_name = contract.primary_service_name();
        let service_path = contract.primary_service_path();
        let injected = |parameter: &&FormalParameter<'a>| {
            same_name(
                self.sources
                    .canonical(type_identity(source, parameter_type(parameter)).as_ref())
                    .as_ref(),
                service_name,
            )
        };

        if let [constructor] = constructors.as_slice() {
            let services: Vec<_> = constructor
                .value
                .params
                .items
                .iter()
                .filter(injected)
                .collect();

            if let [service] = services.as_slice() {
                let identity = self
                    .sources
                    .canonical(type_identity(source, parameter_type(service)).as_ref());

                if !same_path(identity.as_ref(), service_path) {
                    error(
                        "service-owner",
                        "service-path",
                        format!(
                            "{label} resolves primary service to {}; expected {}",
                            identity
                                .and_then(|identity| identity.path)
                                .unwrap_or_else(|| "<unresolved>".to_owned()),
                            service_path.text()
                        ),
                    );
                }

                if source.text(service.pattern.span()) != names.service_member {
                    error(
                        "service-owner",
                        "service-member",
                        format!(
                            "{label} primary controller service member must be named {}",
                            names.service_member
                        ),
                    );
                }
            } else {
                error(
                    "service-owner",
                    "service-injection",
                    format!(
                        "{label} must inject primary service {} exactly once; expected {}",
                        service_name.text(),
                        service_path.text()
                    ),
                );
            }
        } else {
            error(
                "service-owner",
                "constructor",
                format!("{label} must declare exactly one constructor"),
            );
        }

        members::route_handlers(source, class, names, self.findings);
        members::owned_contracts(
            source,
            class,
            &name,
            contract.semantic_owner_name(),
            self.findings,
        );

        let Some(call) = api_controller.filter(|_| !contract.plain()) else {
            return;
        };
        let mut error = |aspect: &'static str, check: &str, message: String| {
            self.findings.push_keyed(
                aspect,
                path.as_str(),
                Some(line),
                format!("{name}#{check}"),
                message,
            );
        };
        let Some([entity_type]) = call
            .type_arguments
            .as_ref()
            .map(|arguments| arguments.params.as_slice())
        else {
            error(
                "api-metadata",
                "entity-generic",
                format!("{label} ApiController must bind exactly one entity generic"),
            );
            return;
        };
        let controller_entity = self
            .sources
            .canonical(type_identity(source, Some(entity_type)).as_ref());

        if !same_name(controller_entity.as_ref(), contract.entity_name())
            || !same_path(controller_entity.as_ref(), contract.entity_path())
        {
            error(
                "api-metadata",
                "entity",
                format!(
                    "{label} binds ApiController entity {}; expected {} from {}, received {}",
                    controller_entity
                        .as_ref()
                        .map_or("<unresolved>", |owner| owner.name.as_str()),
                    contract.entity_name().text(),
                    contract.entity_path().text(),
                    controller_entity
                        .as_ref()
                        .and_then(|owner| owner.path.as_deref())
                        .unwrap_or("<unresolved>")
                ),
            );
        }

        let bases: Vec<Option<Owner>> = class
            .implements
            .iter()
            .filter_map(|implements| {
                let identity = source.type_identity(&implements.expression)?;
                let arguments = implements.type_arguments.as_ref()?;

                (identity.module.as_deref() == Some(names.automator.as_str())
                    && identity.name == "IApiControllerBase"
                    && arguments.params.len() == 1)
                    .then(|| {
                        self.sources
                            .canonical(type_identity(source, arguments.params.first()).as_ref())
                    })
            })
            .collect();

        if !matches!(bases.as_slice(), [base] if same_name(base.as_ref(), service_entity_name)
            && same_path(base.as_ref(), service_entity_path))
        {
            error(
                "api-metadata",
                "base",
                format!(
                    "{label} must implement IApiControllerBase<{}> exactly once",
                    service_entity_name.text()
                ),
            );
        }

        let configuration = call.arguments.first().and_then(Argument::as_expression);
        let config = configuration.and_then(|expression| object_literal(source, expression));

        if distinct {
            members::custom_service_boundary(
                source,
                class,
                config,
                &label,
                line,
                names,
                self.findings,
            );
        }

        let mut error = |aspect: &'static str, check: &str, message: String| {
            self.findings.push_keyed(
                aspect,
                path.as_str(),
                Some(line),
                format!("{name}#{check}"),
                message,
            );
        };

        if let Some(Configured::Entity(entity)) =
            configuration.map(|expression| configuration_type(source, expression, &names.automator))
        {
            let owner = self.sources.canonical(entity.as_ref());

            if !same_name(owner.as_ref(), contract.entity_name())
                || !same_path(owner.as_ref(), contract.entity_path())
            {
                error(
                    "api-metadata",
                    "config-type",
                    format!(
                        "{label} config type must be IApiControllerProperties<{}>",
                        contract.entity_name().text()
                    ),
                );
            }
        }

        if let Some(config) = config {
            let entity = object_property(config, "entity").and_then(|expression| {
                self.sources
                    .canonical(source.identity(expression, true).as_ref())
            });
            // `semanticOwnerName ?? controllerName.slice(0, -"Controller".length)`: the slice drops
            // the last ten characters whether or not they spell `Controller`, and a value that is
            // no string equals no name.
            let units: Vec<u16> = name.encode_utf16().collect();
            let kept = units.len().saturating_sub("Controller".len());
            let sliced = String::from_utf16_lossy(&units[..kept]);
            let expected = match contract.semantic_owner_name() {
                Js::Undefined | Js::Null => Value::String(sliced),
                Js::Str(text) => Value::String(text.to_owned()),
                Js::Other(value) => value.clone(),
            };

            if !same_name(entity.as_ref(), contract.entity_name())
                || !same_path(entity.as_ref(), contract.entity_path())
            {
                error(
                    "api-metadata",
                    "config-entity",
                    format!(
                        "{label} config entity must be {}",
                        contract.entity_name().text()
                    ),
                );
            }

            if !matches!((object_property(config, "name"), &expected), (Some(Expression::StringLiteral(literal)), Value::String(expected)) if literal.value == expected.as_str())
            {
                error(
                    "api-metadata",
                    "config-name",
                    format!(
                        "{label} config name must be {}",
                        Js::of(Some(&expected)).json()
                    ),
                );
            }
        } else {
            error(
                "api-metadata",
                "config",
                format!("{label} ApiController configuration must resolve to one object literal"),
            );
        }

        self.service(
            source,
            &constructors,
            &label,
            line,
            controller_entity.as_ref(),
            distinct,
        );
    }

    /// The primary service checks, which start from the first constructor's first parameter that
    /// injects it.
    fn service(
        &mut self,
        source: &SourceContext<'a>,
        constructors: &[&MethodDefinition<'a>],
        label: &str,
        line: u32,
        controller_entity: Option<&Owner>,
        distinct: bool,
    ) {
        let contract = self.contract;
        let service_name = contract.primary_service_name();
        let service_entity_name = contract.service_entity_name();
        let service_entity_path = contract.service_entity_path();
        let identity = constructors.first().and_then(|constructor| {
            constructor.value.params.items.iter().find_map(|parameter| {
                let owner = self
                    .sources
                    .canonical(type_identity(source, parameter_type(parameter)).as_ref());

                same_name(owner.as_ref(), service_name).then_some(owner)
            })
        });
        let service = identity
            .flatten()
            .and_then(|owner| owner.path)
            .and_then(|path| self.sources.get(&path));
        let class = service
            .zip(service_name.string())
            .and_then(|(service, name)| {
                find_class(&service.file, name).map(|class| (service, class))
            });
        let Some((service, class)) = class else {
            self.findings.push_keyed(
                "service-owner",
                source.path.as_str(),
                Some(line),
                format!("{}#service-class", contract.controller_name().text()),
                format!(
                    "{label} primary service {} must resolve to one source class",
                    service_name.text()
                ),
            );
            return;
        };
        let entities = service_entities(service, class, &self.names.automator);

        if entities.is_empty() {
            self.findings.push_keyed(
                "service-owner",
                source.path.as_str(),
                Some(line),
                format!("{}#service-entities", contract.controller_name().text()),
                format!(
                    "{label} primary service {} must bind {} through ApiServiceBase or ApiService",
                    service_name.text(),
                    service_entity_name.text()
                ),
            );
        }

        let service_line = class_line(service, class);

        for (identity, owner_kind) in &entities {
            let owner = self.sources.canonical(identity.as_ref());

            if !same_name(owner.as_ref(), service_entity_name)
                || !same_path(owner.as_ref(), service_entity_path)
            {
                self.findings.push_keyed(
                    "service-owner",
                    service.path.as_str(),
                    Some(service_line),
                    format!("{}#{owner_kind}-entity", service_name.text()),
                    format!(
                        "{}:{service_line} {} {owner_kind} entity is {}; expected {} from {}, received {}",
                        service.path,
                        service_name.text(),
                        owner.as_ref().map_or("<unresolved>", |owner| owner.name.as_str()),
                        service_entity_name.text(),
                        service_entity_path.text(),
                        owner
                            .as_ref()
                            .and_then(|owner| owner.path.as_deref())
                            .unwrap_or("<unresolved>")
                    ),
                );
            }

            if !distinct
                && let Some(controller_entity) = controller_entity
                && identity.is_some()
                && owner.as_ref().is_none_or(|owner| {
                    controller_entity.path != owner.path || controller_entity.name != owner.name
                })
            {
                self.findings.push_keyed(
                    "service-owner",
                    source.path.as_str(),
                    Some(line),
                    format!("{}#service-entity-owner", contract.controller_name().text()),
                    format!(
                        "{label} and primary service {} resolve {} from different source owners",
                        service_name.text(),
                        contract.entity_name().text()
                    ),
                );
            }
        }

        members::owned_contracts(
            service,
            class,
            &service_name.text(),
            Js::Undefined,
            self.findings,
        );
    }
}

/// The entity of a configuration variable typed `IApiControllerProperties`.
enum Configured {
    /// The configuration is no such variable (`undefined`).
    Untyped,
    /// The identity of its one type argument; `None` (`null`) for another arity.
    Entity(Option<Identity>),
}

fn configuration_type(
    source: &SourceContext<'_>,
    expression: &Expression<'_>,
    automator: &str,
) -> Configured {
    configured(source, expression, automator).map_or(Configured::Untyped, Configured::Entity)
}

/// `Some(entity)` for a configuration variable typed `IApiControllerProperties`.
#[expect(
    clippy::option_option,
    reason = "the outer level is the JavaScript undefined, the inner one its null"
)]
fn configured(
    source: &SourceContext<'_>,
    expression: &Expression<'_>,
    automator: &str,
) -> Option<Option<Identity>> {
    let Expression::Identifier(identifier) = unwrap(expression) else {
        return None;
    };
    let annotation = source
        .variables
        .get(identifier.name.as_str())?
        .type_annotation
        .as_ref()?;
    let kind = &annotation.type_annotation;
    let identity = type_identity(source, Some(kind))?;

    if identity.module.as_deref() != Some(automator) || identity.name != "IApiControllerProperties"
    {
        return None;
    }

    Some(match kind {
        TSType::TSTypeReference(reference) => match reference
            .type_arguments
            .as_ref()
            .map(|arguments| arguments.params.as_slice())
        {
            Some([argument]) => type_identity(source, Some(argument)),
            _ => None,
        },
        _ => None,
    })
}

/// The entity bindings of a primary service: `extends ApiServiceBase<E>` and the `entity` of an
/// `ApiService(...)` call with one argument, in that order.
fn service_entities<'a>(
    source: &SourceContext<'a>,
    class: &Class<'a>,
    automator: &str,
) -> Vec<(Option<Identity>, &'static str)> {
    let mut entities = Vec::new();

    if let Some(heritage) = &class.heritage
        && let Some(identity) = source.identity(&heritage.expression, true)
        && identity.module.as_deref() == Some(automator)
        && identity.name == "ApiServiceBase"
        && let Some([argument]) = heritage
            .type_arguments
            .as_ref()
            .map(|arguments| arguments.params.as_slice())
    {
        entities.push((type_identity(source, Some(argument)), "base"));
    }

    if let Some(call) = decorator_call(source, &class.decorators, automator, "ApiService")
        && let [argument] = call.arguments.as_slice()
    {
        let entity = argument
            .as_expression()
            .and_then(|expression| object_literal(source, expression))
            .and_then(|object| object_property(object, "entity"))
            .and_then(|expression| source.identity(expression, true));
        entities.push((entity, "decorator"));
    }

    entities
}

/// `X.NAME` of a computed route key whose `X` is the automator's `EApiRouteType`.
pub(super) fn route_type(
    source: &SourceContext<'_>,
    key: &Expression<'_>,
    automator: &str,
) -> Option<String> {
    let (object, name) = property_access(unwrap(key))?;
    let identity = source.identity(object, true)?;

    (identity.module.as_deref() == Some(automator) && identity.name == "EApiRouteType")
        .then(|| name.to_owned())
}
