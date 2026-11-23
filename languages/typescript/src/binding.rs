//! Which export of one package an expression denotes, within one file.
//!
//! `ImportBindings::new(file, "pkg", false)?.resolve(expression)` is `Some(["A", "b"])` when the
//! expression is `A.b` of `pkg`: through a named, namespace or `import =` import, `const` aliases and
//! object destructuring, property access, `require("pkg")` (also through `createRequire` of
//! `node:module`) and `await import("pkg")`. `ImportBindings::any(file, false)` resolves an
//! expression for whichever module it reaches, which `reached` then names.

use std::cell::RefCell;
use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, CallExpression, ChainElement, ComputedMemberExpression, Expression,
    ImportOrExportKind, ModuleExportName, PropertyKey, StaticMemberExpression, TSModuleReference,
    TSTypeName, VariableDeclarationKind,
};
use oxc_syntax::symbol::SymbolId;

use super::SourceFile;
use super::value::{string_like, unwrap};

/// Resolves expressions of one file to exports of one package, or of any module.
#[derive(Debug)]
pub struct ImportBindings<'s, 'a> {
    file: &'s SourceFile<'a>,
    /// The package, or `None` for whichever module an expression reaches.
    module: Option<&'s str>,
    type_only: bool,
    /// The module the last resolution reached.
    reached: RefCell<Option<String>>,
}

/// Whether a string literal of `file` may have the value `module`, read from the text alone. Without
/// a backslash in the text, the value of a string and the cooked value of a template are the text
/// between their delimiters, but for a template's line breaks, which `module` must not hold; JSX,
/// whose attribute strings oxc may decode, is always read. `false` only when no literal can name it.
fn may_name(file: &SourceFile<'_>, module: &str) -> bool {
    let source = file.source();

    source.contains(module)
        || source.contains('\\')
        || module.contains(['\r', '\n'])
        || file.program().source_type.is_jsx()
}

/// What an expression is with respect to `node:module`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Loader {
    /// The `node:module` namespace or default import.
    Module,
    /// `createRequire`.
    Factory,
    /// A `require` function.
    Require,
}

impl<'s, 'a> ImportBindings<'s, 'a> {
    /// The resolver, or `None` when no string literal of the file names `module`, so nothing in it
    /// can denote the package. `type_only` also follows type-only imports.
    #[must_use]
    pub fn new(file: &'s SourceFile<'a>, module: &'s str, type_only: bool) -> Option<Self> {
        if !may_name(file, module) {
            return None;
        }
        let named = file
            .semantic()
            .nodes()
            .iter()
            .any(|node| match node.kind() {
                AstKind::StringLiteral(literal) => literal.value == module,
                AstKind::TemplateLiteral(template) => {
                    template.expressions.is_empty()
                        && template
                            .quasis
                            .first()
                            .and_then(|quasi| quasi.value.cooked.as_ref())
                            .is_some_and(|cooked| cooked == module)
                }
                _ => false,
            });

        named.then_some(Self {
            file,
            module: Some(module),
            type_only,
            reached: RefCell::new(None),
        })
    }

    /// The resolver for whichever module an expression reaches: one walk answers what one resolver
    /// per loaded module would, as a binding ends at exactly one import.
    #[must_use]
    pub fn any(file: &'s SourceFile<'a>, type_only: bool) -> Self {
        Self {
            file,
            module: None,
            type_only,
            reached: RefCell::new(None),
        }
    }

    /// The module the last successful resolution reached.
    #[must_use]
    pub fn reached(&self) -> Option<String> {
        self.reached.borrow().clone()
    }

    /// Whether `module` is the package, recording it as the module reached.
    fn names(&self, module: &str) -> bool {
        if self.module.is_some_and(|expected| expected != module) {
            return false;
        }

        *self.reached.borrow_mut() = Some(module.to_owned());
        true
    }

    /// The export path the expression denotes, if it denotes one of the package.
    #[must_use]
    pub fn resolve(&self, expression: &Expression<'a>) -> Option<Vec<String>> {
        self.reached.replace(None);
        self.binding(expression, &mut HashSet::new())
    }

    /// The export path an identifier bound to `symbol` denotes.
    #[must_use]
    pub fn resolve_symbol(&self, symbol: SymbolId) -> Option<Vec<String>> {
        self.reached.replace(None);
        self.symbol_binding(symbol, &mut HashSet::from([symbol]))
    }

    /// The export path `a.b` denotes.
    #[must_use]
    pub fn resolve_static_member(
        &self,
        member: &StaticMemberExpression<'a>,
    ) -> Option<Vec<String>> {
        self.reached.replace(None);
        self.static_member(member, &mut HashSet::new())
    }

    /// The export path `a["b"]` denotes.
    #[must_use]
    pub fn resolve_computed_member(
        &self,
        member: &ComputedMemberExpression<'a>,
    ) -> Option<Vec<String>> {
        self.reached.replace(None);
        self.computed_member(member, &mut HashSet::new())
    }

    /// The export path a type name `A.b` denotes, through the same bindings as an expression.
    #[must_use]
    pub fn resolve_type_name(&self, name: &TSTypeName<'a>) -> Option<Vec<String>> {
        self.reached.replace(None);
        self.type_name(name, &mut HashSet::new())
    }

    fn type_name(
        &self,
        name: &TSTypeName<'a>,
        seen: &mut HashSet<SymbolId>,
    ) -> Option<Vec<String>> {
        match name {
            TSTypeName::IdentifierReference(identifier) => {
                let symbol = identifier.reference_id.get().and_then(|reference| {
                    self.file
                        .semantic()
                        .scoping()
                        .get_reference(reference)
                        .symbol_id()
                })?;

                if !seen.insert(symbol) {
                    return None;
                }

                self.symbol_binding(symbol, seen)
            }
            TSTypeName::QualifiedName(name) => {
                let mut path = self.type_name(&name.left, seen)?;
                path.push(name.right.name.to_string());
                Some(path)
            }
            TSTypeName::ThisExpression(_) => None,
        }
    }

    fn symbol_of(&self, expression: &Expression<'a>) -> Option<(SymbolId, &'a str)> {
        let Expression::Identifier(identifier) = expression else {
            return None;
        };
        let symbol = identifier.reference_id.get().and_then(|reference| {
            self.file
                .semantic()
                .scoping()
                .get_reference(reference)
                .symbol_id()
        })?;
        Some((symbol, identifier.name.as_str()))
    }

    fn binding(
        &self,
        expression: &Expression<'a>,
        seen: &mut HashSet<SymbolId>,
    ) -> Option<Vec<String>> {
        let value = unwrap(expression);

        match value {
            Expression::Identifier(_) => {
                let (symbol, _) = self.symbol_of(value)?;

                if !seen.insert(symbol) {
                    return None;
                }

                self.symbol_binding(symbol, seen)
            }
            Expression::StaticMemberExpression(member) => self.static_member(member, seen),
            Expression::ComputedMemberExpression(member) => self.computed_member(member, seen),
            Expression::CallExpression(call) => self.required(call),
            // TypeScript keeps `a?.b` a member access and `a?.()` a call.
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => self.static_member(member, seen),
                ChainElement::ComputedMemberExpression(member) => {
                    self.computed_member(member, seen)
                }
                ChainElement::CallExpression(call) => self.required(call),
                ChainElement::TSNonNullExpression(expression) => {
                    self.binding(&expression.expression, seen)
                }
                ChainElement::PrivateFieldExpression(_) => None,
            },
            Expression::AwaitExpression(awaited) => match &awaited.argument {
                Expression::ImportExpression(import)
                    if import.options.is_none()
                        && string_like(&import.source).is_some_and(|name| self.names(&name)) =>
                {
                    Some(Vec::new())
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn static_member(
        &self,
        member: &StaticMemberExpression<'a>,
        seen: &mut HashSet<SymbolId>,
    ) -> Option<Vec<String>> {
        let mut path = self.binding(&member.object, seen)?;
        path.push(member.property.name.to_string());
        Some(path)
    }

    fn computed_member(
        &self,
        member: &ComputedMemberExpression<'a>,
        seen: &mut HashSet<SymbolId>,
    ) -> Option<Vec<String>> {
        let name = string_like(&member.expression)?;
        let mut path = self.binding(&member.object, seen)?;
        path.push(name);
        Some(path)
    }

    /// `require("<module>")` through a detected loader: the module itself.
    fn required(&self, call: &CallExpression<'a>) -> Option<Vec<String>> {
        match call.arguments.as_slice() {
            [argument]
                if self.loader(&call.callee, &mut HashSet::new()) == Some(Loader::Require)
                    && argument
                        .as_expression()
                        .and_then(string_like)
                        .is_some_and(|name| self.names(&name)) =>
            {
                Some(Vec::new())
            }
            _ => None,
        }
    }

    fn symbol_binding(
        &self,
        symbol: SymbolId,
        seen: &mut HashSet<SymbolId>,
    ) -> Option<Vec<String>> {
        let semantic = self.file.semantic();
        let nodes = semantic.nodes();
        let declarations: Vec<_> = semantic.scoping().symbol_declarations(symbol).collect();
        let [declaration] = declarations.as_slice() else {
            return None;
        };

        match nodes.kind(*declaration) {
            AstKind::ImportSpecifier(specifier) => {
                let AstKind::ImportDeclaration(import) = nodes.parent_kind(*declaration) else {
                    return None;
                };
                let allowed = self.type_only
                    || (specifier.import_kind != ImportOrExportKind::Type
                        && import.import_kind != ImportOrExportKind::Type);

                (allowed && self.names(&import.source.value))
                    .then(|| vec![export_name(&specifier.imported)])
            }
            AstKind::ImportNamespaceSpecifier(_) => {
                let AstKind::ImportDeclaration(import) = nodes.parent_kind(*declaration) else {
                    return None;
                };
                let allowed = self.type_only || import.import_kind != ImportOrExportKind::Type;

                (allowed && self.names(&import.source.value)).then(Vec::new)
            }
            AstKind::TSImportEqualsDeclaration(import) => {
                let allowed = self.type_only || import.import_kind != ImportOrExportKind::Type;
                let TSModuleReference::ExternalModuleReference(reference) =
                    &import.module_reference
                else {
                    return None;
                };

                (allowed && self.names(&reference.expression.value)).then(Vec::new)
            }
            AstKind::VariableDeclarator(declarator) => {
                let constant = matches!(
                    nodes.parent_kind(*declaration),
                    AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                );
                let init = declarator.init.as_ref()?;

                if !constant {
                    return None;
                }

                match &declarator.id {
                    BindingPattern::BindingIdentifier(binding)
                        if binding.symbol_id.get() == Some(symbol) =>
                    {
                        self.binding(init, seen)
                    }
                    // `const { A, b: B } = pkg`: one level of an object pattern, without a rest
                    // element or a default value.
                    BindingPattern::ObjectPattern(pattern) => {
                        let property = pattern.properties.iter().find(|property| {
                            matches!(&property.value, BindingPattern::BindingIdentifier(binding) if binding.symbol_id.get() == Some(symbol))
                        })?;
                        let name = match &property.key {
                            PropertyKey::StaticIdentifier(name) if !property.computed => {
                                name.name.to_string()
                            }
                            PropertyKey::StringLiteral(literal) if !property.computed => {
                                literal.value.to_string()
                            }
                            _ => return None,
                        };
                        let mut path = self.binding(init, seen)?;
                        path.push(name);
                        Some(path)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// What `expression` is with respect to `node:module`, as the JavaScript module-loader
    /// resolver reads it.
    fn loader(&self, expression: &Expression<'a>, seen: &mut HashSet<SymbolId>) -> Option<Loader> {
        let value = unwrap(expression);

        match value {
            Expression::Identifier(identifier) => {
                let Some((symbol, _)) = self.symbol_of(value) else {
                    return (identifier.name == "require").then_some(Loader::Require);
                };

                if !seen.insert(symbol) {
                    return None;
                }

                let semantic = self.file.semantic();
                let nodes = semantic.nodes();
                let declaration = semantic.scoping().symbol_declarations(symbol).next()?;
                let node_module = |id| match nodes.parent_kind(id) {
                    AstKind::ImportDeclaration(import) => {
                        import.import_kind != ImportOrExportKind::Type
                            && matches!(import.source.value.as_str(), "node:module" | "module")
                    }
                    _ => false,
                };

                match nodes.kind(declaration) {
                    AstKind::ImportSpecifier(specifier)
                        if specifier.import_kind != ImportOrExportKind::Type
                            && export_name(&specifier.imported) == "createRequire"
                            && node_module(declaration) =>
                    {
                        Some(Loader::Factory)
                    }
                    AstKind::ImportNamespaceSpecifier(_) | AstKind::ImportDefaultSpecifier(_)
                        if node_module(declaration) =>
                    {
                        Some(Loader::Module)
                    }
                    AstKind::VariableDeclarator(declarator) => {
                        let constant = matches!(
                            nodes.parent_kind(declaration),
                            AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                        );

                        match (&declarator.id, &declarator.init) {
                            (BindingPattern::BindingIdentifier(_), Some(init)) if constant => {
                                self.loader(init, seen)
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            Expression::StaticMemberExpression(member)
                if member.property.name == "createRequire"
                    && self.loader(&member.object, seen) == Some(Loader::Module) =>
            {
                Some(Loader::Factory)
            }
            Expression::ComputedMemberExpression(member)
                if string_like(&member.expression).as_deref() == Some("createRequire")
                    && self.loader(&member.object, seen) == Some(Loader::Module) =>
            {
                Some(Loader::Factory)
            }
            Expression::CallExpression(call)
                if call.arguments.len() == 1
                    && self.loader(&call.callee, seen) == Some(Loader::Factory) =>
            {
                Some(Loader::Require)
            }
            // TypeScript keeps `m?.createRequire` a member access and `factory?.(url)` a call.
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => (member.property.name
                    == "createRequire"
                    && self.loader(&member.object, seen) == Some(Loader::Module))
                .then_some(Loader::Factory),
                ChainElement::ComputedMemberExpression(member) => {
                    (string_like(&member.expression).as_deref() == Some("createRequire")
                        && self.loader(&member.object, seen) == Some(Loader::Module))
                    .then_some(Loader::Factory)
                }
                ChainElement::CallExpression(call) => (call.arguments.len() == 1
                    && self.loader(&call.callee, seen) == Some(Loader::Factory))
                .then_some(Loader::Require),
                ChainElement::TSNonNullExpression(expression) => {
                    self.loader(&expression.expression, seen)
                }
                ChainElement::PrivateFieldExpression(_) => None,
            },
            _ => None,
        }
    }
}

/// Whether `callee` is a `require` function, as the JavaScript module-loader resolver reads it: the
/// library `require`, or a `createRequire` of `node:module` or `module` result, through `const`
/// aliases.
pub(super) fn is_require<'a>(file: &SourceFile<'a>, callee: &Expression<'a>) -> bool {
    let bindings = ImportBindings::any(file, false);

    bindings.loader(callee, &mut HashSet::new()) == Some(Loader::Require)
}

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.to_string(),
        ModuleExportName::IdentifierReference(name) => name.name.to_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.to_string(),
    }
}
