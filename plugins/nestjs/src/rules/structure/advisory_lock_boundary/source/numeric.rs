//! The registered advisory-lock identifiers a source holds.
//!
//! Every numeric literal of the text counts, and so does every value a syntax node evaluates to
//! from numeric and big-integer literals, parentheses, `as` and `<T>` casts, `Number(…)` of a
//! number or a rebuilt string, unary `-`, `+` and `~`, the arithmetic, shift, `|` and `^` operators
//! and their compound assignments, and every assignment of a variable, as JavaScript numbers,
//! within depth, work and candidate budgets.

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    Argument, AssignmentOperator, AssignmentTarget, BinaryOperator, CallExpression, ChainElement,
    Expression, UnaryOperator,
};
use gate_typescript::oxc_span::Span;
use rustc_hash::FxHashSet as HashSet;

use super::interpreter::Interpreter;
use super::node::{Node, argument_node};
use super::text::{CANDIDATE_LIMIT, consume};
use super::{Fail, Outcome, Seen};

/// A binary operation the evaluator computes.
#[derive(Clone, Copy, Debug)]
enum Operation {
    Power,
    Multiply,
    Or,
    Xor,
    UnsignedShift,
    Shift,
    LeftShift,
    Subtract,
    Remainder,
    Add,
    Divide,
}

impl Operation {
    fn binary(operator: BinaryOperator) -> Option<Self> {
        Some(match operator {
            BinaryOperator::Exponential => Self::Power,
            BinaryOperator::Multiplication => Self::Multiply,
            BinaryOperator::BitwiseOR => Self::Or,
            BinaryOperator::BitwiseXOR => Self::Xor,
            BinaryOperator::ShiftRightZeroFill => Self::UnsignedShift,
            BinaryOperator::ShiftRight => Self::Shift,
            BinaryOperator::ShiftLeft => Self::LeftShift,
            BinaryOperator::Subtraction => Self::Subtract,
            BinaryOperator::Remainder => Self::Remainder,
            BinaryOperator::Addition => Self::Add,
            BinaryOperator::Division => Self::Divide,
            _ => return None,
        })
    }

    fn assignment(operator: AssignmentOperator) -> Option<Self> {
        Some(match operator {
            AssignmentOperator::Exponential => Self::Power,
            AssignmentOperator::Multiplication => Self::Multiply,
            AssignmentOperator::BitwiseOR => Self::Or,
            AssignmentOperator::BitwiseXOR => Self::Xor,
            AssignmentOperator::ShiftRightZeroFill => Self::UnsignedShift,
            AssignmentOperator::ShiftRight => Self::Shift,
            AssignmentOperator::ShiftLeft => Self::LeftShift,
            AssignmentOperator::Subtraction => Self::Subtract,
            AssignmentOperator::Remainder => Self::Remainder,
            AssignmentOperator::Addition => Self::Add,
            AssignmentOperator::Division => Self::Divide,
            _ => return None,
        })
    }

    fn apply(self, left: f64, right: f64) -> f64 {
        let count = to_uint32(right) & 31;

        match self {
            Self::Power => power(left, right),
            Self::Multiply => left * right,
            Self::Or => f64::from(to_int32(left) | to_int32(right)),
            Self::Xor => f64::from(to_int32(left) ^ to_int32(right)),
            Self::UnsignedShift => f64::from(to_uint32(left) >> count),
            Self::Shift => f64::from(to_int32(left) >> count),
            Self::LeftShift => f64::from(to_int32(left).wrapping_shl(count)),
            Self::Subtract => left - right,
            Self::Remainder => left % right,
            Self::Add => left + right,
            Self::Divide => left / right,
        }
    }
}

/// JavaScript's `**`, whose `NaN` and `±1 ** ±Infinity` cases differ from `powf`.
#[expect(clippy::float_cmp, reason = "the special cases are exact values")]
fn power(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() {
        return f64::NAN;
    }

    if exponent == 0.0 {
        return 1.0;
    }

    if base.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        return f64::NAN;
    }

    base.powf(exponent)
}

/// `ToUint32`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is reduced modulo 2^32 before the cast"
)]
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }

    value.trunc().rem_euclid(4_294_967_296.0) as u32
}

/// `ToInt32`.
#[expect(
    clippy::cast_possible_wrap,
    reason = "ToInt32 reinterprets the 32 bits of ToUint32"
)]
fn to_int32(value: f64) -> i32 {
    to_uint32(value) as i32
}

/// JavaScript whitespace and line terminators, which `Number(…)` trims.
pub(super) fn is_space(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// The value of an unsigned integer in a radix, as JavaScript rounds it.
#[expect(
    clippy::cast_precision_loss,
    reason = "JavaScript rounds a long integer literal to the nearest double too"
)]
fn radix_value(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() || !digits.chars().all(|digit| digit.is_digit(radix)) {
        return f64::NAN;
    }

    let mut value: u128 = 0;

    for digit in digits.chars() {
        let Some(next) = value
            .checked_mul(u128::from(radix))
            .and_then(|value| value.checked_add(u128::from(digit.to_digit(radix).unwrap_or(0))))
        else {
            return f64::INFINITY;
        };

        value = next;
    }

    value as f64
}

/// Whether a text is JavaScript's `StrDecimalLiteral` without its `Infinity` forms.
fn is_decimal(text: &str) -> bool {
    let body = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(index) => (&body[..index], Some(&body[index + 1..])),
        None => (body, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.chars().all(|digit| digit.is_ascii_digit());

    (!whole.is_empty() || !fraction.is_empty())
        && digits(whole)
        && digits(fraction)
        && exponent.is_none_or(|exponent| {
            let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
            !exponent.is_empty() && digits(exponent)
        })
}

/// JavaScript's `Number(text)` for a string.
pub(super) fn number(text: &str) -> f64 {
    let text = text.trim_matches(is_space);

    if text.is_empty() {
        return 0.0;
    }

    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }

    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return radix_value(digits, radix);
        }
    }

    if is_decimal(text) {
        text.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// The value of a numeric literal's source text: separators and a big-integer `n` dropped.
pub(super) fn literal_value(raw: &str) -> f64 {
    let text = raw.replace('_', "");
    number(text.strip_suffix('n').unwrap_or(&text))
}

/// The candidate numbers of one evaluation, each once, in the order found.
#[derive(Default)]
struct Numbers(Vec<f64>);

impl Numbers {
    fn add(&mut self, candidate: f64, budget: &mut u32, depth: usize) -> Outcome<()> {
        consume(budget, depth)?;

        // A Set keeps one of 0 and -0, and NaN never reaches it here.
        if candidate.is_finite() && !self.0.contains(&candidate) {
            self.0.push(candidate);
        }

        if self.0.len() > CANDIDATE_LIMIT {
            return Err(Fail::new(
                "Numeric expression candidate budget was exceeded.",
            ));
        }

        Ok(())
    }
}

/// The shape of a node the numeric evaluation reads, from an expression or a visited syntax node.
#[derive(Clone, Copy)]
enum Shape<'a> {
    Literal(Span),
    Wrapped(&'a Expression<'a>),
    Name(&'a str),
    Number(&'a Argument<'a>),
    Unary(UnaryOperator, &'a Expression<'a>),
    Binary(Option<Operation>, Side<'a>, &'a Expression<'a>),
    Sequence(&'a [Expression<'a>]),
    Other,
}

/// The left side of a binary operation: an expression or an assignment target.
#[derive(Clone, Copy)]
enum Side<'a> {
    Expression(&'a Expression<'a>),
    Name(&'a str),
    Other,
}

fn number_call<'a>(call: &'a CallExpression<'a>) -> Shape<'a> {
    match (&call.callee, call.arguments.as_slice()) {
        (Expression::Identifier(callee), [argument]) if callee.name == "Number" => {
            Shape::Number(argument)
        }
        _ => Shape::Other,
    }
}

fn unary_shape<'a>(operator: UnaryOperator, argument: &'a Expression<'a>) -> Shape<'a> {
    match operator {
        UnaryOperator::Typeof | UnaryOperator::Void | UnaryOperator::Delete => Shape::Other,
        _ => Shape::Unary(operator, argument),
    }
}

fn target_side<'a>(target: &'a AssignmentTarget<'a>) -> Side<'a> {
    match target {
        AssignmentTarget::AssignmentTargetIdentifier(identifier) => Side::Name(&identifier.name),
        _ => Side::Other,
    }
}

fn expression_shape<'a>(expression: &'a Expression<'a>) -> Shape<'a> {
    match expression {
        Expression::NumericLiteral(literal) => Shape::Literal(literal.span),
        Expression::BigIntLiteral(literal) => Shape::Literal(literal.span),
        Expression::ParenthesizedExpression(inner) => Shape::Wrapped(&inner.expression),
        Expression::TSAsExpression(inner) => Shape::Wrapped(&inner.expression),
        Expression::TSTypeAssertion(inner) => Shape::Wrapped(&inner.expression),
        Expression::Identifier(identifier) => Shape::Name(&identifier.name),
        Expression::CallExpression(call) => number_call(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => number_call(call),
            _ => Shape::Other,
        },
        Expression::UnaryExpression(unary) => unary_shape(unary.operator, &unary.argument),
        Expression::BinaryExpression(binary) => Shape::Binary(
            Operation::binary(binary.operator),
            Side::Expression(&binary.left),
            &binary.right,
        ),
        Expression::LogicalExpression(logical) => {
            Shape::Binary(None, Side::Expression(&logical.left), &logical.right)
        }
        Expression::AssignmentExpression(assignment) => Shape::Binary(
            Operation::assignment(assignment.operator),
            target_side(&assignment.left),
            &assignment.right,
        ),
        Expression::SequenceExpression(sequence) => Shape::Sequence(&sequence.expressions),
        _ => Shape::Other,
    }
}

/// The shape of a syntax node the evaluation reads; every other node yields nothing.
fn kind_shape(kind: AstKind<'_>) -> Shape<'_> {
    match kind {
        AstKind::NumericLiteral(literal) => Shape::Literal(literal.span),
        AstKind::BigIntLiteral(literal) => Shape::Literal(literal.span),
        AstKind::ParenthesizedExpression(inner) => Shape::Wrapped(&inner.expression),
        AstKind::TSAsExpression(inner) => Shape::Wrapped(&inner.expression),
        AstKind::TSTypeAssertion(inner) => Shape::Wrapped(&inner.expression),
        AstKind::IdentifierReference(identifier) => Shape::Name(&identifier.name),
        AstKind::BindingIdentifier(identifier) => Shape::Name(&identifier.name),
        AstKind::IdentifierName(identifier) => Shape::Name(&identifier.name),
        AstKind::LabelIdentifier(identifier) => Shape::Name(&identifier.name),
        AstKind::CallExpression(call) => number_call(call),
        AstKind::UnaryExpression(unary) => unary_shape(unary.operator, &unary.argument),
        AstKind::BinaryExpression(binary) => Shape::Binary(
            Operation::binary(binary.operator),
            Side::Expression(&binary.left),
            &binary.right,
        ),
        AstKind::LogicalExpression(logical) => {
            Shape::Binary(None, Side::Expression(&logical.left), &logical.right)
        }
        AstKind::AssignmentExpression(assignment) => Shape::Binary(
            Operation::assignment(assignment.operator),
            target_side(&assignment.left),
            &assignment.right,
        ),
        AstKind::SequenceExpression(sequence) => Shape::Sequence(&sequence.expressions),
        _ => Shape::Other,
    }
}

impl<'a> Interpreter<'a> {
    fn numbers(
        &self,
        shape: Shape<'a>,
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<Vec<f64>> {
        consume(budget, depth)?;

        match shape {
            Shape::Literal(span) => {
                let raw = self
                    .text
                    .get(span.start as usize..span.end as usize)
                    .unwrap_or_default();
                let value = literal_value(raw);

                Ok(if value.is_finite() {
                    vec![value]
                } else {
                    Vec::new()
                })
            }
            Shape::Wrapped(inner) => self.numbers(expression_shape(inner), seen, budget, depth + 1),
            Shape::Name(name) => {
                if seen.contains(name) {
                    return Ok(Vec::new());
                }

                let mut next = seen.clone();
                next.insert(name);
                let mut numbers = Numbers::default();

                for initializer in self.get(name) {
                    let shape = match initializer {
                        Node::Expression(expression) => expression_shape(expression),
                        _ => Shape::Other,
                    };

                    for candidate in self.numbers(shape, &next, budget, depth + 1)? {
                        numbers.add(candidate, budget, depth + 1)?;
                    }
                }

                Ok(numbers.0)
            }
            Shape::Number(argument) => {
                let mut numbers = Numbers::default();
                let shape = argument
                    .as_expression()
                    .map_or(Shape::Other, expression_shape);

                for candidate in self.numbers(shape, seen, budget, depth + 1)? {
                    numbers.add(candidate, budget, depth + 1)?;
                }

                for text in self.fresh_candidates(argument_node(argument))? {
                    numbers.add(number(&String::from_utf16_lossy(&text)), budget, depth + 1)?;
                }

                Ok(numbers.0)
            }
            Shape::Unary(operator, operand) => {
                let mut numbers = Numbers::default();

                for value in self.numbers(expression_shape(operand), seen, budget, depth + 1)? {
                    let candidate = match operator {
                        UnaryOperator::UnaryNegation => -value,
                        UnaryOperator::UnaryPlus => value,
                        UnaryOperator::BitwiseNot => f64::from(!to_int32(value)),
                        _ => continue,
                    };

                    numbers.add(candidate, budget, depth + 1)?;
                }

                Ok(numbers.0)
            }
            Shape::Binary(operation, left, right) => {
                let left = match left {
                    Side::Expression(left) => {
                        self.numbers(expression_shape(left), seen, budget, depth + 1)?
                    }
                    Side::Name(name) => self.numbers(Shape::Name(name), seen, budget, depth + 1)?,
                    Side::Other => self.numbers(Shape::Other, seen, budget, depth + 1)?,
                };
                let right = self.numbers(expression_shape(right), seen, budget, depth + 1)?;
                let mut numbers = Numbers::default();

                if let Some(operation) = operation {
                    for &left in &left {
                        for &right in &right {
                            numbers.add(operation.apply(left, right), budget, depth + 1)?;
                        }
                    }
                }

                Ok(numbers.0)
            }
            Shape::Sequence(expressions) => {
                // TypeScript reads `a, b, c` as `(a, b), c`, a comma operation whose sides are
                // evaluated and which yields no number.
                if let [init @ .., last] = expressions {
                    self.comma(init, seen, budget, depth + 1)?;
                    self.numbers(expression_shape(last), seen, budget, depth + 1)?;
                }

                Ok(Vec::new())
            }
            Shape::Other => Ok(Vec::new()),
        }
    }

    /// The left side of a comma operation: one expression, or the nested comma operation of the
    /// expressions before the last, evaluated for their budget alone.
    fn comma(
        &self,
        expressions: &'a [Expression<'a>],
        seen: &Seen<'a>,
        budget: &mut u32,
        depth: usize,
    ) -> Outcome<()> {
        match expressions {
            [] => Ok(()),
            [only] => self
                .numbers(expression_shape(only), seen, budget, depth)
                .map(|_| ()),
            [init @ .., last] => {
                consume(budget, depth)?;
                self.comma(init, seen, budget, depth + 1)?;
                self.numbers(expression_shape(last), seen, budget, depth + 1)
                    .map(|_| ())
            }
        }
    }
}

/// Whether a byte is one of `[A-Za-z0-9_]`.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The values of the raw numeric literals of a text, as
/// `/(?<![A-Za-z0-9_])-?(?:0[xX]…|0[bB]…|0[oO]…|\d(?:_?\d)*)n?(?![A-Za-z0-9_])/gu` finds them.
///
/// The text is scanned as bytes: every character the pattern tests is ASCII, and each byte of a
/// character that is not is at least `0x80`, so it is neither a digit nor a word character, as
/// the character is not; the matches are the matches of the code points.
fn raw_literals(text: &str) -> Vec<f64> {
    let bytes = text.as_bytes();
    let mut values = Vec::new();
    let mut resume = 0;
    let digits = |from: usize, accepts: fn(u8) -> bool| {
        // One digit, then digits each optionally preceded by one separator.
        if !bytes.get(from).copied().is_some_and(accepts) {
            return None;
        }

        let mut end = from + 1;

        loop {
            match bytes.get(end) {
                Some(&digit) if accepts(digit) => end += 1,
                Some(b'_') if bytes.get(end + 1).copied().is_some_and(accepts) => end += 2,
                _ => return Some(end),
            }
        }
    };

    for (start, &first) in bytes.iter().enumerate() {
        let body = start + usize::from(first == b'-');

        // Every form begins with a digit at `body`, so the other positions are passed at once.
        if !bytes[body..].first().is_some_and(u8::is_ascii_digit) {
            continue;
        }

        // A match resumes the search after its end, as `matchAll` does.
        if start < resume || (start > 0 && is_word(bytes[start - 1])) {
            continue;
        }

        let prefixed = |marks: [u8; 2], accepts: fn(u8) -> bool| {
            (bytes.get(body) == Some(&b'0')
                && bytes.get(body + 1).is_some_and(|mark| marks.contains(mark)))
            .then(|| digits(body + 2, accepts))
            .flatten()
        };
        let end = prefixed(*b"xX", |digit| digit.is_ascii_hexdigit())
            .or_else(|| prefixed(*b"bB", |digit| matches!(digit, b'0' | b'1')))
            .or_else(|| prefixed(*b"oO", |digit| matches!(digit, b'0'..=b'7')))
            .or_else(|| digits(body, |digit| digit.is_ascii_digit()));
        let Some(mut end) = end else {
            continue;
        };

        if bytes.get(end) == Some(&b'n') {
            end += 1;
        }

        if bytes.get(end).copied().is_some_and(is_word) {
            continue;
        }

        // The match is ASCII, so it is whole characters of the text.
        values.push(literal_value(&text[start..end]));
        resume = end;
    }

    values
}

/// Every value a syntax node of a parsed source evaluates to.
pub(super) fn evaluated(file: &SourceFile<'_>) -> Outcome<Vec<f64>> {
    evaluated_with(file, &Interpreter::new(file))
}

/// [`evaluated`] with the file's reconstruction already built.
pub(super) fn evaluated_with<'a>(
    file: &SourceFile<'a>,
    interpreter: &Interpreter<'a>,
) -> Outcome<Vec<f64>> {
    // A name evaluated from the top depends on the name alone, so each is evaluated once.
    let mut names: HashSet<&str> = HashSet::default();
    let mut values = Vec::new();

    for node in file.semantic().nodes().iter() {
        let shape = kind_shape(node.kind());

        match shape {
            Shape::Other => continue,
            Shape::Name(name) if !names.insert(name) => continue,
            _ => {}
        }

        values.extend(interpreter.numbers(shape, &Seen::default(), &mut 0, 0)?);
    }

    Ok(values)
}

/// The registered identifiers a text holds, in the order of `registered`: every raw numeric
/// literal and every value a syntax node evaluates to, or, where the text does not parse, every
/// value the recovery reads.
pub(super) fn registered_ids<'a>(
    text: &str,
    file: Option<(&SourceFile<'a>, &Interpreter<'a>)>,
    registered: &[i64],
) -> Outcome<Vec<i64>> {
    let mut values = raw_literals(text);

    if let Some((file, interpreter)) = file {
        values.extend(evaluated_with(file, interpreter)?);
    }

    // A text with syntax errors may hold values only the compiler's recovery reached.
    if file.is_none_or(|(file, _)| !file.errors().is_empty()) {
        values.extend(super::recovery::recovered(text)?);
    }

    // Each identifier is looked up among the text's values as a double: a value equals it exactly
    // when their bits are the same, once both zeros are one and NaN, equal to nothing, is left out.
    let values: HashSet<u64> = values
        .into_iter()
        .filter(|value| !value.is_nan())
        .map(double_key)
        .collect();
    #[expect(
        clippy::cast_precision_loss,
        reason = "registered identifiers are 32-bit integers, exact as doubles"
    )]
    let present = registered
        .iter()
        .copied()
        .filter(|&id| values.contains(&double_key(id as f64)))
        .collect();

    Ok(present)
}

/// The key under which a double that is not NaN equals another: its bits, with `-0` as `0`.
fn double_key(value: f64) -> u64 {
    if value == 0.0 { 0.0_f64 } else { value }.to_bits()
}
