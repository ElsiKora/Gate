//! Statically known values of expressions, read across files without executing code.
//!
//! A value is known when it is a string, number, boolean or `null` literal, a string
//! concatenation, a built-in `String.raw` template, a `const` binding, a property of a known object
//! literal, or an import or re-export of one of those from a tracked file. Calls, getters, `let` and
//! `var` bindings, spreads that may replace a property and cycles stay unknown.

use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, ChainElement, Expression, ImportOrExportKind, ModuleExportName,
    ObjectExpression, ObjectPropertyKind, PropertyKey, PropertyKind, Statement,
    TaggedTemplateExpression, TemplateLiteral, VariableDeclarationKind,
};
use oxc_span::GetSpan;
use oxc_syntax::operator::BinaryOperator;
use oxc_syntax::symbol::SymbolId;

use super::{SourceFile, TypeScript};
use crate::Result;

/// A statically known value.
#[derive(Clone, Debug, PartialEq)]
pub enum StaticValue {
    /// A string.
    String(String),
    /// A number.
    Number(f64),
    /// A boolean.
    Boolean(bool),
    /// `null`.
    Null,
}

/// What is being read on the current path: `(file, start, end)` of expressions, and
/// `(file, u32::MAX, u32::MAX)` of files whose exports are read; a repeat is a cycle.
type Active = Vec<(String, u32, u32)>;

/// Reads static values through the parse cache and the module resolver of one run.
#[derive(Debug)]
pub struct ValueReader<'t, 'a> {
    typescript: &'t TypeScript<'a>,
}

impl<'t, 'a> ValueReader<'t, 'a> {
    /// A reader over the files and the resolver of `typescript`.
    #[must_use]
    pub fn new(typescript: &'t TypeScript<'a>) -> Self {
        Self { typescript }
    }

    /// The value of `expression` in `file`.
    ///
    /// # Errors
    /// Fails when an imported tracked file cannot be read or resolution fails.
    pub fn read(
        &self,
        file: &SourceFile<'a>,
        expression: &Expression<'a>,
    ) -> Result<Option<StaticValue>> {
        self.expression(file, expression, &[], &Vec::new())
    }

    /// The value of the export `name` of `file`, followed through `properties`.
    ///
    /// # Errors
    /// Fails when an imported tracked file cannot be read or resolution fails.
    pub fn read_export(
        &self,
        file: &SourceFile<'a>,
        name: &str,
        properties: &[String],
    ) -> Result<Option<StaticValue>> {
        self.export(file, name, properties, &Vec::new())
    }

    fn expression(
        &self,
        file: &SourceFile<'a>,
        expression: &Expression<'a>,
        properties: &[String],
        active: &Active,
    ) -> Result<Option<StaticValue>> {
        // Symbols and references exist only once the semantic model is built.
        let _ = file.semantic();
        let value = unwrap(expression);
        let span = value.span();
        let key = (file.path().to_owned(), span.start, span.end);

        if active.contains(&key) {
            return Ok(None);
        }

        let mut next = active.clone();
        next.push(key);
        let nested = |name: String| -> Vec<String> {
            std::iter::once(name)
                .chain(properties.iter().cloned())
                .collect()
        };

        match value {
            Expression::Identifier(identifier) => {
                let symbol = identifier.reference_id.get().and_then(|reference| {
                    file.semantic()
                        .scoping()
                        .get_reference(reference)
                        .symbol_id()
                });

                match symbol {
                    Some(symbol) => self.symbol(file, symbol, properties, &next),
                    None => Ok(None),
                }
            }
            Expression::StaticMemberExpression(member) => self.expression(
                file,
                &member.object,
                &nested(member.property.name.to_string()),
                &next,
            ),
            Expression::ComputedMemberExpression(member) => match string_like(&member.expression) {
                Some(name) => self.expression(file, &member.object, &nested(name), &next),
                None => Ok(None),
            },
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::StaticMemberExpression(member) => self.expression(
                    file,
                    &member.object,
                    &nested(member.property.name.to_string()),
                    &next,
                ),
                ChainElement::ComputedMemberExpression(member) => {
                    match string_like(&member.expression) {
                        Some(name) => self.expression(file, &member.object, &nested(name), &next),
                        None => Ok(None),
                    }
                }
                _ => Ok(None),
            },
            Expression::ObjectExpression(object) if !properties.is_empty() => {
                self.object_property(file, object, properties, &next)
            }
            _ if !properties.is_empty() => Ok(None),
            Expression::StringLiteral(literal) => {
                Ok(Some(StaticValue::String(literal.value.to_string())))
            }
            Expression::TemplateLiteral(template) => {
                Ok(no_substitution(template).map(StaticValue::String))
            }
            Expression::NumericLiteral(literal) => Ok(Some(StaticValue::Number(literal.value))),
            Expression::BooleanLiteral(literal) => Ok(Some(StaticValue::Boolean(literal.value))),
            Expression::NullLiteral(_) => Ok(Some(StaticValue::Null)),
            Expression::BinaryExpression(binary) if binary.operator == BinaryOperator::Addition => {
                let left = self.expression(file, &binary.left, &[], &next)?;
                let right = self.expression(file, &binary.right, &[], &next)?;

                Ok(match (left, right) {
                    (Some(StaticValue::String(left)), Some(StaticValue::String(right))) => {
                        Some(StaticValue::String(left + &right))
                    }
                    _ => None,
                })
            }
            Expression::TaggedTemplateExpression(tagged) => {
                Ok(raw_template(file, tagged).map(StaticValue::String))
            }
            _ => Ok(None),
        }
    }

    /// The value of the last property named `properties[0]` of an object literal, followed through
    /// the remaining properties; unknown when a later spread or computed key may replace it.
    fn object_property(
        &self,
        file: &SourceFile<'a>,
        object: &ObjectExpression<'a>,
        properties: &[String],
        active: &Active,
    ) -> Result<Option<StaticValue>> {
        let (name, remaining) = (&properties[0], &properties[1..]);

        for property in object.properties.iter().rev() {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return Ok(None);
            };

            if property.computed {
                return Ok(None);
            }

            if property_key_text(&property.key).as_deref() != Some(name.as_str()) {
                continue;
            }

            // A method or accessor has no static value.
            if property.method || property.kind != PropertyKind::Init {
                return Ok(None);
            }

            return self.expression(file, &property.value, remaining, active);
        }

        Ok(None)
    }

    /// The value a binding denotes: a `const` initializer or an imported value.
    fn symbol(
        &self,
        file: &SourceFile<'a>,
        symbol: SymbolId,
        properties: &[String],
        active: &Active,
    ) -> Result<Option<StaticValue>> {
        let semantic = file.semantic();
        let nodes = semantic.nodes();
        let declarations: Vec<_> = semantic.scoping().symbol_declarations(symbol).collect();
        let [declaration] = declarations.as_slice() else {
            return Ok(None);
        };

        match nodes.kind(*declaration) {
            AstKind::VariableDeclarator(declarator) => {
                let BindingPattern::BindingIdentifier(binding) = &declarator.id else {
                    return Ok(None);
                };
                let constant = matches!(
                    nodes.parent_kind(*declaration),
                    AstKind::VariableDeclaration(list) if list.kind == VariableDeclarationKind::Const
                );

                match &declarator.init {
                    Some(init) if constant && binding.symbol_id.get() == Some(symbol) => {
                        self.expression(file, init, properties, active)
                    }
                    _ => Ok(None),
                }
            }
            AstKind::ImportSpecifier(specifier) => {
                let AstKind::ImportDeclaration(import) = nodes.parent_kind(*declaration) else {
                    return Ok(None);
                };

                if specifier.import_kind == ImportOrExportKind::Type
                    || import.import_kind == ImportOrExportKind::Type
                {
                    return Ok(None);
                }

                self.imported(
                    file,
                    &import.source.value,
                    &export_name(&specifier.imported),
                    properties,
                    active,
                )
            }
            AstKind::ImportNamespaceSpecifier(_) if !properties.is_empty() => {
                let AstKind::ImportDeclaration(import) = nodes.parent_kind(*declaration) else {
                    return Ok(None);
                };

                if import.import_kind == ImportOrExportKind::Type {
                    return Ok(None);
                }

                self.imported(
                    file,
                    &import.source.value,
                    &properties[0],
                    &properties[1..],
                    active,
                )
            }
            _ => Ok(None),
        }
    }

    fn imported(
        &self,
        file: &SourceFile<'a>,
        specifier: &str,
        name: &str,
        properties: &[String],
        active: &Active,
    ) -> Result<Option<StaticValue>> {
        let Some(target) = self.typescript.resolve(file.path(), specifier)? else {
            return Ok(None);
        };
        let Some(target) = self.typescript.parse(&target)? else {
            return Ok(None);
        };

        self.export(&target, name, properties, active)
    }

    fn export(
        &self,
        file: &SourceFile<'a>,
        name: &str,
        properties: &[String],
        active: &Active,
    ) -> Result<Option<StaticValue>> {
        let _ = file.semantic();
        let key = (file.path().to_owned(), u32::MAX, u32::MAX);

        if active.contains(&key) {
            return Ok(None);
        }

        let mut next = active.clone();
        next.push(key);
        let body = &file.program().body;
        let mut stars = Vec::new();

        for statement in body {
            match statement {
                Statement::ExportDeclaration(export) => {
                    let oxc_ast::ast::Declaration::VariableDeclaration(list) = &export.declaration
                    else {
                        continue;
                    };

                    if list.kind != VariableDeclarationKind::Const {
                        continue;
                    }

                    for declarator in &list.declarations {
                        if matches!(&declarator.id, BindingPattern::BindingIdentifier(binding) if binding.name == name)
                        {
                            return match &declarator.init {
                                Some(init) => self.expression(file, init, properties, &next),
                                None => Ok(None),
                            };
                        }
                    }
                }
                Statement::ExportAllDeclaration(export)
                    if export.export_kind != ImportOrExportKind::Type =>
                {
                    if export.exported.is_none() {
                        stars.push(export.source.value.to_string());
                    }
                }
                Statement::ExportFromDeclaration(export)
                    if export.export_kind != ImportOrExportKind::Type =>
                {
                    for specifier in &export.specifiers {
                        if specifier.export_kind == ImportOrExportKind::Type
                            || export_name(&specifier.exported) != name
                        {
                            continue;
                        }

                        return self.imported(
                            file,
                            &export.source.value,
                            &export_name(&specifier.local),
                            properties,
                            &next,
                        );
                    }
                }
                Statement::ExportNamedDeclaration(export)
                    if export.export_kind != ImportOrExportKind::Type =>
                {
                    for specifier in &export.specifiers {
                        if specifier.export_kind == ImportOrExportKind::Type
                            || export_name(&specifier.exported) != name
                        {
                            continue;
                        }

                        // An export alias names a binding of this file; it creates no new value.
                        let local = export_name(&specifier.local);

                        return match local_binding(body, &local) {
                            Some(symbol) => self.symbol(file, symbol, properties, &next),
                            None => Ok(None),
                        };
                    }
                }
                _ => {}
            }
        }

        let mut values = Vec::new();

        for star in stars {
            if let Some(value) = self.imported(file, &star, name, properties, &next)? {
                values.push(value);
            }
        }

        Ok(match values.as_slice() {
            [value] => Some(value.clone()),
            _ => None,
        })
    }
}

/// The raw text of a built-in ``String.raw`...` `` template without substitutions; a local or
/// imported `String` is not the built-in.
fn raw_template(file: &SourceFile<'_>, tagged: &TaggedTemplateExpression<'_>) -> Option<String> {
    let Expression::StaticMemberExpression(tag) = &tagged.tag else {
        return None;
    };
    let Expression::Identifier(object) = &tag.object else {
        return None;
    };
    let built_in = object.name == "String"
        && tag.property.name == "raw"
        && object.reference_id.get().is_none_or(|reference| {
            file.semantic()
                .scoping()
                .get_reference(reference)
                .symbol_id()
                .is_none()
        });

    match tagged.quasi.quasis.as_slice() {
        [quasi] if built_in && tagged.quasi.expressions.is_empty() => {
            Some(quasi.value.raw.to_string())
        }
        _ => None,
    }
}

/// The symbol of the top-level variable named `name`, else of the named import with that local
/// name.
fn local_binding(body: &[Statement<'_>], name: &str) -> Option<SymbolId> {
    let variables = body.iter().filter_map(|statement| match statement {
        Statement::VariableDeclaration(list) => Some(list.as_ref()),
        Statement::ExportDeclaration(export) => match &export.declaration {
            oxc_ast::ast::Declaration::VariableDeclaration(list) => Some(list.as_ref()),
            _ => None,
        },
        _ => None,
    });

    for list in variables {
        for declarator in &list.declarations {
            if let BindingPattern::BindingIdentifier(binding) = &declarator.id
                && binding.name == name
            {
                return binding.symbol_id.get();
            }
        }
    }

    body.iter()
        .filter_map(|statement| match statement {
            Statement::ImportDeclaration(import) => import.specifiers.as_ref(),
            _ => None,
        })
        .flatten()
        .find_map(|specifier| match specifier {
            oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(specifier)
                if specifier.local.name == name =>
            {
                specifier.local.symbol_id.get()
            }
            _ => None,
        })
}

fn export_name(name: &ModuleExportName<'_>) -> String {
    match name {
        ModuleExportName::IdentifierName(name) => name.name.to_string(),
        ModuleExportName::IdentifierReference(name) => name.name.to_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.to_string(),
    }
}

/// The text of a property key that is an identifier, string or number.
fn property_key_text(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
        PropertyKey::NumericLiteral(literal) => Some(number_text(literal.value)),
        _ => None,
    }
}

/// A number as JavaScript's `String(number)` prints it: the shortest digits that read back to the
/// same value, in positional notation from 1e-6 up to 1e21 and in exponent notation outside it.
///
/// # Panics
/// Never: Rust's exponent notation of a finite number always carries an integer exponent.
#[must_use]
pub fn number_text(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }

    if value == 0.0 {
        return "0".to_owned();
    }

    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }

    let sign = if value < 0.0 { "-" } else { "" };
    // Rust prints the shortest round-trip digits; `{:e}` gives them with their exponent.
    let scientific = format!("{:e}", value.abs());
    // `{:e}` of a finite number always has an integer exponent and at most 17 digits; the
    // fallbacks keep this function total without a panic.
    let Some((mantissa, exponent)) = scientific.split_once('e') else {
        return value.to_string();
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let Ok(exponent) = exponent.parse::<i64>() else {
        return value.to_string();
    };
    let (digits, exponent) = even_tie(value.abs(), digits, exponent);
    let count = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    // The decimal point sits after `point` digits.
    let point = exponent + 1;
    let zeros = |count: i64| "0".repeat(usize::try_from(count).unwrap_or(0));
    let body = if count <= point && point <= 21 {
        format!("{digits}{}", zeros(point - count))
    } else if 0 < point && point <= 21 {
        let (whole, fraction) = digits.split_at(usize::try_from(point).unwrap_or(0));
        format!("{whole}.{fraction}")
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", zeros(-point))
    } else {
        let exponent = point - 1;
        let sign = if exponent < 0 { '-' } else { '+' };
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        format!("{first}{fraction}e{sign}{}", exponent.abs())
    };

    format!("{sign}{body}")
}

/// The shortest digits JavaScript picks: when the value lies exactly halfway between two shortest
/// candidates that both read back to it, `Number::toString` takes the one with the even last
/// digit, and Rust may take the other. Rust's fixed-precision exponent notation prints the exact
/// decimal value, which tells a tie apart. A candidate that does not read back never counts: below
/// a power of two such as 2^-24 the next binary value lies half as far away as above it, so the
/// interval that reads back is narrower below and the lower candidate can fall outside it.
fn even_tie(value: f64, digits: String, exponent: i64) -> (String, i64) {
    // An `f64` has at most 767 significant decimal digits.
    let exact = format!("{value:.800e}");
    let Some((mantissa, exact_exponent)) = exact.split_once('e') else {
        return (digits, exponent);
    };
    let exact_digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();

    if exact_exponent.parse::<i64>().ok() != Some(exponent) || exact_digits.len() <= digits.len() {
        return (digits, exponent);
    }

    let (lower, rest) = exact_digits.split_at(digits.len());

    if rest.first() != Some(&b'5') || rest[1..].iter().any(|digit| *digit != b'0') {
        return (digits, exponent);
    }

    // The exact value of a tie is an odd multiple of 2^-p, where 10^-p is the place of its final 5.
    // Both candidates lie 5 * 10^-p away, which for p <= 1 exceeds the half binary step of at most
    // 2^-(p + 1), so a tie has p >= 2. An odd multiple of 2^-p = 5^p / 10^p then ends in 25 or 75:
    // the digit before the final 5 is a 2 or a 7, and the upper candidate raises only the last
    // digit.
    let Some(&last) = lower.last() else {
        return (digits, exponent);
    };

    if last == b'9' {
        return (digits, exponent);
    }

    let mut upper = lower.to_vec();
    upper[lower.len() - 1] = last + 1;
    let candidates = if (last - b'0').is_multiple_of(2) {
        [lower.to_vec(), upper]
    } else {
        [upper, lower.to_vec()]
    };

    candidates
        .into_iter()
        .map(|candidate| String::from_utf8_lossy(&candidate).into_owned())
        .find(|candidate| reads_back(candidate, exponent, value))
        .map_or((digits, exponent), |candidate| (candidate, exponent))
}

/// Whether the digits with the exponent of their first digit read back to `value`.
fn reads_back(digits: &str, exponent: i64, value: f64) -> bool {
    let (first, rest) = digits.split_at(1);
    let text = if rest.is_empty() {
        format!("{first}e{exponent}")
    } else {
        format!("{first}.{rest}e{exponent}")
    };

    text.parse::<f64>() == Ok(value)
}

/// The text of a string literal or a template literal without substitutions.
#[must_use]
pub fn string_like(expression: &Expression<'_>) -> Option<String> {
    match expression {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::TemplateLiteral(template) => no_substitution(template),
        _ => None,
    }
}

fn no_substitution(template: &TemplateLiteral<'_>) -> Option<String> {
    match template.quasis.as_slice() {
        [quasi] if template.expressions.is_empty() => {
            quasi.value.cooked.as_ref().map(ToString::to_string)
        }
        _ => None,
    }
}

/// `expression` without parentheses, `as`, `satisfies`, type assertions and non-null assertions.
#[must_use]
pub fn unwrap<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    loop {
        expression = match expression {
            Expression::ParenthesizedExpression(inner) => &inner.expression,
            Expression::TSAsExpression(inner) => &inner.expression,
            Expression::TSSatisfiesExpression(inner) => &inner.expression,
            Expression::TSTypeAssertion(inner) => &inner.expression,
            Expression::TSNonNullExpression(inner) => &inner.expression,
            _ => return expression,
        };
    }
}
