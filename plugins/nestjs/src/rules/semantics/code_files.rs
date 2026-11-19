//! The code files `nestjs/entity-descriptions` and `nestjs/eslint-directives` read: the project's
//! root files and the files below a set of roots, outside `[languages.typescript] skip-folders`,
//! and the JavaScript whitespace those rules trim.

use gate_typescript::{SourceFile, TypeScript};

use std::rc::Rc;

use crate::shared::{project, tree};

/// Which tracked files are code files.
#[derive(Debug)]
pub(crate) struct Universe<'o> {
    /// Path prefixes (with a final `/`) whose files are read; files at the project root are read
    /// too.
    pub roots: &'o [String],
    /// The extensions of code files, without the dot, compared case-sensitively.
    pub extensions: &'o [String],
}

impl Universe<'_> {
    /// Whether `path`, a file the walk reaches, is a code file of the universe: it has one of the
    /// extensions and lies at the project root or below one of the roots.
    fn contains(&self, path: &str) -> bool {
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str());

        if !extension
            .is_some_and(|extension| self.extensions.iter().any(|known| known == extension))
        {
            return false;
        }

        if !path.contains('/') {
            return true;
        }

        self.roots
            .iter()
            .any(|root| path.starts_with(root.as_str()))
    }

    /// The universe's files with a syntax tree, in path order: the walk of the whole tree outside
    /// `[languages.typescript] skip-folders` ([`tree::files`]), narrowed to the code files. A file
    /// without a syntax tree is left out, because `typescript/parse` reports it.
    ///
    /// # Errors
    /// When a tracked file cannot be read.
    pub(crate) fn parsed<'a>(
        &self,
        typescript: &TypeScript<'a>,
    ) -> gate_sdk::Result<Vec<(String, Rc<SourceFile<'a>>)>> {
        let mut files = Vec::new();

        for path in tree::files(typescript.source(), "", typescript.settings()) {
            if !self.contains(path) {
                continue;
            }

            if let Some(file) = project::parsed(typescript, path)? {
                files.push((path.to_owned(), file));
            }
        }

        Ok(files)
    }
}

/// Whether `character` is JavaScript whitespace (`WhiteSpace` and `LineTerminator`).
pub(crate) fn js_space(character: char) -> bool {
    matches!(
        character,
        '\t' | '\u{B}'
            | '\u{C}'
            | ' '
            | '\u{A0}'
            | '\u{FEFF}'
            | '\n'
            | '\r'
            | '\u{2028}'
            | '\u{2029}'
    ) || (character.is_whitespace() && !character.is_control())
}

/// `text` without JavaScript whitespace at both ends.
pub(crate) fn js_trim(text: &str) -> &str {
    text.trim_matches(js_space)
}

/// `text` without leading JavaScript whitespace.
pub(crate) fn js_trim_start(text: &str) -> &str {
    text.trim_start_matches(js_space)
}

/// `text` without trailing JavaScript whitespace.
pub(crate) fn js_trim_end(text: &str) -> &str {
    text.trim_end_matches(js_space)
}
