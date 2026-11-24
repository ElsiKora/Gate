//! The environment reads of `raw-config-access`: calls of a getter of a raw configuration service
//! and reads of `process.env`, each with the key it names when the file names it statically.

use gate_sdk::Span;
use gate_typescript::Result;
use gate_typescript::env::EnvRead;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, ClassElement, Expression, ForStatementLeft, FormalParameter,
    MethodDefinitionKind, StaticMemberExpression, TSType, TSTypeAnnotation, TSTypeName,
    VariableDeclarationKind,
};
use gate_typescript::oxc_semantic::Reference;
use gate_typescript::oxc_span::{GetSpan as _, Span as Range};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::oxc_syntax::operator::BinaryOperator;
use gate_typescript::oxc_syntax::symbol::SymbolId;

use super::Graph;

/// How deep a name is followed through `const` bindings before the rule gives up on it.
const DEPTH: usize = 4;

/// The array methods whose callback's first parameter is the element: a key when the array is a
/// literal array of strings. `reduce` and `reduceRight` pass the accumulator first and are not here.
const ELEMENT_METHODS: [&str; 10] = [
    "map",
    "flatMap",
    "forEach",
    "filter",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "some",
    "every",
];

/// One `process.env` occurrence outside the configuration boundary: the read expression's span
/// and text.
pub(super) struct ProcessEnv {
    pub(super) span: Span,
    pub(super) text: String,
}

/// What one file reads: its environment reads and its `process.env` occurrences.
#[derive(Default)]
pub(super) struct FileReads {
    pub(super) reads: Vec<EnvRead>,
    pub(super) process_env: Vec<ProcessEnv>,
}

impl<'a> Graph<'_, 'a> {
    /// The environment reads of `file`: every call of one of `getters` on a raw configuration
    /// service, and every read of `process.env`.
    pub(super) fn env_reads(&self, file: usize, getters: &[String]) -> Result<FileReads> {
        let source = self.file(file);
        let semantic = source.semantic();
        let nodes = semantic.nodes();
        let path = self.sources[file].0;
        let mut found = FileReads::default();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::CallExpression(call) => {
                    let Expression::StaticMemberExpression(callee) =
                        call.callee.without_parentheses()
                    else {
                        continue;
                    };
                    if !getters
                        .iter()
                        .any(|getter| callee.property.name == getter.as_str())
                    {
                        continue;
                    }
                    let Some(argument) =
                        call.arguments.first().and_then(|item| item.as_expression())
                    else {
                        continue;
                    };
                    if never_key(argument) {
                        continue;
                    }
                    if !self.raw_receiver(file, node.id(), &callee.object, DEPTH)? {
                        continue;
                    }
                    self.push_reads(&mut found.reads, file, path, argument);
                }
                AstKind::StaticMemberExpression(member) if self.is_process_env(file, member) => {
                    let read = self.process_env_read(file, node.id(), member, &mut found.reads);
                    found.process_env.push(ProcessEnv {
                        span: source.span(read),
                        text: source.text(read).to_owned(),
                    });
                }
                _ => {}
            }
        }

        Ok(found)
    }

    /// The reads of one `process.env`, at `node`: a member, an element, a destructuring or an `in`
    /// test, each key it names; and the range of the whole read expression.
    fn process_env_read(
        &self,
        file: usize,
        node: NodeId,
        member: &StaticMemberExpression<'a>,
        reads: &mut Vec<EnvRead>,
    ) -> Range {
        let source = self.file(file);
        let path = self.sources[file].0;
        match source.semantic().nodes().parent_kind(node) {
            AstKind::StaticMemberExpression(outer) if outer.object.span() == member.span => {
                reads.push(EnvRead {
                    key: Some(outer.property.name.to_string()),
                    path: path.to_owned(),
                    span: source.span(outer.span),
                    expression: source.text(outer.span).to_owned(),
                });
                outer.span
            }
            AstKind::ComputedMemberExpression(outer) if outer.object.span() == member.span => {
                self.push_reads(reads, file, path, &outer.expression);
                outer.span
            }
            AstKind::VariableDeclarator(declarator) => {
                if let BindingPattern::ObjectPattern(pattern) = &declarator.id {
                    for property in &pattern.properties {
                        let key = property
                            .key
                            .static_name()
                            .filter(|_| !property.computed)
                            .map(|name| name.to_string());
                        reads.push(EnvRead {
                            key,
                            path: path.to_owned(),
                            span: source.span(property.span),
                            expression: source.text(property.key.span()).to_owned(),
                        });
                    }
                    if let Some(rest) = &pattern.rest {
                        reads.push(EnvRead {
                            key: None,
                            path: path.to_owned(),
                            span: source.span(rest.span),
                            expression: source.text(rest.span).to_owned(),
                        });
                    }
                }
                declarator.span
            }
            AstKind::BinaryExpression(binary)
                if binary.operator == BinaryOperator::In && binary.right.span() == member.span =>
            {
                self.push_reads(reads, file, path, &binary.left);
                binary.span
            }
            _ => member.span,
        }
    }

    /// The reads `argument` names: one per key it names statically, or one without a key.
    fn push_reads(
        &self,
        reads: &mut Vec<EnvRead>,
        file: usize,
        path: &str,
        argument: &Expression<'a>,
    ) {
        let source = self.file(file);
        let span = source.span(argument.span());
        let expression = source.text(argument.span()).to_owned();
        match self.keys(file, argument, DEPTH) {
            Some(keys) => reads.extend(keys.into_iter().map(|key| EnvRead {
                key: Some(key),
                path: path.to_owned(),
                span,
                expression: expression.clone(),
            })),
            None => reads.push(EnvRead {
                key: None,
                path: path.to_owned(),
                span,
                expression,
            }),
        }
    }

    /// Whether `member` is `process.env`, with `process` the global.
    fn is_process_env(&self, file: usize, member: &StaticMemberExpression<'a>) -> bool {
        member.property.name == "env"
            && matches!(
                &member.object,
                Expression::Identifier(process)
                    if process.name == "process"
                        && self.reference(file, process.reference_id.get()).is_none()
            )
    }

    /// The keys an expression names statically: a string, a template without substitutions, a
    /// `const` bound to one, the variable of a `for ... of` loop over a literal array of strings,
    /// or the element parameter of a callback over one; `None` for anything else.
    fn keys(&self, file: usize, expression: &Expression<'a>, depth: usize) -> Option<Vec<String>> {
        match expression.without_parentheses() {
            Expression::StringLiteral(literal) => Some(vec![literal.value.to_string()]),
            Expression::TemplateLiteral(template) => {
                template.single_quasi().map(|text| vec![text.to_string()])
            }
            Expression::TSAsExpression(inner) => self.keys(file, &inner.expression, depth),
            Expression::TSSatisfiesExpression(inner) => self.keys(file, &inner.expression, depth),
            Expression::TSNonNullExpression(inner) => self.keys(file, &inner.expression, depth),
            Expression::Identifier(identifier) if depth > 0 => {
                let symbol = self.reference(file, identifier.reference_id.get())?;
                self.bound_keys(file, symbol, depth - 1)
            }
            _ => None,
        }
    }

    /// The keys a name binds: a `const` initialized with keys, a loop variable over an array of
    /// them, or the element parameter of a callback over one.
    fn bound_keys(&self, file: usize, symbol: SymbolId, depth: usize) -> Option<Vec<String>> {
        let semantic = self.file(file).semantic();
        let nodes = semantic.nodes();
        let declaration = semantic.scoping().symbol_declaration(symbol);
        let declarator = match nodes.kind(declaration) {
            AstKind::VariableDeclarator(declarator) => declarator,
            AstKind::FormalParameter(parameter) => {
                return self.callback_keys(file, declaration, parameter, symbol, depth);
            }
            _ => return None,
        };
        if !matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol))
        {
            return None;
        }
        let list = nodes.parent_id(declaration);
        let AstKind::VariableDeclaration(variables) = nodes.kind(list) else {
            return None;
        };
        match nodes.parent_kind(list) {
            AstKind::ForOfStatement(statement) if matches!(&statement.left, ForStatementLeft::VariableDeclaration(left) if left.span == variables.span) =>
            {
                if self.written(file, symbol) {
                    return None;
                }
                self.array_keys(file, &statement.right, depth)
            }
            _ if variables.kind == VariableDeclarationKind::Const => {
                let keys = self.keys(file, declarator.init.as_ref()?, depth)?;
                (keys.len() == 1).then_some(keys)
            }
            _ => None,
        }
    }

    /// The keys a callback parameter binds: the strings of the array an element method is called
    /// on, when the parameter is a plain first parameter, never written, of an arrow function or
    /// function expression passed directly as that method's first argument.
    fn callback_keys(
        &self,
        file: usize,
        declaration: NodeId,
        parameter: &FormalParameter<'a>,
        symbol: SymbolId,
        depth: usize,
    ) -> Option<Vec<String>> {
        let plain = parameter.initializer.is_none()
            && matches!(&parameter.pattern, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol));
        if !plain || self.written(file, symbol) {
            return None;
        }
        let nodes = self.file(file).semantic().nodes();
        let list = nodes.parent_id(declaration);
        let AstKind::FormalParameters(parameters) = nodes.kind(list) else {
            return None;
        };
        if parameters.items.first().map(|first| first.span) != Some(parameter.span) {
            return None;
        }
        let function = nodes.parent_id(list);
        let callback = match nodes.kind(function) {
            AstKind::ArrowFunctionExpression(arrow) => arrow.span,
            AstKind::Function(function) if function.is_expression() => function.span,
            _ => return None,
        };
        let call = nodes
            .ancestor_ids(function)
            .find(|id| !matches!(nodes.kind(*id), AstKind::ParenthesizedExpression(_)))?;
        let AstKind::CallExpression(call) = nodes.kind(call) else {
            return None;
        };
        let first = call.arguments.first()?.as_expression()?;
        if first.without_parentheses().span() != callback {
            return None;
        }
        let Expression::StaticMemberExpression(callee) = call.callee.without_parentheses() else {
            return None;
        };
        if !ELEMENT_METHODS.contains(&callee.property.name.as_str()) {
            return None;
        }
        self.array_keys(file, &callee.object, depth)
    }

    /// Whether anything writes `symbol`: an assignment, an update, or a redeclaration such as a
    /// `var` of the same name in the function.
    fn written(&self, file: usize, symbol: SymbolId) -> bool {
        let scoping = self.file(file).semantic().scoping();
        !scoping.symbol_redeclarations(symbol).is_empty()
            || scoping
                .get_resolved_references(symbol)
                .any(Reference::is_write)
    }

    /// The strings of a non-empty literal array, or of a `const` bound to one.
    fn array_keys(
        &self,
        file: usize,
        expression: &Expression<'a>,
        depth: usize,
    ) -> Option<Vec<String>> {
        match expression.without_parentheses() {
            Expression::ArrayExpression(array) => array
                .elements
                .iter()
                .map(|element| {
                    let keys = self.keys(file, element.as_expression()?, depth)?;
                    match keys.as_slice() {
                        [key] => Some(key.clone()),
                        _ => None,
                    }
                })
                .collect::<Option<Vec<_>>>()
                .filter(|keys| !keys.is_empty()),
            Expression::TSAsExpression(inner) => self.array_keys(file, &inner.expression, depth),
            Expression::TSNonNullExpression(inner) => {
                self.array_keys(file, &inner.expression, depth)
            }
            Expression::TSSatisfiesExpression(inner) => {
                self.array_keys(file, &inner.expression, depth)
            }
            Expression::Identifier(identifier) if depth > 0 => {
                let symbol = self.reference(file, identifier.reference_id.get())?;
                let semantic = self.file(file).semantic();
                let declaration = semantic.scoping().symbol_declaration(symbol);
                let AstKind::VariableDeclarator(declarator) = semantic.nodes().kind(declaration)
                else {
                    return None;
                };
                let constant = matches!(
                    semantic.nodes().parent_kind(declaration),
                    AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                );
                if !constant {
                    return None;
                }
                self.array_keys(file, declarator.init.as_ref()?, depth - 1)
            }
            _ => None,
        }
    }

    /// Whether `receiver`, at `node`, is a raw configuration service: a name typed as one or bound
    /// to one, or a property of the enclosing class typed as one.
    fn raw_receiver(
        &self,
        file: usize,
        node: NodeId,
        receiver: &Expression<'a>,
        depth: usize,
    ) -> Result<bool> {
        match receiver.without_parentheses() {
            Expression::TSNonNullExpression(inner) => {
                self.raw_receiver(file, node, &inner.expression, depth)
            }
            Expression::Identifier(identifier) => {
                let Some(symbol) = self.reference(file, identifier.reference_id.get()) else {
                    return Ok(false);
                };
                if self.raw_bound(file, symbol, &[], &mut Vec::new())? {
                    return Ok(true);
                }
                self.typed_raw(file, symbol, depth)
            }
            Expression::StaticMemberExpression(member)
                if matches!(member.object, Expression::ThisExpression(_)) =>
            {
                self.class_property_raw(file, node, &member.property.name)
            }
            _ => Ok(false),
        }
    }

    /// Whether a name is declared with a raw configuration type, or is a `const` bound to a raw
    /// receiver.
    fn typed_raw(&self, file: usize, symbol: SymbolId, depth: usize) -> Result<bool> {
        let semantic = self.file(file).semantic();
        let declaration = semantic.scoping().symbol_declaration(symbol);
        match semantic.nodes().kind(declaration) {
            AstKind::FormalParameter(parameter) => {
                self.raw_type(file, parameter.type_annotation.as_deref())
            }
            AstKind::VariableDeclarator(declarator) => {
                if self.raw_type(file, declarator.type_annotation.as_deref())? {
                    return Ok(true);
                }
                match &declarator.init {
                    Some(init) if depth > 0 => {
                        self.raw_receiver(file, declaration, init, depth - 1)
                    }
                    _ => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    /// Whether the class around `node` declares the property `name`, or a constructor parameter
    /// property of that name, with a raw configuration type.
    fn class_property_raw(&self, file: usize, node: NodeId, name: &str) -> Result<bool> {
        let nodes = self.file(file).semantic().nodes();
        let Some(class) = nodes
            .ancestor_ids(node)
            .find_map(|id| match nodes.kind(id) {
                AstKind::Class(class) => Some(class),
                _ => None,
            })
        else {
            return Ok(false);
        };

        for element in &class.body.body {
            match element {
                ClassElement::PropertyDefinition(property)
                    if property.key.static_name().as_deref() == Some(name) =>
                {
                    return self.raw_type(file, property.type_annotation.as_deref());
                }
                ClassElement::MethodDefinition(method)
                    if method.kind == MethodDefinitionKind::Constructor =>
                {
                    for parameter in &method.value.params.items {
                        if parameter.has_modifier()
                            && parameter
                                .pattern
                                .get_binding_identifier()
                                .is_some_and(|binding| binding.name == name)
                        {
                            return self.raw_type(file, parameter.type_annotation.as_deref());
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(false)
    }

    /// Whether a type annotation names a raw configuration service.
    fn raw_type(&self, file: usize, annotation: Option<&'a TSTypeAnnotation<'a>>) -> Result<bool> {
        let Some(TSType::TSTypeReference(reference)) =
            annotation.map(|annotation| &annotation.type_annotation)
        else {
            return Ok(false);
        };
        match &reference.type_name {
            TSTypeName::IdentifierReference(identifier) => {
                match self.reference(file, identifier.reference_id.get()) {
                    Some(symbol) => self.raw_bound(file, symbol, &[], &mut Vec::new()),
                    None => Ok(false),
                }
            }
            TSTypeName::QualifiedName(name) => {
                match self.qualified_entity(file, &name.left, &name.right.name, false)? {
                    Some(entity) if !self.destructured(entity) => {
                        self.raw_symbol(entity, &[], &mut Vec::new())
                    }
                    _ => {
                        self.raw_readers(file, super::Probe::Qualified(name), &[], &mut Vec::new())
                    }
                }
            }
            TSTypeName::ThisExpression(_) => Ok(false),
        }
    }
}

/// Whether a getter's first argument can never be a key: under parentheses, `as`, `satisfies` and
/// `!`, an object, array, function or class expression, or a literal other than a string.
fn never_key(argument: &Expression<'_>) -> bool {
    match argument.without_parentheses() {
        Expression::TSAsExpression(inner) => never_key(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => never_key(&inner.expression),
        Expression::TSNonNullExpression(inner) => never_key(&inner.expression),
        Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_)
        | Expression::ClassExpression(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::RegExpLiteral(_) => true,
        _ => false,
    }
}
