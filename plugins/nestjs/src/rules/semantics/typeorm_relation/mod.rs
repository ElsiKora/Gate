//! `nestjs/typeorm-relation`: every application-owned foreign key has persisted scalars and exactly
//! one owning relation typed through `Relation<Target>` that declares its complete join and one
//! semantic foreign key name, and no entity uses a banned decorator (`@ForeignKey`, `@RelationId`,
//! inverse collections).
//!
//! Decorators count only when they resolve to a runtime import from the `TypeORM` package (by name,
//! alias or namespace) through the TypeScript binder; a relation target resolves to an exported
//! same-file `@Entity` or through one runtime named import and explicit named re-exports to one; a
//! scalar type is a primitive, an intrinsic `Date` or a repository enum. Every tracked `.ts` file
//! below `layout.source-root` is read, declaration files and every folder included. Specifiers
//! resolve relatively or through the `layout.path-aliases` entries `resolved-aliases` names, never
//! through tsconfig.

mod catalog;
mod decorators;
mod grammar;
mod names;
mod relation;
mod types;

use std::num::NonZeroUsize;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::binder::Binder;
use gate_typescript::oxc_ast::ast::{ClassElement, PropertyKey};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::syntax::{computed_key_text, constructor, member_start};
use gate_typescript::value::number_text;
use gate_typescript::{Allocator, SourceFile, TypeScript};

use crate::layout::{Layout, PluginOptions};
use crate::shared::{project, tree};
use catalog::{Catalog, Unit};
use decorators::Bindings;
use relation::{Pass, Reuse};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The case a semantic foreign key name is spelled in.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Case {
    /// `FK_` followed by lower-case words joined by `_`.
    Upper,
    /// `fk_` followed by lower-case words joined by `_`.
    Lower,
}

/// The primitive type of both components of a nullable relation's join.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(crate = "gate_sdk::serde", rename_all = "kebab-case")]
#[schemars(crate = "gate_sdk::schemars")]
pub enum Primitive {
    /// `bigint`.
    Bigint,
    /// `boolean`.
    Boolean,
    /// `number`.
    Number,
    /// `string`.
    String,
}

impl Primitive {
    /// The keyword.
    fn keyword(self) -> &'static str {
        match self {
            Self::Bigint => "bigint",
            Self::Boolean => "boolean",
            Self::Number => "number",
            Self::String => "string",
        }
    }
}

/// One relation whose relation metadata is nullable while its typed property and its one join
/// component are not, as an insertion cycle of a self relation needs. The rule checks this contract
/// and reports a relation that departs from it; the option/property nullability mismatch the
/// contract allows is still reported, with the target `<path>:<class>.<member>`, for an exception
/// to accept.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct NullableRelation {
    /// The file of the owning class.
    pub path: String,
    /// The owning class.
    pub class: String,
    /// The relation member.
    pub member: String,
    /// The file of the target entity.
    pub target_path: String,
    /// The target entity class.
    pub target_class: String,
    /// The local column of the one join component.
    pub local_name: String,
    /// The referenced column of the one join component.
    pub referenced_name: String,
    /// The primitive type of both join components.
    pub scalar: Primitive,
    /// The relation's `onDelete`.
    pub on_delete: String,
    /// The relation's `onUpdate`.
    pub on_update: String,
    /// Why the relation is nullable, as the message of a departing relation names it.
    #[serde(default)]
    pub reason: Option<String>,
}

/// The options of `nestjs/typeorm-relation`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The `layout.path-aliases` keys import specifiers resolve through.
    pub resolved_aliases: Vec<String>,
    /// The case of a semantic foreign key name; without it no name is checked, and a relation whose
    /// join reaches its foreign key name stops the run.
    pub foreign_key_case: Option<Case>,
    /// The word the foreign key name message prints after the pattern; required with
    /// `foreign-key-case`.
    pub foreign_key_label: Option<String>,
    /// The `TypeORM` package the decorators and the `Relation` wrapper come from. Absent means
    /// `layout.typeorm-package`.
    pub package: Option<String>,
    /// The decorators no entity may use.
    pub banned_decorators: Vec<String>,
    /// The decorators that declare a persisted column.
    pub column_decorators: Vec<String>,
    /// The decorators that declare an owning relation.
    pub owning_relations: Vec<String>,
    /// The values `onDelete` and `onUpdate` may take.
    pub relation_actions: Vec<String>,
    /// The values `deferrable` may take.
    pub relation_deferrability: Vec<String>,
    /// The role prefixes a sole relation to its target must not carry.
    pub mutable_roles: Vec<String>,
    /// A foreign key name must be shorter than this many UTF-8 bytes.
    pub max_foreign_key_bytes: NonZeroUsize,
    /// The relations whose relation metadata is nullable while their property and join are not.
    pub nullable_relations: Vec<NullableRelation>,
}

/// Owned strings of a list of literals.
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

impl Default for Options {
    fn default() -> Self {
        Self {
            resolved_aliases: strings(&["@modules"]),
            foreign_key_case: None,
            foreign_key_label: None,
            package: None,
            banned_decorators: strings(&["ForeignKey", "ManyToMany", "OneToMany", "RelationId"]),
            column_decorators: strings(&[
                "Column",
                "CreateDateColumn",
                "DeleteDateColumn",
                "PrimaryColumn",
                "PrimaryGeneratedColumn",
                "UpdateDateColumn",
                "VersionColumn",
            ]),
            owning_relations: strings(&["ManyToOne", "OneToOne"]),
            relation_actions: strings(&["CASCADE", "NO ACTION", "RESTRICT", "SET NULL"]),
            relation_deferrability: strings(&["INITIALLY DEFERRED", "INITIALLY IMMEDIATE"]),
            mutable_roles: strings(&["active", "current", "default", "selected"]),
            max_foreign_key_bytes: NonZeroUsize::new(63).unwrap_or(NonZeroUsize::MIN),
            nullable_relations: Vec::new(),
        }
    }
}

/// The aspects, sorted.
const ASPECTS: &[&str] = &[
    "column-declaration",
    "foreign-key-name",
    "foreign-key-reuse",
    "join-column",
    "join-column-name",
    "join-column-reference",
    "join-column-type",
    "nullability-exception",
    "prohibited-decorator",
    "relation-name",
    "relation-nullability",
    "relation-options",
    "relation-owner",
    "relation-target",
    "relation-type",
    "relation-wrapper",
];

/// The foreign key spelling: its case and the word the message prints.
#[derive(Clone, Copy, Debug)]
pub(super) struct Spelling<'v> {
    pub case: Case,
    pub label: &'v str,
}

/// What the options make of the vocabulary the check reads.
#[derive(Debug)]
pub(super) struct Vocabulary {
    pub package: String,
    pub banned: Vec<String>,
    pub columns: Vec<String>,
    pub owning: Vec<String>,
    /// Fixed decorators: the entity and the join column.
    pub others: [String; 2],
    pub actions: Vec<String>,
    pub deferrability: Vec<String>,
    pub mutable_roles: Vec<String>,
    pub max_foreign_key_bytes: usize,
    /// The followed aliases and the folder each names.
    pub aliases: Vec<(String, String)>,
}

impl Vocabulary {
    /// The vocabulary of `options` and the folders of the followed aliases of `layout`.
    ///
    /// # Errors
    /// When `foreign-key-case` comes without `foreign-key-label`, or `resolved-aliases` names an
    /// alias `layout.path-aliases` does not define.
    fn new(options: &Options, layout: &Layout) -> gate_sdk::Result<Self> {
        if options.foreign_key_case.is_some() && options.foreign_key_label.is_none() {
            return Err(gate_sdk::Error::new(
                "the option foreign-key-case needs foreign-key-label, the word the foreign key name message prints after the pattern; set foreign-key-label",
            ));
        }

        let mut aliases = Vec::new();

        for alias in &options.resolved_aliases {
            let Some(folder) = layout.path_aliases.get(alias) else {
                return Err(gate_sdk::Error::new(format!(
                    "the option resolved-aliases names {alias}, which [plugins.nestjs] options layout.path-aliases does not define; define it there or remove it"
                )));
            };
            aliases.push((alias.clone(), folder.clone()));
        }

        Ok(Self {
            package: super::package::resolve(
                "package",
                options.package.as_deref(),
                &layout.typeorm_package,
            )?,
            banned: options.banned_decorators.clone(),
            columns: options.column_decorators.clone(),
            owning: options.owning_relations.clone(),
            others: ["Entity".to_owned(), "JoinColumn".to_owned()],
            actions: options.relation_actions.clone(),
            deferrability: options.relation_deferrability.clone(),
            mutable_roles: options.mutable_roles.clone(),
            max_foreign_key_bytes: options.max_foreign_key_bytes.get(),
            aliases,
        })
    }

    /// The canonical name of a recognized decorator.
    pub(super) fn recognized(&self, name: &str) -> Option<&str> {
        self.banned
            .iter()
            .chain(&self.columns)
            .chain(&self.owning)
            .chain(&self.others)
            .find(|known| *known == name)
            .map(String::as_str)
    }

    /// The owning relations as a message lists them: `@ManyToOne or @OneToOne`.
    pub(super) fn owning_text(&self) -> String {
        self.owning
            .iter()
            .map(|name| format!("@{name}"))
            .collect::<Vec<_>>()
            .join(" or ")
    }
}

/// One finding before the message order is applied.
#[derive(Debug)]
pub(super) struct Finding {
    aspect: &'static str,
    path: String,
    line: u32,
    message: String,
    target: Option<String>,
    key: String,
}

impl Finding {
    fn new(aspect: &'static str, path: &str, line: u32, key: String, message: String) -> Self {
        Self {
            aspect,
            path: path.to_owned(),
            line,
            message,
            target: None,
            key,
        }
    }
}

/// A text as `JSON.stringify` prints it.
pub(super) fn quote(text: &str) -> String {
    gate_sdk::serde_json::Value::String(text.to_owned()).to_string()
}

/// The name of a noncomputed identifier, private name, string or number key, as its `.text` reads.
pub(super) fn literal_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    if computed {
        return None;
    }

    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::PrivateIdentifier(identifier) => Some(format!("#{}", identifier.name)),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::NumericLiteral(literal) => Some(number_text(literal.value)),
        _ => None,
    }
}

/// A literal name, the source text of any other name with its brackets, or
/// `<unnamed@line>` for a member without a name.
pub(super) fn member_name(file: &SourceFile<'_>, element: &ClassElement<'_>) -> String {
    let key = match element {
        ClassElement::MethodDefinition(method) if !constructor(method) => {
            Some((&method.key, method.computed))
        }
        ClassElement::PropertyDefinition(property) => Some((&property.key, property.computed)),
        ClassElement::AccessorProperty(property) => Some((&property.key, property.computed)),
        _ => None,
    };
    let Some((key, computed)) = key else {
        return format!("<unnamed@{}>", file.line(member_start(element)));
    };

    literal_name(key, computed).unwrap_or_else(|| {
        let span = key.span();

        if computed {
            computed_key_text(file, span)
        } else {
            file.source()[span.start as usize..span.end as usize].to_owned()
        }
    })
}

/// The fix hint of an aspect.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "column-declaration" => {
            "declare the column with a supported literal overload: (), (\"type\"), ({...}) or (\"type\", {...})"
        }
        "foreign-key-name" => {
            "name the foreign key semantically in the configured case, shorter than the byte limit"
        }
        "foreign-key-reuse" => "give each owning relation its own foreign key name",
        "join-column" => {
            "pass @JoinColumn one inline object, or an inline array of them, with name, referencedColumnName and, first, foreignKeyConstraintName"
        }
        "join-column-name" => "join through the relation's own persisted member-id column",
        "join-column-reference" => {
            "name exactly one persisted column of each side in the join item"
        }
        "join-column-type" => {
            "declare both join columns with the same closed scalar type, not optional"
        }
        "nullability-exception" => {
            "make the relation meet its nullable-relations entry, or change the entry"
        }
        "prohibited-decorator" => "remove the decorator; own the foreign key on an owning relation",
        "relation-name" => "name the sole relation after its target resource",
        "relation-nullability" => {
            "make the nullable option agree with the property type and the join columns"
        }
        "relation-options" => {
            "pass one inline options object with nullable, onDelete and onUpdate literals"
        }
        "relation-owner" => {
            "give the member exactly one owning relation decorator and one @JoinColumn"
        }
        "relation-target" => {
            "target an exported @Entity class with a direct arrow, imported by name at runtime"
        }
        "relation-type" => "type the property as Relation<Target>, or Relation<Target> | null",
        _ => "wrap the target as Relation<Target>, imported by name from the TypeORM package",
    }
}

/// Checks that the paths of `nullable-relations` are normalized file paths.
///
/// # Errors
/// At the first path that is not, naming its entry.
fn check_paths(options: &Options) -> gate_sdk::Result<()> {
    use crate::shared::paths::{Form::File, check};

    for (index, relation) in options.nullable_relations.iter().enumerate() {
        check(
            &format!("nullable-relations[{index}].path"),
            &relation.path,
            File,
        )?;
        check(
            &format!("nullable-relations[{index}].target-path"),
            &relation.target_path,
            File,
        )?;
    }

    Ok(())
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "typeorm-relation";
    const TITLE: &'static str =
        "TypeORM relations own their foreign keys with complete, semantically named joins";
    const ASPECTS: &'static [&'static str] = ASPECTS;
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        check_paths(run.options())?;

        let layout = PluginOptions::read(run)?.layout;
        let options = run.options();
        let vocabulary = Vocabulary::new(options, &layout)?;
        let label = options.foreign_key_label.clone();
        let exceptions = options.nullable_relations.clone();
        let spelling = options
            .foreign_key_case
            .zip(label.as_deref())
            .map(|(case, label)| Spelling { case, label });
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let mut files = Vec::new();

        for path in
            tree::typescript_files(typescript.source(), &layout.source_root, &tree::UNSKIPPED)
        {
            if let Some(file) = typescript
                .parse(path)
                .map_err(|failure| project::error(&failure))?
            {
                files.push((path, file));
            }
        }

        // A file oxc cannot parse keeps its path for the import candidates and is not read;
        // `typescript/parse` reports it.
        let units = files
            .iter()
            .map(|(path, file)| Unit {
                path,
                file,
                resolver: (!file.fatal())
                    .then(|| Bindings::read(file, &vocabulary))
                    .flatten()
                    .map(|bindings| (bindings, Binder::new(file))),
            })
            .collect();
        let catalog = Catalog::new(units, &vocabulary);
        let mut findings = Vec::new();
        let mut reuse = Reuse::new();
        let mut pass = Pass {
            catalog: &catalog,
            vocabulary: &vocabulary,
            spelling,
            exceptions: &exceptions,
            findings: &mut findings,
            reuse: &mut reuse,
        };
        pass.files();

        if spelling.is_none() && !reuse.is_empty() {
            return Err(gate_sdk::Error::new(
                "the option foreign-key-case is not set, but an owning relation's join reaches its foreign key name; set foreign-key-case to \"upper\" or \"lower\" and foreign-key-label",
            ));
        }

        for (foreign_key, owners) in reuse {
            let Some((_, (path, line))) = owners.first_key_value() else {
                continue;
            };

            if owners.len() < 2 {
                continue;
            }

            let labels: Vec<&str> = owners.keys().map(String::as_str).collect();
            findings.push(Finding::new(
                "foreign-key-reuse",
                path,
                *line,
                foreign_key.clone(),
                format!(
                    "foreign key name {} is reused across owner relations: {}",
                    quote(&foreign_key),
                    labels.join(", ")
                ),
            ));
        }

        findings.sort_by(|left, right| {
            (&left.message, &left.path, left.line).cmp(&(&right.message, &right.path, right.line))
        });

        for finding in findings {
            let reported = run
                .report()
                .finding(
                    finding.aspect,
                    Location::line(finding.path, finding.line as usize),
                    finding.message,
                )
                .help(help(finding.aspect))
                .key(finding.key);

            if let Some(target) = finding.target {
                reported.target(target);
            }
        }

        Ok(())
    }
}
