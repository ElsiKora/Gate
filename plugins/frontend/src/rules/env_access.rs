//! `frontend/env-access`: one module reads the environment, each key by a literal member, with the
//! pack's public prefix in the module the browser loads; Next.js keeps server keys in a module that
//! imports `server-only`; a library reads no environment.

use gate_sdk::{NoOptions, Rule, Run};
use gate_typescript::SourceFile;
use gate_typescript::call::TypeScriptCall;
use gate_typescript::oxc_ast::AstKind;
use gate_typescript::oxc_ast::ast::{Expression, Statement};
use gate_typescript::oxc_span::GetSpan;

use crate::code::{Project, is_global};
use crate::options::{PACK, Pack};
use crate::rules::{Found, report};

/// The rule.
#[derive(Debug, Default)]
pub struct EnvAccess;

/// The module a browser bundle loads that reads the environment.
pub const ENV_MODULE: &str = "src/shared/config/env.ts";

/// The Next.js module that reads the keys only the server may see.
pub const SERVER_ENV_MODULE: &str = "src/shared/config/server-env.ts";

/// The keys Vite sets itself, without its public prefix.
const VITE_BUILT_INS: [&str; 5] = ["BASE_URL", "DEV", "MODE", "PROD", "SSR"];

/// The keys Next.js inlines without its public prefix.
const NEXT_BUILT_INS: [&str; 1] = ["NODE_ENV"];

impl Rule<TypeScriptCall> for EnvAccess {
    type Options = NoOptions;

    const NAME: &'static str = "env-access";
    const TITLE: &'static str =
        "One module reads the environment, each key by a literal member with the public prefix";
    const ASPECTS: &'static [&'static str] = &["outside", "dynamic", "prefix", "server-guard"];
    const LANGUAGES: &'static [&'static str] = &["typescript"];
    const NEEDS: &'static [gate_sdk::Need] = &[PACK];

    fn check(&self, run: &mut Run<'_, NoOptions, TypeScriptCall>) -> gate_sdk::Result<()> {
        run.with_call(|run, shared| {
            let project = Project::new(run, shared)?;
            let found = check(&project)?;
            report(run, found);
            Ok(())
        })
    }
}

/// How a read reaches the environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Syntax {
    /// `import.meta.env`, Vite's.
    ImportMeta,
    /// `process.env`, Node's and Next.js's.
    Process,
}

impl Syntax {
    fn text(self) -> &'static str {
        match self {
            Self::ImportMeta => "import.meta.env",
            Self::Process => "process.env",
        }
    }
}

/// One read of the environment: its syntax, its line, and its key when it is a literal member.
struct Read {
    syntax: Syntax,
    line: u32,
    key: Option<String>,
}

/// The environment modules of `pack`.
fn modules(pack: Pack) -> &'static [&'static str] {
    match pack {
        Pack::ReactVite => &[ENV_MODULE],
        Pack::Nextjs => &[ENV_MODULE, SERVER_ENV_MODULE],
        Pack::ReactLibrary => &[],
    }
}

fn check(project: &Project<'_, '_>) -> gate_sdk::Result<Vec<Found>> {
    let pack = project.pack;
    let mut found = Vec::new();

    for path in project.all_code() {
        let Some(file) = project.parse(path)? else {
            continue;
        };
        let module = modules(pack).contains(&path);

        if pack == Pack::Nextjs && path == SERVER_ENV_MODULE && !guarded(&file) {
            found.push(Found::at(
                "server-guard",
                path,
                format!("{path} does not start with import \"server-only\""),
                "start the module with import \"server-only\", so a client bundle that imports it fails to build",
            ));
        }

        found.extend(
            reads(&file)
                .iter()
                .filter_map(|read| finding(pack, path, module, read)),
        );
    }

    Ok(found)
}

/// The finding about one read of `path`, which is an environment module when `module`; `None` when
/// the read is as the pack wants it.
fn finding(pack: Pack, path: &str, module: bool, read: &Read) -> Option<Found> {
    let text = read.syntax.text();

    if !module {
        let (message, help) = if pack.library() {
            (
                format!("{path} reads {text}; a library reads no environment"),
                "take the value from the caller, through props or a setup function".to_owned(),
            )
        } else {
            (
                format!(
                    "{path} reads {text} outside {}",
                    modules(pack).join(" and ")
                ),
                format!("read the key in {ENV_MODULE} and import the typed value from there"),
            )
        };
        return Some(
            Found::at("outside", path, message, help)
                .line(read.line)
                .named(text),
        );
    }

    let expected = if pack == Pack::ReactVite {
        Syntax::ImportMeta
    } else {
        Syntax::Process
    };
    if read.syntax != expected {
        return Some(
            Found::at(
                "prefix",
                path,
                format!(
                    "{path} reads {text}; the {} pack reads {}",
                    pack.name(),
                    expected.text()
                ),
                format!("read the key as {}.<KEY>", expected.text()),
            )
            .line(read.line)
            .named(text),
        );
    }

    let Some(key) = &read.key else {
        return Some(
            Found::at(
                "dynamic",
                path,
                format!("{path} reads {text} other than by a literal member; the bundler replaces only {text}.<KEY>"),
                format!("read each key as {text}.<KEY>"),
            )
            .line(read.line)
            .named(text),
        );
    };

    if path != ENV_MODULE || public(pack, key) {
        return None;
    }
    let prefix = if pack == Pack::ReactVite {
        "VITE_"
    } else {
        "NEXT_PUBLIC_"
    };
    let help = if pack == Pack::Nextjs {
        format!(
            "read a server key in {SERVER_ENV_MODULE}, or name a key the browser needs {prefix}<KEY>"
        )
    } else {
        format!("name the key {prefix}<KEY>; Vite exposes no other key to the browser")
    };
    Some(
        Found::at(
            "prefix",
            path,
            format!("{path} reads {key}, which has no {prefix} prefix"),
            help,
        )
        .line(read.line)
        .named(key),
    )
}

/// Whether `key` reaches the browser in `pack`.
fn public(pack: Pack, key: &str) -> bool {
    match pack {
        Pack::ReactVite => key.starts_with("VITE_") || VITE_BUILT_INS.contains(&key),
        Pack::Nextjs => key.starts_with("NEXT_PUBLIC_") || NEXT_BUILT_INS.contains(&key),
        Pack::ReactLibrary => false,
    }
}

/// Whether the first statement of `file` is `import "server-only"`.
fn guarded(file: &SourceFile<'_>) -> bool {
    matches!(
        file.program().body.first(),
        Some(Statement::ImportDeclaration(import))
            if import.source.value.as_str() == "server-only" && import.specifiers.is_none()
    )
}

/// The reads of the environment in `file`.
fn reads(file: &SourceFile<'_>) -> Vec<Read> {
    let nodes = file.semantic().nodes();
    let mut found = Vec::new();

    for node in nodes.iter() {
        let AstKind::StaticMemberExpression(member) = node.kind() else {
            continue;
        };
        if member.property.name.as_str() != "env" {
            continue;
        }
        let syntax = match &member.object {
            Expression::ImportMeta(_) => Syntax::ImportMeta,
            Expression::Identifier(object)
                if object.name.as_str() == "process" && is_global(file, object) =>
            {
                Syntax::Process
            }
            _ => continue,
        };
        let key = match nodes.parent_kind(node.id()) {
            AstKind::StaticMemberExpression(outer) if outer.object.span() == member.span => {
                Some(outer.property.name.as_str().to_owned())
            }
            _ => None,
        };
        found.push(Read {
            syntax,
            line: file.line(member.span.start),
            key,
        });
    }

    found
}
