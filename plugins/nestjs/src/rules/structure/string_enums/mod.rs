//! `nestjs/string-enums`: string-literal variant contracts use enums.
//!
//! Every `.ts` file (not `.d.ts`) below the source root, the test root and the code folders of the
//! infrastructure-as-code root, outside skipped folders, is analysed on its own, with the TypeScript
//! binder's view of one file: names resolve to the declarations of that file only, and an
//! unresolved type name is a library type. Namespace forms beyond the ones the binder follows are
//! reported as `unsupported-namespace` instead of being resolved. A source enum must not copy an
//! installed `ErrorString` action enum (`error-string-shim`).

mod error_string;
mod namespace;

use std::collections::{BTreeMap, HashMap, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::binder::{Binder, Meaning};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Expression, IdentifierReference, PropertyKey, TSEnumDeclaration, TSEnumMember,
    TSEnumMemberName, TSInterfaceDeclaration, TSLiteral, TSModuleReference, TSSignature, TSType,
    TSTypeAliasDeclaration, TSTypeAnnotation, TSTypeName, TSTypeOperatorOperator,
    TSTypeQueryExprName, TSUnionType, TemplateLiteral,
};
use gate_typescript::oxc_semantic::Semantic;
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::oxc_syntax::symbol::{SymbolFlags, SymbolId};
use gate_typescript::{Allocator, Settings, SourceFile, TypeScript};

use crate::layout::{Layout, PluginOptions, folder_prefix};
use crate::shared::report::Findings;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct StringEnums;

/// The options of `nestjs/string-enums`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// Property names that are discriminants wherever they hold a string literal type.
    pub discriminant_names: Vec<String>,
    /// The installed action enums no source enum may copy.
    pub shim_enum_names: Vec<String>,
    /// Library generic types whose argument at the given index is a key list, not a variant.
    pub key_type_arguments: BTreeMap<String, usize>,
    /// The longest chain of `import N = …` aliases followed to a namespace.
    #[schemars(range(min = 1))]
    pub alias_depth: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            discriminant_names: ["action", "kind", "mode", "semantic", "state"]
                .map(str::to_owned)
                .to_vec(),
            shim_enum_names: ["EErrorStringAction", "EErrorStringCompositeAction"]
                .map(str::to_owned)
                .to_vec(),
            key_type_arguments: [("Omit", 1), ("Pick", 1), ("Record", 0)]
                .into_iter()
                .map(|(name, index)| (name.to_owned(), index))
                .collect(),
            alias_depth: 16,
        }
    }
}

impl Rule for StringEnums {
    type Options = Options;

    const NAME: &'static str = "string-enums";
    const TITLE: &'static str = "String-literal variant contracts use enums";
    const ASPECTS: &'static [&'static str] = &[
        "error-string-shim",
        "keyof-typeof-alias",
        "literal-union",
        "raw-discriminant",
        "raw-enum-value",
        "test-literal-union",
        "unsupported-namespace",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let test_prefix = folder_prefix(&layout.test_root);
        let source_prefix = layout.source_prefix();
        let mut findings = Findings::default();

        for path in files(&source, &layout, typescript.settings()) {
            let Some(file) = project::parsed(&typescript, path)? else {
                continue;
            };

            Analysis::new(options, &file, path.starts_with(&test_prefix))
                .report(path, &mut findings);
            namespace::report(&file, path, &mut findings);

            if path.starts_with(&source_prefix) {
                error_string::report(&file, path, &options.shim_enum_names, &mut findings);
            }
        }

        findings.report(run, help);
        Ok(())
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "error-string-shim" => "use the installed enum's members",
        "keyof-typeof-alias" => "use the enum type directly",
        "unsupported-namespace" => "declare the enum or namespace in a form the binder resolves",
        "raw-enum-value" | "raw-discriminant" => "use the enum member instead of the raw string",
        _ => "declare an enum for the variants and use its members",
    }
}

/// The `.ts` files the rule reads, in path order: below the source and test roots and the code
/// folders of the infrastructure-as-code root, without declaration files.
fn files<'s>(
    source: &'s dyn gate_typescript::Source,
    layout: &Layout,
    settings: &Settings,
) -> Vec<&'s str> {
    let mut roots = vec![layout.source_root.clone(), layout.test_root.clone()];
    roots.extend(
        layout
            .cdk_code_folders
            .iter()
            .map(|folder| format!("{}{folder}", folder_prefix(&layout.cdk_root))),
    );
    let mut files: Vec<&str> = roots
        .iter()
        .flat_map(|root| tree::typescript_files(source, root, settings))
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "ts")
                && !path.ends_with(".d.ts")
        })
        .collect();
    files.sort_unstable();
    files.dedup();
    files
}

/// What a type name at one location denotes.
enum TypeDeclaration<'a> {
    /// Nothing in the file: a library type such as `Pick`.
    Library,
    Alias(&'a TSTypeAliasDeclaration<'a>),
    Interface(&'a TSInterfaceDeclaration<'a>),
    /// A class, enum, type parameter, import or merged declaration.
    Other,
}

/// One discriminant property found to take several literal values.
struct Discriminant {
    property: String,
    value: String,
}

struct Analysis<'s, 'a> {
    options: &'s Options,
    file: &'s SourceFile<'a>,
    semantic: &'s Semantic<'a>,
    binder: Binder<'s, 'a>,
    test: bool,
    /// The string literals that name or initialize an enum member.
    enum_literals: HashSet<NodeId>,
    /// Each string enum value with the sorted `Enum.MEMBER` names that own it.
    owners: HashMap<String, Vec<String>>,
    discriminants: HashMap<NodeId, Discriminant>,
}

impl<'s, 'a> Analysis<'s, 'a> {
    fn new(options: &'s Options, file: &'s SourceFile<'a>, test: bool) -> Self {
        let semantic = file.semantic();
        let mut analysis = Self {
            options,
            file,
            semantic,
            binder: Binder::new(file),
            test,
            enum_literals: HashSet::new(),
            owners: HashMap::new(),
            discriminants: HashMap::new(),
        };
        let mut conventional: HashMap<String, Vec<(NodeId, String)>> = HashMap::new();
        let mut unions = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::TSEnumDeclaration(declaration) => analysis.collect_enum(declaration),
                AstKind::TSUnionType(union) => unions.push(union),
                kind => {
                    if let Some((property, value)) =
                        string_literal_property(kind, Some(&options.discriminant_names))
                    {
                        conventional
                            .entry(property)
                            .or_default()
                            .push((node.id(), value));
                    }
                }
            }
        }

        for owners in analysis.owners.values_mut() {
            owners.sort();
        }

        for (property, observations) in conventional {
            let values: HashSet<&str> = observations
                .iter()
                .map(|(_, value)| value.as_str())
                .collect();

            if values.len() < 2 {
                continue;
            }

            for (id, value) in observations {
                analysis.discriminants.insert(
                    id,
                    Discriminant {
                        property: property.clone(),
                        value,
                    },
                );
            }
        }

        for union in unions {
            analysis.collect_union_discriminants(union);
        }

        analysis
    }

    fn collect_enum(&mut self, declaration: &TSEnumDeclaration<'a>) {
        for member in &declaration.body.members {
            if let TSEnumMemberName::String(name) = &member.id {
                self.enum_literals.insert(name.node_id.get());
            }

            let Some(initializer) = &member.initializer else {
                continue;
            };
            let initializer = unwrap_expression(initializer);
            let Some(value) = string_expression(initializer) else {
                continue;
            };

            let owner = format!(
                "{}.{}",
                declaration.id.name,
                member_name(self.file.source(), member)
            );
            self.enum_literals.insert(literal_node(initializer));
            self.owners.entry(value).or_default().push(owner);
        }
    }

    /// Marks a property that every variant of a union declares with a different literal value.
    fn collect_union_discriminants(&mut self, union: &'a TSUnionType<'a>) {
        let mut variants = Vec::new();

        for variant in &union.types {
            match self.variant_members(variant, &mut Vec::new()) {
                Some(members) => variants.push(members),
                None => return,
            }
        }

        let properties: Vec<HashMap<String, (NodeId, String)>> = variants
            .iter()
            .map(|members| {
                members
                    .iter()
                    .filter_map(|member| match member {
                        TSSignature::TSPropertySignature(signature) if !signature.optional => {
                            string_literal_property(AstKind::TSPropertySignature(signature), None)
                                .map(|(name, value)| (name, (signature.node_id.get(), value)))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        let Some(first) = properties.first() else {
            return;
        };

        for name in first.keys() {
            let observations: Option<Vec<&(NodeId, String)>> =
                properties.iter().map(|variant| variant.get(name)).collect();
            let Some(observations) = observations else {
                continue;
            };
            let values: HashSet<&str> = observations
                .iter()
                .map(|(_, value)| value.as_str())
                .collect();

            if values.len() < 2 {
                continue;
            }

            for (id, value) in observations {
                self.discriminants.insert(
                    *id,
                    Discriminant {
                        property: name.clone(),
                        value: value.clone(),
                    },
                );
            }
        }
    }

    /// The members of a variant whose whole shape is visible in the file, if it has one.
    fn variant_members(
        &self,
        variant: &'a TSType<'a>,
        visited: &mut Vec<NodeId>,
    ) -> Option<&'a [TSSignature<'a>]> {
        match unwrap_type(variant) {
            TSType::TSTypeLiteral(literal) => Some(&literal.members),
            TSType::TSTypeReference(reference) => {
                let TSTypeName::IdentifierReference(name) = &reference.type_name else {
                    return None;
                };

                match self.type_declaration(name) {
                    TypeDeclaration::Library if name.name == "Readonly" => {
                        match reference
                            .type_arguments
                            .as_deref()
                            .map(|arguments| &arguments.params)
                        {
                            Some(params) if params.len() == 1 => {
                                self.variant_members(&params[0], visited)
                            }
                            _ => None,
                        }
                    }
                    TypeDeclaration::Alias(alias) => {
                        let id = alias.node_id.get();

                        if visited.contains(&id) {
                            return None;
                        }

                        visited.push(id);
                        self.variant_members(&alias.type_annotation, visited)
                    }
                    TypeDeclaration::Interface(interface) => {
                        let id = interface.node_id.get();

                        // Only an interface without `extends` shows its complete shape.
                        if visited.contains(&id) || !interface.extends.is_empty() {
                            return None;
                        }

                        Some(&interface.body.body)
                    }
                    TypeDeclaration::Library | TypeDeclaration::Other => None,
                }
            }
            _ => None,
        }
    }

    fn type_declaration(&self, name: &IdentifierReference<'a>) -> TypeDeclaration<'a> {
        let Some(symbol) = self.resolve(name, Meaning::Type) else {
            return TypeDeclaration::Library;
        };
        match self.binder.declarations(symbol).as_slice() {
            [declaration] => match self.semantic.nodes().kind(*declaration) {
                AstKind::TSTypeAliasDeclaration(alias) => TypeDeclaration::Alias(alias),
                AstKind::TSInterfaceDeclaration(interface) => TypeDeclaration::Interface(interface),
                _ => TypeDeclaration::Other,
            },
            _ => TypeDeclaration::Other,
        }
    }

    fn resolve(&self, name: &IdentifierReference<'a>, meaning: Meaning) -> Option<SymbolId> {
        self.binder.resolve(name, meaning)
    }

    /// Whether a union or string literal is the key argument of the library `Pick`, `Omit` or
    /// `Record`, through parentheses and enclosing unions.
    fn is_utility_key(&self, node: NodeId) -> bool {
        let nodes = self.semantic.nodes();
        let mut argument = node;

        if matches!(nodes.parent_kind(argument), AstKind::TSLiteralType(_)) {
            argument = nodes.parent_id(argument);
        }

        let mut parent = nodes.parent_id(argument);

        while matches!(
            nodes.kind(parent),
            AstKind::TSParenthesizedType(_) | AstKind::TSUnionType(_)
        ) {
            argument = parent;
            parent = nodes.parent_id(parent);
        }

        let AstKind::TSTypeParameterInstantiation(instantiation) = nodes.kind(parent) else {
            return false;
        };
        let name = match nodes.parent_kind(parent) {
            AstKind::TSTypeReference(reference) => &reference.type_name,
            AstKind::TSInterfaceHeritage(heritage) => &heritage.type_name,
            AstKind::TSClassImplements(implements) => &implements.expression,
            _ => return false,
        };
        let TSTypeName::IdentifierReference(name) = name else {
            return false;
        };
        let Some(&index) = self.options.key_type_arguments.get(name.name.as_str()) else {
            return false;
        };

        instantiation
            .params
            .get(index)
            .is_some_and(|param| type_node(param) == Some(argument))
            && matches!(self.type_declaration(name), TypeDeclaration::Library)
    }

    /// The construct of `type X = keyof typeof N.E` when a prefix of the name (`N`, or `O.N` in
    /// `O.N.E`) is a value rather than a namespace, an enum or an `import N = …` alias: only the
    /// type of that value tells what the name denotes, and gate reads no types.
    fn keyof_value_root(&self, alias: &TSTypeAliasDeclaration<'a>) -> Option<String> {
        let TSType::TSTypeOperatorType(operator) = unwrap_type(&alias.type_annotation) else {
            return None;
        };
        let TSType::TSTypeQuery(query) = unwrap_type(&operator.type_annotation) else {
            return None;
        };
        let TSTypeQueryExprName::QualifiedName(qualified) = &query.expr_name else {
            return None;
        };

        if operator.operator != TSTypeOperatorOperator::Keyof {
            return None;
        }

        let mut prefix = &qualified.left;

        loop {
            let resolved = match prefix {
                TSTypeName::IdentifierReference(name) => self.resolve(name, Meaning::Value),
                TSTypeName::QualifiedName(name) => {
                    self.binder.resolve_qualified(name, Meaning::Value)
                }
                TSTypeName::ThisExpression(_) => None,
            };

            if !resolved.is_some_and(|symbol| self.names_namespace(symbol, 0)) {
                let span = prefix.span();
                let text = &self.file.source()[span.start as usize..span.end as usize];

                return Some(format!(
                    "`keyof typeof` on the value {text}, whose type the rule does not know"
                ));
            }

            match prefix {
                TSTypeName::QualifiedName(name) => prefix = &name.left,
                _ => return None,
            }
        }
    }

    /// Whether `symbol` is a namespace or an enum, or an `import N = …` alias whose target is one;
    /// an alias chain longer than the binder follows, or a cycle, is not.
    fn names_namespace(&self, symbol: SymbolId, depth: usize) -> bool {
        let scoping = self.semantic.scoping();
        let flags = scoping.symbol_flags(symbol);

        if flags
            .intersects(SymbolFlags::ValueModule | SymbolFlags::NamespaceModule | SymbolFlags::Enum)
        {
            return true;
        }

        if depth > self.options.alias_depth || !flags.intersects(SymbolFlags::Import) {
            return false;
        }

        let AstKind::TSImportEqualsDeclaration(alias) = self
            .semantic
            .nodes()
            .kind(scoping.symbol_declaration(symbol))
        else {
            return false;
        };
        let target = match &alias.module_reference {
            TSModuleReference::IdentifierReference(name) => self.resolve(name, Meaning::Value),
            TSModuleReference::QualifiedName(name) => {
                self.binder.resolve_qualified(name, Meaning::Value)
            }
            TSModuleReference::ExternalModuleReference(_) => None,
        };

        target.is_some_and(|target| self.names_namespace(target, depth + 1))
    }

    /// `type X = keyof typeof E` where every declaration of `E` is a non-empty enum whose
    /// members are their own names.
    fn is_identity_keyof_alias(&self, alias: &TSTypeAliasDeclaration<'a>) -> bool {
        let TSType::TSTypeOperatorType(operator) = unwrap_type(&alias.type_annotation) else {
            return false;
        };

        if operator.operator != TSTypeOperatorOperator::Keyof {
            return false;
        }

        let TSType::TSTypeQuery(query) = unwrap_type(&operator.type_annotation) else {
            return false;
        };
        let symbol = match &query.expr_name {
            TSTypeQueryExprName::IdentifierReference(name) => self.resolve(name, Meaning::Value),
            TSTypeQueryExprName::QualifiedName(name) => {
                self.binder.resolve_qualified(name, Meaning::Value)
            }
            _ => None,
        };
        let Some(symbol) = symbol else {
            return false;
        };
        let nodes = self.semantic.nodes();
        let declarations = self.binder.declarations(symbol);

        !declarations.is_empty()
            && declarations
                .iter()
                .all(|&declaration| match nodes.kind(declaration) {
                    AstKind::TSEnumDeclaration(declaration) => {
                        !declaration.body.members.is_empty()
                            && declaration.body.members.iter().all(|member| {
                                member.initializer.as_ref().is_some_and(|initializer| {
                                    string_expression(unwrap_expression(initializer)).is_some_and(
                                        |value| value == member_name(self.file.source(), member),
                                    )
                                })
                            })
                    }
                    _ => false,
                })
    }

    fn report(&self, path: &str, findings: &mut Findings) {
        let nodes = self.semantic.nodes();

        for node in nodes.iter() {
            let id = node.id();

            match node.kind() {
                AstKind::TSUnionType(union) if self.is_literal_union(union, id) => {
                    let aspect = if self.test {
                        "test-literal-union"
                    } else {
                        "literal-union"
                    };
                    findings.push(
                        aspect,
                        path,
                        Some(self.file.line(union.span.start)),
                        "uses a literal union type; use an enum instead",
                    );
                }
                AstKind::TSTypeAliasDeclaration(alias)
                    if let Some(construct) = self.keyof_value_root(alias)
                        && !self.test =>
                {
                    let start = match nodes.parent_kind(id) {
                        AstKind::ExportDeclaration(export) => export.span.start,
                        _ => alias.span.start,
                    };
                    namespace::report_construct(self.file, path, start, &construct, findings);
                }
                AstKind::TSTypeAliasDeclaration(alias)
                    if !self.test && self.is_identity_keyof_alias(alias) =>
                {
                    let start = match nodes.parent_kind(id) {
                        AstKind::ExportDeclaration(export) => export.span.start,
                        _ => alias.span.start,
                    };
                    findings.push(
                        "keyof-typeof-alias",
                        path,
                        Some(self.file.line(start)),
                        "uses a standalone keyof typeof alias; use the enum type directly",
                    );
                }
                AstKind::StringLiteral(_) | AstKind::TemplateLiteral(_) if !self.test => {
                    self.report_raw_enum_value(path, node.kind(), id, findings);
                }
                kind if !self.test => {
                    if let Some(discriminant) = self.discriminants.get(&id) {
                        findings.push(
                            "raw-discriminant",
                            path,
                            Some(self.file.line(kind.span().start)),
                            format!(
                                "uses raw multi-value {:?} discriminant {:?}; use an enum member instead",
                                discriminant.property, discriminant.value
                            ),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn is_literal_union(&self, union: &TSUnionType<'a>, id: NodeId) -> bool {
        let variants: Vec<&TSType<'a>> = union
            .types
            .iter()
            .map(unwrap_type)
            .filter(|variant| {
                !matches!(
                    variant,
                    TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_)
                )
            })
            .collect();

        variants.len() > 1
            && variants
                .iter()
                .all(|variant| string_literal_type(variant).is_some())
            && !self.is_utility_key(id)
    }

    fn report_raw_enum_value(
        &self,
        path: &str,
        kind: AstKind<'a>,
        id: NodeId,
        findings: &mut Findings,
    ) {
        let value = match kind {
            AstKind::StringLiteral(literal) => Some(literal.value.to_string()),
            AstKind::TemplateLiteral(template) => no_substitution_value(template),
            _ => None,
        };
        let Some(value) = value else {
            return;
        };
        let Some(owners) = self.owners.get(&value) else {
            return;
        };

        if self.enum_literals.contains(&id) || self.is_utility_key(id) {
            return;
        }

        findings.push(
            "raw-enum-value",
            path,
            Some(self.file.line(kind.span().start)),
            format!(
                "repeats raw string enum value {value:?} owned by {}; use the enum member instead",
                owners.join(", ")
            ),
        );
    }
}

/// The node of a type argument, to compare it with an ancestor of a union or literal.
fn type_node(ty: &TSType<'_>) -> Option<NodeId> {
    match ty {
        TSType::TSLiteralType(literal) => Some(literal.node_id.get()),
        TSType::TSParenthesizedType(parenthesized) => Some(parenthesized.node_id.get()),
        TSType::TSUnionType(union) => Some(union.node_id.get()),
        _ => None,
    }
}

/// The name and string literal type of a property declaration or signature; with
/// `conventional`, only for the conventional discriminant names.
fn string_literal_property(
    kind: AstKind<'_>,
    conventional: Option<&[String]>,
) -> Option<(String, String)> {
    let (key, computed, annotation): (&PropertyKey<'_>, bool, Option<&TSTypeAnnotation<'_>>) =
        match kind {
            AstKind::PropertyDefinition(property) => (
                &property.key,
                property.computed,
                property.type_annotation.as_deref(),
            ),
            AstKind::AccessorProperty(property) => (
                &property.key,
                property.computed,
                property.type_annotation.as_deref(),
            ),
            AstKind::TSPropertySignature(signature) => (
                &signature.key,
                signature.computed,
                signature.type_annotation.as_deref(),
            ),
            _ => return None,
        };
    let name = property_name(key, computed)?;

    if conventional.is_some_and(|names| !names.contains(&name)) {
        return None;
    }

    let value = string_literal_type(&annotation?.type_annotation)?;
    Some((name, value))
}

/// A property name that is an identifier, a string literal or a computed string literal.
fn property_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) if !computed => Some(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) if !computed => Some(literal.value.to_string()),
        _ if computed => key
            .as_expression()
            .and_then(|expression| string_expression(unwrap_expression(expression))),
        _ => None,
    }
}

/// The value of a string literal type (`"a"` or `` `a` ``), through parentheses.
fn string_literal_type(ty: &TSType<'_>) -> Option<String> {
    match unwrap_type(ty) {
        TSType::TSLiteralType(literal) => match &literal.literal {
            TSLiteral::StringLiteral(literal) => Some(literal.value.to_string()),
            TSLiteral::TemplateLiteral(template) => no_substitution_value(template),
            _ => None,
        },
        _ => None,
    }
}

/// The member name as the TypeScript compiler prints it; a computed name keeps its brackets.
pub(super) fn member_name(source: &str, member: &TSEnumMember<'_>) -> String {
    match &member.id {
        TSEnumMemberName::Identifier(identifier) => identifier.name.to_string(),
        TSEnumMemberName::String(literal) => literal.value.to_string(),
        TSEnumMemberName::ComputedString(_) | TSEnumMemberName::ComputedTemplateString(_) => {
            // The member starts at its `[`; the name ends at the first `]` after the key.
            let start = member.span.start as usize;
            let key_end = member.id.span().end as usize;
            let end = source[key_end..]
                .find(']')
                .map_or(key_end, |offset| key_end + offset + 1);
            source[start..end].to_owned()
        }
    }
}

/// The value of a string literal or a template literal without substitutions.
pub(super) fn string_expression(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::TemplateLiteral(template) => no_substitution_value(template),
        _ => None,
    }
}

fn no_substitution_value(template: &TemplateLiteral<'_>) -> Option<String> {
    match template.quasis.as_slice() {
        [quasi] if template.expressions.is_empty() => {
            quasi.value.cooked.as_ref().map(ToString::to_string)
        }
        _ => None,
    }
}

/// The node of a string or template literal expression.
fn literal_node(expression: &Expression<'_>) -> NodeId {
    match expression {
        Expression::StringLiteral(literal) => literal.node_id.get(),
        Expression::TemplateLiteral(template) => template.node_id.get(),
        _ => NodeId::DUMMY,
    }
}

fn unwrap_type<'t, 'a>(mut ty: &'t TSType<'a>) -> &'t TSType<'a> {
    while let TSType::TSParenthesizedType(parenthesized) = ty {
        ty = &parenthesized.type_annotation;
    }
    ty
}

pub(super) fn unwrap_expression<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    loop {
        expression = match expression {
            Expression::TSAsExpression(inner) => &inner.expression,
            Expression::TSSatisfiesExpression(inner) => &inner.expression,
            Expression::TSNonNullExpression(inner) => &inner.expression,
            Expression::TSTypeAssertion(inner) => &inner.expression,
            Expression::ParenthesizedExpression(inner) => &inner.expression,
            _ => return expression,
        };
    }
}
