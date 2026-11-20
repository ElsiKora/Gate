//! `frontend/import-alias`: the one alias `@/` is declared where the code is checked and bundled:
//! `"@/*": ["./src/*"]` in the tsconfig `paths` and no other alias, and, when the project has a Vite
//! configuration, an `@` alias to `src/` in it or the `vite-tsconfig-paths` plugin. A library
//! imports by relative paths and is not read.

use gate_sdk::serde_json::Value;
use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{Expression, ImportOrExportKind, ObjectExpression};
use gate_typescript::oxc_span::{GetSpan, Span};

use crate::code::Project;
use crate::config::{
    names_src, property, property_text, strings_within, tsconfig_paths, vite_config,
};
use crate::options::PACK;
use crate::rules::{Found, report};

/// The rule.
#[derive(Debug, Default)]
pub struct ImportAlias;

/// The alias of `src/` as the tsconfig `paths` declare it.
const PATHS_KEY: &str = "@/*";

/// The plugin that makes Vite read the tsconfig `paths`.
const TSCONFIG_PATHS_PLUGIN: &str = "vite-tsconfig-paths";

impl Rule<TypeScriptCall> for ImportAlias {
    type Options = NoOptions;

    const NAME: &'static str = "import-alias";
    const TITLE: &'static str = "The alias @/ for src/ is declared in the tsconfig and the Vite configuration, and no other";
    const ASPECTS: &'static [&'static str] = &["tsconfig", "vite"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            if project.pack.library() {
                return Ok(());
            }
            let mut found = tsconfig(&project)?;
            found.extend(vite(&project)?);
            report(run, found);
            Ok(())
        })
    }
}

/// The tsconfig `paths`, wherever the tsconfig chain declares them: `@/*` to `./src/*`, and nothing
/// else.
fn tsconfig(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let mut found = Vec::new();
    let declared = tsconfig_paths(project)?;
    let root = project.tsconfig.as_str();

    if declared.is_empty() {
        found.push(
            Found::at(
                "tsconfig",
                root,
                format!(
                    "{root} declares no compilerOptions.paths, through its extends or references, so @/ resolves nowhere"
                ),
                format!("declare \"{PATHS_KEY}\": [\"./src/*\"] in compilerOptions.paths of {root}"),
            )
            .named(PATHS_KEY),
        );
        return Ok(found);
    }

    for config in &declared {
        let file = config.file.as_str();
        let help = format!(
            "declare \"{PATHS_KEY}\": [\"./src/*\"] in compilerOptions.paths of {file}, and no other alias"
        );
        for (key, value) in &config.paths {
            if key != PATHS_KEY {
                found.push(
                    Found::at(
                        "tsconfig",
                        file,
                        format!("{file} declares the alias {key}; the one alias is {PATHS_KEY}"),
                        format!("import through @/ and remove {key} from compilerOptions.paths"),
                    )
                    .named(key),
                );
            } else if !to_src(value) {
                found.push(
                    Found::at(
                        "tsconfig",
                        file,
                        format!("{file} maps {PATHS_KEY} to {value}, not to ./src/*"),
                        help.clone(),
                    )
                    .named(PATHS_KEY),
                );
            }
        }
        if !config.paths.contains_key(PATHS_KEY) {
            found.push(
                Found::at(
                    "tsconfig",
                    file,
                    format!("{file} declares no {PATHS_KEY} in compilerOptions.paths"),
                    help,
                )
                .named(PATHS_KEY),
            );
        }
    }

    Ok(found)
}

/// Whether a `paths` value is the one target `./src/*`.
fn to_src(value: &Value) -> bool {
    value.as_array().is_some_and(|targets| {
        targets.len() == 1
            && targets[0]
                .as_str()
                .is_some_and(|target| matches!(target, "./src/*" | "src/*"))
    })
}

/// The Vite configuration, when there is one: an `@` alias to `src/`, or the plugin that reads the
/// tsconfig `paths`. An alias whose value is not written out (a variable) gives no finding.
fn vite(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let Some(path) = vite_config(project) else {
        return Ok(Vec::new());
    };
    let Some(file) = project.parse(path)? else {
        return Ok(Vec::new());
    };
    if alias(&file) != Alias::Missing || reads_tsconfig(&file) {
        return Ok(Vec::new());
    }

    Ok(vec![
        Found::at(
            "vite",
            path,
            format!("{path} has no @ alias to src/, so the bundler does not resolve @/"),
            format!(
                "add resolve.alias {{ \"@\": <the path of src> }}, or the {TSCONFIG_PATHS_PLUGIN} plugin"
            ),
        )
        .named("@"),
    ])
}

/// What a Vite configuration's `alias` says of `@`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Alias {
    /// No `@` alias to `src/`.
    Missing,
    /// An alias the rule cannot read without running the configuration: a variable, a spread.
    Unread,
    /// An `@` alias to `src/`.
    Src,
}

/// What `file` declares for `@` in its `alias` properties: the object form `{ "@": ... }` or the
/// `{ find, replacement }` form, `find` being `"@"` or the regular expression `/^@\//`.
fn alias(file: &SourceFile<'_>) -> Alias {
    file.semantic()
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::ObjectProperty(alias)
                if alias.key.static_name().as_deref() == Some("alias") =>
            {
                Some(match &alias.value {
                    Expression::ObjectExpression(object) => property(object, "@")
                        .map_or(Alias::Missing, |at| target(file, at.value.span())),
                    Expression::ArrayExpression(array) => array
                        .elements
                        .iter()
                        .map(|element| match element.as_expression() {
                            Some(Expression::ObjectExpression(object)) => entry(file, object),
                            _ => Alias::Unread,
                        })
                        .max()
                        .unwrap_or(Alias::Missing),
                    _ => Alias::Unread,
                })
            }
            _ => None,
        })
        .max()
        .unwrap_or(Alias::Missing)
}

/// One `{ find, replacement }` entry of an alias array.
fn entry(file: &SourceFile<'_>, object: &ObjectExpression<'_>) -> Alias {
    let finds_alias = property_text(object, "find").as_deref() == Some("@")
        || property(object, "find").is_some_and(|find| match &find.value {
            Expression::RegExpLiteral(pattern) => {
                matches!(pattern.regex.pattern.text.as_str(), "^@\\/" | "^@/" | "^@")
            }
            _ => false,
        });
    if !finds_alias {
        return Alias::Missing;
    }
    property(object, "replacement").map_or(Alias::Missing, |replacement| {
        target(file, replacement.value.span())
    })
}

/// What the alias value at `span` names: `src/` when one of its strings does, unread when it holds
/// no string.
fn target(file: &SourceFile<'_>, span: Span) -> Alias {
    let strings = strings_within(file, span);
    if strings.iter().any(|text| names_src(text)) {
        Alias::Src
    } else if strings.is_empty() {
        Alias::Unread
    } else {
        Alias::Missing
    }
}

/// Whether `file` calls the plugin that makes Vite read the tsconfig `paths`: a call of a name it
/// imports from the package, by a default, named or namespace import.
fn reads_tsconfig(file: &SourceFile<'_>) -> bool {
    let semantic = file.semantic();
    let nodes = semantic.nodes();
    let mut symbols = Vec::new();
    for node in nodes.iter() {
        if let AstKind::ImportDeclaration(import) = node.kind()
            && import.import_kind != ImportOrExportKind::Type
            && import.source.value == TSCONFIG_PATHS_PLUGIN
        {
            symbols.extend(
                import
                    .specifiers
                    .iter()
                    .flatten()
                    .filter_map(|specifier| specifier.local().symbol_id.get()),
            );
        }
    }

    symbols.into_iter().any(|symbol| {
        semantic
            .scoping()
            .get_resolved_references(symbol)
            .any(|reference| {
                let node = nodes.get_node(reference.node_id());
                matches!(
                    nodes.parent_kind(node.id()),
                    AstKind::CallExpression(call) if call.callee.span() == node.span()
                )
            })
    })
}
