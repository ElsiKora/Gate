//! `nestjs/error-string-shim`: no enum below the source root takes the name of an installed Crud
//! Automator `ErrorString` action enum, which would shadow it with a local copy.

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Rule, Run};
use gate_typescript::oxc_ast::AstKind;

use crate::layout::PluginOptions;
use crate::shared::report::Findings;
use crate::shared::{project, tree};

/// The rule.
#[derive(Debug, Default)]
pub struct ErrorStringShim;

/// The options of `nestjs/error-string-shim`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The installed action enums no source enum may copy.
    pub enum_names: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enum_names: ["EErrorStringAction", "EErrorStringCompositeAction"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

impl Rule<crate::Nest> for ErrorStringShim {
    type Options = Options;

    const NAME: &'static str = "error-string-shim";
    const TITLE: &'static str = "No source enum copies an installed ErrorString action enum";
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options, crate::Nest>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let layout = PluginOptions::read(run)?.layout;
            let names = &run.options().enum_names;
            let source = shared.source();
            let typescript = shared.typescript()?;
            let mut findings = Findings::default();

            for path in tree::typescript_files(source, &layout.source_root, typescript.settings()) {
                if path.ends_with(".d.ts") {
                    continue;
                }
                let Some(file) = project::parsed(typescript, path)? else {
                    continue;
                };

                for node in file.semantic().nodes().iter() {
                    let AstKind::TSEnumDeclaration(declaration) = node.kind() else {
                        continue;
                    };
                    let name = declaration.id.name.as_str();

                    if names.iter().any(|known| known == name) {
                        findings.push_keyed(
                            "",
                            path,
                            Some(file.line(declaration.id.span.start)),
                            name,
                            format!(
                                "{path} declares {name}, a local copy of the installed Crud \
                                 Automator enum; use its installed members and record a missing \
                                 action as a follow-up"
                            ),
                        );
                    }
                }
            }

            findings.report(run, |_| "use the installed enum's members");
            Ok(())
        })
    }
}
