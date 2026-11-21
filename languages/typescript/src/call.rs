//! The TypeScript sources of one plugin call as its rules share them ([`TypeScriptCall`]): the
//! tracked files of the call's tree, each read from the host once per call whichever rule asks
//! first, and one TypeScript view with the `[languages.typescript]` settings that parses each file
//! once and resolves each import once. A plugin names [`TypeScriptCall`] as its call state, and a
//! rule reaches the [`Sources`] through [`gate_sdk::Run::with_call`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gate_sdk::{Call, CallState, Location, Report, Run, Tree};

use crate::{Allocator, Settings, Source, SourceFile, TypeScript, UnreadableBase};

/// A plugin's call state ([`CallState`]): the sources and the TypeScript view its rules share.
#[derive(Debug)]
pub struct TypeScriptCall;

impl CallState for TypeScriptCall {
    type Owner<'h> = Owned<'h>;
    type State<'o> = Sources<'o>;

    fn own<'h>(call: &Call<'h>) -> Owned<'h> {
        Owned {
            allocator: Allocator::default(),
            source: CallSource::new(call.tree()),
            project: call.project(),
            settings: call
                .language::<Settings>("typescript")
                .map(Option::unwrap_or_default),
        }
    }

    fn state<'o>(owner: &'o Owned<'_>) -> Sources<'o> {
        Sources {
            source: &owner.source,
            typescript: match &owner.settings {
                Ok(settings) => {
                    Ok(
                        TypeScript::new(&owner.source, settings.clone(), &owner.allocator)
                            .in_project(owner.project),
                    )
                }
                Err(error) => Err(error.clone()),
            },
            reported: Cell::new(false),
        }
    }

    /// Reports each tsconfig base the module resolver could not read ([`report_unreadable_bases`]).
    fn report(state: &Sources<'_>, report: &mut Report) {
        if let Ok(typescript) = &state.typescript {
            report_unreadable_bases(typescript, &state.reported, report);
        }
    }
}

/// Reports each tsconfig base the module resolver of `typescript` could not read
/// ([`TypeScript::unreadable_bases`]) into `report`, once per call: `reported` records that a rule
/// of the call did. A plugin's [`CallState::report`] calls it with its view, so the bases are
/// findings of the first rule whose imports the view resolved, whose results may lack what the
/// bases' `paths` resolve. Each finding is at the tsconfig that names the base, with no aspect;
/// its target is `<tsconfig>:<extends entry>` and its help says how to make the base readable.
pub fn report_unreadable_bases(
    typescript: &TypeScript<'_>,
    reported: &Cell<bool>,
    report: &mut Report,
) {
    let bases = typescript.unreadable_bases();
    if bases.is_empty() || reported.get() {
        return;
    }
    reported.set(true);
    for base in bases {
        report
            .finding(
                "",
                Location::file(base.tsconfig.clone()),
                format!(
                    "{} extends {:?}, which the plugin cannot read, so the module resolver runs \
                     without it: the paths and baseUrl it sets are not applied, and an import \
                     they resolve is not followed",
                    base.tsconfig, base.extends
                ),
            )
            .target(format!("{}:{}", base.tsconfig, base.extends))
            .help(unreadable_help(base))
            .key(base.extends.clone());
    }
}

/// What to do about a tsconfig base the resolver could not read.
fn unreadable_help(base: &UnreadableBase) -> String {
    match (&base.path, &base.repository) {
        (Some(path), Some(repository)) if path.starts_with("../") => format!(
            "the plugin reads a file outside its project only when it is tracked and granted: \
             track {repository} and add it to the plugin's entry in gate.toml, \
             reads = [\"{repository}\"]"
        ),
        (Some(_), Some(repository)) => {
            format!("track {repository}: the plugin reads only tracked files")
        }
        (Some(_), None) => "the base lies outside the repository, which the plugin never \
                            reads; extend a tracked file inside the repository"
            .to_owned(),
        (None, _) => "name the base by a path relative to the tsconfig; the plugin never reads \
                      an absolute path"
            .to_owned(),
    }
}

/// What one call owns: the arena every parse of the call allocates into, the call's tracked files,
/// its project's root and the settings.
pub struct Owned<'h> {
    allocator: Allocator,
    source: CallSource<'h>,
    project: &'h str,
    settings: gate_sdk::Result<Settings>,
}

/// What the rules of one call share: the call's tracked files and its TypeScript view.
pub struct Sources<'o> {
    source: &'o CallSource<'o>,
    typescript: gate_sdk::Result<TypeScript<'o>>,
    /// Whether a rule of the call reported the tsconfig bases the resolver could not read.
    reported: Cell<bool>,
}

impl std::fmt::Debug for Owned<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Owned")
            .field("files", &self.source.paths.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Sources<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Sources")
            .field("files", &self.source.paths.len())
            .finish_non_exhaustive()
    }
}

impl<'o> Sources<'o> {
    /// The call's tracked files as a TypeScript source.
    #[must_use]
    pub fn source(&self) -> &'o dyn Source {
        self.source
    }

    /// The call's TypeScript view: a file one rule parsed, or an import it resolved, is not
    /// parsed or resolved again for another.
    ///
    /// # Errors
    /// When the `[languages.typescript]` settings do not match: an unknown key or a value of the
    /// wrong type.
    pub fn typescript(&self) -> gate_sdk::Result<&TypeScript<'o>> {
        self.typescript.as_ref().map_err(Clone::clone)
    }

    /// The bytes of a tracked file, as [`Tree::bytes`] reads them but once per call; a file the
    /// view parsed answers from its parsed text.
    #[must_use]
    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        if let Some(known) = self.source.read.borrow().get(path) {
            return known.as_deref().map(<[u8]>::to_vec);
        }
        if let Some(file) = self
            .typescript
            .as_ref()
            .ok()
            .and_then(|typescript| typescript.parsed(path))
        {
            return Some(file.source().as_bytes().to_vec());
        }
        self.source.read(path).map(|bytes| bytes.to_vec())
    }

    /// The text of a tracked file, as [`Tree::text`] decodes it but read once per call.
    #[must_use]
    pub fn text(&self, path: &str) -> Option<String> {
        self.bytes(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// The call's tracked files, each read from the host once: what a rule reads stays for the other
/// rules, and what the view parses is dropped once parsed, since the parsed file keeps the text.
struct CallSource<'h> {
    tree: Tree<'h>,
    paths: Vec<String>,
    read: RefCell<HashMap<String, Option<Rc<[u8]>>>>,
}

impl<'h> CallSource<'h> {
    fn new(tree: Tree<'h>) -> Self {
        let mut paths = tree.paths();
        paths.sort_unstable();
        paths.dedup();
        Self {
            tree,
            paths,
            read: RefCell::new(HashMap::new()),
        }
    }

    /// Reads `path` from the host and keeps it for the other rules.
    fn read(&self, path: &str) -> Option<Rc<[u8]>> {
        let bytes = self.tree.bytes(path).map(Rc::from);
        self.read
            .borrow_mut()
            .insert(path.to_owned(), bytes.clone());
        bytes
    }
}

impl Source for CallSource<'_> {
    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.paths.iter().map(String::as_str))
    }

    fn paths_with_prefix(&self, prefix: &str) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(crate::sorted_with_prefix(&self.paths, prefix))
    }

    fn bytes(&self, path: &str) -> crate::Result<Option<Rc<[u8]>>> {
        if let Some(known) = self.read.borrow().get(path) {
            return Ok(known.clone());
        }
        Ok(self.read(path))
    }

    fn bytes_for_parse(&self, path: &str) -> crate::Result<Option<Rc<[u8]>>> {
        let known = self.read.borrow().get(path).cloned();
        let bytes = match known {
            Some(bytes) => bytes,
            None => self.tree.bytes(path).map(Rc::from),
        };
        // The parsed file keeps a UTF-8 file's text; a file that is not UTF-8 keeps its bytes here
        // for a rule that reads them.
        match &bytes {
            Some(contents) if std::str::from_utf8(contents).is_err() => {
                self.read
                    .borrow_mut()
                    .insert(path.to_owned(), bytes.clone());
            }
            _ => {
                self.read.borrow_mut().remove(path);
            }
        }
        Ok(bytes)
    }

    fn contains(&self, path: &str) -> bool {
        self.paths
            .binary_search_by(|tracked| tracked.as_str().cmp(path))
            .is_ok()
    }
}

/// The `[languages.typescript]` settings, or the defaults when the configuration has none. Their
/// `skip-folders` is a plugin's one skip list: every source walk of every rule skips those folders.
///
/// # Errors
/// When the settings do not match: an unknown key or a value of the wrong type.
pub fn settings<O>(run: &Run<'_, O, TypeScriptCall>) -> gate_sdk::Result<Settings> {
    Ok(run.language::<Settings>("typescript")?.unwrap_or_default())
}

/// The syntax tree of `path`, or `None` when the file is not tracked or cannot be parsed: a file
/// without a syntax tree is skipped here, because `typescript/parse` reports it.
///
/// # Errors
/// When the file is tracked but cannot be read.
pub fn parsed<'a>(
    typescript: &TypeScript<'a>,
    path: &str,
) -> gate_sdk::Result<Option<Rc<SourceFile<'a>>>> {
    Ok(typescript.parse(path)?.filter(|file| !file.fatal()))
}

#[cfg(test)]
mod tests {
    use gate_sdk::{Tree, TreeHost};

    use crate::Source;

    use super::CallSource;

    /// A host whose tracked paths come unsorted, with names that sort around `/`.
    struct Paths;

    const PATHS: [&str; 8] = [
        "src/ab/x.ts",
        "src/a/b.ts",
        "src/a.ts",
        "src/a-b/c.ts",
        "src/a/c/d.ts",
        "package.json",
        "src/a0/e.ts",
        "test/a.ts",
    ];

    impl TreeHost for Paths {
        fn list(&self, prefix: &str) -> Vec<String> {
            PATHS
                .iter()
                .filter(|path| path.starts_with(prefix))
                .map(|path| (*path).to_owned())
                .collect()
        }

        fn read(&self, _path: &str) -> Option<Vec<u8>> {
            None
        }

        fn changed(&self) -> Option<Vec<String>> {
            None
        }
    }

    #[test]
    fn the_call_source_answers_a_prefix_with_the_paths_that_start_with_it() {
        let source = CallSource::new(Tree::new(&Paths));

        for prefix in [
            "", "src/", "src/a/", "src/a", "src/a-", "src/ab/", "src/z/", "zz",
        ] {
            let expected: Vec<&str> = source
                .paths()
                .filter(|path| path.starts_with(prefix))
                .collect();
            assert_eq!(
                source.paths_with_prefix(prefix).collect::<Vec<_>>(),
                expected,
                "{prefix:?}"
            );
        }
        assert_eq!(
            source.paths_with_prefix("src/a/").collect::<Vec<_>>(),
            ["src/a/b.ts", "src/a/c/d.ts"]
        );
    }
}
