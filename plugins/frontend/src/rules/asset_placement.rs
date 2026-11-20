//! `frontend/asset-placement`: an image, icon, font, audio or video file under `src/` sits in the
//! `ui` segment of what uses it (a slice's, or `shared/ui`); files served by a fixed URL sit in
//! `public/`, which the rule does not read, and in Next.js the root `app/` holds only its metadata
//! images.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{FileName, NEXT_APP, SOURCE_ROOT, Unit, folder_of, name_of, within};

/// The rule.
#[derive(Debug, Default)]
pub struct AssetPlacement;

/// The options of `frontend/asset-placement`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The extensions of asset files, without the dot, compared without case.
    pub extensions: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            extensions: [
                "png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "woff", "woff2", "ttf",
                "otf", "mp3", "wav", "mp4", "webm",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

/// The stems of the Next.js metadata image files, which Next.js reads from its app folder.
const NEXT_METADATA_IMAGES: [&str; 5] = [
    "apple-icon",
    "favicon",
    "icon",
    "opengraph-image",
    "twitter-image",
];

impl Rule<TypeScriptCall> for AssetPlacement {
    type Options = Options;

    const NAME: &'static str = "asset-placement";
    const TITLE: &'static str = "Assets the code imports sit in the ui segment that uses them";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let pack = project.pack;
            let extensions = &run.options().extensions;
            let found: Vec<Found> = project
                .paths
                .iter()
                .copied()
                .filter(|path| {
                    let file = FileName::new(name_of(path));
                    extensions
                        .iter()
                        .any(|extension| extension.eq_ignore_ascii_case(file.extension))
                        && misplaced(&project, path, file.stem)
                })
                .map(|path| {
                    Found::at(
                        "",
                        path,
                        format!("asset {path} sits outside a ui segment and public/"),
                        "move it to the ui segment of the slice that imports it, to src/shared/ui/ when several slices share it, or to public/ when it is served by a fixed URL",
                    )
                })
                .collect();
            report(run, found);
            Ok(())
        })
    }
}

/// Whether the asset `path` is where no asset belongs: under `src/` outside a `ui` segment, or in
/// the root `app/` of Next.js other than a metadata image.
fn misplaced(project: &Project<'_, '_>, path: &str, stem: &str) -> bool {
    if project.pack == Pack::Nextjs && within(path, NEXT_APP) {
        return !next_metadata(path, stem);
    }
    within(path, SOURCE_ROOT)
        && !project
            .place(path)
            .is_some_and(|place| place.segment == Some("ui") && place.unit != Unit::App)
}

/// Whether `path`, in the root `app/` of Next.js, is a metadata image: `icon.png`, `icon1.png`,
/// `opengraph-image.jpg` and the like anywhere in it, `favicon.ico` in `app/` itself.
fn next_metadata(path: &str, stem: &str) -> bool {
    let base = stem.trim_end_matches(|character: char| character.is_ascii_digit());
    if base == "favicon" {
        return folder_of(path) == NEXT_APP;
    }
    NEXT_METADATA_IMAGES.contains(&base)
}
