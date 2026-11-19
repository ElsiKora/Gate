//! Module specifier resolution with tsconfig `paths` over the tracked files of a source.
//!
//! The resolver sees the source as a virtual file system mounted at [`ROOT`]: every tracked path
//! exists, nothing else does (no directory above the project either), and no real file is read.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use oxc_resolver::{
    EnforceExtension, FileMetadata, FileSystem, ResolveError, ResolveOptions, ResolverGeneric,
    TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};

use crate::source::has_extension;
use crate::{Error, Result, Source};

/// Where the virtual file system of the resolver is mounted. It sits two directories deep, and
/// neither of its ancestors is a directory of the file system, so a `..` that leaves the project
/// (`../../src/a` from `src/app.ts`, a `paths` target `../shared/*`) reaches nothing, as it reached
/// no tracked file of the project before. Error texts name paths relative to it.
const ROOT: &str = "/gate/project";

/// The extensions a specifier without a TypeScript-known extension is tried with, in the order of
/// the TypeScript compiler's `tryAddingExtensions`.
const EXTENSIONS: [&str; 5] = [".ts", ".tsx", ".d.ts", ".js", ".jsx"];

/// For a specifier with a known extension, the files the compiler tries instead, in its order: the
/// extension is replaced, never loaded as written first.
const EXTENSION_ALIASES: [(&str, &[&str]); 9] = [
    (".cjs", &[".cts", ".d.cts", ".cjs"]),
    (".cts", &[".cts", ".d.cts", ".cjs"]),
    (".js", &[".ts", ".tsx", ".d.ts", ".js", ".jsx"]),
    (".json", &[".d.json.ts", ".json"]),
    (".jsx", &[".tsx", ".ts", ".d.ts", ".jsx", ".js"]),
    (".mjs", &[".mts", ".d.mts", ".mjs"]),
    (".mts", &[".mts", ".d.mts", ".mjs"]),
    (".ts", &[".ts", ".tsx", ".d.ts", ".js", ".jsx"]),
    (".tsx", &[".tsx", ".ts", ".d.ts", ".jsx", ".js"]),
];

/// A declaration extension of a specifier and the source extension whose files the compiler tries
/// instead: it drops `.d.ts`, `.d.mts` or `.d.cts` as it drops any extension it knows.
const DECLARATION_EXTENSIONS: [(&str, &str); 3] =
    [(".d.ts", ".ts"), (".d.mts", ".mts"), (".d.cts", ".cts")];

/// Resolves module specifiers the way `ts.resolveModuleName` does for a tsconfig that sets
/// `moduleResolution` `bundler` with `module` `ESNext`: relative paths, then
/// the tsconfig `paths`, then the package's own
/// `imports` and self-referencing `exports` with the `import`, `types` and `default` conditions, then
/// `node_modules`. A specifier with a declaration extension stands first for its source extension
/// ([`DECLARATION_EXTENSIONS`]) and then for itself, a specifier with a known extension tries the
/// [`EXTENSION_ALIASES`] of that extension, and any specifier then tries [`EXTENSIONS`] appended and the directory
/// (`typings`, `types` or `main` of its `package.json`, then `index`); a file is never loaded
/// without one of those extensions.
///
/// Only tracked files exist for the resolver, so a specifier that the compiler would resolve into
/// an untracked `node_modules` package, outside the project or through a differently cased path
/// resolves to nothing; the compiler reports those as external or absent too, except for case. A
/// tracked file below a `node_modules` directory resolves to nothing as well, as rules treat
/// the compiler's external library imports.
///
/// Of the tsconfig only `paths`, `baseUrl`, `rootDirs` and `extends` are read: the conditions and
/// the extensions are those of bundler mode, so the resolver serves only the configured
/// tsconfig. oxc also differs from the compiler in cases no tested source needs: an `exports` or `imports` target or folder mapping without a known extension gains
/// extensions or an `index`, an array target does not fall through to its next entry, and
/// `typesVersions` and versioned `types@` conditions are ignored.
pub struct Resolver {
    root: PathBuf,
    inner: ResolverGeneric<SourceFileSystem>,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Resolver")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Resolver {
    /// A resolver for the tracked files of `source` with the tracked tsconfig at `tsconfig`
    /// (relative to the project root), or without `paths` when there is none.
    ///
    /// # Errors
    /// Fails when a configuration file cannot be read.
    pub(crate) fn new(source: &dyn Source, tsconfig: Option<&str>) -> Result<Self> {
        let root = PathBuf::from(ROOT);
        let file_system = SourceFileSystem::load(source, &root)?;
        let tsconfig = tsconfig.map(|path| {
            TsconfigDiscovery::Manual(TsconfigOptions {
                config_file: root.join(path),
                references: TsconfigReferences::Disabled,
            })
        });
        let options = ResolveOptions {
            cwd: Some(root.clone()),
            tsconfig,
            condition_names: ["import", "types"].map(str::to_owned).to_vec(),
            enforce_extension: EnforceExtension::Enabled,
            extension_alias: EXTENSION_ALIASES
                .iter()
                .map(|(extension, aliases)| {
                    (
                        (*extension).to_owned(),
                        aliases.iter().map(|alias| (*alias).to_owned()).collect(),
                    )
                })
                .collect(),
            extensions: EXTENSIONS.map(str::to_owned).to_vec(),
            main_fields: ["typings", "types", "main"].map(str::to_owned).to_vec(),
            node_path: false,
            symlinks: false,
            ..ResolveOptions::default()
        };

        Ok(Self {
            inner: ResolverGeneric::new_with_file_system(file_system, options),
            root,
        })
    }

    /// The tracked file `specifier` names when it is imported from the tracked file `importer`.
    ///
    /// # Errors
    /// Fails when the tsconfig or a `package.json` on the way cannot be read or parsed.
    pub(crate) fn resolve(&self, importer: &str, specifier: &str) -> Result<Option<String>> {
        let directory = self
            .root
            .join(importer)
            .parent()
            .map_or_else(|| self.root.clone(), Path::to_path_buf);
        // A declaration specifier names its source file first; an exact `paths`, `imports` or
        // `exports` key that ends in the declaration extension still matches as written.
        let source = DECLARATION_EXTENSIONS
            .iter()
            .find_map(|(declaration, source)| {
                specifier
                    .strip_suffix(declaration)
                    .map(|stem| format!("{stem}{source}"))
            });

        if let Some(source) = source
            && let Some(resolved) = self.resolve_from(&directory, importer, &source)?
        {
            return Ok(Some(resolved));
        }

        self.resolve_from(&directory, importer, specifier)
    }

    fn resolve_from(
        &self,
        directory: &Path,
        importer: &str,
        specifier: &str,
    ) -> Result<Option<String>> {
        match self.inner.resolve(directory, specifier) {
            // A `paths` target keeps the `..` of its substitution, which the compiler normalizes.
            Ok(resolution) => Ok(normalize(resolution.path())
                .strip_prefix(&self.root)
                .ok()
                .and_then(relative_path)
                .filter(|path| !path.split('/').any(|segment| segment == "node_modules"))),
            Err(
                error @ (ResolveError::TsconfigNotFound(_)
                | ResolveError::TsconfigSelfReference(_)
                | ResolveError::TsconfigCircularExtend(_)
                | ResolveError::TsconfigLoadFailed { .. }
                | ResolveError::IOError(_)
                | ResolveError::Json(_)),
            ) => Err(Error::new(format!(
                "cannot resolve {specifier:?} from {importer}: {}",
                error.to_string().replace(&format!("{ROOT}/"), "")
            ))),
            Err(_) => Ok(None),
        }
    }
}

/// A slash-separated relative path, when every component is a normal name.
fn relative_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();

    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            _ => return None,
        }
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The tracked files of a source as the resolver sees them: every tracked path exists, every
/// ancestor directory of one exists, and the configuration files the resolver reads (each
/// `package.json`, `tsconfig*.json` and `jsconfig*.json`) have their tracked contents.
#[derive(Debug, Default)]
struct SourceFileSystem {
    files: HashSet<PathBuf>,
    directories: HashSet<PathBuf>,
    contents: HashMap<PathBuf, Vec<u8>>,
}

impl SourceFileSystem {
    fn load(source: &dyn Source, root: &Path) -> Result<Self> {
        let mut file_system = Self::default();
        file_system.directories.insert(root.to_path_buf());

        for path in source.paths() {
            let full = root.join(path);
            if is_configuration(path)
                && let Some(bytes) = source.bytes(path)?
            {
                file_system.contents.insert(full.clone(), bytes.to_vec());
            }

            let mut directory = full.parent();

            while let Some(parent) = directory {
                if !parent.starts_with(root) || !file_system.directories.insert(parent.into()) {
                    break;
                }
                directory = parent.parent();
            }

            file_system.files.insert(full);
        }

        Ok(file_system)
    }

    fn metadata_of(&self, path: &Path) -> io::Result<FileMetadata> {
        let path = normalize(path);

        if self.files.contains(&path) {
            Ok(FileMetadata::new(true, false, false))
        } else if self.directories.contains(&path) {
            Ok(FileMetadata::new(false, true, false))
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }
}

/// Whether the resolver reads `path`: a `package.json`, `tsconfig*.json` or `jsconfig*.json`.
fn is_configuration(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);

    name == "package.json"
        || ((name.starts_with("tsconfig") || name.starts_with("jsconfig"))
            && has_extension(name, &["json"]))
}

/// `path` with `.` removed and `..` applied, without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }

    normalized
}

impl FileSystem for SourceFileSystem {
    fn new() -> Self {
        Self::default()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = normalize(path);

        self.contents.get(&path).cloned().ok_or_else(|| {
            if self.files.contains(&path) {
                io::Error::other(format!(
                    "{} is tracked but is not a configuration file the resolver reads",
                    path.display()
                ))
            } else {
                io::ErrorKind::NotFound.into()
            }
        })
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read(path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.metadata_of(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        self.metadata_of(path)
    }

    fn read_link(&self, path: &Path) -> std::result::Result<PathBuf, ResolveError> {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a symbolic link", path.display()),
        )
        .into())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.metadata_of(path).map(|_| normalize(path))
    }
}
