//! The tree as `nestjs/kind-imports` reads it: which specifier names which file, the kind folders,
//! the resource of each folder, and the module the layout gives each name a file loads.

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use gate_typescript::TypeScript;
use gate_typescript::exports::{ExportMap, Exports};

use crate::layout::Layout;
use crate::shared::modules::{source_target, stem};

/// The folder of a path; empty at the project root.
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// The last segment of a path.
pub fn name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// Whether `path` lies below `folder`; every path lies below the project root (`""`).
pub fn below(folder: &str, path: &str) -> bool {
    folder.is_empty()
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The folders above `path` and below `root`, nearest first, without `root`.
fn ancestors<'p>(root: &str, path: &'p str) -> impl Iterator<Item = &'p str> {
    let root = root.to_owned();
    std::iter::successors(Some(parent(path)), |folder| {
        (!folder.is_empty()).then(|| parent(folder))
    })
    .take_while(move |folder| *folder != root && below(&root, folder))
}

/// The module the layout gives one name a load takes, and how a message words it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Target {
    /// The declaring file: it sits in no kind folder, the importer sits in its kind folder (`own`),
    /// or no kind barrel exports it.
    File { file: String, own: bool },
    /// The barrel of the `internal` child of a kind folder.
    Internal { barrel: String, folder: String },
    /// The kind barrel, which exports the name.
    KindBarrel { barrel: String, folder: String },
    /// The kind barrel, which leaves the name out.
    LeftOut { barrel: String, folder: String },
}

impl Target {
    /// The module the name is loaded from.
    pub fn module(&self) -> &str {
        match self {
            Self::File { file, .. } => file,
            Self::Internal { barrel, .. }
            | Self::KindBarrel { barrel, .. }
            | Self::LeftOut { barrel, .. } => barrel,
        }
    }

    /// Whether a load of `module` is in the form for the name.
    pub fn holds(&self, module: &str) -> bool {
        !matches!(self, Self::LeftOut { .. }) && self.module() == module
    }

    /// How a message says where to import the name from.
    pub fn wording(&self) -> String {
        match self {
            Self::File { file, .. } => format!("by its file {file}"),
            Self::Internal { folder, .. } => format!("through the internal barrel {folder}"),
            Self::KindBarrel { folder, .. } => format!("through the kind barrel {folder}"),
            Self::LeftOut { folder, .. } => {
                format!("through the kind barrel {folder} once it exports them")
            }
        }
    }
}

/// The rule's reading of the tree.
pub struct Model<'c, 'a> {
    pub layout: &'c Layout,
    pub barrel_name: &'c str,
    pub internal_folder: &'c str,
    /// `<source-root>/`, or empty when the source root is the project root.
    source_prefix: String,
    /// The tracked `.ts` files below the source root.
    sources: HashSet<&'a str>,
    /// The kind folders.
    kind_folders: HashSet<String>,
    /// The folders that hold a kind folder somewhere below them.
    above_kind: HashSet<String>,
    /// The module folders: `<modules-root>/<plane>/<name>`.
    planes: Vec<String>,
    pub exports: ExportMap<'c, 'a>,
    /// The files some barrel's `export … from` resolves to.
    reexported: OnceCell<HashSet<String>>,
}

impl<'c, 'a> Model<'c, 'a> {
    /// Reads the kind folders of `kinds` from the tracked files below the source root.
    pub fn new(
        typescript: &'c TypeScript<'a>,
        layout: &'c Layout,
        kinds: &[String],
        barrel_name: &'c str,
        internal_folder: &'c str,
        sources: HashSet<&'a str>,
    ) -> Self {
        let source_prefix = layout.source_prefix();
        let source_root = layout.source_root.trim_end_matches('/').to_owned();
        let structural: HashSet<String> = std::iter::once(source_root.clone())
            .chain(
                layout
                    .source_owners
                    .iter()
                    .map(|owner| format!("{source_prefix}{owner}")),
            )
            .chain(layout.module_planes.iter().map(|plane| {
                format!(
                    "{}{plane}",
                    crate::layout::folder_prefix(&layout.modules_root)
                )
            }))
            .collect();
        let planes = layout
            .module_planes
            .iter()
            .map(|plane| {
                format!(
                    "{}{plane}",
                    crate::layout::folder_prefix(&layout.modules_root)
                )
            })
            .collect();

        // For each folder named like a kind: whether it holds a file other than a barrel, and
        // whether every such file carries the kind.
        let mut named: BTreeMap<&str, (bool, bool)> = BTreeMap::new();
        for path in &sources {
            let file = name(path);
            if file == barrel_name {
                continue;
            }
            let base = file
                .strip_suffix(".d.ts")
                .or_else(|| file.strip_suffix(".ts"))
                .unwrap_or(file);
            for folder in ancestors(&source_root, path) {
                let kind = name(folder);
                if kinds.iter().any(|known| known == kind) {
                    let carries = base == kind
                        || base
                            .strip_suffix(kind)
                            .is_some_and(|rest| rest.ends_with('.'));
                    let entry = named.entry(folder).or_insert((false, true));
                    entry.0 = true;
                    entry.1 &= carries;
                }
            }
        }
        let candidates: HashSet<&str> = named
            .iter()
            .filter(|(folder, (holds, carries))| {
                *holds && *carries && !structural.contains(**folder)
            })
            .map(|(folder, _)| *folder)
            .collect();
        let kind_folders: HashSet<String> = candidates
            .iter()
            .filter(|folder| {
                !ancestors(&source_root, folder).any(|above| candidates.contains(above))
            })
            .map(|folder| (*folder).to_owned())
            .collect();
        let above_kind = kind_folders
            .iter()
            .flat_map(|folder| ancestors(&source_root, folder).map(str::to_owned))
            .collect();

        let resolver_sources = sources.clone();
        let resolver_layout = layout;
        let resolver_prefix = source_prefix.clone();
        let exports = ExportMap::new(typescript, move |importer, specifier| {
            Ok(resolve_in(
                resolver_layout,
                &resolver_prefix,
                &resolver_sources,
                importer,
                specifier,
            ))
        });

        Self {
            layout,
            barrel_name,
            internal_folder,
            source_prefix,
            sources,
            kind_folders,
            above_kind,
            planes,
            exports,
            reexported: OnceCell::new(),
        }
    }

    /// Whether `path` lies below the source root.
    pub fn in_source(&self, path: &str) -> bool {
        self.source_prefix.is_empty() || path.starts_with(&self.source_prefix)
    }

    /// The tracked file below the source root that `specifier` names from `importer`.
    pub fn resolve(&self, importer: &str, specifier: &str) -> Option<String> {
        resolve_in(
            self.layout,
            &self.source_prefix,
            &self.sources,
            importer,
            specifier,
        )
    }

    /// Whether `path` is a barrel.
    pub fn is_barrel(&self, path: &str) -> bool {
        name(path) == self.barrel_name
    }

    /// The kind folder that holds `path`, at any depth.
    pub fn kind_folder<'p>(&self, path: &'p str) -> Option<&'p str> {
        std::iter::successors(Some(parent(path)), |folder| {
            (!folder.is_empty()).then(|| parent(folder))
        })
        .find(|folder| self.kind_folders.contains(*folder))
    }

    /// The barrel of `folder`, when it is tracked.
    fn barrel(&self, folder: &str) -> Option<String> {
        let barrel = format!("{folder}/{}", self.barrel_name);
        self.sources.contains(barrel.as_str()).then_some(barrel)
    }

    /// Whether `path` is the barrel of a kind folder.
    pub fn is_kind_barrel(&self, path: &str) -> bool {
        self.is_barrel(path) && self.kind_folders.contains(parent(path))
    }

    /// Whether `path` is the barrel of the `internal` child of a kind folder.
    pub fn is_internal_barrel(&self, path: &str) -> bool {
        let folder = parent(path);
        self.is_barrel(path)
            && name(folder) == self.internal_folder
            && self.kind_folders.contains(parent(folder))
    }

    /// Whether `path`, not a barrel, is a file no barrel's `export … from` resolves to.
    ///
    /// # Errors
    /// When a barrel cannot be read.
    pub fn is_private(&self, path: &str) -> gate_sdk::Result<bool> {
        if self.reexported.get().is_none() {
            let mut reexported = HashSet::new();
            let mut barrels: Vec<&str> = self
                .sources
                .iter()
                .copied()
                .filter(|source| self.is_barrel(source))
                .collect();
            barrels.sort_unstable();
            for barrel in barrels {
                reexported.extend(self.exports.own(barrel)?.reexported.iter().cloned());
            }
            let _ = self.reexported.set(reexported);
        }
        Ok(!self.is_barrel(path)
            && self
                .reexported
                .get()
                .is_some_and(|reexported| !reexported.contains(path)))
    }

    /// The resource of `folder`, a folder below the source root.
    pub fn resource(&self, folder: &str) -> String {
        if let Some(kind) = self.kind_folder(&format!("{folder}/_")) {
            return parent(kind).to_owned();
        }
        let root = self.layout.source_root.trim_end_matches('/');
        let modules_root = self.layout.modules_root.trim_end_matches('/');
        let nearest = std::iter::successors(Some(folder), |folder| {
            (!folder.is_empty()).then(|| parent(folder))
        })
        .take_while(|candidate| *candidate != root && below(root, candidate))
        .find(|candidate| {
            self.above_kind.contains(*candidate)
                && *candidate != modules_root
                && !self.planes.iter().any(|plane| plane == candidate)
        });
        if let Some(nearest) = nearest {
            return nearest.to_owned();
        }
        if let Some(plane) = self.planes.iter().find(|plane| below(plane, folder)) {
            let rest = &folder[plane.len() + 1..];
            let module = rest.split('/').next().unwrap_or_default();
            return format!("{plane}/{module}");
        }
        let below_root = folder.strip_prefix(&self.source_prefix).unwrap_or(folder);
        match below_root.split('/').next() {
            Some(owner) if !owner.is_empty() && folder != root => {
                format!("{}{owner}", self.source_prefix)
            }
            _ => root.to_owned(),
        }
    }

    /// The top-level owner folder of a path below the source root.
    pub fn owner<'p>(&self, path: &'p str) -> &'p str {
        let rest = path.strip_prefix(&self.source_prefix).unwrap_or_default();
        let owner = rest.split('/').next().unwrap_or_default();
        &path[..self.source_prefix.len() + owner.len()]
    }

    /// The module the layout gives the name `name` of `module`, declared in `declaring`, when
    /// `importer` loads it.
    ///
    /// # Errors
    /// When a barrel cannot be read.
    pub fn target(&self, importer: &str, name: &str, declaring: &str) -> gate_sdk::Result<Target> {
        let Some(kind) = self.kind_folder(declaring) else {
            return Ok(Target::File {
                file: declaring.to_owned(),
                own: false,
            });
        };
        if self.in_source(importer) && self.kind_folder(importer) == Some(kind) {
            return Ok(Target::File {
                file: declaring.to_owned(),
                own: true,
            });
        }
        let internal = format!("{kind}/{}", self.internal_folder);
        if below(&internal, declaring)
            && let Some(barrel) = self.barrel(&internal)
        {
            return Ok(Target::Internal {
                barrel,
                folder: internal,
            });
        }
        let Some(barrel) = self.barrel(kind) else {
            return Ok(Target::File {
                file: declaring.to_owned(),
                own: false,
            });
        };
        if self.is_private(declaring)? {
            return Ok(Target::File {
                file: declaring.to_owned(),
                own: false,
            });
        }
        let exports = self.exports.exports(&barrel)?;
        let folder = kind.to_owned();
        if exports.names.get(name).and_then(Option::as_deref) == Some(declaring) {
            Ok(Target::KindBarrel { barrel, folder })
        } else {
            Ok(Target::LeftOut { barrel, folder })
        }
    }

    /// The names `module` exports and their declaring files.
    ///
    /// # Errors
    /// When a module cannot be read.
    pub fn module_exports(&self, module: &str) -> gate_sdk::Result<Rc<Exports>> {
        Ok(self.exports.exports(module)?)
    }
}

/// The tracked file below the source root that `specifier` names from `importer`: a relative path,
/// a path from the project root through the source root, or a `layout.path-aliases` alias, its
/// script extension dropped, tried as `<p>.ts`, `<p>.d.ts`, `<p>/index.ts` and `<p>/index.d.ts`.
fn resolve_in(
    layout: &Layout,
    source_prefix: &str,
    sources: &HashSet<&str>,
    importer: &str,
    specifier: &str,
) -> Option<String> {
    let target = source_target(layout, importer, specifier)?;
    let path = stem(target.trim_end_matches('/'));
    if path.starts_with("..") || path == "." {
        return None;
    }
    [
        format!("{path}.ts"),
        format!("{path}.d.ts"),
        format!("{path}/index.ts"),
        format!("{path}/index.d.ts"),
    ]
    .into_iter()
    .find(|candidate| {
        (source_prefix.is_empty() || candidate.starts_with(source_prefix))
            && sources.contains(candidate.as_str())
    })
}

/// The value loads of each file, resolved: the edges of a static closure.
pub type Edges = HashMap<String, Vec<String>>;

/// The modules reached from `file` through value loads, transitively.
pub fn closure(edges: &Edges, file: &str) -> HashSet<String> {
    let mut reached = HashSet::new();
    let mut pending: Vec<&str> = edges
        .get(file)
        .map(|next| next.iter().map(String::as_str).collect())
        .unwrap_or_default();
    while let Some(module) = pending.pop() {
        if reached.insert(module.to_owned())
            && let Some(next) = edges.get(module)
        {
            pending.extend(next.iter().map(String::as_str));
        }
    }
    reached
}
