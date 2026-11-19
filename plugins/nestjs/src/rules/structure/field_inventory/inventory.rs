//! The fields, classes, relations, copies and generated controllers of the tracked `src/` files.
//!
//! An entity is a class decorated with the `typeorm` `@Entity` or `@ChildEntity`; its fields are
//! the `@Column` and `@PrimaryColumn` members of a text type (`varchar`, `text`, `json`, `jsonb`
//! and their kin, or no type over a `string` property), its own and those of the classes it
//! extends that are no entity, each keyed to the class that declares it, and its to-one relations
//! are its `@ManyToOne` and `@OneToOne` members that Automator describes as relations.
//! A DTO is any other class with an Automator property decorator; its fields are the
//! `@ApiPropertyString` members and the open `@ApiPropertyObject` members, and its
//! `@ApiPropertyCopy` members are copies of an entity field. A controller is a class decorated
//! with Automator's `@ApiController`. The DTO classes a route uses are those its `dto` map names,
//! and those an `@ApiRouteCustom` method names in its `dto` map and `response`; an
//! `@ApiPropertyObject` nests the classes its `type`, `itemType`, `discriminator.mapping` and
//! `generatedDTOs` name. A reference the check cannot read is `unreadable`.

use std::collections::{BTreeMap, BTreeSet};

use gate_typescript::owners::{
    Owner, SourceContext, class_declaration, find_class, top_level, unwrap,
};
use gate_typescript::oxc_ast::ast::{
    CallExpression, Class, ClassElement, Decorator, Expression, MethodDefinition,
    PropertyDefinition,
};
use gate_typescript::oxc_span::GetSpan;

use super::model::{AutoDto, Field, Kept, Key, Origin, Pattern, Plane, Scope, Slot};
use super::object::{Annotation, Object, Value, annotation, property_key};
use super::{Names, Read};
use crate::shared::decorators::{decorator_call, decorator_identity};

/// The DTO types each generated route builds from the entity, as Automator's controller factory
/// generates them; `PARAMETERS` carries only the primary key.
const ROUTES: [(&str, &[&str]); 6] = [
    ("CREATE", &["BODY", "RESPONSE"]),
    ("DELETE", &["PARAMETERS"]),
    ("GET", &["PARAMETERS", "RESPONSE"]),
    ("GET_LIST", &["QUERY", "RESPONSE"]),
    ("PARTIAL_UPDATE", &["BODY", "PARAMETERS", "RESPONSE"]),
    ("UPDATE", &["BODY", "PARAMETERS", "RESPONSE"]),
];

/// A class the inventory knows.
#[derive(Clone, Debug)]
pub(crate) struct ClassInfo {
    pub plane: Option<Plane>,
    /// Whether it is a `TypeORM` entity: an `@Entity` with a table of its own or a
    /// `@ChildEntity` whose rows live in the table of its inheritance root.
    pub entity: bool,
    /// Whether it is an `@Entity` with `@TableInheritance`, the root of a single table.
    pub root: bool,
    /// Whether it is a `@ChildEntity`.
    pub child: bool,
    /// The classes it extends that resolve to source classes, directly or through nested mapped
    /// types.
    pub bases: Vec<Owner>,
}

/// A relation of an entity that Automator describes as a relation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Relation {
    /// The class that declares the relation member.
    pub entity: Owner,
    pub member: String,
    pub target: Owner,
    pub auto: AutoDto,
    /// `@ManyToOne` or `@OneToOne`; a typed filter plan expands only these, a legacy one also the
    /// collections a typed plan leaves out.
    pub to_one: bool,
}

/// Source the check cannot read where it decides a field's scope or exposure.
#[derive(Clone, Debug)]
pub(crate) struct Unread {
    pub path: String,
    pub line: u32,
    pub message: String,
}

/// A DTO member that copies an entity field.
#[derive(Clone, Debug)]
pub(crate) struct Copy {
    pub class: Owner,
    /// The DTO member's own name, which the classes extending `class` keep or leave out.
    pub property: String,
    pub target: Owner,
    /// The copied entity member.
    pub member: String,
}

/// A generated Automator controller.
#[derive(Clone, Debug)]
pub(crate) struct Controller {
    pub name: String,
    pub plane: Option<Plane>,
    pub entity: Owner,
    /// The route and DTO type pairs it generates from the entity.
    pub slots: BTreeSet<Slot>,
    /// Whether its `GET_LIST` filter plan is typed: it declares a filter configuration or cursor
    /// pagination.
    pub typed_filter: bool,
    /// The related-entity member each route projects into its response (`reference.key`), by
    /// route.
    pub reference_keys: BTreeMap<&'static str, String>,
    /// The DTO classes its route `dto` maps name, directly or as a list's `itemType`.
    pub dtos: Vec<Owner>,
    /// The DTO classes its `dto` maps name as the `BODY` of `UPDATE` or `PARTIAL_UPDATE`.
    pub update_bodies: Vec<Owner>,
    /// The `GET_LIST` filter paths its configuration disables.
    pub disabled_filters: BTreeSet<String>,
    /// The filter paths it lists as enabled, when unlisted paths are rejected.
    pub listed_filters: Option<BTreeSet<String>>,
}

/// Everything the classification reads from the source.
#[derive(Debug, Default)]
pub(crate) struct Inventory {
    pub fields: BTreeMap<Key, Field>,
    /// Whether each field is a primary key column.
    pub primary: BTreeSet<Key>,
    pub classes: BTreeMap<Owner, ClassInfo>,
    pub relations: BTreeSet<Relation>,
    pub copies: Vec<Copy>,
    pub controllers: Vec<Controller>,
    /// The column sets of each class's unique indexes, unique constraints and unique columns, as
    /// its `TypeORM` decorators list them.
    pub uniques: BTreeMap<Owner, Vec<BTreeSet<String>>>,
    /// The primary-key columns each class declares itself; an entity's key also holds those of
    /// the classes it extends.
    pub primary_keys: BTreeMap<Owner, BTreeSet<String>>,
    /// The unique indexes and constraints the rule cannot read in full, per class: their column
    /// set when it is known, `None` when it is not.
    pub unread_uniques: BTreeMap<Owner, Vec<Option<BTreeSet<String>>>>,
    /// The members each heritage edge keeps of the class it builds on, per class and base: one
    /// filter per path through the mapped types of its heritage clause.
    pub kept: BTreeMap<Owner, BTreeMap<Owner, Vec<Kept>>>,
    pub unreadable: Vec<Unread>,
    /// DTO classes a controller route or custom route names, with the plane of that controller.
    pub routed: Vec<(Owner, Plane)>,
    /// DTO classes another class nests through `@ApiPropertyObject({ type })`, as (parent, nested).
    pub nested: Vec<(Owner, Owner)>,
    /// The non-entity base classes whose columns and relations were read for an entity, each with
    /// the least depth an entity reached it at and the bases it extends.
    read_bases: BTreeMap<Owner, (u8, Vec<Owner>)>,
}

impl Inventory {
    /// Reads every source context.
    pub(crate) fn read(sources: Read<'_, '_>) -> Self {
        let mut inventory = Self::default();

        for source in sources.iter() {
            for statement in top_level(&source.file) {
                if let Some(class) = class_declaration(statement) {
                    inventory.class(sources, source, class);
                }
            }
        }

        inventory
    }

    fn class<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        class: &'a Class<'a>,
    ) {
        let Some(name) = class.id.as_ref().map(|id| id.name.to_string()) else {
            return;
        };
        let owner = Owner {
            name: name.clone(),
            path: Some(source.path.clone()),
        };
        let bases = self.heritage(sources, source, &owner, class);

        let child = has_decorator(
            source,
            &class.decorators,
            &sources.names.typeorm,
            "ChildEntity",
        );

        if child || has_decorator(source, &class.decorators, &sources.names.typeorm, "Entity") {
            self.unique_constraints(sources, source, &owner, class);
            self.classes.insert(
                owner,
                ClassInfo {
                    plane: sources.names.plane(&source.path),
                    entity: true,
                    root: !child
                        && has_decorator(
                            source,
                            &class.decorators,
                            &sources.names.typeorm,
                            "TableInheritance",
                        ),
                    child,
                    bases: bases.clone(),
                },
            );
            self.entity_members(sources, source, &name, class);

            for base in bases {
                self.entity_base(sources, &base, 0);
            }

            return;
        }

        if let Some(call) = decorator_call(
            source,
            &class.decorators,
            &sources.names.automator,
            "ApiController",
        ) {
            match Self::controller(sources, source, &name, call) {
                Some(controller) => {
                    if let Some(plane) = controller.plane {
                        self.routed
                            .extend(controller.dtos.iter().map(|dto| (dto.clone(), plane)));
                    }

                    self.controllers.push(controller);
                }
                None => self.unreadable.push(Unread {
                    path: source.path.clone(),
                    line: source.file.line(call.span.start),
                    message: format!(
                        "{name}: nestjs/field-inventory cannot read the entity, routes or GET_LIST configuration of this @ApiController, so the planes it exposes are unknown"
                    ),
                }),
            }
        }

        for method in methods(class) {
            self.custom_route(sources, source, &name, method);
        }

        let mut is_dto = false;

        for property in properties(class) {
            is_dto |= self.dto_field(sources, source, &owner, property);
        }

        if is_dto || !bases.is_empty() {
            self.classes.insert(
                owner,
                ClassInfo {
                    plane: sources.names.plane(&source.path),
                    entity: false,
                    root: false,
                    child: false,
                    bases,
                },
            );
        }
    }

    /// The classes a class's heritage clause extends, recording the members each edge keeps.
    fn heritage<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        owner: &Owner,
        class: &'a Class<'a>,
    ) -> Vec<Owner> {
        let Some(heritage) = class.heritage.as_ref() else {
            return Vec::new();
        };
        let (edges, unresolved) = Value {
            expression: &heritage.expression,
            source,
        }
        .bases(sources);
        let mut bases = Vec::new();

        if unresolved {
            self.unread(
                source,
                source.file.line(heritage.expression.span().start),
                format!(
                    "{}: nestjs/field-inventory cannot read a call in this heritage clause that builds on a source class, so the members the class carries are unknown",
                    owner.name
                ),
            );
        }

        for (base, kept) in edges {
            if !bases.contains(&base) {
                bases.push(base.clone());
            }

            let paths = self
                .kept
                .entry(owner.clone())
                .or_default()
                .entry(base)
                .or_default();

            // The class pass and the entity-base pass both read a base class's heritage.
            if !paths.contains(&kept) {
                paths.push(kept);
            }
        }

        bases
    }

    /// Whether the heritage edge from `class` to its base `base` keeps `member`: some path
    /// through its mapped types keeps it, or the edge is not recorded.
    pub(crate) fn keeps(&self, class: &Owner, base: &Owner, member: &str) -> bool {
        self.kept
            .get(class)
            .and_then(|bases| bases.get(base))
            .is_none_or(|kept| kept.iter().any(|kept| kept.keeps(member)))
    }

    /// Reads the columns and relations a non-entity base class gives the entities that extend it.
    fn entity_base(&mut self, sources: Read<'_, '_>, base: &Owner, depth: u8) {
        if depth > sources.names.base_depth {
            return;
        }

        // Read once per base, whether or not the class pass already registered its heritage. An
        // entity that reaches a base nearer than before follows its bases again from there, so how
        // far a shared chain is read does not depend on which entity reaches it first.
        if let Some((first, next)) = self.read_bases.get_mut(base) {
            if depth < *first {
                *first = depth;
                let next = next.clone();

                for next in next {
                    self.entity_base(sources, &next, depth + 1);
                }
            }
            return;
        }

        self.read_bases.insert(base.clone(), (depth, Vec::new()));

        let Some(source) = base.path.as_deref().and_then(|path| sources.get(path)) else {
            return;
        };
        let Some(class) = find_class(&source.file, &base.name) else {
            return;
        };

        // An `@Entity` or `@ChildEntity` base is an entity, which the class pass reads in its own
        // file.
        if has_decorator(source, &class.decorators, &sources.names.typeorm, "Entity")
            || has_decorator(
                source,
                &class.decorators,
                &sources.names.typeorm,
                "ChildEntity",
            )
        {
            return;
        }

        let bases = self.heritage(sources, source, base, class);
        self.unique_constraints(sources, source, base, class);
        self.classes.insert(
            base.clone(),
            ClassInfo {
                plane: sources.names.plane(&source.path),
                entity: false,
                root: false,
                child: false,
                bases: bases.clone(),
            },
        );
        self.read_bases.insert(base.clone(), (depth, bases.clone()));
        self.entity_members(sources, source, &base.name, class);

        for next in bases {
            self.entity_base(sources, &next, depth + 1);
        }
    }

    fn entity_members<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        class_name: &str,
        class: &'a Class<'a>,
    ) {
        for property in properties(class) {
            self.entity_field(sources, source, class_name, property);
            self.relation(sources, source, class_name, property);
        }
    }

    /// Records a to-one relation that Automator describes as a relation.
    fn relation<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        class_name: &str,
        property: &'a PropertyDefinition<'a>,
    ) {
        let to_one = decorator_call(
            source,
            &property.decorators,
            &sources.names.typeorm,
            "ManyToOne",
        )
        .or_else(|| {
            decorator_call(
                source,
                &property.decorators,
                &sources.names.typeorm,
                "OneToOne",
            )
        });
        let Some((call, to_one)) = to_one.map(|call| (call, true)).or_else(|| {
            decorator_call(
                source,
                &property.decorators,
                &sources.names.typeorm,
                "OneToMany",
            )
            .or_else(|| {
                decorator_call(
                    source,
                    &property.decorators,
                    &sources.names.typeorm,
                    "ManyToMany",
                )
            })
            .map(|call| (call, false))
        }) else {
            return;
        };
        // Without Automator metadata a relation is no filter or DTO property at all.
        let Some(describe) = describe(sources, source, property) else {
            if decorator_call(
                source,
                &property.decorators,
                &sources.names.automator,
                "ApiPropertyDescribe",
            )
            .is_some()
            {
                let line = source.file.line(property.key.span().start);
                self.unread(source, line, format!(
                    "{class_name}: nestjs/field-inventory cannot read this relation's ApiPropertyDescribe options, so the filters it adds are unknown"
                ));
            }

            return;
        };
        let kind = describe
            .get("type")
            .and_then(|value| value.automator_member(sources, "EApiPropertyDescribeType"));
        let member = property_key(&property.key, property.computed);
        let target = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
            .and_then(|expression| Value { expression, source }.class(sources));

        match (kind.as_deref(), member, target) {
            (Some("RELATION"), Some(member), Some(target)) => {
                self.relations.insert(Relation {
                    entity: Owner {
                        name: class_name.to_owned(),
                        path: Some(source.path.clone()),
                    },
                    member,
                    target,
                    auto: auto_dto(sources, &describe),
                    to_one,
                });
            }
            (Some(kind), _, _) if kind != "RELATION" => {}
            _ => self.unreadable.push(Unread {
                path: source.path.clone(),
                line: source.file.line(property.key.span().start),
                message: format!(
                    "{class_name}: nestjs/field-inventory cannot read this relation's Automator type, member or target entity, so the filters it adds are unknown"
                ),
            }),
        }
    }

    /// Records an entity column of a text type.
    fn entity_field<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        class_name: &str,
        property: &'a PropertyDefinition<'a>,
    ) {
        let typeorm =
            |name| decorator_call(source, &property.decorators, &sources.names.typeorm, name);
        let primary = typeorm("PrimaryColumn");
        let Some(call) = typeorm("Column").or(primary) else {
            return;
        };
        let Some(member) = property_key(&property.key, property.computed) else {
            return;
        };
        let kind = property
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.type_annotation);
        let typed = annotation(sources, source, kind);
        let line = source.file.line(property.key.span().start);
        let ColumnArguments {
            persisted,
            primary: options_primary,
            unread: options_unread,
        } = column_arguments(sources, source, call);

        let persisted = match persisted {
            Some(persisted) => persisted.to_ascii_lowercase(),
            None if options_unread || typed == Annotation::Unknown => {
                self.unread(source, line, format!(
                    "{class_name}.{member}: nestjs/field-inventory cannot read the column type from its options or its TypeScript type, so whether this column holds text is unknown"
                ));
                return;
            }
            None if typed == Annotation::String => "varchar".to_owned(),
            None => return,
        };

        if !sources.names.text_types.contains(&persisted) {
            return;
        }

        let describe = describe(sources, source, property);

        if describe.is_none()
            && decorator_call(
                source,
                &property.decorators,
                &sources.names.automator,
                "ApiPropertyDescribe",
            )
            .is_some()
        {
            self.unread(source, line, format!(
                "{class_name}.{member}: nestjs/field-inventory cannot read its ApiPropertyDescribe options, so its scope and exposure are unknown"
            ));
        }

        let mut scope = typed_scope(&typed);
        let auto = describe
            .as_ref()
            .map(|describe| auto_dto(sources, describe))
            .unwrap_or_default();

        if let Some(describe) = &describe {
            let kind = describe.get("type");
            let format = describe.get("format");
            let kind_name =
                kind.and_then(|value| value.automator_member(sources, "EApiPropertyDescribeType"));
            let format_name =
                format.and_then(|value| value.automator_member(sources, "EApiPropertyStringType"));

            if !describe.is_read()
                || (kind.is_some() && kind_name.is_none())
                || (format.is_some() && format_name.is_none())
            {
                self.unread(source, line, format!(
                    "{class_name}.{member}: nestjs/field-inventory cannot read all of its ApiPropertyDescribe metadata, so its scope and exposure are unknown"
                ));
            }

            if scope.is_none()
                && let Some(kind) = kind_name
                && !matches!(kind.as_str(), "STRING" | "OBJECT")
            {
                scope = Some(Scope::Excluded(format!("its Automator type is {kind}")));
            }

            if scope.is_none() {
                scope = format_name
                    .as_deref()
                    .map(|format| format_scope(sources.names, format));
            }
        }

        let key = Key {
            path: source.path.clone(),
            class_name: class_name.to_owned(),
            member,
        };

        if primary.is_some() || options_primary {
            self.primary.insert(key.clone());
        }

        self.fields.insert(
            key.clone(),
            Field {
                key,
                line,
                origin: Origin::Entity,
                scope: scope.unwrap_or(Scope::Text),
                auto,
                pattern: pattern(sources, describe.as_ref()),
            },
        );
    }

    /// Records the DTO classes an `@ApiRouteCustom` handler names in its `dto` map and its
    /// `response.type`, which reach the plane of its file.
    fn custom_route<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        class_name: &str,
        method: &'a MethodDefinition<'a>,
    ) {
        let Some(call) = decorator_call(
            source,
            &method.decorators,
            &sources.names.automator,
            "ApiRouteCustom",
        ) else {
            return;
        };
        let Some(plane) = sources.names.plane(&source.path) else {
            return;
        };
        let read = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
            .and_then(|expression| Value { expression, source }.object(sources))
            .and_then(|options| {
                let dto = nested(sources, Some(&options), "dto")?;
                let response = nested(sources, Some(&options), "response")?;
                let mut classes = Vec::new();

                if let Some(dto) = dto {
                    for value in dto.read()?.properties.into_values() {
                        classes.extend(dto_classes(sources, value)?);
                    }
                }

                // A response that names no class is fine only when no spread may hide one.
                if let Some(response) = response {
                    if names_classes(&response) {
                        classes.extend(object_classes(sources, &response, 0)?);
                    } else {
                        response.read()?;
                    }
                }

                Some(classes)
            });

        if let Some(classes) = read {
            self.routed
                .extend(classes.into_iter().map(|class| (class, plane)));
        } else {
            let line = source.file.line(method.key.span().start);
            self.unread(source, line, format!(
                "{class_name}: nestjs/field-inventory cannot read the DTO classes this @ApiRouteCustom names, so the planes they reach are unknown"
            ));
        }
    }

    /// Records the primary-key columns of an entity class, the column sets of its unique `@Index`
    /// and `@Unique` decorators and of its members' unique `@Index` and `@Column({ unique: true })`.
    fn unique_constraints<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        owner: &Owner,
        class: &'a Class<'a>,
    ) {
        let mut sets = Vec::new();
        let mut unread = Vec::new();

        for decorator in &class.decorators {
            let (identity, call) = decorator_identity(source, decorator);
            let Some(identity) = identity.filter(|identity| {
                identity.module.as_deref() == Some(sources.names.typeorm.as_str())
                    && matches!(identity.name.as_str(), "Index" | "Unique")
            }) else {
                continue;
            };
            let Some(call) = call else {
                unread.push(None);
                continue;
            };

            match class_constraint(sources, source, &identity.name, call) {
                Constraint::Unique(set) => sets.push(set),
                Constraint::NotUnique => {}
                Constraint::Unread(set) => unread.push(set),
            }
        }

        let mut primary = BTreeSet::new();

        for property in properties(class) {
            let Some(member) = property_key(&property.key, property.computed) else {
                continue;
            };

            if ["PrimaryColumn", "PrimaryGeneratedColumn"]
                .iter()
                .any(|name| {
                    decorator_call(source, &property.decorators, &sources.names.typeorm, name)
                        .is_some()
                })
                || decorator_call(
                    source,
                    &property.decorators,
                    &sources.names.typeorm,
                    "Column",
                )
                .is_some_and(|call| column_arguments(sources, source, call).primary)
            {
                primary.insert(member.clone());
            }

            for decorator in &property.decorators {
                let (identity, call) = decorator_identity(source, decorator);
                let Some(name) = identity
                    .filter(|identity| {
                        identity.module.as_deref() == Some(sources.names.typeorm.as_str())
                            && matches!(identity.name.as_str(), "Index" | "Column" | "Unique")
                    })
                    .map(|identity| identity.name)
                else {
                    continue;
                };

                // A member `@Unique` makes the member unique whatever it is named.
                if name == "Unique" {
                    sets.push(BTreeSet::from([member.clone()]));
                    continue;
                }

                match call.map(|call| unique_option(sources, source, call)) {
                    Some(Some(true)) => sets.push(BTreeSet::from([member.clone()])),
                    // A bare `@Index` or `@Column` declares no unique index.
                    Some(Some(false)) | None => {}
                    Some(None) => unread.push(Some(BTreeSet::from([member.clone()]))),
                }
            }
        }

        if !primary.is_empty() {
            self.primary_keys.insert(owner.clone(), primary);
        }

        if !unread.is_empty() {
            self.unread_uniques.insert(owner.clone(), unread);
        }

        if !sets.is_empty() {
            self.uniques.entry(owner.clone()).or_default().extend(sets);
        }
    }

    fn unread(&mut self, source: &SourceContext<'_>, line: u32, message: String) {
        self.unreadable.push(Unread {
            path: source.path.clone(),
            line,
            message,
        });
    }

    /// The scope of an `@ApiPropertyObject` member, recording the DTO classes it nests.
    fn object_scope<'s, 'a>(
        &mut self,
        sources: Read<'s, 'a>,
        source: &SourceContext<'a>,
        owner: &Owner,
        member: &str,
        property: &'a PropertyDefinition<'a>,
        options: Option<&Object<'s, 'a>>,
    ) -> Scope {
        let kind = options.and_then(|options| options.get("type"));
        let names = options.is_some_and(names_classes);
        // A discriminated or generated object holds one of the DTO classes it names, as a typed
        // one does.
        let nested = match kind {
            Some(value) => !matches!(
                unwrap(value.expression),
                Expression::Identifier(identifier) if identifier.name == "Object"
            ),
            None => names,
        };
        let additional = options.and_then(|options| options.get("additionalProperties"));

        // Options read only in part may hide the nested type, so they name no class only when fully
        // read.
        if let Some(options) = options.filter(|options| names || !options.is_read()) {
            if let Some(classes) = object_classes(sources, options, 0) {
                self.nested
                    .extend(classes.into_iter().map(|class| (owner.clone(), class)));
            } else {
                let line = source.file.line(property.key.span().start);
                self.unread(source, line, format!(
                    "{}.{member}: nestjs/field-inventory cannot read the nested DTO type of this ApiPropertyObject, so the planes it reaches are unknown",
                    owner.name
                ));
            }
        }

        if additional.is_some_and(Value::is_false) {
            Scope::Excluded("its object admits no additional properties".to_owned())
        } else if nested && additional.is_none() {
            Scope::Excluded("its object type is a nested DTO".to_owned())
        } else {
            Scope::Text
        }
    }

    /// Records a DTO member; returns whether it carries an Automator property decorator.
    fn dto_field<'a>(
        &mut self,
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        owner: &Owner,
        property: &'a PropertyDefinition<'a>,
    ) -> bool {
        let Some((decorator, call)) = property.decorators.iter().find_map(|decorator| {
            let (identity, call) = decorator_identity(source, decorator);
            let identity = identity?;

            (identity.module.as_deref() == Some(sources.names.automator.as_str())
                && identity.name.starts_with("ApiProperty")
                && identity.name != "ApiPropertyDescribe")
                .then_some((identity.name, call))
        }) else {
            return false;
        };
        let Some(member) = property_key(&property.key, property.computed) else {
            return true;
        };
        let options = call
            .and_then(|call| call.arguments.first()?.as_expression())
            .and_then(|expression| Value { expression, source }.object(sources));
        let kind = property
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.type_annotation);
        let mut field_pattern = Pattern::Absent;
        let scope = match decorator.as_str() {
            "ApiPropertyString" => {
                field_pattern = pattern(sources, options.as_ref());
                let format = options.as_ref().and_then(|options| options.get("format"));
                let format_name = format
                    .and_then(|value| value.automator_member(sources, "EApiPropertyStringType"));

                if (format.is_some() && format_name.is_none())
                    || options.as_ref().is_none_or(|options| !options.is_read())
                {
                    let line = source.file.line(property.key.span().start);
                    self.unread(source, line, format!(
                        "{}.{member}: nestjs/field-inventory cannot read the options of this ApiPropertyString, so its scope is unknown",
                        owner.name
                    ));
                }

                typed_scope(&annotation(sources, source, kind)).unwrap_or_else(|| {
                    format_name
                        .as_deref()
                        .map_or(Scope::Text, |format| format_scope(sources.names, format))
                })
            }
            "ApiPropertyObject" => {
                self.object_scope(sources, source, owner, &member, property, options.as_ref())
            }
            "ApiPropertyCopy" => {
                if let Some(options) = &options
                    && let Some(target) =
                        options.get("entity").and_then(|value| value.class(sources))
                    && let Some(copied) = options
                        .get("propertyName")
                        .and_then(|value| value.text(sources))
                {
                    self.copies.push(Copy {
                        class: owner.clone(),
                        property: member.clone(),
                        target,
                        member: copied,
                    });
                } else {
                    let line = source.file.line(property.key.span().start);
                    self.unread(source, line, format!(
                        "{}.{member}: nestjs/field-inventory cannot read the entity or property this ApiPropertyCopy copies, so the field it exposes is unknown",
                        owner.name
                    ));
                }

                return true;
            }
            _ => return true,
        };
        let key = Key {
            path: source.path.clone(),
            class_name: owner.name.clone(),
            member,
        };

        self.fields.insert(
            key.clone(),
            Field {
                key,
                line: source.file.line(property.key.span().start),
                origin: Origin::Dto,
                scope,
                auto: AutoDto::default(),
                pattern: field_pattern,
            },
        );
        true
    }

    /// The entity, generated route slots and `GET_LIST` filter configuration of an
    /// `@ApiController(config)`.
    fn controller<'a>(
        sources: Read<'_, 'a>,
        source: &SourceContext<'a>,
        name: &str,
        call: &'a CallExpression<'a>,
    ) -> Option<Controller> {
        let config = Value {
            expression: call.arguments.first()?.as_expression()?,
            source,
        }
        .object(sources)?;
        let entity = config.get("entity")?.class(sources)?;
        let routes = match config
            .get("routes")
            .and_then(|routes| routes.object(sources))
        {
            Some(routes) => Some(routes.read()?),
            None => None,
        };
        let mut slots = BTreeSet::new();
        let mut reference_keys = BTreeMap::new();
        let mut dtos = Vec::new();
        let mut update_bodies = Vec::new();

        for (route, types) in ROUTES {
            let settings = nested(sources, routes.as_ref(), route)?;
            let generation = nested(sources, settings.as_ref(), "generation")?;
            let dto = match nested(sources, settings.as_ref(), "dto")? {
                Some(dto) => Some(dto.read()?),
                None => None,
            };

            if let Some(dto) = &dto {
                for value in dto.properties.values() {
                    dtos.extend(dto_classes(sources, *value)?);
                }
            }

            if let Some(key) = settings
                .as_ref()
                .and_then(|settings| settings.get("relations"))
                .and_then(|relations| relations.object(sources))
                .and_then(|relations| relations.get("response"))
                .and_then(|response| response.object(sources))
                .and_then(|response| response.get("reference"))
                .and_then(|reference| reference.object(sources))
                .and_then(|reference| reference.get("key"))
                .and_then(|key| key.text(sources))
            {
                reference_keys.insert(route, key);
            }

            if generation
                .as_ref()
                .and_then(|generation| generation.get("isEnabled"))
                .is_some_and(Value::is_false)
            {
                continue;
            }

            // Only a route that is generated applies its body to the entity.
            if matches!(route, "UPDATE" | "PARTIAL_UPDATE")
                && let Some(body) = dto.as_ref().and_then(|dto| dto.get("BODY"))
            {
                update_bodies.extend(dto_classes(sources, body)?);
            }

            for kind in types {
                if dto
                    .as_ref()
                    .is_none_or(|explicit| explicit.get(kind).is_none())
                {
                    slots.insert((route, *kind));
                }
            }
        }

        let settings = routes
            .as_ref()
            .and_then(|routes| routes.get("GET_LIST"))
            .and_then(|settings| settings.object(sources));
        let query = get_list_query(sources, settings.as_ref())?;

        Some(Controller {
            name: name.to_owned(),
            plane: sources.names.plane(&source.path),
            entity,
            slots,
            typed_filter: query.typed,
            reference_keys,
            dtos,
            update_bodies,
            disabled_filters: query.disabled,
            listed_filters: query.listed,
        })
    }
}

/// The filter plan a `GET_LIST` query configuration declares.
struct Query {
    typed: bool,
    disabled: BTreeSet<String>,
    listed: Option<BTreeSet<String>>,
}

/// The `GET_LIST` query configuration of a route; `None` when part of it cannot be read.
fn get_list_query<'s, 'a>(
    sources: Read<'s, 'a>,
    settings: Option<&Object<'s, 'a>>,
) -> Option<Query> {
    let request = nested(sources, settings, "request")?;
    let query = nested(sources, request.as_ref(), "QUERY")?;
    let pagination = nested(sources, query.as_ref(), "pagination")?;
    let cursor = pagination
        .as_ref()
        .and_then(|pagination| pagination.get("mode"))
        .and_then(|mode| mode.automator_member(sources, "EApiControllerGetListQueryPaginationMode"))
        .is_some_and(|mode| mode == "CURSOR");
    let filter = nested(sources, query.as_ref(), "filter")?;

    // A filter configuration read only in part could reject or disable a path it does not
    // show, so the controller's exposure is unknown rather than narrower.
    if filter.as_ref().is_some_and(|filter| {
        !filter.is_read()
            || filter
                .get("fields")
                .and_then(|fields| fields.object(sources))
                .is_none_or(|fields| !fields.is_read())
    }) {
        return None;
    }
    let mut disabled_filters = BTreeSet::new();
    let mut enabled_filters = BTreeSet::new();

    if let Some(fields) = filter
        .as_ref()
        .and_then(|filter| filter.get("fields"))
        .and_then(|fields| fields.object(sources))
    {
        for (name, settings) in &fields.properties {
            let disabled = settings
                .object(sources)
                .and_then(|settings| settings.get("isEnabled"))
                .is_some_and(Value::is_false);

            if disabled {
                disabled_filters.insert(name.clone());
            } else {
                enabled_filters.insert(name.clone());
            }
        }
    }

    let rejects = filter
        .as_ref()
        .and_then(|filter| filter.get("unlistedFields"))
        .and_then(|value| {
            value.automator_member(sources, "EApiControllerGetListQueryUnlistedFields")
        })
        .is_some_and(|mode| mode == "REJECT");

    Some(Query {
        typed: filter.is_some() || cursor,
        disabled: disabled_filters,
        listed: rejects.then_some(enabled_filters),
    })
}

/// What a column decorator's arguments say: its persisted type, whether its options mark it
/// primary, and whether part of them cannot be read.
struct ColumnArguments {
    persisted: Option<String>,
    primary: bool,
    unread: bool,
}

fn column_arguments<'a>(
    sources: Read<'_, 'a>,
    source: &SourceContext<'a>,
    call: &'a CallExpression<'a>,
) -> ColumnArguments {
    let mut read = ColumnArguments {
        persisted: None,
        primary: false,
        unread: false,
    };

    for argument in &call.arguments {
        let Some(expression) = argument.as_expression() else {
            continue;
        };
        let value = Value { expression, source };

        if let Some(text) = value.text(sources) {
            read.persisted.get_or_insert(text);
        } else if let Some(options) = value.object(sources) {
            read.unread |= !options.is_read();
            read.primary |= options.get("primary").is_some_and(Value::is_true);

            if let Some(text) = options.get("type").and_then(|value| value.text(sources)) {
                read.persisted.get_or_insert(text);
            }
        } else {
            read.unread = true;
        }
    }

    read
}

/// The source classes a DTO reference names: a class, an arrow returning one, a list of them, a
/// constant holding one of those, or an object naming them (see [`object_classes`]). A global or
/// package name such as `Object` or `String` names no source class; `None` when the reference
/// cannot be read.
fn dto_classes<'s, 'a>(sources: Read<'s, 'a>, value: Value<'s, 'a>) -> Option<Vec<Owner>> {
    dto_classes_at(sources, value, 0)
}

fn dto_classes_at<'s, 'a>(
    sources: Read<'s, 'a>,
    value: Value<'s, 'a>,
    depth: u8,
) -> Option<Vec<Owner>> {
    if depth > sources.names.reference_depth {
        return None;
    }

    let expression = unwrap(value.expression);
    let recurse = |expression: &'a Expression<'a>| {
        dto_classes_at(
            sources,
            Value {
                expression,
                source: value.source,
            },
            depth + 1,
        )
    };

    match expression {
        Expression::ArrowFunctionExpression(arrow) => recurse(arrow.get_expression()?),
        Expression::ArrayExpression(array) => {
            let mut classes = Vec::new();

            for element in &array.elements {
                classes.extend(recurse(element.as_expression()?)?);
            }

            Some(classes)
        }
        Expression::ObjectExpression(_) => object_classes(sources, &value.object(sources)?, depth),
        _ => match value.class(sources) {
            // A class declaration of a source file is a DTO class; any other source declaration
            // is a constant whose value names the classes.
            Some(owner) if is_class(sources, &owner) => Some(vec![owner]),
            Some(_) => dto_classes_at(sources, value.resolved(sources)?, depth + 1),
            None if matches!(expression, Expression::Identifier(_))
                || gate_typescript::owners::property_access(expression).is_some() =>
            {
                match value.resolved(sources) {
                    Some(resolved) => dto_classes_at(sources, resolved, depth + 1),
                    // A global or package name, which no source file declares.
                    None => Some(Vec::new()),
                }
            }
            None => None,
        },
    }
}

/// The classes an object naming DTO classes holds: its `type`, a list's `itemType`, the class
/// values of its `discriminator.mapping` and the values of its `generatedDTOs`; `None` when it
/// names none of them or cannot be read.
fn object_classes<'s, 'a>(
    sources: Read<'s, 'a>,
    object: &Object<'s, 'a>,
    depth: u8,
) -> Option<Vec<Owner>> {
    let mut classes = Vec::new();
    let mut named = false;

    for key in ["type", "itemType"] {
        if let Some(value) = object.get(key) {
            named = true;
            classes.extend(dto_classes_at(sources, value, depth + 1)?);
        }
    }

    for key in ["discriminator", "generatedDTOs"] {
        let Some(value) = object.get(key) else {
            continue;
        };
        let map = if key == "discriminator" {
            // A spread the reader cannot follow may hide or replace the mapping.
            match value.object(sources)?.read()?.get("mapping") {
                Some(mapping) => mapping.object(sources)?,
                None => continue,
            }
        } else {
            value.object(sources)?
        };

        named = true;

        // Only the values matter, so a key the reader cannot name does not hide one.
        for entry in map.values()? {
            // A dynamic discriminator maps to `generatedDTOs` keys by string.
            if entry.text(sources).is_none() {
                classes.extend(dto_classes_at(sources, entry, depth + 1)?);
            }
        }
    }

    // An object read only in part may hide another class behind its spread.
    (named && object.is_read()).then_some(classes)
}

/// Whether an object names DTO classes through a key [`object_classes`] reads.
fn names_classes(object: &Object<'_, '_>) -> bool {
    ["type", "itemType", "discriminator", "generatedDTOs"]
        .iter()
        .any(|key| object.get(key).is_some())
}

/// Whether an owner is a top-level class declaration of a source file.
fn is_class(sources: Read<'_, '_>, owner: &Owner) -> bool {
    owner
        .path
        .as_deref()
        .and_then(|path| sources.get(path))
        .is_some_and(|source| find_class(&source.file, &owner.name).is_some())
}

/// The object the `key` of `parent` holds: `Some(None)` when the key is absent from a fully read
/// parent, `None` when it is present but no readable object or may hide behind a spread the parent
/// could not read, so a caller cannot mistake an unreadable setting for a default.
#[expect(
    clippy::option_option,
    reason = "absent and unreadable settings need distinct answers"
)]
fn nested<'s, 'a>(
    sources: Read<'s, 'a>,
    parent: Option<&Object<'s, 'a>>,
    key: &str,
) -> Option<Option<Object<'s, 'a>>> {
    match parent.and_then(|parent| parent.get(key)) {
        // A key missing from a parent read only in part may hide behind its unreadable spread.
        None if parent.is_some_and(|parent| !parent.is_read()) => None,
        None => Some(None),
        Some(value) => value.object(sources).map(Some),
    }
}

/// The `ApiPropertyDescribe` options of a property.
fn describe<'s, 'a>(
    sources: Read<'s, 'a>,
    source: &'s SourceContext<'a>,
    property: &'a PropertyDefinition<'a>,
) -> Option<Object<'s, 'a>> {
    decorator_call(
        source,
        &property.decorators,
        &sources.names.automator,
        "ApiPropertyDescribe",
    )
    .and_then(|call| call.arguments.first()?.as_expression())
    .and_then(|expression| Value { expression, source }.object(sources))
}

/// What Automator's DTO generation reads from an `ApiPropertyDescribe`.
fn auto_dto<'s, 'a>(sources: Read<'s, 'a>, describe: &Object<'s, 'a>) -> AutoDto {
    AutoDto {
        disabled: describe
            .get("isAutoDtoEnabled")
            .is_some_and(Value::is_false),
        excluded: excluded_slots(sources, describe),
        object: describe
            .get("type")
            .and_then(|value| value.automator_member(sources, "EApiPropertyDescribeType"))
            .is_some_and(|kind| kind == "OBJECT"),
        described: true,
    }
}

/// The scope a TypeScript annotation gives a property that is no free text.
fn typed_scope(typed: &Annotation) -> Option<Scope> {
    match typed {
        Annotation::Enum(name) => Some(Scope::Excluded(format!(
            "its TypeScript type is the enum {name}"
        ))),
        Annotation::Literal => Some(Scope::Excluded(
            "its TypeScript type is a string literal".to_owned(),
        )),
        Annotation::String | Annotation::Other | Annotation::Unknown => None,
    }
}

/// The scope an Automator string format gives a field.
fn format_scope(names: &Names, format: &str) -> Scope {
    if let Some(kind) = names.code_formats.get(format) {
        return Scope::Code(*kind, format.to_owned());
    }

    if names.machine_formats.iter().any(|known| known == format) {
        return Scope::Excluded(format!("its Automator format is {format}"));
    }

    Scope::Text
}

/// The route and DTO type pairs an `ApiPropertyDescribe` disables through its `properties`.
fn excluded_slots<'s, 'a>(sources: Read<'s, 'a>, describe: &Object<'s, 'a>) -> BTreeSet<Slot> {
    let mut excluded = BTreeSet::new();
    let Some(properties) = describe
        .get("properties")
        .and_then(|value| value.object(sources))
    else {
        return excluded;
    };

    for (route, types) in ROUTES {
        let Some(settings) = properties
            .get(route)
            .and_then(|value| value.object(sources))
        else {
            continue;
        };

        for kind in types {
            if settings
                .get(kind)
                .and_then(|value| value.object(sources))
                .and_then(|value| value.get("isEnabled"))
                .is_some_and(Value::is_false)
            {
                excluded.insert((route, *kind));
            }
        }
    }

    excluded
}

/// The method definitions of a class.
fn methods<'a>(class: &'a Class<'a>) -> impl Iterator<Item = &'a MethodDefinition<'a>> {
    class.body.body.iter().filter_map(|element| match element {
        ClassElement::MethodDefinition(method) => Some(&**method),
        _ => None,
    })
}

/// The property definitions of a class.
fn properties<'a>(class: &'a Class<'a>) -> impl Iterator<Item = &'a PropertyDefinition<'a>> {
    class.body.body.iter().filter_map(|element| match element {
        ClassElement::PropertyDefinition(property) => Some(&**property),
        _ => None,
    })
}

/// Whether one of the decorators names `module`'s `name`.
fn has_decorator(
    source: &SourceContext<'_>,
    decorators: &[Decorator<'_>],
    module: &str,
    name: &str,
) -> bool {
    decorators.iter().any(|decorator| {
        decorator_identity(source, decorator)
            .0
            .is_some_and(|identity| {
                identity.module.as_deref() == Some(module) && identity.name == name
            })
    })
}

/// What a class-level `@Index` or `@Unique` declares.
enum Constraint {
    Unique(BTreeSet<String>),
    NotUnique,
    /// It cannot be read in full; its column set when that is known.
    Unread(Option<BTreeSet<String>>),
}

/// The column set of a class-level `@Index([...], { unique: true })` or `@Unique([...])`, each
/// optionally named first.
fn class_constraint<'a>(
    sources: Read<'_, 'a>,
    source: &SourceContext<'a>,
    decorator: &str,
    call: &'a CallExpression<'a>,
) -> Constraint {
    let columns = call
        .arguments
        .iter()
        .filter_map(|argument| argument.as_expression())
        .find_map(|expression| match unwrap(expression) {
            Expression::ArrayExpression(array) => Some(array),
            _ => None,
        });
    let Some(columns) = columns else {
        return Constraint::Unread(None);
    };
    let mut set = BTreeSet::new();

    for element in &columns.elements {
        let Some(text) = element
            .as_expression()
            .and_then(|expression| Value { expression, source }.text(sources))
        else {
            return Constraint::Unread(None);
        };

        set.insert(text);
    }

    if decorator == "Unique" {
        return Constraint::Unique(set);
    }

    match unique_option(sources, source, call) {
        Some(true) => Constraint::Unique(set),
        Some(false) => Constraint::NotUnique,
        None => Constraint::Unread(Some(set)),
    }
}

/// Whether the options object of a `TypeORM` decorator call sets `unique: true`; `None` when its
/// options cannot be read in full. A name, a column list or a column function is no options
/// object.
fn unique_option<'a>(
    sources: Read<'_, 'a>,
    source: &SourceContext<'a>,
    call: &'a CallExpression<'a>,
) -> Option<bool> {
    for argument in &call.arguments {
        let expression = argument.as_expression()?;

        if matches!(
            unwrap(expression),
            Expression::ArrayExpression(_) | Expression::ArrowFunctionExpression(_)
        ) || (Value { expression, source }).text(sources).is_some()
        {
            continue;
        }

        let options = Value { expression, source }.object(sources)?;

        // A partial index is unique only where its condition holds.
        if options.get("where").is_some() {
            return Some(false);
        }

        return match options.get("unique") {
            Some(value) if value.is_true() => Some(true),
            Some(value) if value.is_false() => Some(false),
            Some(_) => None,
            None => options.is_read().then_some(false),
        };
    }

    Some(false)
}

/// The Automator `pattern` of a field's options.
fn pattern<'s, 'a>(sources: Read<'s, 'a>, options: Option<&Object<'s, 'a>>) -> Pattern {
    let Some(options) = options else {
        return Pattern::Absent;
    };

    match options.get("pattern") {
        Some(value) => value.text(sources).map_or(Pattern::Unread, Pattern::Text),
        None if options.is_read() => Pattern::Absent,
        None => Pattern::Unread,
    }
}
