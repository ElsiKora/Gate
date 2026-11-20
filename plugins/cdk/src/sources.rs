//! The app's TypeScript sources as the rules of one call share them ([`Cdk`]): one TypeScript view
//! with the `[languages.typescript]` settings, so each file is parsed once whichever rule asks
//! first.

use std::rc::Rc;

use gate_sdk::{Call, CallState};
use gate_typescript::{Allocator, ReadSource, Settings, Source, SourceFile, TypeScript};

/// Reads one tracked file for the view.
type Reader<'h> = Box<dyn Fn(&str) -> Option<Vec<u8>> + 'h>;

/// The plugin's call state ([`CallState`]).
#[derive(Debug)]
pub struct Cdk;

impl CallState for Cdk {
    type Owner<'h> = Owned<'h>;
    type State<'o> = Sources<'o>;

    fn own<'h>(call: &Call<'h>) -> Owned<'h> {
        let tree = call.tree();
        Owned {
            allocator: Allocator::default(),
            source: ReadSource::new(tree.paths(), Box::new(move |path: &str| tree.bytes(path))),
            settings: call
                .language::<Settings>("typescript")
                .map(Option::unwrap_or_default),
        }
    }

    fn state<'o>(owner: &'o Owned<'_>) -> Sources<'o> {
        Sources {
            typescript: match &owner.settings {
                Ok(settings) => Ok(TypeScript::new(
                    &owner.source,
                    settings.clone(),
                    &owner.allocator,
                )),
                Err(error) => Err(error.clone()),
            },
        }
    }
}

/// What one call owns: the arena every parse allocates into, the tracked files and the settings.
pub struct Owned<'h> {
    allocator: Allocator,
    source: ReadSource<Reader<'h>>,
    settings: gate_sdk::Result<Settings>,
}

impl std::fmt::Debug for Owned<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Owned")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// What the rules of one call share: the TypeScript view.
pub struct Sources<'o> {
    typescript: gate_sdk::Result<TypeScript<'o>>,
}

impl std::fmt::Debug for Sources<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Sources").finish_non_exhaustive()
    }
}

impl<'o> Sources<'o> {
    /// The call's TypeScript view.
    ///
    /// # Errors
    /// When the `[languages.typescript]` settings do not match.
    pub fn typescript(&self) -> gate_sdk::Result<&TypeScript<'o>> {
        self.typescript.as_ref().map_err(Clone::clone)
    }
}

/// Whether the file name of `path` has the extension `extension`, in any case.
#[must_use]
pub fn has_extension(path: &str, extension: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// The `.ts` files (not `.d.ts`) below `folder` (with its trailing `/`) that the view's skip list
/// does not skip, in path order.
#[must_use]
pub fn code_files<'a>(typescript: &TypeScript<'a>, folder: &str) -> Vec<&'a str> {
    let settings = typescript.settings();
    let source: &'a dyn Source = typescript.source();
    source
        .paths_with_prefix(folder)
        .filter(|path| {
            has_extension(path, "ts") && !path.ends_with(".d.ts") && !settings.skips(path)
        })
        .collect()
}

/// The code files of `bin/` and `lib/`.
#[must_use]
pub fn app_files<'a>(typescript: &TypeScript<'a>) -> Vec<&'a str> {
    let mut files = code_files(typescript, "bin/");
    files.extend(code_files(typescript, "lib/"));
    files
}

/// `path` parsed, or `None` when it does not parse: `typescript/parse` reports that file.
///
/// # Errors
/// When the file cannot be read.
pub fn parsed<'a>(
    typescript: &TypeScript<'a>,
    path: &str,
) -> gate_sdk::Result<Option<Rc<SourceFile<'a>>>> {
    Ok(typescript.parse(path)?.filter(|file| !file.fatal()))
}
