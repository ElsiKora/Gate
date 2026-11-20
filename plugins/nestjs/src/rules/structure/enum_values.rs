//! `nestjs/enum-values`: every string value of an application enum matches the value pattern.
//!
//! Every member of an enum declared in a `.ts` file (not `.d.ts`) below the source root, outside
//! skipped folders, whose initializer is a string literal or a template literal without
//! substitutions, through type assertions and parentheses, has a value that `value-pattern`
//! matches. A numeric, computed or implicit member is not checked. A finding's target is
//! `<Enum>.<member>`, so one exception keeps one member's value, whatever file declares the enum.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Rule, Run};
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{TSEnumMember, TSEnumMemberName};
use gate_typescript::oxc_span::GetSpan;
use gate_typescript::value::{string_like, unwrap};

use crate::layout::PluginOptions;
use crate::shared::{pattern, project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct EnumValues;

/// The options of `nestjs/enum-values`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The regular expression every string value must match; anchor it to match the whole value.
    pub value_pattern: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            value_pattern: "^[A-Z][A-Z0-9]*(_[A-Z0-9]+)*$".to_owned(),
        }
    }
}

impl Rule<crate::Nest> for EnumValues {
    type Options = Options;

    const NAME: &'static str = "enum-values";
    const TITLE: &'static str = "String enum values follow the value pattern";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let pattern = run.options().value_pattern.clone();
            let regex = pattern::compile("value-pattern", &pattern)?;
            let source = shared.source();
            let typescript = shared.typescript()?;
            let mut findings = Vec::new();

            for path in tree::typescript_files(source, &layout.source_root, typescript.settings()) {
                if path.ends_with(".d.ts")
                    || std::path::Path::new(path)
                        .extension()
                        .is_none_or(|extension| extension != "ts")
                {
                    continue;
                }

                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };

                for node in file.semantic().nodes().iter() {
                    let AstKind::TSEnumDeclaration(declaration) = node.kind() else {
                        continue;
                    };

                    for member in &declaration.body.members {
                        let Some(value) = member
                            .initializer
                            .as_ref()
                            .and_then(|initializer| string_like(unwrap(initializer)))
                        else {
                            continue;
                        };

                        if !regex.is_match(value.as_str()) {
                            let target = format!(
                                "{}.{}",
                                declaration.id.name,
                                member_name(file.source(), member)
                            );
                            let line =
                                usize::try_from(file.line(member.span.start)).unwrap_or(usize::MAX);
                            findings.push((path, line, target, value));
                        }
                    }
                }
            }

            // The default pattern is named by the form it spells.
            let form = if pattern == Options::default().value_pattern {
                "is not UPPER_SNAKE".to_owned()
            } else {
                format!("does not match {pattern}")
            };

            for (path, line, target, value) in findings {
                run.report()
                .finding(
                    "",
                    Location::line(path, line),
                    format!(
                        "{path} {target} has the value {value:?}, which {form}; rename it, or keep \
                         a value an external party's specification fixes as a policy exception \
                         with its decision"
                    ),
                )
                .key(target.as_str())
                .target(target)
                .help("rename the value to match the pattern");
            }

            Ok(())
        })
    }
}

/// The member name as the TypeScript compiler prints it; a computed name keeps its brackets.
fn member_name(source: &str, member: &TSEnumMember<'_>) -> String {
    match &member.id {
        TSEnumMemberName::Identifier(identifier) => identifier.name.to_string(),
        TSEnumMemberName::String(literal) => literal.value.to_string(),
        TSEnumMemberName::ComputedString(_) | TSEnumMemberName::ComputedTemplateString(_) => {
            // The member starts at its `[`; the name ends at the first `]` after the key.
            let start = member.span.start as usize;
            let key_end = member.id.span().end as usize;
            let end = source[key_end..]
                .find(']')
                .map_or(key_end, |offset| key_end + offset + 1);
            source[start..end].to_owned()
        }
    }
}
