//! `frontend/asset-placement`: an image, icon, font, audio or video file under `src/` sits in the
//! assets segment; files served by a fixed URL sit in `public/`, which the rule does not read.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::call::TypeScriptCall;

use crate::code::Project;
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};
use crate::standard::{FileName, folder_of, name_of, within};

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

/// The stems of the Next.js metadata image files, which Next.js reads from `src/app/`.
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
    const TITLE: &'static str = "Assets the code imports sit in the assets segment";
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, Options, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let pack = project.pack;
            let assets = pack.assets_folder();
            let extensions = &run.options().extensions;
            let found: Vec<Found> = project
                .sources()
                .filter(|path| {
                    let file = FileName::new(name_of(path));
                    extensions
                        .iter()
                        .any(|extension| extension.eq_ignore_ascii_case(file.extension))
                        && !within(path, assets)
                        && !next_metadata(pack, path, file.stem)
                })
                .map(|path| {
                    Found::at(
                        "",
                        path,
                        format!("asset {path} sits outside {assets}/"),
                        format!(
                            "move it to {assets}/ when the code imports it, or to public/ when it is served by a fixed URL"
                        ),
                    )
                })
                .collect();
            report(run, found);
            Ok(())
        })
    }
}

/// Whether `path` is a Next.js metadata image of the route folder: `icon.png`, `icon1.png`,
/// `opengraph-image.jpg` and the like anywhere in `src/app/`, `favicon.ico` in `src/app/` itself.
fn next_metadata(pack: Pack, path: &str, stem: &str) -> bool {
    if pack != Pack::Nextjs || !within(path, "src/app") {
        return false;
    }
    let base = stem.trim_end_matches(|character: char| character.is_ascii_digit());
    if base == "favicon" {
        return folder_of(path) == "src/app";
    }
    NEXT_METADATA_IMAGES.contains(&base)
}
