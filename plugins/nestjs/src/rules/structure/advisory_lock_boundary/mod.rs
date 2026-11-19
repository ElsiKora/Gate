//! `nestjs/advisory-lock-boundary`: advisory-lock SQL, keys and identifiers stay inside their
//! registered boundaries, across the JavaScript, TypeScript, SQL, JSON and shell files below
//! `roots` and the `standalone-files`.
//!
//! Each file is read in one order, and its first finding is reported:
//! 1. `inventory`: a registered family or resource identifier of the manifest stands only in the
//!    files `inventory` lists ([`source::registered_ids`]), except the identifiers of the
//!    `contextual-resources` ([`contextual`]).
//! 2. `mint`: the minting files keep their texts, and a structural key stands only where `mint`
//!    allows it ([`mint`]).
//! 3. The executable SQL of a JavaScript, TypeScript or SQL file ([`source::statements`]) stands in
//!    a boundary ([`boundary`]); a shell script, JSON file or package script runs no advisory SQL
//!    (`shell`).
//!
//! The executor file must be tracked, and each migration and the negative test must hold their
//! statement. A source that does not parse, or that is not UTF-8 text, is `unreadable`; a SQL, JSON
//! or shell file that is not UTF-8 text is read with replacement characters and reported
//! `unreadable` as well, besides its first finding. Baseline migrations need the `ledger` key,
//! which the manifest must hold.

mod boundary;
mod contextual;
mod mint;
mod source;
mod sql;

use crate::shared::paths::{self, Form};
use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::serde_json::{self, Value};
use gate_sdk::{Rule, Run, Tree};
use gate_typescript::{Allocator, Settings, TypeScript, parse_source};

use self::boundary::{Boundaries, Rejection, extraction_rejection, validate};
use self::contextual::ContextualResource;
use self::mint::Mint;
use self::source::{Extraction, SqlSinks};
use self::sql::{Text, units};
use crate::layout::{Layout, PluginOptions};
use crate::shared::project;
use crate::shared::report::Findings;

/// The extensions of the JavaScript and TypeScript sources read with a syntax tree.
const AST_EXTENSIONS: [&str; 8] = ["cjs", "cts", "js", "jsx", "mjs", "mts", "ts", "tsx"];
/// The extensions of the files whose shell commands are read.
const SHELL_EXTENSIONS: [&str; 2] = ["json", "sh"];

/// The options of `nestjs/advisory-lock-boundary`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The registry manifest.
    #[serde(default)]
    pub manifest_path: Option<String>,
    /// The folders whose files are read; by default `layout.source-root`, `layout.test-root` and
    /// `layout.scripts-root`.
    #[serde(default)]
    pub roots: Option<Vec<String>>,
    /// The files outside the roots that are read.
    #[serde(default = "defaults::standalone_files")]
    pub standalone_files: Vec<String>,
    /// The one file that executes advisory SQL.
    pub executor: String,
    /// The baseline migrations, each with exactly one transaction lock of the `ledger` key.
    #[serde(default)]
    pub migrations: Vec<String>,
    /// The negative test, with exactly one `pg_advisory_xact_lock($1::bigint)`.
    #[serde(default)]
    pub negative: Option<String>,
    /// The `FAMILY.RESOURCE` static key the baseline migrations lock.
    #[serde(default)]
    pub ledger: Option<String>,
    /// Static resources whose identifiers are too small to stand for them in source text.
    #[serde(default)]
    pub contextual_resources: Option<Vec<ContextualResource>>,
    /// The files that may hold registered identifiers; with none, identifiers are not checked.
    #[serde(default)]
    pub inventory: Option<Vec<String>>,
    /// The mint authority; with none, minting is not checked.
    #[serde(default)]
    pub mint: Option<Mint>,
    /// The advisory functions the executor may call.
    #[serde(default = "defaults::advisory_functions")]
    pub advisory_functions: Vec<String>,
    /// The calls and tags whose argument is SQL.
    #[serde(default)]
    pub sql_sinks: SqlSinks,
    /// The shell executables that run SQL.
    #[serde(default = "defaults::shell_executables")]
    pub shell_executables: Vec<String>,
    /// The properties that make an object literal a structural key.
    #[serde(default = "defaults::mint_properties")]
    pub mint_properties: Vec<String>,
}

/// The defaults of the options that have one.
mod defaults {
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    pub fn standalone_files() -> Vec<String> {
        strings(&["package.json"])
    }

    pub fn advisory_functions() -> Vec<String> {
        strings(&[
            "pg_advisory_lock",
            "pg_advisory_lock_shared",
            "pg_try_advisory_lock",
            "pg_try_advisory_lock_shared",
            "pg_advisory_unlock",
            "pg_advisory_unlock_shared",
            "pg_advisory_xact_lock",
            "pg_advisory_xact_lock_shared",
            "pg_try_advisory_xact_lock",
            "pg_try_advisory_xact_lock_shared",
        ])
    }

    pub fn shell_executables() -> Vec<String> {
        strings(&["execute", "psql", "query"])
    }

    pub fn mint_properties() -> Vec<String> {
        strings(&["family", "familyId", "resourceKey"])
    }
}

/// How a file is read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    /// A JavaScript or TypeScript source, read with its syntax tree.
    Ast,
    /// A SQL file, one statement.
    Sql,
    /// A shell script or JSON file, read for its shell commands.
    Shell,
}

impl Kind {
    fn of(path: &str) -> Option<Self> {
        let (_, extension) = path.rsplit_once('.')?;

        if AST_EXTENSIONS.contains(&extension) {
            Some(Self::Ast)
        } else if extension == "sql" {
            Some(Self::Sql)
        } else if SHELL_EXTENSIONS.contains(&extension) {
            Some(Self::Shell)
        } else {
            None
        }
    }
}

/// What the rule reads from the options and the manifest.
struct Rules<'o> {
    boundaries: Boundaries,
    ledger: Option<(i64, i64)>,
    /// The registered identifiers and the files that may hold them, when `inventory` is set.
    inventory: Option<(Vec<i64>, &'o [String])>,
    mint: Option<&'o Mint>,
    mint_properties: &'o [String],
    sinks: &'o SqlSinks,
    shell_executables: &'o [String],
    shell: source::ShellPatterns,
}

/// The outcome of one file: its statements, or its first finding.
type Outcome = Result<Vec<Text>, Rejection>;

fn rejection(aspect: &'static str, message: &str) -> Rejection {
    Rejection {
        aspect,
        message: units(message),
    }
}

/// The rule.
#[derive(Debug, Default)]
pub struct AdvisoryLockBoundary;

impl Options {
    /// The registry manifest: `manifest-path`, or `common/advisory-lock/registry.manifest.json`
    /// below the source root.
    fn manifest_path(&self, layout: &Layout) -> String {
        self.manifest_path.clone().unwrap_or_else(|| {
            format!(
                "{}common/advisory-lock/registry.manifest.json",
                layout.source_prefix()
            )
        })
    }

    /// Checks that every path option is a normalized path relative to the project root.
    fn check_paths(&self) -> gate_sdk::Result<()> {
        if let Some(manifest) = &self.manifest_path {
            paths::check("manifest-path", manifest, Form::File)?;
        }
        paths::check_all("roots", self.roots.iter().flatten(), Form::Folder)?;
        paths::check_all("standalone-files", &self.standalone_files, Form::File)?;
        paths::check("executor", &self.executor, Form::File)?;
        paths::check_all("migrations", &self.migrations, Form::File)?;
        if let Some(negative) = &self.negative {
            paths::check("negative", negative, Form::File)?;
        }
        paths::check_all("inventory", self.inventory.iter().flatten(), Form::File)?;
        if let Some(mint) = &self.mint {
            paths::check_all("mint.structural", &mint.structural, Form::File)?;
            for (index, required) in mint.required.iter().enumerate() {
                paths::check(
                    &format!("mint.required[{index}].path"),
                    &required.path,
                    Form::File,
                )?;
            }
        }
        Ok(())
    }
}

impl Rule for AdvisoryLockBoundary {
    type Options = Options;

    const NAME: &'static str = "advisory-lock-boundary";
    const TITLE: &'static str =
        "Advisory-lock SQL, keys and identifiers stay inside their registered boundaries";
    const ASPECTS: &'static [&'static str] = &[
        "boundary",
        "derivation",
        "executor",
        "inventory",
        "migration",
        "mint",
        "negative",
        "outside",
        "shell",
        "sql",
        "unreadable",
        "unresolved",
    ];
    const LANGUAGES: &'static [&'static str] = &["typescript"];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let layout = PluginOptions::read(run)?.layout;
        run.options().check_paths()?;
        let options = run.options();
        let mut findings = Findings::default();
        let options_file = run
            .options_file()
            .map_or_else(|| "gate.toml".to_owned(), str::to_owned);
        let manifest = run
            .tree()
            .text(&options.manifest_path(&layout))
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let manifest_path = options.manifest_path(&layout);
        let rules = rules(
            options,
            &manifest_path,
            manifest.as_ref(),
            &options_file,
            source::ShellPatterns::new()?,
            &mut findings,
        );
        let tree = run.tree();
        let source = project::source(run);
        let allocator = Allocator::default();
        let typescript = TypeScript::new(&source, project::settings(run)?, &allocator);
        let files = files(tree, options, &layout, typescript.settings());
        let boundaries = &rules.boundaries;
        let mut observed: Vec<String> = Vec::new();
        let mut rejected: Vec<String> = Vec::new();

        for path in &files {
            let Some(kind) = Kind::of(path) else {
                continue;
            };

            if *path == boundaries.executor {
                observed.push(path.clone());
            }

            let Some(bytes) = tree.bytes(path) else {
                continue;
            };

            let utf8 = std::str::from_utf8(&bytes).is_ok();

            // A SQL, JSON or shell file that is not UTF-8 text is still read, with replacement
            // characters; the loss is reported.
            if kind != Kind::Ast && !utf8 {
                findings.push(
                    "unreadable",
                    path.as_str(),
                    None,
                    "The file is not UTF-8 text, so it was read with replacement characters.",
                );
                rejected.push(path.clone());
            }

            match scan(
                &typescript,
                path,
                kind,
                &String::from_utf8_lossy(&bytes),
                utf8,
                &rules,
            )? {
                Ok(statements) => {
                    if !statements.is_empty()
                        && (boundaries.migrations.contains(path)
                            || boundaries.negative.as_ref() == Some(path))
                    {
                        observed.push(path.clone());
                    }
                }
                Err(rejection) => {
                    findings.push(
                        rejection.aspect,
                        path.as_str(),
                        None,
                        String::from_utf16_lossy(&rejection.message),
                    );
                    rejected.push(path.clone());
                }
            }
        }

        let required = std::iter::once(&boundaries.executor)
            .chain(&boundaries.migrations)
            .chain(boundaries.negative.iter());

        for path in required {
            if !observed.contains(path) && !rejected.contains(path) {
                findings.push(
                    "boundary",
                    path.as_str(),
                    None,
                    format!("Required source SQL boundary {path} was not observed."),
                );
            }
        }

        findings.report(run, help);
        Ok(())
    }
}

/// The files the rule reads: the tracked `standalone-files`, then the files below `roots`, outside
/// the folders of `[languages.typescript] skip-folders`.
fn files(tree: Tree<'_>, options: &Options, layout: &Layout, settings: &Settings) -> Vec<String> {
    let roots = options.roots.clone().unwrap_or_else(|| {
        vec![
            layout.source_root.clone(),
            layout.test_root.clone(),
            layout.scripts_root.clone(),
        ]
    });

    options
        .standalone_files
        .iter()
        .filter(|path| tree.contains(path))
        .cloned()
        .chain(
            roots
                .iter()
                .flat_map(|root| tree.under(root))
                .filter(|path| !settings.skips(path)),
        )
        .collect()
}

/// What to do about a finding of `aspect`.
fn help(aspect: &str) -> &'static str {
    match aspect {
        "inventory" => "keep registered identifiers in the files the inventory lists",
        "mint" => "mint advisory-lock keys only in the mint files",
        "unreadable" => "store the file as UTF-8 text that parses",
        "unresolved" => "pass advisory SQL as a static text the rule can read",
        "shell" => "run advisory SQL only through the executor, not from a shell command",
        _ => "run advisory SQL only through its registered boundary",
    }
}

/// The rules of the options and the manifest. Rules the manifest cannot supply are reported and
/// left out.
fn rules<'o>(
    options: &'o Options,
    manifest_path: &str,
    manifest: Option<&Value>,
    options_file: &str,
    shell: source::ShellPatterns,
    findings: &mut Findings,
) -> Rules<'o> {
    let boundaries = Boundaries {
        executor: options.executor.clone(),
        functions: options.advisory_functions.clone(),
        migrations: options.migrations.clone(),
        negative: options.negative.clone(),
        ledger: options.ledger.clone(),
    };

    // Only the baseline migrations read the ledger key; without it they are not compared, and the
    // options or the manifest are reported instead.
    let ledger = match (
        boundaries.migrations.is_empty(),
        boundaries.ledger.as_deref(),
    ) {
        (true, _) => None,
        (false, None) => {
            findings.push(
                "migration",
                options_file,
                None,
                "Baseline migration boundaries need the ledger key they lock.",
            );
            None
        }
        (false, Some(key)) => {
            let found = manifest.and_then(|manifest| ledger_key(manifest, key));

            if found.is_none() {
                findings.push(
                    "migration",
                    manifest_path,
                    None,
                    "Frozen ledger procedure key is absent from the manifest.",
                );
            }

            found
        }
    };
    // A contextual list is checked whenever it is set; one that does not match the manifest is
    // reported at the options and leaves every identifier in the inventory.
    let excluded = match (manifest, &options.contextual_resources) {
        (Some(manifest), Some(listed)) => match contextual::excluded(manifest, listed) {
            Ok(excluded) => excluded,
            Err(message) => {
                findings.push("inventory", options_file, None, message);
                Vec::new()
            }
        },
        _ => Vec::new(),
    };
    let inventory = match &options.inventory {
        None => None,
        Some(allowed) => {
            let registered = manifest.map(|manifest| registered_ids(manifest, &excluded));
            let problem = match &registered {
                None => Some("cannot be read"),
                Some(registered) if registered.is_empty() => Some("holds no registered identifier"),
                Some(_) => None,
            };

            if let Some(problem) = problem {
                findings.push(
                    "inventory",
                    manifest_path,
                    None,
                    format!(
                        "The advisory-lock registry manifest {problem}, so registered identifiers cannot be checked."
                    ),
                );
            }

            registered
                .filter(|registered| !registered.is_empty())
                .map(|registered| (registered, allowed.as_slice()))
        }
    };

    Rules {
        boundaries,
        ledger,
        inventory,
        mint: options.mint.as_ref(),
        mint_properties: &options.mint_properties,
        sinks: &options.sql_sinks,
        shell_executables: &options.shell_executables,
        shell,
    }
}

/// The first finding of one file, in order, or its executable statements.
fn scan(
    typescript: &TypeScript<'_>,
    path: &str,
    kind: Kind,
    text: &str,
    utf8: bool,
    rules: &Rules<'_>,
) -> gate_sdk::Result<Outcome> {
    let parsed = match kind {
        Kind::Ast => typescript.parse(path)?,
        Kind::Sql | Kind::Shell => None,
    };
    let tree = parsed.as_deref().filter(|file| !file.fatal());

    if let Some((registered, allowed)) = &rules.inventory {
        // Every file is read as TypeScript for its identifiers; a JavaScript or TypeScript source
        // keeps its own syntax tree here.
        let allocator = Allocator::default();
        let present = match kind {
            Kind::Ast => source::registered_ids(text, tree, registered),
            Kind::Sql | Kind::Shell => {
                let file = parse_source(&allocator, "registered-id-inventory.ts", text)
                    .ok()
                    .filter(|file| !file.fatal());
                source::registered_ids(text, file.as_ref(), registered)
            }
        };

        match present {
            // An inventory that cannot be read, past a budget or the recovery's tracking, is
            // unresolved.
            Err(fail) => {
                return Ok(Err(Rejection {
                    aspect: "unresolved",
                    message: fail.message,
                }));
            }
            Ok(present)
                if !present.is_empty() && !allowed.iter().any(|allowed| allowed == path) =>
            {
                return Ok(Err(rejection(
                    "inventory",
                    &format!(
                        "{path} contains registered numeric IDs outside the exact source allowlist."
                    ),
                )));
            }
            Ok(_) => {}
        }
    }

    if let Some(mint) = rules.mint
        && let Err(message) = mint::check(path, text, tree, mint, rules.mint_properties)
    {
        return Ok(Err(rejection("mint", &message)));
    }

    let extracted = match kind {
        Kind::Sql => Ok(vec![units(text)]),
        Kind::Shell => {
            return Ok(
                source::shell_commands(&rules.shell, path, text, rules.shell_executables)
                    .map(|()| Vec::new())
                    .map_err(|fail| Rejection {
                        aspect: "shell",
                        message: fail.message,
                    }),
            );
        }
        Kind::Ast => {
            let Some(file) = parsed else {
                return Ok(Ok(Vec::new()));
            };

            if file.fatal() {
                return Ok(Err(rejection(
                    "unreadable",
                    if utf8 {
                        "The file cannot be parsed, so its executable advisory SQL cannot be checked."
                    } else {
                        "The file is not UTF-8 text, so its executable advisory SQL cannot be checked."
                    },
                )));
            }

            source::statements(
                &file,
                &Extraction {
                    path,
                    canonical: path == rules.boundaries.executor,
                    sinks: rules.sinks,
                },
            )
        }
    };

    Ok(extracted
        .map_err(extraction_rejection)
        .and_then(|statements| {
            for statement in &statements {
                validate(path, statement, &rules.boundaries, rules.ledger)?;
            }

            Ok(statements)
        }))
}

/// The registered identifiers of the manifest: every family's, then every resource's but the
/// `excluded` contextual ones.
fn registered_ids(manifest: &Value, excluded: &[i64]) -> Vec<i64> {
    let families = manifest
        .get("families")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let family_ids = families
        .iter()
        .filter_map(|family| family.get("familyId").and_then(Value::as_i64));
    let resource_ids = families.iter().flat_map(|family| {
        family
            .get("resources")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|resource| resource.get("resourceId").and_then(Value::as_i64))
            .filter(|id| !excluded.contains(id))
    });

    family_ids.chain(resource_ids).collect()
}

/// The family and resource identifiers of the ledger static key `<FAMILY>.<RESOURCE>`.
fn ledger_key(manifest: &Value, ledger: &str) -> Option<(i64, i64)> {
    let (family_name, resource_name) = ledger.split_once('.')?;
    let family = manifest
        .get("families")?
        .as_array()?
        .iter()
        .find(|family| family.get("name").and_then(Value::as_str) == Some(family_name))?;
    let resource = family
        .get("resources")?
        .as_array()?
        .iter()
        .find(|resource| resource.get("name").and_then(Value::as_str) == Some(resource_name))?;

    Some((
        family.get("familyId")?.as_i64()?,
        resource.get("resourceId")?.as_i64()?,
    ))
}
