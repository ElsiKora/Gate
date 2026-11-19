//! Syntax queries the rules share.

use gate_typescript::owners::class_declaration;
use gate_typescript::oxc_ast::ast::{Class, Statement};

/// The classes declared directly in the file body, exported, default-exported or not.
#[must_use]
pub fn top_level_classes<'b, 'a>(body: &'b [Statement<'a>]) -> Vec<&'b Class<'a>> {
    body.iter().filter_map(class_declaration).collect()
}

/// The `.ts` files of `files` without declaration files. The extension compares case-sensitively,
/// and a file named `.ts` has no extension.
pub fn typescript_sources<'a>(files: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    files
        .filter(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            name.rsplit_once('.')
                .is_some_and(|(stem, extension)| !stem.is_empty() && extension == "ts")
                && !name.ends_with(".d.ts")
        })
        .collect()
}
