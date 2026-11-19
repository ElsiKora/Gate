//! The modules each source file loads, and the paths a specifier names.

use gate_typescript::TypeScript;
use gate_typescript::modules::{ModuleLoads, module_loads};

use super::{project, tree};
use crate::layout::Layout;

/// The modules each tracked `.ts` file below `root` loads, in path order; files in a folder
/// `[languages.typescript] skip-folders` names are left out ([`tree::typescript_files`]), and so are
/// declaration files unless `declarations` is set. A file
/// without a syntax tree is skipped, because `typescript/parse` reports it.
///
/// # Errors
/// When a tracked file cannot be read.
pub fn loads<'t>(
    typescript: &TypeScript<'t>,
    root: &str,
    declarations: bool,
) -> gate_sdk::Result<Vec<(&'t str, ModuleLoads)>> {
    let mut loads = Vec::new();

    for path in tree::typescript_files(typescript.source(), root, typescript.settings()) {
        if !declarations && path.ends_with(".d.ts") {
            continue;
        }

        if let Some(file) = project::parsed(typescript, path)? {
            loads.push((path, module_loads(&file)));
        }
    }

    Ok(loads)
}

/// The project path a relative specifier names from `file`, with `..` segments above the root
/// kept, as Node's `path.relative` spells a path outside it.
#[must_use]
pub fn relative_target(file: &str, specifier: &str) -> String {
    let mut segments: Vec<&str> = file.split('/').collect();
    segments.pop();
    let mut above = 0;

    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    above += 1;
                }
            }
            name => segments.push(name),
        }
    }

    let mut parts = vec![".."; above];
    parts.extend(segments);
    parts.join("/")
}

/// `path` with `.` and `..` segments applied and repeated slashes folded, as Node's
/// `path.normalize` spells it; a trailing slash stays, and an empty result is `.`.
#[must_use]
pub fn normalize(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else {
                    segments.push("..");
                }
            }
            name => segments.push(name),
        }
    }

    let joined = segments.join("/");

    if path.ends_with('/') && !joined.is_empty() {
        format!("{joined}/")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

/// The project path a source specifier of `path` names: a relative path, a path below the source
/// root spelled from the project root, or a `layout.path-aliases` alias; `None` for a package.
#[must_use]
pub fn source_target(layout: &Layout, path: &str, specifier: &str) -> Option<String> {
    let root = layout.source_root.as_str();

    if specifier.starts_with('.') {
        Some(relative_target(path, specifier))
    } else if specifier == root
        || specifier
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
    {
        Some(normalize(specifier))
    } else {
        layout
            .resolve_alias(specifier)
            .map(|target| normalize(&target))
    }
}

/// A source path without its script extension: `.js`, `.ts`, `.jsx` or `.tsx`, each also with a
/// `c` or `m` before it.
#[must_use]
pub fn stem(path: &str) -> &str {
    let Some((before, extension)) = path.rsplit_once('.') else {
        return path;
    };
    let bytes = extension.as_bytes();
    let core = bytes
        .strip_prefix(b"c")
        .or_else(|| bytes.strip_prefix(b"m"))
        .unwrap_or(bytes);
    let script = matches!(core, [b'j' | b't', b's'] | [b'j' | b't', b's', b'x']);

    if script { before } else { path }
}

/// Whether a source target names the source file `file`, directly or as its folder's index.
#[must_use]
pub fn names_file(target: &str, file: &str) -> bool {
    let file = stem(file);
    target == file || format!("{target}/index") == file
}
