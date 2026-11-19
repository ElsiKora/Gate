//! Object literals read with their spreads: a spread of a constant object, also through
//! `Object.freeze`, imports and barrel re-exports, contributes its properties, and a spread of
//! Automator's `GetDefaultStringFormatProperties(format)` contributes its `format`.

use std::collections::{BTreeMap, BTreeSet};

use gate_typescript::owners::{
    Owner, SourceContext, TypeDeclaration, export_name, find_class, property_access, top_level,
    unwrap,
};
use gate_typescript::oxc_ast::ast::{
    CallExpression, Expression, ObjectExpression, ObjectPropertyKind, PropertyKey, PropertyKind,
    Statement, TSType,
};
use gate_typescript::value::string_like;

use super::Read;
use super::model::Kept;
use crate::shared::decorators::key_name;

/// How deep constants, spreads and re-exports are followed before a value counts as unknown.
const DEPTH: u8 = 12;

/// An expression and the file it is read in.
#[derive(Clone, Copy)]
pub(crate) struct Value<'s, 'a> {
    pub expression: &'a Expression<'a>,
    pub source: &'s SourceContext<'a>,
}

/// The properties of an object literal after its spreads; `complete` is false when a spread could
/// not be read, so a missing property may still be set.
pub(crate) struct Object<'s, 'a> {
    pub properties: BTreeMap<String, Value<'s, 'a>>,
    complete: bool,
    /// The values of properties whose computed keys cannot be read, for readers that need only
    /// the values, such as a discriminator mapping.
    pub keyless: Vec<Value<'s, 'a>>,
    /// Whether every spread was read, so no value is missing even when a key is unknown.
    pub spreads_read: bool,
}

impl<'s, 'a> Object<'s, 'a> {
    pub(crate) fn get(&self, name: &str) -> Option<Value<'s, 'a>> {
        self.properties.get(name).copied()
    }

    /// This object when it is fully read, so a missing key is truly absent; `None` when a spread or
    /// a computed key the reader cannot name may hide one.
    pub(crate) fn read(self) -> Option<Self> {
        self.complete.then_some(self)
    }

    /// Whether this object is fully read, as [`Self::read`] decides, for a reader that keeps the
    /// object either way.
    pub(crate) fn is_read(&self) -> bool {
        self.complete
    }

    /// Every value of this object, under a key the reader can name or not; `None` when a spread
    /// the reader cannot follow may hide one.
    pub(crate) fn values(&self) -> Option<impl Iterator<Item = Value<'s, 'a>> + '_> {
        self.spreads_read
            .then(|| self.properties.values().chain(&self.keyless).copied())
    }
}

impl<'s, 'a> Value<'s, 'a> {
    /// The object this value is, through constants, `Object.freeze` and property accesses.
    pub(crate) fn object(self, sources: Read<'s, 'a>) -> Option<Object<'s, 'a>> {
        object(sources, self.source, self.expression, 0)
    }

    /// The string this value is, as a literal or through constants and their properties.
    pub(crate) fn text(self, sources: Read<'s, 'a>) -> Option<String> {
        self.text_at(sources, 0)
    }

    fn text_at(self, sources: Read<'s, 'a>, depth: u8) -> Option<String> {
        if depth > DEPTH {
            return None;
        }

        let expression = unwrap(self.expression);

        if let Some(text) = string_like(expression).or_else(|| raw_text(expression)) {
            return Some(text);
        }

        let value = match expression {
            Expression::Identifier(identifier) => {
                constant(sources, self.source, &identifier.name, depth + 1)?
            }
            current => {
                let (parent, name) = property_access(current)?;
                object(sources, self.source, parent, depth + 1)?.get(name)?
            }
        };

        value.text_at(sources, depth + 1)
    }

    /// Whether this value is the literal `true`.
    pub(crate) fn is_true(self) -> bool {
        matches!(unwrap(self.expression), Expression::BooleanLiteral(boolean) if boolean.value)
    }

    /// Whether this value is the literal `false`.
    pub(crate) fn is_false(self) -> bool {
        matches!(unwrap(self.expression), Expression::BooleanLiteral(boolean) if !boolean.value)
    }

    /// The member name of `<enumeration>.<MEMBER>` when the enumeration is Automator's `name`.
    pub(crate) fn automator_member(self, sources: Read<'s, 'a>, name: &str) -> Option<String> {
        self.automator_member_at(sources, name, 0)
    }

    fn automator_member_at(self, sources: Read<'s, 'a>, name: &str, depth: u8) -> Option<String> {
        if depth > DEPTH {
            return None;
        }

        let expression = unwrap(self.expression);

        if let Expression::Identifier(identifier) = expression {
            return constant(sources, self.source, &identifier.name, depth + 1)?
                .automator_member_at(sources, name, depth + 1);
        }

        let (parent, member) = property_access(expression)?;
        let identity = self.source.identity(parent, true)?;

        if identity.module.as_deref() == Some(sources.names.automator.as_str())
            && identity.name == name
        {
            return Some(member.to_owned());
        }

        // A property of a constant object, such as `FORMATS.ID`, read through to its value.
        object(sources, self.source, parent, depth + 1)?
            .get(member)?
            .automator_member_at(sources, name, depth + 1)
    }

    /// The value a constant identifier or a property of a constant object holds, read through
    /// imports and re-exports.
    pub(crate) fn resolved(self, sources: Read<'s, 'a>) -> Option<Self> {
        match unwrap(self.expression) {
            Expression::Identifier(identifier) => {
                constant(sources, self.source, &identifier.name, 0)
            }
            current => {
                let (parent, name) = property_access(current)?;
                object(sources, self.source, parent, 0)?.get(name)
            }
        }
    }

    /// The class an expression or an arrow function returning one names.
    pub(crate) fn class(self, sources: Read<'_, 'a>) -> Option<Owner> {
        let expression = match unwrap(self.expression) {
            Expression::ArrowFunctionExpression(arrow) => unwrap(arrow.get_expression()?),
            other => other,
        };
        let identity = self.source.identity(expression, true)?;

        sources
            .canonical(Some(&identity))
            .filter(|owner| owner.path.is_some())
    }

    /// The classes a heritage clause extends and the members each edge keeps: the class it names,
    /// or the classes a mapped type (`PickType`, `OmitType`, `PartialType`, `IntersectionType`) is
    /// built from, through nested mapped types, with their literal key lists composed; and whether
    /// it holds another call built on a source class, whose members cannot be known.
    pub(crate) fn bases(self, sources: Read<'s, 'a>) -> (Vec<(Owner, Kept)>, bool) {
        let mut bases = Vec::new();
        let unresolved = self.mapped_bases(sources, &Kept::default(), &mut bases, 0);
        (bases, unresolved)
    }

    /// Adds the bases of a heritage expression; returns whether it holds a call other than a
    /// mapped type that builds on a source class.
    fn mapped_bases(
        self,
        sources: Read<'s, 'a>,
        kept: &Kept,
        bases: &mut Vec<(Owner, Kept)>,
        depth: u8,
    ) -> bool {
        let Expression::CallExpression(call) = unwrap(self.expression) else {
            let owner = self.class(sources);

            if let Some(owner) = owner
                .as_ref()
                .filter(|owner| is_source_class(sources, owner))
            {
                bases.push((owner.clone(), kept.clone()));
                return false;
            }

            // A constant that holds a mapped type, another call, a class or another such constant
            // is read through; a source declaration that is no class and cannot be read through,
            // or a chain deeper than the bound, is unknown.
            if depth >= DEPTH {
                return true;
            }

            return match self.resolved(sources) {
                Some(value) => value.mapped_bases(sources, kept, bases, depth + 1),
                None => owner.is_some(),
            };
        };
        let arguments: Vec<&'a Expression<'a>> = call
            .arguments
            .iter()
            .filter_map(|argument| argument.as_expression())
            .collect();
        let Some(identity) = self.source.identity(&call.callee, true) else {
            return self.builds_on_class(sources, &arguments);
        };
        let keys = || {
            arguments.get(1).and_then(|keys| {
                Value {
                    expression: keys,
                    source: self.source,
                }
                .keys(sources)
            })
        };
        let (built_from, kept) = match identity.name.as_str() {
            "IntersectionType" => (arguments.as_slice(), kept.clone()),
            "PartialType" => (arguments.get(..1).unwrap_or_default(), kept.clone()),
            "PickType" => (
                arguments.get(..1).unwrap_or_default(),
                keys().map_or_else(|| kept.clone(), |keys| kept.picked(&keys)),
            ),
            "OmitType" => (
                arguments.get(..1).unwrap_or_default(),
                keys().map_or_else(|| kept.clone(), |keys| kept.omitted(&keys)),
            ),
            _ => return self.builds_on_class(sources, &arguments),
        };
        let mut unresolved = false;

        for expression in built_from {
            unresolved |= Value {
                expression,
                source: self.source,
            }
            .mapped_bases(sources, &kept, bases, depth);
        }

        unresolved
    }

    /// Whether an argument of a call names a source class, directly, through another call or
    /// through a constant that holds either.
    fn builds_on_class(self, sources: Read<'s, 'a>, arguments: &[&'a Expression<'a>]) -> bool {
        arguments.iter().any(|expression| {
            Value {
                expression,
                source: self.source,
            }
            .names_class(sources, 0)
        })
    }

    fn names_class(self, sources: Read<'s, 'a>, depth: u8) -> bool {
        if let Expression::CallExpression(call) = unwrap(self.expression) {
            return call
                .arguments
                .iter()
                .filter_map(|argument| argument.as_expression())
                .any(|expression| {
                    Value {
                        expression,
                        source: self.source,
                    }
                    .names_class(sources, depth)
                });
        }

        self.class(sources)
            .is_some_and(|owner| is_source_class(sources, &owner))
            || depth < DEPTH
                && self
                    .resolved(sources)
                    .is_some_and(|value| value.names_class(sources, depth + 1))
    }

    /// The keys of a literal array of strings or string constants.
    fn keys(self, sources: Read<'_, 'a>) -> Option<BTreeSet<String>> {
        let Expression::ArrayExpression(array) = unwrap(self.expression) else {
            return None;
        };

        array
            .elements
            .iter()
            .map(|element| {
                element.as_expression().and_then(|expression| {
                    Value {
                        expression,
                        source: self.source,
                    }
                    .text(sources)
                })
            })
            .collect()
    }
}

/// The name of an object property key: an identifier, a string, or the member a computed
/// `[Enumeration.MEMBER]` key names.
pub(crate) fn property_key(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    if !computed {
        return key_name(key, false);
    }

    let expression = unwrap(key.as_expression()?);

    string_like(expression).or_else(|| property_access(expression).map(|(_, name)| name.to_owned()))
}

/// The name an object literal key gives its property: an identifier or string, the member of an
/// Automator enum (whose member names equal their values) for `[Enumeration.MEMBER]`, or the
/// string a computed key's constant holds; `None` when the computed key cannot be read.
fn object_key<'s, 'a>(
    sources: Read<'s, 'a>,
    source: &'s SourceContext<'a>,
    key: &'a PropertyKey<'a>,
    computed: bool,
) -> Option<String> {
    if !computed {
        return key_name(key, false);
    }

    let expression = unwrap(key.as_expression()?);

    if let Some(text) = string_like(expression) {
        return Some(text);
    }

    if let Some((parent, name)) = property_access(expression)
        && source.identity(parent, true).is_some_and(|identity| {
            identity.module.as_deref() == Some(sources.names.automator.as_str())
        })
    {
        return Some(name.to_owned());
    }

    Value { expression, source }.text(sources)
}

/// The object an expression is, read in `source`.
pub(crate) fn object<'s, 'a>(
    sources: Read<'s, 'a>,
    source: &'s SourceContext<'a>,
    expression: &'a Expression<'a>,
    depth: u8,
) -> Option<Object<'s, 'a>> {
    if depth > DEPTH {
        return None;
    }

    match unwrap(expression) {
        Expression::ObjectExpression(literal) => Some(properties(sources, source, literal, depth)),
        Expression::CallExpression(call) if is_freeze(call) => object(
            sources,
            source,
            call.arguments.first()?.as_expression()?,
            depth + 1,
        ),
        Expression::Identifier(identifier) => {
            let value = constant(sources, source, &identifier.name, depth + 1)?;
            object(sources, value.source, value.expression, depth + 1)
        }
        current => {
            let (parent, name) = property_access(current)?;
            let value = object(sources, source, parent, depth + 1)?.get(name)?;
            object(sources, value.source, value.expression, depth + 1)
        }
    }
}

/// The properties of an object literal in source order, a later key replacing an earlier one.
fn properties<'s, 'a>(
    sources: Read<'s, 'a>,
    source: &'s SourceContext<'a>,
    literal: &'a ObjectExpression<'a>,
    depth: u8,
) -> Object<'s, 'a> {
    let mut object_value = Object {
        properties: BTreeMap::new(),
        complete: true,
        keyless: Vec::new(),
        spreads_read: true,
    };

    for item in &literal.properties {
        match item {
            ObjectPropertyKind::ObjectProperty(property) => {
                if property.method || property.kind != PropertyKind::Init {
                    continue;
                }

                let value = Value {
                    expression: &property.value,
                    source,
                };

                if let Some(name) = object_key(sources, source, &property.key, property.computed) {
                    object_value.properties.insert(name, value);
                } else {
                    object_value.complete = false;
                    object_value.keyless.push(value);
                }
            }
            ObjectPropertyKind::SpreadProperty(spread) => {
                let argument = unwrap(&spread.argument);

                if let Expression::CallExpression(call) = argument
                    && source.identity(&call.callee, true).is_some_and(|identity| {
                        identity.module.as_deref() == Some(sources.names.automator.as_str())
                            && identity.name == "GetDefaultStringFormatProperties"
                    })
                {
                    if let Some(format) = call
                        .arguments
                        .first()
                        .and_then(|argument| argument.as_expression())
                    {
                        object_value.properties.insert(
                            "format".to_owned(),
                            Value {
                                expression: format,
                                source,
                            },
                        );
                    } else {
                        object_value.complete = false;
                        object_value.spreads_read = false;
                    }
                    continue;
                }

                if let Some(inner) = object(sources, source, argument, depth + 1) {
                    object_value.complete &= inner.complete;
                    object_value.spreads_read &= inner.spreads_read;
                    object_value.properties.extend(inner.properties);
                    object_value.keyless.extend(inner.keyless);
                } else {
                    object_value.complete = false;
                    object_value.spreads_read = false;
                }
            }
        }
    }

    object_value
}

/// Whether a call is `Object.freeze(…)`.
fn is_freeze(call: &CallExpression<'_>) -> bool {
    property_access(unwrap(&call.callee)).is_some_and(|(object, name)| {
        name == "freeze"
            && matches!(object, Expression::Identifier(identifier) if identifier.name == "Object")
    })
}

/// The initializer of the top-level constant `name` of `source`, followed through its import.
fn constant<'s, 'a>(
    sources: Read<'s, 'a>,
    source: &'s SourceContext<'a>,
    name: &str,
    depth: u8,
) -> Option<Value<'s, 'a>> {
    if let Some(declarator) = source.variables.get(name) {
        return Some(Value {
            expression: declarator.init.as_ref()?,
            source,
        });
    }

    let import = source.imports.get(name)?;
    exported(
        sources,
        sources.sources.get(import.source.as_deref()?)?,
        &import.imported,
        depth + 1,
    )
}

/// The initializer of the constant a file exports as `name`, through its imports and its named
/// and star re-exports.
fn exported<'s, 'a>(
    sources: Read<'s, 'a>,
    source: &'s SourceContext<'a>,
    name: &str,
    depth: u8,
) -> Option<Value<'s, 'a>> {
    if depth > DEPTH {
        return None;
    }

    if let Some(declarator) = source.variables.get(name) {
        return Some(Value {
            expression: declarator.init.as_ref()?,
            source,
        });
    }

    if source.imports.contains_key(name) {
        return constant(sources, source, name, depth + 1);
    }

    for statement in top_level(&source.file) {
        match statement {
            Statement::ExportFromDeclaration(export) => {
                let Some(specifier) = export
                    .specifiers
                    .iter()
                    .find(|specifier| export_name(&specifier.exported) == name)
                else {
                    continue;
                };
                let target = sources
                    .sources
                    .get(&source.specifier(export.source.value.as_str())?)?;

                return exported(sources, target, &export_name(&specifier.local), depth + 1);
            }
            Statement::ExportAllDeclaration(export) if export.exported.is_none() => {
                if let Some(target) = source
                    .specifier(export.source.value.as_str())
                    .and_then(|path| sources.sources.get(&path))
                    && let Some(found) = exported(sources, target, name, depth + 1)
                {
                    return Some(found);
                }
            }
            _ => {}
        }
    }

    None
}

/// What a property's TypeScript type says about the text it can hold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Annotation {
    /// `string`, alone or with `null` or `undefined`.
    String,
    /// Only repository enums, with `null` or `undefined`; the first enum's name.
    Enum(String),
    /// Only string literal types.
    Literal,
    /// A template literal type or a generic type the reader does not resolve, which `typeorm` may
    /// still reflect as a string.
    Unknown,
    /// Anything else, or no annotation.
    Other,
}

/// The annotation of a property type, read in `source`.
pub(crate) fn annotation(
    sources: Read<'_, '_>,
    source: &SourceContext<'_>,
    kind: Option<&TSType<'_>>,
) -> Annotation {
    annotation_at(sources, source, kind, 0)
}

fn annotation_at(
    sources: Read<'_, '_>,
    source: &SourceContext<'_>,
    kind: Option<&TSType<'_>>,
    depth: u8,
) -> Annotation {
    let Some(kind) = kind.filter(|_| depth <= DEPTH) else {
        return Annotation::Other;
    };
    let members: Vec<&TSType<'_>> = match kind {
        TSType::TSUnionType(union) => union
            .types
            .iter()
            .filter(|member| {
                !matches!(
                    member,
                    TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)
                )
            })
            .collect(),
        other => vec![other],
    };

    if members.is_empty() {
        return Annotation::Other;
    }

    if members
        .iter()
        .any(|member| matches!(member, TSType::TSStringKeyword(_)))
    {
        return Annotation::String;
    }

    if members
        .iter()
        .all(|member| matches!(member, TSType::TSLiteralType(_)))
    {
        return Annotation::Literal;
    }

    if members
        .iter()
        .any(|member| matches!(member, TSType::TSTemplateLiteralType(_)))
    {
        return Annotation::Unknown;
    }

    let mut first = None;

    for member in &members {
        let TSType::TSTypeReference(reference) = member else {
            return Annotation::Other;
        };

        if reference.type_arguments.is_some() {
            return Annotation::Unknown;
        }

        let Some(owner) = source
            .type_identity(&reference.type_name)
            .and_then(|identity| sources.canonical(Some(&identity)))
        else {
            return Annotation::Other;
        };

        if is_enum(sources, &owner) {
            first.get_or_insert(owner.name);
            continue;
        }

        // A type alias stands for what it names, as TypeORM reflects the aliased type.
        if members.len() == 1
            && let Some(declaring) = owner
                .path
                .as_deref()
                .and_then(|path| sources.sources.get(path))
            && let Some(TypeDeclaration::Alias(alias)) = declaring.types.get(&owner.name)
        {
            return annotation_at(sources, declaring, Some(&alias.type_annotation), depth + 1);
        }

        return Annotation::Other;
    }

    first.map_or(Annotation::Other, Annotation::Enum)
}

/// Whether an owner is a top-level enum declaration of a source file.
fn is_enum(sources: Read<'_, '_>, owner: &Owner) -> bool {
    let Some(source) = owner
        .path
        .as_deref()
        .and_then(|path| sources.sources.get(path))
    else {
        return false;
    };

    top_level(&source.file).any(|statement| {
        let declaration = match statement {
            Statement::TSEnumDeclaration(declaration) => declaration,
            Statement::ExportDeclaration(export) => match &export.declaration {
                gate_typescript::oxc_ast::ast::Declaration::TSEnumDeclaration(declaration) => {
                    declaration
                }
                _ => return false,
            },
            _ => return false,
        };

        declaration.id.name == owner.name.as_str()
    })
}

/// The raw text of a `String.raw` template without substitutions.
fn raw_text(expression: &Expression<'_>) -> Option<String> {
    let Expression::TaggedTemplateExpression(tagged) = expression else {
        return None;
    };
    let (object, name) = property_access(&tagged.tag)?;

    match (unwrap(object), name, tagged.quasi.quasis.as_slice()) {
        (Expression::Identifier(identifier), "raw", [quasi])
            if identifier.name == "String" && tagged.quasi.expressions.is_empty() =>
        {
            Some(quasi.value.raw.to_string())
        }
        _ => None,
    }
}

/// Whether the owner is a class declared in a source file.
fn is_source_class(sources: Read<'_, '_>, owner: &Owner) -> bool {
    owner
        .path
        .as_deref()
        .and_then(|path| sources.sources.get(path))
        .is_some_and(|source| find_class(&source.file, &owner.name).is_some())
}
