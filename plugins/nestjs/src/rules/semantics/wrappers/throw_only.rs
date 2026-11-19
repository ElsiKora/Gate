//! Throw-only error wrappers: a body whose only effect is to throw or return one error
//! construction, after `const` bindings that only feed it.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gate_sdk::Result;
use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Expression, FormalParameters, IdentifierReference, NewExpression, Statement, TSType,
    TSTypeAnnotation, TSTypeName, VariableDeclarationKind,
};
use gate_typescript::oxc_ast_visit::Visit;
use gate_typescript::oxc_syntax::symbol::SymbolId;
use gate_typescript::value::unwrap;

use super::Config;
use super::exception::{nest_exception, plain_error};
use super::provenance::{Provenance, binding_symbols};
use super::target::Exemptions;

/// A function body: statements, or the expression of an arrow function.
#[derive(Clone, Copy)]
pub enum Body<'a> {
    Block(&'a [Statement<'a>]),
    Expression(&'a Expression<'a>),
}

/// How a throw-only wrapper exits with its error.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Throw,
    Return,
}

/// What the wrapper check needs to decide whether a construction is an error construction.
pub struct Constructions<'p, 't, 'a> {
    pub config: &'p Config,
    pub provenance: &'p Provenance<'t, 'a>,
    pub exemptions: &'p Exemptions,
    pub file: &'p Rc<SourceFile<'a>>,
}

impl<'a> Constructions<'_, '_, 'a> {
    /// The error construction `expression` is, through local `const` bindings.
    fn construction(
        &self,
        expression: &'a Expression<'a>,
        bindings: &HashMap<SymbolId, &'a Expression<'a>>,
    ) -> Result<Option<&'a NewExpression<'a>>> {
        let mut current = unwrap(expression);
        let mut seen = HashSet::new();

        while let Expression::Identifier(identifier) = current {
            let Some(symbol) = Provenance::symbol(self.file, identifier) else {
                return Ok(None);
            };
            let Some(initializer) = bindings.get(&symbol) else {
                return Ok(None);
            };

            if !seen.insert(symbol) {
                return Ok(None);
            }

            current = unwrap(initializer);
        }

        let Expression::NewExpression(construction) = current else {
            return Ok(None);
        };

        // Only a Nest exception can be named by an exception.
        let name = nest_exception(self.config, self.provenance, self.file, construction)?;
        let nest = name.is_some();

        if name
            .as_deref()
            .is_some_and(|name| self.exemptions.exempts(self.file, construction, name))
        {
            return Ok(None);
        }

        Ok((nest || plain_error(self.config, self.file, construction)).then_some(&**construction))
    }

    fn exit_construction(
        &self,
        statement: &'a Statement<'a>,
        bindings: &HashMap<SymbolId, &'a Expression<'a>>,
    ) -> Result<Option<Exit>> {
        let (exit, expression) = match statement {
            Statement::ThrowStatement(statement) => (Exit::Throw, &statement.argument),
            Statement::ReturnStatement(statement) => match &statement.argument {
                Some(argument) => (Exit::Return, argument),
                None => return Ok(None),
            },
            _ => return Ok(None),
        };

        Ok(self.construction(expression, bindings)?.map(|_| exit))
    }

    /// How `body` exits when it only throws or returns one error construction.
    pub fn wrapper(&self, body: Body<'a>) -> Result<Option<Exit>> {
        let mut statements = match body {
            Body::Expression(expression) => {
                return Ok(self
                    .construction(expression, &HashMap::new())?
                    .map(|_| Exit::Return));
            }
            Body::Block(statements) => statements,
        };

        while let [Statement::BlockStatement(block)] = statements {
            statements = &block.body;
        }

        let Some((last, bindings)) = statements.split_last() else {
            return Ok(None);
        };

        if bindings.is_empty() {
            return self.exit_construction(last, &HashMap::new());
        }

        let mut initializers: HashMap<SymbolId, &'a Expression<'a>> = HashMap::new();
        let mut order: Vec<SymbolId> = Vec::new();

        for statement in bindings {
            let Statement::VariableDeclaration(declaration) = statement else {
                return Ok(None);
            };

            if declaration.kind != VariableDeclarationKind::Const {
                return Ok(None);
            }

            for declarator in &declaration.declarations {
                let Some(initializer) = &declarator.init else {
                    return Ok(None);
                };
                let mut symbols = Vec::new();
                binding_symbols(&declarator.id, &mut symbols);

                if symbols.is_empty() {
                    return Ok(None);
                }

                for (_, symbol) in symbols {
                    initializers.insert(symbol, initializer);
                    order.push(symbol);
                }
            }
        }

        let Some(exit) = self.exit_construction(last, &initializers)? else {
            return Ok(None);
        };
        let locals: HashSet<SymbolId> = order.iter().copied().collect();
        let mut referenced = References::new(self.file, &locals);
        referenced.visit_statement(last);
        let mut pending: Vec<SymbolId> = referenced.found.iter().copied().collect();
        let mut done: HashSet<SymbolId> = HashSet::new();

        while let Some(symbol) = pending.pop() {
            if !done.insert(symbol) {
                continue;
            }

            if let Some(initializer) = initializers.get(&symbol) {
                let mut nested = References::new(self.file, &locals);
                nested.visit_expression(initializer);
                pending.extend(nested.found);
            }
        }

        Ok(order
            .iter()
            .all(|symbol| done.contains(symbol))
            .then_some(exit))
    }
}

/// The local symbols an expression or statement refers to as values; type positions are skipped.
struct References<'f, 'l, 'a> {
    file: &'f SourceFile<'a>,
    locals: &'l HashSet<SymbolId>,
    found: HashSet<SymbolId>,
}

impl<'f, 'l, 'a> References<'f, 'l, 'a> {
    fn new(file: &'f SourceFile<'a>, locals: &'l HashSet<SymbolId>) -> Self {
        Self {
            file,
            locals,
            found: HashSet::new(),
        }
    }
}

impl<'a> Visit<'a> for References<'_, '_, 'a> {
    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        if let Some(symbol) = Provenance::symbol(self.file, identifier)
            && self.locals.contains(&symbol)
        {
            self.found.insert(symbol);
        }
    }

    fn visit_ts_type(&mut self, _: &TSType<'a>) {}

    fn visit_ts_type_annotation(&mut self, _: &TSTypeAnnotation<'a>) {}
}

/// An alias by its start, with whether each of its type arguments is `never`.
type AliasKey = (u32, Vec<bool>);

/// Reads whether types are `never`, also through local type aliases with their type arguments.
/// Each alias is read once for each set of arguments, and an alias reached again while it is being
/// read is not `never`, as the compiler reads a circular alias as `any`.
struct Never<'f, 'a> {
    file: &'f SourceFile<'a>,
    /// Whether an alias, by its start and whether each argument is `never`, is `never`; `None`
    /// while it is being read.
    aliases: RefCell<HashMap<AliasKey, Option<bool>>>,
    /// How many alias bodies have been read, the work the tests bound.
    reads: Cell<usize>,
}

impl Never<'_, '_> {
    /// Aliases nested deeper than this are not `never`, which bounds the stack.
    const DEPTH: usize = 256;

    /// Whether a type is `never`; `parameters` holds whether each type argument of the alias being
    /// read is `never`, by its type parameter's symbol.
    fn read(&self, annotation: &TSType<'_>, parameters: &[(SymbolId, bool)], depth: usize) -> bool {
        match annotation {
            TSType::TSNeverKeyword(_) => true,
            TSType::TSParenthesizedType(inner) => {
                self.read(&inner.type_annotation, parameters, depth)
            }
            TSType::TSUnionType(union) => union
                .types
                .iter()
                .all(|member| self.read(member, parameters, depth)),
            TSType::TSIntersectionType(intersection) => intersection
                .types
                .iter()
                .any(|member| self.read(member, parameters, depth)),
            // Whichever branch a conditional type takes, both of `never` give `never`.
            TSType::TSConditionalType(conditional) => {
                self.read(&conditional.true_type, parameters, depth)
                    && self.read(&conditional.false_type, parameters, depth)
            }
            TSType::TSTypeReference(reference) if depth < Self::DEPTH => {
                let TSTypeName::IdentifierReference(name) = &reference.type_name else {
                    return false;
                };
                let Some(symbol) = Provenance::symbol(self.file, name) else {
                    return false;
                };

                if let Some((_, never)) = parameters
                    .iter()
                    .find(|(parameter, _)| *parameter == symbol)
                {
                    return *never;
                }

                // A type may share its name with a value, which declares the symbol again.
                let semantic = self.file.semantic();
                let Some(alias) =
                    semantic
                        .scoping()
                        .symbol_declarations(symbol)
                        .find_map(|declaration| match semantic.nodes().kind(declaration) {
                            AstKind::TSTypeAliasDeclaration(alias) => Some(alias),
                            _ => None,
                        })
                else {
                    return false;
                };
                let arguments = reference
                    .type_arguments
                    .as_ref()
                    .map_or(&[][..], |arguments| arguments.params.as_slice());
                let declared = alias
                    .type_parameters
                    .as_ref()
                    .map_or(&[][..], |declared| declared.params.as_slice());

                // Extra type arguments are an error the compiler does not read as `never`.
                if arguments.len() > declared.len() {
                    return false;
                }

                let mut substituted = Vec::new();

                for (index, declared) in declared.iter().enumerate() {
                    // A default is read against the parameters before it.
                    let never = match arguments.get(index) {
                        Some(argument) => self.read(argument, parameters, depth + 1),
                        None => declared
                            .default
                            .as_ref()
                            .is_some_and(|default| self.read(default, &substituted, depth + 1)),
                    };

                    if let Some(parameter) = declared.name.symbol_id.get() {
                        substituted.push((parameter, never));
                    }
                }

                let key = (
                    alias.span.start,
                    substituted
                        .iter()
                        .map(|(_, never)| *never)
                        .collect::<Vec<_>>(),
                );

                if let Some(known) = self.aliases.borrow().get(&key) {
                    return known.unwrap_or(false);
                }

                self.aliases.borrow_mut().insert(key.clone(), None);
                self.reads.set(self.reads.get() + 1);
                let never = self.read(&alias.type_annotation, &substituted, depth + 1);
                self.aliases.borrow_mut().insert(key, Some(never));

                never
            }
            _ => false,
        }
    }
}

/// Whether a throw-only body is an exhaustiveness guard: it returns `never` and requires an input
/// of type `never`.
pub fn exhaustiveness_guard(
    file: &SourceFile<'_>,
    parameters: &FormalParameters<'_>,
    return_type: Option<&TSTypeAnnotation<'_>>,
    exit: Exit,
) -> bool {
    let never = Never {
        file,
        aliases: RefCell::default(),
        reads: Cell::default(),
    };

    exit == Exit::Throw
        && return_type.is_some_and(|annotation| never.read(&annotation.type_annotation, &[], 0))
        && parameters.items.iter().any(|parameter| {
            !parameter.optional
                && parameter.initializer.is_none()
                && parameter
                    .type_annotation
                    .as_ref()
                    .is_some_and(|annotation| never.read(&annotation.type_annotation, &[], 0))
        })
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fmt::Write;

    use gate_typescript::oxc_ast::ast::{Declaration, Statement};

    use super::Never;
    use crate::rules::semantics::wrappers::tests::with_source;

    /// Whether the first parameter type of the exported function of `source` is `never`, with
    /// the number of alias bodies read to decide it.
    fn read(source: &str) -> (bool, usize) {
        with_source(source, |file| {
            let function = file
                .program()
                .body
                .iter()
                .find_map(|statement| match statement {
                    Statement::ExportDeclaration(export) => match &export.declaration {
                        Declaration::FunctionDeclaration(function) => Some(function),
                        _ => None,
                    },
                    _ => None,
                })
                .expect("exported function");
            let annotation = function.params.items[0]
                .type_annotation
                .as_ref()
                .expect("parameter type");
            let never = Never {
                file,
                aliases: RefCell::default(),
                reads: Cell::default(),
            };
            let result = never.read(&annotation.type_annotation, &[], 0);

            (result, never.reads.get())
        })
    }

    // Each alias is read once for its arguments, so aliases that each name the previous one twice
    // in a union, which reads every member, cost one read apiece instead of doubling at each step.
    #[test]
    fn an_alias_named_twice_by_the_next_is_read_once() {
        let mut source = String::from("type A0 = never;\n");

        for index in 1..=28 {
            writeln!(source, "type A{index} = A{} | A{};", index - 1, index - 1).expect("writes");
        }

        source.push_str("export function f(x: A28): never { throw new Error(\"x\"); }\n");

        assert_eq!(read(&source), (true, 29));
    }

    // A chain of aliases is followed to the depth bound and no further, which bounds the stack.
    #[test]
    fn an_alias_chain_is_followed_to_the_depth_bound() {
        let chain = |length: usize| {
            let mut source = String::from("type A0 = never;\n");

            for index in 1..=length {
                writeln!(source, "type A{index} = A{};", index - 1).expect("writes");
            }

            writeln!(
                source,
                "export function f(x: A{length}): never {{ throw new Error(\"x\"); }}"
            )
            .expect("writes");
            source
        };

        assert_eq!(read(&chain(255)), (true, 256));
        assert_eq!(read(&chain(400)), (false, Never::DEPTH));
    }
}
