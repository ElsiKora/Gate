//! `nestjs/native-repository`: a native service's constructor-injected repository is a `protected
//! readonly` parameter property.
//!
//! A top-level class of a source file is a native service when it carries a service decorator call
//! of the automator package, extends the package's service base class, or extends a top-level class
//! that is one, followed across files through the resolver's canonical owners. Every repository
//! parameter property of every one of its constructors must carry both `protected` and `readonly`;
//! a plain repository argument forwarded to the base class is no parameter property.

use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::owners::{
    SourceContext, Sources, call_expression, class_declaration, find_class, top_level,
};
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Decorator, TSAccessibility,
};
use gate_typescript::syntax::constructor;

use crate::layout::PluginOptions;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/native-repository`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The package of the service decorator and base class. Absent means
    /// `layout.automator-package`.
    pub framework_package: Option<String>,
    /// The class decorator that makes a class a native service.
    pub service_decorator: String,
    /// The base class that makes a class a native service.
    pub service_base: String,
    /// The constructor parameter that holds the service's repository.
    pub repository_parameter: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            framework_package: None,
            service_decorator: "ApiService".to_owned(),
            service_base: "ApiServiceBase".to_owned(),
            repository_parameter: "repository".to_owned(),
        }
    }
}

/// What makes a class a native service: a decorator or a base class of a package.
#[derive(Debug)]
pub(crate) struct Service {
    /// The package.
    pub package: String,
    /// The class decorator.
    pub decorator: String,
    /// The base class.
    pub base: String,
}

impl Service {
    /// Whether a class is a native service; `visited` holds the classes already on the chain.
    pub(crate) fn native(
        &self,
        sources: &Sources<'_>,
        source: &SourceContext<'_>,
        class: &Class<'_>,
        visited: &HashSet<String>,
    ) -> bool {
        let key = format!("{}:{}", source.path, class.span.start);

        if visited.contains(&key) {
            return false;
        }

        let mut next = visited.clone();
        next.insert(key);

        if class
            .decorators
            .iter()
            .any(|decorator| self.decorator(source, decorator))
        {
            return true;
        }

        let Some(heritage) = &class.heritage else {
            return false;
        };
        let identity = source.identity(&heritage.expression, false);

        if identity.as_ref().is_some_and(|identity| {
            identity.module.as_deref() == Some(self.package.as_str()) && identity.name == self.base
        }) {
            return true;
        }

        let Some(owner) = sources.canonical(identity.as_ref()) else {
            return false;
        };
        let Some(owner_source) = owner.path.as_deref().and_then(|path| sources.get(path)) else {
            return false;
        };

        find_class(&owner_source.file, &owner.name)
            .is_some_and(|base| self.native(sources, owner_source, base, &next))
    }

    /// Whether a class decorator is a call of the service decorator.
    pub(crate) fn decorator(&self, source: &SourceContext<'_>, decorator: &Decorator<'_>) -> bool {
        call_expression(&decorator.expression).is_some_and(|call| {
            source
                .identity(&call.callee, false)
                .is_some_and(|identity| {
                    identity.module.as_deref() == Some(self.package.as_str())
                        && identity.name == self.decorator
                })
        })
    }
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "native-repository";
    const TITLE: &'static str =
        "A native service's primary repository is a protected readonly parameter property";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let options = run.options();
            let service = Service {
                package: super::package::resolve(
                    "framework-package",
                    options.framework_package.as_deref(),
                    &layout.automator_package,
                )?,
                decorator: options.service_decorator.clone(),
                base: options.service_base.clone(),
            };
            let repository = options.repository_parameter.clone();
            let sources = shared.contexts(&layout.source_root)?;

            for source in sources.iter() {
                for class in top_level(&source.file).filter_map(class_declaration) {
                    if !service.native(&sources, source, class, &HashSet::new()) {
                        continue;
                    }

                    let name = class
                        .id
                        .as_ref()
                        .map_or("<anonymous>", |id| id.name.as_str());

                    for element in &class.body.body {
                        let ClassElement::MethodDefinition(method) = element else {
                            continue;
                        };

                        if !constructor(method) {
                            continue;
                        }

                        for parameter in &method.value.params.items {
                            let BindingPattern::BindingIdentifier(identifier) = &parameter.pattern
                            else {
                                continue;
                            };
                            // A parameter property carries an accessibility, `readonly` or `override`.
                            let property = parameter.accessibility.is_some()
                                || parameter.readonly
                                || parameter.r#override;

                            if identifier.name != repository.as_str()
                                || !property
                                || (parameter.accessibility == Some(TSAccessibility::Protected)
                                    && parameter.readonly)
                            {
                                continue;
                            }

                            let start = parameter
                                .decorators
                                .first()
                                .map_or(parameter.span.start, |decorator| {
                                    decorator.span.start.min(parameter.span.start)
                                });
                            let line = source.file.line(start);

                            run.report()
                                .finding(
                                    "",
                                    Location::line(
                                        source.path.as_str(),
                                        usize::try_from(line).unwrap_or(usize::MAX),
                                    ),
                                    format!(
                                        "{}:{line} {name}.{repository} must be a protected readonly native repository parameter property",
                                        source.path
                                    ),
                                )
                                .help(format!(
                                    "declare the parameter as `protected readonly {repository}`"
                                ))
                                .key(format!("{name}.{repository}"));
                        }
                    }
                }
            }

            Ok(())
        })
    }
}
