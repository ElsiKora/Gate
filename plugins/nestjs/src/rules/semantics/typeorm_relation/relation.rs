//! The persisted columns of every class and each owning
//! relation's target, type, options, name and join.

use std::collections::{BTreeMap, HashMap};

use gate_typescript::binder::Meaning;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, Class, ClassElement, ClassType, Expression, IdentifierReference,
};
use gate_typescript::oxc_span::GetSpan;

use super::catalog::{self, Catalog, Column, Columns, Entity, Kind};
use super::decorators::Resolved;
use super::grammar::{self, Item, Options};
use super::names;
use super::types::{self, Scalar};
use super::{Finding, NullableRelation, Spelling, Vocabulary, member_name, quote};
use gate_typescript::syntax::{constructor, decorated_start, member_start};

/// The foreign key names of every owner relation, each with the owners that declare it and where.
pub(super) type Reuse = BTreeMap<String, BTreeMap<String, (String, u32)>>;

/// What one class check shares across its relations.
pub(super) struct Pass<'r, 's, 'a, 'v> {
    pub catalog: &'r Catalog<'s, 'a, 'v>,
    pub vocabulary: &'v Vocabulary,
    /// The foreign key spelling; without one, no name is checked, and the run stops afterwards
    /// when a relation's join reached its foreign key name.
    pub spelling: Option<Spelling<'v>>,
    pub exceptions: &'r [NullableRelation],
    pub findings: &'r mut Vec<Finding>,
    pub reuse: &'r mut Reuse,
}

/// The identifier of a direct zero-argument arrow `() => Target`, the relation's first argument.
fn target_identifier<'d, 'a>(
    relation: &Resolved<'d, 'a, '_>,
) -> Option<&'d IdentifierReference<'a>> {
    let Some(Argument::ArrowFunctionExpression(arrow)) = relation.call?.arguments.first() else {
        return None;
    };

    if arrow.r#async || !arrow.params.items.is_empty() || arrow.params.rest.is_some() {
        return None;
    }

    match arrow.get_expression()? {
        Expression::Identifier(identifier) => Some(identifier),
        _ => None,
    }
}

/// The exported same-file `@Entity` the relation of `class` targets, or the
/// one exported `@Entity` its runtime named import reaches, with its identity.
fn target(
    catalog: &Catalog<'_, '_, '_>,
    unit: usize,
    class: &Class<'_>,
    identifier: &IdentifierReference<'_>,
) -> Result<(usize, String), String> {
    let name = identifier.name.as_str();
    let Some((_, binder)) = &catalog.units[unit].resolver else {
        return Err(String::new());
    };
    let nodes = catalog.units[unit].file.semantic().nodes();
    let declarations = binder
        .resolve(identifier, Meaning::Value)
        .map(|symbol| binder.declarations(symbol))
        .unwrap_or_default();
    let classes: Vec<&Class<'_>> = declarations
        .iter()
        .filter_map(|&id| match nodes.kind(id) {
            AstKind::Class(declaration) if declaration.r#type == ClassType::ClassDeclaration => {
                Some(declaration)
            }
            _ => None,
        })
        .collect();

    if let [declaration] = classes.as_slice()
        && std::ptr::eq(*declaration, class)
        && let Some(entity) = catalog.entity_of(class)
        && catalog.entities[entity].exported
    {
        let identity = format!(
            "{}:{}",
            catalog.units[unit].path, catalog.entities[entity].name
        );

        return Ok((entity, identity));
    }

    let imports: Vec<_> = declarations
        .iter()
        .filter_map(|&id| types::import_of(catalog, unit, id))
        .collect();
    let [(module, imported, type_only)] = imports.as_slice() else {
        return Err(format!(
            "target {name} must be an exact exported same-file @Entity self-target or one runtime named import of a repository @Entity class"
        ));
    };

    if *type_only {
        return Err(format!(
            "target {name} must be one runtime named import of a repository @Entity class"
        ));
    }

    let quoted = quote(module);
    let Some(source) = catalog.imported(catalog.units[unit].path, module) else {
        return Err(format!(
            "target {name} import {quoted} must resolve to one repository TypeScript source file"
        ));
    };

    match catalog
        .exported(source, imported, Kind::Entity, true, &[])
        .as_slice()
    {
        [(entity, identity)] => Ok((*entity, identity.clone())),
        _ => Err(format!(
            "target {name} import {quoted} must resolve through explicit named exports to exactly one exported @Entity class {imported}"
        )),
    }
}

/// One member that carries an owning relation or a join column.
struct Decorated<'d, 'a, 'v> {
    member: &'d ClassElement<'a>,
    relations: Vec<Resolved<'d, 'a, 'v>>,
    joins: Vec<Resolved<'d, 'a, 'v>>,
}

/// The relations of one class to each target, by target identity.
#[derive(Default)]
struct Targets {
    counts: HashMap<String, usize>,
    names: HashMap<String, Vec<String>>,
}

impl<'a, 'v> Pass<'_, '_, 'a, 'v> {
    /// Every file that imports `typeorm` decorators: each prohibited
    /// decorator and each class, declared or expressed anywhere in the file.
    pub(super) fn files(&mut self) {
        let catalog = self.catalog;
        let vocabulary = self.vocabulary;

        for (index, unit) in catalog.units.iter().enumerate() {
            let Some((bindings, binder)) = &unit.resolver else {
                continue;
            };
            let nodes = unit.file.semantic().nodes();

            for node in nodes.iter() {
                match node.kind() {
                    AstKind::Decorator(_) if on_constructor(nodes.parent_kind(node.id())) => {}
                    AstKind::Decorator(decorator) => {
                        if let Some(resolved) =
                            super::decorators::resolve(decorator, bindings, binder, vocabulary)
                            && vocabulary
                                .banned
                                .iter()
                                .any(|banned| banned == resolved.canonical)
                        {
                            let line = unit.line(decorator.span.start);
                            self.findings.push(Finding::new(
                                "prohibited-decorator",
                                unit.path,
                                line,
                                format!(
                                    "{}:{line} uses prohibited TypeORM @{}",
                                    unit.path, resolved.canonical
                                ),
                            ));
                        }
                    }
                    AstKind::Class(class) => {
                        // The compiler starts a class at its first decorator or its `export`.
                        let start = match nodes.parent_kind(node.id()) {
                            AstKind::ExportDeclaration(export) => export.span.start,
                            AstKind::ExportDefaultDeclaration(export) => export.span.start,
                            _ => class.span.start,
                        };

                        self.class(
                            index,
                            class,
                            decorated_start(start.min(class.span.start), &class.decorators),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// The column declarations of every member, then each owner relation.
    pub(super) fn class(&mut self, unit: usize, class: &'a Class<'a>, start: u32) {
        let source = &self.catalog.units[unit];
        let class_name = class.id.as_ref().map_or_else(
            || format!("<anonymous@{}>", source.line(start)),
            |id| id.name.to_string(),
        );
        let mut columns: Columns<'a> = BTreeMap::new();
        let mut decorated = Vec::new();
        let mut targets = Targets::default();

        for member in &class.body.body {
            let decorators = source.decorators(catalog::member_decorators(member), self.vocabulary);

            for decorator in decorators.iter().filter(|decorator| {
                self.vocabulary
                    .columns
                    .iter()
                    .any(|column| column == decorator.canonical)
            }) {
                let (errors, persisted) =
                    grammar::column(catalog::column_property(member).as_deref(), decorator);
                let line = source.line(member_start(member));
                let label = format!(
                    "{}:{line} {class_name}.{}",
                    source.path,
                    member_name(source.file, member)
                );

                for error in errors {
                    self.findings.push(Finding::new(
                        "column-declaration",
                        source.path,
                        line,
                        format!("{label} {error}"),
                    ));
                }

                if let Some((name, state)) = persisted {
                    columns
                        .entry(name)
                        .or_default()
                        .push(Column { member, state });
                }
            }

            let relations: Vec<Resolved<'_, 'a, 'v>> = decorators
                .iter()
                .filter(|decorator| {
                    self.vocabulary
                        .owning
                        .iter()
                        .any(|owning| owning == decorator.canonical)
                })
                .copied()
                .collect();
            let joins: Vec<Resolved<'_, 'a, 'v>> = decorators
                .iter()
                .filter(|decorator| decorator.canonical == "JoinColumn")
                .copied()
                .collect();

            if relations.is_empty() && joins.is_empty() {
                continue;
            }

            if let [relation] = relations.as_slice()
                && let Some(identifier) = target_identifier(relation)
                && let Ok((_, identity)) = target(self.catalog, unit, class, identifier)
            {
                *targets.counts.entry(identity.clone()).or_default() += 1;
                targets
                    .names
                    .entry(identity)
                    .or_default()
                    .push(member_name(source.file, member));
            }

            decorated.push(Decorated {
                member,
                relations,
                joins,
            });
        }

        for member in &decorated {
            self.owner(unit, class, &class_name, &columns, &targets, member);
        }
    }

    /// The relation's decorators, options, target and type, its name, then its
    /// join.
    fn owner(
        &mut self,
        unit: usize,
        class: &Class<'_>,
        class_name: &str,
        columns: &Columns<'a>,
        targets: &Targets,
        decorated: &Decorated<'_, 'a, 'v>,
    ) {
        let source = &self.catalog.units[unit];
        let member = decorated.member;
        let name = member_name(source.file, member);
        let line = source.line(member_start(member));
        let owner = Owner {
            unit,
            path: source.path,
            line,
            label: format!("{}:{line} {class_name}.{name}", source.path),
            target: format!("{}:{class_name}.{name}", source.path),
            slot: self.exceptions.iter().find(|exception| {
                exception.path == source.path
                    && exception.class == class_name
                    && exception.member == name
            }),
            name,
            member,
        };

        let owning = self.vocabulary.owning_text();
        let Some(relation) = decorated.relations.first() else {
            self.add(
                &owner,
                "relation-owner",
                &format!("has a TypeORM @JoinColumn without an owning {owning}"),
            );
            return;
        };

        if decorated.relations.len() != 1 {
            self.add(
                &owner,
                "relation-owner",
                &format!("must have exactly one owning TypeORM {owning} decorator"),
            );
        }

        if decorated.joins.len() != 1 {
            self.add(
                &owner,
                "relation-owner",
                "must have exactly one TypeORM @JoinColumn decorator",
            );
        }

        let head = self.head(&owner, class, relation);
        let qualifier = self.naming(&owner, &head, targets);

        if let Some(join) = decorated.joins.first() {
            self.join(&owner, &head, qualifier.as_deref(), columns, join);
        }
    }

    /// One finding of an owner relation, labelled with it.
    fn add(&mut self, owner: &Owner<'_, 'a>, aspect: &'static str, message: &str) {
        self.findings.push(Finding::new(
            aspect,
            owner.path,
            owner.line,
            format!("{} {message}", owner.label),
        ));
    }

    /// One finding of an owner relation, labelled with it, with the target `<path>:<class>.<member>`
    /// an exception names.
    fn add_targeted(&mut self, owner: &Owner<'_, 'a>, aspect: &'static str, message: &str) {
        let mut finding = Finding::new(
            aspect,
            owner.path,
            owner.line,
            format!("{} {message}", owner.label),
        );
        finding.target = Some(owner.target.clone());
        self.findings.push(finding);
    }

    /// The options, target and type of a relation, and whether its nullability exception holds.
    fn head(
        &mut self,
        owner: &Owner<'_, 'a>,
        class: &Class<'_>,
        relation: &Resolved<'_, 'a, 'v>,
    ) -> Head {
        let catalog = self.catalog;
        let decorator = format!("@{}", relation.canonical);
        let mut head = Head {
            decorator,
            options: None,
            entity: None,
            identity: None,
            nullability: None,
            excepted: false,
        };
        let decorator = head.decorator.clone();

        match relation.call.map(|call| call.arguments.as_slice()) {
            Some([_, Argument::ObjectExpression(object)]) => {
                let parsed = grammar::options(object, &decorator, self.vocabulary);

                for error in &parsed.errors {
                    self.add(owner, "relation-options", error);
                }

                head.options = Some(parsed);
            }
            Some([_, _]) => self.add(
                owner,
                "relation-options",
                &format!("{decorator} second argument must be an inline options object"),
            ),
            _ => self.add(
                owner,
                "relation-options",
                &format!(
                    "{decorator} must receive a direct target arrow and exactly one inline options object"
                ),
            ),
        }

        match target_identifier(relation) {
            None => self.add(
                owner,
                "relation-target",
                &format!(
                    "{decorator} target must be a direct zero-argument arrow to an entity identifier"
                ),
            ),
            Some(identifier) => {
                let target_name = identifier.name.as_str();
                let declared = types::declared(owner.member).and_then(|kind| {
                    types::target_type(catalog, owner.unit, kind, target_name)
                });
                head.nullability = declared.as_ref().map(|declared| declared.nullable);

                match declared {
                    None => self.add(
                        owner,
                        "relation-type",
                        &format!(
                            "type must be exactly Relation<{target_name}> or null | Relation<{target_name}>"
                        ),
                    ),
                    Some(declared) if !declared.wrapped => self.add(
                        owner,
                        "relation-wrapper",
                        &format!(
                            "type must wrap {target_name} as Relation<{target_name}> imported by name from {}",
                            catalog.vocabulary.package
                        ),
                    ),
                    Some(_) => {}
                }

                match target(catalog, owner.unit, class, identifier) {
                    Err(error) => self.add(owner, "relation-target", &error),
                    Ok((found, identity)) => {
                        if names::resource(&catalog.entities[found].name).is_none() {
                            self.add(
                                owner,
                                "relation-target",
                                &format!(
                                    "resolved target class {} must have a nonempty canonical *Entity name",
                                    catalog.entities[found].name
                                ),
                            );
                        }

                        head.entity = Some(found);
                        head.identity = Some(identity);
                    }
                }
            }
        }

        self.exception(owner, &mut head);

        head
    }

    /// Whether the relation meets its accepted nullability exception, and its nullability: the
    /// relation option against the property type, which the exception may differ in.
    fn exception(&mut self, owner: &Owner<'_, 'a>, head: &mut Head) {
        let catalog = self.catalog;
        let decorator = head.decorator.clone();

        head.excepted = owner.slot.is_some_and(|exception| {
            head.entity.is_some_and(|index| {
                let target = &catalog.entities[index];
                target.name == exception.target_class
                    && catalog.units[target.unit].path == exception.target_path
            }) && head.options.as_ref().is_some_and(|options| {
                options.nullable == Some(true)
                    && options.on_delete.as_deref() == Some(exception.on_delete.as_str())
                    && options.on_update.as_deref() == Some(exception.on_update.as_str())
                    && options.deferrable.is_none()
            }) && head.nullability == Some(false)
        });

        if let Some(exception) = owner.slot
            && !head.excepted
        {
            let named = exception.reason.as_ref().map_or_else(
                || exception.member.clone(),
                |reason| format!("{} {reason}", exception.member),
            );

            self.add(
                owner,
                "nullability-exception",
                &format!(
                    "{named} requires its exact target, non-null relation type, nullable=true, {}/{}, and absent deferrability tuple",
                    exception.on_delete, exception.on_update
                ),
            );
        }

        // A mismatch is reported either way. Only a relation that meets its entry carries the target
        // an exception accepts it by; one that departs from its entry cannot be accepted.
        if let (Some(option), Some(property)) = (head.option_nullable(), head.nullability)
            && option != property
        {
            let message = format!(
                "{decorator} option nullable={option} must match relation property nullability={property}"
            );

            if head.excepted {
                self.add_targeted(owner, "relation-nullability", &message);
            } else {
                self.add(owner, "relation-nullability", &message);
            }
        }
    }

    /// The role qualifier of a relation's name, after reporting a mutable role on a sole relation.
    fn naming(&mut self, owner: &Owner<'_, 'a>, head: &Head, targets: &Targets) -> Option<String> {
        let name = owner.name.as_str();
        let target = head.entity.map(|index| &self.catalog.entities[index]);
        let resource = target.and_then(|target| names::resource(&target.name));
        let stem = resource.as_ref().and(target).map(|target| {
            target
                .name
                .strip_suffix("Entity")
                .unwrap_or(target.name.as_str())
        });
        let role = stem.and_then(|stem| name.strip_suffix(stem)).unwrap_or("");
        let sole = head
            .identity
            .as_ref()
            .is_some_and(|identity| targets.counts.get(identity) == Some(&1));
        let forbidden = resource.as_ref().is_some_and(|resource| {
            sole && name != resource && names::mutable_role(role, self.vocabulary)
        });

        if forbidden && let Some(resource) = &resource {
            self.add(
                owner,
                "relation-name",
                &format!(
                    "must use unambiguous target-resource name {resource}; a sole relation must not carry mutable role prefix {role}"
                ),
            );

            return None;
        }

        let shared = head.identity.as_ref().and_then(|identity| {
            let own = vec![name.to_owned()];
            names::qualifier(name, targets.names.get(identity).unwrap_or(&own))
        });

        shared.or_else(|| (!role.is_empty()).then(|| role.to_owned()))
    }

    /// The join of a relation: its items, their names and references, their scalar types, and its
    /// foreign key name.
    fn join(
        &mut self,
        owner: &Owner<'_, 'a>,
        head: &Head,
        qualifier: Option<&str>,
        columns: &Columns<'a>,
        join: &Resolved<'_, 'a, 'v>,
    ) {
        let (errors, items) = grammar::join(join);

        for error in &errors {
            self.add(owner, "join-column", error);
        }

        if !errors.is_empty() {
            return;
        }

        let local: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
        let referenced: Vec<&str> = items.iter().map(|item| item.referenced.as_str()).collect();
        let member_id = format!("{}Id", owner.name);

        if qualifier.is_none()
            && columns.contains_key(&member_id)
            && !local.contains(&member_id.as_str())
        {
            self.add(
                owner,
                "join-column-name",
                &format!("must join through its own persisted member-id column {member_id}"),
            );
        }

        if let Some(qualifier) = qualifier
            && !local
                .iter()
                .any(|local| names::camel_prefix(local, qualifier))
        {
            self.add(
                owner,
                "join-column-name",
                &format!(
                    "role qualifier {qualifier} must prefix at least one of its own JoinColumn local names"
                ),
            );
        }

        if grammar::repeats(&local) {
            self.add(
                owner,
                "join-column",
                "JoinColumn repeats a local column component",
            );
        }

        if grammar::repeats(&referenced) {
            self.add(
                owner,
                "join-column",
                "JoinColumn repeats a referenced column component",
            );
        }

        let target = head.entity.map(|index| &self.catalog.entities[index]);

        if let Some(target) = target {
            for name in &referenced {
                if target.columns.get(*name).map_or(0, Vec::len) != 1 {
                    self.add(
                        owner,
                        "join-column-reference",
                        &format!(
                            "JoinColumn referenced name {} must resolve to exactly one persisted TypeORM column on {}",
                            quote(name),
                            target.name
                        ),
                    );
                }
            }
        }

        for name in &local {
            if others(columns, owner.member, name).len() != 1 {
                self.add(
                    owner,
                    "join-column-reference",
                    &format!(
                        "JoinColumn local name {} must resolve to exactly one same-class persisted TypeORM column",
                        quote(name)
                    ),
                );
            }
        }

        if let Some(target) = target {
            for item in &items {
                self.component(owner, head, columns, target, item, items.len());
            }
        }

        let foreign_key = items
            .first()
            .and_then(|item| item.foreign_key.clone())
            .unwrap_or_default();

        for error in self
            .spelling
            .map(|spelling| names::foreign_key(&foreign_key, spelling, self.vocabulary))
            .unwrap_or_default()
        {
            self.add(owner, "foreign-key-name", &error);
        }

        self.reuse
            .entry(foreign_key)
            .or_default()
            .insert(owner.label.clone(), (owner.path.to_owned(), owner.line));
    }

    /// One join component whose local and referenced names each resolve to one column: its scalar
    /// types, the nullability exception's component tuple and `SET NULL` nullability.
    fn component(
        &mut self,
        owner: &Owner<'_, 'a>,
        head: &Head,
        columns: &Columns<'a>,
        target: &Entity<'a>,
        item: &Item,
        items: usize,
    ) {
        let catalog = self.catalog;
        let locals = others(columns, owner.member, &item.name);
        let references = target
            .columns
            .get(&item.referenced)
            .cloned()
            .unwrap_or_default();
        let ([local_column], [referenced_column]) = (locals.as_slice(), references.as_slice())
        else {
            return;
        };
        let local_type = types::scalar(catalog, owner.unit, local_column.member);
        let referenced_type = types::scalar(catalog, target.unit, referenced_column.member);

        match (&local_type, &referenced_type) {
            (None, _) => self.add(
                owner,
                "join-column-type",
                &format!(
                    "JoinColumn local name {} must declare one explicit closed TypeScript scalar type and must not be optional",
                    quote(&item.name)
                ),
            ),
            (Some(_), None) => self.add(
                owner,
                "join-column-type",
                &format!(
                    "JoinColumn referenced name {} on {} must declare one explicit closed TypeScript scalar type and must not be optional",
                    quote(&item.referenced),
                    target.name
                ),
            ),
            (Some(local_scalar), Some(referenced_scalar))
                if local_scalar.identity != referenced_scalar.identity =>
            {
                let message = format!(
                    "JoinColumn local name {} scalar type {} must match referenced {}.{} scalar type {}",
                    quote(&item.name),
                    type_text(catalog.units[owner.unit].file, local_column),
                    target.name,
                    item.referenced,
                    type_text(catalog.units[target.unit].file, referenced_column)
                );
                self.add(owner, "join-column-type", &message);
            }
            _ => {}
        }

        if let Some(exception) = owner.slot {
            let scalar = Scalar {
                identity: format!("primitive:{}", exception.scalar.keyword()),
                nullable: false,
            };

            if items != 1
                || item.name != exception.local_name
                || item.referenced != exception.referenced_name
                || local_column.state.nullable
                || referenced_column.state.nullable
                || local_type.as_ref() != Some(&scalar)
                || referenced_type.as_ref() != Some(&scalar)
            {
                self.add(
                    owner,
                    "nullability-exception",
                    &format!(
                        "{} nullability exception requires the exact non-null {}:{} -> {}:{} component tuple",
                        exception.member,
                        exception.local_name,
                        exception.scalar.keyword(),
                        exception.referenced_name,
                        exception.scalar.keyword()
                    ),
                );
            }
        }

        let set_null = head.options.as_ref().is_some_and(|options| {
            options.on_delete.as_deref() == Some("SET NULL")
                || options.on_update.as_deref() == Some("SET NULL")
        });

        if set_null
            && (head.option_nullable() != Some(true)
                || head.nullability != Some(true)
                || local_column.state.primary
                || !local_column.state.nullable
                || local_type.as_ref().is_none_or(|scalar| !scalar.nullable))
        {
            self.add(
                owner,
                "relation-nullability",
                "SET NULL requires a nullable relation option/property and every local persisted column/scalar component to be nullable and non-primary",
            );
        }
    }
}

/// Whether a decorator's parent is a constructor, to which the compiler attaches no decorator
/// (`canHaveDecorators`).
fn on_constructor(parent: AstKind<'_>) -> bool {
    matches!(parent, AstKind::MethodDefinition(method) if constructor(method))
}

/// The owner relation one check reports on.
struct Owner<'o, 'a> {
    unit: usize,
    path: &'o str,
    line: u32,
    label: String,
    /// `<path>:<class>.<member>`, the target of an accepted nullability mismatch.
    target: String,
    name: String,
    member: &'o ClassElement<'a>,
    /// The nullable relation this slot is.
    slot: Option<&'o NullableRelation>,
}

/// What the relation decorator gives the rest of the check.
struct Head {
    decorator: String,
    options: Option<Options>,
    entity: Option<usize>,
    identity: Option<String>,
    /// Whether the relation property is `Target | null` (`true`) or `Target` (`false`).
    nullability: Option<bool>,
    excepted: bool,
}

impl Head {
    /// The `nullable` relation option.
    fn option_nullable(&self) -> Option<bool> {
        self.options.as_ref().and_then(|options| options.nullable)
    }
}

/// The persisted columns named `persisted` other than the relation member itself.
fn others<'a>(
    columns: &Columns<'a>,
    member: &ClassElement<'_>,
    persisted: &str,
) -> Vec<Column<'a>> {
    columns
        .get(persisted)
        .map(|columns| {
            columns
                .iter()
                .filter(|column| !std::ptr::eq(column.member, member))
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

/// The source text of a column's declared type.
fn type_text(file: &gate_typescript::SourceFile<'_>, column: &Column<'_>) -> String {
    types::declared(column.member).map_or_else(String::new, |kind| {
        let span = kind.span();
        file.source()[span.start as usize..span.end as usize].to_owned()
    })
}
