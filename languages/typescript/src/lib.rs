#![deny(missing_docs, clippy::expect_used)]

//! The TypeScript language library of gate: the TypeScript and JavaScript files of a [`Source`],
//! one parse per file and run, line numbers, module resolution with tsconfig `paths`, source
//! owner tables ([`owners`]), environment coverage ([`env`]), import bindings ([`binding`], [`binder`]), module edges
//! ([`modules`]), static value folding ([`value`]), JavaScript text semantics ([`javascript`],
//! [`js`]), the syntax helpers rule plugins share ([`syntax`]) and the call state that lets the
//! rules of one plugin call share their sources and parses ([`call`]).
//!
//! Every parse of one run allocates into one [`Allocator`] that lives as long as the run, so a
//! parsed [`SourceFile`] is shared by every rule that reads it. The oxc crates are re-exported so
//! rule plugins use exactly the versions this library was built with.
//!
//! The library reads files only through [`Source`] and takes its configurable values from
//! [`Settings`] (`[languages.typescript]`), so it depends on neither the gate host nor the plugin
//! runtime.

pub mod binder;
pub mod binding;
pub mod call;
pub mod env;
pub mod javascript;
pub mod js;
mod lines;
pub mod modules;
pub mod owners;
mod resolver;
pub mod source;
pub mod syntax;
pub mod value;

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

pub use oxc_allocator::Allocator;
pub use {oxc_ast, oxc_ast_visit, oxc_ecmascript, oxc_semantic, oxc_span, oxc_syntax};

use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;

pub use self::lines::LineIndex;
use self::resolver::Resolver;
pub use self::resolver::UnreadableBase;
pub use self::source::{
    Error, MemorySource, ReadSource, Result, Settings, Source, sorted_with_prefix,
};

/// The extensions of the files [`TypeScript::files`] lists, including `.d.ts`; they compare
/// case-sensitively, as the TypeScript compiler and oxc read them.
pub const EXTENSIONS: [&str; 4] = ["ts", "tsx", "mts", "cts"];

/// The lowercase extensions of every source file oxc parses: TypeScript and JavaScript.
pub const SOURCE_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The TypeScript view of one source, loaded once per run.
pub struct TypeScript<'a> {
    source: &'a dyn Source,
    settings: Settings,
    /// The project's root relative to the repository; empty for the repository.
    project: String,
    allocator: &'a Allocator,
    files: Vec<&'a str>,
    parsed: RefCell<HashMap<String, Option<Rc<SourceFile<'a>>>>>,
    /// The resolver of the configured tsconfig; resolution follows only its bundler-mode rules,
    /// so no other tsconfig is offered.
    resolver: OnceCell<Resolver>,
    /// The answers already given, by importer and specifier.
    resolved: RefCell<HashMap<(String, String), Option<String>>>,
}

impl std::fmt::Debug for TypeScript<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TypeScript")
            .field("files", &self.files.len())
            .field("parsed", &self.parsed.borrow().len())
            .finish_non_exhaustive()
    }
}

impl<'a> TypeScript<'a> {
    /// The TypeScript view of `source` with `settings`, parsing into `allocator`.
    #[must_use]
    pub fn new(source: &'a dyn Source, settings: Settings, allocator: &'a Allocator) -> Self {
        Self {
            source,
            settings,
            project: String::new(),
            allocator,
            files: source
                .paths()
                .filter(|path| {
                    std::path::Path::new(path)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| EXTENSIONS.contains(&extension))
                })
                .collect(),
            parsed: RefCell::new(HashMap::new()),
            resolver: OnceCell::new(),
            resolved: RefCell::new(HashMap::new()),
        }
    }

    /// The view with the source's files as the files of the project at `project`, relative to the
    /// repository (empty for the repository, the default). The module resolver reads a tsconfig
    /// base above the project root by its path relative to the project root, and maps it to the
    /// repository path the compiler reads, so a base's `paths` that lead back into the project
    /// resolve.
    #[must_use]
    pub fn in_project(mut self, project: &str) -> Self {
        project.clone_into(&mut self.project);
        self
    }

    /// The source the files are read from.
    #[must_use]
    pub fn source(&self) -> &'a dyn Source {
        self.source
    }

    /// The `[languages.typescript]` settings.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Every tracked TypeScript file (`.ts`, `.tsx`, `.mts`, `.cts`, including declarations) in
    /// path order.
    #[must_use]
    pub fn files(&self) -> &[&'a str] {
        &self.files
    }

    /// The tracked TypeScript files below `directory` (without a trailing slash; empty for all),
    /// in path order.
    pub fn files_under<'b>(&'b self, directory: &str) -> impl Iterator<Item = &'a str> + 'b {
        let prefix = if directory.is_empty() {
            String::new()
        } else {
            format!("{}/", directory.trim_end_matches('/'))
        };
        self.files
            .iter()
            .copied()
            .filter(move |path| path.starts_with(&prefix))
    }

    /// Parses one tracked TypeScript or JavaScript file, once per run.
    ///
    /// The result is `None` for an untracked path. A file with syntax errors still yields a
    /// [`SourceFile`] whose [`SourceFile::errors`] list them; unlike the TypeScript compiler, oxc
    /// recovers only from some errors and otherwise returns an empty program
    /// ([`SourceFile::fatal`]), so a check must not read an empty program as a clean file. A file
    /// that is not UTF-8 text is such a fatal file too, with one error and no line.
    ///
    /// # Errors
    /// Fails when the file cannot be read or has no JavaScript or TypeScript extension.
    pub fn parse(&self, path: &str) -> Result<Option<Rc<SourceFile<'a>>>> {
        if let Some(cached) = self.parsed.borrow().get(path) {
            return Ok(cached.clone());
        }

        let parsed = match self.source.bytes_for_parse(path)? {
            None => None,
            Some(bytes) => Some(Rc::new(if let Ok(text) = std::str::from_utf8(&bytes) {
                self.parse_text(path, text)?
            } else {
                let mut file = self.parse_text(path, "")?;
                file.fatal = true;
                file.errors = vec![SyntaxError {
                    line: None,
                    column: None,
                    message: "the file is not UTF-8 text".to_owned(),
                }];
                file
            })),
        };
        #[cfg(feature = "parse-log")]
        if parsed.is_some() {
            PARSED_ON_THREAD.with(|log| log.borrow_mut().push(path.to_owned()));
        }
        self.parsed
            .borrow_mut()
            .insert(path.to_owned(), parsed.clone());
        Ok(parsed)
    }

    /// The file [`Self::parse`] already parsed, without parsing it: `None` for a file not parsed
    /// yet, or not tracked.
    #[must_use]
    pub fn parsed(&self, path: &str) -> Option<Rc<SourceFile<'a>>> {
        self.parsed.borrow().get(path).cloned().flatten()
    }

    fn parse_text(&self, path: &str, text: &str) -> Result<SourceFile<'a>> {
        parse_source(self.allocator, path, text)
    }

    /// The tracked file `specifier` names when imported from `importer`, resolved with the
    /// `paths` of the tracked tsconfig of [`Settings::tsconfig`]. Without that tsconfig the
    /// resolver has no `paths`, which is allowed only for the default name: a tsconfig the
    /// settings name explicitly must be tracked. A base of its `extends` chain that cannot be read
    /// stands as an empty tsconfig and is listed by [`TypeScript::unreadable_bases`].
    ///
    /// # Errors
    /// Fails when the tsconfig or a `package.json` on the way cannot be read, or when a
    /// configured non-default tsconfig is not tracked.
    pub fn resolve(&self, importer: &str, specifier: &str) -> Result<Option<String>> {
        let key = (importer.to_owned(), specifier.to_owned());
        if let Some(known) = self.resolved.borrow().get(&key) {
            return Ok(known.clone());
        }
        if self.resolver.get().is_none() {
            if self.settings.tsconfig != Settings::default().tsconfig
                && !self.source.contains(&self.settings.tsconfig)
            {
                return Err(Error::config(format!(
                    "[languages.typescript] tsconfig names {}, which is not tracked; track that \
                     file or remove the setting to use the default tsconfig.json",
                    self.settings.tsconfig
                )));
            }
            let tsconfig = self
                .source
                .contains(&self.settings.tsconfig)
                .then_some(self.settings.tsconfig.as_str());
            let _ = self
                .resolver
                .set(Resolver::new(self.source, &self.project, tsconfig)?);
        }

        let resolved = self
            .resolver
            .get()
            .ok_or_else(|| Error::new("the module resolver is unavailable"))?
            .resolve(importer, specifier)?;
        self.resolved.borrow_mut().insert(key, resolved.clone());
        Ok(resolved)
    }

    /// The tsconfig bases of the configured tsconfig's `extends` chain that the source could not
    /// read: not tracked, not granted, outside the repository, or named by an absolute path. Each
    /// stood as an empty tsconfig, so the `paths` and `baseUrl` it sets were not applied. Empty
    /// until the first [`TypeScript::resolve`].
    #[must_use]
    pub fn unreadable_bases(&self) -> &[UnreadableBase] {
        self.resolver.get().map_or(&[][..], Resolver::unreadable)
    }
}

#[cfg(feature = "parse-log")]
thread_local! {
    static PARSED_ON_THREAD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The paths the views of this thread parsed since the last call, in order, one entry per parse
/// and none for an answer from a view's cache. Only with the `parse-log` feature, which tests
/// enable to count the parses of a call; a release build has no log.
#[cfg(feature = "parse-log")]
#[must_use]
pub fn take_parsed_on_this_thread() -> Vec<String> {
    PARSED_ON_THREAD.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Parses `text` as one TypeScript expression into `allocator`, as a policy target that names a
/// construction is read; `None` when it is not exactly one expression.
#[must_use]
pub fn parse_expression<'a>(
    allocator: &'a Allocator,
    text: &str,
) -> Option<oxc_ast::ast::Expression<'a>> {
    let source = allocator.alloc_str(text);
    Parser::new(allocator, source, SourceType::ts())
        .parse_expression()
        .ok()
}

/// One syntax error oxc reported while parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxError {
    /// The one-based line oxc points at; `None` when it points nowhere (a file that is not UTF-8).
    pub line: Option<u32>,
    /// The one-based column of that point, in characters; `None` with the line.
    pub column: Option<u32>,
    /// oxc's message.
    pub message: String,
}

/// Checks that `path`, the value of the option `option`, names a file this library parses, so that
/// an option naming a file of another kind is refused when the options are checked.
///
/// # Errors
/// A configuration error that names the option and the extensions it accepts.
pub fn check_source_option(option: &str, path: &str) -> Result<()> {
    if SourceType::from_path(path).is_ok() {
        Ok(())
    } else {
        Err(Error::config(format!(
            "option `{option}` is {path:?}, which is not a JavaScript or TypeScript file; name a \
             file ending in {}",
            extensions()
        )))
    }
}

/// [`SOURCE_EXTENSIONS`] as a message lists them: `.ts, .tsx, … or .cjs`.
fn extensions() -> String {
    let dotted: Vec<String> = SOURCE_EXTENSIONS
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect();
    match dotted.split_last() {
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// Parses one source text as the file `path` names it, outside any source, into `allocator`.
///
/// # Errors
/// Fails when `path` has no JavaScript or TypeScript extension.
pub fn parse_source<'a>(
    allocator: &'a Allocator,
    path: &str,
    text: &str,
) -> Result<SourceFile<'a>> {
    let source_type = SourceType::from_path(path).map_err(|_| {
        Error::new(format!(
            "{path} is not a JavaScript or TypeScript file, so it cannot be parsed; the option \
                 that names it should name a file ending in {}",
            extensions()
        ))
    })?;
    let source = allocator.alloc_str(text);
    let parsed = Parser::new(allocator, source, source_type).parse();
    let lines = LineIndex::new(source);
    let errors = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let offset = diagnostic.labels.first().map(oxc_span::LabeledSpan::offset);
            SyntaxError {
                line: offset.map(|offset| lines.line(offset)),
                column: offset.map(|offset| lines.column(source, offset)),
                message: diagnostic.message.to_string(),
            }
        })
        .collect();

    Ok(SourceFile {
        path: path.to_owned(),
        source,
        program: allocator.alloc(parsed.program),
        fatal: parsed.fatal_error,
        errors,
        lines,
        semantic: OnceCell::new(),
    })
}

/// One parsed file; its syntax tree lives as long as the run.
pub struct SourceFile<'a> {
    path: String,
    source: &'a str,
    program: &'a Program<'a>,
    fatal: bool,
    errors: Vec<SyntaxError>,
    lines: LineIndex,
    semantic: OnceCell<Semantic<'a>>,
}

impl std::fmt::Debug for SourceFile<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceFile")
            .field("path", &self.path)
            .field("fatal", &self.fatal)
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}

impl<'a> SourceFile<'a> {
    /// The file's path in the source.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The file's text.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// The syntax tree; empty when [`Self::fatal`] is `true`.
    #[must_use]
    pub fn program(&self) -> &'a Program<'a> {
        self.program
    }

    /// Whether the file has no syntax tree: oxc gave up and returned an empty program, or the file
    /// is not UTF-8 text.
    #[must_use]
    pub fn fatal(&self) -> bool {
        self.fatal
    }

    /// The syntax errors oxc reported; empty for a valid file.
    #[must_use]
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    /// The one-based line of a byte offset, such as a span start.
    #[must_use]
    pub fn line(&self, offset: u32) -> u32 {
        self.lines.line(offset)
    }

    /// The byte range of one-based `line` including its terminator; `None` past the last line.
    #[must_use]
    pub fn line_bounds(&self, line: u32) -> Option<(u32, u32)> {
        self.lines
            .bounds(line, u32::try_from(self.source.len()).unwrap_or(u32::MAX))
    }

    /// The one-based line and column range of a syntax node's span, for a finding; columns count
    /// bytes, and the end column is the last byte's.
    #[must_use]
    pub fn span(&self, span: oxc_span::Span) -> gate_sdk::Span {
        let column = |offset: u32| {
            let line = self.line(offset);
            let start = self.line_bounds(line).map_or(0, |(start, _)| start);
            (line, offset.saturating_sub(start) + 1)
        };
        let (line, column_start) = column(span.start);
        let (end_line, end_column) = column(span.end.saturating_sub(1).max(span.start));
        gate_sdk::Span {
            line,
            column: column_start,
            end_line,
            end_column,
        }
    }

    /// The source text of a syntax node's span.
    #[must_use]
    pub fn text(&self, span: oxc_span::Span) -> &'a str {
        self.source
            .get(span.start as usize..span.end as usize)
            .unwrap_or_default()
    }

    /// Scopes, symbols, references and parent links of the program, built on first use.
    #[must_use]
    pub fn semantic(&self) -> &Semantic<'a> {
        self.semantic.get_or_init(|| {
            SemanticBuilder::new()
                .with_build_nodes(true)
                .build(self.program)
                .semantic
        })
    }
}
