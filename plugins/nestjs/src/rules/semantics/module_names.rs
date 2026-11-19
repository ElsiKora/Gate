//! `nestjs/module-names`: a module class names its resource folder, and semantic owners carry their
//! full owner stem.
//!
//! Each tracked `module-file` below one of `layer-scopes` (the first scope whose path starts the
//! file's path) is judged by the first top-level `export class` or `export default class` it
//! declares, in this order: there is a named one (`module-class`); it starts with the scope's
//! class prefix, or for an empty scope prefix with the prefix of a matching
//! `resource-class-prefixes` entry (`module-owner-prefix`); it ends with `module-suffix`
//! (`module-class`); for a `semantic-owners` folder it is the owner stem with the suffix
//! (`semantic-owner-class`), and otherwise its stem without the scope prefix, in kebab case with
//! `token-replacements` applied, spells the folder path below the scope with `/` as `-`
//! (`resource-folder`).
//!
//! Each `semantic-class-files` file of a semantic owner exports the owner stem with the file's
//! suffix (`semantic-owner-class`), and each `stale-module-paths` entry that is a tracked file or a
//! folder with tracked files is reported (`shortened-resource-folder`). A file without a syntax
//! tree is skipped: `typescript/parse` reports it.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Run};
use gate_typescript::oxc_ast::ast::{Declaration, ExportDefaultDeclarationKind, Statement};
use gate_typescript::{SourceFile, TypeScript};

use crate::shared::project;

/// The rule.
#[derive(Debug, Default)]
pub struct Rule;

/// A folder whose module files name their resource, and the class prefix they carry.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct LayerScope {
    /// The folder, with a trailing `/`; a module file whose path starts with it is in the scope.
    pub path: String,
    /// The class prefix every module class of the scope starts with; empty for none.
    pub class_prefix: String,
}

/// A class prefix required of a resource in a scope without a class prefix.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct ResourceClassPrefix {
    /// The resource folder below the scope; it matches itself and the folders below it.
    pub resource: String,
    /// The class prefix its module classes start with.
    pub class_prefix: String,
}

/// A class file of a semantic owner and the class suffix it exports.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct SemanticClassFile {
    /// The file, relative to the owner's resource folder.
    pub file: String,
    /// The class suffix after the owner stem.
    pub suffix: String,
}

/// A resource folder whose classes carry `owner-stem` instead of the stem the folder path gives.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct SemanticOwner {
    /// The resource folder, relative to the project root.
    pub resource_path: String,
    /// The class stem of the owner's module and class files.
    pub owner_stem: String,
}

/// The options of `nestjs/module-names`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The scopes whose module files are judged; the first matching scope wins.
    pub layer_scopes: Vec<LayerScope>,
    /// Class prefixes of resources in scopes without a class prefix; the first match wins.
    pub resource_class_prefixes: Vec<ResourceClassPrefix>,
    /// The name of a module file.
    pub module_file: String,
    /// The suffix of a module class.
    pub module_suffix: String,
    /// The class files of a semantic owner besides its module file.
    pub semantic_class_files: Vec<SemanticClassFile>,
    /// The semantic owners.
    pub semantic_owners: Vec<SemanticOwner>,
    /// Shortened module paths that must not exist, as a file or as a folder with files.
    pub stale_module_paths: Vec<String>,
    /// Replacements `[from, to]` of whole dash-separated tokens in the expected folder name,
    /// applied left to right.
    pub token_replacements: Vec<(String, String)>,
}

impl Default for Options {
    fn default() -> Self {
        let scope = |path: &str, class_prefix: &str| LayerScope {
            path: path.to_owned(),
            class_prefix: class_prefix.to_owned(),
        };
        let class_file = |file: &str, suffix: &str| SemanticClassFile {
            file: file.to_owned(),
            suffix: suffix.to_owned(),
        };

        Self {
            layer_scopes: vec![
                scope("src/modules/domain/", ""),
                scope("src/modules/internal/", "Internal"),
                scope("src/modules/integration/", "Integration"),
                scope("src/modules/public/", "Public"),
                scope("src/infrastructure/", "Infrastructure"),
            ],
            resource_class_prefixes: Vec::new(),
            module_file: "module.ts".to_owned(),
            module_suffix: "Module".to_owned(),
            semantic_class_files: vec![
                class_file("controller.ts", "Controller"),
                class_file("entity/entity.ts", "Entity"),
                class_file("service.ts", "Service"),
            ],
            semantic_owners: Vec::new(),
            stale_module_paths: Vec::new(),
            token_replacements: Vec::new(),
        }
    }
}

/// One finding before it is reported: aspect, path, key, message and help.
type Pending = (&'static str, String, Option<String>, String, String);

impl gate_sdk::Rule<crate::Nest> for Rule {
    type Options = Options;

    const NAME: &'static str = "module-names";
    const TITLE: &'static str =
        "Module classes name their resource folder and semantic owners keep their full stem";
    const ASPECTS: &'static [&'static str] = &[
        "module-class",
        "module-owner-prefix",
        "resource-folder",
        "semantic-owner-class",
        "shortened-resource-folder",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            {
                use crate::shared::paths::Form::Folder;
                let o = run.options();
                for (index, owner) in o.semantic_owners.iter().enumerate() {
                    crate::shared::paths::check(
                        &format!("semantic-owners[{index}].resource-path"),
                        &owner.resource_path,
                        Folder,
                    )?;
                }
                super::folders::check_all_below_root("stale-module-paths", &o.stale_module_paths)?;
            }

            let options = run.options();
            let tree = run.tree();
            let source = shared.source();
            let typescript = shared.typescript()?;
            let mut pending: Vec<Pending> = Vec::new();

            for path in crate::shared::tree::files(source, "", &crate::shared::tree::UNSKIPPED) {
                if path.rsplit('/').next() != Some(options.module_file.as_str()) {
                    continue;
                }

                let Some(scope) = options
                    .layer_scopes
                    .iter()
                    .find(|scope| path.starts_with(scope.path.as_str()))
                else {
                    continue;
                };
                let Some(Parsed { class: name }) = class_name(typescript, path)? else {
                    continue;
                };

                if let Some((aspect, key, message, help)) =
                    module_finding(path, scope, name.as_deref(), options)
                {
                    pending.push((aspect, path.to_owned(), Some(key), message, help));
                }
            }

            for owner in &options.semantic_owners {
                for class_file in &options.semantic_class_files {
                    let path = format!("{}/{}", owner.resource_path, class_file.file);

                    if !tree.contains(&path) {
                        continue;
                    }

                    let Some(Parsed { class: name }) = class_name(typescript, &path)? else {
                        continue;
                    };
                    let expected = format!("{}{}", owner.owner_stem, class_file.suffix);
                    let message = match name {
                        Some(name) if name == expected => continue,
                        Some(name) => {
                            format!("{path} exports {name}; semantic owner requires {expected}")
                        }
                        None => format!("{path} does not export {expected}"),
                    };

                    pending.push((
                        "semantic-owner-class",
                        path,
                        Some(expected.clone()),
                        message,
                        format!("export the class {expected} from the file"),
                    ));
                }
            }

            for path in &options.stale_module_paths {
                if tree.contains(path) || !tree.under(path).is_empty() {
                    pending.push((
                    "shortened-resource-folder",
                    path.clone(),
                    None,
                    format!(
                        "{path} is a shortened resource folder; use the full entity resource name"
                    ),
                    "rename the folder to the full name of the entity resource".to_owned(),
                ));
                }
            }

            for (aspect, path, key, message, help) in pending {
                let reported = run
                    .report()
                    .finding(aspect, Location::file(path), message)
                    .help(help);

                if let Some(key) = key {
                    reported.key(key);
                }
            }

            Ok(())
        })
    }
}

/// A file with a syntax tree, and the name of the class it exports first, if any.
struct Parsed {
    class: Option<String>,
}

/// The class `path` exports: `None` when the file has no syntax tree.
///
/// # Errors
/// When the file is tracked but cannot be read.
fn class_name(typescript: &TypeScript<'_>, path: &str) -> gate_sdk::Result<Option<Parsed>> {
    Ok(project::parsed(typescript, path)?.map(|file| Parsed {
        class: exported_class(&file),
    }))
}

/// The finding of one module file in `scope`: aspect, key, message and help.
fn module_finding(
    path: &str,
    scope: &LayerScope,
    name: Option<&str>,
    options: &Options,
) -> Option<(&'static str, String, String, String)> {
    let module_suffix = options.module_suffix.as_str();
    let Some(name) = name else {
        return Some((
            "module-class",
            "#missing".to_owned(),
            format!("{path} does not export a module class"),
            format!(
                "export the module class, named after the resource folder and ending with {module_suffix}"
            ),
        ));
    };
    let class_prefix = scope.class_prefix.as_str();
    let resource = strip_module_file(&path[scope.path.len()..], &options.module_file);
    let required = if class_prefix.is_empty() {
        options
            .resource_class_prefixes
            .iter()
            .find(|entry| {
                resource == entry.resource
                    || resource
                        .strip_prefix(entry.resource.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map_or("", |entry| entry.class_prefix.as_str())
    } else {
        class_prefix
    };

    if !name.starts_with(required) {
        return Some((
            "module-owner-prefix",
            name.to_owned(),
            format!("{path} exports {name}; modules in this owner must use the {required} prefix"),
            format!("start the class name with {required}"),
        ));
    }

    let Some(stem) = name.strip_suffix(module_suffix) else {
        return Some((
            "module-class",
            name.to_owned(),
            format!("{path} exports {name}; module classes must end with {module_suffix}"),
            format!("end the class name with {module_suffix}"),
        ));
    };
    let resource_path = strip_module_file(path, &options.module_file);

    if let Some(owner) = options
        .semantic_owners
        .iter()
        .find(|owner| owner.resource_path == resource_path)
    {
        let expected = format!("{}{module_suffix}", owner.owner_stem);

        return (name != expected).then(|| {
            (
                "semantic-owner-class",
                expected.clone(),
                format!("{path} exports {name}; semantic owner requires {expected}"),
                format!("rename the class to {expected}"),
            )
        });
    }

    let expected = kebab(
        stem.strip_prefix(class_prefix).unwrap_or(stem),
        &options.token_replacements,
    );

    (resource.replace('/', "-") != expected).then(|| {
        (
            "resource-folder",
            name.to_owned(),
            format!("{path} is under {resource}; {name} should live under {expected}"),
            format!(
                "move the module to the folder {expected}, or rename the class after its folder"
            ),
        )
    })
}

/// The path without a trailing module file and the slash before it.
fn strip_module_file<'p>(path: &'p str, module_file: &str) -> &'p str {
    path.strip_suffix(module_file)
        .map_or(path, |rest| rest.strip_suffix('/').unwrap_or(rest))
}

/// The name of the class of the first top-level `export class` or `export default class`
/// statement; `Some(None)` when there is none or it has no name.
fn exported_class(file: &SourceFile<'_>) -> Option<String> {
    file.program()
        .body
        .iter()
        .find_map(|statement| match statement {
            Statement::ExportDeclaration(export) => match &export.declaration {
                Declaration::ClassDeclaration(class) => {
                    Some(class.id.as_ref().map(|id| id.name.to_string()))
                }
                _ => None,
            },
            Statement::ExportDefaultDeclaration(export) => match &export.declaration {
                ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                    Some(class.id.as_ref().map(|id| id.name.to_string()))
                }
                _ => None,
            },
            _ => None,
        })?
}

/// A `PascalCase` stem in kebab case: a dash between a lowercase ASCII letter or digit and an
/// uppercase ASCII letter, and before the last capital of a run of two or more capitals followed by
/// a lowercase letter; all lowercase; then every `[from, to]` replacement applied, left to right,
/// to whole dash-separated tokens.
fn kebab(stem: &str, replacements: &[(String, String)]) -> String {
    let mut result = split_acronyms(&split_words(stem)).to_lowercase();

    for (from, to) in replacements {
        result = replace_tokens(&result, from, to);
    }

    result
}

/// A dash between every lowercase ASCII letter or digit and the uppercase ASCII letter after it.
fn split_words(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut previous: Option<char> = None;

    for character in text.chars() {
        if character.is_ascii_uppercase()
            && previous.is_some_and(|before| before.is_ascii_lowercase() || before.is_ascii_digit())
        {
            result.push('-');
        }

        result.push(character);
        previous = Some(character);
    }

    result
}

/// A dash before the last capital of every run of two or more uppercase ASCII letters that a
/// lowercase ASCII letter follows (`HTTPServer` → `HTTP-Server`).
fn split_acronyms(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut index = 0;

    while index < characters.len() {
        if !characters[index].is_ascii_uppercase() {
            result.push(characters[index]);
            index += 1;
            continue;
        }

        let mut end = index;

        while end < characters.len() && characters[end].is_ascii_uppercase() {
            end += 1;
        }

        if end - index >= 2 && characters.get(end).is_some_and(char::is_ascii_lowercase) {
            result.extend(&characters[index..end - 1]);
            result.push('-');
            result.push(characters[end - 1]);
            result.push(characters[end]);
            index = end + 1;
        } else {
            result.extend(&characters[index..end]);
            index = end;
        }
    }

    result
}

/// Replaces every occurrence of `from` that starts the text or follows a dash and ends the text or
/// precedes a dash, left to right without overlap.
fn replace_tokens(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_owned();
    }

    let bytes = text.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut index = 0;

    while let Some(character) = text[index..].chars().next() {
        let starts_token = index == 0 || bytes[index - 1] == b'-';
        let end = index + from.len();

        if starts_token
            && text[index..].starts_with(from)
            && (end == text.len() || bytes[end] == b'-')
        {
            result.push_str(to);
            index = end;
        } else {
            result.push(character);
            index += character.len_utf8();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::{kebab, replace_tokens};

    #[test]
    fn stems_become_kebab_case() {
        assert_eq!(kebab("BookLoanHistory", &[]), "book-loan-history");
        assert_eq!(kebab("HTTPServer", &[]), "http-server");
        assert_eq!(kebab("Oauth2Client", &[]), "oauth2-client");
        assert_eq!(kebab("S3Adapter", &[]), "s3-adapter");
        assert_eq!(kebab("ABC", &[]), "abc");
        assert_eq!(kebab("aBCDe", &[]), "a-bc-de");
        assert_eq!(kebab("XMLHTTPRequestV2", &[]), "xmlhttp-request-v2");
        assert_eq!(
            kebab(
                "ReadEaseSpreadEase",
                &[("read-ease".to_owned(), "readease".to_owned())]
            ),
            "readease-spread-ease"
        );
    }

    #[test]
    fn token_replacement_needs_whole_tokens() {
        assert_eq!(replace_tokens("a-a-a", "a", "b"), "b-b-b");
        assert_eq!(replace_tokens("aa-a", "a", "b"), "aa-b");
        assert_eq!(replace_tokens("x-read-ease-y", "read-ease", "z"), "x-z-y");
        assert_eq!(replace_tokens("read-easy", "read-ease", "z"), "read-easy");
        assert_eq!(replace_tokens("a-b", "", "z"), "a-b");
    }
}
