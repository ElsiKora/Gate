//! The architecture facts of one source file, read from its own syntax and bindings: the routes of
//! its controllers, the modules its Nest modules export, and the public method parameters of a
//! domain service that hold the native entity manager.

use std::cell::OnceCell;
use std::collections::HashSet;

use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, BinaryOperator, BindingPattern,
    CallExpression, ChainElement, Declaration, Expression, FormalParameters, MethodDefinitionKind,
    ObjectPropertyKind, PropertyKey, PropertyKind, Statement, TSAccessibility, TSModuleReference,
    TSType, TSTypeName, TSTypeReference, VariableDeclarationKind,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::symbol::SymbolId;

use super::{Decorators, Import};

/// The routes of one controller decorator.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Controller {
    /// The literal start of each route that has one.
    pub literal_prefixes: Vec<String>,
    /// Whether every route starts with the internal API base path.
    pub uses_internal_base: bool,
}

/// The facts of one file.
#[derive(Debug, Default)]
pub struct Facts {
    pub controllers: Vec<Controller>,
    /// The module specifier and imported name of each module a Nest module exports.
    pub forwarded: Vec<(String, String)>,
    /// Each route as text, with `\u{0}` for a part that is not statically known.
    pub route_shapes: Vec<String>,
    /// `method(parameter)` for each public method parameter that holds a native `EntityManager`.
    pub managers: Vec<String>,
}

/// The internal API base a route may start with: an export path of one of `modules`.
#[derive(Debug)]
pub struct InternalBase<'p> {
    pub modules: &'p [String],
    pub export: &'p [String],
}

/// A value a route or metadata reading reaches: an expression, or another node such as a spread
/// argument, an array hole or a block body, which is never a known value.
#[derive(Clone, Copy, Debug)]
enum Node<'a> {
    Expression(&'a Expression<'a>),
    Other(usize),
}

impl Node<'_> {
    fn address(self) -> usize {
        match self {
            Self::Expression(expression) => std::ptr::from_ref(expression) as usize,
            Self::Other(address) => address,
        }
    }
}

/// The value of a metadata property: found, unknown (an unresolved spread or a non-literal key
/// stops the search) or absent.
enum Property<'a> {
    Found(Node<'a>),
    Unknown,
    Missing,
}

fn other<T>(node: &T) -> Node<'_> {
    Node::Other(std::ptr::from_ref(node) as usize)
}

fn argument<'a>(argument: Option<&'a Argument<'a>>) -> Option<Node<'a>> {
    match argument? {
        Argument::SpreadElement(spread) => Some(other(&**spread)),
        argument => argument.as_expression().map(Node::Expression),
    }
}

/// Reads the facts of one parsed file whose semantic model is built.
pub struct Reader<'s, 'a> {
    file: &'s SourceFile<'a>,
    /// The bindings of the modules that export the internal API base, which the file names,
    /// built on first use.
    internal_modules: OnceCell<Vec<ImportBindings<'s, 'a>>>,
    internal_base: &'s InternalBase<'s>,
    /// The controller and module decorators and the entity manager type.
    decorators: &'s Decorators,
}

impl<'s, 'a> Reader<'s, 'a> {
    /// The reader of `file`.
    pub fn new(
        file: &'s SourceFile<'a>,
        internal_base: &'s InternalBase<'s>,
        decorators: &'s Decorators,
    ) -> Self {
        Self {
            file,
            internal_modules: OnceCell::new(),
            internal_base,
            decorators,
        }
    }

    /// The facts of the file; the manager parameters only when `managers` holds.
    pub fn facts(&self, managers: bool) -> Facts {
        let mut facts = Facts::default();

        if managers {
            facts.managers = self.manager_parameters();
        }

        let classes: Vec<_> = self
            .file
            .semantic()
            .nodes()
            .iter()
            .filter_map(|node| match node.kind() {
                AstKind::Class(class) if !class.decorators.is_empty() => Some(class),
                _ => None,
            })
            .collect();

        if classes.is_empty() {
            return facts;
        }

        let module = &self.decorators.module;
        let nest = ImportBindings::new(self.file, &module.package, false);
        let controllers: Vec<(&Import, Option<ImportBindings<'_, 'a>>)> = self
            .decorators
            .controllers
            .iter()
            .map(|controller| {
                (
                    controller,
                    ImportBindings::new(self.file, &controller.package, false),
                )
            })
            .collect();
        let mut imported: Option<Vec<(String, Option<ImportBindings<'_, 'a>>)>> = None;
        let specifiers = self.imported_specifiers();

        for class in classes {
            for decorator in &class.decorators {
                let Some(call) = self
                    .resolve(Some(Node::Expression(&decorator.expression)))
                    .and_then(call)
                else {
                    continue;
                };
                let first = argument(call.arguments.first());
                let controller = controllers.iter().find(|(controller, bindings)| {
                    bindings
                        .as_ref()
                        .and_then(|bindings| bindings.resolve(&call.callee))
                        .is_some_and(|path| path == [controller.name.as_str()])
                });

                if let Some((controller, _)) = controller {
                    self.controller(first, controller.package == module.package, &mut facts);
                }

                if nest
                    .as_ref()
                    .and_then(|nest| nest.resolve(&call.callee))
                    .is_some_and(|path| path == [module.name.as_str()])
                {
                    let imported = imported.get_or_insert_with(|| {
                        specifiers
                            .iter()
                            .map(|specifier| {
                                (
                                    specifier.clone(),
                                    ImportBindings::new(self.file, specifier, false),
                                )
                            })
                            .collect()
                    });

                    self.module_exports(first, nest.as_ref(), imported, &mut facts);
                }
            }
        }

        facts
    }

    /// The routes of a controller decorator: the first argument of a decorator of the module
    /// decorator's package (`nest`) unless it is an object literal, otherwise its `path` property.
    fn controller(&self, first: Option<Node<'a>>, nest: bool, facts: &mut Facts) {
        let resolved = self.resolve(first);
        let route = if nest
            && resolved.is_some()
            && !matches!(
                resolved,
                Some(Node::Expression(Expression::ObjectExpression(_)))
            ) {
            first
        } else {
            self.found(first, "path")
        };
        let paths = self.array_values(route, &mut Vec::new());

        facts.route_shapes.extend(
            paths
                .iter()
                .map(|path| self.route_shape(Some(*path), &mut Vec::new())),
        );
        facts.controllers.push(Controller {
            literal_prefixes: paths
                .iter()
                .filter_map(|path| self.literal_prefix(Some(*path), &mut Vec::new()))
                .collect(),
            uses_internal_base: !paths.is_empty()
                && paths
                    .iter()
                    .all(|path| self.starts_with_internal_base(Some(*path), &mut Vec::new())),
        });
    }

    /// The modules a module decorator exports, directly or through the `forwardRef` of its
    /// package, as the `…Module` export of a module the file imports.
    fn module_exports(
        &self,
        first: Option<Node<'a>>,
        nest: Option<&ImportBindings<'_, 'a>>,
        imported: &[(String, Option<ImportBindings<'_, 'a>>)],
        facts: &mut Facts,
    ) {
        for exported in self.array_values(self.found(first, "exports"), &mut Vec::new()) {
            let Some(mut exported) = self.resolve(Some(exported)) else {
                continue;
            };

            if let Some(call) = call(exported)
                && nest.and_then(|nest| nest.resolve(&call.callee)).as_deref()
                    == Some(&["forwardRef".to_owned()])
                && let Some(Argument::ArrowFunctionExpression(callback)) = call.arguments.first()
            {
                let body = match &callback.body {
                    ArrowFunctionBody::FunctionBody(body) => other(&**body),
                    body => body
                        .as_expression()
                        .map_or_else(|| other(body), Node::Expression),
                };
                let Some(resolved) = self.resolve(Some(body)) else {
                    continue;
                };
                exported = resolved;
            }

            let Node::Expression(expression) = exported else {
                continue;
            };

            for (specifier, bindings) in imported {
                if let Some([name]) = bindings
                    .as_ref()
                    .and_then(|bindings| bindings.resolve(expression))
                    .as_deref()
                    && name.ends_with("Module")
                {
                    facts.forwarded.push((specifier.clone(), name.clone()));
                }
            }
        }
    }

    /// The module specifiers of the file's top-level imports and `import x = require(...)`, each
    /// once, in order.
    fn imported_specifiers(&self) -> Vec<String> {
        let mut specifiers: Vec<String> = Vec::new();

        for statement in &self.file.program().body {
            let specifier = match statement {
                Statement::ImportDeclaration(import) => Some(import.source.value.as_str()),
                Statement::TSImportEqualsDeclaration(import) => external(&import.module_reference),
                Statement::ExportDeclaration(export) => match &export.declaration {
                    Declaration::TSImportEqualsDeclaration(import) => {
                        external(&import.module_reference)
                    }
                    _ => None,
                },
                _ => None,
            };

            if let Some(specifier) = specifier
                && !specifiers.iter().any(|known| known == specifier)
            {
                specifiers.push(specifier.to_owned());
            }
        }

        specifiers
    }

    /// The value an expression stands for: through parentheses and type and non-null assertions,
    /// and through a name bound by exactly one `const` declaration to its initializer; `None` when
    /// the name cycles or a `const` has no initializer.
    fn resolve(&self, node: Option<Node<'a>>) -> Option<Node<'a>> {
        let mut value = node?;
        let mut seen: HashSet<SymbolId> = HashSet::new();
        let semantic = self.file.semantic();

        loop {
            let Node::Expression(expression) = value else {
                return Some(value);
            };

            value = match expression {
                Expression::TSAsExpression(inner) => Node::Expression(&inner.expression),
                Expression::TSSatisfiesExpression(inner) => Node::Expression(&inner.expression),
                Expression::TSNonNullExpression(inner) => Node::Expression(&inner.expression),
                Expression::ParenthesizedExpression(inner) => Node::Expression(&inner.expression),
                Expression::TSTypeAssertion(inner) => Node::Expression(&inner.expression),
                Expression::Identifier(identifier) => {
                    let Some(symbol) = identifier.reference_id.get().and_then(|reference| {
                        semantic.scoping().get_reference(reference).symbol_id()
                    }) else {
                        return Some(value);
                    };
                    let declarations: Vec<_> =
                        semantic.scoping().symbol_declarations(symbol).collect();
                    let [declaration] = declarations.as_slice() else {
                        return Some(value);
                    };
                    let nodes = semantic.nodes();
                    let AstKind::VariableDeclarator(declarator) = nodes.kind(*declaration) else {
                        return Some(value);
                    };
                    let constant = matches!(
                        nodes.parent_kind(*declaration),
                        AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                    );

                    if !constant
                        || !matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol))
                    {
                        return Some(value);
                    }

                    if !seen.insert(symbol) {
                        return None;
                    }

                    Node::Expression(declarator.init.as_ref()?)
                }
                _ => return Some(value),
            };
        }
    }

    /// The value of a found property `name` of a metadata object, or nothing.
    fn found(&self, node: Option<Node<'a>>, name: &str) -> Option<Node<'a>> {
        match self.property(node, name, &mut Vec::new()) {
            Property::Found(value) => Some(value),
            Property::Unknown | Property::Missing => None,
        }
    }

    /// The last property `name` of the object literal a value stands for, through spreads of known
    /// object literals and literal computed keys.
    fn property(&self, node: Option<Node<'a>>, name: &str, seen: &mut Vec<usize>) -> Property<'a> {
        let Some(object @ Node::Expression(Expression::ObjectExpression(literal))) =
            self.resolve(node)
        else {
            return Property::Unknown;
        };

        if seen.contains(&object.address()) {
            return Property::Unknown;
        }

        seen.push(object.address());
        let property = self.object_property(literal.properties.iter().rev(), name, seen);
        seen.pop();
        property
    }

    fn object_property(
        &self,
        properties: impl Iterator<Item = &'a ObjectPropertyKind<'a>>,
        name: &str,
        seen: &mut Vec<usize>,
    ) -> Property<'a> {
        for property in properties {
            match property {
                ObjectPropertyKind::SpreadProperty(spread) => {
                    let spread = self.resolve(Some(Node::Expression(&spread.argument)));

                    // An unknown later spread can replace an earlier property.
                    if !matches!(
                        spread,
                        Some(Node::Expression(Expression::ObjectExpression(_)))
                    ) {
                        return Property::Unknown;
                    }

                    match self.property(spread, name, seen) {
                        Property::Missing => {}
                        value => return value,
                    }
                }
                ObjectPropertyKind::ObjectProperty(property) => {
                    let key = if property.computed {
                        let key = property
                            .key
                            .as_expression()
                            .and_then(|key| self.resolve(Some(Node::Expression(key))));

                        match key.and_then(string_like) {
                            Some(key) => Some(key),
                            None => return Property::Unknown,
                        }
                    } else {
                        match &property.key {
                            PropertyKey::StaticIdentifier(key) => Some(key.name.to_string()),
                            PropertyKey::StringLiteral(key) => Some(key.value.to_string()),
                            key => key
                                .as_expression()
                                .and_then(|key| string_like(Node::Expression(key))),
                        }
                    };

                    if key.as_deref() == Some(name) {
                        return if property.kind == PropertyKind::Init && !property.method {
                            Property::Found(Node::Expression(&property.value))
                        } else {
                            Property::Unknown
                        };
                    }
                }
            }
        }

        Property::Missing
    }

    /// The elements of the array literal a value stands for, spreads of known arrays flattened; any
    /// other value is its own one element.
    fn array_values(&self, node: Option<Node<'a>>, seen: &mut Vec<usize>) -> Vec<Node<'a>> {
        let Some(value) = self.resolve(node) else {
            return Vec::new();
        };

        if seen.contains(&value.address()) {
            return Vec::new();
        }

        let Node::Expression(Expression::ArrayExpression(array)) = value else {
            return vec![value];
        };

        seen.push(value.address());
        let mut values = Vec::new();

        for element in &array.elements {
            match element {
                ArrayExpressionElement::SpreadElement(spread) => {
                    values
                        .extend(self.array_values(Some(Node::Expression(&spread.argument)), seen));
                }
                ArrayExpressionElement::Elision(hole) => values.push(other(hole)),
                element => values.extend(element.as_expression().map(Node::Expression)),
            }
        }

        seen.pop();
        values
    }

    /// The statically known start of a route: a string, a template's head, or the start of the
    /// left side of `+`.
    fn literal_prefix(&self, node: Option<Node<'a>>, seen: &mut Vec<usize>) -> Option<String> {
        let value = self.resolve(node)?;

        if seen.contains(&value.address()) {
            return None;
        }

        if let Some(text) = string_like(value) {
            return Some(text);
        }

        seen.push(value.address());
        let prefix = match value {
            Node::Expression(Expression::TemplateLiteral(template)) => template
                .quasis
                .first()
                .map(|quasi| cooked(quasi.value.cooked.as_ref())),
            Node::Expression(Expression::BinaryExpression(binary))
                if binary.operator == BinaryOperator::Addition =>
            {
                self.literal_prefix(Some(Node::Expression(&binary.left)), seen)
            }
            _ => None,
        };
        seen.pop();
        prefix
    }

    /// Whether a route starts with the internal API base: the base itself, a template whose empty
    /// head is followed by it, or the left side of `+` that starts with it.
    fn starts_with_internal_base(&self, node: Option<Node<'a>>, seen: &mut Vec<usize>) -> bool {
        let Some(value) = self.resolve(node) else {
            return false;
        };

        if seen.contains(&value.address()) {
            return false;
        }

        let Node::Expression(expression) = value else {
            return false;
        };

        let internal_modules = self.internal_modules.get_or_init(|| {
            self.internal_base
                .modules
                .iter()
                .filter_map(|module| ImportBindings::new(self.file, module, false))
                .collect()
        });

        if internal_modules.iter().any(|bindings| {
            bindings
                .resolve(expression)
                .is_some_and(|binding| binding == self.internal_base.export)
        }) {
            return true;
        }

        seen.push(value.address());
        let starts = match expression {
            Expression::TemplateLiteral(template)
                if !template.expressions.is_empty()
                    && template
                        .quasis
                        .first()
                        .is_some_and(|quasi| cooked(quasi.value.cooked.as_ref()).is_empty()) =>
            {
                self.starts_with_internal_base(
                    Some(Node::Expression(&template.expressions[0])),
                    seen,
                )
            }
            Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
                self.starts_with_internal_base(Some(Node::Expression(&binary.left)), seen)
            }
            _ => false,
        };
        seen.pop();
        starts
    }

    /// A route as text: strings, templates and `+` joined, with `\u{0}` for any part that is not
    /// statically known.
    fn route_shape(&self, node: Option<Node<'a>>, seen: &mut Vec<usize>) -> String {
        let Some(value) = self.resolve(node) else {
            return "\u{0}".to_owned();
        };

        if seen.contains(&value.address()) {
            return "\u{0}".to_owned();
        }

        if let Some(text) = string_like(value) {
            return text;
        }

        seen.push(value.address());
        let shape = match value {
            Node::Expression(Expression::TemplateLiteral(template)) => {
                let mut shape = template
                    .quasis
                    .first()
                    .map(|quasi| cooked(quasi.value.cooked.as_ref()))
                    .unwrap_or_default();

                for (expression, quasi) in template
                    .expressions
                    .iter()
                    .zip(template.quasis.iter().skip(1))
                {
                    shape.push_str(&self.route_shape(Some(Node::Expression(expression)), seen));
                    shape.push_str(&cooked(quasi.value.cooked.as_ref()));
                }

                shape
            }
            Node::Expression(Expression::BinaryExpression(binary))
                if binary.operator == BinaryOperator::Addition =>
            {
                let left = self.route_shape(Some(Node::Expression(&binary.left)), seen);
                left + &self.route_shape(Some(Node::Expression(&binary.right)), seen)
            }
            _ => "\u{0}".to_owned(),
        };
        seen.pop();
        shape
    }

    /// `method(parameter)` for each parameter of a public method, class or object literal, whose
    /// type holds the native entity manager, directly or through local type aliases and
    /// interfaces.
    fn manager_parameters(&self) -> Vec<String> {
        let manager = &self.decorators.entity_manager;
        let typeorm = ImportBindings::new(self.file, &manager.package, true);
        let mut parameters = Vec::new();

        for node in self.file.semantic().nodes().iter() {
            let (key, function, computed) = match node.kind() {
                AstKind::MethodDefinition(method)
                    if method.kind == MethodDefinitionKind::Method
                        && !matches!(method.key, PropertyKey::PrivateIdentifier(_))
                        && !matches!(
                            method.accessibility,
                            Some(TSAccessibility::Private | TSAccessibility::Protected)
                        ) =>
                {
                    (&method.key, &method.value.params, method.computed)
                }
                AstKind::ObjectProperty(property) if property.method => {
                    let Expression::FunctionExpression(function) = &property.value else {
                        continue;
                    };
                    (&property.key, &function.params, property.computed)
                }
                _ => continue,
            };

            for (name, annotation) in self.annotated_parameters(function) {
                let mut visitor = ManagerType {
                    file: self.file,
                    typeorm: typeorm.as_ref(),
                    name: &manager.name,
                    seen: HashSet::new(),
                    found: false,
                };
                visitor.visit_ts_type(annotation);

                if visitor.found {
                    parameters.push(format!("{}({name})", self.key_text(key, computed)));
                }
            }
        }

        parameters
    }

    /// The name text and type of each typed parameter, the rest parameter included.
    fn annotated_parameters(
        &self,
        parameters: &'a FormalParameters<'a>,
    ) -> Vec<(String, &'a TSType<'a>)> {
        let source = self.file.source();
        let mut annotated: Vec<(String, &'a TSType<'a>)> = parameters
            .items
            .iter()
            .filter_map(|parameter| {
                let annotation = parameter.type_annotation.as_ref()?;
                let span = parameter.pattern.span();
                Some((
                    source[span.start as usize..span.end as usize].to_owned(),
                    &annotation.type_annotation,
                ))
            })
            .collect();

        if let Some(rest) = &parameters.rest
            && let Some(annotation) = &rest.type_annotation
        {
            let span = rest.rest.argument.span();
            annotated.push((
                source[span.start as usize..span.end as usize].to_owned(),
                &annotation.type_annotation,
            ));
        }

        annotated
    }

    /// The source text of a method name, a computed name with its brackets.
    fn key_text(&self, key: &PropertyKey<'_>, computed: bool) -> String {
        let span = key.span();
        let text = &self.file.source()[span.start as usize..span.end as usize];

        if computed {
            format!("[{text}]")
        } else {
            text.to_owned()
        }
    }
}

/// The call a value is, an optional call `f?.()` included.
fn call(node: Node<'_>) -> Option<&CallExpression<'_>> {
    match node {
        Node::Expression(Expression::CallExpression(call)) => Some(call),
        Node::Expression(Expression::ChainExpression(chain)) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// The module `import x = require("m")` names.
fn external<'a>(reference: &'a TSModuleReference<'a>) -> Option<&'a str> {
    match reference {
        TSModuleReference::ExternalModuleReference(reference) => {
            Some(reference.expression.value.as_str())
        }
        _ => None,
    }
}

/// The text of a string literal or a template without substitutions.
fn string_like(node: Node<'_>) -> Option<String> {
    match node {
        Node::Expression(Expression::StringLiteral(literal)) => Some(literal.value.to_string()),
        Node::Expression(Expression::TemplateLiteral(template))
            if template.expressions.is_empty() =>
        {
            Some(
                template
                    .quasis
                    .first()
                    .map(|quasi| cooked(quasi.value.cooked.as_ref()))
                    .unwrap_or_default(),
            )
        }
        _ => None,
    }
}

fn cooked(text: Option<&impl ToString>) -> String {
    text.map(ToString::to_string).unwrap_or_default()
}

/// Whether a type holds the native entity manager: a type reference that names it through the
/// file's bindings of its package, or a local type alias or interface whose declaration holds it.
struct ManagerType<'s, 'a> {
    file: &'s SourceFile<'a>,
    typeorm: Option<&'s ImportBindings<'s, 'a>>,
    name: &'s str,
    seen: HashSet<SymbolId>,
    found: bool,
}

impl<'a> Visit<'a> for ManagerType<'_, 'a> {
    fn visit_ts_type_reference(&mut self, reference: &TSTypeReference<'a>) {
        if self.found {
            return;
        }

        if self
            .typeorm
            .and_then(|typeorm| typeorm.resolve_type_name(&reference.type_name))
            .is_some_and(|path| path == [self.name])
        {
            self.found = true;
            return;
        }

        if let TSTypeName::IdentifierReference(identifier) = &reference.type_name {
            let semantic = self.file.semantic();
            let symbol = identifier
                .reference_id
                .get()
                .and_then(|reference| semantic.scoping().get_reference(reference).symbol_id());

            if let Some(symbol) = symbol
                && self.seen.insert(symbol)
            {
                for declaration in semantic.scoping().symbol_declarations(symbol) {
                    match semantic.nodes().kind(declaration) {
                        AstKind::TSTypeAliasDeclaration(alias) => {
                            self.visit_ts_type_alias_declaration(alias);
                        }
                        AstKind::TSInterfaceDeclaration(interface) => {
                            self.visit_ts_interface_declaration(interface);
                        }
                        _ => {}
                    }
                }
            }
        }

        walk::walk_ts_type_reference(self, reference);
    }
}
