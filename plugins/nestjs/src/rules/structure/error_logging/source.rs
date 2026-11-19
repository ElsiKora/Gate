//! The declarations of one file as a compiler program of that file alone reads them: a name's first declaration, and a member read from `this`, from a class of the file
//! or from a value typed as or constructed from one, which the compiler bound through the type.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Expression, FormalParameter, IdentifierReference,
    MethodDefinitionKind, PropertyDefinition, PropertyKey, TSType, TSTypeAliasDeclaration,
    TSTypeName, TSTypeQueryExprName, VariableDeclarationKind, VariableDeclarator,
};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::value::string_like;

use super::Names;

/// What a name or member was declared as.
#[derive(Clone)]
pub enum Declaration<'a> {
    /// A named import of `name` from `module`.
    Import {
        module: &'a str,
        name: String,
    },
    /// A namespace import of `module`.
    Namespace {
        module: &'a str,
    },
    /// A variable declared by itself, not by a destructuring pattern.
    Variable {
        declarator: &'a VariableDeclarator<'a>,
        constant: bool,
    },
    /// A binding of a destructuring pattern.
    Element(Element<'a>),
    /// A parameter declared by itself, also a constructor parameter property.
    Parameter(&'a FormalParameter<'a>),
    /// A `catch` binding declared by itself.
    Catch,
    Class(&'a Class<'a>),
    /// A class field.
    Property(&'a PropertyDefinition<'a>),
    TypeAlias(&'a TSTypeAliasDeclaration<'a>),
    Other,
}

/// A binding of a destructuring pattern and the pattern elements that lead to it.
#[derive(Clone)]
pub struct Element<'a> {
    /// Each element from the outermost pattern to the binding.
    pub links: Vec<Link<'a>>,
    pub owner: Owner<'a>,
}

/// One pattern element: an object property, an array element or a rest element.
#[derive(Clone)]
pub struct Link<'a> {
    /// The property name of an object pattern element; `None` for an array or rest element or a
    /// computed name that is not a string literal.
    pub key: Option<String>,
    pub object: bool,
    pub rest: bool,
    pub default: Option<&'a Expression<'a>>,
}

/// What declares a destructuring pattern.
#[derive(Clone)]
pub enum Owner<'a> {
    Variable {
        declarator: &'a VariableDeclarator<'a>,
        constant: bool,
    },
    Parameter,
    Catch,
}

/// What a guard against cycles has already followed.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Symbol(SymbolId),
    Node(NodeId),
}

pub type Seen = HashSet<Key>;

impl<'a> Declaration<'a> {
    /// The initializer of a `const` variable.
    pub fn constant_initializer(&self) -> Option<&'a Expression<'a>> {
        match self {
            Self::Variable {
                declarator,
                constant: true,
            } => declarator.init.as_ref(),
            _ => None,
        }
    }
}

/// A class of the file, with `true` for the class itself rather than an instance.
pub type Held<'a> = (&'a Class<'a>, bool);

/// What a declaration holds: unknown while it is read, then its class if any.
#[derive(Clone, Copy)]
pub enum Holding<'a> {
    Reading,
    Read(Option<Held<'a>>),
}

/// The declarations of one file.
pub struct Source<'f, 'a> {
    pub file: &'f SourceFile<'a>,
    /// The names the rule looks for.
    pub names: &'f Names,
    /// The class each declaration holds, once read; `None` while it is being read.
    pub holds: RefCell<HashMap<Key, Holding<'a>>>,
    /// How many declarations have been read for the class they hold, the work the tests bound.
    pub readings: Cell<usize>,
}

impl<'a> Source<'_, 'a> {
    /// The symbol a reference names, or `None` for a library name.
    pub fn symbol(&self, identifier: &IdentifierReference<'a>) -> Option<SymbolId> {
        let scoping = self.file.semantic().scoping();

        identifier
            .reference_id
            .get()
            .and_then(|reference| scoping.get_reference(reference).symbol_id())
    }

    /// The first declaration of a symbol.
    pub fn declaration(&self, symbol: SymbolId) -> Declaration<'a> {
        let semantic = self.file.semantic();
        let nodes = semantic.nodes();
        let node = semantic.scoping().symbol_declaration(symbol);

        match nodes.kind(node) {
            AstKind::ImportSpecifier(specifier) => nodes
                .ancestor_ids(node)
                .find_map(|ancestor| match nodes.kind(ancestor) {
                    AstKind::ImportDeclaration(import) => Some(Declaration::Import {
                        module: import.source.value.as_str(),
                        name: specifier.imported.name().to_string(),
                    }),
                    _ => None,
                })
                .unwrap_or(Declaration::Other),
            AstKind::ImportNamespaceSpecifier(_) => nodes
                .ancestor_ids(node)
                .find_map(|ancestor| match nodes.kind(ancestor) {
                    AstKind::ImportDeclaration(import) => Some(Declaration::Namespace {
                        module: import.source.value.as_str(),
                    }),
                    _ => None,
                })
                .unwrap_or(Declaration::Other),
            AstKind::VariableDeclarator(declarator) => {
                let constant = matches!(
                    nodes.parent_kind(node),
                    AstKind::VariableDeclaration(list)
                        if matches!(list.kind, VariableDeclarationKind::Const | VariableDeclarationKind::AwaitUsing)
                );

                if matches!(&declarator.id, BindingPattern::BindingIdentifier(_)) {
                    Declaration::Variable {
                        declarator,
                        constant,
                    }
                } else {
                    element(
                        &declarator.id,
                        symbol,
                        Owner::Variable {
                            declarator,
                            constant,
                        },
                    )
                }
            }
            AstKind::FormalParameter(parameter) => {
                if matches!(&parameter.pattern, BindingPattern::BindingIdentifier(_)) {
                    Declaration::Parameter(parameter)
                } else {
                    element(&parameter.pattern, symbol, Owner::Parameter)
                }
            }
            AstKind::CatchParameter(parameter) => {
                if matches!(&parameter.pattern, BindingPattern::BindingIdentifier(_)) {
                    Declaration::Catch
                } else {
                    element(&parameter.pattern, symbol, Owner::Catch)
                }
            }
            AstKind::Class(class) => Declaration::Class(class),
            AstKind::TSTypeAliasDeclaration(alias) => Declaration::TypeAlias(alias),
            _ => Declaration::Other,
        }
    }

    /// The declaration a reference names, with its cycle key.
    pub fn named(&self, identifier: &IdentifierReference<'a>) -> Option<(Key, Declaration<'a>)> {
        let symbol = self.symbol(identifier)?;

        Some((Key::Symbol(symbol), self.declaration(symbol)))
    }

    /// The declaration a type name's identifier names.
    pub fn type_named(&self, name: &TSTypeName<'a>) -> Option<(Key, Declaration<'a>)> {
        match name {
            TSTypeName::IdentifierReference(identifier) => self.named(identifier),
            _ => None,
        }
    }

    /// The declaration of the member `name` read from `object`, which the compiler bound through
    /// the type of `object`.
    pub fn member(&self, object: &'a Expression<'a>, name: &str) -> Option<(Key, Declaration<'a>)> {
        let (class, r#static) = self.receiver(object)?;

        self.class_member(class, name, r#static, &mut vec![class.span.start])
    }

    /// The class whose instance, or with `true` whose constructor, a member is read from. A type
    /// assertion gives the asserted type.
    fn receiver(&self, object: &'a Expression<'a>) -> Option<(&'a Class<'a>, bool)> {
        match object {
            Expression::ParenthesizedExpression(inner) => return self.receiver(&inner.expression),
            Expression::TSSatisfiesExpression(inner) => return self.receiver(&inner.expression),
            Expression::TSNonNullExpression(inner) => return self.receiver(&inner.expression),
            Expression::AwaitExpression(inner) => return self.receiver(&inner.argument),
            Expression::TSAsExpression(inner) => {
                return self.instance_type(&inner.type_annotation);
            }
            Expression::TSTypeAssertion(inner) => {
                return self.instance_type(&inner.type_annotation);
            }
            _ => {}
        }

        match super::form(object) {
            super::Form::Other(Expression::ThisExpression(this)) => {
                self.this_owner(this.node_id.get())
            }
            super::Form::Other(Expression::Super(base)) => {
                let (class, r#static) = self.this_owner(base.node_id.get())?;
                Some((
                    self.constructor(&class.heritage.as_ref()?.expression)?,
                    r#static,
                ))
            }
            super::Form::Other(Expression::NewExpression(construction)) => {
                Some((self.constructor(&construction.callee)?, false))
            }
            super::Form::Other(Expression::ClassExpression(class)) => Some((class, true)),
            super::Form::Other(Expression::Identifier(identifier)) => {
                let (key, declaration) = self.named(identifier)?;
                self.held(key, &declaration)
            }
            super::Form::Static(member) => {
                let (class, r#static) = self.receiver(&member.object)?;
                let (key, declaration) = self.class_member(
                    class,
                    member.property.name.as_str(),
                    r#static,
                    &mut vec![class.span.start],
                )?;
                self.held(key, &declaration)
            }
            _ => None,
        }
    }

    /// The class a constructor value is, read as a receiver so that a type assertion decides.
    fn constructor(&self, expression: &'a Expression<'a>) -> Option<&'a Class<'a>> {
        match self.receiver(expression)? {
            (class, true) => Some(class),
            (_, false) => None,
        }
    }

    /// The class a declared name or member holds: the class itself, or an instance of the class it
    /// is annotated with, or what its initializer holds. Each declaration is read once, and one
    /// reached again while it is read holds nothing.
    fn held(&self, key: Key, declaration: &Declaration<'a>) -> Option<(&'a Class<'a>, bool)> {
        match self.holds.borrow().get(&key) {
            Some(Holding::Reading) => return None,
            Some(Holding::Read(held)) => return *held,
            None => {}
        }

        self.holds.borrow_mut().insert(key, Holding::Reading);
        let held = self.hold(declaration);
        self.holds.borrow_mut().insert(key, Holding::Read(held));

        held
    }

    fn hold(&self, declaration: &Declaration<'a>) -> Option<(&'a Class<'a>, bool)> {
        self.readings.set(self.readings.get() + 1);
        let (annotation, initializer) = match declaration {
            Declaration::Class(class) => return Some((*class, true)),
            Declaration::TypeAlias(alias) => return self.instance_type(&alias.type_annotation),
            Declaration::Variable { declarator, .. } => (
                declarator.type_annotation.as_deref(),
                declarator.init.as_ref(),
            ),
            Declaration::Property(property) => {
                (property.type_annotation.as_deref(), property.value.as_ref())
            }
            Declaration::Parameter(parameter) => (
                parameter.type_annotation.as_deref(),
                parameter.initializer.as_deref(),
            ),
            _ => return None,
        };

        if let Some(annotation) = annotation {
            return self.instance_type(&annotation.type_annotation);
        }

        self.receiver(initializer?)
    }

    /// The class of the file a type names an instance of, or with `true` the class itself: a class,
    /// `typeof` a class, a local alias of one, or one with `undefined` or `null`, which the compiler
    /// drops without strict null checks.
    fn instance_type(&self, annotation: &TSType<'a>) -> Option<Held<'a>> {
        match annotation {
            // An alias is read once, and one reached again while it is read names nothing.
            TSType::TSTypeReference(reference) => {
                let (key, declaration) = self.type_named(&reference.type_name)?;

                match &declaration {
                    Declaration::Class(class) => Some((*class, false)),
                    Declaration::TypeAlias(_) => self.held(key, &declaration),
                    _ => None,
                }
            }
            TSType::TSParenthesizedType(inner) => self.instance_type(&inner.type_annotation),
            // `typeof` a class is the class itself; `typeof` a value is what the value holds.
            TSType::TSTypeQuery(query) => match &query.expr_name {
                TSTypeQueryExprName::IdentifierReference(name) => {
                    let (key, declaration) = self.named(name)?;
                    self.held(key, &declaration)
                }
                _ => None,
            },
            TSType::TSUnionType(union) => {
                let mut members = union.types.iter().filter(|member| {
                    !matches!(
                        member,
                        TSType::TSUndefinedKeyword(_) | TSType::TSNullKeyword(_)
                    )
                });

                match (members.next(), members.next()) {
                    (Some(member), None) => self.instance_type(member),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The class `this` denotes at a node, with `true` in its static members.
    fn this_owner(&self, node: NodeId) -> Option<(&'a Class<'a>, bool)> {
        let nodes = self.file.semantic().nodes();

        for ancestor in nodes.ancestor_ids(node) {
            let r#static = match nodes.kind(ancestor) {
                AstKind::Function(function) => {
                    // An explicit `this` parameter types `this` in the function.
                    if let Some(annotation) = function
                        .this_param
                        .as_ref()
                        .and_then(|this| this.type_annotation.as_ref())
                    {
                        return self.instance_type(&annotation.type_annotation);
                    }

                    match nodes.parent_kind(ancestor) {
                        AstKind::MethodDefinition(method) => method.r#static,
                        _ => return None,
                    }
                }
                AstKind::PropertyDefinition(property) => property.r#static,
                AstKind::AccessorProperty(property) => property.r#static,
                AstKind::StaticBlock(_) => true,
                // A decorator is evaluated outside the class it decorates, in the scope around it.
                AstKind::Decorator(_) => {
                    let class = nodes
                        .ancestor_ids(ancestor)
                        .find(|class| matches!(nodes.kind(*class), AstKind::Class(_)))?;
                    return self.this_owner(class);
                }
                AstKind::Class(_) | AstKind::Program(_) => return None,
                _ => continue,
            };

            return nodes
                .ancestor_ids(ancestor)
                .find_map(|class| match nodes.kind(class) {
                    AstKind::Class(class) => Some((class, r#static)),
                    _ => None,
                });
        }

        None
    }

    /// The member `name` of a class, inherited from a base class of the file when it has none;
    /// `seen` holds the classes already searched.
    fn class_member(
        &self,
        class: &'a Class<'a>,
        name: &str,
        r#static: bool,
        seen: &mut Vec<u32>,
    ) -> Option<(Key, Declaration<'a>)> {
        for element in &class.body.body {
            match element {
                ClassElement::PropertyDefinition(property)
                    if property.r#static == r#static
                        && key_name(&property.key, property.computed).as_deref() == Some(name) =>
                {
                    return Some((
                        Key::Node(property.node_id.get()),
                        Declaration::Property(property),
                    ));
                }
                ClassElement::AccessorProperty(property)
                    if property.r#static == r#static
                        && key_name(&property.key, property.computed).as_deref() == Some(name) =>
                {
                    return Some((Key::Node(property.node_id.get()), Declaration::Other));
                }
                ClassElement::MethodDefinition(method)
                    if method.kind == MethodDefinitionKind::Constructor && !r#static =>
                {
                    // A constructor parameter property declares an instance member.
                    for parameter in &method.value.params.items {
                        if let BindingPattern::BindingIdentifier(binding) = &parameter.pattern
                            && (parameter.accessibility.is_some()
                                || parameter.readonly
                                || parameter.r#override)
                            && binding.name == name
                        {
                            return Some((
                                Key::Node(parameter.node_id.get()),
                                Declaration::Parameter(parameter),
                            ));
                        }
                    }
                }
                ClassElement::MethodDefinition(method)
                    if method.r#static == r#static
                        && key_name(&method.key, method.computed).as_deref() == Some(name) =>
                {
                    return Some((Key::Node(method.node_id.get()), Declaration::Other));
                }
                _ => {}
            }
        }

        let parent = self.constructor(&class.heritage.as_ref()?.expression)?;

        // A base class reached again is a heritage cycle, which binds nothing.
        if seen.contains(&parent.span.start) {
            return None;
        }

        seen.push(class.span.start);
        self.class_member(parent, name, r#static, seen)
    }
}

/// A property name as the compiler reads it: an identifier, string or numeric literal, or a computed
/// string literal.
pub fn key_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) if !computed => Some(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::NumericLiteral(literal) if !computed => Some(literal.value.to_string()),
        PropertyKey::TemplateLiteral(_) if computed => key.as_expression().and_then(string_like),
        _ => None,
    }
}

/// The binding of `symbol` in a destructuring pattern.
fn element<'a>(
    pattern: &'a BindingPattern<'a>,
    symbol: SymbolId,
    owner: Owner<'a>,
) -> Declaration<'a> {
    match links(pattern, symbol) {
        Some(links) => Declaration::Element(Element { links, owner }),
        None => Declaration::Other,
    }
}

/// The pattern elements from `pattern` to the binding of `symbol`.
fn links<'a>(pattern: &'a BindingPattern<'a>, symbol: SymbolId) -> Option<Vec<Link<'a>>> {
    let descend = |value: &'a BindingPattern<'a>, key: Option<String>, object: bool, rest: bool| {
        let (inner, default) = match value {
            BindingPattern::AssignmentPattern(assignment) => {
                (&assignment.left, Some(&assignment.right))
            }
            other => (other, None),
        };
        let mut found = links(inner, symbol)?;
        found.insert(
            0,
            Link {
                key,
                object,
                rest,
                default,
            },
        );
        Some(found)
    };

    match pattern {
        BindingPattern::BindingIdentifier(binding) => {
            (binding.symbol_id.get() == Some(symbol)).then(Vec::new)
        }
        BindingPattern::AssignmentPattern(assignment) => links(&assignment.left, symbol),
        BindingPattern::ObjectPattern(object) => object
            .properties
            .iter()
            .find_map(|property| {
                descend(
                    &property.value,
                    key_name(&property.key, property.computed),
                    true,
                    false,
                )
            })
            .or_else(|| {
                object
                    .rest
                    .as_ref()
                    .and_then(|rest| descend(&rest.argument, None, true, true))
            }),
        BindingPattern::ArrayPattern(array) => array
            .elements
            .iter()
            .flatten()
            .find_map(|element| descend(element, None, false, false))
            .or_else(|| {
                array
                    .rest
                    .as_ref()
                    .and_then(|rest| descend(&rest.argument, None, false, true))
            }),
    }
}
