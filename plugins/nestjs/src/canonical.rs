//! The canonical spelling of a wrapper construction: the form `nestjs/wrappers` prints in its
//! finding targets and matches exception targets against.
//!
//! A construction is a `new` expression whose constructor is named by an identifier, such as
//! `new BadRequestException({ code: "INVALID" })`. Two constructions are the same when their
//! constructor names, type arguments and arguments are the same syntax trees, whatever their
//! white space, comments (other than `@__PURE__` annotations), trailing commas, string quotes and
//! number spellings; parentheses and the spelling of an object key (quoted, shorthand or
//! computed) still count. The canonical form prints that syntax tree on one line:
//!
//! - one space after each comma and colon, around binary, logical and assignment operators, `?`
//!   and `:` of a conditional, `=>`, `as` and `satisfies`, and none inside parentheses or
//!   brackets;
//! - an object literal or type literal as `{ a: 1, b }` (`{}` when empty), an array as `[a, b]`;
//! - a string in double quotes, escaping only `\`, `"`, and control and line separator
//!   characters; a number as JavaScript's `String(number)` prints it; a bigint by its decimal
//!   value with `n`; a template literal and a regular expression by their source text;
//! - every parameter list in parentheses, also of a one-parameter arrow function, and `new X()`
//!   for `new X`;
//! - a function body, a class and any other syntax outside expressions and the common type
//!   forms by its source text without comments, every run of white space outside strings and
//!   template literals collapsed to one space.
//!
//! A construction may also be named with the call it is passed to first, `f(new X(a), b)`; the
//! call's other arguments, type arguments and the optional chain are not part of it, so its
//! canonical form is `f(new X(a))`: the callee, then the construction in parentheses.

use std::fmt::Write as _;

use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget, BindingPattern,
    ChainElement, Expression, FormalParameter, FormalParameters, NewExpression, ObjectPropertyKind,
    PropertyKey, PropertyKind, SimpleAssignmentTarget, TSLiteral, TSSignature, TSTupleElement,
    TSType, TSTypeName, TSTypeOperatorOperator, TSTypeParameterInstantiation, TSTypeQueryExprName,
    TemplateLiteral,
};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::oxc_syntax::identifier::is_identifier_name;
use gate_typescript::value::number_text;
use gate_typescript::{Allocator, parse_expression};

/// The canonical form of the wrapper construction `construction`, or `None` when it is not one.
///
/// `construction` is one TypeScript expression: a `new` expression whose constructor is an
/// identifier, `new X(...)`, or a call (not in an optional chain) whose first argument is one,
/// `f(new X(...), ...)`, which reads as `f(new X(...))`. The result prints its syntax tree as the module documentation
/// describes, so two constructions that differ only in white space, comments, trailing commas,
/// string quotes or number spellings have the same canonical form, and the canonical form of a
/// canonical form is itself.
#[must_use]
pub fn canonical_construction(construction: &str) -> Option<String> {
    let allocator = Allocator::default();
    // The parser reads `construction` itself, so its spans index into it.
    let construction_text = construction;
    let expression = parse_expression(&allocator, construction_text)?;
    let (construction, receiving) = constructed(&expression)?;
    let Expression::Identifier(constructor) = &construction.callee else {
        return None;
    };

    Some(print_construction(
        construction_text,
        &constructor.name,
        construction,
        receiving,
    ))
}

/// The canonical form of a `nestjs/wrappers` raw Nest exception target,
/// `<path>: <scope>[ catch][ parameters <name>[, <name>]]: <construction>`, or `None` when it is
/// not one: the same target with its construction in canonical form.
///
/// `<scope>` is `<Class>.<member>` or the name of a top-level function; each parameter name is an
/// identifier and named once. The rule lists the parameters in the order the function declares
/// them, so a target that lists them in another order names no finding.
#[must_use]
pub fn canonical_target(target: &str) -> Option<String> {
    let (path, rest) = target.split_once(": ")?;
    let (scope, construction) = rest.split_once(": ")?;
    let (scope, parameters) = match scope.split_once(" parameters ") {
        Some((scope, names)) => (scope, names.split(", ").collect::<Vec<_>>()),
        None => (scope, Vec::new()),
    };

    if scope.ends_with(" parameters") {
        return None;
    }

    let (member_scope, in_catch) = match scope.strip_suffix(" catch") {
        Some(scope) => (scope, true),
        None => (scope, false),
    };
    let member = match member_scope.split_once('.') {
        Some((class, member)) if !class.is_empty() => member,
        None if is_identifier_name(member_scope) => member_scope,
        _ => return None,
    };
    let mut names = parameters.clone();
    names.sort_unstable();

    if path.is_empty()
        || member.is_empty()
        || member.contains('.')
        || !parameters.iter().all(|name| is_identifier_name(name))
        || names.windows(2).any(|pair| pair[0] == pair[1])
    {
        return None;
    }

    Some(target_text(
        path,
        member_scope,
        in_catch,
        &parameters,
        &canonical_construction(construction)?,
    ))
}

/// A raw Nest exception target from its parts; `parameters` are printed in the order given.
#[must_use]
pub(crate) fn target_text(
    path: &str,
    scope: &str,
    in_catch: bool,
    parameters: &[&str],
    construction: &str,
) -> String {
    let mut text = format!("{path}: {scope}");

    if in_catch {
        text.push_str(" catch");
    }

    if !parameters.is_empty() {
        text.push_str(" parameters ");
        text.push_str(&parameters.join(", "));
    }

    text.push_str(": ");
    text.push_str(construction);
    text
}

/// The construction an expression names and the callee of the call it is the first argument of:
/// a `new` expression alone, or as the first argument of a call.
fn constructed<'e, 'a>(
    expression: &'e Expression<'a>,
) -> Option<(&'e NewExpression<'a>, Option<&'e Expression<'a>>)> {
    match expression {
        Expression::NewExpression(construction) => Some((construction, None)),
        Expression::CallExpression(call) => match call.arguments.first()? {
            Argument::NewExpression(construction) => Some((construction, Some(&call.callee))),
            _ => None,
        },
        _ => None,
    }
}

/// The canonical form of `construction` of the source text `source`, with `constructor` as its
/// constructor's name, `new <constructor><type arguments>(<arguments>)`, passed to `receiving`
/// when it is given: `<receiving>(new ...)`.
#[must_use]
pub(crate) fn print_construction(
    source: &str,
    constructor: &str,
    construction: &NewExpression<'_>,
    receiving: Option<&Expression<'_>>,
) -> String {
    let mut printer = Printer {
        source,
        out: String::new(),
    };

    if let Some(callee) = receiving {
        printer.expression(callee);
        printer.out.push('(');
    }

    printer.out.push_str("new ");
    printer.out.push_str(constructor);
    printer.type_arguments(construction.type_arguments.as_deref());
    printer.arguments(&construction.arguments);

    if receiving.is_some() {
        printer.out.push(')');
    }

    printer.out
}

/// Prints syntax trees of `source` in the canonical form.
struct Printer<'s> {
    source: &'s str,
    out: String,
}

impl Printer<'_> {
    /// The source text of `span` without comments, with every run of white space and comments
    /// outside strings and template literals collapsed to one space.
    fn verbatim(&mut self, span: Span) {
        let text = self
            .source
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default();
        let mut characters = text.chars().peekable();
        let mut quote: Option<char> = None;
        let mut escaped = false;
        let mut space = false;

        while let Some(character) = characters.next() {
            if let Some(open) = quote {
                self.out.push(character);

                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }

                continue;
            }

            if character == '/' && characters.peek() == Some(&'/') {
                while characters.next_if(|next| *next != '\n').is_some() {}
                space = true;
                continue;
            }

            if character == '/' && characters.peek() == Some(&'*') {
                characters.next();
                let mut star = false;

                for next in characters.by_ref() {
                    if star && next == '/' {
                        break;
                    }
                    star = next == '*';
                }

                space = true;
                continue;
            }

            if character.is_whitespace() {
                space = true;
                continue;
            }

            if space && !self.out.is_empty() && !self.out.ends_with(' ') {
                self.out.push(' ');
            }

            space = false;

            if matches!(character, '"' | '\'' | '`') {
                quote = Some(character);
            }

            self.out.push(character);
        }
    }

    fn list<T>(&mut self, items: &[T], mut print: impl FnMut(&mut Self, &T)) {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            print(self, item);
        }
    }

    fn arguments(&mut self, arguments: &[Argument<'_>]) {
        self.out.push('(');
        self.list(arguments, |printer, argument| match argument {
            Argument::SpreadElement(spread) => {
                printer.out.push_str("...");
                printer.expression(&spread.argument);
            }
            argument => match argument.as_expression() {
                Some(expression) => printer.expression(expression),
                None => printer.verbatim(argument.span()),
            },
        });
        self.out.push(')');
    }

    fn type_arguments(&mut self, arguments: Option<&TSTypeParameterInstantiation<'_>>) {
        if let Some(arguments) = arguments {
            self.out.push('<');
            self.list(&arguments.params, Self::ts_type);
            self.out.push('>');
        }
    }

    fn string(&mut self, value: &str) {
        self.out.push('"');

        for character in value.chars() {
            match character {
                '"' => self.out.push_str("\\\""),
                '\\' => self.out.push_str("\\\\"),
                '\n' => self.out.push_str("\\n"),
                '\r' => self.out.push_str("\\r"),
                '\t' => self.out.push_str("\\t"),
                '\u{2028}' | '\u{2029}' => {
                    let _ = write!(self.out, "\\u{:04x}", u32::from(character));
                }
                control if control.is_control() => {
                    let _ = write!(self.out, "\\u{:04x}", u32::from(control));
                }
                other => self.out.push(other),
            }
        }

        self.out.push('"');
    }

    fn template(&mut self, template: &TemplateLiteral<'_>) {
        self.out.push('`');

        for (index, quasi) in template.quasis.iter().enumerate() {
            self.out.push_str(&quasi.value.raw);

            if let Some(expression) = template.expressions.get(index) {
                self.out.push_str("${");
                self.expression(expression);
                self.out.push('}');
            }
        }

        self.out.push('`');
    }

    fn property_key(&mut self, key: &PropertyKey<'_>, computed: bool) {
        if computed {
            self.out.push('[');
        }

        match key {
            PropertyKey::StaticIdentifier(identifier) => self.out.push_str(&identifier.name),
            PropertyKey::PrivateIdentifier(identifier) => {
                self.out.push('#');
                self.out.push_str(&identifier.name);
            }
            key => match key.as_expression() {
                Some(expression) => self.expression(expression),
                None => self.verbatim(key.span()),
            },
        }

        if computed {
            self.out.push(']');
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per expression kind reads best as one match"
    )]
    fn expression(&mut self, expression: &Expression<'_>) {
        match expression {
            Expression::BooleanLiteral(literal) => {
                self.out
                    .push_str(if literal.value { "true" } else { "false" });
            }
            Expression::NullLiteral(_) => self.out.push_str("null"),
            Expression::NumericLiteral(literal) => self.out.push_str(&number_text(literal.value)),
            Expression::BigIntLiteral(literal) => {
                self.out.push_str(&literal.value);
                self.out.push('n');
            }
            Expression::StringLiteral(literal) => self.string(&literal.value),
            Expression::RegExpLiteral(literal) => {
                let _ = write!(
                    self.out,
                    "/{}/{}",
                    literal.regex.pattern.text, literal.regex.flags
                );
            }
            Expression::TemplateLiteral(template) => self.template(template),
            Expression::Identifier(identifier) => self.out.push_str(&identifier.name),
            Expression::Super(_) => self.out.push_str("super"),
            Expression::ThisExpression(_) => self.out.push_str("this"),
            Expression::ArrayExpression(array) => {
                self.out.push('[');
                self.list(&array.elements, |printer, element| match element {
                    ArrayExpressionElement::SpreadElement(spread) => {
                        printer.out.push_str("...");
                        printer.expression(&spread.argument);
                    }
                    ArrayExpressionElement::Elision(_) => {}
                    element => match element.as_expression() {
                        Some(expression) => printer.expression(expression),
                        None => printer.verbatim(element.span()),
                    },
                });

                if matches!(
                    array.elements.last(),
                    Some(ArrayExpressionElement::Elision(_))
                ) {
                    self.out.push(',');
                }

                self.out.push(']');
            }
            Expression::ObjectExpression(object) => {
                if object.properties.is_empty() {
                    self.out.push_str("{}");
                    return;
                }

                self.out.push_str("{ ");
                self.list(&object.properties, |printer, property| match property {
                    ObjectPropertyKind::SpreadProperty(spread) => {
                        printer.out.push_str("...");
                        printer.expression(&spread.argument);
                    }
                    ObjectPropertyKind::ObjectProperty(property)
                        if property.kind == PropertyKind::Init && !property.method =>
                    {
                        if property.shorthand {
                            printer.expression(&property.value);
                        } else {
                            printer.property_key(&property.key, property.computed);
                            printer.out.push_str(": ");
                            printer.expression(&property.value);
                        }
                    }
                    ObjectPropertyKind::ObjectProperty(property) => {
                        printer.verbatim(property.span);
                    }
                });
                self.out.push_str(" }");
            }
            Expression::ArrowFunctionExpression(arrow) => {
                if arrow.type_parameters.is_some() {
                    self.verbatim(arrow.span);
                    return;
                }

                if arrow.r#async {
                    self.out.push_str("async ");
                }

                self.parameters(&arrow.params);

                if let Some(annotation) = &arrow.return_type {
                    self.out.push_str(": ");
                    self.ts_type(&annotation.type_annotation);
                }

                self.out.push_str(" => ");

                match &arrow.body {
                    ArrowFunctionBody::FunctionBody(body) => self.verbatim(body.span),
                    body => match body.as_expression() {
                        Some(expression) => self.expression(expression),
                        None => self.verbatim(body.span()),
                    },
                }
            }
            Expression::AssignmentExpression(assignment) => {
                self.assignment_target(&assignment.left);
                let _ = write!(self.out, " {} ", assignment.operator.as_str());
                self.expression(&assignment.right);
            }
            Expression::AwaitExpression(inner) => {
                self.out.push_str("await ");
                self.expression(&inner.argument);
            }
            Expression::BinaryExpression(binary) => {
                self.expression(&binary.left);
                let _ = write!(self.out, " {} ", binary.operator.as_str());
                self.expression(&binary.right);
            }
            Expression::LogicalExpression(logical) => {
                self.expression(&logical.left);
                let _ = write!(self.out, " {} ", logical.operator.as_str());
                self.expression(&logical.right);
            }
            Expression::PrivateInExpression(inner) => {
                let _ = write!(self.out, "#{} in ", inner.left.name);
                self.expression(&inner.right);
            }
            Expression::CallExpression(call) => {
                if call.pure {
                    self.out.push_str("/* @__PURE__ */ ");
                }
                self.expression(&call.callee);

                if call.optional {
                    self.out.push_str("?.");
                }

                self.type_arguments(call.type_arguments.as_deref());
                self.arguments(&call.arguments);
            }
            Expression::NewExpression(construction) => {
                if construction.pure {
                    self.out.push_str("/* @__PURE__ */ ");
                }
                self.out.push_str("new ");
                self.expression(&construction.callee);
                self.type_arguments(construction.type_arguments.as_deref());
                self.arguments(&construction.arguments);
            }
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(call) => {
                    if call.pure {
                        self.out.push_str("/* @__PURE__ */ ");
                    }
                    self.expression(&call.callee);

                    if call.optional {
                        self.out.push_str("?.");
                    }

                    self.type_arguments(call.type_arguments.as_deref());
                    self.arguments(&call.arguments);
                }
                ChainElement::TSNonNullExpression(inner) => {
                    self.expression(&inner.expression);
                    self.out.push('!');
                }
                element => match element.as_member_expression() {
                    Some(member) => self.member(member),
                    None => self.verbatim(chain.span),
                },
            },
            Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::PrivateFieldExpression(_) => match expression.as_member_expression() {
                Some(member) => self.member(member),
                None => self.verbatim(expression.span()),
            },
            Expression::ConditionalExpression(conditional) => {
                self.expression(&conditional.test);
                self.out.push_str(" ? ");
                self.expression(&conditional.consequent);
                self.out.push_str(" : ");
                self.expression(&conditional.alternate);
            }
            Expression::ParenthesizedExpression(inner) => {
                self.out.push('(');
                self.expression(&inner.expression);
                self.out.push(')');
            }
            Expression::SequenceExpression(sequence) => {
                self.list(&sequence.expressions, Self::expression);
            }
            Expression::TaggedTemplateExpression(tagged) => {
                self.expression(&tagged.tag);
                self.type_arguments(tagged.type_arguments.as_deref());
                self.template(&tagged.quasi);
            }
            Expression::UnaryExpression(unary) => {
                let operator = unary.operator.as_str();
                self.out.push_str(operator);
                let start = self.out.len();
                self.expression(&unary.argument);
                let argument = &self.out[start..];

                if operator.chars().all(char::is_alphabetic)
                    || (matches!(operator, "+" | "-") && argument.starts_with(operator))
                {
                    self.out.insert(start, ' ');
                }
            }
            Expression::UpdateExpression(update) => {
                let operator = update.operator.as_str();

                if update.prefix {
                    self.out.push_str(operator);
                }

                self.verbatim(update.argument.span());

                if !update.prefix {
                    self.out.push_str(operator);
                }
            }
            Expression::YieldExpression(inner) => {
                self.out
                    .push_str(if inner.delegate { "yield*" } else { "yield" });

                if let Some(argument) = &inner.argument {
                    self.out.push(' ');
                    self.expression(argument);
                }
            }
            Expression::ImportExpression(import) => {
                self.out.push_str("import(");
                self.expression(&import.source);

                if let Some(options) = &import.options {
                    self.out.push_str(", ");
                    self.expression(options);
                }

                self.out.push(')');
            }
            Expression::TSAsExpression(inner) => {
                self.expression(&inner.expression);
                self.out.push_str(" as ");
                self.ts_type(&inner.type_annotation);
            }
            Expression::TSSatisfiesExpression(inner) => {
                self.expression(&inner.expression);
                self.out.push_str(" satisfies ");
                self.ts_type(&inner.type_annotation);
            }
            Expression::TSTypeAssertion(inner) => {
                self.out.push('<');
                self.ts_type(&inner.type_annotation);
                self.out.push('>');
                self.expression(&inner.expression);
            }
            Expression::TSNonNullExpression(inner) => {
                self.expression(&inner.expression);
                self.out.push('!');
            }
            Expression::TSInstantiationExpression(inner) => {
                self.expression(&inner.expression);
                self.type_arguments(Some(&inner.type_arguments));
            }
            other => self.verbatim(other.span()),
        }
    }

    fn member(&mut self, member: &gate_typescript::oxc_ast::ast::MemberExpression<'_>) {
        use gate_typescript::oxc_ast::ast::MemberExpression;

        match member {
            MemberExpression::StaticMemberExpression(member) => {
                self.expression(&member.object);
                self.out.push_str(if member.optional { "?." } else { "." });
                self.out.push_str(&member.property.name);
            }
            MemberExpression::ComputedMemberExpression(member) => {
                self.expression(&member.object);
                self.out.push_str(if member.optional { "?.[" } else { "[" });
                self.expression(&member.expression);
                self.out.push(']');
            }
            MemberExpression::PrivateFieldExpression(member) => {
                self.expression(&member.object);
                self.out
                    .push_str(if member.optional { "?.#" } else { ".#" });
                self.out.push_str(&member.field.name);
            }
        }
    }

    fn assignment_target(&mut self, target: &AssignmentTarget<'_>) {
        match target.as_simple_assignment_target() {
            Some(SimpleAssignmentTarget::AssignmentTargetIdentifier(identifier)) => {
                self.out.push_str(&identifier.name);
            }
            Some(simple) => match simple.as_member_expression() {
                Some(member) => self.member(member),
                None => self.verbatim(simple.span()),
            },
            None => self.verbatim(target.span()),
        }
    }

    fn parameters(&mut self, parameters: &FormalParameters<'_>) {
        self.out.push('(');
        self.list(&parameters.items, Self::parameter);

        if let Some(rest) = &parameters.rest {
            if !parameters.items.is_empty() {
                self.out.push_str(", ");
            }

            if rest.decorators.is_empty() {
                self.out.push_str("...");
                self.binding(&rest.rest.argument);

                if let Some(annotation) = &rest.type_annotation {
                    self.out.push_str(": ");
                    self.ts_type(&annotation.type_annotation);
                }
            } else {
                self.verbatim(rest.span);
            }
        }

        self.out.push(')');
    }

    fn parameter(&mut self, parameter: &FormalParameter<'_>) {
        if !parameter.decorators.is_empty()
            || parameter.accessibility.is_some()
            || parameter.readonly
            || parameter.r#override
        {
            self.verbatim(parameter.span);
            return;
        }

        self.binding(&parameter.pattern);

        if parameter.optional {
            self.out.push('?');
        }

        if let Some(annotation) = &parameter.type_annotation {
            self.out.push_str(": ");
            self.ts_type(&annotation.type_annotation);
        }

        if let Some(initializer) = &parameter.initializer {
            self.out.push_str(" = ");
            self.expression(initializer);
        }
    }

    fn binding(&mut self, pattern: &BindingPattern<'_>) {
        match pattern {
            BindingPattern::BindingIdentifier(identifier) => self.out.push_str(&identifier.name),
            BindingPattern::ObjectPattern(object) => {
                if object.properties.is_empty() && object.rest.is_none() {
                    self.out.push_str("{}");
                    return;
                }

                self.out.push_str("{ ");
                self.list(&object.properties, |printer, property| {
                    if property.shorthand {
                        printer.binding(&property.value);
                    } else {
                        printer.property_key(&property.key, property.computed);
                        printer.out.push_str(": ");
                        printer.binding(&property.value);
                    }
                });

                if let Some(rest) = &object.rest {
                    if !object.properties.is_empty() {
                        self.out.push_str(", ");
                    }
                    self.out.push_str("...");
                    self.binding(&rest.argument);
                }

                self.out.push_str(" }");
            }
            BindingPattern::ArrayPattern(array) => {
                self.out.push('[');
                self.list(&array.elements, |printer, element| {
                    if let Some(element) = element {
                        printer.binding(element);
                    }
                });

                if let Some(rest) = &array.rest {
                    if !array.elements.is_empty() {
                        self.out.push_str(", ");
                    }
                    self.out.push_str("...");
                    self.binding(&rest.argument);
                } else if matches!(array.elements.last(), Some(None)) {
                    self.out.push(',');
                }

                self.out.push(']');
            }
            BindingPattern::AssignmentPattern(assignment) => {
                self.binding(&assignment.left);
                self.out.push_str(" = ");
                self.expression(&assignment.right);
            }
        }
    }

    fn type_name(&mut self, name: &TSTypeName<'_>) {
        match name {
            TSTypeName::IdentifierReference(identifier) => self.out.push_str(&identifier.name),
            TSTypeName::QualifiedName(qualified) => {
                self.type_name(&qualified.left);
                self.out.push('.');
                self.out.push_str(&qualified.right.name);
            }
            TSTypeName::ThisExpression(_) => self.out.push_str("this"),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per type kind reads best as one match"
    )]
    fn ts_type(&mut self, kind: &TSType<'_>) {
        let keyword = match kind {
            TSType::TSAnyKeyword(_) => Some("any"),
            TSType::TSBigIntKeyword(_) => Some("bigint"),
            TSType::TSBooleanKeyword(_) => Some("boolean"),
            TSType::TSIntrinsicKeyword(_) => Some("intrinsic"),
            TSType::TSNeverKeyword(_) => Some("never"),
            TSType::TSNullKeyword(_) => Some("null"),
            TSType::TSNumberKeyword(_) => Some("number"),
            TSType::TSObjectKeyword(_) => Some("object"),
            TSType::TSStringKeyword(_) => Some("string"),
            TSType::TSSymbolKeyword(_) => Some("symbol"),
            TSType::TSUndefinedKeyword(_) => Some("undefined"),
            TSType::TSUnknownKeyword(_) => Some("unknown"),
            TSType::TSVoidKeyword(_) => Some("void"),
            TSType::TSThisType(_) => Some("this"),
            _ => None,
        };

        if let Some(keyword) = keyword {
            self.out.push_str(keyword);
            return;
        }

        match kind {
            TSType::TSTypeReference(reference) => {
                self.type_name(&reference.type_name);
                self.type_arguments(reference.type_arguments.as_deref());
            }
            TSType::TSArrayType(array) => {
                self.ts_type(&array.element_type);
                self.out.push_str("[]");
            }
            TSType::TSIndexedAccessType(access) => {
                self.ts_type(&access.object_type);
                self.out.push('[');
                self.ts_type(&access.index_type);
                self.out.push(']');
            }
            TSType::TSUnionType(union) => {
                for (index, member) in union.types.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(" | ");
                    }
                    self.ts_type(member);
                }
            }
            TSType::TSIntersectionType(intersection) => {
                for (index, member) in intersection.types.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(" & ");
                    }
                    self.ts_type(member);
                }
            }
            TSType::TSParenthesizedType(inner) => {
                self.out.push('(');
                self.ts_type(&inner.type_annotation);
                self.out.push(')');
            }
            TSType::TSTypeOperatorType(operator) => {
                self.out.push_str(match operator.operator {
                    TSTypeOperatorOperator::Keyof => "keyof ",
                    TSTypeOperatorOperator::Unique => "unique ",
                    TSTypeOperatorOperator::Readonly => "readonly ",
                });
                self.ts_type(&operator.type_annotation);
            }
            TSType::TSTypeQuery(query) => match &query.expr_name {
                TSTypeQueryExprName::TSImportType(_) => self.verbatim(query.span),
                name => match name.as_ts_type_name() {
                    Some(name) => {
                        self.out.push_str("typeof ");
                        self.type_name(name);
                        self.type_arguments(query.type_arguments.as_deref());
                    }
                    None => self.verbatim(query.span),
                },
            },
            TSType::TSLiteralType(literal) => match &literal.literal {
                TSLiteral::BooleanLiteral(literal) => {
                    self.out
                        .push_str(if literal.value { "true" } else { "false" });
                }
                TSLiteral::NumericLiteral(literal) => {
                    self.out.push_str(&number_text(literal.value));
                }
                TSLiteral::BigIntLiteral(literal) => {
                    self.out.push_str(&literal.value);
                    self.out.push('n');
                }
                TSLiteral::StringLiteral(literal) => self.string(&literal.value),
                TSLiteral::TemplateLiteral(template) => self.template(template),
                TSLiteral::UnaryExpression(unary) => {
                    self.out.push_str(unary.operator.as_str());
                    self.expression(&unary.argument);
                }
            },
            TSType::TSTupleType(tuple) => {
                self.out.push('[');
                self.list(&tuple.element_types, |printer, element| match element {
                    TSTupleElement::TSOptionalType(optional) => {
                        printer.ts_type(&optional.type_annotation);
                        printer.out.push('?');
                    }
                    TSTupleElement::TSRestType(rest) => {
                        printer.out.push_str("...");
                        printer.ts_type(&rest.type_annotation);
                    }
                    element => match element.as_ts_type() {
                        Some(kind) => printer.ts_type(kind),
                        None => printer.verbatim(element.span()),
                    },
                });
                self.out.push(']');
            }
            TSType::TSTypeLiteral(literal) => {
                if literal.members.is_empty() {
                    self.out.push_str("{}");
                    return;
                }

                self.out.push_str("{ ");

                for (index, member) in literal.members.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str("; ");
                    }

                    match member {
                        TSSignature::TSPropertySignature(property) => {
                            if property.readonly {
                                self.out.push_str("readonly ");
                            }
                            self.property_key(&property.key, property.computed);

                            if property.optional {
                                self.out.push('?');
                            }

                            if let Some(annotation) = &property.type_annotation {
                                self.out.push_str(": ");
                                self.ts_type(&annotation.type_annotation);
                            }
                        }
                        member => self.verbatim(member.span()),
                    }
                }

                self.out.push_str(" }");
            }
            other => self.verbatim(other.span()),
        }
    }
}
