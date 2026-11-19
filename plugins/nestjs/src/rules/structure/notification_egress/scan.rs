//! The scan of the sources against the options: egress uses, package families, notification loads
//! and the stale entries.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gate_typescript::SourceFile;
use gate_typescript::modules::{is_type_only_import, module_loads};
use gate_typescript::owners::{
    Owner, SourceContext, Sources, class_declaration, export_name, top_level, unwrap,
};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Expression, IdentifierReference, ImportDeclaration, ImportDeclarationSpecifier,
    ImportOrExportKind, Statement,
};

use super::{Options, Transport};
use crate::shared::json_lines::text_line;
use crate::shared::report::Findings;

/// One egress use of a file.
#[derive(Debug)]
pub(super) enum Use {
    /// A runtime load of the listed egress module.
    Module(String),
    /// A runtime import of the transport symbol.
    Symbol(String),
    /// A reference to the global egress function.
    Global(String),
}

impl std::fmt::Display for Use {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Module(module) => write!(formatter, "the module {module}"),
            Self::Symbol(symbol) => write!(formatter, "the transport symbol {symbol}"),
            Self::Global(name) => write!(formatter, "the global {name}"),
        }
    }
}

/// The egress uses of every file that has any, by path, each with its line.
pub(super) type Uses = BTreeMap<String, Vec<(u32, Use)>>;

pub(super) struct Scan<'s, 'a> {
    pub(super) sources: &'s Sources<'a>,
    pub(super) options: &'s Options,
    pub(super) consumer: Option<Owner>,
}

impl<'a> Scan<'_, 'a> {
    fn transport(&self) -> Option<&Transport> {
        self.options.egress.transport.as_ref()
    }

    /// The egress uses of every file that has any, by path.
    pub(super) fn uses(&self) -> Uses {
        self.sources
            .iter()
            .map(|source| (source.path.clone(), self.egress_uses(source)))
            .filter(|(_, uses)| !uses.is_empty())
            .collect()
    }

    /// Reports `unclassified-package` in every file and returns the platform clients loaded.
    pub(super) fn unclassified_packages(&self, findings: &mut Findings) -> BTreeSet<usize> {
        let mut platform_loaded = BTreeSet::new();

        for source in self.sources.iter() {
            for (line, specifier) in runtime_loads(&source.file) {
                platform_loaded.extend(self.platform_module(&specifier));

                if let Some(family) = self.unclassified_family(&specifier) {
                    findings.push_keyed(
                        "unclassified-package",
                        source.path.as_str(),
                        Some(line),
                        specifier.as_str(),
                        format!(
                            "{} loads {specifier} of the package family {family}, which the \
                             nestjs/notification-egress policy does not classify; list it as an \
                             egress module used under its owner, or as a platform client with \
                             its decisions",
                            source.path
                        ),
                    );
                }
            }
        }

        platform_loaded
    }

    /// Reports `unclassified-egress`.
    pub(super) fn unclassified_egress(&self, uses: &Uses, findings: &mut Findings) {
        let transport = self.transport();

        for (path, found) in uses {
            if self.owner_of(path).is_some() {
                continue;
            }

            let inside = transport.filter(|transport| under(path, &transport.root));
            let Some((line, first)) = found.iter().find(|(_, used)| match inside {
                None => true,
                Some(transport) => match used {
                    Use::Module(module) => !transport.modules.contains(module),
                    Use::Symbol(_) => false,
                    Use::Global(_) => true,
                },
            }) else {
                continue;
            };
            let message = match inside {
                Some(transport) => format!(
                    "{path} sends outside the application through {first}, which the \
                     transport's module list in the nestjs/notification-egress policy does not \
                     admit; send through the transport's modules, or widen the list in a \
                     reviewed slice that cites {}",
                    transport.decision
                ),
                None => format!(
                    "{path} sends outside the application through {first}, but lies under no \
                     egress owner of the nestjs/notification-egress policy; move it under its \
                     owner or register its root with a class and an accepted decision"
                ),
            };
            findings.push_keyed(
                "unclassified-egress",
                path.as_str(),
                Some(*line),
                "egress",
                message,
            );
        }
    }

    /// Reports `direct-notification` and returns, by notification owner, the files that load it.
    pub(super) fn direct_notifications(
        &self,
        findings: &mut Findings,
    ) -> HashMap<usize, BTreeSet<String>> {
        let mut loaded: HashMap<usize, BTreeSet<String>> = HashMap::new();
        let (Some(class), Some(consumer), Some(label)) = (
            self.options.notification_class.as_deref(),
            self.consumer.as_ref(),
            self.options.consumer_label.as_deref(),
        ) else {
            return loaded;
        };
        let protected = self.protected();

        for source in self.sources.iter() {
            if self.is_notification(&source.path).is_some()
                || self.is_composition(&source.path)
                || self.is_consumer(source)
            {
                continue;
            }

            for (indexes, line, what) in self.notification_loads(source, &protected) {
                for index in &indexes {
                    loaded
                        .entry(*index)
                        .or_default()
                        .insert(source.path.clone());
                }

                if indexes.iter().any(|index| {
                    self.options.owners[*index]
                        .senders
                        .iter()
                        .any(|sender| sender.path == source.path)
                }) {
                    continue;
                }

                let roots = indexes
                    .iter()
                    .map(|index| self.options.owners[*index].root.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                findings.push_keyed(
                    "direct-notification",
                    source.path.as_str(),
                    Some(line),
                    what.as_str(),
                    format!(
                        "{} loads {what} of the {class} owner {roots}; a notification leaves \
                         only through a class extending {}, so send it from a {label}",
                        source.path, consumer.name
                    ),
                );
            }
        }

        loaded
    }

    /// Reports the `stale-owner` findings of the owners and their senders, at each owner's line.
    pub(super) fn stale_owners(
        &self,
        uses: &Uses,
        loaded: &HashMap<usize, BTreeSet<String>>,
        (path, owner_lines): (&str, &[u32]),
        findings: &mut Findings,
    ) {
        for (index, owner) in self.options.owners.iter().enumerate() {
            let line = owner_lines.get(index).copied();
            let sends = self.sources.iter().any(|source| {
                self.owner_of(&source.path) == Some(index) && uses.contains_key(&source.path)
            });

            if !sends {
                findings.push_keyed(
                    "stale-owner",
                    path,
                    line,
                    format!("{}#owner", owner.root),
                    format!(
                        "the egress owner {} has no tracked source file that uses an egress \
                         point; remove it",
                        owner.root
                    ),
                );
            }

            for sender in &owner.senders {
                if !loaded
                    .get(&index)
                    .is_some_and(|paths| paths.contains(&sender.path))
                {
                    findings.push_keyed(
                        "stale-owner",
                        path,
                        line,
                        format!("{}:{}#sender", owner.root, sender.path),
                        format!(
                            "the sender {} of the egress owner {} is not tracked or loads \
                             nothing of that owner; remove it",
                            sender.path, owner.root
                        ),
                    );
                }
            }
        }
    }

    /// Reports the `stale-owner` findings of the transport and the platform clients. Lines are
    /// those of the first occurrence in the `egress` entry of the options text.
    pub(super) fn stale_egress(
        &self,
        uses: &Uses,
        platform_loaded: &BTreeSet<usize>,
        (path, text): (&str, &str),
        findings: &mut Findings,
    ) {
        let (egress_text, egress_start) = block(text, "\"egress\"");
        let line = |block: &str, start: u32, needle: &str| {
            text_line(block, needle).map(|line| line + start)
        };

        if let Some(transport) = self.transport() {
            self.stale_transport(uses, transport, (path, egress_text, egress_start), findings);
        }

        for (index, platform) in self.options.egress.platform.iter().enumerate() {
            if !platform_loaded.contains(&index) {
                findings.push_keyed(
                    "stale-owner",
                    path,
                    line(
                        egress_text,
                        egress_start,
                        &format!("\"{}\"", platform.module),
                    ),
                    format!("{}#platform", platform.module),
                    format!(
                        "the platform client {} is loaded by no source file; remove it",
                        platform.module
                    ),
                );
            }
        }
    }

    fn stale_transport(
        &self,
        uses: &Uses,
        transport: &Transport,
        (path, egress_text, egress_start): (&str, &str, u32),
        findings: &mut Findings,
    ) {
        let line = |block: &str, start: u32, needle: &str| {
            text_line(block, needle).map(|line| line + start)
        };
        let transport_files: Vec<&SourceContext<'_>> = self
            .sources
            .iter()
            .filter(|source| under(&source.path, &transport.root))
            .collect();

        if transport_files.is_empty() {
            findings.push_keyed(
                "stale-owner",
                path,
                line(egress_text, egress_start, &transport.root),
                format!("{}#transport-root", transport.root),
                format!(
                    "the transport root {} has no tracked source file; remove it",
                    transport.root
                ),
            );
        }

        for symbol in &transport.symbols {
            let declared = transport_files.iter().any(|source| {
                self.sources
                    .owner(Some(&source.path), symbol)
                    .is_some_and(|owner| owner.path.as_deref() == Some(source.path.as_str()))
            });

            if !declared {
                findings.push_keyed(
                    "stale-owner",
                    path,
                    line(egress_text, egress_start, &format!("\"{symbol}\"")),
                    format!("{symbol}#transport-symbol"),
                    format!(
                        "the transport symbol {symbol} is declared by no file below {}; remove it",
                        transport.root
                    ),
                );
            }
        }

        // The transport's own block, where its modules are listed.
        let (transport_text, transport_start) = block(egress_text, "\"transport\"");

        for module in &transport.modules {
            let loaded = uses.iter().any(|(path, found)| {
                under(path, &transport.root)
                    && self.owner_of(path).is_none()
                    && found
                        .iter()
                        .any(|(_, used)| matches!(used, Use::Module(name) if name == module))
            });

            if !loaded {
                findings.push_keyed(
                    "stale-owner",
                    path,
                    line(
                        transport_text,
                        egress_start + transport_start,
                        &format!("\"{module}\""),
                    ),
                    format!("{module}#transport-module"),
                    format!(
                        "the transport module {module} is loaded by no file below {}; remove it",
                        transport.root
                    ),
                );
            }
        }
    }

    /// The owner with the deepest root containing `path`.
    fn owner_of(&self, path: &str) -> Option<usize> {
        self.options
            .owners
            .iter()
            .enumerate()
            .filter(|(_, owner)| under(path, &owner.root))
            .max_by_key(|(_, owner)| owner.root.len())
            .map(|(index, _)| index)
    }

    /// The owner of `path` when its classes include the notification class.
    fn is_notification(&self, path: &str) -> Option<usize> {
        let class = self.options.notification_class.as_deref()?;

        self.owner_of(path).filter(|index| {
            self.options.owners[*index]
                .classes
                .iter()
                .any(|own| own == class)
        })
    }

    /// A barrel or a module file, which composes owners without sending.
    fn is_composition(&self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);

        self.options
            .composition_files
            .iter()
            .any(|file| file == name)
    }

    /// Every egress use of a file with its line, in source order.
    fn egress_uses(&self, source: &SourceContext<'a>) -> Vec<(u32, Use)> {
        let file = &source.file;
        let mut uses = Vec::new();

        for (line, specifier) in runtime_loads(file) {
            if let Some(module) = self.egress_module(&specifier) {
                uses.push((line, Use::Module(module.to_owned())));
            }
        }

        for statement in top_level(file) {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };

            for name in self.transport_imports(source, import) {
                uses.push((file.line(import.span.start), Use::Symbol(name)));
            }
        }

        for node in file.semantic().nodes().iter() {
            if let Some((start, name)) =
                global_egress(file, node.kind(), &self.options.global_egress_functions)
            {
                uses.push((file.line(start), Use::Global(name.to_owned())));
            }
        }

        uses.sort_by_key(|(line, _)| *line);
        uses
    }

    fn egress_module<'m>(&'m self, specifier: &str) -> Option<&'m str> {
        self.options
            .egress
            .modules
            .iter()
            .find(|module| loads_module(specifier, module))
            .map(String::as_str)
    }

    /// The index of the platform client a specifier loads.
    fn platform_module(&self, specifier: &str) -> Option<usize> {
        self.options
            .egress
            .platform
            .iter()
            .position(|platform| loads_module(specifier, &platform.module))
    }

    /// The family prefix of a specifier that is neither an egress module nor a platform client.
    fn unclassified_family(&self, specifier: &str) -> Option<&str> {
        self.options
            .egress
            .families
            .iter()
            .find(|family| specifier.starts_with(family.as_str()))
            .filter(|_| {
                self.egress_module(specifier).is_none() && self.platform_module(specifier).is_none()
            })
            .map(String::as_str)
    }

    /// The transport symbols a runtime import declaration binds.
    fn transport_imports(
        &self,
        source: &SourceContext<'a>,
        import: &ImportDeclaration<'_>,
    ) -> Vec<String> {
        let Some(transport) = self.transport() else {
            return Vec::new();
        };

        if import.import_kind == ImportOrExportKind::Type {
            return Vec::new();
        }

        let resolved = source.specifier(&import.source.value);

        import
            .specifiers
            .iter()
            .flatten()
            .filter_map(|specifier| match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(named)
                    if named.import_kind != ImportOrExportKind::Type =>
                {
                    Some(export_name(&named.imported))
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => Some("default".to_owned()),
                _ => None,
            })
            .filter_map(|imported| {
                let owner = self.sources.owner(resolved.as_deref(), &imported)?;

                (transport.symbols.contains(&owner.name)
                    && owner
                        .path
                        .as_deref()
                        .is_some_and(|path| under(path, &transport.root)))
                .then_some(owner.name)
            })
            .collect()
    }

    /// The classes outside the notification owners that their classes extend or implement, each
    /// with its owners.
    fn protected(&self) -> HashMap<Owner, BTreeSet<usize>> {
        let mut protected: HashMap<Owner, BTreeSet<usize>> = HashMap::new();

        for source in self.sources.iter() {
            let Some(index) = self.is_notification(&source.path) else {
                continue;
            };

            for base in self.bases(source) {
                if Some(&base) != self.consumer.as_ref()
                    && base
                        .path
                        .as_deref()
                        .is_some_and(|path| self.is_notification(path).is_none())
                {
                    protected.entry(base).or_default().insert(index);
                }
            }
        }

        protected
    }

    /// The source owners of every class a file's top-level classes extend or implement.
    fn bases(&self, source: &SourceContext<'a>) -> Vec<Owner> {
        let mut bases = Vec::new();

        for statement in top_level(&source.file) {
            let Some(class) = class_declaration(statement) else {
                continue;
            };

            if let Some(owner) = class.heritage.as_ref().and_then(|heritage| {
                self.sources
                    .canonical(source.identity(&heritage.expression, true).as_ref())
            }) {
                bases.push(owner);
            }

            for implements in &class.implements {
                if let Some(owner) = self
                    .sources
                    .canonical(source.type_identity(&implements.expression).as_ref())
                {
                    bases.push(owner);
                }
            }
        }

        bases
    }

    fn is_consumer(&self, source: &SourceContext<'a>) -> bool {
        self.consumer
            .as_ref()
            .is_some_and(|consumer| self.bases(source).contains(consumer))
    }

    /// What a file loads of the notification owners: the owners, the line and a description. A
    /// base class that several owners' classes extend loads all of them.
    fn notification_loads(
        &self,
        source: &SourceContext<'a>,
        protected: &HashMap<Owner, BTreeSet<usize>>,
    ) -> Vec<(BTreeSet<usize>, u32, String)> {
        let file = &source.file;
        let mut loads = Vec::new();

        for statement in top_level(file) {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };
            let line = file.line(import.span.start);
            let resolved = source.specifier(&import.source.value);

            if let Some(index) = resolved
                .as_deref()
                .and_then(|path| self.is_notification(path))
            {
                loads.push((
                    BTreeSet::from([index]),
                    line,
                    format!("the module {}", import.source.value),
                ));
                continue;
            }

            for specifier in import.specifiers.iter().flatten() {
                let imported = match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(named) => {
                        export_name(&named.imported)
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => "default".to_owned(),
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => continue,
                };
                let Some(owner) = self.sources.owner(resolved.as_deref(), &imported) else {
                    continue;
                };
                let indexes = match owner
                    .path
                    .as_deref()
                    .and_then(|path| self.is_notification(path))
                {
                    Some(index) => BTreeSet::from([index]),
                    None => protected.get(&owner).cloned().unwrap_or_default(),
                };

                if !indexes.is_empty() {
                    loads.push((indexes, line, format!("the symbol {}", owner.name)));
                }
            }
        }

        loads
    }
}

/// The text from the first occurrence of `key` on, and the number of lines before it; the whole
/// text when it has no such key.
fn block<'t>(text: &'t str, key: &str) -> (&'t str, u32) {
    text.find(key).map_or((text, 0), |offset| {
        (
            &text[offset..],
            u32::try_from(text[..offset].matches('\n').count()).unwrap_or(0),
        )
    })
}

/// The start and name of a reference to one of the global egress functions, called or not: an
/// unshadowed name read as a value, or the member of that name of the unshadowed `globalThis`,
/// static or by a string key. A reference in a type, such as `typeof fetch`, is no value.
fn global_egress<'n>(
    file: &SourceFile<'_>,
    kind: AstKind<'_>,
    names: &'n [String],
) -> Option<(u32, &'n str)> {
    let global = |name: &str, identifier: &IdentifierReference<'_>| {
        identifier.name == name
            && identifier.reference_id.get().is_some_and(|reference| {
                let reference = file.semantic().scoping().get_reference(reference);

                reference.symbol_id().is_none() && reference.is_value()
            })
    };
    let global_this = |object: &Expression<'_>| matches!(unwrap(object), Expression::Identifier(object) if global("globalThis", object));

    names.iter().find_map(|name| {
        let start = match kind {
            AstKind::IdentifierReference(identifier) if global(name, identifier) => {
                identifier.span.start
            }
            AstKind::StaticMemberExpression(member)
                if member.property.name == name.as_str() && global_this(&member.object) =>
            {
                member.span.start
            }
            AstKind::ComputedMemberExpression(member)
                if matches!(unwrap(&member.expression), Expression::StringLiteral(key) if key.value == name.as_str())
                    && global_this(&member.object) =>
            {
                member.span.start
            }
            _ => return None,
        };

        Some((start, name.as_str()))
    })
}

/// Every runtime module load of a file with its line: a runtime import declaration at its line,
/// and a re-export, `require` or dynamic `import()` at the first occurrence of its quoted specifier
/// outside comments and type-only import declarations, since the loader keeps no span.
fn runtime_loads(file: &SourceFile<'_>) -> Vec<(u32, String)> {
    let runtime = module_loads(file).runtime;
    let mut loads = Vec::new();
    let mut declared = BTreeSet::new();
    // Comments and type-only import declarations, where a quoted specifier is no load.
    let mut skipped: Vec<std::ops::Range<u32>> = file
        .program()
        .comments
        .iter()
        .map(|comment| comment.span.start..comment.span.end)
        .collect();

    for statement in top_level(file) {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };

        if is_type_only_import(import) {
            skipped.push(import.span.start..import.span.end);
            continue;
        }

        let specifier = import.source.value.to_string();
        loads.push((file.line(import.span.start), specifier.clone()));
        declared.insert(specifier);
    }

    for specifier in runtime
        .iter()
        .filter(|specifier| !declared.contains(*specifier))
    {
        let offset = ['"', '\'', '`']
            .iter()
            .flat_map(|quote| {
                file.source()
                    .match_indices(&format!("{quote}{specifier}{quote}"))
                    .filter_map(|(offset, _)| u32::try_from(offset).ok())
                    .collect::<Vec<_>>()
            })
            .filter(|offset| !skipped.iter().any(|span| span.contains(offset)))
            .min()
            .unwrap_or(0);
        loads.push((file.line(offset), specifier.clone()));
    }

    loads
}

/// Whether a specifier is a module or a path below it.
fn loads_module(specifier: &str, module: &str) -> bool {
    specifier == module
        || specifier
            .strip_prefix(module)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn under(path: &str, root: &str) -> bool {
    path.starts_with(root)
}
