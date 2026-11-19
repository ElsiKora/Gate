//! `nestjs/wrappers`: no function, method or field only forwards its parameters to one call or only
//! throws or returns one plain or Nest error construction, and no Nest exception is constructed
//! without a message of the message factory.
//!
//! The rule reads every tracked `.ts` file below `layout.source-root` (not `.d.ts`); a file without
//! a syntax tree is skipped, because `typescript/parse` reports it. Values are followed across the
//! files below the source root through `const` bindings, destructuring, property reads, imports and
//! re-exports; imports of the known packages resolve through the package export table.

mod exception;
mod guard;
mod packages;
mod provenance;
mod shape;
mod target;
mod throw_only;

use std::collections::BTreeMap;
use std::rc::Rc;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Report, Result, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    ArrowFunctionBody, ArrowFunctionExpression, BindingPattern, CallExpression, Class, Decorator,
    Expression, FormalParameters, Function, FunctionType, MethodDefinition, MethodDefinitionKind,
    NewExpression, PropertyKey, TSTypeAnnotation,
};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::node::NodeId;
use gate_typescript::value::unwrap;
use gate_typescript::{Allocator, SourceFile, TypeScript};

use self::exception::{factory_message, nest_exception};
use self::packages::Packages;
use self::provenance::Provenance;
use self::shape::{
    expression_call, forwarding_call, framework_boundary, intrinsic_alias, statement_call,
};
use self::target::Exemptions;
use self::throw_only::{Body, Constructions, Exit, exhaustiveness_guard};
use crate::layout::PluginOptions;
use crate::shared::{project, syntax, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The message factory: the call whose result is the message of a Nest exception.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct MessageFactory {
    /// The package that exports the factory. Absent means `layout.automator-package`.
    pub package: Option<String>,
    /// The factory's export name.
    pub name: String,
}

impl Default for MessageFactory {
    fn default() -> Self {
        Self {
            package: None,
            name: "ErrorString".to_owned(),
        }
    }
}

/// A class that adapts a reporter port: its method is a framework boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct ReporterAdapter {
    /// The module the port class is imported from, as the import names it.
    pub module: String,
    /// The port class's export name, which the adapter class extends.
    pub class: String,
    /// The adapter's method that is a boundary.
    pub method: String,
    /// The path prefix of the files whose classes may adapt the port.
    pub path_prefix: String,
}

/// The options of `nestjs/wrappers`.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The packages whose exports named with the exception suffix are Nest exceptions.
    pub exception_packages: Vec<String>,
    /// The suffix of a Nest exception's export name.
    pub exception_suffix: String,
    /// The message factory.
    pub message_factory: MessageFactory,
    /// The built-in errors that are plain error constructions when no local binding shadows them.
    pub plain_errors: Vec<String>,
    /// Built-in calls, each an object and a method, that build a value from a new literal and are
    /// the `intrinsic-alias` aspect when a wrapper only forwards to them.
    pub intrinsic_calls: Vec<[String; 2]>,
    /// Methods that are framework boundaries in every class.
    pub lifecycle_methods: Vec<String>,
    /// Methods that are framework boundaries in the files whose path contains the key, such as
    /// `/guard/`.
    pub framework_methods: BTreeMap<String, Vec<String>>,
    /// Classes that adapt a reporter port.
    pub reporter_adapters: Vec<ReporterAdapter>,
    /// The tracked file that lists the value exports of the known packages; absent means none.
    pub package_exports_file: Option<String>,
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

impl Default for Options {
    fn default() -> Self {
        Self {
            exception_packages: strings(&["@nestjs/common", "@nestjs/throttler"]),
            exception_suffix: "Exception".to_owned(),
            message_factory: MessageFactory::default(),
            plain_errors: strings(&[
                "AggregateError",
                "Error",
                "EvalError",
                "RangeError",
                "ReferenceError",
                "SyntaxError",
                "TypeError",
                "URIError",
            ]),
            intrinsic_calls: vec![
                ["JSON".to_owned(), "stringify".to_owned()],
                ["Object".to_owned(), "freeze".to_owned()],
            ],
            lifecycle_methods: strings(&[
                "beforeApplicationShutdown",
                "onApplicationBootstrap",
                "onApplicationShutdown",
                "onModuleDestroy",
                "onModuleInit",
            ]),
            framework_methods: framework_methods(),
            reporter_adapters: Vec::new(),
            package_exports_file: None,
        }
    }
}

fn framework_methods() -> BTreeMap<String, Vec<String>> {
    let subscriber: Vec<String> = ["Commit", "Create", "Custom", "Delete", "Get", "GetList"]
        .into_iter()
        .chain(["GetMany", "PartialUpdate", "Rollback", "Update"])
        .flat_map(|event| {
            [
                format!("onAfter{event}"),
                format!("onAfterError{event}"),
                format!("onBefore{event}"),
                format!("onBeforeError{event}"),
            ]
        })
        .filter(|method| method != "onBeforeCommit" && method != "onBeforeRollback")
        .collect();

    [
        (
            "/policy/",
            strings(&[
                "getCustomActionRule",
                "onBeforeCreate",
                "onBeforeDelete",
                "onBeforeGet",
                "onBeforeGetList",
                "onBeforePartialUpdate",
                "onBeforeUpdate",
            ]),
        ),
        ("/subscriber/", subscriber),
        (
            "/guard/",
            strings(&[
                "canActivate",
                "getAuthenticateOptions",
                "getRequest",
                "handleRequest",
                "logIn",
            ]),
        ),
        ("/filter/", strings(&["catch"])),
        ("/interceptor/", strings(&["intercept"])),
        ("/migration/typeorm/", strings(&["down", "up"])),
        ("/resolver/", strings(&["resolve"])),
        ("/pipe/", strings(&["transform"])),
        ("/middleware/", strings(&["use"])),
        ("/strategy/", strings(&["validate"])),
    ]
    .into_iter()
    .map(|(segment, mut methods)| {
        methods.sort();
        (segment.to_owned(), methods)
    })
    .collect()
}

/// The options with the message factory's package resolved against the layout.
#[derive(Debug)]
pub struct Config {
    options: Options,
    factory_package: String,
}

impl gate_sdk::Rule for Rule {
    type Options = Options;

    const NAME: &'static str = "wrappers";
    const TITLE: &'static str = "No forwarding or throw-only wrappers and no raw Nest exceptions";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const ASPECTS: &'static [&'static str] = &[
        "intrinsic-alias",
        "one-call-forwarding",
        "raw-nest-exception",
        "throw-only-error",
    ];

    fn check(&self, run: &mut Run<'_, Options>) -> Result<()> {
        {
            use crate::shared::paths::Form::File;
            let o = run.options();
            if let Some(file) = &o.package_exports_file {
                crate::shared::paths::check("package-exports-file", file, File)?;
            }
        }

        let layout = PluginOptions::read(run)?.layout;
        let options = run.options().clone();
        let factory_package = super::package::resolve(
            "message-factory.package",
            options.message_factory.package.as_deref(),
            &layout.automator_package,
        )?;
        let packages = packages(run, &options, &factory_package)?;
        let config = Config {
            options,
            factory_package,
        };
        let exemptions = Exemptions::new(run.exceptions());
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let provenance = Provenance::new(&typescript, &packages, &layout.source_root);
        let paths = syntax::typescript_sources(tree::files(
            typescript.source(),
            &layout.source_root,
            &tree::UNSKIPPED,
        ));

        for path in paths {
            let Some(file) = project::parsed(&typescript, path)? else {
                continue;
            };
            let constructions = Constructions {
                config: &config,
                provenance: &provenance,
                exemptions: &exemptions,
                file: &file,
            };
            FileCheck {
                file: &file,
                constructions: &constructions,
                comments: file
                    .program()
                    .comments
                    .iter()
                    .map(|comment| (comment.span.start as usize, comment.span.end as usize))
                    .collect(),
            }
            .run(run.report())?;
        }

        Ok(())
    }
}

/// The known packages: the table of `package-exports-file`, or without it the exception packages
/// and the message factory's package, each exporting every name.
fn packages(run: &Run<'_, Options>, options: &Options, factory_package: &str) -> Result<Packages> {
    let Some(path) = options.package_exports_file.as_deref() else {
        return Ok(Packages::open(
            options
                .exception_packages
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(factory_package)),
        ));
    };

    let text = run.tree().text(path).ok_or_else(|| {
        gate_sdk::Error::config(format!(
            "the package-exports-file {path} is not a tracked text file; generate and commit it, or remove the option"
        ))
    })?;

    Packages::parse(path, &text).map_err(gate_sdk::Error::new)
}

/// A function-like owner of a body.
struct Callable<'a> {
    parameters: &'a FormalParameters<'a>,
    body: Body<'a>,
    return_type: Option<&'a TSTypeAnnotation<'a>>,
}

impl<'a> Callable<'a> {
    fn function(function: &'a Function<'a>) -> Option<Self> {
        Some(Self {
            parameters: &function.params,
            body: Body::Block(&function.body.as_ref()?.statements),
            return_type: function.return_type.as_deref(),
        })
    }

    fn arrow(arrow: &'a ArrowFunctionExpression<'a>) -> Self {
        Self {
            parameters: &arrow.params,
            body: match &arrow.body {
                ArrowFunctionBody::FunctionBody(body) => Body::Block(&body.statements),
                body => match body.as_expression() {
                    Some(expression) => Body::Expression(expression),
                    None => Body::Block(&[]),
                },
            },
            return_type: arrow.return_type.as_deref(),
        }
    }

    /// A function or arrow function expression, through type assertions and parentheses.
    fn expression(expression: &'a Expression<'a>) -> Option<Self> {
        match unwrap(expression) {
            Expression::ArrowFunctionExpression(arrow) => Some(Self::arrow(arrow)),
            Expression::FunctionExpression(function) => Self::function(function),
            _ => None,
        }
    }

    /// The call of a body that is one call statement.
    fn call(&self) -> Option<&'a CallExpression<'a>> {
        match self.body {
            Body::Expression(expression) => expression_call(expression),
            Body::Block([statement]) => statement_call(statement),
            Body::Block(_) => None,
        }
    }
}

/// The members of a class or object literal the rule reads.
enum Owner<'a> {
    Method {
        class: Option<&'a Class<'a>>,
        decorators: &'a [Decorator<'a>],
        definition: Option<&'a MethodDefinition<'a>>,
    },
    Field {
        class: Option<&'a Class<'a>>,
        decorators: &'a [Decorator<'a>],
    },
    Function,
}

struct FileCheck<'c, 'p, 't, 'a> {
    file: &'c Rc<SourceFile<'a>>,
    constructions: &'c Constructions<'p, 't, 'a>,
    /// The start and end of each comment of the file, in source order.
    comments: Vec<(usize, usize)>,
}

impl<'a> FileCheck<'_, '_, '_, 'a> {
    fn run(&self, report: &mut Report) -> Result<()> {
        let semantic = self.file.semantic();

        for node in semantic.nodes().iter() {
            self.node(node.id(), node.kind(), report)?;
        }

        Ok(())
    }

    fn config(&self) -> &Config {
        self.constructions.config
    }

    /// Checks one node: a construction, or a method, function or field with a body.
    fn node(&self, id: NodeId, kind: AstKind<'a>, report: &mut Report) -> Result<()> {
        match kind {
            AstKind::NewExpression(construction) => {
                self.construction(construction, report)?;
            }
            AstKind::MethodDefinition(method)
                if method.kind == MethodDefinitionKind::Method && method.value.body.is_some() =>
            {
                let class = self.class(id);
                let Some(callable) = Callable::function(&method.value) else {
                    return Ok(());
                };
                self.member(
                    method.span,
                    &key_text(self.file, &self.comments, &method.key, method.computed),
                    &callable,
                    &Owner::Method {
                        class,
                        decorators: &method.decorators,
                        definition: Some(method),
                    },
                    report,
                )?;
            }
            AstKind::ObjectProperty(property) if property.method => {
                let Expression::FunctionExpression(function) = &property.value else {
                    return Ok(());
                };
                let Some(callable) = Callable::function(function) else {
                    return Ok(());
                };
                self.member(
                    property.span,
                    &key_text(self.file, &self.comments, &property.key, property.computed),
                    &callable,
                    &Owner::Method {
                        class: None,
                        decorators: &[],
                        definition: None,
                    },
                    report,
                )?;
            }
            AstKind::Function(function)
                if function.r#type == FunctionType::FunctionDeclaration
                    && function.body.is_some() =>
            {
                let (Some(identifier), Some(callable)) =
                    (&function.id, Callable::function(function))
                else {
                    return Ok(());
                };
                // The declaration starts at its `export` keyword.
                let start = match self.file.semantic().nodes().parent_kind(id) {
                    AstKind::ExportDeclaration(export) => export.span,
                    AstKind::ExportDefaultDeclaration(export) => export.span,
                    _ => function.span,
                };
                self.member(start, &identifier.name, &callable, &Owner::Function, report)?;
            }
            AstKind::VariableDeclarator(declarator) => {
                let (BindingPattern::BindingIdentifier(id), Some(init)) =
                    (&declarator.id, &declarator.init)
                else {
                    return Ok(());
                };
                let Some(callable) = Callable::expression(init) else {
                    return Ok(());
                };
                self.member(
                    declarator.span,
                    &id.name,
                    &callable,
                    &Owner::Function,
                    report,
                )?;
            }
            AstKind::PropertyDefinition(property) if !property.computed => {
                self.field(
                    id,
                    property.span,
                    &property.key,
                    property.value.as_ref(),
                    &property.decorators,
                    report,
                )?;
            }
            AstKind::AccessorProperty(property) if !property.computed => {
                self.field(
                    id,
                    property.span,
                    &property.key,
                    property.value.as_ref(),
                    &property.decorators,
                    report,
                )?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Checks a class field initialized with a function.
    fn field(
        &self,
        id: NodeId,
        span: Span,
        key: &PropertyKey<'a>,
        value: Option<&'a Expression<'a>>,
        decorators: &'a [Decorator<'a>],
        report: &mut Report,
    ) -> Result<()> {
        let (Some(name), Some(callable)) = (field_name(key), value.and_then(Callable::expression))
        else {
            return Ok(());
        };

        self.member(
            span,
            &name,
            &callable,
            &Owner::Field {
                class: self.class(id),
                decorators,
            },
            report,
        )
    }

    /// The class a class element belongs to.
    fn class(&self, element: NodeId) -> Option<&'a Class<'a>> {
        let nodes = self.file.semantic().nodes();

        nodes
            .ancestor_ids(element)
            .find_map(|ancestor| match nodes.kind(ancestor) {
                AstKind::Class(class) => Some(class),
                _ => None,
            })
    }

    fn line(&self, span: Span) -> u32 {
        self.file.line(span.start)
    }

    fn construction(&self, construction: &'a NewExpression<'a>, report: &mut Report) -> Result<()> {
        let constructions = self.constructions;
        let Some(name) = nest_exception(
            self.config(),
            constructions.provenance,
            self.file,
            construction,
        )?
        else {
            return Ok(());
        };

        if factory_message(
            self.config(),
            constructions.provenance,
            self.file,
            construction,
        )? {
            return Ok(());
        }

        let path = self.file.path();
        let line = self.line(construction.span);
        let constructor = match unwrap(&construction.callee) {
            Expression::Identifier(identifier) => identifier.name.to_string(),
            _ => self
                .file
                .source()
                .get(
                    construction.callee.span().start as usize
                        ..construction.callee.span().end as usize,
                )
                .map_or_else(|| name.clone(), str::to_owned),
        };
        let factory = &self.config().options.message_factory.name;

        report
            .finding(
                "raw-nest-exception",
                Location::line(path, line_number(line)),
                format!(
                    "{path}:{line} {constructor} is constructed without {factory}; use a direct canonical {factory}-backed exception"
                ),
            )
            .target(constructions.exemptions.target(self.file, construction, &name))
            .help(format!(
                "pass a {factory} message as the exception's first argument"
            ))
            .key(constructor);
        Ok(())
    }

    fn member(
        &self,
        span: Span,
        name: &str,
        callable: &Callable<'a>,
        owner: &Owner<'a>,
        report: &mut Report,
    ) -> Result<()> {
        let path = self.file.path();
        let config = self.config();
        let (class, boundary, guard) = match owner {
            Owner::Method {
                class,
                decorators,
                definition,
            } => (
                *class,
                Some(framework_boundary(
                    config, self.file, path, name, decorators, *class,
                )),
                class.zip(*definition).and_then(|(class, method)| {
                    guard::constant_failure_branch(self.file, class, method)
                }),
            ),
            Owner::Field { class, decorators } => (
                *class,
                Some(framework_boundary(
                    config, self.file, path, name, decorators, *class,
                )),
                None,
            ),
            Owner::Function => (None, None, None),
        };
        let mut exit = self.constructions.wrapper(callable.body)?;

        if exit.is_none()
            && let Some(branch) = guard
        {
            exit = self.constructions.wrapper(Body::Block(branch))?;
        }

        let line = self.line(span);

        if let Some(exit) = exit
            && !exhaustiveness_guard(self.file, callable.parameters, callable.return_type, exit)
            && (exit != Exit::Throw || boundary != Some(true))
        {
            report
                .finding(
                    "throw-only-error",
                    Location::line(path, line_number(line)),
                    format!(
                        "{path}:{line} {name} is a throw-only error wrapper; inline the complete error construction at each decision site"
                    ),
                )
                .help("inline the complete error construction at each decision site")
                .key(name);
        }

        if let Some(call) = callable.call()
            && boundary != Some(true)
            && forwarding_call(config, self.file, call, callable.parameters, class)
        {
            let callee = self
                .file
                .source()
                .get(call.callee.span().start as usize..call.callee.span().end as usize)
                .unwrap_or_default();
            report
                .finding(
                    if intrinsic_alias(config, call) {
                        "intrinsic-alias"
                    } else {
                        "one-call-forwarding"
                    },
                    Location::line(path, line_number(line)),
                    format!("{path}:{line} {name} only forwards to {callee}; inline the call"),
                )
                .help(format!("call {callee} directly where {name} is called"))
                .key(name);
        }

        Ok(())
    }
}

/// A one-based line as a location line.
fn line_number(line: u32) -> usize {
    usize::try_from(line).unwrap_or(usize::MAX)
}

/// The name of a class field: an identifier or private name.
fn field_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::PrivateIdentifier(identifier) => Some(format!("#{}", identifier.name)),
        _ => None,
    }
}

/// The source text of a member name, with the brackets of a computed name.
fn key_text(
    file: &SourceFile<'_>,
    comments: &[(usize, usize)],
    key: &PropertyKey<'_>,
    computed: bool,
) -> String {
    let span = key.span();
    let text = file
        .source()
        .get(span.start as usize..span.end as usize)
        .unwrap_or_default();

    if let PropertyKey::PrivateIdentifier(identifier) = key {
        return format!("#{}", identifier.name);
    }

    if !computed {
        return text.to_owned();
    }

    computed_text(
        file.source(),
        span.start as usize,
        span.end as usize,
        comments,
    )
    .map_or_else(|| format!("[{text}]"), str::to_owned)
}

/// A computed name's text from its `[` to its `]`: the brackets are the first tokens before and
/// after the key, past white space and the `comments`, given by their start and end in source
/// order.
fn computed_text<'s>(
    source: &'s str,
    start: usize,
    end: usize,
    comments: &[(usize, usize)],
) -> Option<&'s str> {
    let mut open = start;

    loop {
        let before = source.get(..open)?.trim_end();

        if let Ok(index) = comments.binary_search_by_key(&before.len(), |(_, end)| *end) {
            open = comments[index].0;
        } else if before.ends_with('[') {
            open = before.len() - 1;
            break;
        } else {
            return None;
        }
    }

    let mut close = end;

    loop {
        close = source.len() - source.get(close..)?.trim_start().len();

        if let Ok(index) = comments.binary_search_by_key(&close, |(start, _)| *start) {
            close = comments[index].1;
        } else if source.get(close..)?.starts_with(']') {
            return source.get(open..=close);
        } else {
            return None;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use gate_typescript::{Allocator, MemorySource, Settings, SourceFile, TypeScript};

    use super::computed_text;

    /// Runs `check` on `source` parsed as the tracked `src/a.ts`.
    pub(crate) fn with_source<T>(source: &str, check: impl FnOnce(&SourceFile<'_>) -> T) -> T {
        let files = MemorySource::new([("src/a.ts", source)]);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&files, Settings::default(), &allocator);
        let file = typescript
            .parse("src/a.ts")
            .expect("parses")
            .expect("tracked");

        check(&file)
    }

    /// The start and end of each `/* */` and `//` comment of a source without strings.
    fn comments(source: &str) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        let mut index = 0;

        while let Some(offset) = source[index..].find('/') {
            let start = index + offset;
            let end = match source.get(start..start + 2) {
                Some("/*") => source[start..].find("*/").map(|close| start + close + 2),
                Some("//") => Some(
                    source[start..]
                        .find('\n')
                        .map_or(source.len(), |close| start + close),
                ),
                _ => None,
            };

            if let Some(end) = end {
                found.push((start, end));
                index = end;
            } else {
                index = start + 1;
            }
        }

        found
    }

    // A computed wrapper is named by its bracketed key with the comments inside the brackets.
    #[test]
    fn a_computed_name_is_read_between_its_brackets() {
        let cases = [
            (r#"class S { ["a" + "b"](x) {} }"#, Some(r#"["a" + "b"]"#)),
            (r#"class S { [ "a" ](x) {} }"#, Some(r#"[ "a" ]"#)),
            (
                r#"class S { [/* [ */ "a" /* ] */](x) {} }"#,
                Some(r#"[/* [ */ "a" /* ] */]"#),
            ),
            (
                "class S { [ // [\n \"a\" // ]\n ](x) {} }",
                Some("[ // [\n \"a\" // ]\n ]"),
            ),
            ("// before\nclass S { [ \"a\" ](x) {} }", Some("[ \"a\" ]")),
            (r#"class S { "a"(x) {} }"#, None),
        ];

        for (source, expected) in cases {
            let start = source.find("\"a\"").expect("key");
            let end = if source.contains("\"a\" + \"b\"") {
                source.find("\"b\"").expect("key end") + 3
            } else {
                start + 3
            };

            assert_eq!(
                computed_text(source, start, end, &comments(source)),
                expected,
                "{source}"
            );
        }
    }

    // A `//` inside a string before the key is no comment: only the parser's comments are skipped.
    #[test]
    fn a_slash_pair_outside_a_comment_is_not_skipped() {
        let source = r#"const u = "http://x"; class S { [ "a" ](x) {} }"#;
        let start = source.find("\"a\"").expect("key");

        assert_eq!(
            computed_text(source, start, start + 3, &[]),
            Some(r#"[ "a" ]"#)
        );
    }
}
