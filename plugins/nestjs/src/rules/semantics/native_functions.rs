//! `nestjs/native-functions`: every asynchronous method of a native service is one automator
//! function.
//!
//! A native service is a top-level class that `nestjs/native-repository` reads as one: it carries
//! the automator's service decorator call, extends its service base or extends a native service.
//! Each one carries its own service decorator call whose options name the entity in an object
//! literal, and a class of a service file whose constructor injects a repository through the
//! repository injection decorator is one (`api-service`); its members are checked as a native
//! service's.
//!
//! Every non-private asynchronous method of a native service carries exactly one automator function
//! decorator (`undecorated-method`, `function-decorator-count`). A method is asynchronous when it is
//! `async`, declares a `Promise` return type or declares none, since a non-`async` method without a
//! return type cannot be told from one. Constructors, accessors, overload signatures, abstract
//! methods, the lifecycle hooks and the inherited native functions are exempt; a non-private static
//! method and a non-private function-valued property of a native service are reported too,
//! synchronous or not, because neither can be an automator function.
//!
//! Every automator function decorator names `transaction: { mode }` in an object literal
//! (`transaction-mode`), sits on an asynchronous method (`synchronous-function`) of a native service
//! (`function-owner`), and the custom function decorator never sits on a private method
//! (`private-custom`).

use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Report, Run};
use gate_typescript::owners::{
    SourceContext, call_expression, class_declaration, top_level, unwrap,
};
use gate_typescript::oxc_ast::ast::{
    Argument, CallExpression, Class, ClassElement, Decorator, Expression, MethodDefinition,
    MethodDefinitionKind, PropertyKey, TSAccessibility, TSType, TSTypeName,
};
use gate_typescript::syntax::{
    class_start, constructor, decorator_identity, key_name, member_name, member_start, method_kind,
    object_literal, object_property,
};

use super::native_repository::Service;
use crate::layout::PluginOptions;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// A decorator of a package.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct PackageDecorator {
    /// The package.
    pub package: String,
    /// The decorator.
    pub decorator: String,
}

/// The options of `nestjs/native-functions`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The package of the service and function decorators. Absent means
    /// `layout.automator-package`.
    pub framework_package: Option<String>,
    /// The class decorator that makes a class a native service.
    pub service_decorator: String,
    /// The base class that makes a class a native service.
    pub service_base: String,
    /// The automator function decorators.
    pub function_decorators: Vec<String>,
    /// The function decorator that never sits on a private method.
    pub custom_function_decorator: String,
    /// The lifecycle hooks the framework calls, exempt from being functions.
    pub lifecycle_methods: Vec<String>,
    /// The native functions a service inherits from the base, exempt from being functions.
    pub inherited_methods: Vec<String>,
    /// The service decorator option that names the entity.
    pub entity_option: String,
    /// The path of object keys a function decorator's options must name, as `transaction`,
    /// `mode` for `transaction: { mode }`.
    pub transaction_option: Vec<String>,
    /// The names of service files: an exact file name, or a suffix when it starts with `.`.
    pub service_file_names: Vec<String>,
    /// The parameter decorator that injects a repository.
    pub inject_repository: PackageDecorator,
}

impl Default for Options {
    fn default() -> Self {
        let strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();

        Self {
            framework_package: None,
            service_decorator: "ApiService".to_owned(),
            service_base: "ApiServiceBase".to_owned(),
            function_decorators: strings(&["ApiFunctionCustom", "ApiFunctionStep"]),
            custom_function_decorator: "ApiFunctionCustom".to_owned(),
            lifecycle_methods: strings(&[
                "beforeApplicationShutdown",
                "onApplicationBootstrap",
                "onApplicationShutdown",
                "onModuleDestroy",
                "onModuleInit",
            ]),
            inherited_methods: strings(&[
                "create", "delete", "get", "getList", "getMany", "update",
            ]),
            entity_option: "entity".to_owned(),
            transaction_option: strings(&["transaction", "mode"]),
            service_file_names: strings(&["service.ts", ".service.ts"]),
            inject_repository: PackageDecorator {
                package: "@nestjs/typeorm".to_owned(),
                decorator: "InjectRepository".to_owned(),
            },
        }
    }
}

/// The options a run reads, with the package resolved.
struct Settings {
    service: Service,
    options: Options,
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "native-functions";
    const TITLE: &'static str =
        "Every asynchronous method of a native service is one Crud Automator function";
    const ASPECTS: &'static [&'static str] = &[
        "api-service",
        "function-decorator-count",
        "function-owner",
        "private-custom",
        "synchronous-function",
        "transaction-mode",
        "undecorated-method",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let options = run.options();

            if options.function_decorators.is_empty() {
                return Err(gate_sdk::Error::config(
                    "`function-decorators` is empty; name at least one automator function decorator",
                ));
            }

            let settings = Settings {
                service: Service {
                    package: super::package::resolve(
                        "framework-package",
                        options.framework_package.as_deref(),
                        &layout.automator_package,
                    )?,
                    decorator: options.service_decorator.clone(),
                    base: options.service_base.clone(),
                },
                options: options.clone(),
            };
            let sources = shared.contexts(&layout.source_root)?;
            let report = run.report();

            for source in sources.iter() {
                for statement in top_level(&source.file) {
                    let Some(class) = class_declaration(statement) else {
                        continue;
                    };
                    let name = class
                        .id
                        .as_ref()
                        .map_or("<anonymous>", |id| id.name.as_str());
                    let line = source.file.line(class_start(statement, class));
                    let path = source.path.as_str();
                    let is_native = settings
                        .service
                        .native(&sources, source, class, &HashSet::new());
                    let repository_service = !is_native
                        && settings.service_file(path)
                        && settings.injects_repository(source, class);

                    if is_native
                        && !class
                            .decorators
                            .iter()
                            .any(|decorator| settings.entity_service_decorator(source, decorator))
                    {
                        report
                            .finding(
                                "api-service",
                                at(path, line),
                                format!(
                                    "{path}:{line} {name} is a native service without its own Crud Automator {} decorator that names its entity",
                                    settings.service.decorator
                                ),
                            )
                            .help(format!(
                                "decorate the class with {}({{ {}: ... }})",
                                settings.service.decorator, settings.options.entity_option
                            ))
                            .key(name);
                    }

                    if repository_service {
                        report
                            .finding(
                                "api-service",
                                at(path, line),
                                format!(
                                    "{path}:{line} {name} injects a repository through {} but is no Crud Automator {}",
                                    settings.options.inject_repository.decorator,
                                    settings.service.decorator
                                ),
                            )
                            .help(format!(
                                "decorate the class with {}({{ {}: ... }})",
                                settings.service.decorator, settings.options.entity_option
                            ))
                            .key(name);
                    }

                    let native_members = is_native || repository_service;

                    for element in &class.body.body {
                        settings.member(source, report, name, native_members, element);
                    }
                }
            }

            Ok(())
        })
    }
}

/// A finding location at a line.
fn at(path: &str, line: u32) -> Location {
    Location::line(path, usize::try_from(line).unwrap_or(usize::MAX))
}

/// What a member of a native service must be.
enum Requirement {
    /// Nothing: a private, exempt or synchronous member.
    None,
    /// One automator function.
    Function,
    /// An instance method instead: a static method or a function-valued property.
    Unfit(&'static str),
}

impl Settings {
    /// The findings of one class member.
    fn member<'a>(
        &self,
        source: &SourceContext<'a>,
        report: &mut Report,
        class: &str,
        is_native: bool,
        element: &ClassElement<'a>,
    ) {
        let decorators: &[Decorator<'a>] = match element {
            ClassElement::MethodDefinition(method) => &method.decorators,
            ClassElement::PropertyDefinition(property) => &property.decorators,
            ClassElement::AccessorProperty(property) => &property.decorators,
            ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => return,
        };
        let label = format!("{class}.{}", member_name(source, element));
        let path = source.path.as_str();
        let functions: Vec<(&Decorator<'a>, &str, Option<&CallExpression<'a>>)> = decorators
            .iter()
            .filter_map(|decorator| {
                let (identity, call) = decorator_identity(source, decorator);
                let identity = identity?;

                if identity.module.as_deref() != Some(self.service.package.as_str()) {
                    return None;
                }

                self.options
                    .function_decorators
                    .iter()
                    .find(|function| **function == identity.name)
                    .map(|function| (decorator, function.as_str(), call))
            })
            .collect();
        let custom = self.options.custom_function_decorator.as_str();

        for (index, (decorator, function, call)) in functions.iter().enumerate() {
            let line = source.file.line(decorator.span.start);

            if !is_native {
                report
                    .finding(
                        "function-owner",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} carries {function} outside a native Crud Automator service"
                        ),
                    )
                    .help("move the method into a native service, or remove the decorator")
                    .key(format!("{label}#{function}"));
            }

            if index > 0 {
                report
                    .finding(
                        "function-decorator-count",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} carries more than one Crud Automator function decorator"
                        ),
                    )
                    .help("keep one function decorator on the method")
                    .key(format!("{label}#{function}"));
            }

            if !self.explicit_mode(source, *call) {
                report
                    .finding(
                        "transaction-mode",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} carries {function} without an explicit transaction: {{ mode }}"
                        ),
                    )
                    .help("name the transaction mode in the decorator's options")
                    .key(format!("{label}#{function}"));
            }

            if !matches!(element, ClassElement::MethodDefinition(method) if asynchronous_method(method))
            {
                report
                    .finding(
                        "synchronous-function",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} carries {function} but is no asynchronous method, and the decorator makes every call return a Promise"
                        ),
                    )
                    .help("make the method asynchronous, or remove the decorator")
                    .key(format!("{label}#{function}"));
            }

            if *function == custom && private(element) {
                let step = self
                    .options
                    .function_decorators
                    .iter()
                    .find(|other| other.as_str() != custom)
                    .map_or("a step function", String::as_str);

                report
                    .finding(
                        "private-custom",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} is private and carries {custom}; a participant inside its owner is an {step}"
                        ),
                    )
                    .help(format!("decorate the private participant with {step}"))
                    .key(format!("{label}#{function}"));
            }
        }

        if is_native {
            self.shape(source, report, &label, element, functions.is_empty());
        }
    }

    /// The finding of a native service's member that is no function where it must be one, or
    /// cannot be one.
    fn shape(
        &self,
        source: &SourceContext<'_>,
        report: &mut Report,
        label: &str,
        element: &ClassElement<'_>,
        undecorated: bool,
    ) {
        let path = source.path.as_str();
        let line = source.file.line(member_start(element));

        match self.requirement(element) {
            Requirement::Function if undecorated => {
                report
                    .finding(
                        "undecorated-method",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} is a non-private asynchronous method of a native service without {}",
                            self.options.function_decorators.join(" or ")
                        ),
                    )
                    .help("decorate the method with one automator function decorator, or make it private")
                    .key(format!("{label}#async"));
            }
            Requirement::None | Requirement::Function => {}
            Requirement::Unfit(kind) => {
                report
                    .finding(
                        "undecorated-method",
                        at(path, line),
                        format!(
                            "{path}:{line} {label} is a non-private {kind} of a native service, which cannot be a Crud Automator function"
                        ),
                    )
                    .help("make it an instance method with a function decorator, or make it private")
                    .key(format!("{label}#unfit"));
            }
        }
    }

    /// What a member of a native service must be.
    fn requirement(&self, element: &ClassElement<'_>) -> Requirement {
        if private(element) {
            return Requirement::None;
        }

        match element {
            ClassElement::MethodDefinition(method) => {
                if method_kind(method) != MethodDefinitionKind::Method
                    || method.value.body.is_none()
                {
                    Requirement::None
                } else if method.r#static {
                    Requirement::Unfit("static method")
                } else if key_name(&method.key, method.computed).is_some_and(|name| {
                    self.options.lifecycle_methods.contains(&name)
                        || self.options.inherited_methods.contains(&name)
                }) || !asynchronous(method)
                {
                    Requirement::None
                } else {
                    Requirement::Function
                }
            }
            ClassElement::PropertyDefinition(property)
                if property.value.as_ref().is_some_and(|value| {
                    matches!(
                        unwrap(value),
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    )
                }) =>
            {
                Requirement::Unfit("function-valued property")
            }
            _ => Requirement::None,
        }
    }

    /// Whether a function decorator's options name the transaction option path in object
    /// literals.
    fn explicit_mode<'a>(
        &self,
        source: &SourceContext<'a>,
        call: Option<&CallExpression<'a>>,
    ) -> bool {
        let Some(mut current) = call
            .and_then(|call| call.arguments.first())
            .and_then(Argument::as_expression)
        else {
            return false;
        };

        for key in &self.options.transaction_option {
            let Some(next) =
                object_literal(source, current).and_then(|object| object_property(object, key))
            else {
                return false;
            };
            current = next;
        }

        true
    }

    /// Whether a class decorator is a service decorator call whose options name the entity in an
    /// object literal, directly or through a top-level variable.
    fn entity_service_decorator<'a>(
        &self,
        source: &SourceContext<'a>,
        decorator: &Decorator<'a>,
    ) -> bool {
        self.service.decorator(source, decorator)
            && call_expression(&decorator.expression)
                .and_then(|call| call.arguments.first())
                .and_then(Argument::as_expression)
                .and_then(|options| object_literal(source, options))
                .is_some_and(|options| {
                    object_property(options, &self.options.entity_option).is_some()
                })
    }

    /// Whether a path is a service file.
    fn service_file(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);

        self.options.service_file_names.iter().any(|service| {
            if service.starts_with('.') {
                name.ends_with(service.as_str())
            } else {
                name == service
            }
        })
    }

    /// Whether a constructor of the class injects a repository through the injection decorator.
    fn injects_repository(&self, source: &SourceContext<'_>, class: &Class<'_>) -> bool {
        let inject = &self.options.inject_repository;

        class.body.body.iter().any(|element| {
            let ClassElement::MethodDefinition(method) = element else {
                return false;
            };

            constructor(method)
                && method.value.params.items.iter().any(|parameter| {
                    parameter.decorators.iter().any(|decorator| {
                        decorator_identity(source, decorator)
                            .0
                            .is_some_and(|identity| {
                                identity.module.as_deref() == Some(inject.package.as_str())
                                    && identity.name == inject.decorator
                            })
                    })
                })
        })
    }
}

/// Whether a member is private: `private` or a `#` name.
fn private(element: &ClassElement<'_>) -> bool {
    let (accessibility, key) = match element {
        ClassElement::MethodDefinition(method) => (method.accessibility, &method.key),
        ClassElement::PropertyDefinition(property) => (property.accessibility, &property.key),
        ClassElement::AccessorProperty(property) => (property.accessibility, &property.key),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => return false,
    };

    accessibility == Some(TSAccessibility::Private)
        || matches!(key, PropertyKey::PrivateIdentifier(_))
}

/// Whether a method returns a Promise: `async`, a `Promise` return type or none declared.
fn asynchronous(method: &MethodDefinition<'_>) -> bool {
    method.value.r#async
        || method.value.return_type.as_ref().is_none_or(|annotation| {
            matches!(
                &annotation.type_annotation,
                TSType::TSTypeReference(reference)
                    if matches!(&reference.type_name, TSTypeName::IdentifierReference(name) if name.name == "Promise")
            )
        })
}

/// Whether a member is an asynchronous method a function decorator can wrap.
fn asynchronous_method(method: &MethodDefinition<'_>) -> bool {
    method_kind(method) == MethodDefinitionKind::Method && asynchronous(method)
}
