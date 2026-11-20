//! The standard as paths: the source root, its layers, their slices, groups and segments, the files
//! a pack allows in the source root, the route folder of each pack, and the forms of a name.

use std::collections::BTreeSet;

use crate::options::Pack;

/// The source root, relative to the project root.
pub const SOURCE_ROOT: &str = "src";

/// A layer of the source root. The order is the rank: a layer imports only layers below it.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Layer {
    /// What knows no business: one slice of segments.
    Shared,
    /// Business objects, one slice per object.
    Entities,
    /// User actions with value, one slice per action.
    Features,
    /// Self-contained blocks of a page, one slice per block.
    Widgets,
    /// The pages, one slice per page.
    Pages,
    /// The app: one slice of segments.
    App,
}

impl Layer {
    /// The layers of an app, from the top.
    pub const ALL: [Self; 6] = [
        Self::App,
        Self::Pages,
        Self::Widgets,
        Self::Features,
        Self::Entities,
        Self::Shared,
    ];

    /// The layer's folder name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Entities => "entities",
            Self::Features => "features",
            Self::Widgets => "widgets",
            Self::Pages => "pages",
            Self::App => "app",
        }
    }

    /// The layer a folder of the source root names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|layer| layer.name() == name)
    }

    /// Whether the layer holds slices.
    #[must_use]
    pub fn sliced(self) -> bool {
        matches!(
            self,
            Self::Pages | Self::Widgets | Self::Features | Self::Entities
        )
    }

    /// Whether a folder of the layer may group slices.
    #[must_use]
    pub fn grouped(self) -> bool {
        matches!(self, Self::Widgets | Self::Features | Self::Entities)
    }
}

/// The segments of a slice, which are also the names no folder inside a segment may take (Steiger's
/// reserved folder names).
pub const SLICE_SEGMENTS: [&str; 5] = ["ui", "model", "api", "lib", "config"];

/// The segments of an app's shared layer.
pub const SHARED_SEGMENTS: [&str; 6] = ["api", "ui", "lib", "config", "i18n", "routes"];

/// The shared segments reached one level down (`@/shared/ui/button`), which need no `index.ts`.
pub const OPEN_SEGMENTS: [&str; 2] = ["ui", "lib"];

/// The segments of a library, its source root being one shared layer.
pub const LIBRARY_SEGMENTS: [&str; 6] = ["api", "config", "i18n", "lib", "model", "ui"];

/// The name of the folder of an entity that holds what it offers another entity.
pub const CROSS_IMPORT: &str = "@x";

/// The dotted parts a file name may carry between its name and its extension.
pub const DOTTED_PARTS: [&str; 4] = ["test", "stories", "module", "d"];

/// The extensions of stylesheets.
pub const STYLE_EXTENSIONS: [&str; 4] = ["css", "scss", "sass", "less"];

/// The extensions of JavaScript sources, which the standard does not allow under the source root.
pub const JAVASCRIPT_EXTENSIONS: [&str; 4] = ["js", "jsx", "mjs", "cjs"];

/// The route file of `TanStack` Router's generator in a Vite app: its one dotted part, `gen`, is the
/// generator's.
pub const VITE_ROUTE_TREE: &str = "src/app/route-tree.gen.ts";

/// The App Router folder of a Next.js app, at the project root.
pub const NEXT_APP: &str = "app";

/// The file that keeps Next.js from reading `src/pages` as the Pages Router.
pub const NEXT_PAGES_README: &str = "pages/README.md";

impl Pack {
    /// The files the pack allows directly in the source root, by name.
    #[must_use]
    pub fn root_files(self) -> &'static [&'static str] {
        match self {
            Self::ReactVite => &["main.tsx", "vite-env.d.ts"],
            Self::Nextjs => &[],
            Self::ReactLibrary => &["index.ts"],
        }
    }

    /// The segments of the app layer.
    #[must_use]
    pub fn app_segments(self) -> &'static [&'static str] {
        match self {
            Self::ReactVite => &[
                "entrypoint",
                "providers",
                "store",
                "styles",
                "routes",
                "config",
                "lib",
            ],
            Self::Nextjs | Self::ReactLibrary => &[
                "entrypoint",
                "providers",
                "store",
                "styles",
                "config",
                "lib",
            ],
        }
    }

    /// The segments of the shared layer.
    #[must_use]
    pub fn shared_segments(self) -> &'static [&'static str] {
        if self.library() {
            &LIBRARY_SEGMENTS
        } else {
            &SHARED_SEGMENTS
        }
    }

    /// The folder of the route files: `src/app/routes` for Vite, the root `app` for Next.js, none
    /// for a library.
    #[must_use]
    pub fn route_folder(self) -> Option<&'static str> {
        match self {
            Self::ReactVite => Some("src/app/routes"),
            Self::Nextjs => Some(NEXT_APP),
            Self::ReactLibrary => None,
        }
    }

    /// The dotted parts a file name may carry: `.server` too in Next.js (`index.server.ts`).
    #[must_use]
    pub fn dotted_parts(self) -> &'static [&'static str] {
        match self {
            Self::Nextjs => &["test", "stories", "module", "d", "server"],
            Self::ReactVite | Self::ReactLibrary => &DOTTED_PARTS,
        }
    }

    /// The names of a slice's public API files.
    #[must_use]
    pub fn public_files(self) -> &'static [&'static str] {
        match self {
            Self::Nextjs => &["index.ts", "index.server.ts"],
            Self::ReactVite | Self::ReactLibrary => &["index.ts"],
        }
    }

    /// The folder of the network client.
    #[must_use]
    pub fn api_folder(self) -> &'static str {
        if self.library() {
            "src/api"
        } else {
            "src/shared/api"
        }
    }

    /// The folder of the translation messages, one `<locale>.json` per locale.
    #[must_use]
    pub fn messages_folder(self) -> &'static str {
        if self.library() {
            "src/i18n/messages"
        } else {
            "src/shared/i18n/messages"
        }
    }

    /// The folder that holds the one global stylesheet: the app layer's `styles` segment, or a
    /// library's `ui` segment.
    #[must_use]
    pub fn global_styles_folder(self) -> &'static str {
        if self.library() {
            "src/ui"
        } else {
            "src/app/styles"
        }
    }
}

/// The slice groups of a project: each a folder of `widgets/`, `features/` or `entities/` that holds
/// folders only, none of them a segment or `@x`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Groups {
    folders: BTreeSet<String>,
}

impl Groups {
    /// The groups among `paths`, for `pack`; a library has none.
    #[must_use]
    pub fn new<'p>(pack: Pack, paths: impl IntoIterator<Item = &'p str>) -> Self {
        let mut candidates = BTreeSet::new();
        let mut slices = BTreeSet::new();
        if pack.library() {
            return Self::default();
        }
        for path in paths {
            let parts: Vec<&str> = path.split('/').collect();
            let [SOURCE_ROOT, layer, unit, child, ..] = parts.as_slice() else {
                continue;
            };
            if Layer::from_name(layer).is_none_or(|found| !found.grouped()) {
                continue;
            }
            let folder = format!("{SOURCE_ROOT}/{layer}/{unit}");
            if parts.len() == 4 || SLICE_SEGMENTS.contains(child) || *child == CROSS_IMPORT {
                slices.insert(folder);
            } else {
                candidates.insert(folder);
            }
        }
        Self {
            folders: candidates.difference(&slices).cloned().collect(),
        }
    }

    /// Whether `folder` is a group.
    #[must_use]
    pub fn contains(&self, folder: &str) -> bool {
        self.folders.contains(folder)
    }
}

/// The unit of the standard a file under the source root belongs to: an import inside it is
/// relative, an import out of it names its target from the source root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unit<'p> {
    /// A file directly in the source root.
    Root,
    /// The app layer, one slice.
    App,
    /// The shared layer (a library's whole source root), one slice.
    Shared,
    /// A slice of a sliced layer, named by its folder below the layer, its group included
    /// (`auth/sign-in`).
    Slice(Layer, &'p str),
    /// No unit of the standard: a file directly in a layer or a group, or in a folder that is no
    /// layer.
    None,
}

/// Where a file under the source root sits in the standard tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Place<'p> {
    /// The layer; `None` for a file directly in the source root or in a folder that is no layer.
    pub layer: Option<Layer>,
    /// The unit.
    pub unit: Unit<'p>,
    /// The segment the file is in, of its slice, the app or the shared layer; `None` for a file in
    /// a slice's root.
    pub segment: Option<&'p str>,
}

/// Where `path` sits in the standard tree of `pack`; `None` when it is not below the source root.
#[must_use]
pub fn place<'p>(pack: Pack, groups: &Groups, path: &'p str) -> Option<Place<'p>> {
    let rest = path.strip_prefix("src/")?;
    let parts: Vec<&'p str> = rest.split('/').collect();
    let none = Place {
        layer: None,
        unit: Unit::None,
        segment: None,
    };
    let segmented = |layer, unit, at: usize| Place {
        layer: Some(layer),
        unit,
        segment: parts.get(at).copied().filter(|_| parts.len() > at + 1),
    };

    if parts.len() == 1 {
        return Some(Place {
            unit: Unit::Root,
            ..none
        });
    }
    if pack.library() {
        return Some(segmented(Layer::Shared, Unit::Shared, 0));
    }
    let Some(layer) = Layer::from_name(parts[0]) else {
        return Some(none);
    };

    Some(match layer {
        Layer::App if parts.len() > 2 => segmented(layer, Unit::App, 1),
        Layer::Shared if parts.len() > 2 => segmented(layer, Unit::Shared, 1),
        _ if layer.sliced() && parts.len() > 2 => {
            let grouped = groups.contains(&format!("{SOURCE_ROOT}/{}/{}", parts[0], parts[1]));
            if grouped && parts.len() < 4 {
                return Some(Place {
                    layer: Some(layer),
                    ..none
                });
            }
            let depth = if grouped { 3 } else { 2 };
            let start = parts[0].len() + 1;
            let length = parts[1..depth]
                .iter()
                .map(|part| part.len() + 1)
                .sum::<usize>()
                - 1;
            segmented(
                layer,
                Unit::Slice(layer, &rest[start..start + length]),
                depth,
            )
        }
        _ => Place {
            layer: Some(layer),
            ..none
        },
    })
}

/// Whether `name` is kebab-case: lowercase letters and digits in words joined by single hyphens.
#[must_use]
pub fn kebab(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Whether `name` is a camel-case identifier, the form of a route parameter: `id`, `userId`.
#[must_use]
pub fn camel(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

/// The kebab-case form of a camel-case name: `useBalance` is `use-balance`.
#[must_use]
pub fn kebab_of(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                out.push('-');
            }
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// The `PascalCase` form of a kebab-case name: `user-card` is `UserCard`.
#[must_use]
pub fn pascal_of(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + characters.as_str()
            })
        })
        .collect()
}

/// A file name in its parts: the stem, the dotted parts and the extension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileName<'n> {
    /// The part before the first dot.
    pub stem: &'n str,
    /// The parts between the stem and the extension.
    pub dotted: Vec<&'n str>,
    /// The part after the last dot; empty for a name without one.
    pub extension: &'n str,
}

impl<'n> FileName<'n> {
    /// The parts of `name`, the last segment of a path.
    #[must_use]
    pub fn new(name: &'n str) -> Self {
        let mut parts: Vec<&str> = name.split('.').collect();
        let stem = parts.remove(0);
        let extension = if parts.is_empty() {
            ""
        } else {
            parts.remove(parts.len() - 1)
        };
        Self {
            stem,
            dotted: parts,
            extension,
        }
    }

    /// Whether the file is a test or a story.
    #[must_use]
    pub fn test_or_story(&self) -> bool {
        self.dotted
            .iter()
            .any(|part| *part == "test" || *part == "stories")
    }

    /// Whether the file is a CSS module.
    #[must_use]
    pub fn module(&self) -> bool {
        self.dotted.contains(&"module")
    }
}

/// The last segment of `path`.
#[must_use]
pub fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The folder of `path`; empty for a path without one.
#[must_use]
pub fn folder_of(path: &str) -> &str {
    path.rfind('/').map_or("", |slash| &path[..slash])
}

/// Whether `path` is `folder` or below it.
#[must_use]
pub fn within(path: &str, folder: &str) -> bool {
    path == folder
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The extension of `path`, lowercase compared by the caller.
#[must_use]
pub fn extension_of(path: &str) -> &str {
    FileName::new(name_of(path)).extension
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_finds_its_layer_unit_and_segment() {
        let app = Pack::Nextjs;
        let paths = [
            "src/features/auth/sign-in/index.ts",
            "src/features/auth/sign-in/ui/form.tsx",
            "src/features/sign-out/index.ts",
            "src/features/sign-out/model/x.ts",
            "src/entities/user/@x/invoice.ts",
        ];
        let groups = Groups::new(app, paths);
        assert!(groups.contains("src/features/auth"));
        assert!(!groups.contains("src/features/sign-out"));
        assert!(!groups.contains("src/entities/user"));
        let at = |path| place(app, &groups, path);

        assert_eq!(
            at("src/features/auth/sign-in/ui/form.tsx"),
            Some(Place {
                layer: Some(Layer::Features),
                unit: Unit::Slice(Layer::Features, "auth/sign-in"),
                segment: Some("ui"),
            })
        );
        assert_eq!(
            at("src/features/sign-out/index.ts"),
            Some(Place {
                layer: Some(Layer::Features),
                unit: Unit::Slice(Layer::Features, "sign-out"),
                segment: None,
            })
        );
        assert_eq!(
            at("src/pages/home/ui/page.tsx").map(|place| place.unit),
            Some(Unit::Slice(Layer::Pages, "home"))
        );
        assert_eq!(
            at("src/shared/api/client.ts"),
            Some(Place {
                layer: Some(Layer::Shared),
                unit: Unit::Shared,
                segment: Some("api"),
            })
        );
        assert_eq!(
            at("src/app/providers/query.tsx").map(|place| (place.unit, place.segment)),
            Some((Unit::App, Some("providers")))
        );
        assert_eq!(at("src/main.tsx").map(|place| place.unit), Some(Unit::Root));
        assert_eq!(
            at("src/modules/x.ts").map(|place| place.unit),
            Some(Unit::None)
        );
        assert_eq!(at("app/page.tsx"), None);
        assert_eq!(
            place(
                Pack::ReactLibrary,
                &Groups::default(),
                "src/ui/button/button.tsx"
            )
            .map(|place| (place.unit, place.segment)),
            Some((Unit::Shared, Some("ui")))
        );
    }

    #[test]
    fn names_convert_between_forms() {
        assert!(kebab("user-card") && kebab("404") && !kebab("UserCard") && !kebab("a--b"));
        assert!(camel("userId") && !camel("UserId") && !camel("user-id"));
        assert_eq!(kebab_of("useBalance"), "use-balance");
        assert_eq!(pascal_of("user-card"), "UserCard");
        assert_eq!(
            FileName::new("user-card.module.css"),
            FileName {
                stem: "user-card",
                dotted: vec!["module"],
                extension: "css",
            }
        );
    }
}
