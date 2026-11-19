//! Private constant-failure guards: a private one-parameter method whose only statement is
//! `if (x)` or `if (!x)` without `else`, and which every use calls directly with the one boolean
//! literal that enters the branch. Such a guard always throws, so its branch is read as its body.
//!
//! Without a type checker, a use is the member
//! named like the guard read from a receiver whose declared or asserted type, else initializer,
//! denotes the guard's class: `this` in its instance members, a class-typed parameter or
//! variable, `new` of the class, or for a static guard the class itself, `typeof` it or `this` in
//! its static members; a type with `undefined` or `null` is read without them. A private `#name`
//! inside the class reads the guard whatever its receiver. An instance of another class of the
//! file, or an interface of it, that declares a member of that name, the guard class's member of
//! the other staticness, and an object literal with one bind another symbol and are no use; any
//! other receiver of a member with the guard's name, or of a computed member whose name is not a
//! literal on such a receiver or on any `this`, makes the uses uncertain. So does destructuring
//! the guard's name, a computed name or the rest of a public guard, in a declaration, parameter or
//! assignment and at any depth, from a receiver that may hold the guard: not an object literal
//! unless it spreads `this` or a value the rule cannot read (a spread drops another instance's
//! methods), not an interface that declares no such member and extends nothing, not a type with
//! no class member (`any`, `unknown`, `object`, a primitive, or an unannotated parameter of a
//! declared function or of a method other than a setter), and not one that binds another symbol. A nested pattern reads the
//! property of an object literal, or else an unknown receiver. An object assignment pattern, also
//! a nested one with its default, makes the uses uncertain whatever it reads when its receiver may
//! bind the guard: its own receiver, a subclass of the file, a literal spreading such a value, an
//! interface extending another type, or a receiver the rule cannot read. Each literal and class is
//! read once for the check.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, AssignmentTargetProperty, BindingPattern,
    BindingProperty, Class, ClassElement, Expression, IdentifierReference, MethodDefinition,
    MethodDefinitionKind, ObjectAssignmentTarget, ObjectExpression, ObjectPropertyKind,
    PropertyKey, Statement, TSAccessibility, TSInterfaceDeclaration, TSSignature, TSType,
    TSTypeName, TSTypeQueryExprName, UnaryOperator, VariableDeclarationKind,
};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::value::{string_like, unwrap};

use super::provenance::Provenance;
use super::shape::declaration_name;

/// What a receiver denotes, as far as its type can be read without the type checker.
#[derive(Clone, Copy)]
enum Receiver<'a> {
    /// An instance of a class of the file.
    Instance(&'a Class<'a>),
    /// A class of the file itself, as the receiver of its static members.
    Constructor(&'a Class<'a>),
    /// An object literal, which binds its own properties.
    Literal(&'a ObjectExpression<'a>),
    /// An interface of the file, which binds its own members.
    Interface(&'a TSInterfaceDeclaration<'a>),
    /// A value whose declared type holds no member of a class: `any`, `unknown`, `object`, a
    /// primitive, or the implicit `any` of an unannotated parameter of a declared function or
    /// method. Member reads from it are as uncertain as from an unknown receiver.
    Untyped,
    Unknown,
}

/// How an access relates to the guard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Relation {
    /// It reads the guard.
    Guard,
    /// It reads a member of another class that has its own member of that name.
    Other,
    /// It may read the guard.
    Unknown,
}

/// The guard's branch, when `method` is a private constant-failure guard of `class`.
pub fn constant_failure_branch<'a>(
    file: &SourceFile<'a>,
    class: &'a Class<'a>,
    method: &'a MethodDefinition<'a>,
) -> Option<&'a [Statement<'a>]> {
    let body = method.value.body.as_ref()?;
    let [Statement::IfStatement(branch)] = body.statements.as_slice() else {
        return None;
    };
    let parameters = &method.value.params;
    let [parameter] = parameters.items.as_slice() else {
        return None;
    };
    let name = match &method.key {
        PropertyKey::PrivateIdentifier(name) if !method.computed => format!("#{}", name.name),
        PropertyKey::StaticIdentifier(name)
            if !method.computed && method.accessibility == Some(TSAccessibility::Private) =>
        {
            name.name.to_string()
        }
        PropertyKey::StringLiteral(name)
            if !method.computed && method.accessibility == Some(TSAccessibility::Private) =>
        {
            name.value.to_string()
        }
        _ => return None,
    };
    let BindingPattern::BindingIdentifier(binding) = &parameter.pattern else {
        return None;
    };

    if parameters.rest.is_some()
        || parameter.initializer.is_some()
        || parameter.optional
        || branch.alternate.is_some()
    {
        return None;
    }

    let condition = unwrap(&branch.test);
    let (negated, predicate) = match condition {
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
            (true, unwrap(&unary.argument))
        }
        other => (false, other),
    };
    let Expression::Identifier(predicate) = predicate else {
        return None;
    };

    if Provenance::symbol(file, predicate).is_none()
        || Provenance::symbol(file, predicate) != binding.symbol_id.get()
    {
        return None;
    }

    let uses = Uses {
        file,
        class,
        name: &name,
        r#static: method.r#static,
        rejecting: !negated,
        bindings: RefCell::default(),
        walks: Cell::default(),
        holds: RefCell::default(),
        mays: RefCell::default(),
    };

    if !uses.only_constant_calls() {
        return None;
    }

    Some(match &branch.consequent {
        Statement::BlockStatement(block) => &block.body,
        statement => std::slice::from_ref(statement),
    })
}

/// The remembered answer of `key`, computing it once with `compute`; a key reached again while it
/// is computed answers `false`.
fn remembered<K: Copy + Eq + std::hash::Hash>(
    answers: &RefCell<HashMap<K, Option<bool>>>,
    key: K,
    compute: impl FnOnce() -> bool,
) -> bool {
    if let Some(answer) = answers.borrow().get(&key) {
        return answer.unwrap_or(false);
    }

    answers.borrow_mut().insert(key, None);
    let answer = compute();
    answers.borrow_mut().insert(key, Some(answer));

    answer
}

struct Uses<'f, 'a> {
    file: &'f SourceFile<'a>,
    class: &'a Class<'a>,
    name: &'f str,
    r#static: bool,
    rejecting: bool,
    /// The receiver each binding denotes, once followed.
    bindings: RefCell<HashMap<SymbolId, Receiver<'a>>>,
    /// How many patterns and bindings have been walked, the work the tests bound.
    walks: Cell<usize>,
    /// Whether a literal or class, by its start and for a class whether it is an instance, may
    /// bind the guard in an object assignment; `None` while it is being read.
    holds: RefCell<HashMap<(u32, bool), Option<bool>>>,
    /// Whether a literal, by its start, may hold the guard when destructured; `None` while it is
    /// being read.
    mays: RefCell<HashMap<u32, Option<bool>>>,
}

impl<'a> Uses<'_, 'a> {
    /// Whether the guard has a use and every use is a direct call with the rejecting literal.
    fn only_constant_calls(&self) -> bool {
        let nodes = self.file.semantic().nodes();
        let mut calls = 0;

        for node in nodes.iter() {
            let id = node.id();

            match node.kind() {
                AstKind::StaticMemberExpression(access) => {
                    match self.access(
                        id,
                        access.span,
                        &access.object,
                        Some(access.property.name.to_string()),
                        access.optional,
                    ) {
                        Some(true) => calls += 1,
                        Some(false) => {}
                        None => return false,
                    }
                }
                AstKind::PrivateFieldExpression(access) => {
                    match self.access(
                        id,
                        access.span,
                        &access.object,
                        Some(format!("#{}", access.field.name)),
                        access.optional,
                    ) {
                        Some(true) => calls += 1,
                        Some(false) => {}
                        None => return false,
                    }
                }
                AstKind::ComputedMemberExpression(access) => {
                    let name = self.literal(&access.expression);

                    match self.access(id, access.span, &access.object, name, access.optional) {
                        Some(true) => calls += 1,
                        Some(false) => {}
                        None => return false,
                    }
                }
                // Destructuring never reads a private name.
                _ if self.name.starts_with('#')
                    && matches!(
                        node.kind(),
                        AstKind::VariableDeclarator(_)
                            | AstKind::FormalParameter(_)
                            | AstKind::AssignmentExpression(_)
                            | AstKind::AssignmentTargetWithDefault(_)
                    ) => {}
                AstKind::VariableDeclarator(declarator)
                    if !matches!(declarator.id, BindingPattern::BindingIdentifier(_)) =>
                {
                    let receiver = match (&declarator.type_annotation, &declarator.init) {
                        (Some(annotation), _) => self.typed(&annotation.type_annotation),
                        (None, Some(init)) => self.receiver(init),
                        (None, None) => Receiver::Unknown,
                    };

                    if self.destructures(&declarator.id, &[receiver]) {
                        return false;
                    }
                }
                AstKind::FormalParameter(parameter)
                    if !matches!(parameter.pattern, BindingPattern::BindingIdentifier(_)) =>
                {
                    let receiver = match (&parameter.type_annotation, &parameter.initializer) {
                        (Some(annotation), _) => self.typed(&annotation.type_annotation),
                        (None, Some(initializer)) => self.receiver(initializer),
                        (None, None) if self.declared_function_parameter(id) => Receiver::Untyped,
                        (None, None) => Receiver::Unknown,
                    };

                    if self.destructures(&parameter.pattern, &[receiver]) {
                        return false;
                    }
                }
                // An object assignment pattern, also a nested one with its default, binds its
                // properties to the receiver's members: it makes the uses uncertain when it reads
                // the guard's name from a receiver that may hold the guard, or reads anything from
                // one that holds it.
                AstKind::AssignmentExpression(assignment)
                    if assignment.operator == AssignmentOperator::Assign =>
                {
                    if let AssignmentTarget::ObjectAssignmentTarget(target) = &assignment.left
                        && self.assigns(target, self.receiver(&assignment.right))
                    {
                        return false;
                    }
                }
                AstKind::AssignmentTargetWithDefault(default) => {
                    if let AssignmentTarget::ObjectAssignmentTarget(target) = &default.binding
                        && self.assigns(target, self.receiver(&default.init))
                    {
                        return false;
                    }
                }
                _ => {}
            }
        }

        calls > 0
    }

    /// Whether a binding pattern, or one nested in it, reads the guard's name, a computed name or
    /// the rest from one of the `receivers` it may read, which may be the guard's. A default adds
    /// its receiver, so each pattern is walked once.
    fn destructures(&self, pattern: &'a BindingPattern<'a>, receivers: &[Receiver<'a>]) -> bool {
        self.walks.set(self.walks.get() + 1);

        match pattern {
            BindingPattern::BindingIdentifier(_) => false,
            // A default supplies the value when the receiver's is undefined.
            BindingPattern::AssignmentPattern(assignment) => {
                let mut receivers = receivers.to_vec();
                receivers.push(self.receiver(&assignment.right));

                self.destructures(&assignment.left, &receivers)
            }
            BindingPattern::ArrayPattern(array) => array
                .elements
                .iter()
                .flatten()
                .chain(array.rest.as_ref().map(|rest| &rest.argument))
                .any(|element| self.destructures(element, &[Receiver::Unknown])),
            BindingPattern::ObjectPattern(object) => {
                let reads = object.rest.is_some()
                    || object.properties.iter().any(|property| {
                        property.computed
                            || super::provenance::static_name(&property.key, false).as_deref()
                                == Some(self.name)
                    });

                reads
                    && receivers
                        .iter()
                        .any(|receiver| self.may_hold_guard(*receiver))
                    || object.properties.iter().any(|property| {
                        let nested: Vec<_> = receivers
                            .iter()
                            .map(|receiver| self.property(*receiver, property))
                            .collect();

                        self.destructures(&property.value, &nested)
                    })
            }
        }
    }

    /// Whether an object assignment target reads from a receiver in a way that makes the guard's
    /// uses uncertain.
    fn assigns(&self, target: &'a ObjectAssignmentTarget<'a>, receiver: Receiver<'a>) -> bool {
        let reads = target.rest.is_some()
            || target.properties.iter().any(|property| match property {
                AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
                    property.binding.name == self.name
                }
                AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
                    property.computed
                        || super::provenance::static_name(&property.name, false).as_deref()
                            == Some(self.name)
                }
            });

        reads && self.may_hold_guard(receiver) || self.holds_guard(receiver)
    }

    /// Whether an object assignment from a receiver may bind the guard: the guard's own receiver,
    /// a subclass of the file, an object literal spreading such a value, an interface extending
    /// another type, or a receiver the rule cannot read; a type with no class member holds none.
    /// Each literal and class is read once for the check.
    fn holds_guard(&self, receiver: Receiver<'a>) -> bool {
        self.walks.set(self.walks.get() + 1);

        match receiver {
            Receiver::Literal(object) => {
                remembered(&self.holds, (object.span.start, false), || {
                    self.spreads(object, |spread| self.holds_guard(spread))
                })
            }
            Receiver::Instance(class) | Receiver::Constructor(class)
                if class.span == self.class.span =>
            {
                self.relation(receiver) == Relation::Guard
            }
            Receiver::Instance(class) | Receiver::Constructor(class) => {
                let instance = matches!(receiver, Receiver::Instance(_));

                remembered(&self.holds, (class.span.start, instance), || {
                    let Some(heritage) = &class.heritage else {
                        return false;
                    };
                    let Expression::Identifier(parent) = unwrap(&heritage.expression) else {
                        return true;
                    };

                    self.class_named(parent).is_none_or(|parent| {
                        self.holds_guard(if instance {
                            Receiver::Instance(parent)
                        } else {
                            Receiver::Constructor(parent)
                        })
                    })
                })
            }
            Receiver::Interface(interface) => !interface.extends.is_empty(),
            Receiver::Untyped => false,
            Receiver::Unknown => true,
        }
    }

    /// Whether a destructured receiver may hold the guard: an object literal holds only its own
    /// properties and what it spreads, an interface without the guard's name holds it only through
    /// a type it extends, a type with no class member holds none, and another symbol of the
    /// guard's name is not the guard.
    fn may_hold_guard(&self, receiver: Receiver<'a>) -> bool {
        self.may_hold(receiver)
    }

    /// `may_hold_guard`, reading each literal once for the check.
    fn may_hold(&self, receiver: Receiver<'a>) -> bool {
        self.walks.set(self.walks.get() + 1);

        match receiver {
            Receiver::Literal(object) => remembered(&self.mays, object.span.start, || {
                self.spreads(object, |spread| self.may_hold(spread))
            }),
            Receiver::Interface(interface) => {
                self.relation(receiver) != Relation::Other && !interface.extends.is_empty()
            }
            Receiver::Untyped => false,
            receiver => self.relation(receiver) != Relation::Other,
        }
    }

    /// Whether a spread of `object` passes `holds`. A spread keeps the members of `this`, as the
    /// compiler reads its type, but drops the methods of any other instance or class.
    fn spreads(
        &self,
        object: &'a ObjectExpression<'a>,
        mut holds: impl FnMut(Receiver<'a>) -> bool,
    ) -> bool {
        object.properties.iter().any(|property| {
            let ObjectPropertyKind::SpreadProperty(spread) = property else {
                return false;
            };
            let receiver = self.receiver(&spread.argument);

            match receiver {
                Receiver::Instance(_) | Receiver::Constructor(_)
                    if !matches!(unwrap(&spread.argument), Expression::ThisExpression(_)) =>
                {
                    false
                }
                receiver => holds(receiver),
            }
        })
    }

    /// The receiver a nested pattern reads: an object literal's property, else unknown.
    fn property(&self, receiver: Receiver<'a>, property: &'a BindingProperty<'a>) -> Receiver<'a> {
        let Receiver::Literal(object) = receiver else {
            return Receiver::Unknown;
        };

        super::provenance::static_name(&property.key, property.computed)
            .and_then(|name| {
                object
                    .properties
                    .iter()
                    .find_map(|candidate| match candidate {
                        ObjectPropertyKind::ObjectProperty(candidate)
                            if super::provenance::static_name(
                                &candidate.key,
                                candidate.computed,
                            )
                            .as_deref()
                                == Some(name.as_str()) =>
                        {
                            Some(self.receiver(&candidate.value))
                        }
                        _ => None,
                    })
            })
            .unwrap_or(Receiver::Unknown)
    }

    /// A computed key's name: a string literal, or a `const` bound to one without a type.
    fn literal(&self, key: &Expression<'a>) -> Option<String> {
        let key = unwrap(key);

        if let Some(name) = string_like(key) {
            return Some(name);
        }

        let Expression::Identifier(identifier) = key else {
            return None;
        };
        let initializer = self.constant(Provenance::symbol(self.file, identifier)?)?;

        string_like(unwrap(initializer))
    }

    /// The initializer of a `const` binding.
    fn constant(&self, symbol: SymbolId) -> Option<&'a Expression<'a>> {
        let semantic = self.file.semantic();
        let declarations: Vec<_> = semantic.scoping().symbol_declarations(symbol).collect();
        let [declaration] = declarations.as_slice() else {
            return None;
        };

        match semantic.nodes().kind(*declaration) {
            AstKind::VariableDeclarator(declarator)
                if matches!(
                    semantic.nodes().parent_kind(*declaration),
                    AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                ) && matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol)) =>
            {
                // A declared type is the key's type, which need not be its initializer's literal.
                declarator
                    .type_annotation
                    .is_none()
                    .then_some(declarator.init.as_ref())
                    .flatten()
            }
            _ => None,
        }
    }

    /// `Some(true)` for a direct call of the guard with the rejecting literal, `Some(false)` for an
    /// access that is no use, and `None` for an uncertain use.
    fn access(
        &self,
        id: NodeId,
        span: Span,
        object: &'a Expression<'a>,
        name: Option<String>,
        optional: bool,
    ) -> Option<bool> {
        // A private name always reads the member of the class that lexically declares it.
        let relation = if self.name.starts_with('#') && name.as_deref() == Some(self.name) {
            match self.private_owner(id) {
                Some(class) if class.span == self.class.span => Relation::Guard,
                _ => Relation::Other,
            }
        } else {
            self.relation(self.receiver(object))
        };
        let literal_this = matches!(unwrap(object), Expression::ThisExpression(_));

        match name {
            Some(name) if name == self.name => match relation {
                Relation::Guard => (!optional && self.constant_call(id, span)).then_some(true),
                Relation::Other => Some(false),
                Relation::Unknown => None,
            },
            None if relation == Relation::Guard || literal_this => None,
            _ => Some(false),
        }
    }

    /// How a receiver's member named like the guard relates to the guard.
    fn relation(&self, receiver: Receiver<'a>) -> Relation {
        let (class, r#static) = match receiver {
            Receiver::Instance(class) => (class, false),
            Receiver::Constructor(class) => (class, true),
            Receiver::Literal(object) => {
                let declares = object.properties.iter().any(|property| {
                    matches!(property, ObjectPropertyKind::ObjectProperty(property)
                        if super::provenance::static_name(&property.key, property.computed).as_deref() == Some(self.name))
                });

                return if declares {
                    Relation::Other
                } else {
                    Relation::Unknown
                };
            }
            Receiver::Interface(interface) => {
                let declares = interface.body.body.iter().any(|signature| {
                    let (key, computed) = match signature {
                        TSSignature::TSPropertySignature(property) => {
                            (&property.key, property.computed)
                        }
                        TSSignature::TSMethodSignature(method) => (&method.key, method.computed),
                        _ => return false,
                    };

                    declaration_name(key, computed).as_deref() == Some(self.name)
                });

                return if declares {
                    Relation::Other
                } else {
                    Relation::Unknown
                };
            }
            Receiver::Untyped | Receiver::Unknown => return Relation::Unknown,
        };

        if class.span == self.class.span && r#static == self.r#static {
            return Relation::Guard;
        }

        let declares = class.body.body.iter().any(|element| {
            let (key, computed, element_static) = match element {
                ClassElement::MethodDefinition(method) => {
                    (&method.key, method.computed, method.r#static)
                }
                ClassElement::PropertyDefinition(property) => {
                    (&property.key, property.computed, property.r#static)
                }
                ClassElement::AccessorProperty(property) => {
                    (&property.key, property.computed, property.r#static)
                }
                _ => return false,
            };

            element_static == r#static
                && declaration_name(key, computed).as_deref() == Some(self.name)
        });

        // Another class's own member, or the guard class's member of the other staticness, binds
        // another symbol; without one the compiler binds none.
        if declares {
            Relation::Other
        } else {
            Relation::Unknown
        }
    }

    /// Whether the access is the callee of a call, through parentheses and type assertions, with
    /// the rejecting boolean literal as its only argument.
    fn constant_call(&self, id: NodeId, span: Span) -> bool {
        let nodes = self.file.semantic().nodes();
        let mut callee = span;
        let mut parent = nodes.parent_id(id);

        loop {
            match nodes.kind(parent) {
                AstKind::ParenthesizedExpression(wrapper)
                    if wrapper.expression.span() == callee =>
                {
                    callee = wrapper.span;
                }
                AstKind::TSAsExpression(wrapper) if wrapper.expression.span() == callee => {
                    callee = wrapper.span;
                }
                AstKind::TSSatisfiesExpression(wrapper) if wrapper.expression.span() == callee => {
                    callee = wrapper.span;
                }
                AstKind::TSNonNullExpression(wrapper) if wrapper.expression.span() == callee => {
                    callee = wrapper.span;
                }
                AstKind::TSTypeAssertion(wrapper) if wrapper.expression.span() == callee => {
                    callee = wrapper.span;
                }
                AstKind::CallExpression(call) => {
                    let [argument] = call.arguments.as_slice() else {
                        return false;
                    };

                    return call.callee.span() == callee
                        && !call.optional
                        && matches!(
                            argument.as_expression().map(unwrap),
                            Some(Expression::BooleanLiteral(literal)) if literal.value == self.rejecting
                        );
                }
                _ => return false,
            }

            parent = nodes.parent_id(parent);
        }
    }

    /// What an expression denotes as a receiver: its declared or asserted type, else the type of
    /// its initializer.
    fn receiver(&self, expression: &'a Expression<'a>) -> Receiver<'a> {
        self.followed(expression, &mut Vec::new())
    }

    /// The receiver of an expression; `seen` holds the bindings whose initializers are being
    /// followed, so a binding that reaches itself is unknown.
    fn followed(&self, expression: &'a Expression<'a>, seen: &mut Vec<SymbolId>) -> Receiver<'a> {
        match expression {
            Expression::ParenthesizedExpression(inner) => self.followed(&inner.expression, seen),
            Expression::TSNonNullExpression(inner) => self.followed(&inner.expression, seen),
            Expression::TSSatisfiesExpression(inner) => self.followed(&inner.expression, seen),
            Expression::TSAsExpression(inner) => self.typed(&inner.type_annotation),
            Expression::TSTypeAssertion(inner) => self.typed(&inner.type_annotation),
            Expression::ThisExpression(this) => self.this_owner(this.node_id.get()),
            Expression::ObjectExpression(object) => Receiver::Literal(object),
            Expression::NewExpression(construction) => match unwrap(&construction.callee) {
                Expression::Identifier(identifier) => match self.class_named(identifier) {
                    Some(class) => Receiver::Instance(class),
                    None => Receiver::Unknown,
                },
                _ => Receiver::Unknown,
            },
            Expression::Identifier(identifier) => {
                if let Some(class) = self.class_named(identifier) {
                    return Receiver::Constructor(class);
                }

                let Some(symbol) = Provenance::symbol(self.file, identifier) else {
                    return Receiver::Unknown;
                };
                if let Some(receiver) = self.bindings.borrow().get(&symbol) {
                    return *receiver;
                }

                if seen.contains(&symbol) {
                    return Receiver::Unknown;
                }

                seen.push(symbol);
                let receiver = self.bound(symbol, seen);
                self.bindings.borrow_mut().insert(symbol, receiver);

                receiver
            }
            _ => Receiver::Unknown,
        }
    }

    /// The receiver the one declaration of a binding gives it.
    fn bound(&self, symbol: SymbolId, seen: &mut Vec<SymbolId>) -> Receiver<'a> {
        self.walks.set(self.walks.get() + 1);
        let semantic = self.file.semantic();
        let declarations: Vec<_> = semantic.scoping().symbol_declarations(symbol).collect();
        let [declaration] = declarations.as_slice() else {
            return Receiver::Unknown;
        };

        match semantic.nodes().kind(*declaration) {
            AstKind::FormalParameter(parameter) => match &parameter.type_annotation {
                Some(annotation) => self.typed(&annotation.type_annotation),
                None if self.declared_function_parameter(*declaration) => Receiver::Untyped,
                None => Receiver::Unknown,
            },
            AstKind::VariableDeclarator(declarator) if matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol)) => {
                match (&declarator.type_annotation, &declarator.init) {
                    (Some(annotation), _) => self.typed(&annotation.type_annotation),
                    (None, Some(init)) => self.followed(init, seen),
                    (None, None) => Receiver::Unknown,
                }
            }
            _ => Receiver::Unknown,
        }
    }

    /// Whether a parameter belongs to a function declaration or a method other than a setter, whose
    /// unannotated parameters the compiler reads as `any`; a setter's takes its getter's type and a
    /// function expression's may be contextually typed.
    fn declared_function_parameter(&self, parameter: NodeId) -> bool {
        let nodes = self.file.semantic().nodes();
        let function = nodes.parent_id(nodes.parent_id(parameter));

        match nodes.kind(function) {
            AstKind::Function(function_node) => {
                function_node.is_declaration()
                    || matches!(nodes.parent_kind(function), AstKind::MethodDefinition(method)
                        if method.kind != MethodDefinitionKind::Set)
            }
            _ => false,
        }
    }

    /// The class of the file an identifier names.
    fn class_named(&self, identifier: &IdentifierReference<'a>) -> Option<&'a Class<'a>> {
        match self.declared(identifier)? {
            AstKind::Class(class) => Some(class),
            _ => None,
        }
    }

    /// The one declaration of the file an identifier names.
    fn declared(&self, identifier: &IdentifierReference<'a>) -> Option<AstKind<'a>> {
        let symbol = Provenance::symbol(self.file, identifier)?;
        let semantic = self.file.semantic();
        let declarations: Vec<_> = semantic.scoping().symbol_declarations(symbol).collect();
        let [declaration] = declarations.as_slice() else {
            return None;
        };

        Some(semantic.nodes().kind(*declaration))
    }

    /// The innermost class around a node that declares a private member of the guard's name.
    fn private_owner(&self, id: NodeId) -> Option<&'a Class<'a>> {
        let nodes = self.file.semantic().nodes();

        nodes
            .ancestor_ids(id)
            .find_map(|ancestor| match nodes.kind(ancestor) {
                AstKind::Class(class)
                    if class.body.body.iter().any(|element| {
                        matches!(element.property_key(), Some(PropertyKey::PrivateIdentifier(name))
                            if format!("#{}", name.name) == self.name)
                    }) =>
                {
                    Some(class)
                }
                _ => None,
            })
    }

    /// What a type names: an instance of a class of the file, or with `typeof` the class itself.
    fn typed(&self, annotation: &TSType<'a>) -> Receiver<'a> {
        match annotation {
            TSType::TSTypeReference(reference) => match &reference.type_name {
                TSTypeName::IdentifierReference(name) => match self.declared(name) {
                    Some(AstKind::Class(class)) => Receiver::Instance(class),
                    Some(AstKind::TSInterfaceDeclaration(interface)) => {
                        Receiver::Interface(interface)
                    }
                    _ => Receiver::Unknown,
                },
                _ => Receiver::Unknown,
            },
            TSType::TSParenthesizedType(inner) => self.typed(&inner.type_annotation),
            // Without strict null checks the compiler reads `T | undefined` as `T`.
            TSType::TSUnionType(union) => {
                let mut members = union.types.iter().filter(|member| {
                    !matches!(
                        member,
                        TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_)
                    )
                });

                match (members.next(), members.next()) {
                    (Some(member), None) => self.typed(member),
                    _ => Receiver::Unknown,
                }
            }
            TSType::TSAnyKeyword(_)
            | TSType::TSUnknownKeyword(_)
            | TSType::TSObjectKeyword(_)
            | TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSBooleanKeyword(_)
            | TSType::TSBigIntKeyword(_)
            | TSType::TSSymbolKeyword(_) => Receiver::Untyped,
            TSType::TSTypeQuery(query) => match &query.expr_name {
                TSTypeQueryExprName::IdentifierReference(name) => self
                    .class_named(name)
                    .map_or(Receiver::Unknown, Receiver::Constructor),
                _ => Receiver::Unknown,
            },
            _ => Receiver::Unknown,
        }
    }

    /// The class `this` denotes at a node: an instance in instance members, the class itself in
    /// static members.
    fn this_owner(&self, id: NodeId) -> Receiver<'a> {
        let nodes = self.file.semantic().nodes();

        for ancestor in nodes.ancestor_ids(id) {
            let r#static = match nodes.kind(ancestor) {
                AstKind::Function(_) => match nodes.parent_kind(ancestor) {
                    AstKind::MethodDefinition(method) => method.r#static,
                    _ => return Receiver::Unknown,
                },
                AstKind::PropertyDefinition(property) => property.r#static,
                AstKind::StaticBlock(_) => true,
                AstKind::Class(_) | AstKind::Program(_) => return Receiver::Unknown,
                _ => continue,
            };

            return match self.class_of(ancestor) {
                Some(class) if r#static => Receiver::Constructor(class),
                Some(class) => Receiver::Instance(class),
                None => Receiver::Unknown,
            };
        }

        Receiver::Unknown
    }

    /// The class a class element belongs to.
    fn class_of(&self, element: NodeId) -> Option<&'a Class<'a>> {
        let nodes = self.file.semantic().nodes();

        nodes
            .ancestor_ids(element)
            .find_map(|ancestor| match nodes.kind(ancestor) {
                AstKind::Class(class) => Some(class),
                _ => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fmt::Write;

    use super::Uses;
    use crate::rules::semantics::wrappers::tests::with_source;
    use crate::shared::syntax::top_level_classes;

    const GUARD: &str =
        "private check(failed: boolean): void { if (failed) { throw new Error(\"x\"); } }";

    /// Whether the instance guard `check` of the first class of `source` has only constant calls,
    /// with the number of patterns and bindings walked to decide it.
    fn uses(source: &str) -> (bool, usize) {
        with_source(source, |file| {
            let class = top_level_classes(&file.program().body)[0];
            let uses = Uses {
                file,
                class,
                name: "check",
                r#static: false,
                rejecting: true,
                bindings: RefCell::default(),
                walks: Cell::default(),
                holds: RefCell::default(),
                mays: RefCell::default(),
            };
            let constant = uses.only_constant_calls();

            (constant, uses.walks.get())
        })
    }

    // A default adds its receiver to the one walk of its pattern, so each level of nested
    // defaults costs two walks, the default and its object pattern, instead of doubling.
    #[test]
    fn nested_destructuring_defaults_are_walked_once() {
        let mut pattern = String::from("{ z }");

        for _ in 0..26 {
            pattern = format!("{{ a: {pattern} = {{}} }}");
        }

        let source = format!(
            "export class Owner {{\n {GUARD}\n run() {{ this.check(true); const {pattern} = {{}}; void z; }}\n}}\n"
        );

        assert_eq!(uses(&source), (true, 2 * 26 + 2));
    }

    // Each binding of a receiver chain is followed once for all its reads.
    #[test]
    fn a_receiver_chain_is_followed_once_per_binding() {
        let mut body = String::from("const a0 = this;");

        for index in 1..1000 {
            write!(body, " const a{index} = a{};", index - 1).expect("writes");
        }

        for index in 0..1000 {
            write!(body, " a{index}.check(true);").expect("writes");
        }

        let source = format!("export class Owner {{\n {GUARD}\n run() {{ {body} }}\n}}\n");

        assert_eq!(uses(&source), (true, 1000));
    }

    // Each literal is read once for the spreads that reach it, so literals that each spread the
    // previous one twice cost three walks apiece, its read, its second spread finding it visited
    // and its binding followed once, instead of doubling at each step.
    #[test]
    fn a_spread_tree_is_read_once_per_literal() {
        let mut body = String::from("const a0 = {};");

        for index in 1..=30 {
            write!(
                body,
                " const a{index} = {{ ...a{}, ...a{} }};",
                index - 1,
                index - 1
            )
            .expect("writes");
        }

        let source = format!(
            "export class Owner {{\n {GUARD}\n run() {{ this.check(true); {body} let x; ({{ x }} = a30); void x; }}\n}}\n"
        );

        assert_eq!(uses(&source), (true, 3 * 30 + 2));
    }

    // A literal is read once for the whole check, so many assignments from the literals of one
    // spread chain cost a walk per literal and per assignment, not one chain walk per assignment.
    #[test]
    fn a_spread_chain_is_read_once_for_all_assignments() {
        let walks = |length: usize| {
            let mut body = String::from("const a0 = {};");

            for index in 1..=length {
                write!(body, " const a{index} = {{ ...a{} }};", index - 1).expect("writes");
            }

            for index in 0..=length {
                write!(body, " ({{ x }} = a{index});").expect("writes");
            }

            let source = format!(
                "export class Owner {{\n {GUARD}\n run() {{ this.check(true); let x; {body} void x; }}\n}}\n"
            );
            let (constant, walks) = uses(&source);

            assert!(constant);
            walks
        };

        assert_eq!(walks(400), 2 * walks(200) - walks(0));
    }
}
