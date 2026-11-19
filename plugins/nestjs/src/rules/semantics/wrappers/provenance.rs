//! Where a value comes from, followed across the tracked files: through `const` bindings, top-level
//! object destructuring, property access with literal names, imports, named, default, namespace and
//! star re-exports.
//!
//! A value is unresolved when a step cannot be proven, missing when a module provably does not
//! export a name, or found: an expression with the property path still to read, or a value export
//! of one of the known [`Packages`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gate_sdk::Result;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, ChainElement, Declaration, ExportDefaultDeclarationKind, Expression,
    IdentifierReference, ImportOrExportKind, ModuleExportName, PropertyKey, Statement,
    VariableDeclarationKind,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::value::{string_like, unwrap};
use gate_typescript::{SourceFile, TypeScript};

use super::packages::{Export, Packages};
use crate::shared::project;
use crate::shared::syntax::typescript_sources;

/// What a reference resolves to.
#[derive(Clone, Debug)]
pub enum Resolution<'a> {
    Unresolved,
    Missing,
    Found(Value<'a>),
}

/// A resolved value.
#[derive(Clone, Debug)]
pub enum Value<'a> {
    /// A value export of one of the known [`Packages`].
    Package { module: String, name: String },
    /// An expression of a tracked file with the properties still to read from it, and the `const`
    /// declaration it was reached through, if any.
    Expression {
        file: Rc<SourceFile<'a>>,
        expression: &'a Expression<'a>,
        properties: Vec<String>,
        declaration: Option<(String, u32)>,
    },
}

impl Value<'_> {
    /// The identity compared to decide whether star re-exports agree.
    fn identity(&self) -> (String, u32, Vec<String>, Option<(String, u32)>) {
        match self {
            Self::Package { module, name } => (module.clone(), u32::MAX, vec![name.clone()], None),
            Self::Expression {
                file,
                expression,
                properties,
                declaration,
            } => (
                file.path().to_owned(),
                expression.span().start,
                properties.clone(),
                declaration.clone(),
            ),
        }
    }
}

/// A file, an export name and the properties read from it.
type ExportKey = (String, String, Vec<String>);

/// Resolves values through the parse cache and the module resolver of one run.
pub struct Provenance<'t, 'a> {
    typescript: &'t TypeScript<'a>,
    packages: &'t Packages,
    /// The source root with a trailing `/`: only its `.ts` sources are followed.
    sources: String,
    exports: RefCell<HashMap<ExportKey, Resolution<'a>>>,
}

impl<'t, 'a> Provenance<'t, 'a> {
    pub fn new(typescript: &'t TypeScript<'a>, packages: &'t Packages, source_root: &str) -> Self {
        Self {
            typescript,
            packages,
            sources: crate::layout::folder_prefix(source_root),
            exports: RefCell::new(HashMap::new()),
        }
    }

    /// What `expression` of `file` denotes, followed through `properties`.
    pub fn reference(
        &self,
        file: &Rc<SourceFile<'a>>,
        expression: &'a Expression<'a>,
        properties: &[String],
    ) -> Result<Resolution<'a>> {
        self.expression(file, expression, properties, &mut Vec::new())
    }

    /// The symbol an identifier reference of `file` resolves to; `None` for a global name.
    #[must_use]
    pub fn symbol(file: &SourceFile<'_>, identifier: &IdentifierReference<'_>) -> Option<SymbolId> {
        // References and symbols are set on the tree when the semantic model is built.
        let scoping = file.semantic().scoping();

        identifier
            .reference_id
            .get()
            .and_then(|reference| scoping.get_reference(reference).symbol_id())
    }

    fn expression(
        &self,
        file: &Rc<SourceFile<'a>>,
        expression: &'a Expression<'a>,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Resolution<'a>> {
        let value = unwrap(expression);
        let nested = |name: String| -> Vec<String> {
            std::iter::once(name)
                .chain(properties.iter().cloned())
                .collect()
        };

        match value {
            Expression::StaticMemberExpression(member) => self.expression(
                file,
                &member.object,
                &nested(member.property.name.to_string()),
                active,
            ),
            Expression::ComputedMemberExpression(member) => match string_like(&member.expression) {
                Some(name) => self.expression(file, &member.object, &nested(name), active),
                None => Ok(Resolution::Unresolved),
            },
            Expression::PrivateFieldExpression(member) => self.expression(
                file,
                &member.object,
                &nested(format!("#{}", member.field.name)),
                active,
            ),
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => self.expression(
                    file,
                    &member.object,
                    &nested(member.property.name.to_string()),
                    active,
                ),
                ChainElement::ComputedMemberExpression(member) => {
                    match string_like(&member.expression) {
                        Some(name) => self.expression(file, &member.object, &nested(name), active),
                        None => Ok(Resolution::Unresolved),
                    }
                }
                ChainElement::PrivateFieldExpression(member) => self.expression(
                    file,
                    &member.object,
                    &nested(format!("#{}", member.field.name)),
                    active,
                ),
                _ => Ok(Resolution::Found(Value::Expression {
                    file: file.clone(),
                    expression: value,
                    properties: properties.to_vec(),
                    declaration: None,
                })),
            },
            Expression::Identifier(identifier) => match Self::symbol(file, identifier) {
                Some(symbol) => self.binding(file, symbol, properties, active),
                None => Ok(Resolution::Unresolved),
            },
            _ => Ok(Resolution::Found(Value::Expression {
                file: file.clone(),
                expression: value,
                properties: properties.to_vec(),
                declaration: None,
            })),
        }
    }

    /// The value a binding of `file` denotes.
    fn binding(
        &self,
        file: &Rc<SourceFile<'a>>,
        symbol: SymbolId,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Resolution<'a>> {
        let key = format!("{}#symbol:{}", file.path(), symbol.index());

        if active.contains(&key) {
            return Ok(Resolution::Unresolved);
        }

        let semantic = file.semantic();
        let nodes = semantic.nodes();
        let declarations: Vec<_> = semantic.scoping().symbol_declarations(symbol).collect();
        let [declaration] = declarations.as_slice() else {
            return Ok(Resolution::Unresolved);
        };
        active.push(key);
        let resolution = match nodes.kind(*declaration) {
            AstKind::ImportSpecifier(specifier) => match nodes.parent_kind(*declaration) {
                AstKind::ImportDeclaration(import)
                    if specifier.import_kind != ImportOrExportKind::Type
                        && import.import_kind != ImportOrExportKind::Type =>
                {
                    self.imported(
                        file,
                        &import.source.value,
                        &export_name(&specifier.imported),
                        properties,
                        active,
                    )?
                }
                _ => Resolution::Unresolved,
            },
            kind @ (AstKind::ImportDefaultSpecifier(_) | AstKind::ImportNamespaceSpecifier(_)) => {
                match nodes.parent_kind(*declaration) {
                    AstKind::ImportDeclaration(import)
                        if import.import_kind != ImportOrExportKind::Type =>
                    {
                        let name = if matches!(kind, AstKind::ImportDefaultSpecifier(_)) {
                            "default"
                        } else {
                            "*"
                        };
                        self.imported(file, &import.source.value, name, properties, active)?
                    }
                    _ => Resolution::Unresolved,
                }
            }
            AstKind::VariableDeclarator(declarator) => {
                let constant = matches!(
                    nodes.parent_kind(*declaration),
                    AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                );

                match (&declarator.id, &declarator.init) {
                    (_, None) => Resolution::Unresolved,
                    _ if !constant => Resolution::Unresolved,
                    (BindingPattern::BindingIdentifier(binding), Some(init))
                        if binding.symbol_id.get() == Some(symbol) =>
                    {
                        match self.expression(file, init, properties, active)? {
                            Resolution::Found(Value::Expression {
                                file: found,
                                expression,
                                properties,
                                declaration: None,
                            }) => Resolution::Found(Value::Expression {
                                file: found,
                                expression,
                                properties,
                                declaration: Some((file.path().to_owned(), declarator.span.start)),
                            }),
                            other => other,
                        }
                    }
                    // A top-level property of an object destructuring without a default.
                    (BindingPattern::ObjectPattern(pattern), Some(init)) => {
                        let property =
                            pattern
                                .properties
                                .iter()
                                .find_map(|property| match &property.value {
                                    BindingPattern::BindingIdentifier(binding)
                                        if binding.symbol_id.get() == Some(symbol) =>
                                    {
                                        static_name(&property.key, property.computed)
                                    }
                                    _ => None,
                                });

                        match property {
                            Some(property) => {
                                let mut path = vec![property];
                                path.extend(properties.iter().cloned());
                                self.expression(file, init, &path, active)?
                            }
                            None => Resolution::Unresolved,
                        }
                    }
                    _ => Resolution::Unresolved,
                }
            }
            _ => Resolution::Unresolved,
        };
        active.pop();
        Ok(resolution)
    }

    /// The export `name` (`*` for the namespace itself) of the module `specifier` imports from
    /// `file`, followed through `properties`.
    fn imported(
        &self,
        file: &Rc<SourceFile<'a>>,
        specifier: &str,
        name: &str,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Resolution<'a>> {
        let (member, remaining) = if name == "*" {
            match properties.split_first() {
                Some((member, remaining)) => (member.as_str(), remaining),
                None => return Ok(Resolution::Unresolved),
            }
        } else {
            (name, properties)
        };

        match self.packages.export(specifier, member) {
            Export::Missing => return Ok(Resolution::Missing),
            Export::Found if remaining.is_empty() => {
                return Ok(Resolution::Found(Value::Package {
                    module: specifier.to_owned(),
                    name: member.to_owned(),
                }));
            }
            Export::Found => return Ok(Resolution::Unresolved),
            Export::Unknown => {}
        }

        // Only the `.ts` sources below the source root are followed.
        let Some(target) = self
            .typescript
            .resolve(file.path(), specifier)?
            .filter(|target| {
                target.starts_with(&self.sources)
                    && !typescript_sources(std::iter::once(target.as_str())).is_empty()
            })
        else {
            return Ok(Resolution::Unresolved);
        };
        let Some(target) = project::parsed(self.typescript, &target)? else {
            return Ok(Resolution::Unresolved);
        };

        self.export(&target, member, remaining, active)
    }

    /// The export `name` of `file`, followed through `properties`.
    fn export(
        &self,
        file: &Rc<SourceFile<'a>>,
        name: &str,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Resolution<'a>> {
        let key = format!("{}#export:{name}", file.path());

        if active.contains(&key) {
            return Ok(Resolution::Unresolved);
        }

        let cache = (file.path().to_owned(), name.to_owned(), properties.to_vec());

        if let Some(cached) = self.exports.borrow().get(&cache) {
            return Ok(cached.clone());
        }

        active.push(key);
        let resolution = self.find_export(file, name, properties, active);
        active.pop();
        let resolution = resolution?;

        if !matches!(resolution, Resolution::Unresolved) {
            self.exports.borrow_mut().insert(cache, resolution.clone());
        }

        Ok(resolution)
    }

    fn find_export(
        &self,
        file: &Rc<SourceFile<'a>>,
        name: &str,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Resolution<'a>> {
        // Symbols are set on the tree when the semantic model is built.
        let _ = file.semantic();
        let program = file.program();
        let mut stars = Vec::new();

        for statement in &program.body {
            match statement {
                Statement::ExportAllDeclaration(export)
                    if export.export_kind != ImportOrExportKind::Type =>
                {
                    match &export.exported {
                        Some(exported) if export_name(exported) == name => {
                            return self.imported(
                                file,
                                &export.source.value,
                                "*",
                                properties,
                                active,
                            );
                        }
                        Some(_) => {}
                        None => stars.push(export.source.value.to_string()),
                    }
                }
                Statement::ExportFromDeclaration(export)
                    if export.export_kind != ImportOrExportKind::Type =>
                {
                    if let Some(specifier) = export.specifiers.iter().find(|specifier| {
                        specifier.export_kind != ImportOrExportKind::Type
                            && export_name(&specifier.exported) == name
                    }) {
                        return self.imported(
                            file,
                            &export.source.value,
                            &export_name(&specifier.local),
                            properties,
                            active,
                        );
                    }
                }
                Statement::ExportNamedDeclaration(export)
                    if export.export_kind != ImportOrExportKind::Type =>
                {
                    if let Some(specifier) = export.specifiers.iter().find(|specifier| {
                        specifier.export_kind != ImportOrExportKind::Type
                            && export_name(&specifier.exported) == name
                    }) {
                        return match &specifier.local {
                            ModuleExportName::IdentifierReference(local) => {
                                match Self::symbol(file, local) {
                                    Some(symbol) => self.binding(file, symbol, properties, active),
                                    None => Ok(Resolution::Unresolved),
                                }
                            }
                            _ => Ok(Resolution::Unresolved),
                        };
                    }
                }
                Statement::ExportDefaultDeclaration(export) if name == "default" => {
                    match &export.declaration {
                        ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {}
                        ExportDefaultDeclarationKind::FunctionDeclaration(_)
                        | ExportDefaultDeclarationKind::ClassDeclaration(_) => {
                            return Ok(Resolution::Unresolved);
                        }
                        kind => {
                            if let Some(expression) = kind.as_expression() {
                                return self.expression(file, expression, properties, active);
                            }
                        }
                    }
                }
                Statement::ExportDeclaration(export) => {
                    if let Some(resolution) =
                        self.declared(file, &export.declaration, name, properties, active)?
                    {
                        return Ok(resolution);
                    }
                }
                _ => {}
            }
        }

        if name == "default" {
            return Ok(Resolution::Missing);
        }

        self.stars(file, &stars, name, properties, active)
    }

    /// The export `name` of an exported declaration, when it declares that name.
    fn declared(
        &self,
        file: &Rc<SourceFile<'a>>,
        declaration: &'a Declaration<'a>,
        name: &str,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Option<Resolution<'a>>> {
        Ok(match declaration {
            Declaration::VariableDeclaration(list) => {
                let mut bindings = Vec::new();

                for declarator in &list.declarations {
                    binding_symbols(&declarator.id, &mut bindings);
                }

                match bindings.into_iter().find(|(binding, _)| binding == name) {
                    Some((_, symbol)) => Some(self.binding(file, symbol, properties, active)?),
                    None => None,
                }
            }
            // A class, function, enum or namespace of a source file has no value to follow.
            Declaration::FunctionDeclaration(function)
                if function.id.as_ref().is_some_and(|id| id.name == name) =>
            {
                Some(Resolution::Unresolved)
            }
            Declaration::ClassDeclaration(class)
                if class.id.as_ref().is_some_and(|id| id.name == name) =>
            {
                Some(Resolution::Unresolved)
            }
            Declaration::TSEnumDeclaration(declaration) if declaration.id.name == name => {
                Some(Resolution::Unresolved)
            }
            Declaration::TSNamespaceDeclaration(declaration) if declaration.id.name == name => {
                Some(Resolution::Unresolved)
            }
            _ => None,
        })
    }

    /// The export `name` through the star re-exports of a file: unresolved when one of them is,
    /// missing when none exports it, and found only when every one that exports it agrees.
    fn stars(
        &self,
        file: &Rc<SourceFile<'a>>,
        stars: &[String],
        name: &str,
        properties: &[String],
        active: &mut Vec<String>,
    ) -> Result<Resolution<'a>> {
        let mut candidates: Vec<Value<'a>> = Vec::new();

        for star in stars {
            match self.imported(file, star, name, properties, active)? {
                Resolution::Unresolved => return Ok(Resolution::Unresolved),
                Resolution::Missing => {}
                Resolution::Found(value) => candidates.push(value),
            }
        }

        let Some(first) = candidates.first() else {
            return Ok(Resolution::Missing);
        };
        let identity = first.identity();

        Ok(
            if candidates
                .iter()
                .all(|candidate| candidate.identity() == identity)
            {
                Resolution::Found(first.clone())
            } else {
                Resolution::Unresolved
            },
        )
    }
}

/// The name of a module export.
pub fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.to_string(),
        ModuleExportName::IdentifierReference(name) => name.name.to_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.to_string(),
    }
}

/// A property name the compiler reads statically: an identifier or string, or a computed string.
pub fn static_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    if computed {
        return key.as_expression().and_then(string_like);
    }

    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::TemplateLiteral(_) => key.as_expression().and_then(string_like),
        _ => None,
    }
}

/// Every binding identifier of a pattern with its symbol.
pub fn binding_symbols(pattern: &BindingPattern<'_>, bindings: &mut Vec<(String, SymbolId)>) {
    match pattern {
        BindingPattern::BindingIdentifier(binding) => {
            if let Some(symbol) = binding.symbol_id.get() {
                bindings.push((binding.name.to_string(), symbol));
            }
        }
        BindingPattern::ObjectPattern(object) => {
            for property in &object.properties {
                binding_symbols(&property.value, bindings);
            }

            if let Some(rest) = &object.rest {
                binding_symbols(&rest.argument, bindings);
            }
        }
        BindingPattern::ArrayPattern(array) => {
            for element in array.elements.iter().flatten() {
                binding_symbols(element, bindings);
            }

            if let Some(rest) = &array.rest {
                binding_symbols(&rest.argument, bindings);
            }
        }
        BindingPattern::AssignmentPattern(assignment) => {
            binding_symbols(&assignment.left, bindings);
        }
    }
}
