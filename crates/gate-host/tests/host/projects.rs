//! Project discovery and ownership.

use gate_host::{Origin, Source, Workspace};

use crate::fixture::{ROOT, Repo, monorepo};

fn summary(workspace: &Workspace) -> Vec<String> {
    workspace
        .projects()
        .iter()
        .map(|project| {
            let origins: Vec<String> = project.origins.iter().map(ToString::to_string).collect();
            let dependencies: Vec<&str> = project.dependencies.iter().map(String::as_str).collect();
            format!(
                "{} {} [{}] -> [{}]",
                project.name,
                project.root,
                origins.join(","),
                dependencies.join(",")
            )
        })
        .collect()
}

#[test]
fn pnpm_workspace_with_negation_and_nested_globs() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[workspace]\nprojects = \"pnpm\"\n"),
    )
    .write(
        "pnpm-workspace.yaml",
        "packages:\n  - 'apps/*'\n  - 'packages/**'\n  - '!packages/legacy'\n",
    )
    .write(
        "apps/api/package.json",
        "{\"name\": \"api\", \"dependencies\": {\"core\": \"workspace:*\"}}",
    )
    .write("apps/notes.txt", "not a project\n")
    .write("packages/core/package.json", "{\"name\": \"core\"}")
    .write(
        "packages/group/inner/package.json",
        "{\"name\": \"inner\", \"dependencies\": {\"left-pad\": \"1.0.0\"}}",
    )
    .write("packages/legacy/package.json", "{\"name\": \"legacy\"}")
    .write("tools/script.ts", "x\n")
    .add();

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        summary(&workspace),
        [
            "api apps/api [pnpm] -> [core]",
            "core packages/core [pnpm] -> []",
            "inner packages/group/inner [pnpm] -> []",
        ]
    );
}

#[test]
fn npm_workspaces_as_a_list_and_as_packages() {
    for manifest in [
        "{\"name\": \"root\", \"workspaces\": [\"libs/*\"]}",
        "{\"name\": \"root\", \"workspaces\": {\"packages\": [\"libs/*\"]}}",
    ] {
        let repo = Repo::new();
        repo.write(
            "gate.toml",
            &format!("{ROOT}\n[workspace]\nprojects = \"npm\"\n"),
        )
        .write("package.json", manifest)
        .write(
            "libs/a/package.json",
            "{\"name\": \"@x/a\", \"peerDependencies\": {\"@x/b\": \"*\"}}",
        )
        .write("libs/b/package.json", "{\"name\": \"@x/b\"}")
        .add();

        let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
        assert_eq!(
            summary(&workspace),
            ["@x/a libs/a [npm] -> [@x/b]", "@x/b libs/b [npm] -> []"],
            "{manifest}"
        );
    }
}

#[test]
fn explicit_globs_match_directories_with_tracked_files() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[workspace]\nprojects = [\"services/*\"]\nignore = [\"services/old\"]\n"),
    )
    .write("services/a/main.go", "package a\n")
    .write("services/b/deep/main.go", "package b\n")
    .write("services/old/main.go", "package old\n")
    .add();
    std::fs::create_dir_all(repo.file("services/empty")).expect("directory");

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        summary(&workspace),
        [
            "services/a services/a [glob] -> []",
            "services/b services/b [glob] -> []"
        ]
    );
}

#[test]
fn tracked_gate_toml_files_make_projects_and_ignored_ones_do_not() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!("{ROOT}\n[workspace]\nignore = [\"samples/**\"]\n"),
    )
    .write("tools/gen/gate.toml", "")
    .write("tools/gen/package.json", "{\"name\": \"generator\"}")
    .write("apps/server/gate.toml", "[project]\nname = \"server\"\n")
    .write("samples/demo/gate.toml", "[gate]\nversion = \"0.1.0\"\n")
    .write("samples/gate.toml", "[gate]\nversion = \"0.1.0\"\n")
    .add();
    repo.write("untracked/gate.toml", "[project]\nname = \"ghost\"\n");

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        summary(&workspace),
        [
            "generator tools/gen [gate.toml] -> []",
            "server apps/server [gate.toml] -> []"
        ]
    );
}

#[test]
fn a_path_belongs_to_the_project_with_the_longest_root() {
    let repo = Repo::new();
    repo.write("gate.toml", &format!("{ROOT}\n[workspace]\n"))
        .write("apps/server/gate.toml", "[project]\nname = \"server\"\n")
        .write(
            "apps/server/tools/gen/gate.toml",
            "[project]\nname = \"gen\"\n",
        )
        .write(
            "apps/server-two/gate.toml",
            "[project]\nname = \"server-two\"\n",
        )
        .add();

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    let projects = workspace.projects();
    let owner = |path: &str| projects.owner(path).map(|project| project.name.as_str());

    assert_eq!(owner("apps/server/src/main.ts"), Some("server"));
    assert_eq!(owner("apps/server/tools/gen/x.ts"), Some("gen"));
    assert_eq!(owner("apps/server/tools/other.ts"), Some("server"));
    assert_eq!(owner("apps/server-two/a.ts"), Some("server-two"));
    assert_eq!(
        owner("apps/server"),
        None,
        "the root directory itself is not inside"
    );
    assert_eq!(owner("README.md"), None);
}

#[test]
fn package_manager_and_gate_toml_origins_merge() {
    let workspace = Workspace::open(monorepo().path(), Source::WorkTree).expect("open");
    let server = workspace.projects().get("server").expect("server");
    assert_eq!(
        server.origins.iter().copied().collect::<Vec<_>>(),
        [Origin::GateToml, Origin::Pnpm]
    );
    assert_eq!(server.package.as_deref(), Some("@acme/server"));
    assert_eq!(server.config.as_deref(), Some("apps/server/gate.toml"));
    assert_eq!(
        summary(&workspace),
        [
            "@acme/shared packages/shared [pnpm] -> []",
            "server apps/server [gate.toml,pnpm] -> [@acme/shared]",
            "web apps/web [pnpm] -> [@acme/shared]",
        ]
    );
}

#[test]
fn a_workspace_that_lists_the_top_makes_the_root_a_project() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!(
            "{ROOT}\n[project]\nname = \"top\"\n\n[workspace]\nprojects = [\".\", \"libs/*\"]\n"
        ),
    )
    .write("libs/a/lib.rs", "\n")
    .write("README.md", "x\n")
    .add();

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert!(workspace.root_is_project());
    assert_eq!(
        summary(&workspace),
        ["libs/a libs/a [glob] -> []", "top  [glob] -> []"]
    );
    let owner = |path: &str| {
        workspace
            .projects()
            .owner(path)
            .map(|project| project.name.as_str())
    };
    assert_eq!(owner("README.md"), Some("top"));
    assert_eq!(owner("libs/a/lib.rs"), Some("libs/a"));
}

#[test]
fn without_a_workspace_the_root_is_a_project_beside_nested_ones() {
    let repo = Repo::new();
    repo.write("gate.toml", ROOT)
        .write("tools/gen/gate.toml", "")
        .write("README.md", "x\n")
        .add();

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("open");
    assert_eq!(
        summary(&workspace),
        [
            ".  [no workspace] -> []",
            "tools/gen tools/gen [gate.toml] -> []"
        ]
    );
    assert_eq!(
        workspace
            .projects()
            .owner("README.md")
            .map(|project| project.name.as_str()),
        Some(".")
    );
}
