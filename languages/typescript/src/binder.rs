//! Name lookup within one file as the TypeScript binder performs it, on top of oxc's scopes.
//!
//! oxc resolves each reference to the nearest binding of a fitting kind. The TypeScript binder also
//! sees the exports of every block of a namespace, whether written as separate blocks, as a dotted
//! `namespace A.B` or nested in merged blocks, while a nested namespace that is not a member of its
//! outer namespace stays local to its block; shares the members and namespaces of all `declare
//! global` blocks and of all `declare module "m"` blocks of one name; merges every declaration of
//! one exported name across those blocks, whatever its meaning; treats every member of an ambient
//! block without `export {}`, `export {} from`, `export *` or `export =`, other than an
//! `import X = A` alias, as exported;
//! follows `import X = A` aliases, chained or repeated, to the namespace they name; looks up the
//! left side of `typeof N.E` as a value; skips a binding whose meaning does not fit; and finds
//! `declare global` declarations from anywhere in the file. A name that resolves to nothing is a
//! library name. Type information is not used, so `typeof N.E` where `N` is a value typed as a
//! namespace resolves to nothing.
//!
//! The rules are proven only for the forms the binder test and the fixtures of the checks using it
//! cover. Other forms can resolve differently from TypeScript, among them namespaces declared
//! outside the top level and namespace bodies, names declared with different kinds in different
//! blocks of one namespace, namespaces merged with a `const enum`, a variable or an `import` alias,
//! module augmentations, and values initialized from or typed as a namespace; a check that relies on
//! the binder decides how it treats them.

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{
    IdentifierReference, Statement, TSImportEqualsDeclaration, TSModuleBlock, TSModuleReference,
    TSNamespaceDeclarationBody, TSQualifiedName, TSTypeName,
};
use oxc_semantic::{AstNodes, Semantic};
use oxc_syntax::node::NodeId;
use oxc_syntax::scope::ScopeId;
use oxc_syntax::symbol::{SymbolFlags, SymbolId};

use super::SourceFile;

/// The declaration space a name is looked up in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Meaning {
    /// The type space: interfaces, type aliases, classes, enums.
    Type,
    /// The value space: variables, functions, classes, enums.
    Value,
    /// The namespace space: namespaces and enums.
    Namespace,
}

impl Meaning {
    /// Whether a symbol with `flags` can be the target; an import alias always can.
    #[must_use]
    pub fn accepts(self, flags: SymbolFlags) -> bool {
        let meaning = match self {
            Self::Type => SymbolFlags::Type,
            Self::Value => SymbolFlags::Value,
            Self::Namespace => SymbolFlags::Namespace,
        };
        flags.intersects(meaning | SymbolFlags::Import | SymbolFlags::TypeImport)
    }
}

/// A namespace path within the scope that declares its outermost name.
type Key = (ScopeId, Vec<String>);

/// The namespace structure of one file.
pub struct Binder<'s, 'a> {
    semantic: &'s Semantic<'a>,
    /// The key of each namespace declaration node.
    declarations: HashMap<NodeId, Key>,
    /// The key of each namespace block scope.
    keys: HashMap<ScopeId, Key>,
    /// The block scopes of each namespace path.
    blocks: HashMap<Key, Vec<ScopeId>>,
    /// The blocks whose members are exported without an `export` keyword.
    ambient: HashSet<ScopeId>,
    /// The scopes of `declare global` blocks.
    globals: Vec<ScopeId>,
    /// The first block of every `declare global` or `declare module "m"` block of the same name,
    /// which all declare into one scope.
    canonical: HashMap<ScopeId, ScopeId>,
}

impl std::fmt::Debug for Binder<'_, '_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Binder")
            .field("namespaces", &self.blocks.len())
            .field("globals", &self.globals.len())
            .finish_non_exhaustive()
    }
}

impl<'s, 'a> Binder<'s, 'a> {
    /// A binder of the declarations of `file`.
    #[must_use]
    pub fn new(file: &'s SourceFile<'a>) -> Self {
        let semantic = file.semantic();
        let nodes = semantic.nodes();
        let declaration_file = file.program().source_type.is_typescript_definition();
        let mut binder = Self {
            semantic,
            declarations: HashMap::new(),
            keys: HashMap::new(),
            blocks: HashMap::new(),
            ambient: HashSet::new(),
            globals: Vec::new(),
            canonical: HashMap::new(),
        };
        let mut modules: HashMap<&str, ScopeId> = HashMap::new();

        // `declare global` blocks merge into the global scope and `declare module "m"` blocks of
        // one name into that module, so their members and namespaces are shared across blocks.
        for node in nodes.iter() {
            let (scope, block, first) = match node.kind() {
                AstKind::TSGlobalDeclaration(global) => {
                    let Some(scope) = global.scope_id.get() else {
                        continue;
                    };

                    binder.globals.push(scope);
                    (scope, Some(&global.body), binder.globals[0])
                }
                AstKind::TSExternalModuleDeclaration(module) => {
                    let Some(scope) = module.scope_id.get() else {
                        continue;
                    };

                    (
                        scope,
                        module.body.as_deref(),
                        *modules.entry(module.id.value.as_str()).or_insert(scope),
                    )
                }
                _ => continue,
            };
            let key = (first, Vec::new());

            if !block.is_some_and(explicit) {
                binder.ambient.insert(scope);
            }

            binder.canonical.insert(scope, first);
            binder.keys.insert(scope, key.clone());
            binder.blocks.entry(key).or_default().push(scope);
        }

        for node in nodes.iter() {
            let AstKind::TSNamespaceDeclaration(namespace) = node.kind() else {
                continue;
            };
            let key = binder.key(node.id(), declaration_file);
            binder.declarations.insert(node.id(), key.clone());

            let Some(scope) = namespace.scope_id.get() else {
                continue;
            };

            if let TSNamespaceDeclarationBody::TSModuleBlock(block) = &namespace.body
                && ambient_context(nodes, node.id(), declaration_file)
                && !explicit(block)
            {
                binder.ambient.insert(scope);
            }

            // The outer part of a dotted `namespace A.B` has no block of its own, but its scope
            // holds the inner name.
            binder.keys.insert(scope, key.clone());
            binder.blocks.entry(key).or_default().push(scope);
        }

        binder
    }

    /// The symbol `name` denotes in `meaning`, or `None` for a library name.
    #[must_use]
    pub fn resolve(&self, name: &IdentifierReference<'a>, meaning: Meaning) -> Option<SymbolId> {
        let scoping = self.semantic.scoping();
        let resolved = name
            .reference_id
            .get()
            .and_then(|reference| scoping.get_reference(reference).symbol_id());
        let start = self
            .semantic
            .nodes()
            .get_node(name.node_id.get())
            .scope_id();

        match resolved {
            Some(symbol) if meaning.accepts(scoping.symbol_flags(symbol)) => {
                let target = scoping.symbol_scope_id(symbol);

                for scope in scoping.scope_ancestors(start) {
                    if scope == target {
                        return Some(symbol);
                    }

                    if let Some(member) = self.merged(scope, &name.name, meaning) {
                        return Some(member);
                    }
                }

                Some(symbol)
            }
            _ => self.walk(start, &name.name, meaning),
        }
    }

    /// The symbol a qualified name such as `N.M.E` denotes: the export `E` of any block of the
    /// namespace path `N.M`. In `typeof N.M.E` (`meaning` [`Meaning::Value`]) `N` is a value, so a
    /// local value shadows a namespace `N`.
    #[must_use]
    pub fn resolve_qualified(
        &self,
        name: &TSQualifiedName<'a>,
        meaning: Meaning,
    ) -> Option<SymbolId> {
        let left = if meaning == Meaning::Value {
            Meaning::Value
        } else {
            Meaning::Namespace
        };
        let key = self.namespace(&name.left, left, &mut Vec::new())?;

        self.blocks
            .get(&key)?
            .iter()
            .find_map(|&block| self.export(block, &name.right.name, meaning))
    }

    /// Every declaration of the merged symbol that `symbol` belongs to: its own and those of the
    /// same exported name in the other blocks of its namespace, `declare global` or `declare
    /// module` block, whatever their meaning, as TypeScript merges them all into one symbol.
    #[must_use]
    pub fn declarations(&self, symbol: SymbolId) -> Vec<NodeId> {
        let scoping = self.semantic.scoping();
        let scope = scoping.symbol_scope_id(symbol);
        let name = scoping.symbol_name(symbol);
        let symbols: Vec<SymbolId> = match self.keys.get(&scope) {
            Some(key) if self.exported(symbol, scope) => self.blocks[key]
                .iter()
                .filter_map(|&block| {
                    if block == scope {
                        Some(symbol)
                    } else {
                        scoping
                            .get_binding(block, name.into())
                            .filter(|&member| self.exported(member, block))
                    }
                })
                .collect(),
            _ => vec![symbol],
        };

        symbols
            .into_iter()
            .flat_map(|member| scoping.symbol_declarations(member))
            .collect()
    }

    /// The namespace path a name denotes; `left` is the meaning its leftmost identifier is looked
    /// up in, and `aliases` holds the `import X = A` declarations being followed.
    fn namespace(
        &self,
        name: &TSTypeName<'a>,
        left: Meaning,
        aliases: &mut Vec<NodeId>,
    ) -> Option<Key> {
        match name {
            TSTypeName::IdentifierReference(identifier) => self.named(identifier, left, aliases),
            TSTypeName::QualifiedName(qualified) => self.qualified(qualified, left, aliases),
            TSTypeName::ThisExpression(_) => None,
        }
    }

    /// The namespace path a name denotes, through `import X = A` aliases.
    fn named(
        &self,
        identifier: &IdentifierReference<'a>,
        meaning: Meaning,
        aliases: &mut Vec<NodeId>,
    ) -> Option<Key> {
        let symbol = self.resolve(identifier, meaning)?;

        if let Some(alias) = self.alias(symbol) {
            return self.target(alias, aliases);
        }

        self.semantic
            .scoping()
            .symbol_declarations(symbol)
            .find_map(|declaration| self.declarations.get(&declaration).cloned())
    }

    /// The namespace path of a qualified name such as `N.M`, through an exported alias `M`.
    fn qualified(
        &self,
        name: &TSQualifiedName<'a>,
        left: Meaning,
        aliases: &mut Vec<NodeId>,
    ) -> Option<Key> {
        let (scope, mut path) = self.namespace(&name.left, left, aliases)?;
        let member = self.blocks.get(&(scope, path.clone())).and_then(|blocks| {
            blocks
                .iter()
                .find_map(|&block| self.export(block, &name.right.name, Meaning::Namespace))
        });

        if let Some(alias) = member.and_then(|member| self.alias(member)) {
            return self.target(alias, aliases);
        }

        path.push(name.right.name.to_string());
        Some((scope, path))
    }

    /// The `import X = A` declaration of an alias symbol.
    fn alias(&self, symbol: SymbolId) -> Option<&'a TSImportEqualsDeclaration<'a>> {
        let nodes = self.semantic.nodes();

        self.semantic
            .scoping()
            .symbol_declarations(symbol)
            .find_map(|declaration| match nodes.kind(declaration) {
                AstKind::TSImportEqualsDeclaration(alias) => Some(alias),
                _ => None,
            })
    }

    /// The namespace path an `import X = A` alias names; an alias that its own target needs again
    /// is a cycle and names nothing, while a path may pass the same alias several times.
    fn target(
        &self,
        alias: &TSImportEqualsDeclaration<'a>,
        aliases: &mut Vec<NodeId>,
    ) -> Option<Key> {
        let id = alias.node_id.get();

        if aliases.contains(&id) {
            return None;
        }

        aliases.push(id);

        let key = match &alias.module_reference {
            TSModuleReference::IdentifierReference(target) => {
                self.named(target, Meaning::Namespace, aliases)
            }
            TSModuleReference::QualifiedName(target) => {
                self.qualified(target, Meaning::Namespace, aliases)
            }
            TSModuleReference::ExternalModuleReference(_) => None,
        };

        aliases.pop();
        key
    }

    /// The key of a namespace declaration: its path up to the outermost namespace it is a member
    /// of, within the scope that declares that namespace. A namespace is a member of the
    /// namespace around it when it is exported, the inner name of a dotted `namespace A.B`, or
    /// declared in an ambient block without explicit exports; any other namespace is local to its
    /// block.
    fn key(&self, node: NodeId, declaration_file: bool) -> Key {
        let nodes = self.semantic.nodes();
        let mut current = node;
        let mut path = Vec::new();

        while let AstKind::TSNamespaceDeclaration(namespace) = nodes.kind(current) {
            path.push(namespace.id.name.to_string());

            let parent = nodes.parent_id(current);
            let outer = match nodes.kind(parent) {
                AstKind::TSNamespaceDeclaration(_) => parent,
                AstKind::ExportDeclaration(_) => {
                    let block = nodes.parent_id(parent);

                    if !matches!(nodes.kind(block), AstKind::TSModuleBlock(_)) {
                        break;
                    }

                    nodes.parent_id(block)
                }
                AstKind::TSModuleBlock(block)
                    if ambient_context(nodes, nodes.parent_id(parent), declaration_file)
                        && !explicit(block) =>
                {
                    nodes.parent_id(parent)
                }
                _ => break,
            };

            if !matches!(nodes.kind(outer), AstKind::TSNamespaceDeclaration(_)) {
                break;
            }

            current = outer;
        }

        path.reverse();
        let scope = nodes.get_node(current).scope_id();

        (self.canonical.get(&scope).copied().unwrap_or(scope), path)
    }

    /// Outward lookup from `start` that skips bindings of another meaning.
    fn walk(&self, start: ScopeId, name: &str, meaning: Meaning) -> Option<SymbolId> {
        let scoping = self.semantic.scoping();

        for scope in scoping.scope_ancestors(start) {
            if let Some(symbol) = scoping.get_binding(scope, name.into())
                && meaning.accepts(scoping.symbol_flags(symbol))
            {
                return Some(symbol);
            }

            if let Some(member) = self.merged(scope, name, meaning) {
                return Some(member);
            }
        }

        self.globals.iter().find_map(|&global| {
            scoping
                .get_binding(global, name.into())
                .filter(|&symbol| meaning.accepts(scoping.symbol_flags(symbol)))
        })
    }

    /// An export named `name` of another block of the namespace whose block is `scope`.
    fn merged(&self, scope: ScopeId, name: &str, meaning: Meaning) -> Option<SymbolId> {
        let key = self.keys.get(&scope)?;

        self.blocks[key]
            .iter()
            .filter(|&&block| block != scope)
            .find_map(|&block| self.export(block, name, meaning))
    }

    /// The exported member `name` of one namespace block.
    fn export(&self, block: ScopeId, name: &str, meaning: Meaning) -> Option<SymbolId> {
        let scoping = self.semantic.scoping();
        let member = scoping.get_binding(block, name.into())?;

        (meaning.accepts(scoping.symbol_flags(member)) && self.exported(member, block))
            .then_some(member)
    }

    /// Whether a member of a namespace block is visible from the other blocks: declared under an
    /// `export`, as the inner name of a dotted namespace, or other than as an `import X = A` alias
    /// in an ambient block.
    fn exported(&self, member: SymbolId, block: ScopeId) -> bool {
        let nodes = self.semantic.nodes();
        let declaration = self.semantic.scoping().symbol_declaration(member);

        if matches!(
            nodes.kind(declaration),
            AstKind::TSImportEqualsDeclaration(_)
        ) && !matches!(
            nodes.parent_kind(declaration),
            AstKind::ExportDeclaration(_)
        ) {
            return false;
        }

        for ancestor in nodes.ancestor_ids(declaration) {
            match nodes.kind(ancestor) {
                AstKind::ExportDeclaration(_) | AstKind::TSNamespaceDeclaration(_) => return true,
                AstKind::TSModuleBlock(_) | AstKind::Program(_) => break,
                _ => {}
            }
        }

        self.ambient.contains(&block)
    }
}

/// Whether a declaration is in an ambient context: a `declare` namespace around it or of its
/// own, `declare global`, `declare module "m"` or a declaration file.
fn ambient_context(nodes: &AstNodes<'_>, node: NodeId, declaration_file: bool) -> bool {
    declaration_file
        || std::iter::once(node)
            .chain(nodes.ancestor_ids(node))
            .any(|id| match nodes.kind(id) {
                AstKind::TSNamespaceDeclaration(namespace) => namespace.declare,
                AstKind::TSGlobalDeclaration(_) | AstKind::TSExternalModuleDeclaration(_) => true,
                _ => false,
            })
}

/// Whether a namespace block names its exports with `export {}`, `export {} from`, `export *` or
/// `export =`.
fn explicit(block: &TSModuleBlock<'_>) -> bool {
    block.body.iter().any(|statement| {
        matches!(
            statement,
            Statement::ExportNamedDeclaration(_)
                | Statement::ExportAllDeclaration(_)
                | Statement::ExportFromDeclaration(_)
                | Statement::TSExportAssignment(_)
        )
    })
}
