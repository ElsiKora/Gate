//! `agents/instruction-references`: backticked paths and commands.

use crate::fixture::{Fixture, lines};

const ENTRY: &str = "\
Run `npm run build` and `npm run missing --flag`, then `npm run`.

Read `docs/guide.md`, `docs/guide.md:12`, `docs/guide.md:12:3`, `docs/`, `src/*.ts`, `README.md`, `.constant.ts`.

Missing: `docs/nothing.md`, `nothing.md`, `src/gone/`, `docs/*.rs`, `.github/none.yml`.

Ignored: `dist/app.js`, `build/`, `secrets.json`.

Prose: `<name>.md`, `./local.ts`, `../up.ts`, `a b.md`, `https://x.example/a.md`, `--flag.md`, `*.md`, `.md`, `words`, `unknown/top.md`, `x=1.md`.
";

/// The npm form of a command: `npm run <script>`.
const NPM: &str = r#"{"command-pattern": "^npm run ([A-Za-z0-9:_.-]+)(?:\\s|$)"}"#;

fn fixture(entry: &str) -> Fixture {
    Fixture::new(&[
        ("AGENTS.md", entry),
        ("package.json", r#"{"scripts": {"build": "tsc"}}"#),
        (".gitignore", "dist/\nbuild\nsecrets.json\n"),
        ("docs/guide.md", "# Guide\n"),
        ("docs/README.md", "# Docs\n"),
        ("src/app.ts", "\n"),
        ("src/app.constant.ts", "\n"),
        ("docs/agents/guide.md", "See `docs/agents/missing.md`.\n"),
        (
            "docs/agents/notes.txt",
            "See `docs/agents/also-missing.md`.\n",
        ),
    ])
}

#[test]
fn commands_name_scripts_and_paths_name_tracked_or_ignored_locations() {
    let path = |line: u32, token: &str| {
        format!("#path AGENTS.md:{line} `{token}` is neither tracked nor an ignored location")
    };
    let findings = fixture(ENTRY)
        .check("instruction-references", NPM)
        .expect("runs");
    assert_eq!(
        lines(&findings),
        [
            "#command AGENTS.md:1 `npm run missing` names no package.json script".to_owned(),
            path(5, "docs/nothing.md"),
            path(5, "nothing.md"),
            path(5, "src/gone/"),
            path(5, "docs/*.rs"),
            path(5, ".github/none.yml"),
            "#path docs/agents/guide.md:1 `docs/agents/missing.md` is neither tracked nor an ignored location"
                .to_owned(),
        ]
    );
    assert_eq!(findings[0].target.as_deref(), Some("missing"));
}

#[test]
fn without_a_command_pattern_commands_are_not_checked() {
    let findings = fixture(ENTRY)
        .check("instruction-references", "{}")
        .expect("runs");
    assert!(
        lines(&findings)
            .iter()
            .all(|line| line.starts_with("#path ")),
        "{:?}",
        lines(&findings)
    );
    assert_eq!(findings.len(), 6);
}

#[test]
fn the_command_pattern_and_extensions_are_options() {
    let entry = "Use `make lint` and `make ghost`, and read `notes.adoc`.\n";
    let options = r#"{"command-pattern": "^make ([a-z]+)$", "package-json": "tools/package.json", "extensions": [".adoc"]}"#;
    let fixture = Fixture::new(&[
        ("AGENTS.md", entry),
        ("tools/package.json", r#"{"scripts": {"lint": "x"}}"#),
    ]);
    assert_eq!(
        lines(
            &fixture
                .check("instruction-references", options)
                .expect("runs")
        ),
        [
            "#command AGENTS.md:1 `make ghost` names no tools/package.json script",
            "#path AGENTS.md:1 `notes.adoc` is neither tracked nor an ignored location",
        ]
    );
    for pattern in ["(", "^make [a-z]+$"] {
        let error = fixture
            .check(
                "instruction-references",
                &format!(r#"{{"command-pattern": "{pattern}"}}"#),
            )
            .expect_err("refuses the pattern");
        assert!(error.to_string().contains("command-pattern"), "{error}");
    }
}

#[test]
fn a_missing_entry_is_an_input_finding_and_the_root_is_still_read() {
    let fixture = Fixture::new(&[
        ("docs/guide.md", "`npm run ghost` and `docs/none.md`\n"),
        ("docs/agents/notes.md", "`docs/none.md`\n"),
    ]);
    assert_eq!(
        lines(&fixture.check("instruction-references", "{}").expect("runs")),
        [
            "#input AGENTS.md AGENTS.md is not tracked, and agents/instruction-references reads it (its option entry); create it, point entry at the file, or turn the input off with a decision: \"agents/instruction-references\" = { levels = { input = \"off\" }, decision = \"<record>\" }",
            "#path docs/agents/notes.md:1 `docs/none.md` is neither tracked nor an ignored location",
        ]
    );
}

/// The instructions of the project `apps/server` in a workspace whose root also holds
/// `infrastructure` and the workflows.
const SERVER: &str = "\
Root: `infrastructure/cdk`, `infrastructure/cdk/**/*.ts`, `.github/workflows/test.yml`, `infrastructure/cdk/`.

Project: `src/main.ts`, `src/`.

Missing at the root: `infrastructure/cdk/missing.ts`, `infrastructure/none/`, `infrastructure/**/*.rs`, `.github/workflows/none.yml`.

Missing in the project: `src/missing.ts`.
";

/// The tree of `apps/server`: its own files and, with `granted`, the repository paths the
/// configuration grants it (`reads = ["infrastructure/cdk", ".github/workflows"]`).
fn server(granted: bool) -> Fixture {
    let mut files = vec![("AGENTS.md", SERVER), ("src/main.ts", "\n")];
    if granted {
        files.extend([
            ("../../infrastructure/cdk/lib/stack.ts", "\n"),
            ("../../.github/workflows/test.yml", "on: push\n"),
        ]);
    }
    Fixture::new(&files)
}

fn missing(line: u32, token: &str) -> String {
    format!("#path AGENTS.md:{line} `{token}` is neither tracked nor an ignored location")
}

#[test]
fn a_project_path_cited_from_the_repository_root_resolves_among_the_granted_paths() {
    let findings = server(true)
        .check_in("apps/server", "instruction-references", "{}", &[])
        .expect("runs");
    assert_eq!(
        lines(&findings),
        [
            missing(5, "infrastructure/cdk/missing.ts"),
            missing(5, "infrastructure/none/"),
            missing(5, "infrastructure/**/*.rs"),
            missing(5, ".github/workflows/none.yml"),
            missing(7, "src/missing.ts"),
        ]
    );
}

#[test]
fn without_a_grant_a_root_path_resolves_nowhere_and_is_judged_as_before() {
    let findings = server(false)
        .check_in("apps/server", "instruction-references", "{}", &[])
        .expect("runs");
    // `infrastructure/…` names no top-level entry of the project, so it is prose, as it was;
    // a dot-prefixed path is a path and is reported.
    assert_eq!(
        lines(&findings),
        [
            missing(1, ".github/workflows/test.yml"),
            missing(5, ".github/workflows/none.yml"),
            missing(7, "src/missing.ts"),
        ]
    );
}

#[test]
fn a_root_path_is_ignored_only_by_the_granted_root_gitignore_never_by_the_projects() {
    let entry = "Root: `infrastructure/cdk.out/app.js`, `infrastructure/ghost.ts`.\n";
    let tree = |root_gitignore: bool| {
        let mut files = vec![
            ("AGENTS.md", entry),
            // The project's own ignore file names the root folder; it must not hide a root path.
            (".gitignore", "infrastructure/\n"),
            ("../../infrastructure/cdk/lib/stack.ts", "\n"),
        ];
        if root_gitignore {
            files.push(("../../.gitignore", "cdk.out/\n"));
        }
        Fixture::new(&files)
    };
    let run = |root_gitignore: bool| {
        lines(
            &tree(root_gitignore)
                .check_in("apps/server", "instruction-references", "{}", &[])
                .expect("runs"),
        )
    };
    assert_eq!(run(true), [missing(1, "infrastructure/ghost.ts")]);
    assert_eq!(
        run(false),
        [
            missing(1, "infrastructure/cdk.out/app.js"),
            missing(1, "infrastructure/ghost.ts"),
        ]
    );
}
