//! `nestjs/error-logging`: in production TypeScript, a native `Logger.error` passes only formatted
//! error evidence and a bounded message, and native `TypeORM` options end with an effective
//! `logging: false` that no local write changes.
//!
//! Each file is read as a compiler program of that file alone reads it: names, the members of
//! `this` and of the file's classes, and imports by module name.

mod logger;
mod options;
mod source;

use crate::shared::paths::{self, Form as PathForm};
use crate::shared::tree;
use std::collections::HashSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, CallExpression, ChainElement, ComputedMemberExpression, Expression, ObjectExpression,
    StaticMemberExpression,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::oxc_syntax::node::NodeId;

use self::logger::Kind;
use self::options::final_false_logging;
use self::source::{Seen, Source};
use crate::layout::{PluginOptions, folder_prefix};
use crate::shared::project;
use crate::shared::report::Findings;

const LOGGER: &str = "logger-error-evidence";
const LOGGING: &str = "typeorm-logging-disabled";

/// The rule.
#[derive(Debug, Default)]
pub struct ErrorLogging;

/// A type a module exports.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct ModuleType {
    /// The module specifier, matched literally.
    pub module: String,
    /// The exported name.
    pub name: String,
}

/// The options of `nestjs/error-logging`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The folders whose `.ts` files are production code; with none given, the source root and the
    /// code folders of the infrastructure-as-code root.
    pub roots: Option<Vec<String>>,
    /// The logger class of `layout.nest-common-package`.
    pub logger_class: String,
    /// The logger's error method.
    pub logger_method: String,
    /// The error evidence formatter of `layout.automator-package`.
    pub evidence_formatter: String,
    /// The types of native ORM options.
    pub orm_options_types: Vec<ModuleType>,
    /// The ORM's data source class of `layout.typeorm-package`, whose first argument is its options.
    pub orm_constructor: String,
    /// The options property that must end as `false`.
    pub logging_property: String,
    /// Library generics whose first type argument is the type they wrap.
    pub wrapper_types: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        let options = |module: &str, name: &str| ModuleType {
            module: module.to_owned(),
            name: name.to_owned(),
        };

        Self {
            roots: None,
            logger_class: "Logger".to_owned(),
            logger_method: "error".to_owned(),
            evidence_formatter: "FormatErrorEvidenceForLog".to_owned(),
            orm_options_types: vec![
                options("typeorm", "DataSourceOptions"),
                options(
                    "typeorm/driver/postgres/PostgresDataSourceOptions",
                    "PostgresDataSourceOptions",
                ),
                options(
                    "typeorm/driver/postgres/PostgresDataSourceOptions.js",
                    "PostgresDataSourceOptions",
                ),
            ],
            orm_constructor: "DataSource".to_owned(),
            logging_property: "logging".to_owned(),
            wrapper_types: ["Promise", "Readonly", "Awaited", "NonNullable", "Extract"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

/// The names the rule looks for: its options and the packages of the layout.
#[derive(Debug)]
pub(super) struct Names {
    nest_package: String,
    automator_package: String,
    orm_package: String,
    logger_class: String,
    logger_method: String,
    evidence_formatter: String,
    orm_options_types: Vec<ModuleType>,
    orm_constructor: String,
    logging_property: String,
    wrapper_types: Vec<String>,
}

impl Options {
    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        paths::check_all("roots", self.roots.iter().flatten(), PathForm::Folder)
    }
}

impl Rule<crate::Nest> for ErrorLogging {
    type Options = Options;

    const NAME: &'static str = "error-logging";
    const TITLE: &'static str =
        "Native error logs carry formatted evidence and native TypeORM logging stays disabled";
    const ASPECTS: &'static [&'static str] = &[LOGGER, LOGGING];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            run.options().check_paths()?;
            let options = run.options();
            let roots = options.roots.clone().unwrap_or_else(|| {
                let mut roots = vec![layout.source_root.clone()];
                roots.extend(
                    layout
                        .cdk_code_folders
                        .iter()
                        .map(|folder| format!("{}{folder}", folder_prefix(&layout.cdk_root))),
                );
                roots
            });
            let names = Names {
                nest_package: layout.nest_common_package.clone(),
                automator_package: layout.automator_package.clone(),
                orm_package: layout.typeorm_package.clone(),
                logger_class: options.logger_class.clone(),
                logger_method: options.logger_method.clone(),
                evidence_formatter: options.evidence_formatter.clone(),
                orm_options_types: options.orm_options_types.clone(),
                orm_constructor: options.orm_constructor.clone(),
                logging_property: options.logging_property.clone(),
                wrapper_types: options.wrapper_types.clone(),
            };
            let typescript = shared.typescript()?;
            // Every `.ts` file but a declaration file, in every folder: the walk skips none.
            let mut paths: Vec<&str> = roots
                .iter()
                .flat_map(|root| tree::files(typescript.source(), root, &tree::UNSKIPPED))
                .filter(|path| {
                    path.strip_suffix(".ts")
                        .is_some_and(|stem| stem.strip_suffix(".d").is_none())
                })
                .collect();
            paths.sort_unstable();
            paths.dedup();
            let mut findings = Findings::default();

            for path in paths {
                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };

                FileCheck {
                    source: Source {
                        file: &file,
                        names: &names,
                        holds: std::cell::RefCell::default(),
                        readings: std::cell::Cell::default(),
                    },
                    findings: &mut findings,
                    objects: Vec::new(),
                    known: HashSet::new(),
                }
                .run();
            }

            findings.report(run, |aspect| {
            if aspect == LOGGER {
                "pass the formatted error evidence and a bounded message, and nothing else"
            } else {
                "end the options object with a literal logging: false and do not change it later"
            }
        });
            Ok(())
        })
    }
}

/// A value as the compiler's tree has it: an optional chain is its member access or call, which
/// the compiler marks optional rather than wrapping.
pub(super) enum Form<'a> {
    Static(&'a StaticMemberExpression<'a>),
    Computed(&'a ComputedMemberExpression<'a>),
    Call(&'a CallExpression<'a>),
    Other(&'a Expression<'a>),
}

pub(super) fn form<'a>(expression: &'a Expression<'a>) -> Form<'a> {
    match expression {
        Expression::StaticMemberExpression(member) => Form::Static(member),
        Expression::ComputedMemberExpression(member) => Form::Computed(member),
        Expression::CallExpression(call) => Form::Call(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => Form::Static(member),
            ChainElement::ComputedMemberExpression(member) => Form::Computed(member),
            ChainElement::CallExpression(call) => Form::Call(call),
            ChainElement::TSNonNullExpression(inner) => match unwrap(&inner.expression) {
                Some(inner) => form(inner),
                None => Form::Other(expression),
            },
            ChainElement::PrivateFieldExpression(_) => Form::Other(expression),
        },
        other => Form::Other(other),
    }
}

/// A value without the wrappers that pass it through: parentheses, type assertions, non-null
/// assertions and `await`.
fn unwrap<'e, 'a>(mut expression: &'e Expression<'a>) -> Option<&'e Expression<'a>> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(inner) => &inner.expression,
            Expression::TSAsExpression(inner) => &inner.expression,
            Expression::TSSatisfiesExpression(inner) => &inner.expression,
            Expression::TSTypeAssertion(inner) => &inner.expression,
            Expression::TSNonNullExpression(inner) => &inner.expression,
            Expression::AwaitExpression(inner) => &inner.argument,
            _ => return Some(expression),
        };
    }
}

struct FileCheck<'f, 'r, 'a> {
    source: Source<'f, 'a>,
    findings: &'r mut Findings,
    /// The native options objects, in the order they were found.
    objects: Vec<&'a ObjectExpression<'a>>,
    /// The starts of `objects`.
    known: HashSet<u32>,
}

impl<'a> FileCheck<'_, '_, 'a> {
    fn error(&mut self, aspect: &'static str, check: &str, start: u32, message: &str) {
        let path = self.source.file.path();
        let line = self.source.file.line(start);
        self.findings.push_keyed(
            aspect,
            path,
            Some(line),
            check,
            format!("{path}:{line} {message}"),
        );
    }

    fn run(mut self) {
        let file = self.source.file;
        let nodes = file.semantic().nodes();
        let mut writes = Vec::new();

        for node in nodes.iter() {
            let kind = node.kind();

            if let Some(target) = self.source.logging_write(kind) {
                writes.push((kind.span().start, target));
            }

            self.node(node.id(), kind);
        }

        for (start, target) in writes {
            if self
                .source
                .objects(target, &mut Seen::new())
                .into_iter()
                .any(|object| object.is_some_and(|object| self.known.contains(&object.span.start)))
            {
                self.error(
                    LOGGING,
                    "mutated",
                    start,
                    "native TypeORM options logging must not be mutated through a materialized object or its local aliases",
                );
            }
        }

        for object in std::mem::take(&mut self.objects) {
            if !final_false_logging(object, &self.source.names.logging_property) {
                self.error(
                    LOGGING,
                    "final-false",
                    object.span.start,
                    "native TypeORM options must end with effective literal logging:false after overriding spreads or computed properties",
                );
            }
        }
    }

    fn node(&mut self, id: NodeId, kind: AstKind<'a>) {
        match kind {
            AstKind::CallExpression(call) => {
                if let Some(invocation) = self.source.invocation(call) {
                    let (evidence, message) = match invocation.kind {
                        Kind::Static => (0, 1),
                        Kind::Instance => (1, 0),
                    };
                    let valid = invocation.arguments.as_deref().is_some_and(|arguments| {
                        arguments.len() == 2
                            && arguments[evidence].is_some_and(|argument| {
                                self.source
                                    .formatter_evidence(argument, &mut Seen::new())
                                    .is_some()
                            })
                            && arguments[message].is_some_and(|argument| {
                                self.source.message(argument, &mut Seen::new())
                            })
                    });

                    if !valid {
                        self.error(
                            LOGGER,
                            "logger-error",
                            call.span.start,
                            "native Logger.error must pass FormatErrorEvidenceForLog(actual error evidence) and a bounded message/context, with no raw additional arguments",
                        );
                    }
                }
            }
            AstKind::VariableDeclarator(declarator) => {
                if let (Some(annotation), Some(init)) =
                    (&declarator.type_annotation, &declarator.init)
                    && self
                        .source
                        .native_options(&annotation.type_annotation, &mut Seen::new())
                    && self
                        .source
                        .objects(init, &mut Seen::new())
                        .iter()
                        .any(Option::is_some)
                {
                    self.add(init);
                }
            }
            AstKind::NewExpression(construction) => {
                if self.source.data_source(&construction.callee)
                    && let Some(first) = construction.arguments.first()
                {
                    self.add_argument(first);
                }
            }
            AstKind::ReturnStatement(statement) => {
                if let Some(argument) = &statement.argument
                    && self.returns_native_options(id)
                {
                    self.add(argument);
                }
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if let Some(annotation) = &arrow.return_type
                    && self
                        .source
                        .native_options(&annotation.type_annotation, &mut Seen::new())
                    && let Some(body) = arrow.body.as_expression()
                {
                    self.add(body);
                }
            }
            _ => {}
        }
    }

    /// Whether the function a return statement returns from is declared to return native options.
    fn returns_native_options(&self, statement: NodeId) -> bool {
        let nodes = self.source.file.semantic().nodes();

        for ancestor in nodes.ancestor_ids(statement) {
            let return_type = match nodes.kind(ancestor) {
                AstKind::Function(function) => &function.return_type,
                AstKind::ArrowFunctionExpression(arrow) => &arrow.return_type,
                _ => continue,
            };

            return return_type.as_ref().is_some_and(|annotation| {
                self.source
                    .native_options(&annotation.type_annotation, &mut Seen::new())
            });
        }

        false
    }

    fn add_argument(&mut self, argument: &'a Argument<'a>) {
        match argument.as_expression() {
            Some(expression) => self.add(expression),
            None => self.error(
                LOGGING,
                "finalized",
                argument.span().start,
                "native TypeORM options factory or constructor must finalize its effective logging:false options locally on every branch",
            ),
        }
    }

    /// Records the options objects a native options value materializes into, reporting a branch
    /// that is not a local object literal.
    fn add(&mut self, expression: &'a Expression<'a>) {
        let objects = self.source.objects(expression, &mut Seen::new());

        if objects.iter().any(Option::is_none) {
            self.error(
                LOGGING,
                "finalized",
                expression.span().start,
                "native TypeORM options factory or constructor must finalize its effective logging:false options locally on every branch",
            );
        }

        for object in objects.into_iter().flatten() {
            if self.known.insert(object.span.start) {
                self.objects.push(object);
            }
        }
    }
}
