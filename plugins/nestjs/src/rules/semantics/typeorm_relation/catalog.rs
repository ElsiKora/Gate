//! The repository catalog: every source file, its `@Entity` classes
//! with their persisted columns, its enums, and the exports that reach them across files.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gate_typescript::SourceFile;
use gate_typescript::binder::Binder;
use gate_typescript::oxc_ast::ast::{
    Class, ClassElement, Declaration, Decorator, ExportDefaultDeclarationKind, ImportMeta,
    ImportOrExportKind, Program, Statement, TSEnumDeclaration, TSModuleReference,
};
use gate_typescript::oxc_ast_visit::Visit;
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::syntax::constructor;

use super::Vocabulary;
use super::decorators::{self, Bindings, Resolved};
use super::grammar::{self, Persisted};

/// One source file: its path, tree and, when it imports `TypeORM` decorators, their bindings and
/// a binder to resolve names with. A file oxc cannot parse has an empty tree.
pub(super) struct Unit<'s, 'a, 'v> {
    pub path: &'s str,
    pub file: &'s SourceFile<'a>,
    pub resolver: Option<(Bindings<'v>, Binder<'s, 'a>)>,
}

impl<'a, 'v> Unit<'_, 'a, 'v> {
    /// The recognized `TypeORM` decorators of a node, in order.
    pub(super) fn decorators<'d>(
        &self,
        decorators: &'d [Decorator<'a>],
        vocabulary: &'v Vocabulary,
    ) -> Vec<Resolved<'d, 'a, 'v>> {
        let Some((bindings, binder)) = &self.resolver else {
            return Vec::new();
        };

        decorators
            .iter()
            .filter_map(|decorator| decorators::resolve(decorator, bindings, binder, vocabulary))
            .collect()
    }

    /// The line of an offset.
    pub(super) fn line(&self, offset: u32) -> u32 {
        self.file.line(offset)
    }
}

/// A persisted column of a class.
#[derive(Clone, Copy, Debug)]
pub(super) struct Column<'a> {
    pub member: &'a ClassElement<'a>,
    pub state: Persisted,
}

/// The persisted columns of a class by persisted name, in member order.
pub(super) type Columns<'a> = BTreeMap<String, Vec<Column<'a>>>;

/// One `@Entity` class.
pub(super) struct Entity<'a> {
    pub unit: usize,
    pub name: String,
    pub columns: Columns<'a>,
    pub exported: bool,
}

/// One top-level enum.
struct Enum {
    exported: bool,
}

/// What a repository export lookup looks for: an entity or an enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Entity,
    Enum,
}

/// The catalog of every source file.
pub(super) struct Catalog<'s, 'a, 'v> {
    pub units: Vec<Unit<'s, 'a, 'v>>,
    pub entities: Vec<Entity<'a>>,
    pub vocabulary: &'v Vocabulary,
    enums: Vec<Enum>,
    /// The unit index of each path.
    by_path: HashMap<&'s str, usize>,
    /// The entities and enums of each unit by name.
    entity_names: Vec<HashMap<String, Vec<usize>>>,
    enum_names: Vec<HashMap<String, Vec<usize>>>,
    /// The entity or enum a declaration is, by its address.
    entity_declarations: HashMap<usize, usize>,
    enum_declarations: HashMap<usize, (usize, String)>,
    /// Whether any file declares a global `Date` type, which makes `Date` no intrinsic anywhere.
    pub global_date: bool,
}

/// Whether a declaration declares a type named `Date`: a class, interface, type alias or enum.
fn date_type(declaration: &Declaration<'_>) -> bool {
    match declaration {
        Declaration::ClassDeclaration(class) => {
            class.id.as_ref().is_some_and(|id| id.name == "Date")
        }
        Declaration::TSInterfaceDeclaration(declaration) => declaration.id.name == "Date",
        Declaration::TSTypeAliasDeclaration(declaration) => declaration.id.name == "Date",
        Declaration::TSEnumDeclaration(declaration) => declaration.id.name == "Date",
        _ => false,
    }
}

/// Whether statements declare a type named `Date`, exported or not.
fn declares_date(statements: &[Statement<'_>]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::ExportDeclaration(export) => date_type(&export.declaration),
        statement => statement.as_declaration().is_some_and(date_type),
    })
}

/// Whether TypeScript reads a file as a module: an import or export declaration, an
/// `import X = require(...)` or an `import.meta` anywhere. `export as namespace` alone does not
/// make one.
fn module(program: &Program<'_>) -> bool {
    struct Meta {
        found: bool,
    }

    impl Visit<'_> for Meta {
        fn visit_import_meta(&mut self, _: &ImportMeta) {
            self.found = true;
        }
    }

    let declared = program.body.iter().any(|statement| {
        matches!(
            statement,
            Statement::ImportDeclaration(_)
                | Statement::ExportAllDeclaration(_)
                | Statement::ExportDefaultDeclaration(_)
                | Statement::ExportDeclaration(_)
                | Statement::ExportNamedDeclaration(_)
                | Statement::ExportFromDeclaration(_)
                | Statement::TSExportAssignment(_)
        ) || matches!(statement, Statement::TSImportEqualsDeclaration(declaration)
            if matches!(declaration.module_reference, TSModuleReference::ExternalModuleReference(_)))
    });

    declared || {
        let mut meta = Meta { found: false };
        meta.visit_program(program);
        meta.found
    }
}

/// Whether a file declares a global type named `Date`: a script's top-level class, interface,
/// type alias or enum, or one in a module's `declare global` block, which the compiler's root-only
/// program merged into the global scope of every file.
fn declares_global_date(program: &Program<'_>) -> bool {
    if module(program) {
        program.body.iter().any(|statement| {
            matches!(statement, Statement::TSGlobalDeclaration(global) if declares_date(&global.body.body))
        })
    } else {
        declares_date(&program.body)
    }
}

/// The persisted name of a member.
pub(super) fn column_property(element: &ClassElement<'_>) -> Option<String> {
    let (key, computed) = match element {
        ClassElement::MethodDefinition(method) => (&method.key, method.computed),
        ClassElement::PropertyDefinition(property) => (&property.key, property.computed),
        ClassElement::AccessorProperty(property) => (&property.key, property.computed),
        _ => return None,
    };

    super::literal_name(key, computed)
}

/// The decorators written on a class member.
pub(super) fn member_decorators<'e, 'a>(element: &'e ClassElement<'a>) -> &'e [Decorator<'a>] {
    match element {
        // The compiler attaches no decorator to a constructor (`canHaveDecorators`).
        ClassElement::MethodDefinition(method) if constructor(method) => &[],
        ClassElement::MethodDefinition(method) => &method.decorators,
        ClassElement::PropertyDefinition(property) => &property.decorators,
        ClassElement::AccessorProperty(property) => &property.decorators,
        _ => &[],
    }
}

/// The persisted columns of a class, from every recognized column decorator of each member.
fn columns<'a>(
    unit: &Unit<'_, 'a, '_>,
    class: &'a Class<'a>,
    vocabulary: &Vocabulary,
) -> Columns<'a> {
    let mut columns: Columns<'a> = BTreeMap::new();

    for member in &class.body.body {
        for decorator in unit.decorators(member_decorators(member), vocabulary) {
            if !vocabulary
                .columns
                .iter()
                .any(|name| name == decorator.canonical)
            {
                continue;
            }

            if let (_, Some((name, state))) =
                grammar::column(column_property(member).as_deref(), &decorator)
            {
                columns
                    .entry(name)
                    .or_default()
                    .push(Column { member, state });
            }
        }
    }

    columns
}

/// Whether a class has exactly one `@Entity(...)` call decorator.
fn entity<'a>(unit: &Unit<'_, 'a, '_>, class: &Class<'a>, vocabulary: &Vocabulary) -> bool {
    unit.decorators(&class.decorators, vocabulary)
        .iter()
        .filter(|decorator| decorator.canonical == "Entity" && decorator.call.is_some())
        .count()
        == 1
}

/// The names `export { Name }` exports without renaming, outside type-only exports.
fn local_exports(statements: &[Statement<'_>]) -> BTreeSet<String> {
    statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::ExportNamedDeclaration(export)
                if export.export_kind != ImportOrExportKind::Type =>
            {
                Some(export)
            }
            _ => None,
        })
        .flat_map(|export| &export.specifiers)
        .filter(|specifier| {
            specifier.export_kind != ImportOrExportKind::Type
                && specifier.local.span() == specifier.exported.span()
        })
        .map(|specifier| decorators::export_name(&specifier.exported).to_owned())
        .collect()
}

impl<'s, 'a, 'v> Catalog<'s, 'a, 'v> {
    pub(super) fn new(units: Vec<Unit<'s, 'a, 'v>>, vocabulary: &'v Vocabulary) -> Self {
        let mut catalog = Self {
            by_path: units
                .iter()
                .enumerate()
                .map(|(index, unit)| (unit.path, index))
                .collect(),
            entity_names: vec![HashMap::new(); units.len()],
            enum_names: vec![HashMap::new(); units.len()],
            global_date: units
                .iter()
                .any(|unit| declares_global_date(unit.file.program())),
            units,
            entities: Vec::new(),
            vocabulary,
            enums: Vec::new(),
            entity_declarations: HashMap::new(),
            enum_declarations: HashMap::new(),
        };

        for index in 0..catalog.units.len() {
            catalog.read(index);
        }

        catalog
    }

    /// Catalogs the top-level entities and enums of one unit. A class is exported by an `export`
    /// other than `export default` or by a local `export { Name }`, an enum by either.
    fn read(&mut self, index: usize) {
        let vocabulary = self.vocabulary;
        let unit = &self.units[index];
        let program = unit.file.program();
        let exports = local_exports(&program.body);
        let mut entities: Vec<(&'a Class<'a>, String, bool)> = Vec::new();
        let mut enums: Vec<(&'a TSEnumDeclaration<'a>, bool)> = Vec::new();
        let mut class = |class: &'a Class<'a>, export: bool| {
            if let Some(id) = &class.id
                && entity(unit, class, vocabulary)
            {
                let name = id.name.to_string();
                let exported = export || exports.contains(&name);
                entities.push((class, name, exported));
            }
        };

        for statement in &program.body {
            match statement {
                Statement::ClassDeclaration(declaration) => class(declaration, false),
                Statement::ExportDefaultDeclaration(export) => {
                    if let ExportDefaultDeclarationKind::ClassDeclaration(declaration) =
                        &export.declaration
                    {
                        class(declaration, false);
                    }
                }
                Statement::ExportDeclaration(export) => match &export.declaration {
                    Declaration::ClassDeclaration(declaration) => class(declaration, true),
                    Declaration::TSEnumDeclaration(declaration) => {
                        enums.push((declaration, true));
                    }
                    _ => {}
                },
                Statement::TSEnumDeclaration(declaration) => {
                    enums.push((declaration, exports.contains(declaration.id.name.as_str())));
                }
                _ => {}
            }
        }

        for (class, name, exported) in entities {
            let position = self.entities.len();
            self.entity_names[index]
                .entry(name.clone())
                .or_default()
                .push(position);
            self.entity_declarations
                .insert(std::ptr::from_ref(class).addr(), position);
            self.entities.push(Entity {
                unit: index,
                columns: columns(&self.units[index], class, vocabulary),
                name,
                exported,
            });
        }

        for (declaration, exported) in enums {
            let position = self.enums.len();
            let name = declaration.id.name.to_string();
            self.enum_names[index]
                .entry(name.clone())
                .or_default()
                .push(position);
            self.enum_declarations
                .insert(std::ptr::from_ref(declaration).addr(), (position, name));
            self.enums.push(Enum { exported });
        }
    }

    /// The entity a class declaration is.
    pub(super) fn entity_of(&self, class: &Class<'_>) -> Option<usize> {
        self.entity_declarations
            .get(&std::ptr::from_ref(class).addr())
            .copied()
    }

    /// The identity of an exported top-level enum declaration of `unit`.
    pub(super) fn enum_identity(
        &self,
        unit: usize,
        declaration: &TSEnumDeclaration<'_>,
    ) -> Option<String> {
        let (index, name) = self
            .enum_declarations
            .get(&std::ptr::from_ref(declaration).addr())?;

        self.enums[*index]
            .exported
            .then(|| format!("{}:{name}", self.units[unit].path))
    }

    /// An aliased or relative specifier to the one source file it
    /// names, as `<base>.ts` or `<base>/index.ts`. A specifier is aliased when it starts with one
    /// of the followed aliases and a `/` (the longest wins), and that part is replaced by the
    /// alias's folder; its base is normalized as `path.join` does, keeping a trailing slash. A
    /// relative base is normalized as `path.resolve` does, without it.
    pub(super) fn imported(&self, importer: &str, specifier: &str) -> Option<usize> {
        let alias = self
            .vocabulary
            .aliases
            .iter()
            .filter_map(|(alias, folder)| {
                specifier
                    .strip_prefix(alias.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
                    .map(|rest| (alias.len(), folder, rest))
            })
            .max_by_key(|(length, _, _)| *length);
        let base = if let Some((_, folder, rest)) = alias {
            let joined = format!("{folder}/{rest}");
            let mut base = normalize(&joined)?;

            if joined.ends_with('/') {
                base.push('/');
            }

            base
        } else if specifier.starts_with('.') {
            let directory = importer
                .rsplit_once('/')
                .map_or("", |(directory, _)| directory);
            normalize(&format!("{directory}/{specifier}"))?
        } else {
            return None;
        };
        let candidates: Vec<usize> = [format!("{base}.ts"), format!("{base}/index.ts")]
            .iter()
            .filter_map(|candidate| normalize(candidate))
            .filter_map(|candidate| self.by_path.get(candidate.as_str()).copied())
            .collect();

        match candidates.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// The exported entities or enums named `name` in a unit, directly
    /// or through its named re-exports, once per path that reaches each, with their identities.
    pub(super) fn exported(
        &self,
        unit: usize,
        name: &str,
        kind: Kind,
        runtime: bool,
        seen: &[(usize, String)],
    ) -> Vec<(usize, String)> {
        if seen
            .iter()
            .any(|(visited, visited_name)| *visited == unit && visited_name == name)
        {
            return Vec::new();
        }

        let mut next = seen.to_vec();
        next.push((unit, name.to_owned()));
        let names = match kind {
            Kind::Entity => &self.entity_names[unit],
            Kind::Enum => &self.enum_names[unit],
        };
        let mut found: Vec<(usize, String)> = names
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| match kind {
                Kind::Entity => self.entities[index].exported,
                Kind::Enum => self.enums[index].exported,
            })
            .map(|index| (index, format!("{}:{name}", self.units[unit].path)))
            .collect();

        for statement in &self.units[unit].file.program().body {
            let Statement::ExportFromDeclaration(export) = statement else {
                continue;
            };
            let source = &export.source;

            if runtime && export.export_kind == ImportOrExportKind::Type {
                continue;
            }

            for specifier in &export.specifiers {
                if decorators::export_name(&specifier.exported) != name
                    || (runtime && specifier.export_kind == ImportOrExportKind::Type)
                {
                    continue;
                }

                let Some(target) = self.imported(self.units[unit].path, source.value.as_str())
                else {
                    continue;
                };

                found.extend(self.exported(
                    target,
                    decorators::export_name(&specifier.local),
                    kind,
                    runtime,
                    &next,
                ));
            }
        }

        found
    }
}

/// A path with its `.`, `..` and empty segments removed; `None` above the root.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }

    Some(parts.join("/"))
}
