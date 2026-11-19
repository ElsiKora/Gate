//! Source contexts (the top-level bindings of each source file) and the owner of a declaration.
//!
//! Every tracked `.ts` file below a source root (not `.d.ts`) gets a context of its top-level imports,
//! namespace imports, variables and interface or type alias declarations. Names resolve by name
//! alone, without scopes, and a declaration's owner is found by chasing exports: a top-level class, enum, interface or type alias of the name counts
//! whether exported or not, a type alias of a plain type reference is followed, the first named
//! export of the name decides, and a star export decides only when exactly one owner comes back.
//! This is deliberately simpler than TypeScript's name resolution.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::SourceFile;
use crate::oxc_ast::ast::{
    BindingPattern, CallExpression, ChainElement, Class, Declaration, ExportDefaultDeclarationKind,
    Expression, ImportDeclaration, ImportDeclarationSpecifier, ModuleExportName, Statement,
    TSInterfaceDeclaration, TSType, TSTypeAliasDeclaration, TSTypeName, VariableDeclarationKind,
    VariableDeclarator,
};
use crate::oxc_span::{GetSpan, Span};
use crate::{Error, Result, TypeScript};

/// A named import of one context.
#[derive(Clone, Debug)]
pub struct Import {
    /// The name imported from the module (`default` for a default import).
    pub imported: String,
    /// The module specifier as written.
    pub module: String,
    /// The tracked file the specifier resolves to; `None` outside the source.
    pub source: Option<String>,
}

/// A namespace import of one context.
#[derive(Clone, Debug)]
pub struct Namespace {
    /// The module specifier as written.
    pub module: String,
    /// The tracked file the specifier resolves to; `None` outside the source.
    pub source: Option<String>,
}

/// A top-level interface or type alias declaration.
#[derive(Clone, Copy, Debug)]
pub enum TypeDeclaration<'a> {
    /// An `interface` declaration.
    Interface(&'a TSInterfaceDeclaration<'a>),
    /// A `type` alias declaration.
    Alias(&'a TSTypeAliasDeclaration<'a>),
}

/// What an expression or type name denotes: its local text, the module it is imported from (none
/// for a name of the file itself), the exported name and the file it resolves to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identity {
    /// The name as written in the file.
    pub local: String,
    /// The module it is imported from; `None` for a name of the file itself.
    pub module: Option<String>,
    /// The exported name it denotes.
    pub name: String,
    /// The tracked file it resolves to; `None` when it resolves outside the source.
    pub source: Option<String>,
}

/// The source owner of an identity; `path` is `None` for an owner outside the contexts.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Owner {
    /// The declared name.
    pub name: String,
    /// The file that declares it; `None` outside the contexts.
    pub path: Option<String>,
}

/// How the statement of a top-level variable declares it.
#[derive(Clone, Copy, Debug, Default)]
pub struct VariableFlags {
    /// The statement is exported.
    pub exported: bool,
    /// The statement declares a `const`.
    pub constant: bool,
}

/// One file's top-level bindings.
#[derive(Debug)]
pub struct SourceContext<'a> {
    /// The file's path.
    pub path: String,
    /// The parsed file.
    pub file: Rc<SourceFile<'a>>,
    /// Each named or default import, by its local name.
    pub imports: HashMap<String, Import>,
    /// Each namespace import (`import * as X`), by its local name.
    pub namespaces: HashMap<String, Namespace>,
    /// Every top-level variable declarator with an identifier name; a later one replaces an
    /// earlier one of the same name.
    pub variables: HashMap<String, &'a VariableDeclarator<'a>>,
    /// Whether the statement of each name's variable is exported and declares a `const`.
    pub variable_flags: HashMap<String, VariableFlags>,
    /// Each top-level interface or type alias, by name.
    pub types: HashMap<String, TypeDeclaration<'a>>,
    /// The resolved path of each module specifier of an import or export declaration.
    specifiers: HashMap<String, Option<String>>,
}

/// The contexts of every source file, in path order.
#[derive(Debug)]
pub struct Sources<'a> {
    contexts: BTreeMap<String, SourceContext<'a>>,
    /// Listed files oxc cannot parse, which `typescript/parse` reports.
    unparsed: BTreeSet<String>,
    /// The answers of [`Self::owner`] so far, by source and name.
    owners: RefCell<HashMap<(String, String), Option<Owner>>>,
}

/// The top-level statements of a file.
pub fn top_level<'a>(file: &SourceFile<'a>) -> std::slice::Iter<'a, Statement<'a>> {
    file.program().body.iter()
}

/// The top-level class declaration of a statement, exported, default-exported or not.
#[must_use]
pub fn class_declaration<'s, 'a>(statement: &'s Statement<'a>) -> Option<&'s Class<'a>> {
    match statement {
        Statement::ClassDeclaration(class) => Some(class),
        Statement::ExportDeclaration(export) => match &export.declaration {
            Declaration::ClassDeclaration(class) => Some(class),
            _ => None,
        },
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(class) => Some(class),
            _ => None,
        },
        _ => None,
    }
}

/// The first top-level class declaration named `name`.
pub fn find_class<'a>(file: &SourceFile<'a>, name: &str) -> Option<&'a Class<'a>> {
    top_level(file)
        .filter_map(class_declaration)
        .find(|class| class.id.as_ref().is_some_and(|id| id.name == name))
}

/// Whether a top-level variable statement is exported and declares a `const`.
fn variable_flags(statement: &Statement<'_>) -> VariableFlags {
    match statement {
        Statement::VariableDeclaration(list) => VariableFlags {
            exported: false,
            constant: list.kind == VariableDeclarationKind::Const,
        },
        Statement::ExportDeclaration(export) => match &export.declaration {
            Declaration::VariableDeclaration(list) => VariableFlags {
                exported: true,
                constant: list.kind == VariableDeclarationKind::Const,
            },
            _ => VariableFlags::default(),
        },
        _ => VariableFlags::default(),
    }
}

/// The variable declarators of a top-level statement, exported or not.
pub fn variable_declarators<'a>(
    statement: &'a Statement<'a>,
) -> impl Iterator<Item = &'a VariableDeclarator<'a>> {
    let list = match statement {
        Statement::VariableDeclaration(list) => Some(list.as_ref()),
        Statement::ExportDeclaration(export) => match &export.declaration {
            Declaration::VariableDeclaration(list) => Some(list.as_ref()),
            _ => None,
        },
        _ => None,
    };

    list.into_iter().flat_map(|list| list.declarations.iter())
}

/// The name a variable declarator binds, when its target is a plain identifier.
pub fn identifier_name<'d>(declarator: &'d VariableDeclarator<'_>) -> Option<&'d str> {
    match &declarator.id {
        BindingPattern::BindingIdentifier(identifier) => Some(identifier.name.as_str()),
        _ => None,
    }
}

/// The text of an exported or imported module name.
pub fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.to_string(),
        ModuleExportName::IdentifierReference(name) => name.name.to_string(),
        ModuleExportName::StringLiteral(name) => name.value.to_string(),
    }
}

/// The call an expression is, also inside an optional chain: TypeScript keeps `a?.b()` a call.
#[must_use]
pub fn call_expression<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e CallExpression<'a>> {
    match expression {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// Through `as`, `!`, parentheses, `satisfies` and type assertions, also a `!` that ends an
/// optional chain, which TypeScript keeps an assertion around the chain.
#[must_use]
pub fn unwrap<'e, 'a>(expression: &'e Expression<'a>) -> &'e Expression<'a> {
    match expression {
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::TSNonNullExpression(inner) => unwrap(&inner.expression),
            _ => expression,
        },
        Expression::TSAsExpression(inner) => unwrap(&inner.expression),
        Expression::TSNonNullExpression(inner) => unwrap(&inner.expression),
        Expression::ParenthesizedExpression(inner) => unwrap(&inner.expression),
        Expression::TSSatisfiesExpression(inner) => unwrap(&inner.expression),
        Expression::TSTypeAssertion(inner) => unwrap(&inner.expression),
        _ => expression,
    }
}

/// The object and name of a property access `object.name`, also inside an optional chain, which
/// TypeScript keeps a property access.
#[must_use]
pub fn property_access<'e, 'a>(
    expression: &'e Expression<'a>,
) -> Option<(&'e Expression<'a>, &'e str)> {
    match expression {
        Expression::StaticMemberExpression(member) => {
            Some((&member.object, member.property.name.as_str()))
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => {
                Some((&member.object, member.property.name.as_str()))
            }
            _ => None,
        },
        _ => None,
    }
}

impl<'a> SourceContext<'a> {
    fn new(typescript: &TypeScript<'a>, path: &str, file: &Rc<SourceFile<'a>>) -> Result<Self> {
        let mut source = Self {
            path: path.to_owned(),
            file: Rc::clone(file),
            imports: HashMap::new(),
            namespaces: HashMap::new(),
            variables: HashMap::new(),
            variable_flags: HashMap::new(),
            types: HashMap::new(),
            specifiers: HashMap::new(),
        };

        for statement in top_level(file) {
            if let Some(module) = module_specifier(statement)
                && !source.specifiers.contains_key(module)
            {
                let resolved = typescript.resolve(path, module)?;
                source.specifiers.insert(module.to_owned(), resolved);
            }

            if let Statement::ImportDeclaration(import) = statement {
                source.bind(import);
            }

            let flags = variable_flags(statement);

            for declarator in variable_declarators(statement) {
                if let Some(name) = identifier_name(declarator) {
                    source.variables.insert(name.to_owned(), declarator);
                    source.variable_flags.insert(name.to_owned(), flags);
                }
            }

            if let Some((name, declaration)) = type_declaration(statement) {
                source.types.insert(name.to_owned(), declaration);
            }
        }

        Ok(source)
    }

    /// Records the bindings of an import declaration with a clause.
    fn bind(&mut self, import: &ImportDeclaration<'_>) {
        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let module = import.source.value.to_string();
        let source = self.specifiers.get(&module).cloned().flatten();

        for specifier in specifiers {
            match specifier {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                    self.imports.insert(
                        default.local.name.to_string(),
                        Import {
                            imported: "default".to_owned(),
                            module: module.clone(),
                            source: source.clone(),
                        },
                    );
                }
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    self.imports.insert(
                        named.local.name.to_string(),
                        Import {
                            imported: export_name(&named.imported),
                            module: module.clone(),
                            source: source.clone(),
                        },
                    );
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                    self.namespaces.insert(
                        namespace.local.name.to_string(),
                        Namespace {
                            module: module.clone(),
                            source: source.clone(),
                        },
                    );
                }
            }
        }
    }

    /// The source text of a node.
    #[must_use]
    pub fn text(&self, span: Span) -> &str {
        &self.file.source()[span.start as usize..span.end as usize]
    }

    /// What an expression denotes through this file's imports; `unwrap_first` takes off assertions
    /// and parentheses first.
    #[must_use]
    pub fn identity(&self, expression: &Expression<'_>, unwrap_first: bool) -> Option<Identity> {
        let current = if unwrap_first {
            unwrap(expression)
        } else {
            expression
        };

        if let Expression::Identifier(identifier) = current {
            return Some(self.name_identity(&identifier.name));
        }

        let (object, name) = property_access(current)?;

        self.member_identity(object, name, current.span())
    }

    /// What a property access `object.name` at `span` denotes: a member of a namespace import.
    #[must_use]
    pub fn member_identity(
        &self,
        object: &Expression<'_>,
        name: &str,
        span: Span,
    ) -> Option<Identity> {
        let Expression::Identifier(object) = object else {
            return None;
        };
        let namespace = self.namespaces.get(object.name.as_str())?;

        Some(Identity {
            local: self.text(span).to_owned(),
            module: Some(namespace.module.clone()),
            name: name.to_owned(),
            source: namespace.source.clone(),
        })
    }

    /// What an identifier with this text denotes: its import, or a declaration of this file, as
    /// by its text alone, without scopes.
    #[must_use]
    pub fn name_identity(&self, name: &str) -> Identity {
        let local = name.to_owned();

        match self.imports.get(&local) {
            Some(import) => Identity {
                local,
                module: Some(import.module.clone()),
                name: import.imported.clone(),
                source: import.source.clone(),
            },
            None => Identity {
                name: local.clone(),
                local,
                module: None,
                source: Some(self.path.clone()),
            },
        }
    }

    /// The resolved path of a module specifier of this file's import or re-export declarations.
    #[must_use]
    pub fn specifier(&self, module: &str) -> Option<String> {
        self.specifiers.get(module).cloned().flatten()
    }

    /// What a type reference names.
    #[must_use]
    pub fn type_identity(&self, name: &TSTypeName<'_>) -> Option<Identity> {
        match name {
            TSTypeName::IdentifierReference(identifier) => {
                let local = identifier.name.to_string();

                Some(match self.imports.get(&local) {
                    Some(import) => Identity {
                        local,
                        module: Some(import.module.clone()),
                        name: import.imported.clone(),
                        source: import.source.clone(),
                    },
                    None => Identity {
                        name: local.clone(),
                        local,
                        module: None,
                        source: Some(self.path.clone()),
                    },
                })
            }
            TSTypeName::QualifiedName(qualified) => {
                let TSTypeName::IdentifierReference(left) = &qualified.left else {
                    return None;
                };
                let namespace = self.namespaces.get(left.name.as_str())?;

                Some(Identity {
                    local: self.text(qualified.span).to_owned(),
                    module: Some(namespace.module.clone()),
                    name: qualified.right.name.to_string(),
                    source: namespace.source.clone(),
                })
            }
            TSTypeName::ThisExpression(_) => None,
        }
    }
}

impl<'a> Sources<'a> {
    /// The contexts of every tracked `.ts` file below the directory `root` (relative to the
    /// project, without a final `/`; a rule option of the plugin that lists them) other than
    /// `.d.ts` files, outside the directories of [`crate::Settings::skip_folders`]; a file oxc
    /// cannot parse has no context, as `typescript/parse` reports it.
    ///
    /// # Errors
    /// Fails when a file cannot be read or a module cannot be resolved.
    pub fn new(typescript: &TypeScript<'a>, root: &str) -> Result<Self> {
        let root = root.trim_end_matches('/');
        let prefix = if root.is_empty() {
            String::new()
        } else {
            format!("{root}/")
        };
        let settings = typescript.settings();
        let mut contexts = BTreeMap::new();
        let mut unparsed = BTreeSet::new();

        for path in typescript.source().paths().filter(|path| {
            // The extension compares case-sensitively: `App.TS` is not a source.
            path.starts_with(&prefix)
                && std::path::Path::new(path).extension() == Some(std::ffi::OsStr::new("ts"))
                && !path.ends_with(".d.ts")
                && !settings.skips(path)
        }) {
            let file = typescript
                .parse(path)?
                .ok_or_else(|| Error::new(format!("{path} is listed but not tracked")))?;

            if file.fatal() {
                unparsed.insert(path.to_owned());
                continue;
            }

            contexts.insert(
                path.to_owned(),
                SourceContext::new(typescript, path, &file)?,
            );
        }

        Ok(Self {
            contexts,
            unparsed,
            owners: RefCell::default(),
        })
    }

    /// The context of the file at `path`; `None` when it is not listed or does not parse.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&SourceContext<'a>> {
        self.contexts.get(path)
    }

    /// Whether `path` is a listed file without a context because oxc cannot parse it.
    #[must_use]
    pub fn unparsed(&self, path: &str) -> bool {
        self.unparsed.contains(path)
    }

    /// The contexts in path order.
    pub fn iter(&self) -> impl Iterator<Item = &SourceContext<'a>> {
        self.contexts.values()
    }

    /// The owner of `name` in the file at `source`, found by chasing exports as this module describes; each source
    /// and name is resolved once per run.
    #[must_use]
    pub fn owner(&self, source: Option<&str>, name: &str) -> Option<Owner> {
        let key = (source?.to_owned(), name.to_owned());

        if let Some(owner) = self.owners.borrow().get(&key) {
            return owner.clone();
        }

        let owner = self.chase(&key.0, name, &HashSet::new());
        self.owners.borrow_mut().insert(key, owner.clone());
        owner
    }

    /// The canonical identity: its source owner, or the bare name when it has none.
    #[must_use]
    pub fn canonical(&self, identity: Option<&Identity>) -> Option<Owner> {
        let identity = identity?;

        Some(
            self.owner(identity.source.as_deref(), &identity.name)
                .unwrap_or_else(|| Owner {
                    name: identity.name.clone(),
                    path: None,
                }),
        )
    }

    fn chase(&self, path: &str, name: &str, visited: &HashSet<String>) -> Option<Owner> {
        let key = format!("{path}:{name}");

        if visited.contains(&key) {
            return None;
        }

        let context = self.contexts.get(path)?;
        let mut next = visited.clone();
        next.insert(key);
        let local = |local: &str| match context.imports.get(local) {
            Some(import) => import
                .source
                .as_deref()
                .and_then(|source| self.chase(source, &import.imported, &next)),
            None => self.chase(path, local, &next),
        };
        let here = || Owner {
            name: name.to_owned(),
            path: Some(path.to_owned()),
        };

        match declared(&context.file, name) {
            Some(Declared::Alias(target)) => return local(target).or_else(|| Some(here())),
            Some(Declared::Itself) => return Some(here()),
            None => {}
        }

        if name == "default" {
            if let Some(default) = default_declaration(&context.file) {
                return Some(Owner {
                    name: default.to_owned(),
                    path: Some(path.to_owned()),
                });
            }

            if let Some(target) = export_assignment(&context.file) {
                return local(target);
            }
        }

        let variable = top_level(&context.file)
            .flat_map(variable_declarators)
            .find(|declarator| identifier_name(declarator) == Some(name));

        if let Some(initializer) = variable.and_then(|declarator| declarator.init.as_ref()) {
            match initializer {
                Expression::Identifier(target) => {
                    return local(&target.name).or_else(|| Some(here()));
                }
                Expression::ClassExpression(class) => {
                    return Some(Owner {
                        name: class
                            .id
                            .as_ref()
                            .map_or_else(|| name.to_owned(), |id| id.name.to_string()),
                        path: Some(path.to_owned()),
                    });
                }
                Expression::FunctionExpression(function) => {
                    return Some(Owner {
                        name: function
                            .id
                            .as_ref()
                            .map_or_else(|| name.to_owned(), |id| id.name.to_string()),
                        path: Some(path.to_owned()),
                    });
                }
                _ => {}
            }
        }

        if context.imports.contains_key(name) {
            return local(name);
        }

        self.exports(context, name, &next)
    }

    /// The owner of `name` through the export declarations of a context: the first named export
    /// of the name decides, and star exports decide only when exactly one owner comes back.
    fn exports(
        &self,
        context: &SourceContext<'a>,
        name: &str,
        next: &HashSet<String>,
    ) -> Option<Owner> {
        let mut stars: Vec<Owner> = Vec::new();

        for statement in top_level(&context.file) {
            match statement {
                Statement::ExportNamedDeclaration(export) => {
                    if let Some(specifier) = export
                        .specifiers
                        .iter()
                        .find(|specifier| export_name(&specifier.exported) == name)
                    {
                        let local = export_name(&specifier.local);

                        return match context.imports.get(&local) {
                            Some(import) => import
                                .source
                                .as_deref()
                                .and_then(|source| self.chase(source, &import.imported, next)),
                            None => self.chase(&context.path, &local, next),
                        };
                    }
                }
                Statement::ExportFromDeclaration(export) => {
                    if let Some(specifier) = export
                        .specifiers
                        .iter()
                        .find(|specifier| export_name(&specifier.exported) == name)
                    {
                        let target = context
                            .specifiers
                            .get(export.source.value.as_str())
                            .cloned()
                            .flatten()?;

                        return self.chase(&target, &export_name(&specifier.local), next);
                    }
                }
                Statement::ExportAllDeclaration(export) if export.exported.is_none() => {
                    let owner = context
                        .specifiers
                        .get(export.source.value.as_str())
                        .cloned()
                        .flatten()
                        .and_then(|target| self.chase(&target, name, next));

                    if let Some(owner) = owner
                        && !stars.contains(&owner)
                    {
                        stars.push(owner);
                    }
                }
                _ => {}
            }
        }

        match stars.as_slice() {
            [owner] => Some(owner.clone()),
            _ => None,
        }
    }
}

/// How a top-level class, enum, interface or type alias declares a name.
enum Declared<'a> {
    Itself,
    /// A type alias of the plain type reference to this name.
    Alias(&'a str),
}

/// The top-level class, enum, interface or type alias named `name`.
fn declared<'a>(file: &SourceFile<'a>, name: &str) -> Option<Declared<'a>> {
    top_level(file).find_map(|statement| {
        let declaration = match statement {
            Statement::ExportDeclaration(export) => Some(&export.declaration),
            _ => None,
        };
        let (declared, alias) = match (statement, declaration) {
            (Statement::ClassDeclaration(class), _)
            | (_, Some(Declaration::ClassDeclaration(class))) => {
                (class.id.as_ref().map(|id| id.name.as_str()), None)
            }
            (Statement::TSEnumDeclaration(declaration), _)
            | (_, Some(Declaration::TSEnumDeclaration(declaration))) => {
                (Some(declaration.id.name.as_str()), None)
            }
            (Statement::TSInterfaceDeclaration(declaration), _)
            | (_, Some(Declaration::TSInterfaceDeclaration(declaration))) => {
                (Some(declaration.id.name.as_str()), None)
            }
            (Statement::TSTypeAliasDeclaration(declaration), _)
            | (_, Some(Declaration::TSTypeAliasDeclaration(declaration))) => {
                (Some(declaration.id.name.as_str()), Some(declaration))
            }
            (Statement::ExportDefaultDeclaration(export), _) => match &export.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    (class.id.as_ref().map(|id| id.name.as_str()), None)
                }
                ExportDefaultDeclarationKind::TSInterfaceDeclaration(declaration) => {
                    (Some(declaration.id.name.as_str()), None)
                }
                _ => (None, None),
            },
            _ => (None, None),
        };

        (declared == Some(name)).then(|| match alias.map(|alias| &alias.type_annotation) {
            Some(TSType::TSTypeReference(reference)) => match &reference.type_name {
                TSTypeName::IdentifierReference(target) => Declared::Alias(target.name.as_str()),
                _ => Declared::Itself,
            },
            _ => Declared::Itself,
        })
    })
}

/// The name of the first default-exported class, function or interface, `default` when it has
/// none.
fn default_declaration<'a>(file: &SourceFile<'a>) -> Option<&'a str> {
    top_level(file).find_map(|statement| {
        let Statement::ExportDefaultDeclaration(export) = statement else {
            return None;
        };

        match &export.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                Some(class.id.as_ref().map_or("default", |id| id.name.as_str()))
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => Some(
                function
                    .id
                    .as_ref()
                    .map_or("default", |id| id.name.as_str()),
            ),
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                Some(interface.id.name.as_str())
            }
            _ => None,
        }
    })
}

/// The identifier of the first `export default <expression>` or `export = <expression>`, when
/// its expression is an identifier.
fn export_assignment<'a>(file: &SourceFile<'a>) -> Option<&'a str> {
    let expression = top_level(file).find_map(|statement| match statement {
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(_)
            | ExportDefaultDeclarationKind::FunctionDeclaration(_)
            | ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => None,
            ExportDefaultDeclarationKind::Identifier(identifier) => Some(Some(identifier)),
            _ => Some(None),
        },
        Statement::TSExportAssignment(assignment) => Some(match &assignment.expression {
            Expression::Identifier(identifier) => Some(identifier),
            _ => None,
        }),
        _ => None,
    });

    expression
        .flatten()
        .map(|identifier| identifier.name.as_str())
}

/// The interface or type alias a top-level statement declares, exported or not.
fn type_declaration<'a>(statement: &'a Statement<'a>) -> Option<(&'a str, TypeDeclaration<'a>)> {
    match statement {
        Statement::TSInterfaceDeclaration(interface) => Some((
            interface.id.name.as_str(),
            TypeDeclaration::Interface(interface),
        )),
        Statement::TSTypeAliasDeclaration(alias) => {
            Some((alias.id.name.as_str(), TypeDeclaration::Alias(alias)))
        }
        Statement::ExportDeclaration(export) => match &export.declaration {
            Declaration::TSInterfaceDeclaration(interface) => Some((
                interface.id.name.as_str(),
                TypeDeclaration::Interface(interface),
            )),
            Declaration::TSTypeAliasDeclaration(alias) => {
                Some((alias.id.name.as_str(), TypeDeclaration::Alias(alias)))
            }
            _ => None,
        },
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => Some((
                interface.id.name.as_str(),
                TypeDeclaration::Interface(interface),
            )),
            _ => None,
        },
        _ => None,
    }
}

/// The module specifier of an import or re-export declaration.
fn module_specifier<'a>(statement: &'a Statement<'a>) -> Option<&'a str> {
    match statement {
        Statement::ImportDeclaration(import) => Some(import.source.value.as_str()),
        Statement::ExportFromDeclaration(export) => Some(export.source.value.as_str()),
        Statement::ExportAllDeclaration(export) => Some(export.source.value.as_str()),
        _ => None,
    }
}
