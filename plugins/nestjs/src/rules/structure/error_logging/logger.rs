//! Native `Logger.error` calls and their arguments: which calls reach the native sink, which
//! argument is formatted error evidence and which is a bounded message.

use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentTargetPropertyIdentifier, BindingIdentifier,
    CallExpression, Expression, FormalParameter, IdentifierReference, ObjectProperty,
    StaticMemberExpression, TSAsExpression, TSSatisfiesExpression, TSThisParameter, TSType,
    TSTypeAssertion, TSTypeName,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::value::string_like;

use super::source::{Declaration, Key, Seen, Source};
use super::{Form, form, unwrap};

/// The module and export a value was imported as; `*` for a namespace.
pub struct Origin<'a> {
    pub module: &'a str,
    pub name: String,
}

/// Whether the native logger is called on the class or on an instance.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Static,
    Instance,
}

/// A native `error` method with the arguments bound to it.
struct Method<'a> {
    kind: Kind,
    bound: Vec<Option<&'a Expression<'a>>>,
}

/// A call of the native `error` method: its kind and every argument it receives, or `None` when an
/// `apply` passes them other than as one array literal. A spread or hole is `None`.
pub struct Invocation<'a> {
    pub kind: Kind,
    pub arguments: Option<Vec<Option<&'a Expression<'a>>>>,
}

fn argument<'a>(argument: &'a Argument<'a>) -> Option<&'a Expression<'a>> {
    argument.as_expression()
}

/// The member name of a property or literal element access.
fn access_name(expression: &Expression<'_>) -> Option<String> {
    match form(expression) {
        Form::Static(member) => Some(member.property.name.to_string()),
        Form::Computed(member) => string_like(&member.expression),
        _ => None,
    }
}

/// The object a property or element access reads from.
fn accessed<'a>(expression: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    match form(expression) {
        Form::Static(member) => Some(&member.object),
        Form::Computed(member) => Some(&member.object),
        _ => None,
    }
}

impl<'a> Source<'_, 'a> {
    /// Where a value was imported from: an import, a namespace member read by name or literal,
    /// a top-level destructuring of a namespace, or a `const` alias of one.
    pub fn origin(&self, expression: &'a Expression<'a>, seen: &mut Seen) -> Option<Origin<'a>> {
        match form(unwrap(expression)?) {
            Form::Computed(member) => {
                let name = string_like(&member.expression)?;
                let parent = self.origin(&member.object, seen)?;
                (parent.name == "*").then_some(Origin {
                    module: parent.module,
                    name,
                })
            }
            Form::Static(member) => {
                let parent = self.origin(&member.object, seen)?;
                (parent.name == "*").then(|| Origin {
                    module: parent.module,
                    name: member.property.name.to_string(),
                })
            }
            Form::Other(Expression::Identifier(identifier)) => {
                self.identifier_origin(identifier, seen)
            }
            _ => None,
        }
    }

    fn identifier_origin(
        &self,
        identifier: &IdentifierReference<'a>,
        seen: &mut Seen,
    ) -> Option<Origin<'a>> {
        let (key, declaration) = self.named(identifier)?;

        if !seen.insert(key) {
            return None;
        }

        match declaration {
            Declaration::Import { module, name } => Some(Origin { module, name }),
            Declaration::Namespace { module } => Some(Origin {
                module,
                name: "*".to_owned(),
            }),
            Declaration::Element(element) => {
                let [link] = element.links.as_slice() else {
                    return None;
                };
                let super::source::Owner::Variable {
                    declarator,
                    constant: true,
                } = element.owner
                else {
                    return None;
                };

                if !link.object || link.rest || link.default.is_some() {
                    return None;
                }

                let parent = self.origin(declarator.init.as_ref()?, seen)?;
                (parent.name == "*").then_some(Origin {
                    module: parent.module,
                    name: link.key.clone()?,
                })
            }
            declaration => self.origin(declaration.constant_initializer()?, seen),
        }
    }

    /// Where a type name was imported from.
    pub(super) fn type_origin(
        &self,
        name: &'a TSTypeName<'a>,
        seen: &mut Seen,
    ) -> Option<Origin<'a>> {
        match name {
            TSTypeName::IdentifierReference(identifier) => self.identifier_origin(identifier, seen),
            TSTypeName::QualifiedName(qualified) => {
                let parent = self.type_origin(&qualified.left, seen)?;
                (parent.name == "*").then(|| Origin {
                    module: parent.module,
                    name: qualified.right.name.to_string(),
                })
            }
            TSTypeName::ThisExpression(_) => None,
        }
    }

    pub fn imported(&self, expression: &'a Expression<'a>, module: &str, name: &str) -> bool {
        self.origin(expression, &mut Seen::new())
            .is_some_and(|origin| origin.module == module && origin.name == name)
    }

    fn type_imported(&self, type_name: &'a TSTypeName<'a>, module: &str, name: &str) -> bool {
        self.type_origin(type_name, &mut Seen::new())
            .is_some_and(|origin| origin.module == module && origin.name == name)
    }

    /// Whether a value is the native constructor `module.name` or a class extending it.
    pub(super) fn native_constructor(
        &self,
        expression: &'a Expression<'a>,
        module: &str,
        name: &str,
        seen: &mut Seen,
    ) -> bool {
        let Some(value) = unwrap(expression) else {
            return false;
        };

        if self.imported(value, module, name) {
            return true;
        }

        match value {
            Expression::ClassExpression(class) => class.heritage.as_ref().is_some_and(|heritage| {
                self.native_constructor(&heritage.expression, module, name, seen)
            }),
            Expression::Identifier(identifier) => {
                let Some((key, declaration)) = self.named(identifier) else {
                    return false;
                };

                if !seen.insert(key) {
                    return false;
                }

                match declaration {
                    Declaration::Class(class) => class.heritage.as_ref().is_some_and(|heritage| {
                        self.native_constructor(&heritage.expression, module, name, seen)
                    }),
                    declaration => declaration
                        .constant_initializer()
                        .is_some_and(|initializer| {
                            self.native_constructor(initializer, module, name, seen)
                        }),
                }
            }
            _ => false,
        }
    }

    /// Whether a type name is the native constructor `module.name` or a class extending it.
    fn native_type(&self, type_name: &'a TSTypeName<'a>, module: &str, name: &str) -> bool {
        if self.type_imported(type_name, module, name) {
            return true;
        }

        match self.type_named(type_name) {
            Some((key, Declaration::Class(class))) => {
                let mut seen = Seen::from([key]);
                class.heritage.as_ref().is_some_and(|heritage| {
                    self.native_constructor(&heritage.expression, module, name, &mut seen)
                })
            }
            _ => false,
        }
    }

    /// Whether a type is the native `Logger`, a class extending it, or an alias of one.
    fn logger_type(&self, annotation: &'a TSType<'a>, seen: &mut Seen) -> bool {
        let TSType::TSTypeReference(reference) = annotation else {
            return false;
        };

        if self.native_type(
            &reference.type_name,
            &self.names.nest_package,
            &self.names.logger_class,
        ) {
            return true;
        }

        let Some((key, Declaration::TypeAlias(alias))) = self.type_named(&reference.type_name)
        else {
            return false;
        };

        seen.insert(key) && self.logger_type(&alias.type_annotation, seen)
    }

    /// The declaration a value or member reads, for a name, `this.name` or a member of a class or
    /// class-typed value of the file.
    fn reference(&self, value: &'a Expression<'a>) -> Option<(Key, Declaration<'a>)> {
        match form(value) {
            Form::Other(Expression::Identifier(identifier)) => self.named(identifier),
            Form::Static(member) => self.member(&member.object, member.property.name.as_str()),
            _ => None,
        }
    }

    /// Whether a value is the native `Logger` class or an instance of it.
    fn logger_kind(&self, expression: &'a Expression<'a>, seen: &mut Seen) -> Option<Kind> {
        let value = unwrap(expression)?;

        if self.native_constructor(
            value,
            &self.names.nest_package,
            &self.names.logger_class,
            &mut Seen::new(),
        ) {
            return Some(Kind::Static);
        }

        if let Expression::NewExpression(construction) = value
            && self.native_constructor(
                &construction.callee,
                &self.names.nest_package,
                &self.names.logger_class,
                &mut Seen::new(),
            )
        {
            return Some(Kind::Instance);
        }

        let (key, declaration) = self.reference(value)?;

        if !seen.insert(key) {
            return None;
        }

        let (annotation, initializer) = match &declaration {
            Declaration::Variable {
                declarator,
                constant,
            } => (
                declarator.type_annotation.as_deref(),
                declarator.init.as_ref().filter(|_| *constant),
            ),
            Declaration::Property(property) => {
                (property.type_annotation.as_deref(), property.value.as_ref())
            }
            Declaration::Parameter(parameter) => (parameter.type_annotation.as_deref(), None),
            _ => (None, None),
        };

        if annotation.is_some_and(|annotation| {
            self.logger_type(&annotation.type_annotation, &mut Seen::new())
        }) {
            return Some(Kind::Instance);
        }

        self.logger_kind(initializer?, seen)
    }

    /// The native `error` method a value is, with the arguments `bind` gave it.
    fn logger_method(&self, expression: &'a Expression<'a>, seen: &mut Seen) -> Option<Method<'a>> {
        let value = unwrap(expression)?;

        match form(value) {
            Form::Static(member) if member.property.name.as_str() == self.names.logger_method => {
                let kind = self.logger_kind(&member.object, &mut Seen::new())?;
                return Some(Method {
                    kind,
                    bound: Vec::new(),
                });
            }
            Form::Computed(member)
                if string_like(&member.expression).as_deref()
                    == Some(self.names.logger_method.as_str()) =>
            {
                let kind = self.logger_kind(&member.object, &mut Seen::new())?;
                return Some(Method {
                    kind,
                    bound: Vec::new(),
                });
            }
            Form::Call(call) if access_name(&call.callee).as_deref() == Some("bind") => {
                let object = accessed(&call.callee)?;
                let mut method = self.logger_method(object, seen)?;
                method
                    .bound
                    .extend(call.arguments.iter().skip(1).map(argument));
                return Some(method);
            }
            _ => {}
        }

        let (key, declaration) = self.reference(value)?;

        if !seen.insert(key) {
            return None;
        }

        match declaration {
            Declaration::Element(element) => {
                let [link] = element.links.as_slice() else {
                    return None;
                };
                let super::source::Owner::Variable {
                    declarator,
                    constant: true,
                } = element.owner
                else {
                    return None;
                };

                if !link.object
                    || link.rest
                    || link.default.is_some()
                    || link.key.as_deref() != Some(self.names.logger_method.as_str())
                {
                    return None;
                }

                let kind = self.logger_kind(declarator.init.as_ref()?, &mut Seen::new())?;
                Some(Method {
                    kind,
                    bound: Vec::new(),
                })
            }
            Declaration::Property(property) if property.readonly => {
                self.logger_method(property.value.as_ref()?, seen)
            }
            declaration => self.logger_method(declaration.constant_initializer()?, seen),
        }
    }

    /// The native `error` call a call expression is, directly or through `call` or `apply`.
    pub fn invocation(&self, call: &'a CallExpression<'a>) -> Option<Invocation<'a>> {
        let callee = unwrap(&call.callee)?;
        let through = access_name(callee).filter(|name| name == "call" || name == "apply");

        if let Some(through) = through {
            let object = accessed(callee)?;
            let method = self.logger_method(object, &mut Seen::new())?;
            let array = call.arguments.get(1).and_then(argument).and_then(unwrap);
            let arguments: Option<Vec<_>> = if through == "call" {
                Some(call.arguments.iter().skip(1).map(argument).collect())
            } else {
                match array {
                    Some(Expression::ArrayExpression(array)) if call.arguments.len() == 2 => Some(
                        array
                            .elements
                            .iter()
                            .map(ArrayExpressionElement::as_expression)
                            .collect(),
                    ),
                    _ => None,
                }
            };

            return Some(Invocation {
                kind: method.kind,
                arguments: arguments.map(|arguments| [method.bound, arguments].concat()),
            });
        }

        let method = self.logger_method(callee, &mut Seen::new())?;
        let mut arguments = method.bound;
        arguments.extend(call.arguments.iter().map(argument));

        Some(Invocation {
            kind: method.kind,
            arguments: Some(arguments),
        })
    }

    /// The caught or `unknown`/`Error`-typed parameter an error value is, also through `const`
    /// aliases.
    fn error_evidence(&self, expression: &'a Expression<'a>, seen: &mut Seen) -> Option<SymbolId> {
        let Expression::Identifier(identifier) = unwrap(expression)? else {
            return None;
        };

        self.error_evidence_symbol(self.symbol(identifier)?, seen)
    }

    /// Whether a value carries raw error evidence anywhere within it.
    pub fn raw_evidence(&self, expression: &'a Expression<'a>, seen: &mut Seen) -> bool {
        let Some(value) = unwrap(expression) else {
            return false;
        };

        if let Expression::Identifier(identifier) = value {
            return self.raw_identifier(identifier, seen);
        }

        let mut walk = RawEvidence {
            source: self,
            seen,
            found: false,
        };
        walk.visit_expression(value);
        walk.found
    }

    fn raw_identifier(&self, identifier: &IdentifierReference<'a>, seen: &mut Seen) -> bool {
        self.symbol(identifier)
            .is_some_and(|symbol| self.raw_symbol(symbol, seen))
    }

    /// Whether a name, read or declared, is raw error evidence or is bound to a value carrying it.
    fn raw_symbol(&self, symbol: SymbolId, seen: &mut Seen) -> bool {
        if self
            .error_evidence_symbol(symbol, &mut Seen::new())
            .is_some()
        {
            return true;
        }

        seen.insert(Key::Symbol(symbol)) && self.raw_declaration(self.declaration(symbol), seen)
    }

    /// `error_evidence` for a symbol already resolved.
    fn error_evidence_symbol(&self, symbol: SymbolId, seen: &mut Seen) -> Option<SymbolId> {
        if !seen.insert(Key::Symbol(symbol)) {
            return None;
        }

        match self.declaration(symbol) {
            Declaration::Parameter(parameter) => {
                self.evidence_parameter(parameter).then_some(symbol)
            }
            Declaration::Catch => Some(symbol),
            declaration => self.error_evidence(declaration.constant_initializer()?, seen),
        }
    }

    /// Whether a parameter is typed `unknown` or as the library `Error`.
    fn evidence_parameter(&self, parameter: &FormalParameter<'a>) -> bool {
        self.evidence_type(
            parameter
                .type_annotation
                .as_deref()
                .map(|annotation| &annotation.type_annotation),
        )
    }

    /// Whether a declared type is `unknown` or the library `Error`.
    fn evidence_type(&self, annotation: Option<&TSType<'a>>) -> bool {
        match annotation {
            Some(TSType::TSUnknownKeyword(_)) => true,
            Some(TSType::TSTypeReference(reference)) => matches!(
                &reference.type_name,
                TSTypeName::IdentifierReference(name)
                    if name.name == "Error" && self.symbol(name).is_none()
            ),
            _ => false,
        }
    }

    /// Whether a declaration's initializer, or the defaults of the pattern elements around a
    /// destructured binding, carry raw error evidence.
    fn raw_declaration(&self, declaration: Declaration<'a>, seen: &mut Seen) -> bool {
        match declaration {
            Declaration::Element(element) => {
                if element.links.iter().rev().any(|link| {
                    link.default
                        .is_some_and(|default| self.raw_evidence(default, seen))
                }) {
                    return true;
                }

                match element.owner {
                    super::source::Owner::Catch => true,
                    super::source::Owner::Variable { declarator, .. } => declarator
                        .init
                        .as_ref()
                        .is_some_and(|initializer| self.raw_evidence(initializer, seen)),
                    super::source::Owner::Parameter => false,
                }
            }
            Declaration::Variable { declarator, .. } => declarator
                .init
                .as_ref()
                .is_some_and(|initializer| self.raw_evidence(initializer, seen)),
            Declaration::Property(property) => property
                .value
                .as_ref()
                .is_some_and(|initializer| self.raw_evidence(initializer, seen)),
            // A constructor parameter property read as a member is the parameter.
            Declaration::Parameter(parameter) => self.evidence_parameter(parameter),
            _ => false,
        }
    }

    /// Whether a value is a bounded message: a string, a template without raw error evidence, a
    /// conditional of two messages, a `string` parameter or a `const` alias of one.
    pub fn message(&self, expression: &'a Expression<'a>, seen: &mut Seen) -> bool {
        let Some(value) = unwrap(expression) else {
            return false;
        };

        if self.raw_evidence(value, &mut Seen::new()) {
            return false;
        }

        match value {
            Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => true,
            Expression::ConditionalExpression(conditional) => match unwrap(&conditional.test) {
                Some(Expression::BooleanLiteral(literal)) if literal.value => {
                    self.message(&conditional.consequent, seen)
                }
                Some(Expression::BooleanLiteral(_)) => self.message(&conditional.alternate, seen),
                _ => {
                    self.message(&conditional.consequent, &mut seen.clone())
                        && self.message(&conditional.alternate, &mut seen.clone())
                }
            },
            Expression::Identifier(identifier) => {
                let Some((key, declaration)) = self.named(identifier) else {
                    return false;
                };

                if !seen.insert(key) {
                    return false;
                }

                match declaration {
                    Declaration::Parameter(parameter) => matches!(
                        parameter
                            .type_annotation
                            .as_deref()
                            .map(|annotation| &annotation.type_annotation),
                        Some(TSType::TSStringKeyword(_))
                    ),
                    declaration => declaration
                        .constant_initializer()
                        .is_some_and(|initializer| self.message(initializer, seen)),
                }
            }
            _ => false,
        }
    }

    /// The error evidence a `FormatErrorEvidenceForLog(error)` call of Automator formats, also
    /// through `const` aliases of the call.
    pub fn formatter_evidence(
        &self,
        expression: &'a Expression<'a>,
        seen: &mut Seen,
    ) -> Option<SymbolId> {
        match form(unwrap(expression)?) {
            Form::Other(Expression::Identifier(identifier)) => {
                let (key, declaration) = self.named(identifier)?;

                if !seen.insert(key) {
                    return None;
                }

                self.formatter_evidence(declaration.constant_initializer()?, seen)
            }
            Form::Call(call)
                if call.arguments.len() == 1
                    && self.imported(
                        &call.callee,
                        &self.names.automator_package,
                        &self.names.evidence_formatter,
                    ) =>
            {
                self.error_evidence(argument(&call.arguments[0])?, &mut Seen::new())
            }
            _ => None,
        }
    }
}

/// Finds raw error evidence in a value as the compiler's child walk does: every name it reads,
/// except a shorthand property's, whose name the compiler binds to the property, and every member
/// read from a class of the file.
struct RawEvidence<'s, 'f, 'r, 'a> {
    source: &'s Source<'f, 'a>,
    seen: &'r mut Seen,
    found: bool,
}

impl<'a> Visit<'a> for RawEvidence<'_, '_, '_, 'a> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if !self.found && self.source.raw_identifier(it, self.seen) {
            self.found = true;
        }
    }

    // The compiler's walk also reads the names that declarations within the value introduce.
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        if !self.found
            && let Some(symbol) = it.symbol_id.get()
            && self.source.raw_symbol(symbol, self.seen)
        {
            self.found = true;
        }
    }

    // A typed `this` parameter is a name the compiler's walk reads like any parameter.
    fn visit_ts_this_parameter(&mut self, it: &TSThisParameter<'a>) {
        if !self.found
            && self.source.evidence_type(
                it.type_annotation
                    .as_deref()
                    .map(|annotation| &annotation.type_annotation),
            )
        {
            self.found = true;
        }

        walk::walk_ts_this_parameter(self, it);
    }

    // A destructuring assignment's shorthand target names the property, as in an object literal.
    fn visit_assignment_target_property_identifier(
        &mut self,
        it: &AssignmentTargetPropertyIdentifier<'a>,
    ) {
        if let Some(init) = &it.init {
            self.visit_expression(init);
        }
    }

    // The compiler strips type assertions before it walks a value, so their types are not read.
    fn visit_ts_as_expression(&mut self, it: &TSAsExpression<'a>) {
        self.visit_expression(&it.expression);
    }

    fn visit_ts_satisfies_expression(&mut self, it: &TSSatisfiesExpression<'a>) {
        self.visit_expression(&it.expression);
    }

    fn visit_ts_type_assertion(&mut self, it: &TSTypeAssertion<'a>) {
        self.visit_expression(&it.expression);
    }

    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.shorthand {
            return;
        }

        walk::walk_object_property(self, it);
    }

    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if self.found {
            return;
        }

        let nodes = self.source.file.semantic().nodes();

        if let AstKind::StaticMemberExpression(member) = nodes.kind(it.node_id.get())
            && let Some((key, declaration)) = self
                .source
                .member(&member.object, member.property.name.as_str())
            && self.seen.insert(key)
            && self.source.raw_declaration(declaration, self.seen)
        {
            self.found = true;
            return;
        }

        walk::walk_static_member_expression(self, it);
    }
}
