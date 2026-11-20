//! `cdk/tags`: tags are applied only in `lib/naming/`, with the standard's keys and the declared
//! ones, and every standard key is applied there.
//!
//! A key is read as a literal, through a `const` of the same file, or, when it comes from a loop
//! over `Object.entries(map)` or `Object.keys(map)` (a `for … of` or a `forEach`), as the keys of
//! `map`, an object literal of the same file. A key read no other way gives no finding, and while
//! `lib/naming/` applies such a key, `required` reports nothing.

use std::collections::BTreeSet;

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::binding::ImportBindings;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{BindingPattern, Expression, ObjectPropertyKind};
use gate_typescript::syntax::key_name;
use gate_typescript::value::unwrap;

use crate::follow::{constant, declaration, object};
use crate::options::PluginOptions;
use crate::rules::{Found, argument, report, text};
use crate::sources::{Cdk, app_files, parsed};
use crate::standard::TAGS;

/// The rule.
#[derive(Debug, Default)]
pub struct Tags;

impl Rule<Cdk> for Tags {
    type Options = NoOptions;

    const NAME: &'static str = "tags";
    const TITLE: &'static str =
        "Tags are applied in lib/naming/ only, with the standard's keys, each of them applied";
    const ASPECTS: &'static [&'static str] = &["place", "key", "required"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, NoOptions, Cdk>) -> gate_sdk::Result<()> {
        let options = PluginOptions::read(run)?;
        run.with_call(|run, shared| {
            let typescript = shared.typescript()?;
            let mut applied = BTreeSet::new();
            let mut unread = false;

            for path in app_files(typescript) {
                let Some(file) = parsed(typescript, path)? else {
                    continue;
                };
                for (line, keys) in tag_calls(&file) {
                    let naming = path.starts_with("lib/naming/");
                    if !naming {
                        report(
                            run,
                            Found {
                                aspect: "place",
                                path,
                                line: Some(line),
                                key: keys
                                    .as_ref()
                                    .and_then(|keys| keys.first().cloned())
                                    .unwrap_or_else(|| "tag".to_owned()),
                                message: format!(
                                    "{path} applies a tag; tags are applied only in lib/naming/"
                                ),
                                help: "apply the tag through the tag function of lib/naming/",
                            },
                        );
                    }
                    let Some(keys) = keys else {
                        unread |= naming;
                        continue;
                    };
                    for key in keys {
                        judge_key(run, &options, path, line, &key);
                        if naming {
                            applied.insert(key);
                        }
                    }
                }
            }

            if unread {
                return Ok(());
            }
            for tag in TAGS {
                if !applied.contains(tag) {
                    report(
                        run,
                        Found {
                            aspect: "required",
                            path: "lib/naming",
                            line: None,
                            key: tag.to_owned(),
                            message: format!("lib/naming/ never applies the standard tag {tag}"),
                            help: "apply the tag in the tag function of lib/naming/",
                        },
                    );
                }
            }
            Ok(())
        })
    }
}

/// Reports `key`, applied at `line` of `path`, unless it is a standard or a declared key.
fn judge_key(
    run: &mut Run<'_, NoOptions, Cdk>,
    options: &PluginOptions,
    path: &str,
    line: u32,
    key: &str,
) {
    if options.allows_tag(key) {
        return;
    }
    report(
        run,
        Found {
            aspect: "key",
            path,
            line: Some(line),
            message: format!(
                "{path} applies the tag key {key}, which is neither a standard key ({}) nor declared in extra-tags",
                TAGS.join(", ")
            ),
            key: key.to_owned(),
            help: "use the standard key, or declare the key in [plugins.cdk] options extra-tags with its job",
        },
    );
}

/// The tag applications of `file`, `Tags.of(x).add(key, …)` and `manager.setTag(key, …)`, with
/// their line and the keys they apply, `None` when those cannot be read.
fn tag_calls(file: &SourceFile<'_>) -> Vec<(u32, Option<Vec<String>>)> {
    let bindings = ImportBindings::new(file, "aws-cdk-lib", false);
    let mut calls = Vec::new();

    for node in file.semantic().nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
            continue;
        };
        let tagging = match member.property.name.as_str() {
            "setTag" => true,
            "add" => match unwrap(&member.object) {
                Expression::CallExpression(of) => bindings
                    .as_ref()
                    .and_then(|bindings| bindings.resolve(&of.callee))
                    .is_some_and(|path| path == ["Tags", "of"]),
                _ => false,
            },
            _ => false,
        };
        if tagging {
            calls.push((
                file.line(call.span.start),
                argument(&call.arguments, 0).and_then(|key| keys(file, key)),
            ));
        }
    }

    calls
}

/// The keys `key` stands for: its text, or the keys of the map a loop over `Object.entries(map)` or
/// `Object.keys(map)` takes it from.
fn keys<'a>(file: &SourceFile<'a>, key: &Expression<'a>) -> Option<Vec<String>> {
    let value = constant(file, key);
    if let Some(literal) = text(value) {
        return Some(vec![literal]);
    }
    let Expression::Identifier(identifier) = value else {
        return None;
    };
    let nodes = file.semantic().nodes();
    let declared = declaration(file, identifier)?;
    let (pattern, iterated) = match nodes.kind(declared) {
        AstKind::VariableDeclarator(declarator) => {
            let list = nodes.parent_id(declared);
            let AstKind::ForOfStatement(statement) = nodes.parent_kind(list) else {
                return None;
            };
            (&declarator.id, &statement.right)
        }
        AstKind::FormalParameter(parameter) => {
            let function = nodes.parent_id(nodes.parent_id(declared));
            let AstKind::CallExpression(call) = nodes.parent_kind(function) else {
                return None;
            };
            let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
                return None;
            };
            if member.property.name != "forEach" {
                return None;
            }
            (&parameter.pattern, &member.object)
        }
        _ => return None,
    };
    let (method, map) = object_method(iterated)?;
    let names = identifier.name.as_str();
    let bound = match (method, pattern) {
        ("entries", BindingPattern::ArrayPattern(array)) => array
            .elements
            .first()
            .and_then(Option::as_ref)
            .is_some_and(|first| matches!(first, BindingPattern::BindingIdentifier(binding) if binding.name == names)),
        ("keys", BindingPattern::BindingIdentifier(binding)) => binding.name == names,
        _ => false,
    };
    if !bound {
        return None;
    }
    map_keys(file, map, 0)
}

/// `Object.<method>(map)`: the method and the map.
fn object_method<'e, 'a>(expression: &'e Expression<'a>) -> Option<(&'e str, &'e Expression<'a>)> {
    let Expression::CallExpression(call) = unwrap(expression) else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = unwrap(&call.callee) else {
        return None;
    };
    let Expression::Identifier(owner) = unwrap(&member.object) else {
        return None;
    };
    if owner.name != "Object" {
        return None;
    }
    Some((member.property.name.as_str(), argument(&call.arguments, 0)?))
}

/// The keys of the object literal `map`, through its spreads; `None` when one cannot be read.
fn map_keys<'a>(file: &SourceFile<'a>, map: &Expression<'a>, depth: usize) -> Option<Vec<String>> {
    if depth > 4 {
        return None;
    }
    let mut names = Vec::new();
    for item in &object(file, map)?.properties {
        match item {
            ObjectPropertyKind::ObjectProperty(entry) => {
                names.push(key_name(&entry.key, entry.computed)?);
            }
            ObjectPropertyKind::SpreadProperty(spread) => {
                names.extend(map_keys(file, &spread.argument, depth + 1)?);
            }
        }
    }
    Some(names)
}
