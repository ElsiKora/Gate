//! The pages a rule reads: tracked `.md` files below its roots, and its extra files.

use std::collections::BTreeSet;

use gate_sdk::Tree;

/// The default of every rule's `roots`.
pub(crate) fn default_roots() -> Vec<String> {
    vec!["docs".to_owned()]
}

/// A `.md` file; the extension compares case-sensitively, as link destinations do.
pub(crate) fn is_markdown(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "md")
}

/// The tracked `.md` files below each of `roots` (an empty root is the whole project), and each of
/// `extra_files` that is tracked, whatever its extension, sorted and each once. An extra file that
/// is not tracked is left out without a word: the option names files a project may not have.
pub(crate) fn pages(tree: &Tree<'_>, roots: &[String], extra_files: &[String]) -> Vec<String> {
    let mut pages = BTreeSet::new();

    for root in roots {
        pages.extend(
            tree.under(root)
                .into_iter()
                .filter(|path| is_markdown(path)),
        );
    }

    for file in extra_files {
        if tree.contains(file) {
            pages.insert(file.clone());
        }
    }

    pages.into_iter().collect()
}

/// The text of a page that must be UTF-8.
///
/// # Errors
/// When the page is not UTF-8 text: the rule cannot read its links.
pub(crate) fn strict_text(tree: &Tree<'_>, path: &str) -> gate_sdk::Result<Option<String>> {
    tree.bytes(path)
        .map(|bytes| {
            String::from_utf8(bytes).map_err(|_| {
                gate_sdk::Error::new(format!(
                    "{path} is not UTF-8 text; save it as UTF-8, or leave it out of the rule's \
                     roots and extra-files"
                ))
            })
        })
        .transpose()
}
