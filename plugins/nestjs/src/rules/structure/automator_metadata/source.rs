//! One parsed source file read the way the automator's own inventory reads it: a file-wide map of
//! imported names, decorator and expression names, and the semantic value of a metadata
//! expression.

use std::collections::{BTreeMap, HashMap};

use gate_typescript::SourceFile;
use gate_typescript::oxc_ast::ast::{
    Argument, BindingPattern, CallExpression, ChainElement, Declaration, Decorator, Expression,
    ImportDeclarationSpecifier, ObjectExpression, ObjectPropertyKind, PropertyKey, PropertyKind,
    Statement, StaticMemberExpression,
};
use gate_typescript::oxc_span::{GetSpan, Span};
use gate_typescript::value::{self, number_text};

use super::policy::Policy;

/// Automator decorators that keep their bare name only when imported from Automator.
const AUTOMATOR_DECORATORS: [&str; 5] = [
    "ApiController",
    "ApiControllerObservable",
    "ApiControllerSecurable",
    "ApiMethod",
    "ApiRouteCustom",
];

/// A name a file imports: the imported name (`*` for a namespace) and its module.
#[derive(Clone, Debug)]
pub struct Imported {
    pub name: String,
    pub module: String,
}

/// One source file with its imports.
pub struct Source<'s, 'a> {
    pub path: &'s str,
    pub file: &'s SourceFile<'a>,
    /// What the rule expects.
    pub policy: &'s Policy,
    imports: HashMap<String, Imported>,
}

impl<'s, 'a> Source<'s, 'a> {
    /// Reads the named and namespace imports of the file's top-level import declarations; a later
    /// import of a local name replaces an earlier one, and default imports are not read.
    pub fn new(path: &'s str, file: &'s SourceFile<'a>, policy: &'s Policy) -> Self {
        let mut imports = HashMap::new();

        for statement in &file.program().body {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };
            let Some(specifiers) = &import.specifiers else {
                continue;
            };
            let module = import.source.value.to_string();
            let namespace = specifiers.iter().find_map(|specifier| match specifier {
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                    Some(namespace.local.name.to_string())
                }
                _ => None,
            });

            if let Some(namespace) = namespace {
                imports.insert(
                    namespace,
                    Imported {
                        name: "*".to_owned(),
                        module,
                    },
                );
                continue;
            }

            for specifier in specifiers {
                if let ImportDeclarationSpecifier::ImportSpecifier(named) = specifier {
                    imports.insert(
                        named.local.name.to_string(),
                        Imported {
                            name: named.imported.name().to_string(),
                            module: module.clone(),
                        },
                    );
                }
            }
        }

        Self {
            path,
            file,
            policy,
            imports,
        }
    }

    /// The one-based line of a node's first token.
    pub fn line(&self, span: Span) -> u32 {
        self.file.line(span.start)
    }

    /// The source text of a span.
    pub fn text(&self, span: Span) -> &'a str {
        self.file
            .source()
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default()
    }

    /// The import an identifier or a namespace member names.
    pub fn imported(&self, expression: &Expression<'a>) -> Option<Imported> {
        let current = unwrap(expression);

        if let Expression::Identifier(identifier) = current {
            return self.imports.get(identifier.name.as_str()).cloned();
        }

        let member = member(current)?;
        let Expression::Identifier(object) = &member.object else {
            return None;
        };
        self.imports
            .get(object.name.as_str())
            .filter(|namespace| namespace.name == "*")
            .map(|namespace| Imported {
                name: member.property.name.to_string(),
                module: namespace.module.clone(),
            })
    }

    /// The name a decorator callee resolves to; an Automator or framework decorator imported from
    /// elsewhere is qualified with its module (`local` when not imported).
    pub fn expression_name(&self, expression: &Expression<'a>) -> Option<String> {
        let current = unwrap(expression);
        let imported = self.imported(current);
        let identifier = match current {
            Expression::Identifier(identifier) => Some(identifier.name.to_string()),
            _ => None,
        };
        let name = imported
            .as_ref()
            .map(|imported| imported.name.clone())
            .or_else(|| identifier.clone());
        let module = imported.as_ref().map(|imported| imported.module.as_str());

        if let Some(name) = &name {
            if name.starts_with("ApiProperty") || AUTOMATOR_DECORATORS.contains(&name.as_str()) {
                return Some(if module == Some(self.policy.automator_package.as_str()) {
                    name.clone()
                } else {
                    format!("{}::{name}", module.unwrap_or("local"))
                });
            }

            if let Some(framework) = self.policy.decorator_modules.get(name)
                && module != Some(framework.as_str())
            {
                return Some(format!("{}::{name}", module.unwrap_or("local")));
            }
        }

        imported.map(|imported| imported.name).or(identifier)
    }

    /// The semantic value of a metadata expression, as the JavaScript inventory compared it.
    pub fn semantic(&self, expression: Option<&Expression<'a>>) -> Semantic {
        let Some(expression) = expression else {
            return Semantic::Str("[Missing]".to_owned());
        };

        match unwrap(expression) {
            Expression::StringLiteral(literal) => Semantic::Str(literal.value.to_string()),
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                Semantic::Str(
                    template
                        .quasis
                        .first()
                        .and_then(|quasi| quasi.value.cooked)
                        .map_or_else(String::new, |cooked| cooked.to_string()),
                )
            }
            Expression::NumericLiteral(number) => Semantic::Num(number.value),
            Expression::BooleanLiteral(boolean) => Semantic::Bool(boolean.value),
            Expression::NullLiteral(_) => Semantic::Null,
            Expression::Identifier(identifier) => {
                Semantic::Str(self.imports.get(identifier.name.as_str()).map_or_else(
                    || identifier.name.to_string(),
                    |imported| imported.name.clone(),
                ))
            }
            current @ (Expression::StaticMemberExpression(_) | Expression::ChainExpression(_))
                if let Some(member) = member(current) =>
            {
                match self.imported(current) {
                    Some(imported) => Semantic::Str(imported.name),
                    None => Semantic::Str(format!(
                        "{}.{}",
                        self.semantic(Some(&member.object)).string(),
                        member.property.name
                    )),
                }
            }
            Expression::ArrowFunctionExpression(arrow) => match arrow.get_expression() {
                Some(body) => self.semantic(Some(body)),
                None => Semantic::Str(strip_space(self.text(arrow.body.span()))),
            },
            Expression::TaggedTemplateExpression(tagged) if tagged.quasi.expressions.is_empty() => {
                Semantic::Str(
                    tagged
                        .quasi
                        .quasis
                        .first()
                        .map_or_else(String::new, |quasi| quasi.value.raw.to_string()),
                )
            }
            other => Semantic::Str(strip_space(self.text(other.span()))),
        }
    }
}

/// A metadata value compared as the JavaScript inventory serialized it.
#[derive(Clone, Debug, PartialEq)]
pub enum Semantic {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
    Arr(Vec<Semantic>),
    Obj(BTreeMap<String, Semantic>),
}

impl Semantic {
    /// JavaScript `String(value)` of a scalar.
    pub fn string(&self) -> String {
        match self {
            Self::Str(text) => text.clone(),
            Self::Num(number) => number_string(*number),
            Self::Bool(flag) => flag.to_string(),
            Self::Null => "null".to_owned(),
            Self::Arr(items) => items.iter().map(Self::string).collect::<Vec<_>>().join(","),
            Self::Obj(_) => "[object Object]".to_owned(),
        }
    }

    /// `JSON.stringify` of the value with sorted keys and the special numbers named.
    pub fn json(&self) -> String {
        match self {
            Self::Str(text) => gate_sdk::serde_json::to_string(text).unwrap_or_default(),
            Self::Num(number) if number.is_nan() => "\"[NaN]\"".to_owned(),
            Self::Num(number) if number.is_infinite() && *number > 0.0 => {
                "\"[Infinity]\"".to_owned()
            }
            Self::Num(number) if number.is_infinite() => "\"[-Infinity]\"".to_owned(),
            Self::Num(number) => number_text(*number),
            Self::Bool(flag) => flag.to_string(),
            Self::Null => "null".to_owned(),
            Self::Arr(items) => format!(
                "[{}]",
                items.iter().map(Self::json).collect::<Vec<_>>().join(",")
            ),
            Self::Obj(entries) => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        gate_sdk::serde_json::to_string(key).unwrap_or_default(),
                        value.json()
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
}

fn number_string(number: f64) -> String {
    if number.is_nan() {
        "NaN".to_owned()
    } else if number.is_infinite() {
        if number > 0.0 {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_owned()
    } else {
        number_text(number)
    }
}

/// The text with JavaScript's white space removed, as `replace(/\s+/gu, "")` leaves it.
fn strip_space(text: &str) -> String {
    text.chars()
        .filter(|character| {
            !matches!(
                character,
                '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200a}'
                        | '\u{2028}'
                        | '\u{2029}'
                        | '\u{202f}'
                        | '\u{205f}'
                        | '\u{3000}'
                        | '\u{feff}'
            )
        })
        .collect()
}

/// The expression inside `as`, `!` (also at the end of an optional chain), parentheses,
/// `satisfies` and `<T>` assertions.
pub fn unwrap<'e, 'a>(mut expression: &'e Expression<'a>) -> &'e Expression<'a> {
    loop {
        expression = match value::unwrap(expression) {
            // `a?.b!` ends its chain in the assertion, which the TypeScript tree keeps outside.
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::TSNonNullExpression(inner) => &inner.expression,
                _ => return value::unwrap(expression),
            },
            other => return other,
        };
    }
}

/// A `.name` member access, also as the outermost link of an optional chain, which the
/// TypeScript tree the JavaScript read keeps as an ordinary member access.
pub fn member<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e StaticMemberExpression<'a>> {
    match expression {
        Expression::StaticMemberExpression(member) => Some(member),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::StaticMemberExpression(member) => Some(member),
            _ => None,
        },
        _ => None,
    }
}

/// A call, also as the outermost link of an optional chain.
pub fn call<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e CallExpression<'a>> {
    match expression {
        Expression::CallExpression(call) => Some(call),
        Expression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => Some(call),
            _ => None,
        },
        _ => None,
    }
}

/// The call of a decorator, when its expression is one.
pub fn decorator_call<'d, 'a>(decorator: &'d Decorator<'a>) -> Option<&'d CallExpression<'a>> {
    call(unwrap(&decorator.expression))
}

/// The first argument of a call, unwrapped, when it is an expression.
pub fn first_argument<'c, 'a>(call: &'c CallExpression<'a>) -> Option<&'c Expression<'a>> {
    call.arguments
        .first()
        .and_then(Argument::as_expression)
        .map(unwrap)
}

/// A property name as the inventory keyed it: an identifier, private name or string literal, or
/// a computed member access or string literal; other names have none.
pub fn property_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    if !computed {
        return match key {
            PropertyKey::StaticIdentifier(name) => Some(name.name.to_string()),
            PropertyKey::PrivateIdentifier(name) => Some(format!("#{}", name.name)),
            PropertyKey::StringLiteral(literal) => Some(literal.value.to_string()),
            _ => None,
        };
    }

    let expression = unwrap(key.as_expression()?);

    if let Expression::StringLiteral(literal) = expression {
        return Some(literal.value.to_string());
    }

    member(expression).map(|member| member.property.name.to_string())
}

/// The value of the first `name: value` or shorthand `name` property of an object literal.
pub fn object_property<'o, 'a>(
    object: Option<&'o ObjectExpression<'a>>,
    name: &str,
) -> Option<&'o Expression<'a>> {
    object?
        .properties
        .iter()
        .find_map(|property| match property {
            ObjectPropertyKind::ObjectProperty(property)
                if !property.method
                    && property.kind == PropertyKind::Init
                    && property_name(&property.key, property.computed).as_deref() == Some(name) =>
            {
                Some(&property.value)
            }
            _ => None,
        })
}

/// Whether an expression is the literal `false`, read without unwrapping.
pub fn is_false(expression: &Expression<'_>) -> bool {
    matches!(expression, Expression::BooleanLiteral(boolean) if !boolean.value)
}

/// The object literal an expression is, after unwrapping.
pub fn as_object<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e ObjectExpression<'a>> {
    match unwrap(expression) {
        Expression::ObjectExpression(object) => Some(object),
        _ => None,
    }
}

/// The object literal a top-level variable named `name` is initialized with; the first variable
/// of that name decides.
pub fn variable_object<'b, 'a>(
    body: &'b [Statement<'a>],
    name: &str,
) -> Option<&'b ObjectExpression<'a>> {
    for statement in body {
        let declaration = match statement {
            Statement::VariableDeclaration(declaration) => &**declaration,
            Statement::ExportDeclaration(export) => match &export.declaration {
                Declaration::VariableDeclaration(declaration) => &**declaration,
                _ => continue,
            },
            _ => continue,
        };

        for declarator in &declaration.declarations {
            if let BindingPattern::BindingIdentifier(identifier) = &declarator.id
                && identifier.name == name
            {
                return declarator.init.as_ref().and_then(as_object);
            }
        }
    }

    None
}
