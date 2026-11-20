//! `gate check` running the built `node` plugin on a synthetic pnpm workspace of a root, a service
//! and a library: the standard passes, and one change against each rule gives that rule's finding.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The version of the core under test.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The node plugin built as a component; the one test of this binary builds it once.
fn component() -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gate-plugin-node");
    let status = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args([
            "build",
            "--locked",
            "--release",
            "-p",
            "gate-plugin-node",
            "--target",
            "wasm32-wasip2",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("cargo runs");
    assert!(
        status.success(),
        "gate-plugin-node builds for wasm32-wasip2"
    );
    target.join("wasm32-wasip2/release/node.wasm")
}

const ROOT_GATE: &str = r#"
[workspace]
projects = ["apps/*", "packages/*"]

[plugins]
node = { source = "acme/rules:node@0.1.0", options = { kind = "monorepo-root" } }

[rules]
"node/package-manager" = "error"
"node/node-version" = "error"
"node/dependency-versions" = "error"
"node/formatter" = "error"
"node/linter" = "error"
"#;

/// A project's `gate.toml`: its name, its kind and the three project-scope rules.
fn project_gate(name: &str, kind: &str) -> String {
    format!(
        "[project]\nname = \"{name}\"\n\n[plugins]\nnode = {{ source = \"acme/rules:node@0.1.0\", options = {{ kind = \"{kind}\" }} }}\n\n[rules]\n\"node/package\" = \"error\"\n\"node/scripts\" = \"error\"\n\"node/tsconfig\" = \"error\"\n"
    )
}

const ROOT_MANIFEST: &str = r#"{
  "name": "workspace",
  "version": "1.0.0",
  "private": true,
  "license": "MIT",
  "type": "module",
  "packageManager": "pnpm@9.1.0",
  "engines": { "node": ">=24.11.0 <25" },
  "scripts": {
    "build": "pnpm -r run build",
    "lint": "eslint . --max-warnings 0",
    "lint:fix": "eslint . --fix",
    "typecheck": "pnpm -r run typecheck",
    "test": "pnpm -r run test",
    "format": "prettier --check .",
    "format:fix": "prettier --write ."
  },
  "devDependencies": {
    "@elsikora/eslint-config": "catalog:",
    "eslint": "catalog:",
    "typescript": "catalog:"
  }
}
"#;

const WORKSPACE: &str = "packages:\n  - apps/*\n  - packages/*\ncatalog:\n  \"@elsikora/eslint-config\": 3.10.2\n  eslint: 10.15.0\n  typescript: 6.0.3\n";

const SERVICE: &str = r#"{
  "name": "@scope/api",
  "version": "1.0.0",
  "private": true,
  "license": "MIT",
  "type": "module",
  "engines": { "node": ">=24.11.0 <25" },
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "lint": "eslint . --max-warnings 0",
    "lint:fix": "eslint . --fix",
    "typecheck": "tsc --noEmit",
    "test": "node --test"
  },
  "dependencies": {
    "@scope/lib": "workspace:*"
  },
  "devDependencies": {
    "typescript": "catalog:"
  }
}
"#;

const LIBRARY: &str = r#"{
  "name": "@scope/lib",
  "version": "1.0.0",
  "license": "MIT",
  "type": "module",
  "engines": { "node": ">=24.11.0 <25" },
  "publishConfig": { "access": "public" },
  "exports": { ".": "./dist/index.js" },
  "files": ["dist"],
  "sideEffects": false,
  "repository": "github:example/lib",
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "lint": "eslint . --max-warnings 0",
    "lint:fix": "eslint . --fix",
    "typecheck": "tsc --noEmit",
    "test": "node --test"
  },
  "devDependencies": {
    "typescript": "catalog:"
  }
}
"#;

const BASE: &str = r#"{
  "compilerOptions": {
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noImplicitOverride": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "useUnknownInCatchVariables": true,
    "forceConsistentCasingInFileNames": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true,
    "module": "NodeNext",
    "moduleResolution": "NodeNext"
  }
}
"#;

const MEMBER_TSCONFIG: &str = "{\n  \"extends\": \"../../tsconfig.base.json\",\n  \"compilerOptions\": { \"outDir\": \"dist\" }\n}\n";

const ROOT_ESLINT: &str = "import { createConfig } from \"@elsikora/eslint-config\";\n\nexport default await createConfig({ withTypescript: true });\n";

const MEMBER_ESLINT: &str =
    "import root from \"../../eslint.config.mjs\";\n\nexport default root;\n";

/// The workspace's files as the standard wants them.
fn standard() -> Vec<(&'static str, String)> {
    vec![
        ("gate.toml", format!("[gate]\nversion = \"{VERSION}\"\n{ROOT_GATE}")),
        ("package.json", ROOT_MANIFEST.to_owned()),
        ("pnpm-workspace.yaml", WORKSPACE.to_owned()),
        ("pnpm-lock.yaml", "lockfileVersion: '9.0'\n".to_owned()),
        (".npmrc", "save-exact=true\n".to_owned()),
        (".nvmrc", "24.11.0\n".to_owned()),
        (
            ".prettierrc.json",
            "{\n  \"printWidth\": 120,\n  \"proseWrap\": \"preserve\",\n  \"trailingComma\": \"all\"\n}\n"
                .to_owned(),
        ),
        ("eslint.config.mjs", ROOT_ESLINT.to_owned()),
        ("tsconfig.base.json", BASE.to_owned()),
        ("apps/api/gate.toml", project_gate("api", "service")),
        ("apps/api/package.json", SERVICE.to_owned()),
        ("apps/api/eslint.config.mjs", MEMBER_ESLINT.to_owned()),
        ("apps/api/tsconfig.json", MEMBER_TSCONFIG.to_owned()),
        ("apps/api/src/main.ts", "export const answer = 42;\n".to_owned()),
        ("packages/lib/gate.toml", project_gate("lib", "library")),
        ("packages/lib/package.json", LIBRARY.to_owned()),
        ("packages/lib/eslint.config.mjs", MEMBER_ESLINT.to_owned()),
        ("packages/lib/tsconfig.json", MEMBER_TSCONFIG.to_owned()),
        ("packages/lib/src/index.ts", "export const one = 1;\n".to_owned()),
    ]
}

/// [`standard`] with one change against each of the eight rules.
fn broken() -> Vec<(&'static str, String)> {
    standard()
        .into_iter()
        .map(|(path, text)| {
            let text = match path {
                "package.json" => text.replace("pnpm@9.1.0", "pnpm@^9"),
                ".nvmrc" => "24\n".to_owned(),
                "pnpm-workspace.yaml" => text.replace("typescript: 6.0.3", "typescript: ^6.0.3"),
                ".prettierrc.json" => text.replace("120", "100"),
                "apps/api/eslint.config.mjs" => "export default [];\n".to_owned(),
                "packages/lib/package.json" => {
                    text.replace("\"license\"", "\"private\": true,\n  \"license\"")
                }
                "apps/api/package.json" => text.replace(
                    "\"test\": \"node --test\"",
                    "\"test\": \"node --test\",\n    \"lint:types\": \"tsc --noEmit\"",
                ),
                "apps/api/tsconfig.json" => {
                    text.replace("\"outDir\": \"dist\"", "\"strict\": false")
                }
                _ => text,
            };
            (path, text)
        })
        .collect()
}

struct Repo {
    directory: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Repo {
    fn new(files: &[(&str, String)]) -> Self {
        let repo = Self {
            directory: tempfile::tempdir().expect("temporary directory"),
            cache: tempfile::tempdir().expect("temporary cache"),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        for (path, contents) in files {
            let full = repo.directory.path().join(path);
            std::fs::create_dir_all(full.parent().expect("parent")).expect("directories");
            std::fs::write(full, contents).expect("write");
        }
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-q", "-m", "initial"]);
        repo
    }

    fn git(&self, arguments: &[&str]) {
        let output = isolated(
            Command::new("git")
                .arg("-C")
                .arg(self.directory.path())
                .args(arguments),
        )
        .output()
        .expect("git");
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// `gate check` with the built plugin given by `--plugin`.
    fn check(&self, plugin: &Path) -> Output {
        let argument = format!("node={}", plugin.display());
        isolated(
            Command::new(env!("CARGO_BIN_EXE_gate"))
                .args(["check", "--plugin", &argument])
                .current_dir(self.directory.path())
                .env("GATE_CACHE_DIR", self.cache.path()),
        )
        .output()
        .expect("gate")
    }
}

fn isolated(command: &mut Command) -> &mut Command {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GATE_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
}

/// The `rule#aspect path` of every error the text report prints, sorted.
fn findings(output: &Output) -> Vec<String> {
    let mut found: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("error["))
        .filter_map(|rest| {
            let (rule, rest) = rest.split_once("] ")?;
            let path = rest.split([':', ' ']).next()?;
            Some(format!("{rule} {path}"))
        })
        .collect();
    found.sort();
    found
}

#[test]
fn the_standard_workspace_passes_and_one_change_against_each_rule_gives_that_rules_finding() {
    let plugin = component();
    let output = Repo::new(&standard()).check(&plugin);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Repo::new(&broken()).check(&plugin);
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        findings(&output),
        [
            "node/dependency-versions#range pnpm-workspace.yaml",
            "node/formatter#settings .prettierrc.json",
            "node/linter#detached apps/api/eslint.config.mjs",
            "node/node-version#file .nvmrc",
            "node/package#forbidden packages/lib/package.json",
            "node/package-manager#field package.json",
            "node/scripts#synonym apps/api/package.json",
            "node/tsconfig#weakened apps/api/tsconfig.json"
        ],
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
