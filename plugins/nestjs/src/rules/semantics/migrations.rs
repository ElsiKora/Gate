//! `nestjs/migrations`: the generated `TypeORM` migrations are fresh-only classes named by
//! timestamp, and the generated barrel and the registry constant register them in timestamp order.
//!
//! A migration is a tracked `.ts` file directly in `generated-folder` other than the barrel, named
//! `<timestamp>-<SemanticName>.ts`; it exports the class `<SemanticName><timestamp>` as a plain
//! class (no `extends`, `implements`, `abstract` or `declare`, so nothing can supply a downgrade)
//! with `name: string = "<class>"` and no `down` member. The barrel is exactly one
//! `export { … } from` line per migration in timestamp order, and the registry imports every
//! migration from `registry-import` and lists them in `registry-constant` in that order. With
//! `single-baseline`, exactly one generated migration, the baseline, is kept.
//!
//! A tree without migrations, registry and barrel has nothing to check. A migration or registry
//! without a syntax tree is skipped: `typescript/parse` reports it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::owners::{
    class_declaration, export_name, identifier_name, top_level, unwrap, variable_declarators,
};
use gate_typescript::oxc_ast::ast::{
    ArrayExpressionElement, ClassElement, Expression, ImportDeclarationSpecifier,
    ImportOrExportKind, PropertyKey, Statement, TSType,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::{Allocator, SourceFile, TypeScript};

use crate::shared::{project, tree as walk};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/migrations`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The folder that holds the generated migrations.
    pub generated_folder: String,
    /// The generated barrel that re-exports every migration.
    pub barrel: String,
    /// The registry module that lists the migrations.
    pub registry: String,
    /// The constant of the registry that lists the migrations.
    pub registry_constant: String,
    /// The specifier the registry imports the migrations from.
    pub registry_import: String,
    /// The number of digits of a migration timestamp.
    #[schemars(range(min = 1))]
    pub timestamp_digits: usize,
    /// Keep exactly one generated migration, the baseline, which changes in place instead of
    /// gaining a successor.
    pub single_baseline: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            generated_folder: "src/migration/typeorm/generated".to_owned(),
            barrel: "src/migration/typeorm/generated/index.ts".to_owned(),
            registry: "src/migration/typeorm/migrations.ts".to_owned(),
            registry_constant: "TYPEORM_MIGRATIONS_CONSTANT".to_owned(),
            registry_import: "./generated".to_owned(),
            timestamp_digits: 13,
            single_baseline: false,
        }
    }
}

/// One generated migration: its class, file name and path.
struct Migration {
    class: String,
    file: String,
    path: String,
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "migrations";
    const TITLE: &'static str =
        "Generated migrations are fresh-only and registered in timestamp order";
    const ASPECTS: &'static [&'static str] = &[
        "barrel",
        "baseline",
        "class",
        "filename",
        "registry",
        "timestamp",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        {
            use crate::shared::paths::Form::{File, Folder};
            let o = run.options();
            crate::shared::paths::check("generated-folder", &o.generated_folder, Folder)?;
            crate::shared::paths::check("barrel", &o.barrel, File)?;
            crate::shared::paths::check("registry", &o.registry, File)?;
        }

        let options = run.options();
        let generated = options.generated_folder.trim_end_matches('/').to_owned();
        let barrel_path = options.barrel.clone();
        let registry = Registry {
            path: options.registry.clone(),
            constant: options.registry_constant.clone(),
            import: options.registry_import.clone(),
        };
        let digits = options.timestamp_digits;
        let single_baseline = options.single_baseline;
        let tree = run.tree();
        let offset = if generated.is_empty() {
            0
        } else {
            generated.len() + 1
        };
        let files: Vec<String> = walk::files(&project::source(run), &generated, &walk::UNSKIPPED)
            .filter(|path| {
                *path != barrel_path
                    && path[offset..]
                        .strip_suffix(".ts")
                        .is_some_and(|stem| !stem.contains('/') && stem != "index")
            })
            .map(str::to_owned)
            .collect();

        // A tree without TypeORM migrations has nothing to check.
        if files.is_empty() && !tree.contains(&registry.path) && !tree.contains(&barrel_path) {
            return Ok(());
        }

        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut migrations: BTreeMap<u64, Vec<Migration>> = BTreeMap::new();

        for path in &files {
            let file = &path[offset..];
            let Some((timestamp, class)) = parse_name(file, digits) else {
                run.report()
                    .finding(
                        "filename",
                        Location::file(path),
                        format!(
                            "Invalid migration filename \"{file}\"; use \"<timestamp>-<SemanticName>.ts\""
                        ),
                    )
                    .help(format!(
                        "rename the file to <{digits}-digit timestamp>-<SemanticName>.ts"
                    ));
                continue;
            };

            if migrations.contains_key(&timestamp) {
                run.report()
                    .finding(
                        "timestamp",
                        Location::file(path),
                        format!("Duplicate migration timestamp \"{timestamp}\""),
                    )
                    .help("give each migration its own timestamp");
            }

            if let Some(source) = project::parsed(&typescript, path)? {
                for (line, message, help) in class_findings(&source, file, &class) {
                    run.report()
                        .finding("class", location(path, line), message)
                        .help(help);
                }
            }

            migrations.entry(timestamp).or_default().push(Migration {
                class,
                file: file.to_owned(),
                path: path.clone(),
            });
        }

        let ordered: Vec<Migration> = migrations.into_values().flatten().collect();

        if single_baseline {
            baseline(run, &generated, &ordered);
        }

        barrel(run, &barrel_path, &ordered);

        registry.report(run, &typescript, &ordered)?;

        Ok(())
    }
}

/// A location in `path`: the line when there is one, the whole file otherwise.
fn location(path: &str, line: Option<u32>) -> Location {
    match line {
        Some(line) => Location::line(path, usize::try_from(line).unwrap_or(usize::MAX)),
        None => Location::file(path),
    }
}

/// The timestamp and class a migration file name gives: `<digits>-<SemanticName>.ts`.
fn parse_name(file: &str, digits: usize) -> Option<(u64, String)> {
    let (timestamp, semantic) = file.strip_suffix(".ts")?.split_once('-')?;
    let mut characters = semantic.chars();

    (timestamp.len() == digits
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && characters
            .next()
            .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| character.is_ascii_alphanumeric()))
    .then_some(())?;

    let timestamp: u64 = timestamp.parse().ok()?;

    Some((timestamp, format!("{semantic}{timestamp}")))
}

/// The class contract findings of one migration: the exported class, its `name` and no
/// downgrade; each with its line and a fix hint.
fn class_findings(
    source: &SourceFile<'_>,
    file: &str,
    class: &str,
) -> Vec<(Option<u32>, String, String)> {
    let exported = top_level(source).find_map(|statement| match statement {
        Statement::ExportDeclaration(_) => class_declaration(statement)
            .filter(|declaration| declaration.id.as_ref().is_some_and(|id| id.name == class)),
        _ => None,
    });
    let Some(exported) = exported else {
        return vec![(
            None,
            format!("{file} must export the fresh-only class {class}"),
            format!("export the class {class}"),
        )];
    };
    let line = Some(source.line(exported.span.start));
    let mut findings = Vec::new();

    // A fresh-only migration is a plain class: a base class or an interface could supply a
    // downgrade that the class itself does not declare.
    if exported.heritage.is_some()
        || !exported.implements.is_empty()
        || exported.r#abstract
        || exported.declare
    {
        findings.push((
            line,
            format!("{file} must declare {class} as a plain class without extends, implements, abstract or declare"),
            "remove the extends, implements, abstract or declare".to_owned(),
        ));
    }

    let downgrade = exported.body.body.iter().any(|element| {
        let key = match element {
            ClassElement::MethodDefinition(method) => Some(&method.key),
            ClassElement::PropertyDefinition(property) => Some(&property.key),
            ClassElement::AccessorProperty(accessor) => Some(&accessor.key),
            ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
        };
        key.and_then(PropertyKey::static_name)
            .is_some_and(|name| name == "down")
    });

    if downgrade {
        findings.push((
            line,
            format!("{file} must not expose an executable downgrade contract"),
            "remove the down member".to_owned(),
        ));
    }

    let named = exported.body.body.iter().any(|element| {
        let ClassElement::PropertyDefinition(property) = element else {
            return false;
        };

        !property.r#static
            && !property.computed
            && property.key.static_name().is_some_and(|name| name == "name")
            && property.type_annotation.as_ref().is_some_and(|annotation| {
                matches!(annotation.type_annotation, TSType::TSStringKeyword(_))
            })
            && property.value.as_ref().is_some_and(|value| {
                matches!(unwrap(value), Expression::StringLiteral(literal) if literal.value == class)
            })
    });

    if !named {
        findings.push((
            line,
            format!("{file} must set name: string = \"{class}\""),
            format!("declare name: string = \"{class}\""),
        ));
    }

    findings
}

/// Reports every migration but the first, and a missing baseline: one generated migration holds
/// the complete schema.
fn baseline(run: &mut Run<'_, Options>, generated: &str, ordered: &[Migration]) {
    let Some((first, rest)) = ordered.split_first() else {
        run.report()
            .finding(
                "baseline",
                Location::file(generated),
                "A pre-launch product keeps exactly one generated migration, its baseline, and has none",
            )
            .help("generate the baseline migration");
        return;
    };

    for migration in rest {
        run.report()
            .finding(
                "baseline",
                Location::file(&migration.path),
                format!(
                    "A pre-launch product keeps exactly one generated migration; fold {} into the baseline {} instead of adding a migration",
                    migration.class, first.class
                ),
            )
            .help(format!("fold the change into {}", first.class));
    }
}

/// Reports a generated barrel that differs from one export line per migration in timestamp
/// order.
fn barrel(run: &mut Run<'_, Options>, path: &str, ordered: &[Migration]) {
    let mut expected = String::new();

    for migration in ordered {
        // Writing to a `String` cannot fail.
        let _ = writeln!(
            expected,
            "export {{ {} }} from \"./{}\";",
            migration.class,
            migration.file.trim_end_matches(".ts")
        );
    }

    let help = "regenerate the barrel: one export line per migration in timestamp order";

    match run.tree().bytes(path) {
        Some(actual) => {
            // A barrel that is not UTF-8 text cannot match the expected lines either.
            if actual != expected.as_bytes() {
                run.report()
                    .finding(
                        "barrel",
                        Location::file(path),
                        "The generated migration barrel is not timestamp ordered or does not match the generated migration files",
                    )
                    .help(help);
            }
        }
        None => {
            run.report()
                .finding(
                    "barrel",
                    Location::file(path),
                    "The generated migration barrel is missing",
                )
                .help(help);
        }
    }
}

/// The registry: its path, its constant and the specifier its migrations come from.
struct Registry {
    path: String,
    constant: String,
    import: String,
}

impl Registry {
    /// Reports the registry: missing, or its findings when it has a syntax tree.
    fn report(
        &self,
        run: &mut Run<'_, Options>,
        typescript: &TypeScript<'_>,
        ordered: &[Migration],
    ) -> gate_sdk::Result<()> {
        match typescript.parse(&self.path)? {
            Some(source) if !source.fatal() => {
                for (line, message) in self.findings(&source, ordered) {
                    run.report()
                        .finding("registry", location(&self.path, line), message)
                        .help(format!(
                            "import every generated migration from {} and list them in {} in timestamp order",
                            self.import, self.constant
                        ));
                }
            }
            Some(_) => {}
            None => {
                run.report()
                    .finding(
                        "registry",
                        Location::file(&self.path),
                        "The TypeORM migration registry is missing",
                    )
                    .help("add the registry that lists the generated migrations");
            }
        }

        Ok(())
    }

    /// The registry findings: the migrations imported from the generated folder and the order of
    /// the registry constant.
    fn findings(
        &self,
        source: &SourceFile<'_>,
        ordered: &[Migration],
    ) -> Vec<(Option<u32>, String)> {
        let expected: Vec<&str> = ordered
            .iter()
            .map(|migration| migration.class.as_str())
            .collect();
        let mut imported: Vec<String> = Vec::new();

        for statement in top_level(source) {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };

            if import.source.value != self.import.as_str()
                || import.import_kind == ImportOrExportKind::Type
            {
                continue;
            }

            for specifier in import.specifiers.iter().flatten() {
                if let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier
                    && specifier.import_kind != ImportOrExportKind::Type
                {
                    let name = export_name(&specifier.imported);
                    imported.push(if name == specifier.local.name.as_str() {
                        name
                    } else {
                        format!("{name} as {}", specifier.local.name)
                    });
                }
            }
        }

        if imported.is_empty() {
            return vec![(
                None,
                format!(
                    "The migration registry must import the migrations explicitly from {}",
                    self.import
                ),
            )];
        }

        let registered = top_level(source)
            .flat_map(variable_declarators)
            .find(|declarator| identifier_name(declarator) == Some(self.constant.as_str()))
            .and_then(|declarator| declarator.init.as_ref())
            .and_then(|init| match unwrap(init) {
                Expression::ArrayExpression(array) => Some(array),
                _ => None,
            });
        let Some(registered) = registered else {
            return vec![(
                None,
                format!(
                    "The migration registry must declare {} as an explicit array",
                    self.constant
                ),
            )];
        };

        let mut findings = Vec::new();

        if imported.len() != expected.len()
            || !expected
                .iter()
                .all(|class| imported.iter().any(|name| name == class))
        {
            findings.push((
                None,
                "The migration registry imports do not match the generated migrations".to_owned(),
            ));
        }

        let listed: Vec<String> = registered
            .elements
            .iter()
            .map(|element| match element {
                ArrayExpressionElement::Identifier(identifier) => identifier.name.to_string(),
                other => other.span().source_text(source.source()).to_owned(),
            })
            .collect();

        if listed != expected {
            findings.push((
                Some(source.line(registered.span.start)),
                format!(
                    "{} does not match the timestamp-ordered generated migrations",
                    self.constant
                ),
            ));
        }

        findings
    }
}
