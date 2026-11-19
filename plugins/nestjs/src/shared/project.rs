//! The project's TypeScript sources as a rule reads them: the tracked files of the run's tree,
//! read on demand, with the `[languages.typescript]` settings.

use std::rc::Rc;

use gate_sdk::Run;
use gate_typescript::{ReadSource, Settings, SourceFile, TypeScript};

/// The run's tracked files as a TypeScript source, read when the library asks for a file.
pub fn source<'h, O>(
    run: &Run<'h, O>,
) -> ReadSource<impl Fn(&str) -> Option<Vec<u8>> + use<'h, O>> {
    let tree = run.tree();
    ReadSource::new(tree.paths(), move |path| tree.bytes(path))
}

/// The `[languages.typescript]` settings, or the defaults when the configuration has none. Their
/// `skip-folders` is the plugin's one skip list: every source walk of every rule skips those folders.
///
/// # Errors
/// When the settings do not match: an unknown key or a value of the wrong type.
pub fn settings<O>(run: &Run<'_, O>) -> gate_sdk::Result<Settings> {
    Ok(run.language::<Settings>("typescript")?.unwrap_or_default())
}

/// A library error (a file that cannot be read, a tsconfig that cannot be parsed) as the run's
/// error.
#[must_use]
pub fn error(error: &gate_typescript::Error) -> gate_sdk::Error {
    gate_sdk::Error::new(error.to_string())
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
    Ok(typescript
        .parse(path)
        .map_err(|failure| error(&failure))?
        .filter(|file| !file.fatal()))
}
