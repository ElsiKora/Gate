//! What the rule expects, read from its options and the layout: the paths and names it reads, and
//! the per-member contracts that widen or narrow an inventory entry's expected value.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;

use super::source::Semantic;
use crate::layout::{Layout, folder_prefix};

/// A persisted property hidden from generated DTOs with its exact description type.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Hidden {
    /// The description's `type`, as its source expression reads.
    #[schemars(length(min = 1))]
    pub r#type: String,
    /// Why the property is hidden.
    #[schemars(length(min = 1))]
    pub reason: String,
    /// The decision that settled it.
    pub decision: Option<String>,
}

/// A persisted buffer property hidden from generated DTOs.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Buffer {
    /// Why the property is hidden.
    #[schemars(length(min = 1))]
    pub reason: String,
    /// The decision that settled it.
    pub decision: Option<String>,
}

/// An owner-local decorator that counts as a DTO property's automator decorator, or a
/// documentation decorator a custom handler may carry.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Allowed {
    /// The decorator's name.
    #[schemars(length(min = 1))]
    pub decorator: String,
    /// Why it is allowed.
    #[schemars(length(min = 1))]
    pub reason: String,
    /// The decision that settled it.
    pub decision: Option<String>,
}

/// The exact parameter decorators of a custom handler.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct HandlerParameters {
    /// The decorator names of each parameter, in order; `this` has none.
    pub parameters: Vec<Vec<String>>,
    /// Why the handler takes them.
    #[schemars(length(min = 1))]
    pub reason: String,
    /// The decision that settled it.
    pub decision: Option<String>,
}

/// A helper call that generates the routes of a controller.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct RoutesHelper {
    /// The helper's name.
    pub name: String,
    /// The property of its options object that, when `false`, turns the update routes off.
    pub update_flag: String,
}

/// The options of `nestjs/automator-metadata`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// Entity properties hidden from generated DTOs, by `Entity.property`.
    pub hidden_properties: BTreeMap<String, Hidden>,
    /// Entity buffer properties hidden from generated DTOs, by `Entity.property`.
    pub buffer_properties: BTreeMap<String, Buffer>,
    /// Owner-local decorators that count as a DTO property's decorator, by `DTO.property`.
    pub custom_decorators: BTreeMap<String, Allowed>,
    /// Documentation decorators a custom handler may carry, by `Controller.method`.
    pub documentation_decorators: BTreeMap<String, Vec<Allowed>>,
    /// The exact parameter decorators of custom handlers, by `Controller.method`.
    pub parameter_decorators: BTreeMap<String, HandlerParameters>,
    /// The one handler, `Controller.method`, that may carry `health-check-decorator`.
    pub health_check_route: Option<String>,
    /// The health-check decorator.
    pub health_check_decorator: String,
    /// The helper call that generates a controller's routes; with none, no call does.
    pub routes_helper: Option<RoutesHelper>,
    /// The ending of a domain entity file's path.
    pub entity_file_suffix: String,
    /// The path fragment of a DTO file.
    pub dto_folder: String,
    /// The ending of a DTO class's name.
    pub dto_suffix: String,
    /// The ending of a controller file's path.
    pub controller_file_suffix: String,
    /// The class decorators a controller may carry.
    pub controller_class_decorators: Vec<String>,
    /// The handler decorators every handler may carry.
    pub always_allowed_route_decorators: Vec<String>,
    /// Framework decorators that keep their bare name only when imported from their module.
    pub decorator_modules: BTreeMap<String, String>,
    /// How many bases of an entity class are followed beyond its direct base.
    pub base_depth: u8,
}

impl Default for Options {
    fn default() -> Self {
        let strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();

        Self {
            hidden_properties: BTreeMap::new(),
            buffer_properties: BTreeMap::new(),
            custom_decorators: BTreeMap::new(),
            documentation_decorators: BTreeMap::new(),
            parameter_decorators: BTreeMap::new(),
            health_check_route: None,
            health_check_decorator: "HealthCheck".to_owned(),
            routes_helper: None,
            entity_file_suffix: "/entity/entity.ts".to_owned(),
            dto_folder: "/dto/".to_owned(),
            dto_suffix: "DTO".to_owned(),
            controller_file_suffix: "/controller.ts".to_owned(),
            controller_class_decorators: strings(&[
                "ApiController",
                "ApiControllerObservable",
                "ApiControllerSecurable",
                "ApiTags",
                "Controller",
                "UsePipes",
            ]),
            always_allowed_route_decorators: strings(&["RouteConfig"]),
            decorator_modules: [
                ("ApiBody", "@nestjs/swagger"),
                ("ApiTags", "@nestjs/swagger"),
                ("Controller", "@nestjs/common"),
                ("UsePipes", "@nestjs/common"),
                ("HealthCheck", "@nestjs/terminus"),
                ("RouteConfig", "@nestjs/platform-fastify"),
            ]
            .into_iter()
            .map(|(name, module)| (name.to_owned(), module.to_owned()))
            .collect(),
            base_depth: 8,
        }
    }
}

/// What the rule expects: the options resolved against the layout.
#[derive(Debug)]
pub struct Policy {
    pub automator_package: String,
    pub typeorm_package: String,
    pub validator_package: String,
    pub persisted_decorators: Vec<String>,
    pub generated_routes: Vec<String>,
    pub modules_root: String,
    pub domain_prefix: String,
    pub entity_file_suffix: String,
    pub dto_folder: String,
    pub dto_suffix: String,
    pub controller_file_suffix: String,
    pub controller_class_decorators: Vec<String>,
    pub always_allowed_route_decorators: Vec<String>,
    pub decorator_modules: BTreeMap<String, String>,
    pub health_check_route: Option<String>,
    pub health_check_decorator: String,
    pub routes_helper: Option<RoutesHelper>,
    pub base_depth: u8,
    pub hidden: BTreeMap<String, String>,
    pub buffer: BTreeSet<String>,
    pub decorators: BTreeMap<String, String>,
    pub documentation: BTreeMap<String, Vec<String>>,
    pub parameters: BTreeMap<String, Semantic>,
}

impl Policy {
    pub fn new(options: &Options, layout: &Layout) -> Self {
        let mut persisted = layout.typeorm_column_decorators.clone();
        persisted.extend(layout.typeorm_relation_decorators.iter().cloned());
        persisted.extend(layout.typeorm_other_persisted_decorators.iter().cloned());

        Self {
            automator_package: layout.automator_package.clone(),
            typeorm_package: layout.typeorm_package.clone(),
            validator_package: layout.validator_package.clone(),
            persisted_decorators: persisted,
            generated_routes: layout.generated_routes.clone(),
            modules_root: layout.modules_root.clone(),
            domain_prefix: format!(
                "{}{}/",
                folder_prefix(&layout.modules_root),
                layout.domain_folder
            ),
            entity_file_suffix: options.entity_file_suffix.clone(),
            dto_folder: options.dto_folder.clone(),
            dto_suffix: options.dto_suffix.clone(),
            controller_file_suffix: options.controller_file_suffix.clone(),
            controller_class_decorators: options.controller_class_decorators.clone(),
            always_allowed_route_decorators: options.always_allowed_route_decorators.clone(),
            decorator_modules: options.decorator_modules.clone(),
            health_check_route: options.health_check_route.clone(),
            health_check_decorator: options.health_check_decorator.clone(),
            routes_helper: options.routes_helper.clone(),
            base_depth: options.base_depth,
            hidden: options
                .hidden_properties
                .iter()
                .map(|(key, hidden)| (key.clone(), hidden.r#type.clone()))
                .collect(),
            buffer: options.buffer_properties.keys().cloned().collect(),
            decorators: options
                .custom_decorators
                .iter()
                .map(|(key, allowed)| (key.clone(), allowed.decorator.clone()))
                .collect(),
            documentation: options
                .documentation_decorators
                .iter()
                .map(|(key, allowed)| {
                    let mut names: Vec<String> = Vec::new();

                    for entry in allowed {
                        if !names.contains(&entry.decorator) {
                            names.push(entry.decorator.clone());
                        }
                    }

                    (key.clone(), names)
                })
                .collect(),
            parameters: options
                .parameter_decorators
                .iter()
                .map(|(key, parameters)| {
                    let lists = parameters
                        .parameters
                        .iter()
                        .map(|names| {
                            Semantic::Arr(names.iter().cloned().map(Semantic::Str).collect())
                        })
                        .collect();
                    (key.clone(), Semantic::Arr(lists))
                })
                .collect(),
        }
    }
}
