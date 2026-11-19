//! The syntax nodes the reconstruction reads and the literal texts they hold.

use gate_typescript::oxc_ast::ast::{
    Argument, ArrayExpressionElement, BinaryOperator, BindingProperty, CallExpression,
    ChainElement, Expression, LogicalOperator, SpreadElement, TemplateLiteral,
};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::value::number_text;

use super::super::sql::{Text, units};

/// A syntax node the reconstruction reads: an expression, an array or argument spread, an array
/// hole, an object-pattern property, the template of a tagged template, a name that is no
/// expression (a property, binding or label name), read as an identifier too,
/// or a node the advisory-evidence search does not model, such as a function or class body.
#[derive(Clone, Copy, Debug)]
pub(super) enum Node<'a> {
    Expression(&'a Expression<'a>),
    Spread(&'a SpreadElement<'a>),
    Omitted(Span),
    Binding(&'a BindingProperty<'a>),
    Template(&'a TemplateLiteral<'a>),
    Name(&'a str, Span),
    Opaque(Span),
}

impl<'a> Node<'a> {
    pub(super) fn span(self) -> Span {
        match self {
            Node::Expression(expression) => expression.span(),
            Node::Spread(spread) => spread.span,
            Node::Omitted(span) | Node::Name(_, span) | Node::Opaque(span) => span,
            Node::Binding(binding) => binding.span,
            Node::Template(template) => template.span,
        }
    }

    /// The name of an identifier, as `ts.isIdentifier` reads it.
    pub(super) fn identifier(self) -> Option<&'a str> {
        match self {
            Node::Expression(Expression::Identifier(identifier)) => Some(identifier.name.as_str()),
            Node::Name(name, _) => Some(name),
            _ => None,
        }
    }

    /// The template of a template literal or of a tagged template.
    pub(super) fn template(self) -> Option<&'a TemplateLiteral<'a>> {
        match self {
            Node::Expression(Expression::TemplateLiteral(template)) => Some(template),
            Node::Template(template) => Some(template),
            _ => None,
        }
    }

    /// The operand of a parenthesis or an `as` expression.
    pub(super) fn parenthesized_or_as(self) -> Option<Node<'a>> {
        match self {
            Node::Expression(Expression::ParenthesizedExpression(expression)) => {
                Some(Node::Expression(&expression.expression))
            }
            Node::Expression(Expression::TSAsExpression(expression)) => {
                Some(Node::Expression(&expression.expression))
            }
            _ => None,
        }
    }

    /// The operand of a parenthesis or a type operation (`as`, `<T>`, `!`, `satisfies`).
    pub(super) fn typed_operand(self) -> Option<Node<'a>> {
        self.parenthesized_or_as().or(match self {
            Node::Expression(Expression::TSTypeAssertion(expression)) => {
                Some(Node::Expression(&expression.expression))
            }
            Node::Expression(Expression::TSNonNullExpression(expression)) => {
                Some(Node::Expression(&expression.expression))
            }
            Node::Expression(Expression::TSSatisfiesExpression(expression)) => {
                Some(Node::Expression(&expression.expression))
            }
            _ => None,
        })
    }

    /// The call of a call expression, an optional one included.
    pub(super) fn call(self) -> Option<&'a CallExpression<'a>> {
        match self {
            Node::Expression(Expression::CallExpression(call)) => Some(call),
            Node::Expression(Expression::ChainExpression(chain)) => match &chain.expression {
                ChainElement::CallExpression(call) => Some(call),
                _ => None,
            },
            _ => None,
        }
    }

    /// The two branches of a conditional or a `??`, `||` or `&&` expression.
    pub(super) fn branches(self) -> Option<[Node<'a>; 2]> {
        match self {
            Node::Expression(Expression::ConditionalExpression(conditional)) => Some([
                Node::Expression(&conditional.consequent),
                Node::Expression(&conditional.alternate),
            ]),
            Node::Expression(Expression::LogicalExpression(logical))
                if matches!(
                    logical.operator,
                    LogicalOperator::Coalesce | LogicalOperator::Or | LogicalOperator::And
                ) =>
            {
                Some([
                    Node::Expression(&logical.left),
                    Node::Expression(&logical.right),
                ])
            }
            _ => None,
        }
    }

    /// The two operands of a `+` expression.
    pub(super) fn addition(self) -> Option<[Node<'a>; 2]> {
        match self {
            Node::Expression(Expression::BinaryExpression(binary))
                if binary.operator == BinaryOperator::Addition =>
            {
                Some([
                    Node::Expression(&binary.left),
                    Node::Expression(&binary.right),
                ])
            }
            _ => None,
        }
    }
}

pub(super) fn argument_node<'a>(argument: &'a Argument<'a>) -> Node<'a> {
    match argument {
        Argument::SpreadElement(spread) => Node::Spread(spread),
        _ => argument
            .as_expression()
            .map_or(Node::Omitted(argument.span()), Node::Expression),
    }
}

pub(super) fn element_node<'a>(element: &'a ArrayExpressionElement<'a>) -> Node<'a> {
    match element {
        ArrayExpressionElement::SpreadElement(spread) => Node::Spread(spread),
        ArrayExpressionElement::Elision(elision) => Node::Omitted(elision.span),
        _ => element
            .as_expression()
            .map_or(Node::Omitted(element.span()), Node::Expression),
    }
}

/// The UTF-16 code units of an oxc string value, whose unpaired surrogates oxc encodes as
/// `U+FFFD` followed by four hexadecimal digits.
pub(super) fn string_units(value: &str, lone_surrogates: bool) -> Text {
    if !lone_surrogates {
        return units(value);
    }

    let mut text = Text::new();
    let mut characters = value.chars().peekable();

    while let Some(character) = characters.next() {
        if character == char::REPLACEMENT_CHARACTER {
            let digits: String = characters.by_ref().take(4).collect();

            if let Ok(unit) = u16::from_str_radix(&digits, 16) {
                text.push(unit);
                continue;
            }

            text.extend(units(&format!("{character}{digits}")));
        } else {
            let mut buffer = [0; 2];
            text.extend_from_slice(character.encode_utf16(&mut buffer));
        }
    }

    text
}

/// The cooked text of a template element, as the TypeScript scanner reads it.
pub(super) fn quasi(template: &TemplateLiteral<'_>, index: usize) -> Text {
    template.quasis.get(index).map_or_else(Text::new, |quasi| {
        quasi.value.cooked.as_ref().map_or_else(
            || cook(&units(quasi.value.raw.as_str())),
            |cooked| string_units(cooked.as_str(), quasi.lone_surrogates),
        )
    })
}

/// The text of a tagged-template element with an invalid escape, cooked as the TypeScript scanner
/// cooks it (`scanTemplateAndSetTokenValue` and `scanEscapeSequence`): each valid escape decoded,
/// each invalid one kept as its raw text, and `\r\n` and `\r` read as `\n`.
fn cook(raw: &[u16]) -> Text {
    let unit = |index: usize| raw.get(index).copied();
    let is = |index: usize, test: fn(u8) -> bool| {
        unit(index).is_some_and(|value| u8::try_from(value).is_ok_and(test))
    };
    let octal = |value: u8| (b'0'..=b'7').contains(&value);
    let mut cooked = Text::new();
    let mut position = 0;

    while let Some(current) = unit(position) {
        if current == u16::from(b'\r') {
            cooked.push(u16::from(b'\n'));
            position += if unit(position + 1) == Some(u16::from(b'\n')) {
                2
            } else {
                1
            };
            continue;
        }

        if current != u16::from(b'\\') {
            cooked.push(current);
            position += 1;
            continue;
        }

        let start = position;
        let Some(escaped) = unit(start + 1) else {
            break;
        };
        position = start + 2;
        let character = u8::try_from(escaped).ok();

        match character {
            Some(b'0') if !is(position, |value| value.is_ascii_digit()) => cooked.push(0),
            Some(b'0'..=b'7') => {
                if matches!(character, Some(b'0'..=b'3')) && is(position, octal) {
                    position += 1;
                }
                if is(position, octal) {
                    position += 1;
                }
                cooked.extend_from_slice(&raw[start..position]);
            }
            Some(b'8' | b'9') => cooked.extend_from_slice(&raw[start..position]),
            Some(b'b') => cooked.push(0x08),
            Some(b't') => cooked.push(u16::from(b'\t')),
            Some(b'n') => cooked.push(u16::from(b'\n')),
            Some(b'v') => cooked.push(0x0b),
            Some(b'f') => cooked.push(0x0c),
            Some(b'r') => cooked.push(u16::from(b'\r')),
            Some(b'u') if unit(position) == Some(u16::from(b'{')) => {
                position += 1;
                let digits = position;
                let mut value: u32 = 0;

                while is(position, |value| value.is_ascii_hexdigit()) {
                    let digit = char::from_u32(u32::from(raw[position]))
                        .and_then(|digit| digit.to_digit(16))
                        .unwrap_or_default();
                    value = value.saturating_mul(16).saturating_add(digit);
                    position += 1;
                }

                let closed = unit(position) == Some(u16::from(b'}'));

                if closed {
                    position += 1;
                }

                match char::from_u32(value) {
                    Some(decoded) if closed && position > digits + 1 => {
                        let mut buffer = [0; 2];
                        cooked.extend_from_slice(decoded.encode_utf16(&mut buffer));
                    }
                    // A lone surrogate is a valid escape; `char` cannot hold it.
                    None if closed && position > digits + 1 && value <= 0xFFFF => {
                        cooked.extend(u16::try_from(value));
                    }
                    _ => cooked.extend_from_slice(&raw[start..position]),
                }
            }
            Some(b'u' | b'x') => {
                let length = if character == Some(b'u') { 4 } else { 2 };
                let end = start + 2 + length;

                while position < end && is(position, |value| value.is_ascii_hexdigit()) {
                    position += 1;
                }

                if position < end {
                    cooked.extend_from_slice(&raw[start..position]);
                } else {
                    let value = String::from_utf16_lossy(&raw[start + 2..end]);
                    cooked.extend(u16::from_str_radix(&value, 16));
                }
            }
            Some(b'\r') => {
                if unit(position) == Some(u16::from(b'\n')) {
                    position += 1;
                }
            }
            Some(b'\n') => {}
            _ if escaped == 0x2028 || escaped == 0x2029 => {}
            _ => cooked.push(escaped),
        }
    }

    cooked
}

/// The text of a string, numeric or substitution-free template literal, as the TypeScript compiler
/// gives it.
pub(super) fn literal(node: Node<'_>) -> Option<Text> {
    match node {
        Node::Expression(Expression::StringLiteral(literal)) => Some(string_units(
            literal.value.as_str(),
            literal.lone_surrogates,
        )),
        Node::Expression(Expression::NumericLiteral(literal)) => {
            Some(units(&number_text(literal.value)))
        }
        _ => node
            .template()
            .filter(|template| template.expressions.is_empty())
            .map(|template| quasi(template, 0)),
    }
}

pub(super) fn unwrap_static(node: Node<'_>) -> Node<'_> {
    let mut node = node;

    while let Some(inner) = node.parenthesized_or_as() {
        node = inner;
    }

    node
}

pub(super) fn equals(text: &[u16], ascii: &str) -> bool {
    text.iter().copied().eq(ascii.encode_utf16())
}
