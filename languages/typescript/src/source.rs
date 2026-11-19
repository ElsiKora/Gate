//! What the library reads and how it is configured: a [`Source`] of tracked files, the library's
//! [`Error`], and the `[languages.typescript]` [`Settings`].

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use serde::Deserialize;

/// The tracked files the library reads. A rule plugin passes its project tree (`gate-sdk`'s `Tree`
/// implements this trait); tests pass an in-memory map. Paths are relative to the project root,
/// `/`-separated, in sorted order.
pub trait Source {
    /// Every tracked path, in sorted order.
    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_>;

    /// The contents of one tracked file; `None` for a path that is not tracked.
    ///
    /// # Errors
    /// Fails when a tracked file cannot be read.
    fn bytes(&self, path: &str) -> Result<Option<Rc<[u8]>>>;

    /// The contents of one tracked file that a view is about to parse ([`crate::TypeScript::parse`]);
    /// `None` for a path that is not tracked. The parsed file keeps the text
    /// ([`crate::SourceFile::source`]), so a source that keeps what it reads may drop its copy of a
    /// UTF-8 file here; the default reads it as [`Self::bytes`] does.
    ///
    /// # Errors
    /// Fails when a tracked file cannot be read.
    fn bytes_for_parse(&self, path: &str) -> Result<Option<Rc<[u8]>>> {
        self.bytes(path)
    }

    /// The contents of one tracked UTF-8 text file; `None` for a path that is not tracked.
    ///
    /// # Errors
    /// Fails when the file cannot be read or is not UTF-8.
    fn text(&self, path: &str) -> Result<Option<Rc<str>>> {
        match self.bytes(path)? {
            None => Ok(None),
            Some(bytes) => std::str::from_utf8(&bytes)
                .map(|text| Some(Rc::from(text)))
                .map_err(|_| Error::new(format!("{path} is not UTF-8 text"))),
        }
    }

    /// Whether `path` is tracked.
    fn contains(&self, path: &str) -> bool {
        self.paths().any(|tracked| tracked == path)
    }
}

/// A [`Source`] held in memory: for tests, and for a plugin that already holds its files.
#[derive(Clone, Debug, Default)]
pub struct MemorySource {
    files: BTreeMap<String, Rc<[u8]>>,
}

impl MemorySource {
    /// A source of `files`, each a path and its contents.
    #[must_use]
    pub fn new<P, C>(files: impl IntoIterator<Item = (P, C)>) -> Self
    where
        P: Into<String>,
        C: AsRef<[u8]>,
    {
        Self {
            files: files
                .into_iter()
                .map(|(path, contents)| (path.into(), Rc::from(contents.as_ref())))
                .collect(),
        }
    }
}

impl Source for MemorySource {
    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.files.keys().map(String::as_str))
    }

    fn bytes(&self, path: &str) -> Result<Option<Rc<[u8]>>> {
        Ok(self.files.get(path).map(Rc::clone))
    }

    fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}

/// A [`Source`] over a list of tracked paths and a function that reads one: how a rule plugin
/// bridges its tree (`ReadSource::new(tree.paths(), move |path| tree.bytes(path))`) without this
/// library depending on the plugin SDK. A file is read when the library asks for it, not before.
pub struct ReadSource<F> {
    paths: Vec<String>,
    read: F,
}

impl<F> fmt::Debug for ReadSource<F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReadSource")
            .field("paths", &self.paths.len())
            .finish_non_exhaustive()
    }
}

impl<F> ReadSource<F>
where
    F: Fn(&str) -> Option<Vec<u8>>,
{
    /// A source of `paths` (sorted here), whose contents `read` returns; `read` answers `None` for
    /// a path it cannot read, which the library takes for an untracked path.
    #[must_use]
    pub fn new(mut paths: Vec<String>, read: F) -> Self {
        paths.sort_unstable();
        paths.dedup();
        Self { paths, read }
    }
}

impl<F> Source for ReadSource<F>
where
    F: Fn(&str) -> Option<Vec<u8>>,
{
    fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.paths.iter().map(String::as_str))
    }

    fn bytes(&self, path: &str) -> Result<Option<Rc<[u8]>>> {
        if !self.contains(path) {
            return Ok(None);
        }
        Ok((self.read)(path).map(Rc::from))
    }

    fn contains(&self, path: &str) -> bool {
        self.paths
            .binary_search_by(|tracked| tracked.as_str().cmp(path))
            .is_ok()
    }
}

/// A failure of a library call: a configuration error when the `[languages.typescript]` settings
/// do not fit the tree, an environment error when a file cannot be read or a configuration file
/// cannot be parsed. A violation of a rule is a finding, never an error. It converts into the
/// SDK's error with its kind kept, so a plugin passes it on with `?`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    configuration: bool,
    message: String,
}

impl Error {
    /// An environment error, with a message that says what is wrong and what to do.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            configuration: false,
            message: message.into(),
        }
    }

    /// A configuration error: a `[languages.typescript]` setting that does not fit the tree. The
    /// message names the setting and says what to write instead.
    #[must_use]
    pub fn config(message: impl Into<String>) -> Self {
        Self {
            configuration: true,
            message: message.into(),
        }
    }

    /// Whether this is a configuration error ([`Error::config`]).
    #[must_use]
    pub fn is_config(&self) -> bool {
        self.configuration
    }

    /// The message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<Error> for gate_sdk::Error {
    fn from(error: Error) -> Self {
        if error.configuration {
            Self::config(error.message)
        } else {
            Self::new(error.message)
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// The result of a library call.
pub type Result<T> = std::result::Result<T, Error>;

/// `[languages.typescript]`: the settings of the language, configured once per project and passed
/// to every plugin that declares it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Settings {
    /// The folders a source walk skips, as `.gitignore` folder entries ([`Self::skips`]): a name
    /// without `/` at any depth, an entry with `/` from the project root.
    pub skip_folders: Vec<String>,
    /// The tsconfig whose `paths` the module resolver reads when it is tracked, relative to the
    /// project root.
    pub tsconfig: String,
}

impl Settings {
    /// Whether `path` (relative to the project root) lies in a skipped folder, matched the way
    /// `.gitignore` matches a folder entry:
    ///
    /// - an entry without `/` names a folder at any depth: `node_modules` skips
    ///   `node_modules/a.ts` and `src/node_modules/a.ts`;
    /// - an entry with a `/` (leading or inner) names that folder from the project root: `/build`
    ///   skips `build/a.ts` but not `src/build/a.ts`, and `infrastructure/cdk/cdk.out` skips only
    ///   the files below that folder.
    ///
    /// A trailing `/` changes nothing. Every walk that skips folders asks this.
    #[must_use]
    pub fn skips(&self, path: &str) -> bool {
        let Some((folders, _)) = path.rsplit_once('/') else {
            return false;
        };

        self.skip_folders.iter().any(|entry| {
            let entry = entry.trim_end_matches('/');

            if entry.contains('/') {
                let anchored = entry.trim_start_matches('/');

                !anchored.is_empty()
                    && (folders == anchored
                        || folders
                            .strip_prefix(anchored)
                            .is_some_and(|rest| rest.starts_with('/')))
            } else {
                folders.split('/').any(|folder| folder == entry)
            }
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skip_folders: [".git", "node_modules", "/build", "/coverage", "/dist"]
                .map(str::to_owned)
                .to_vec(),
            tsconfig: "tsconfig.json".to_owned(),
        }
    }
}

/// Whether `path` ends with one of `extensions` (without the dot), ignoring ASCII case.
#[must_use]
pub fn has_extension(path: &str, extensions: &[&str]) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
}
