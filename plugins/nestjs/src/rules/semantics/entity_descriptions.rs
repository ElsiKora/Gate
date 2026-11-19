//! `nestjs/entity-descriptions`: the API descriptions of entity properties are static, lowercase
//! and do not repeat their owner's name.
//!
//! In every `.ts` and `.tsx` code file (the project's root files and the files below `roots`,
//! outside `[languages.typescript] skip-folders`) that names the framework package, each class at any
//! depth is read: every decorator call of its members that resolves to the package's describe
//! decorator, with an object literal first argument, has its first non-computed `description` init
//! property read as a static string, directly or through constants across files. A description that
//! is no static string is reported (`static`); one that starts with a trailing run of the owner's
//! name tokens (`owner-prefix`) or with an uppercase token that is not allowed
//! (`uppercase-token`) is reported with the target `<Class>.<member>: <description>`.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{
    ChainElement, Class, ClassElement, Decorator, Expression, MethodDefinitionKind,
    ObjectExpression, ObjectPropertyKind, PropertyKey, PropertyKind,
};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::value::{StaticValue, ValueReader, number_text, string_like};

use super::code_files::{Universe, js_trim_start};
use crate::layout::PluginOptions;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// The options of `nestjs/entity-descriptions`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// Path prefixes (with a final `/`) whose files are read, besides the project's root files.
    pub roots: Vec<String>,
    /// The extensions of the files read.
    pub extensions: Vec<String>,
    /// The package of the describe decorator. Absent means `layout.automator-package`.
    pub framework_package: Option<String>,
    /// The decorator whose options carry the description.
    pub describe_decorator: String,
    /// The option that holds the description.
    pub description_key: String,
    /// The suffix of an owner's class name that is not one of its name tokens.
    pub owner_suffix: String,
    /// Uppercase tokens a description may start with.
    pub allowed_initial_uppercase: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        let strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();

        Self {
            roots: strings(&["infrastructure/", "scripts/", "src/", "test/"]),
            extensions: strings(&["ts", "tsx"]),
            framework_package: None,
            describe_decorator: "ApiPropertyDescribe".to_owned(),
            description_key: "description".to_owned(),
            owner_suffix: "Entity".to_owned(),
            allowed_initial_uppercase: Vec::new(),
        }
    }
}

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "entity-descriptions";
    const TITLE: &'static str = "Entity API descriptions are lowercase owner suffixes";
    const ASPECTS: &'static [&'static str] = &["owner-prefix", "static", "uppercase-token"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            {
                use crate::shared::paths::Form::Prefix;
                let o = run.options();
                crate::shared::paths::check_all("roots", &o.roots, Prefix)?;
            }

            let layout = PluginOptions::read(run)?.layout;
            let options = run.options();
            let package = super::package::resolve(
                "framework-package",
                options.framework_package.as_deref(),
                &layout.automator_package,
            )?;
            let typescript = shared.typescript()?;
            let reader = ValueReader::new(typescript);
            let universe = Universe {
                roots: &options.roots,
                extensions: &options.extensions,
            };
            let files = universe.parsed(typescript)?;
            let mut findings = Vec::new();

            for (path, file) in &files {
                let Some(bindings) = ImportBindings::new(file, &package, false) else {
                    continue;
                };
                let descriptions = Descriptions {
                    path,
                    file,
                    bindings: &bindings,
                    reader: &reader,
                    options,
                };

                for node in file.semantic().nodes().iter() {
                    if let AstKind::Class(class) = node.kind() {
                        descriptions.class(class, &mut findings)?;
                    }
                }
            }

            for finding in findings {
                let reported = run.report().finding(
                    finding.aspect,
                    Location::line(
                        finding.path,
                        usize::try_from(finding.line).unwrap_or(usize::MAX),
                    ),
                    finding.message,
                );

                reported.help(finding.help).key(finding.key);

                if let Some(target) = finding.target {
                    reported.target(target);
                }
            }

            Ok(())
        })
    }
}

/// A finding before it is reported.
struct Finding {
    aspect: &'static str,
    path: String,
    line: u32,
    message: String,
    help: &'static str,
    target: Option<String>,
    key: String,
}

/// The description checks of one file.
struct Descriptions<'d, 'a> {
    path: &'d str,
    file: &'d SourceFile<'a>,
    bindings: &'d ImportBindings<'d, 'a>,
    reader: &'d ValueReader<'d, 'a>,
    options: &'d Options,
}

impl<'a> Descriptions<'_, 'a> {
    fn class(&self, class: &Class<'a>, findings: &mut Vec<Finding>) -> gate_typescript::Result<()> {
        let Some(name) = class.id.as_ref().map(|id| id.name.as_str()) else {
            return Ok(());
        };

        for element in &class.body.body {
            let (key, computed, decorators) = match element {
                ClassElement::PropertyDefinition(property) => {
                    (&property.key, property.computed, &property.decorators)
                }
                ClassElement::AccessorProperty(property) => {
                    (&property.key, property.computed, &property.decorators)
                }
                ClassElement::MethodDefinition(method)
                    if method.kind != MethodDefinitionKind::Constructor =>
                {
                    (&method.key, method.computed, &method.decorators)
                }
                _ => continue,
            };
            let property = self.property_name(key, computed);

            // A member whose name text is empty, such as `"": string`, is skipped.
            if property.is_empty() {
                continue;
            }

            for decorator in decorators {
                self.decorator(name, &property, decorator, findings)?;
            }
        }

        Ok(())
    }

    /// The member name as the TypeScript compiler prints it.
    fn property_name(&self, key: &PropertyKey<'a>, computed: bool) -> String {
        let source = self.file.source();
        let span = key.span();

        if computed {
            let start = source[..span.start as usize]
                .rfind('[')
                .unwrap_or(span.start as usize);
            let end = source[span.end as usize..]
                .find(']')
                .map_or(span.end as usize, |offset| span.end as usize + offset + 1);
            return source[start..end].to_owned();
        }

        match key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name.to_string(),
            PropertyKey::PrivateIdentifier(identifier) => format!("#{}", identifier.name),
            PropertyKey::StringLiteral(literal) => literal.value.to_string(),
            // The compiler's text of a numeric name is its value, as `0x10` prints `16`.
            PropertyKey::NumericLiteral(literal) => number_text(literal.value),
            _ => source[span.start as usize..span.end as usize].to_owned(),
        }
    }

    fn decorator(
        &self,
        class: &str,
        property: &str,
        decorator: &Decorator<'a>,
        findings: &mut Vec<Finding>,
    ) -> gate_typescript::Result<()> {
        // TypeScript keeps `@Crud?.ApiPropertyDescribe(...)` a call.
        let call = match &decorator.expression {
            Expression::CallExpression(call) => call,
            Expression::ChainExpression(chain) => match &chain.expression {
                ChainElement::CallExpression(call) => call,
                _ => return Ok(()),
            },
            _ => return Ok(()),
        };

        if self
            .bindings
            .resolve(&call.callee)
            .is_none_or(|path| path.len() != 1 || path[0] != self.options.describe_decorator)
        {
            return Ok(());
        }

        let Some(Expression::ObjectExpression(options)) = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
        else {
            return Ok(());
        };
        let Some((start, initializer)) = description(options, &self.options.description_key) else {
            return Ok(());
        };
        let line = self.file.line(start);
        let label = format!("{class}.{property}");
        let value = match string_like(initializer) {
            Some(text) => Some(StaticValue::String(text)),
            None => self.reader.read(self.file, initializer)?,
        };
        let finding = |aspect, message, help, target| Finding {
            aspect,
            path: self.path.to_owned(),
            line,
            message,
            help,
            target,
            key: label.clone(),
        };
        let Some(StaticValue::String(text)) = value else {
            findings.push(finding(
                "static",
                format!("{label} description must resolve to a static string"),
                "write the description as a string literal or a string constant",
                None,
            ));
            return Ok(());
        };
        let target = format!("{label}: {text}");

        if starts_with_owner_suffix(&text, class, &self.options.owner_suffix) {
            findings.push(finding(
                "owner-prefix",
                format!("{label} description repeats its owner/resource prefix: {text:?}"),
                "describe the property without repeating its owner's name",
                Some(target.clone()),
            ));
        }

        if let Some(token) = initial_token(&text)
            && token.starts_with(|character: char| character.is_ascii_uppercase())
            && !self
                .options
                .allowed_initial_uppercase
                .iter()
                .any(|allowed| allowed == token)
        {
            findings.push(finding(
                "uppercase-token",
                format!("{label} description starts with unjustified uppercase token {token:?}"),
                "start the description in lowercase, or allow the token",
                Some(target),
            ));
        }

        Ok(())
    }
}

/// The start of the first property assignment of `key` in the options, and its value.
fn description<'o, 'a>(
    options: &'o ObjectExpression<'a>,
    key: &str,
) -> Option<(u32, &'o Expression<'a>)> {
    options.properties.iter().find_map(|property| {
        let ObjectPropertyKind::ObjectProperty(property) = property else {
            return None;
        };

        if property.computed
            || property.shorthand
            || property.method
            || property.kind != PropertyKind::Init
        {
            return None;
        }

        let name = match &property.key {
            PropertyKey::StaticIdentifier(identifier) => identifier.name.to_string(),
            PropertyKey::StringLiteral(literal) => literal.value.to_string(),
            PropertyKey::NumericLiteral(literal) => number_text(literal.value),
            _ => return None,
        };

        (name == key).then_some((property.span.start, &property.value))
    })
}

/// Whether the description starts with a trailing run of the owner's name tokens; a terminal
/// `suffix` of the owner is ignored.
fn starts_with_owner_suffix(description: &str, class: &str, suffix: &str) -> bool {
    let owner = tokens(if suffix.is_empty() {
        class
    } else {
        class.strip_suffix(suffix).unwrap_or(class)
    });
    let words = tokens(description);

    (0..owner.len()).any(|start| {
        owner[start..]
            .iter()
            .enumerate()
            .all(|(index, token)| words.get(index) == Some(token))
    })
}

/// The lowercase word tokens of a name, as
/// `/[A-Z]+(?=[A-Z][a-z]|$)|[A-Z]?[a-z]+|[0-9]+/g` splits it.
fn tokens(value: &str) -> Vec<String> {
    let characters: Vec<char> = value.chars().collect();
    let length = characters.len();
    let upper = |index: usize| characters.get(index).is_some_and(char::is_ascii_uppercase);
    let lower = |index: usize| characters.get(index).is_some_and(char::is_ascii_lowercase);
    let digit = |index: usize| characters.get(index).is_some_and(char::is_ascii_digit);
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < length {
        let run = (index..length)
            .take_while(|position| upper(*position))
            .count();
        let acronym = (1..=run)
            .rev()
            .map(|size| index + size)
            .find(|&end| end == length || (upper(end) && lower(end + 1)));

        let end = if let Some(end) = acronym {
            end
        } else if upper(index) && lower(index + 1) {
            (index + 1..length)
                .find(|position| !lower(*position))
                .unwrap_or(length)
        } else if lower(index) {
            (index..length)
                .find(|position| !lower(*position))
                .unwrap_or(length)
        } else if digit(index) {
            (index..length)
                .find(|position| !digit(*position))
                .unwrap_or(length)
        } else {
            index += 1;
            continue;
        };

        tokens.push(
            characters[index..end]
                .iter()
                .collect::<String>()
                .to_lowercase(),
        );
        index = end;
    }

    tokens
}

/// The leading word of a description, with hyphenated parts: `SHA-256` in `"SHA-256 digest"`.
fn initial_token(description: &str) -> Option<&str> {
    let trimmed = js_trim_start(description);
    let bytes = trimmed.as_bytes();

    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }

    let mut end = bytes
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(bytes.len());

    while bytes.get(end) == Some(&b'-') && bytes.get(end + 1).is_some_and(u8::is_ascii_alphanumeric)
    {
        end += 1;
        end += bytes[end..]
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric())
            .unwrap_or(bytes.len() - end);
    }

    Some(&trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::{initial_token, starts_with_owner_suffix, tokens};

    #[test]
    fn names_split_into_the_tokens_of_the_javascript_pattern() {
        assert_eq!(tokens("LibraryShelfEntity"), ["library", "shelf", "entity"]);
        assert_eq!(tokens("HTTPServer2Error"), ["http", "server", "2", "error"]);
        assert_eq!(tokens("TLS policy"), ["policy"]);
        assert_eq!(tokens("ab_CD9"), ["ab", "9"]);
        assert_eq!(tokens("SHA-256 digest"), ["256", "digest"]);
        assert_eq!(tokens("APIKeys"), ["api", "keys"]);
        assert_eq!(tokens("IPv4"), ["i", "pv", "4"]);
    }

    #[test]
    fn a_description_may_not_start_with_a_trailing_run_of_its_owner_tokens() {
        let owner = "LibraryShelfEntity";

        assert!(starts_with_owner_suffix(
            "library shelf state",
            owner,
            "Entity"
        ));
        assert!(starts_with_owner_suffix("shelf priority", owner, "Entity"));
        assert!(!starts_with_owner_suffix("state", owner, "Entity"));
        assert!(!starts_with_owner_suffix("library state", owner, "Entity"));
        assert!(starts_with_owner_suffix("entity id", owner, ""));
    }

    #[test]
    fn the_initial_token_keeps_hyphenated_parts() {
        assert_eq!(initial_token("  SHA-256 digest"), Some("SHA-256"));
        assert_eq!(initial_token("Status"), Some("Status"));
        assert_eq!(initial_token("TLS-"), Some("TLS"));
        assert_eq!(initial_token("2fa code"), None);
    }
}
