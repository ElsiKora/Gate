//! `nestjs/constant-files`: a constant file exposes exactly one exported object bag (named with
//! the bag suffix) with `UPPER_CASE` keys, after its `UPPER_CASE` private backing constants, and no
//! constant retains callable code.

use std::collections::{HashMap, HashSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::binder::{Binder, Meaning};
use gate_typescript::owners::{call_expression, export_name, unwrap};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, BindingPattern, ChainElement, Declaration, ExportSpecifier,
    Expression, FunctionType, IdentifierReference, LogicalOperator, ObjectPropertyKind,
    PropertyKey, PropertyKind, Statement, VariableDeclaration, VariableDeclarationKind,
    VariableDeclarator,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::syntax::statement_start;
use gate_typescript::value::string_like;
use gate_typescript::{Allocator, SourceFile, TypeScript};

use crate::layout::PluginOptions;
use crate::shared::{project, report, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct ConstantFiles;

/// The options of `nestjs/constant-files`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The endings of the constant files' paths below `layout.source-root`.
    pub file_suffixes: Vec<String>,
    /// The suffix of the exported bag's name.
    pub bag_suffix: String,
    /// The `Object` methods whose single argument is read through when looking for callable code.
    pub freeze_calls: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            file_suffixes: vec![".constant.ts".to_owned(), "/constant.ts".to_owned()],
            bag_suffix: "_CONSTANT".to_owned(),
            freeze_calls: ["freeze", "seal", "preventExtensions"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

impl Rule for ConstantFiles {
    type Options = Options;

    const NAME: &'static str = "constant-files";
    const TITLE: &'static str =
        "Constant files expose one UPPER_CASE object bag without callable code";
    const ASPECTS: &'static [&'static str] = &["callable", "naming", "shape"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        let options = Options {
            file_suffixes: run.options().file_suffixes.clone(),
            bag_suffix: run.options().bag_suffix.clone(),
            freeze_calls: run.options().freeze_calls.clone(),
        };
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);

        for path in tree::typescript_files(&source, &layout.source_root, typescript.settings())
            .filter(|path| {
                options
                    .file_suffixes
                    .iter()
                    .any(|suffix| path.ends_with(suffix.as_str()))
            })
        {
            let Some(file) = project::parsed(&typescript, path)? else {
                continue;
            };

            for (aspect, line, message) in findings(&file, &options) {
                run.report()
                    .finding(
                        aspect,
                        report::location(path, line.and_then(|line| usize::try_from(line).ok())),
                        message,
                    )
                    .help(help(aspect));
            }
        }

        Ok(())
    }
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "callable" => "move the function or class to the code file that owns it",
        "naming" => "rename the constant or key in UPPER_CASE, and the bag with the bag suffix",
        _ => {
            "keep only const declarations: private UPPER_CASE backing constants, then one exported object bag"
        }
    }
}

type Finding = (&'static str, Option<u32>, String);

/// The findings of one constant file, in report order.
fn findings(file: &SourceFile<'_>, options: &Options) -> Vec<Finding> {
    let mut scan = Scan {
        file,
        options,
        binder: Binder::new(file),
        findings: Vec::new(),
        declarations: HashMap::new(),
        exported: Vec::new(),
        seen_bag: false,
    };

    scan.statements();
    scan.bag();
    scan.findings
}

/// The statements of one constant file read in order: its findings so far, the declarator of
/// each constant name, the exported names and whether an export has been seen.
struct Scan<'f, 'a> {
    file: &'f SourceFile<'a>,
    options: &'f Options,
    binder: Binder<'f, 'a>,
    findings: Vec<Finding>,
    declarations: HashMap<String, &'a VariableDeclarator<'a>>,
    exported: Vec<String>,
    seen_bag: bool,
}

impl<'a> Scan<'_, 'a> {
    fn push(&mut self, aspect: &'static str, start: Option<u32>, message: String) {
        let line = start.map(|start| self.file.line(start));
        self.findings.push((aspect, line, message));
    }

    fn statements(&mut self) {
        let path = self.file.path().to_owned();
        let program = self.file.program();
        let non_constant =
            format!("{path} contains a non-constant statement in a .constant.ts file");

        // A directive is an expression statement of the compiler.
        for directive in &program.directives {
            self.push("shape", Some(directive.span.start), non_constant.clone());
        }

        for statement in &program.body {
            match statement {
                Statement::VariableDeclaration(variables) => {
                    self.variables(statement, variables, false);
                }
                Statement::ExportDeclaration(export) => {
                    if let Declaration::VariableDeclaration(variables) = &export.declaration {
                        self.variables(statement, variables, true);
                    } else {
                        self.push(
                            "shape",
                            Some(statement_start(statement)),
                            non_constant.clone(),
                        );
                    }
                }
                Statement::ExportNamedDeclaration(export) => {
                    self.exported.extend(exported_names(&export.specifiers));
                    self.seen_bag = true;
                }
                Statement::ExportFromDeclaration(export) => {
                    self.exported.extend(exported_names(&export.specifiers));
                    self.seen_bag = true;
                }
                Statement::ExportAllDeclaration(export) if export.exported.is_some() => {
                    self.seen_bag = true;
                }
                Statement::ImportDeclaration(_) | Statement::EmptyStatement(_) => {}
                _ => self.push(
                    "shape",
                    Some(statement_start(statement)),
                    non_constant.clone(),
                ),
            }
        }
    }

    /// One variable statement, exported or private.
    fn variables(
        &mut self,
        statement: &Statement<'a>,
        variables: &'a VariableDeclaration<'a>,
        export: bool,
    ) {
        let path = self.file.path().to_owned();
        let source = self.file.source();
        let start = Some(statement.span().start);

        if !constant(variables) {
            self.push(
                "shape",
                start,
                format!("{path} contains a non-const variable statement"),
            );
        }

        for declarator in &variables.declarations {
            if declarator.init.as_ref().is_some_and(|init| {
                retains_callable(
                    &self.binder,
                    self.file,
                    &self.options.freeze_calls,
                    init,
                    &mut HashSet::new(),
                )
            }) {
                self.push(
                    "callable",
                    Some(declarator.span.start),
                    format!(
                        "{path} has constant {} that retains callable code; move the callable to its owning code file",
                        binding_name(source, &declarator.id)
                    ),
                );
            }
        }

        if export {
            let [declarator] = variables.declarations.as_slice() else {
                self.push(
                    "shape",
                    start,
                    format!(
                        "{path} must export exactly one constant declaration per export statement"
                    ),
                );
                return;
            };
            let name = binding_name(source, &declarator.id);
            self.declarations.insert(name.clone(), declarator);
            self.exported.push(name);
            self.seen_bag = true;
            return;
        }

        if self.seen_bag {
            self.push(
                "shape",
                start,
                format!(
                    "{path} declares private backing constants after the exported constant bag"
                ),
            );
        }

        for declarator in &variables.declarations {
            let name = binding_name(source, &declarator.id);

            if !is_constant_name(&name) {
                self.push(
                    "naming",
                    Some(declarator.span.start),
                    format!("{path} has private backing constant {name}; use UPPER_CASE"),
                );
            }

            self.declarations.insert(name, declarator);
        }
    }

    /// The one exported bag: declared in the file, named with the bag suffix, an object literal,
    /// optionally frozen, without spreads and with `UPPER_CASE` keys.
    fn bag(&mut self) {
        let path = self.file.path().to_owned();
        let source = self.file.source();
        let [name] = self.exported.as_slice() else {
            self.push(
                "shape",
                None,
                format!(
                    "{path} must export exactly one *{} object bag",
                    self.options.bag_suffix
                ),
            );
            return;
        };
        let name = name.clone();
        let Some(declarator) = self.declarations.get(&name).copied() else {
            self.push(
                "shape",
                None,
                format!(
                    "{path} exports {name}; exported constant must be declared in the same file"
                ),
            );
            return;
        };
        let declared = Some(declarator.span.start);

        if !name.ends_with(self.options.bag_suffix.as_str()) {
            self.push(
                "naming",
                declared,
                format!(
                    "{path} exports {name}; exported constant bags must end with {}",
                    self.options.bag_suffix
                ),
            );
        }

        let Some(Expression::ObjectExpression(object)) = declarator.init.as_ref().map(unfrozen)
        else {
            self.push(
                "shape",
                declared,
                format!("{path} exports {name}; exported constant bag must be an object literal"),
            );
            return;
        };

        for property in &object.properties {
            match property {
                ObjectPropertyKind::SpreadProperty(spread) => self.push(
                    "shape",
                    Some(spread.span.start),
                    format!("{path} exports {name}; constant bags must not use spread assignments"),
                ),
                ObjectPropertyKind::ObjectProperty(property) => {
                    let text = key_text(
                        source,
                        property.span.start,
                        &property.key,
                        property.computed,
                    );

                    if !is_constant_name(&text.replace(['"', '\''], "")) {
                        self.push(
                            "naming",
                            Some(property.span.start),
                            format!(
                                "{path} exports {name}; object-bag key {text} must use UPPER_CASE"
                            ),
                        );
                    }
                }
            }
        }
    }
}

fn exported_names(specifiers: &[ExportSpecifier<'_>]) -> impl Iterator<Item = String> {
    specifiers
        .iter()
        .map(|specifier| export_name(&specifier.exported))
}

/// A bag initializer without assertions and an `Object.freeze` call of one argument around it.
fn unfrozen<'e, 'a>(init: &'e Expression<'a>) -> &'e Expression<'a> {
    let init = unwrap(init);

    match call_expression(init) {
        Some(call)
            if call.arguments.len() == 1
                && matches!(&call.callee, Expression::StaticMemberExpression(member)
                    if member.property.name == "freeze"
                        && matches!(&member.object, Expression::Identifier(object) if object.name == "Object")) =>
        {
            call.arguments[0].as_expression().map_or(init, unwrap)
        }
        _ => init,
    }
}

/// Whether the compiler flags a variable list `const`, which `await using` lists are too.
fn constant(variables: &VariableDeclaration<'_>) -> bool {
    matches!(
        variables.kind,
        VariableDeclarationKind::Const | VariableDeclarationKind::AwaitUsing
    )
}

fn is_constant_name(name: &str) -> bool {
    let mut characters = name.chars();

    characters
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

/// The name of a binding: an identifier, or the source text of a destructuring pattern.
fn binding_name(source: &str, binding: &BindingPattern<'_>) -> String {
    match binding {
        BindingPattern::BindingIdentifier(identifier) => identifier.name.to_string(),
        pattern => {
            let span = pattern.span();
            source[span.start as usize..span.end as usize].to_owned()
        }
    }
}

/// The text of an object-bag key as the compiler's `getText` of the property name read it: an
/// identifier's name, otherwise its source text, with the brackets of a computed name.
fn key_text(source: &str, property_start: u32, key: &PropertyKey<'_>, computed: bool) -> String {
    if let PropertyKey::StaticIdentifier(identifier) = key {
        return identifier.name.to_string();
    }

    let span = key.span();
    let (start, end) = if computed {
        let open = source[property_start as usize..span.start as usize]
            .rfind('[')
            .map_or(span.start as usize, |offset| {
                property_start as usize + offset
            });
        let close = source[span.end as usize..]
            .find(']')
            .map_or(span.end as usize, |offset| span.end as usize + offset + 1);
        (open, close)
    } else {
        (span.start as usize, span.end as usize)
    };

    source[start..end].to_owned()
}

/// Whether a constant initializer retains a function or class, directly, through a `const`
/// binding of the file, inside an array or object literal, a branch of a conditional or logical
/// expression, the last operand of a comma, or a library `Object.freeze`, `seal` or
/// `preventExtensions` call.
fn retains_callable<'a>(
    binder: &Binder<'_, 'a>,
    file: &SourceFile<'a>,
    calls: &[String],
    expression: &Expression<'a>,
    active: &mut HashSet<usize>,
) -> bool {
    let value = unwrap(expression);
    // The node itself counts among the nodes it is inside.
    let key = std::ptr::from_ref(value) as usize;

    if matches!(
        value,
        Expression::ArrowFunctionExpression(_)
            | Expression::FunctionExpression(_)
            | Expression::ClassExpression(_)
    ) {
        return true;
    }

    if !active.insert(key) {
        return false;
    }

    let retained = match value {
        Expression::Identifier(identifier) => {
            identifier_retains(binder, file, calls, identifier, active)
        }
        Expression::ArrayExpression(array) => array.elements.iter().any(|element| match element {
            ArrayExpressionElement::SpreadElement(spread) => {
                retains_callable(binder, file, calls, &spread.argument, active)
            }
            ArrayExpressionElement::Elision(_) => false,
            element => element
                .as_expression()
                .is_some_and(|element| retains_callable(binder, file, calls, element, active)),
        }),
        Expression::ObjectExpression(object) => {
            object.properties.iter().any(|property| match property {
                ObjectPropertyKind::SpreadProperty(spread) => {
                    retains_callable(binder, file, calls, &spread.argument, active)
                }
                ObjectPropertyKind::ObjectProperty(property) => {
                    property.method
                        || property.kind != PropertyKind::Init
                        || retains_callable(binder, file, calls, &property.value, active)
                }
            })
        }
        Expression::ConditionalExpression(conditional) => {
            retains_callable(binder, file, calls, &conditional.consequent, active)
                || retains_callable(binder, file, calls, &conditional.alternate, active)
        }
        Expression::LogicalExpression(logical) => match logical.operator {
            LogicalOperator::And => retains_callable(binder, file, calls, &logical.right, active),
            LogicalOperator::Or | LogicalOperator::Coalesce => {
                retains_callable(binder, file, calls, &logical.left, active)
                    || retains_callable(binder, file, calls, &logical.right, active)
            }
        },
        Expression::SequenceExpression(sequence) => sequence
            .expressions
            .last()
            .is_some_and(|last| retains_callable(binder, file, calls, last, active)),
        _ => frozen_argument(binder, calls, value)
            .is_some_and(|argument| retains_callable(binder, file, calls, argument, active)),
    };

    active.remove(&key);
    retained
}

/// The argument of a library `Object.freeze`, `Object.seal` or `Object.preventExtensions` call
/// with exactly one argument.
fn frozen_argument<'e, 'a>(
    binder: &Binder<'_, 'a>,
    calls: &[String],
    value: &'e Expression<'a>,
) -> Option<&'e Expression<'a>> {
    let call = call_expression(value)?;
    let [argument] = call.arguments.as_slice() else {
        return None;
    };

    if matches!(argument, Argument::SpreadElement(_)) {
        return None;
    }

    let callee = unwrap(&call.callee);
    let (receiver, method) = match callee {
        Expression::StaticMemberExpression(member) => {
            (&member.object, Some(member.property.name.to_string()))
        }
        Expression::ComputedMemberExpression(member) => {
            (&member.object, string_like(&member.expression))
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => {
                (&member.object, Some(member.property.name.to_string()))
            }
            ChainElement::ComputedMemberExpression(member) => {
                (&member.object, string_like(&member.expression))
            }
            _ => return None,
        },
        _ => return None,
    };
    let Expression::Identifier(receiver) = unwrap(receiver) else {
        return None;
    };

    (receiver.name == "Object"
        && binder.resolve(receiver, Meaning::Value).is_none()
        && method.is_some_and(|method| calls.contains(&method)))
    .then(|| argument.as_expression())
    .flatten()
}

/// Whether a name denotes a function or class declaration of the file, or a `const` binding whose
/// initializer retains one.
fn identifier_retains<'a>(
    binder: &Binder<'_, 'a>,
    file: &SourceFile<'a>,
    calls: &[String],
    identifier: &IdentifierReference<'a>,
    active: &mut HashSet<usize>,
) -> bool {
    let Some(symbol) = binder.resolve(identifier, Meaning::Value) else {
        return false;
    };
    let semantic = file.semantic();
    let nodes = semantic.nodes();

    binder
        .declarations(symbol)
        .into_iter()
        .any(|declaration| match nodes.kind(declaration) {
            AstKind::Function(function) => matches!(
                function.r#type,
                FunctionType::FunctionDeclaration | FunctionType::TSDeclareFunction
            ),
            AstKind::Class(class) => class.is_declaration(),
            AstKind::VariableDeclarator(declarator) => {
                matches!(&declarator.id, BindingPattern::BindingIdentifier(binding)
                    if binding.symbol_id.get() == Some(symbol))
                    && matches!(nodes.parent_kind(declaration),
                        AstKind::VariableDeclaration(list) if constant(list))
                    && declarator
                        .init
                        .as_ref()
                        .is_some_and(|init| retains_callable(binder, file, calls, init, active))
            }
            _ => false,
        })
}
