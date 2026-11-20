//! The standard as paths: the source root, its layers, their slices and segments, the files a pack
//! allows in the source root, the route folder of each pack, and the forms of a name.

use crate::options::Pack;

/// The source root, relative to the project root.
pub const SOURCE_ROOT: &str = "src";

/// A layer of the source root. The order is the rank: a layer imports only layers below it.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Layer {
    /// What knows no business: segments only.
    Shared,
    /// Business objects, one slice per object.
    Entities,
    /// User actions with value, one slice per action.
    Features,
    /// Self-contained blocks of a page, one slice per block.
    Widgets,
    /// The application: entry, providers, global styles and the routes.
    App,
}

impl Layer {
    /// The layers of an app, from the top.
    pub const ALL: [Self; 5] = [
        Self::App,
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
        matches!(self, Self::Widgets | Self::Features | Self::Entities)
    }
}

/// The segments of a slice.
pub const SLICE_SEGMENTS: [&str; 5] = ["ui", "model", "api", "lib", "config"];

/// The segments of an app's shared layer.
pub const SHARED_SEGMENTS: [&str; 6] = ["api", "assets", "config", "i18n", "lib", "ui"];

/// The segments of a library, its source root being one shared layer.
pub const LIBRARY_SEGMENTS: [&str; 7] = ["api", "assets", "config", "i18n", "lib", "model", "ui"];

/// The dotted parts a file name may carry between its name and its extension.
pub const DOTTED_PARTS: [&str; 4] = ["test", "stories", "module", "d"];

/// The extensions of stylesheets.
pub const STYLE_EXTENSIONS: [&str; 4] = ["css", "scss", "sass", "less"];

/// The extensions of JavaScript sources, which the standard does not allow under the source root.
pub const JAVASCRIPT_EXTENSIONS: [&str; 4] = ["js", "jsx", "mjs", "cjs"];

/// The route file of TanStack Router's generator in a Vite app: its one dotted part, `gen`, is the
/// generator's.
pub const VITE_ROUTE_TREE: &str = "src/app/route-tree.gen.ts";

impl Pack {
    /// The files the pack allows directly in the source root, by name.
    #[must_use]
    pub fn root_files(self) -> &'static [&'static str] {
        match self {
            Self::ReactVite => &["main.tsx", "vite-env.d.ts"],
            Self::Nextjs => &[
                "instrumentation-client.ts",
                "instrumentation.ts",
                "middleware.ts",
                "proxy.ts",
            ],
            Self::ReactLibrary => &["index.ts"],
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

    /// The folder of the shared layer: `src/shared`, or a library's `src`.
    #[must_use]
    pub fn shared_root(self) -> &'static str {
        if self.library() {
            SOURCE_ROOT
        } else {
            "src/shared"
        }
    }

    /// The folder of the route files: `src/app/routes` for Vite, `src/app` for Next.js, none for
    /// a library.
    #[must_use]
    pub fn route_folder(self) -> Option<&'static str> {
        match self {
            Self::ReactVite => Some("src/app/routes"),
            Self::Nextjs => Some("src/app"),
            Self::ReactLibrary => None,
        }
    }

    /// The folder of the assets the code imports.
    #[must_use]
    pub fn assets_folder(self) -> &'static str {
        if self.library() {
            "src/assets"
        } else {
            "src/shared/assets"
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

    /// The folder that holds the global stylesheets: the app layer, or a library's assets.
    #[must_use]
    pub fn global_styles_folder(self) -> &'static str {
        if self.library() {
            "src/assets"
        } else {
            "src/app"
        }
    }
}

/// The unit of the standard a file under the source root belongs to: an import inside it is
/// relative, an import out of it names its target from the source root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unit<'p> {
    /// A file directly in the source root.
    Root,
    /// The app layer.
    App,
    /// A slice of a sliced layer.
    Slice(Layer, &'p str),
    /// A segment of the shared layer (a library's `src/<segment>`).
    Segment(&'p str),
    /// No unit of the standard: a file directly in a layer, or in a folder that is no layer.
    None,
}

/// Where a file under the source root sits in the standard tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Place<'p> {
    /// The layer; `None` for a file directly in the source root or in a folder that is no layer.
    pub layer: Option<Layer>,
    /// The unit.
    pub unit: Unit<'p>,
    /// The segment the file is in: of its slice, of the shared layer, or none.
    pub segment: Option<&'p str>,
}

/// Where `path` sits in the standard tree of `pack`; `None` when it is not below the source root.
#[must_use]
pub fn place(pack: Pack, path: &str) -> Option<Place<'_>> {
    let rest = path.strip_prefix("src/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    let none = Place {
        layer: None,
        unit: Unit::None,
        segment: None,
    };

    if parts.len() == 1 {
        return Some(Place {
            unit: Unit::Root,
            ..none
        });
    }

    if pack.library() {
        return Some(Place {
            layer: Some(Layer::Shared),
            unit: Unit::Segment(parts[0]),
            segment: Some(parts[0]),
        });
    }

    let Some(layer) = Layer::from_name(parts[0]) else {
        return Some(none);
    };

    Some(match layer {
        Layer::App => Place {
            layer: Some(layer),
            unit: Unit::App,
            segment: None,
        },
        Layer::Shared if parts.len() > 2 => Place {
            layer: Some(layer),
            unit: Unit::Segment(parts[1]),
            segment: Some(parts[1]),
        },
        _ if layer.sliced() && parts.len() > 2 => Place {
            layer: Some(layer),
            unit: Unit::Slice(layer, parts[1]),
            segment: (parts.len() > 3).then_some(parts[2]),
        },
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
        assert_eq!(
            place(app, "src/features/sign-in/ui/form.tsx"),
            Some(Place {
                layer: Some(Layer::Features),
                unit: Unit::Slice(Layer::Features, "sign-in"),
                segment: Some("ui"),
            })
        );
        assert_eq!(
            place(app, "src/features/sign-in/index.ts").map(|place| place.segment),
            Some(None)
        );
        assert_eq!(
            place(app, "src/shared/api/client.ts").map(|place| place.unit),
            Some(Unit::Segment("api"))
        );
        assert_eq!(
            place(app, "src/main.tsx").map(|place| place.unit),
            Some(Unit::Root)
        );
        assert_eq!(
            place(app, "src/modules/x.ts").map(|place| place.unit),
            Some(Unit::None)
        );
        assert_eq!(place(app, "lib/x.ts"), None);
        assert_eq!(
            place(Pack::ReactLibrary, "src/ui/button/button.tsx").map(|place| place.unit),
            Some(Unit::Segment("ui"))
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
