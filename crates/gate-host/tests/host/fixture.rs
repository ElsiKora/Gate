//! Synthetic git repositories for the tests, made with `git init` in a temporary directory.

use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Repo {
    directory: tempfile::TempDir,
}

impl Repo {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let repo = Self { directory };
        repo.git(&["init", "-q", "-b", "main"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    pub fn file(&self, path: &str) -> PathBuf {
        self.path().join(path)
    }

    pub fn write(&self, path: &str, contents: &str) -> &Self {
        let full = self.file(path);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("create directories");
        std::fs::write(full, contents).expect("write file");
        self
    }

    pub fn remove(&self, path: &str) -> &Self {
        std::fs::remove_file(self.file(path)).expect("remove file");
        self
    }

    pub fn git(&self, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.path())
            .args(arguments)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("utf-8")
            .trim_end()
            .to_owned()
    }

    pub fn add(&self) -> &Self {
        self.git(&["add", "-A"]);
        self
    }

    pub fn commit(&self, message: &str) -> String {
        self.add();
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
}

pub const ROOT: &str = "[gate]\nversion = \"0.1.0\"\n";

/// A pnpm monorepo: `server` (gate.toml) and `web` depend on `shared`; `docs` holds root files.
pub fn monorepo() -> Repo {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        "[gate]\nversion = \"0.1.0\"\n\n[workspace]\nprojects = \"pnpm\"\n",
    )
    .write(
        "pnpm-workspace.yaml",
        "packages:\n  - \"apps/*\"\n  - \"packages/*\"\n",
    )
    .write("package.json", "{\"name\": \"root\", \"private\": true}\n")
    .write(
        "apps/server/package.json",
        "{\"name\": \"@acme/server\", \"dependencies\": {\"@acme/shared\": \"workspace:*\"}}\n",
    )
    .write("apps/server/gate.toml", "[project]\nname = \"server\"\n")
    .write("apps/server/src/main.ts", "export const main = 1;\n")
    .write(
        "apps/web/package.json",
        "{\"name\": \"web\", \"devDependencies\": {\"@acme/shared\": \"workspace:*\"}}\n",
    )
    .write("apps/web/index.ts", "export {};\n")
    .write(
        "packages/shared/package.json",
        "{\"name\": \"@acme/shared\"}\n",
    )
    .write("packages/shared/index.ts", "export const shared = 1;\n")
    .write("docs/guide.md", "# Guide\n");
    repo.commit("initial");
    repo
}
