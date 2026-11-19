//! `nestjs/dto-decorators`: DTOs use the CRUD automator's property decorators instead of the
//! `OpenAPI` package's `ApiProperty*` ones, and the automator's describe decorator stays in entity
//! folders.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::owners::{call_expression, export_name, unwrap};
use gate_typescript::oxc_ast::ast::{
    AccessorProperty, Class, Decorator, Expression, FormalParameter, ImportDeclarationSpecifier,
    MemberExpression, MethodDefinition, PropertyDefinition, Statement,
};
use gate_typescript::oxc_ast_visit::{Visit, walk};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::syntax::constructor;

use crate::layout::PluginOptions;
use crate::shared::paths::{self, Form};
use crate::shared::{project, tree};

/// The characters JavaScript's `\s` matches.
const JAVASCRIPT_SPACES: [char; 25] = [
    '\t', '\n', '\u{0b}', '\u{0c}', '\r', ' ', '\u{a0}', '\u{1680}', '\u{2000}', '\u{2001}',
    '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}',
    '\u{200a}', '\u{2028}', '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}', '\u{feff}',
];

/// The rule.
#[derive(Debug, Default)]
pub struct DtoDecorators;

/// The options of `nestjs/dto-decorators`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The folder name that marks a DTO file below `layout.modules-root`: a path that contains
    /// `/<dto-folder>/`.
    pub dto_folder: String,
    /// The folder name that marks an entity file, where the describe decorator belongs: a path that
    /// contains `/<entity-folder>/`.
    pub entity_folder: String,
    /// The automator's describe decorator.
    pub describe_decorator: String,
    /// The word that names the `OpenAPI` package's property decorators.
    pub swagger_property_word: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dto_folder: "dto".to_owned(),
            entity_folder: "entity".to_owned(),
            describe_decorator: "ApiPropertyDescribe".to_owned(),
            swagger_property_word: "ApiProperty".to_owned(),
        }
    }
}

impl Rule<crate::Nest> for DtoDecorators {
    type Options = Options;

    const NAME: &'static str = "dto-decorators";
    const TITLE: &'static str =
        "DTOs use Crud Automator property decorators and ApiPropertyDescribe stays in entities";
    const ASPECTS: &'static [&'static str] = &["describe-outside-entity", "swagger-decorator"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let options = run.options();
            paths::check("dto-folder", &options.dto_folder, Form::Name)?;
            paths::check("entity-folder", &options.entity_folder, Form::Name)?;
            let (dto, entity) = (
                format!("/{}/", options.dto_folder),
                format!("/{}/", options.entity_folder),
            );
            let (describe, word) = (
                options.describe_decorator.clone(),
                options.swagger_property_word.clone(),
            );
            let source = shared.source();
            let typescript = shared.typescript()?;
            let settings = typescript.settings();

            let dto_files: Vec<&str> =
                tree::typescript_files(source, &layout.modules_root, settings)
                    .filter(|path| path.contains(dto.as_str()))
                    .collect();

            for path in dto_files {
                let Some(bytes) = shared.bytes(path) else {
                    continue;
                };

                if imports_swagger_property(
                    &String::from_utf8_lossy(&bytes),
                    &layout.swagger_package,
                    &word,
                ) {
                    run.report()
                        .finding(
                            "swagger-decorator",
                            Location::file(path),
                            format!(
                                "{path} imports Nest Swagger {word} decorators in a DTO folder"
                            ),
                        )
                        .help(format!(
                            "use the {} property decorators in DTOs",
                            layout.automator_package
                        ))
                        .key("swagger-import");
                }
            }

            let sources: Vec<&str> = tree::typescript_files(source, &layout.source_root, settings)
                .filter(|path| !path.contains(entity.as_str()))
                .collect();

            for path in sources {
                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };

                if let Some(start) = describe_decorator(&file, &layout.automator_package, &describe)
                {
                    run.report()
                    .finding(
                        "describe-outside-entity",
                        Location::line(
                            path,
                            usize::try_from(file.line(start)).unwrap_or(usize::MAX),
                        ),
                        format!("{path} uses {describe} outside an entity folder"),
                    )
                    .help(
                        "describe the property on its entity; DTOs take the description from there",
                    )
                    .key("describe");
                }
            }

            Ok(())
        })
    }
}

/// Whether a source text imports from `@nestjs/swagger` and names an `ApiProperty*` word, read as
/// two text patterns rather than through the syntax tree.
fn imports_swagger_property(content: &str, package: &str, word: &str) -> bool {
    let imports = content.match_indices("from").any(|(offset, _)| {
        let rest = &content[offset + "from".len()..];
        let trimmed = rest.trim_start_matches(JAVASCRIPT_SPACES);

        trimmed.len() < rest.len()
            && ["\"", "'"].iter().any(|open| {
                trimmed.strip_prefix(open).is_some_and(|tail| {
                    tail.strip_prefix(package)
                        .is_some_and(|tail| tail.starts_with(['"', '\'']))
                })
            })
    });

    imports
        && content.match_indices(word).any(|(offset, _)| {
            !content[..offset]
                .bytes()
                .next_back()
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

/// The start of the `ApiPropertyDescribe` decorator that comes first, if any.
///
/// The walk visits the syntax tree from the last child to the first and checks a node's own
/// decorators, in source order, before its children: a decorated node is found before the decorated
/// nodes it contains, and of two unrelated ones the later.
fn describe_decorator(file: &SourceFile<'_>, package: &str, describe: &str) -> Option<u32> {
    let mut finder = DescribeFinder {
        describe: describe.to_owned(),
        names: BTreeSet::new(),
        namespaces: BTreeSet::new(),
        owners: Vec::new(),
        next: 0,
        found: Vec::new(),
    };

    for statement in &file.program().body {
        let Statement::ImportDeclaration(import) = statement else {
            continue;
        };

        if import.source.value != package {
            continue;
        }

        for specifier in import.specifiers.iter().flatten() {
            match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(specifier)
                    if export_name(&specifier.imported) == describe =>
                {
                    finder.names.insert(specifier.local.name.to_string());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(specifier) => {
                    finder.namespaces.insert(specifier.local.name.to_string());
                }
                _ => {}
            }
        }
    }

    if finder.names.is_empty() && finder.namespaces.is_empty() {
        return None;
    }

    finder.visit_program(file.program());

    finder
        .found
        .iter()
        // An owner holds at most one found decorator, its first that calls `ApiPropertyDescribe`.
        .min_by(|left, right| {
            let (left_owner, right_owner) = (&left.owner, &right.owner);

            if left_owner.contains(right_owner) {
                std::cmp::Ordering::Less
            } else if right_owner.contains(left_owner) {
                std::cmp::Ordering::Greater
            } else {
                right_owner.start.cmp(&left_owner.start)
            }
        })
        .map(|found| found.start)
}

/// A node that can carry decorators: its preorder number, the last number of its subtree and its
/// start.
#[derive(Clone, Copy, Debug)]
struct Owner {
    id: usize,
    last: usize,
    start: u32,
}

impl Owner {
    fn contains(&self, other: &Self) -> bool {
        self.id < other.id && other.id <= self.last
    }
}

#[derive(Debug)]
struct Found {
    owner: Owner,
    start: u32,
}

struct DescribeFinder {
    /// The describe decorator.
    describe: String,
    names: BTreeSet<String>,
    namespaces: BTreeSet<String>,
    /// The owners being walked, innermost last.
    owners: Vec<Owner>,
    next: usize,
    found: Vec<Found>,
}

impl DescribeFinder {
    fn enter(&mut self, start: u32) {
        self.owners.push(Owner {
            id: self.next,
            last: self.next,
            start,
        });
        self.next += 1;
    }

    fn exit(&mut self) {
        if let Some(owner) = self.owners.pop() {
            let last = self.next - 1;

            for found in &mut self.found {
                if found.owner.id == owner.id {
                    found.owner.last = last;
                }
            }
        }
    }

    /// Whether a decorator calls `ApiPropertyDescribe` through a named or namespace import.
    fn describes(&self, decorator: &Decorator<'_>) -> bool {
        let Some(call) = call_expression(unwrap(&decorator.expression)) else {
            return false;
        };

        let callee = unwrap(&call.callee);

        if let Expression::Identifier(callee) = callee {
            return self.names.contains(callee.name.as_str());
        }

        let member = match callee {
            Expression::ChainExpression(chain) => chain.expression.as_member_expression(),
            callee => callee.as_member_expression(),
        };

        member.is_some_and(|member| {
            let name = match member {
                MemberExpression::StaticMemberExpression(member) => {
                    Some(member.property.name.as_str())
                }
                MemberExpression::ComputedMemberExpression(member) => match &member.expression {
                    Expression::StringLiteral(name) => Some(name.value.as_str()),
                    _ => None,
                },
                MemberExpression::PrivateFieldExpression(_) => None,
            };

            name == Some(self.describe.as_str())
                && matches!(member.object(), Expression::Identifier(object)
                    if self.namespaces.contains(object.name.as_str()))
        })
    }

    fn decorators(&mut self, decorators: &[Decorator<'_>]) {
        let Some(owner) = self.owners.last().copied() else {
            return;
        };

        if let Some(decorator) = decorators
            .iter()
            .find(|decorator| self.describes(decorator))
        {
            self.found.push(Found {
                owner,
                start: decorator.span.start,
            });
        }
    }
}

impl<'a> Visit<'a> for DescribeFinder {
    fn visit_class(&mut self, class: &Class<'a>) {
        self.enter(class.span.start);
        self.decorators(&class.decorators);
        walk::walk_class(self, class);
        self.exit();
    }

    fn visit_method_definition(&mut self, method: &MethodDefinition<'a>) {
        // The compiler reads no decorators on a constructor.
        if constructor(method) {
            walk::walk_method_definition(self, method);
            return;
        }

        self.enter(method.span.start);
        self.decorators(&method.decorators);
        walk::walk_method_definition(self, method);
        self.exit();
    }

    fn visit_property_definition(&mut self, property: &PropertyDefinition<'a>) {
        self.enter(property.span.start);
        self.decorators(&property.decorators);
        walk::walk_property_definition(self, property);
        self.exit();
    }

    fn visit_accessor_property(&mut self, property: &AccessorProperty<'a>) {
        self.enter(property.span.start);
        self.decorators(&property.decorators);
        walk::walk_accessor_property(self, property);
        self.exit();
    }

    fn visit_formal_parameter(&mut self, parameter: &FormalParameter<'a>) {
        self.enter(parameter.span().start);
        self.decorators(&parameter.decorators);
        walk::walk_formal_parameter(self, parameter);
        self.exit();
    }
}
