//! The target of a raw Nest exception finding, which an exception of the rule names:
//! `<path>: <scope>[ catch][ parameters <name>[, <name>]]: <construction>`.
//!
//! `<scope>` is the scope whose body holds the construction: `<Class>.<member>`, the class and its
//! method, accessor or property, or `<function>`, a named function declaration at the top level of
//! the file, not in a namespace, when no class member holds it and no class of the file has its
//! name. `catch` follows when the construction lies in a `catch` clause of that scope. The
//! parameters are those the construction's arguments read: plain parameters (an identifier without
//! a default or a destructuring pattern, also as a rest element), which no body redeclares or
//! assigns, of the scope's own function and of the innermost function or arrow holding the
//! construction within it, such as a `ValidationPipe` `exceptionFactory` callback; they are listed
//! in the order the source declares them. The construction is printed in the
//! canonical form of [`crate::canonical`], with the constructor's export name. When it is the first
//! argument of a call whose callee reads no binding local to the scope, the target names the call,
//! `<callee>(new X(...))`, unless only the form without the call is among the rule's exceptions.
//!
//! Every other name the arguments read must resolve to an import or a top-level declaration of the
//! file, to a global or to a binding the construction declares itself, never to the enclosing
//! function's `arguments`. A construction outside a named scope, or one that reads another name,
//! such as a local variable or a reassigned parameter, has the target
//! `<path>:<line>: <construction>`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    BindingPattern, Expression, FormalParameters, IdentifierReference, NewExpression,
};
use gate_typescript::oxc_ast_visit::Visit;
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::scope::ScopeId;
use gate_typescript::syntax::key_name;

use super::provenance::Provenance;
use crate::canonical::{canonical_construction, canonical_target, print_construction, target_text};

/// The targets of the rule's exceptions, in canonical form.
#[derive(Debug)]
pub struct Exemptions {
    targets: HashSet<String>,
}

/// What a construction's target can be.
enum Target {
    /// A construction an exception can name: its target, and the target naming the call it is
    /// passed to first, when there is one.
    Named {
        construction: String,
        call: Option<String>,
    },
    /// A construction no scope names.
    Unnamed(String),
}

impl Exemptions {
    /// The exception targets `targets`, each respelled in canonical form when it has one.
    pub fn new(targets: &[String]) -> Self {
        Self {
            targets: targets
                .iter()
                .map(|target| canonical_target(target).unwrap_or_else(|| target.clone()))
                .collect(),
        }
    }

    /// The target to report for `construction` of `file`, which constructs the Nest exception
    /// export `name`.
    pub fn target(
        &self,
        file: &SourceFile<'_>,
        construction: &NewExpression<'_>,
        name: &str,
    ) -> String {
        match target(file, construction, name) {
            Target::Named {
                construction,
                call: Some(call),
            } if !self.targets.contains(&construction) || self.targets.contains(&call) => call,
            Target::Named { construction, .. } => construction,
            Target::Unnamed(text) => text,
        }
    }

    /// Whether an exception of the rule names `construction` of `file`, which constructs the Nest
    /// exception export `name`; such a construction is no throw-only error construction.
    pub fn exempts(
        &self,
        file: &SourceFile<'_>,
        construction: &NewExpression<'_>,
        name: &str,
    ) -> bool {
        if self.targets.is_empty() {
            return false;
        }

        match target(file, construction, name) {
            Target::Named { construction, call } => {
                self.targets.contains(&construction)
                    || call.is_some_and(|call| self.targets.contains(&call))
            }
            Target::Unnamed(_) => false,
        }
    }
}

/// The target of `construction` of `file`, which constructs the export `name`.
fn target(file: &SourceFile<'_>, construction: &NewExpression<'_>, name: &str) -> Target {
    let path = file.path();
    let printed = print_construction(file.source(), name, construction, None);
    let unnamed = || {
        Target::Unnamed(format!(
            "{path}:{}: {printed}",
            file.line(construction.span.start)
        ))
    };
    let Some(scope) = scope(file, construction) else {
        return unnamed();
    };

    if scope.class.is_none() && class_names(file).contains(scope.member.as_str()) {
        return unnamed();
    }

    let functions: Vec<&Parameters> = scope.own.iter().chain(&scope.function).collect();
    let Some(parameters) = Reads::parameters(file, construction.span, &functions, |reads| {
        for argument in &construction.arguments {
            reads.visit_argument(argument);
        }
    }) else {
        return unnamed();
    };
    let parameters: Vec<&str> = parameters.iter().map(String::as_str).collect();
    let scope_text = match &scope.class {
        Some(class) => format!("{class}.{}", scope.member),
        None => scope.member.clone(),
    };
    let named =
        |text: String| (canonical_target(&text).as_deref() == Some(text.as_str())).then_some(text);
    let Some(own) = named(target_text(
        path,
        &scope_text,
        scope.in_catch,
        &parameters,
        &printed,
    )) else {
        return unnamed();
    };
    let call = match file
        .semantic()
        .nodes()
        .parent_kind(construction.node_id.get())
    {
        AstKind::CallExpression(call)
            if call
                .arguments
                .first()
                .is_some_and(|argument| argument.span() == construction.span)
                && Reads::parameters(file, call.callee.span(), &[], |reads| {
                    reads.visit_expression(&call.callee);
                })
                .is_some() =>
        {
            let printed = print_construction(file.source(), name, construction, Some(&call.callee));

            (canonical_construction(&printed).as_deref() == Some(printed.as_str()))
                .then(|| target_text(path, &scope_text, scope.in_catch, &parameters, &printed))
                .and_then(named)
        }
        _ => None,
    };

    Target::Named {
        construction: own,
        call,
    }
}

/// The names of the classes `file` declares, at any depth.
fn class_names<'f>(file: &'f SourceFile<'_>) -> BTreeSet<&'f str> {
    let scoping = file.semantic().scoping();

    scoping
        .symbol_ids()
        .filter(|symbol| scoping.symbol_flags(*symbol).is_class())
        .map(|symbol| scoping.symbol_name(symbol))
        .collect()
}

/// Where a construction lies: its class and member or its top-level function, whether a `catch`
/// clause of that scope holds it, the scope's own function and the innermost function within the
/// scope that holds it, when that is another one.
#[derive(Debug, Eq, PartialEq)]
struct Scope {
    class: Option<String>,
    member: String,
    in_catch: bool,
    own: Option<Parameters>,
    function: Option<Parameters>,
}

/// The plain parameters of a function, those bound to an identifier without a default value or a
/// destructuring pattern, also as a rest element, by their binding spans, and the scope that binds
/// them.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Parameters {
    plain: Vec<Span>,
    scope: Option<ScopeId>,
}

impl Parameters {
    fn of(list: &FormalParameters<'_>, scope: Option<ScopeId>) -> Self {
        let identifier = |pattern: &BindingPattern<'_>| match pattern {
            BindingPattern::BindingIdentifier(identifier) => Some(identifier.span),
            _ => None,
        };

        Self {
            plain: list
                .items
                .iter()
                .filter(|parameter| parameter.initializer.is_none())
                .filter_map(|parameter| identifier(&parameter.pattern))
                .chain(
                    list.rest
                        .as_ref()
                        .and_then(|rest| identifier(&rest.rest.argument)),
                )
                .collect(),
            scope,
        }
    }
}

/// The functions around a construction, met from the construction outwards.
#[derive(Default)]
struct Functions {
    /// The innermost function's parameters, and its span, so that the scope's own function is not
    /// also its callback.
    innermost: Option<(Span, Parameters)>,
    /// The outermost function met so far: the scope's own when the scope's value is it.
    outermost: Option<(Span, Parameters)>,
    /// The scope's own function.
    own: Option<Parameters>,
}

impl Functions {
    /// Meets a function of `span` with `parameters`.
    fn enter(&mut self, span: Span, parameters: Parameters) {
        if self.innermost.is_none() {
            self.innermost = Some((span, parameters.clone()));
        }
        self.outermost = Some((span, parameters));
    }

    /// Meets the scope, whose value spans `value`: the outermost function met is the scope's own
    /// when it is that value, and no callback of the scope when it is also the innermost.
    fn scope(&mut self, value: Option<Span>) {
        let Some((span, parameters)) = self.outermost.take() else {
            return;
        };

        if value == Some(span) {
            if self
                .innermost
                .as_ref()
                .is_some_and(|(innermost, _)| *innermost == span)
            {
                self.innermost = None;
            }
            self.own = Some(parameters);
        }
    }

    /// The scope's own function and the callback within it that holds the construction.
    fn into_parts(self) -> (Option<Parameters>, Option<Parameters>) {
        (self.own, self.innermost.map(|(_, parameters)| parameters))
    }
}

/// The class and member or the top-level function of a construction, its `catch` position, the
/// scope's own function and the innermost function within that scope holding it; `None` outside
/// both.
fn scope(file: &SourceFile<'_>, construction: &NewExpression<'_>) -> Option<Scope> {
    let mut in_catch = false;
    let mut member: Option<String> = None;
    let mut functions = Functions::default();
    let mut declaration: Option<String> = None;
    let mut ancestors = file
        .semantic()
        .nodes()
        .ancestor_kinds(construction.node_id.get());
    let value_span =
        |value: Option<&Expression<'_>>| value.map(|value| value.get_inner_expression().span());

    while let Some(kind) = ancestors.next() {
        match kind {
            // A member's class is its parent and a top-level function's parent is the program or
            // an export, so every catch clause seen holds the construction within the scope or
            // within a function nested in it.
            AstKind::CatchClause(_) => in_catch = true,
            AstKind::Function(value) if member.is_none() && declaration.is_none() => {
                functions.enter(
                    value.span,
                    Parameters::of(&value.params, value.scope_id.get()),
                );

                if let Some(id) = &value.id
                    && top_level(ancestors.clone())
                {
                    declaration = Some(id.name.to_string());
                    functions.scope(Some(value.span));
                }
            }
            AstKind::ArrowFunctionExpression(arrow)
                if member.is_none() && declaration.is_none() =>
            {
                functions.enter(
                    arrow.span,
                    Parameters::of(&arrow.params, arrow.scope_id.get()),
                );
            }
            AstKind::MethodDefinition(method) if member.is_none() => {
                member = Some(key_name(&method.key, method.computed)?);
                functions.scope(Some(method.value.span));
            }
            AstKind::PropertyDefinition(property) if member.is_none() => {
                member = Some(key_name(&property.key, property.computed)?);
                functions.scope(value_span(property.value.as_ref()));
            }
            AstKind::AccessorProperty(property) if member.is_none() => {
                member = Some(key_name(&property.key, property.computed)?);
                functions.scope(value_span(property.value.as_ref()));
            }
            AstKind::Class(class) => {
                let (own, function) = functions.into_parts();

                return Some(Scope {
                    class: Some(class.id.as_ref()?.name.to_string()),
                    member: member?,
                    in_catch,
                    own,
                    function,
                });
            }
            _ => {}
        }
    }

    let (own, function) = functions.into_parts();

    Some(Scope {
        class: None,
        member: declaration?,
        in_catch,
        own,
        function,
    })
}

/// Whether the ancestors of a function declaration, from its parent up, place it at the top level
/// of the file: in the program itself or in an export of it, not in a namespace.
fn top_level<'a>(mut ancestors: impl Iterator<Item = AstKind<'a>>) -> bool {
    match ancestors.next() {
        Some(AstKind::Program(_)) => true,
        Some(AstKind::ExportDeclaration(_) | AstKind::ExportDefaultDeclaration(_)) => {
            matches!(ancestors.next(), Some(AstKind::Program(_)))
        }
        _ => false,
    }
}

/// The names the visited syntax reads: the plain parameters of `functions` it reads, and whether it
/// reads a name that resolves to a binding below the file's top level that the syntax within
/// `within` does not declare itself, such as a local variable, a callback parameter of another
/// function, or the enclosing function's `arguments`.
struct Reads<'f, 's> {
    file: &'f SourceFile<'s>,
    within: Span,
    functions: &'f [&'f Parameters],
    /// The plain parameters read, by the start of their binding, with their names.
    read: BTreeMap<u32, String>,
    /// The names read that resolve to something other than a plain parameter.
    others: BTreeSet<String>,
    local: bool,
}

impl<'f, 's> Reads<'f, 's> {
    /// The plain parameters of `functions` that the syntax `visit` walks reads, in the order the
    /// source declares them; `None` when it reads another binding local to the scope, or a name
    /// both as a plain parameter and as another binding.
    fn parameters(
        file: &'f SourceFile<'s>,
        within: Span,
        functions: &'f [&'f Parameters],
        visit: impl FnOnce(&mut Self),
    ) -> Option<Vec<String>> {
        let mut reads = Self {
            file,
            within,
            functions,
            read: BTreeMap::new(),
            others: BTreeSet::new(),
            local: false,
        };
        visit(&mut reads);

        (!reads.local && reads.read.values().all(|name| !reads.others.contains(name)))
            .then(|| reads.read.into_values().collect())
    }
}

impl<'a> Visit<'a> for Reads<'_, '_> {
    fn visit_identifier_reference(&mut self, identifier: &IdentifierReference<'a>) {
        let scoping = self.file.semantic().scoping();
        let name = identifier.name.to_string();

        let Some(symbol) = Provenance::symbol(self.file, identifier) else {
            // oxc binds no symbol for `arguments`, which is the enclosing function's own.
            if name == "arguments" {
                self.local = true;
            }

            self.others.insert(name);
            return;
        };
        let declared = scoping.symbol_span(symbol);

        // A plain parameter of one of the functions, bound in that function's own scope, which the
        // body neither redeclares, as a `var` or a function of the same name shares that binding,
        // nor assigns.
        if self.functions.iter().any(|function| {
            function.scope == Some(scoping.symbol_scope_id(symbol))
                && function.plain.contains(&declared)
        }) && scoping.symbol_redeclarations(symbol).is_empty()
            && !scoping.symbol_is_mutated(symbol)
        {
            self.read.insert(declared.start, name);
        } else {
            if scoping.symbol_scope_id(symbol) != scoping.root_scope_id()
                && !(self.within.start <= declared.start && declared.end <= self.within.end)
            {
                self.local = true;
            }

            self.others.insert(name);
        }
    }
}
