//! Configuration: one test per class of error, and what a valid file resolves to.

use gate_host::config::{PluginName, PluginSource, RuleId, SchemaValidator};
use gate_host::{ErrorKind, Level, RuleDeclaration, RuleScope, Setting, Source, Workspace};

use crate::fixture::{ROOT, Repo};

/// Builds a repository with `files` tracked and returns the error of opening it.
fn error(files: &[(&str, &str)]) -> String {
    let repo = Repo::new();
    for (path, contents) in files {
        repo.write(path, contents);
    }
    repo.add();
    let error = Workspace::open(repo.path(), Source::WorkTree).expect_err("configuration error");
    assert_eq!(error.kind(), ErrorKind::Config, "{error}");
    error.message().to_owned()
}

fn root_with(rest: &str) -> String {
    format!("{ROOT}{rest}")
}

fn assert_all(message: &str, parts: &[&str]) {
    for part in parts {
        assert!(
            message.contains(part),
            "{message:?} does not contain {part:?}"
        );
    }
}

const GIT_PLUGIN: &str = "\n[plugins]\ngit = { source = \"ElsiKora/Gate:git@0.1.0\" }\n";

/// A root that discovers the projects under `apps/`, so the root itself is not a project.
const WORKSPACE_ROOT: &str =
    "[gate]\nversion = \"0.1.0\"\n\n[workspace]\nprojects = [\"apps/*\"]\n";

#[test]
fn unknown_key_names_file_line_key_and_the_expected_keys() {
    let message = error(&[(
        "gate.toml",
        "[gate]\nversion = \"0.1.0\"\ncolour = \"blue\"\n",
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:3:",
            "unknown field `colour`",
            "expected `version`",
        ],
    );
}

#[test]
fn unknown_section_in_a_project_file_names_that_file() {
    let message = error(&[
        ("gate.toml", ROOT),
        ("apps/a/gate.toml", "[project]\nname = \"a\"\n\n[rule]\n"),
    ]);
    assert_all(&message, &["apps/a/gate.toml:4:", "unknown field `rule`"]);
}

#[test]
fn invalid_toml_names_the_position() {
    let message = error(&[("gate.toml", "[gate\nversion = \"0.1.0\"\n")]);
    assert_all(&message, &["gate.toml:1:"]);
}

#[test]
fn malformed_level() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = \"fatal\"\n"
        )),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:8:",
            "\"fatal\" is not a level",
            "\"error\", \"warning\", \"note\" or \"off\"",
        ],
    );
}

#[test]
fn malformed_rule_key() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"Git/Commit\" = \"error\"\n"
        )),
    )]);
    assert_all(
        &message,
        &["gate.toml:8:", "\"Git/Commit\" is not a rule id"],
    );
}

#[test]
fn malformed_plugin_source() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[plugins]\ngit = { source = \"https://example.invalid/git.wasm\" }\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:5:",
            "is not a plugin source",
            "<owner>/<repo>:<plugin>@<version>",
        ],
    );
}

#[test]
fn core_plugin_name_cannot_be_declared() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[plugins]\ngate = { source = \"ElsiKora/Gate:gate@0.1.0\" }\n"),
    )]);
    assert_all(&message, &["gate.toml:5:", "the core's own name"]);
}

#[test]
fn options_and_options_file_together() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = {{ options = {{}}, options-file = \"o.json\" }}\n"
        )),
    )]);
    assert_all(
        &message,
        &["gate.toml:8:", "options and options-file are both set"],
    );
}

#[test]
fn family_wildcard_takes_only_a_level() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/*\" = {{ level = \"off\", norm = \"RULE-1\" }}\n"
        )),
    )]);
    assert_all(
        &message,
        &["gate.toml:8:", "\"git/*\"", "takes only a level"],
    );
}

#[test]
fn aspect_is_not_a_key_of_its_own() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject#length\" = \"warning\"\n"
        )),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:8:",
            "an aspect is not a key of its own",
            "levels = { length = ",
        ],
    );
}

#[test]
fn empty_aspect_norm() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = {{ norms = {{ length = \"\" }} }}\n"
        )),
    )]);
    assert_all(&message, &["gate.toml:8:", "norms.length is empty"]);
}

#[test]
fn rule_of_an_undeclared_plugin() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[rules]\n\"layers/barrel-exports\" = \"error\"\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:5 [rules]",
            "\"layers/barrel-exports\"",
            "plugin layers is not declared for the repository",
            "[plugins.layers] source",
        ],
    );
}

#[test]
fn core_rules_cannot_be_configured() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[rules]\n\"gate/exceptions\" = \"off\"\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:5 [rules]",
            "\"gate/exceptions\"",
            "the core's own rules keep the gate honest, so they cannot be turned off",
        ],
    );
}

#[test]
fn core_rules_take_only_norm_and_norms_in_the_root_rules() {
    for (entry, parts) in [
        (
            "\"gate/exceptions\" = { level = \"warning\", norm = \"D-1\" }",
            &[
                "gate.toml:5 [rules]",
                "\"gate/exceptions\"",
                "cannot be turned off or given another level",
            ][..],
        ),
        (
            "\"gate/sync\" = { levels = { hooks = \"note\" } }",
            &[
                "\"gate/sync\"",
                "cannot be turned off or given another level",
            ][..],
        ),
        (
            "\"gate/baseline\" = { options = { keep = true } }",
            &["\"gate/baseline\"", "take no options"][..],
        ),
        (
            "\"gate/everything\" = { norm = \"D-1\" }",
            &[
                "\"gate/everything\" is not a core rule",
                "gate/baseline, gate/budget, gate/exceptions, gate/sync",
            ][..],
        ),
    ] {
        let message = error(&[("gate.toml", &root_with(&format!("\n[rules]\n{entry}\n")))]);
        assert_all(&message, parts);
    }

    let message = error(&[
        ("gate.toml", WORKSPACE_ROOT),
        (
            "apps/a/gate.toml",
            "[project]\nname = \"a\"\n\n[rules]\n\"gate/exceptions\" = { norm = \"D-1\" }\n",
        ),
    ]);
    assert_all(
        &message,
        &[
            "apps/a/gate.toml:5 [rules]",
            "set their norm and norms in the root gate.toml's [rules]",
        ],
    );
}

#[test]
fn a_core_rule_norm_attaches_to_the_cores_findings() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &root_with(
            "\n[rules]\n\"gate/exceptions\" = { norm = \"D-7\" }\n\"gate/sync\" = { norm = \"D-8\", norms = { hooks = \"D-9\" } }\n",
        ),
    )
    .add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    workspace
        .validate(&declarations, &Accepting)
        .expect("core rules need no plugin declaration");

    let core = |rule: &str, aspect: &str| {
        let mut finding =
            gate_host::Finding::new(&RuleId::parse(rule).expect("id"), Level::Error, "core");
        aspect.clone_into(&mut finding.aspect);
        finding
    };
    let mut findings = vec![
        core("gate/exceptions", ""),
        core("gate/sync", "hooks"),
        core("gate/sync", "ci"),
        core("gate/baseline", ""),
    ];
    gate_host::attach_core_norms(&mut findings, workspace.repository());
    let norms: Vec<Option<&str>> = findings
        .iter()
        .map(|finding| finding.norm.as_deref())
        .collect();
    assert_eq!(norms, [Some("D-7"), Some("D-9"), Some("D-8"), None]);
    assert_eq!(
        findings[0].level,
        Level::Error,
        "the level stays the core's"
    );
}

#[test]
fn rule_filters_name_configured_rules_only() {
    let repo = Repo::new();
    repo.write("gate.toml", WORKSPACE_ROOT)
        .write(
            "apps/a/gate.toml",
            &format!(
                "[project]\nname = \"a\"\n{LAYERS_PLUGIN}\n[rules]\n\"layers/import-direction\" = \"warning\"\n"
            ),
        )
        .add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    let check = |key: &str, known: bool| {
        let filter =
            gate_host::RuleFilter::new(vec![gate_host::RuleFilter::parse(key).expect("key")]);
        let none = |_: &PluginName, _: &PluginSource, _: &str| None;
        let result = if known {
            workspace.check_rule_filter(&filter, &declarations)
        } else {
            workspace.check_rule_filter(&filter, &none)
        };
        result.map_err(|error| {
            assert_eq!(error.kind(), ErrorKind::Config);
            error.message().to_owned()
        })
    };

    assert_eq!(
        check("layers/import-direction", false),
        Ok(()),
        "named in [rules]"
    );
    assert_eq!(
        check("layers/barrel-exports", true),
        Ok(()),
        "declared by the plugin"
    );
    assert_eq!(check("layers/*", false), Ok(()));
    assert_eq!(
        check("layers/barrel-exports", false),
        Err("--rule layers/barrel-exports names no configured rule: no [rules] entry names it and plugin layers declares no rule barrel-exports; gate list prints the configured rules".to_owned())
    );
    assert_eq!(
        check("git/*", true),
        Err("--rule git/* names no configured rule: no gate.toml declares plugin git; gate list prints the configured rules".to_owned())
    );
    assert_eq!(
        check("gate/exceptions", true),
        Err("--rule gate/exceptions: the core's own rules always run; name a plugin's rule or <plugin>/*".to_owned())
    );
}

#[test]
fn preset_of_an_undeclared_plugin() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[project]\nextends = [\"layers/recommended\"]\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:5 [project] extends",
            "\"layers/recommended\"",
            "plugin layers is not declared",
        ],
    );
}

#[test]
fn top_level_extends_points_to_project_and_profiles() {
    let message = error(&[(
        "gate.toml",
        &format!("extends = [\"git/recommended\"]\n{ROOT}"),
    )]);
    assert_all(
        &message,
        &[
            "unknown field `extends`",
            "[project] extends",
            "[[profiles]]",
        ],
    );
}

#[test]
fn exceptions_in_gate_toml_point_to_the_exceptions_file() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[[exceptions]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\n"
        )),
    )]);
    assert_all(
        &message,
        &[
            "unknown field `exceptions`",
            "gate.exceptions.toml",
            "[[exception]]",
        ],
    );
}

fn exceptions_error(exceptions: &str) -> String {
    error(&[
        ("gate.toml", &root_with(GIT_PLUGIN)),
        ("gate.exceptions.toml", exceptions),
    ])
}

#[test]
fn exception_without_reason() {
    let message =
        exceptions_error("[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\n");
    assert_all(
        &message,
        &["gate.exceptions.toml:", "missing field `reason`"],
    );
}

#[test]
fn exception_with_empty_reason() {
    let message = exceptions_error(
        "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"  \"\n",
    );
    assert_all(&message, &["gate.exceptions.toml:1:", "reason is empty"]);
}

#[test]
fn exception_with_an_empty_target_or_owner() {
    let message = exceptions_error(
        "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \" \"\nreason = \"old\"\n",
    );
    assert_all(&message, &["gate.exceptions.toml:1:", "target is empty"]);

    let message = exceptions_error(
        "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\nowner = \"\"\n",
    );
    assert_all(&message, &["gate.exceptions.toml:1:", "owner is empty"]);
}

#[test]
fn a_plugin_entry_is_a_table() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[plugins]\ngit = \"ElsiKora/Gate:git@0.1.0\"\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:5:",
            "a plugin entry is a table; write { source = \"ElsiKora/Gate:git@0.1.0\" }",
        ],
    );
}

#[test]
fn plugin_options_and_options_file_together() {
    let message = error(&[(
        "gate.toml",
        &root_with(
            "\n[plugins.git]\nsource = \"ElsiKora/Gate:git@0.1.0\"\noptions = { main = \"main\" }\noptions-file = \"gate/git.json\"\n",
        ),
    )]);
    assert_all(
        &message,
        &["gate.toml:", "options and options-file are both set"],
    );
}

#[test]
fn a_language_entry_is_a_table() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[languages]\ntypescript = \"tsconfig.json\"\n"),
    )]);
    assert_all(
        &message,
        &["gate.toml:5:", "[languages] typescript: expected a table"],
    );
}

#[test]
fn languages_in_a_workspace_root_belong_to_projects() {
    let message = error(&[(
        "gate.toml",
        &format!("{WORKSPACE_ROOT}\n[languages.typescript]\ntsconfig = \"tsconfig.json\"\n"),
    )]);
    assert_all(
        &message,
        &["gate.toml: [languages] belongs in a project's gate.toml or a [[profiles]] entry"],
    );
}

#[test]
fn exception_with_an_unknown_key() {
    let message = exceptions_error(
        "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\nexpires = \"2027\"\n",
    );
    assert_all(
        &message,
        &["gate.exceptions.toml:5:", "unknown field `expires`"],
    );
}

#[test]
fn exception_of_an_undeclared_plugin() {
    let message = exceptions_error(
        "[[exception]]\nrule = \"layers/barrel-exports\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
    );
    assert_all(
        &message,
        &["gate.exceptions.toml:1:", "plugin layers is not declared"],
    );
}

#[test]
fn exception_without_decision_under_a_required_policy() {
    let message = error(&[
        (
            "gate.toml",
            &root_with(&format!(
                "{GIT_PLUGIN}\n[exception-policy]\nrequire-decision = true\n"
            )),
        ),
        (
            "gate.exceptions.toml",
            "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
        ),
    ]);
    assert_all(
        &message,
        &[
            "gate.exceptions.toml:1:",
            "has no decision",
            "gate.toml [exception-policy] requires one",
        ],
    );
}

#[test]
fn a_project_policy_overrides_the_root_policy() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!("{WORKSPACE_ROOT}{GIT_PLUGIN}\n[exception-policy]\nrequire-decision = true\n"),
    )
    .write(
        "apps/a/gate.toml",
        "[exception-policy]\nrequire-decision = false\n",
    )
    .write(
        "apps/a/gate.exceptions.toml",
        "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
    )
    .add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    assert_eq!(workspace.exceptions().len(), 1);
    assert_eq!(workspace.exceptions()[0].project.as_deref(), Some("apps/a"));
}

#[test]
fn exceptions_file_without_a_gate_toml_beside_it() {
    let message = error(&[
        ("gate.toml", &root_with(GIT_PLUGIN)),
        (
            "apps/a/gate.exceptions.toml",
            "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
        ),
    ]);
    assert_all(
        &message,
        &["apps/a/gate.exceptions.toml has no gate.toml beside it"],
    );
}

#[test]
fn root_only_section_in_a_project_file() {
    let message = error(&[
        ("gate.toml", ROOT),
        (
            "apps/a/gate.toml",
            "[hooks]\npre-commit = [\"check --staged\"]\n",
        ),
    ]);
    assert_all(
        &message,
        &["apps/a/gate.toml:1: [hooks] is allowed only in the repository root"],
    );
}

#[test]
fn project_section_in_a_workspace_root() {
    let message = error(&[(
        "gate.toml",
        &format!("{WORKSPACE_ROOT}\n[project]\nname = \"root\"\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml",
            "[project] belongs in a project's gate.toml",
            "without [workspace]",
            "\".\"",
        ],
    );
}

#[test]
fn profiles_in_a_project_file() {
    let message = error(&[
        ("gate.toml", ROOT),
        (
            "apps/a/gate.toml",
            "[[profiles]]\nprojects = [\"apps/*\"]\n",
        ),
    ]);
    assert_all(
        &message,
        &[
            "apps/a/gate.toml",
            "[[profiles]] is allowed only in the repository root",
        ],
    );
}

#[test]
fn profile_that_matches_no_project() {
    let message = error(&[(
        "gate.toml",
        &format!("{WORKSPACE_ROOT}\n[[profiles]]\nprojects = [\"libs/*\"]\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:7:",
            "[[profiles]] projects = [\"libs/*\"] matches no project",
        ],
    );
}

#[test]
fn profile_without_projects() {
    let message = error(&[(
        "gate.toml",
        &format!("{WORKSPACE_ROOT}\n[[profiles]]\nprojects = []\n"),
    )]);
    assert_all(&message, &["gate.toml:7:", "projects is empty"]);
}

#[test]
fn profile_rule_of_a_plugin_its_project_does_not_declare() {
    let message = error(&[
        (
            "gate.toml",
            &format!(
                "{WORKSPACE_ROOT}\n[[profiles]]\nprojects = [\"apps/*\"]\n\n[profiles.rules]\n\"layers/barrel-exports\" = \"error\"\n"
            ),
        ),
        ("apps/a/index.ts", "export {};\n"),
    ]);
    assert_all(
        &message,
        &[
            "gate.toml:11 [[profiles]] rules",
            "plugin layers is not declared for project apps/a",
        ],
    );
}

#[test]
fn step_glob_without_files() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[steps]\nfmt = { run = \"fmt\", glob = \"*.rs\" }\n"),
    )]);
    assert_all(
        &message,
        &["gate.toml:5:", "[steps] fmt", "add files = \"staged\""],
    );
}

#[test]
fn hooks_budget_of_zero() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[hooks]\nbudget = 0\npre-commit = [\"check --staged\"]\n"),
    )]);
    assert_all(&message, &["gate.toml:4: [hooks] budget is 0"]);
}

#[test]
fn root_file_without_gate_section() {
    let message = error(&[("gate.toml", "[workspace]\nprojects = \"pnpm\"\n")]);
    assert_all(&message, &["gate.toml", "[gate] is missing"]);
}

#[test]
fn malformed_version() {
    let message = error(&[("gate.toml", "[gate]\nversion = \"latest\"\n")]);
    assert_all(&message, &["gate.toml:2:", "\"latest\" is not a version"]);
}

#[test]
fn suite_with_an_undefined_step() {
    let message = error(&[(
        "gate.toml",
        &root_with(
            "\n[steps]\nlint = { run = \"lint .\" }\n\n[suites]\nall = [\"lint\", \"unit\"]\n",
        ),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:8 [suites]",
            "suite all",
            "step unit is not defined for the repository",
        ],
    );
}

#[test]
fn unknown_git_hook() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[hooks]\npre-comit = [\"check --staged\"]\n"),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:",
            "\"pre-comit\" is not a git hook",
            "pre-commit",
        ],
    );
}

#[test]
fn tool_with_a_malformed_checksum() {
    let message = error(&[(
        "gate.toml",
        &root_with(
            "\n[tools.lint]\nversion = \"1.0.0\"\ntargets = { aarch64-apple-darwin = { url = \"https://example.invalid/lint.tar.gz\", sha256 = \"abc\", bin = \"lint\" } }\n",
        ),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:4: [tools] lint.targets.aarch64-apple-darwin",
            "sha256 must be 64 lowercase hex digits",
        ],
    );
}

#[test]
fn options_file_that_is_not_tracked() {
    let message = error(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = {{ options-file = \"gate/subject.json\" }}\n"
        )),
    )]);
    assert_all(
        &message,
        &[
            "gate.toml:8:",
            "options-file",
            "gate/subject.json is not a tracked file",
        ],
    );
}

#[test]
fn options_file_that_is_not_json() {
    let message = error(&[
        (
            "gate.toml",
            &root_with(&format!(
                "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = {{ options-file = \"gate/subject.json\" }}\n"
            )),
        ),
        ("gate/subject.json", "{ types: [] }\n"),
    ]);
    assert_all(&message, &["gate.toml:8:", "gate/subject.json:1:"]);
}

#[test]
fn two_projects_with_one_name() {
    let message = error(&[
        ("gate.toml", ROOT),
        ("apps/a/gate.toml", "[project]\nname = \"same\"\n"),
        ("apps/b/gate.toml", "[project]\nname = \"same\"\n"),
    ]);
    assert_all(
        &message,
        &["two projects are named same", "apps/a", "apps/b"],
    );
}

#[test]
fn pnpm_without_its_workspace_file() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[workspace]\nprojects = \"pnpm\"\n"),
    )]);
    assert_all(&message, &["pnpm-workspace.yaml is not tracked"]);
}

#[test]
fn root_file_that_is_not_tracked() {
    let repo = Repo::new();
    repo.write("README.md", "x\n").add();
    repo.write("gate.toml", ROOT);
    let error = Workspace::open(repo.path(), Source::WorkTree).expect_err("not tracked");
    assert_eq!(error.kind(), ErrorKind::Config);
    assert_all(
        error.message(),
        &["gate.toml is not tracked", "git add gate.toml"],
    );
}

#[test]
fn profiles_and_project_files_merge_field_by_field_with_origins() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        r#"[gate]
version = "0.1.0"

[workspace]
projects = ["apps/*"]

[plugins]
ledger = { source = "ElsiKora/Gate:ledger@0.1.0", reads = ["docs/ledger"] }

[[profiles]]
projects = ["apps/*"]
extends = ["ledger/recommended"]

[profiles.languages.typescript]
tsconfig = "tsconfig.json"
skip-folders = ["dist"]

[profiles.rules]
"ledger/*" = "off"
"ledger/record-status" = { level = "note", norm = "RULE-L-1" }

[profiles.steps]
lint = { run = "lint", timeout = 60, heavy = true, files = "staged", glob = "*.ts" }

[profiles.suites]
static = { steps = ["lint"], fail-fast = true }
"#,
    )
    .write(
        "apps/server/gate.toml",
        r#"[project]
name = "server"
extends = ["layers/recommended"]

[plugins]
layers = { source = "ElsiKora/Gate:layers@0.1.0", options-file = "gate/layers.json" }

[languages.typescript]
tsconfig = "tsconfig.build.json"

[rules]
"layers/import-direction" = { level = "error", norm = "RULE-IMP-001", options = { roots = ["src"] } }
"layers/field-inventory" = { options-file = "gate/fields.json", levels = { hidden = "warning" }, norms = { hidden = "RULE-FLD-002" } }
"ledger/record-status" = "warning"

[generate]
"layers/table" = { options = { out = "docs/table.md" } }
"#,
    )
    .write("apps/server/gate/fields.json", "{\"hidden\": [\"password\"]}\n")
    .write("apps/server/gate/layers.json", "{\"layout\": \"modules\"}\n")
    .write(
        "apps/server/gate.exceptions.toml",
        "[[exception]]\nrule = \"layers/import-direction\"\ntarget = \"src/a.ts:A.b\"\nreason = \"historic\"\ndecision = \"D-1\"\nowner = \"T-9\"\n",
    )
    .write("apps/web/index.ts", "export {};\n")
    .add();

    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    assert!(!workspace.root_is_project());
    let server = workspace.settings("server").expect("server settings");
    let web = workspace.settings("apps/web").expect("web settings");
    let direction = RuleId::parse("layers/import-direction").expect("id");
    let fields = RuleId::parse("layers/field-inventory").expect("id");
    let status = RuleId::parse("ledger/record-status").expect("id");
    let closers = RuleId::parse("ledger/closers").expect("id");

    assert_eq!(
        server.setting(&direction, ""),
        Some(Setting::On(Level::Error))
    );
    assert_eq!(
        server.setting(&fields, ""),
        None,
        "no level: the preset decides"
    );
    assert_eq!(
        server.setting(&fields, "hidden"),
        Some(Setting::On(Level::Warning))
    );
    assert_eq!(
        server.setting(&status, ""),
        Some(Setting::On(Level::Warning)),
        "the project's level replaces the profile's"
    );
    assert_eq!(
        server.norm(&status, ""),
        Some("RULE-L-1"),
        "the profile's norm stays: fields merge one by one"
    );
    assert_eq!(
        server.setting(&closers, ""),
        Some(Setting::Off),
        "the family"
    );
    assert_eq!(server.norm(&fields, "hidden"), Some("RULE-FLD-002"));
    assert_eq!(server.norm(&direction, "any"), Some("RULE-IMP-001"));
    assert_eq!(
        server.options(&fields),
        Some(&serde_json::json!({ "hidden": ["password"] }))
    );
    assert_eq!(
        server.options(&direction),
        Some(&serde_json::json!({ "roots": ["src"] }))
    );
    let presets: Vec<String> = server
        .extends
        .iter()
        .map(|preset| preset.value.to_string())
        .collect();
    assert_eq!(presets, ["ledger/recommended", "layers/recommended"]);
    let plugins: Vec<&str> = server.plugins.keys().map(PluginName::as_str).collect();
    assert_eq!(
        plugins,
        ["layers", "ledger"],
        "the root's plugins and the project's, by name"
    );

    let entry = &server.rules[&status];
    let level = entry.level.as_ref().expect("level");
    let norm = entry.norm.as_ref().expect("norm");
    assert_eq!(level.origin.to_string(), "apps/server/gate.toml:14 [rules]");
    assert_eq!(norm.origin.to_string(), "gate.toml:20 [[profiles]] rules");
    assert_eq!(
        server.plugins["ledger"].origin.to_string(),
        "gate.toml:8 [plugins]"
    );

    let layers = PluginName::try_from("layers".to_owned()).expect("name");
    assert_eq!(
        server.plugin_options(&layers),
        Some(&serde_json::json!({ "layout": "modules" }))
    );
    assert_eq!(
        server.language("typescript"),
        Some(&serde_json::json!({ "tsconfig": "tsconfig.build.json" })),
        "the project's language table replaces the profile's"
    );
    assert_eq!(
        web.language("typescript"),
        Some(&serde_json::json!({ "tsconfig": "tsconfig.json", "skip-folders": ["dist"] }))
    );

    assert_eq!(web.setting(&status, ""), Some(Setting::On(Level::Note)));
    let lint = &web.steps.values().next().expect("a step").value;
    assert!(lint.heavy);
    assert_eq!(lint.glob.as_deref(), Some("*.ts"));
    assert!(web.suites.values().next().expect("a suite").value.fail_fast);

    assert!(workspace.repository().rules.is_empty());
    let exceptions = workspace.exceptions();
    assert_eq!(exceptions.len(), 1);
    assert_eq!(exceptions[0].target, "src/a.ts:A.b");
    assert_eq!(
        exceptions[0].path.as_deref(),
        Some("apps/server/src/a.ts:A.b")
    );
    assert_eq!(exceptions[0].owner.as_deref(), Some("T-9"));
    assert_eq!(exceptions[0].file, "apps/server/gate.exceptions.toml");
    assert_eq!(exceptions[0].line, 1);
    assert_eq!(exceptions[0].project.as_deref(), Some("server"));
}

#[test]
fn a_root_without_workspace_is_the_only_project() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &root_with(&format!(
            "\n[project]\nname = \"tool\"\nextends = [\"git/recommended\"]\n{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = \"error\"\n"
        )),
    )
    .write("src/main.rs", "fn main() {}\n")
    .add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    assert!(workspace.root_is_project());
    let names: Vec<&str> = workspace
        .projects()
        .iter()
        .map(|project| project.name.as_str())
        .collect();
    assert_eq!(names, ["tool"]);
    assert_eq!(
        workspace
            .projects()
            .owner("src/main.rs")
            .map(|project| project.name.as_str()),
        Some("tool")
    );
    let settings = workspace.settings("tool").expect("settings");
    assert_eq!(settings.extends.len(), 1);
    assert!(workspace.project_config("tool").is_some());
}

struct Accepting;

impl SchemaValidator for Accepting {
    fn validate(&self, schema: &serde_json::Value, value: &serde_json::Value) -> Vec<String> {
        let mut violations = Vec::new();
        if let Some(key) = schema.get("forbid").and_then(serde_json::Value::as_str)
            && value.get(key).is_some()
        {
            violations.push(format!("/{key}: not allowed"));
        }
        if let Some(key) = schema.get("require").and_then(serde_json::Value::as_str)
            && value.get(key).is_none()
        {
            violations.push(format!("{key} is required"));
        }
        if let Some(key) = schema.get("string").and_then(serde_json::Value::as_str)
            && value.get(key).is_some_and(|found| !found.is_string())
        {
            violations.push(format!("/{key}: not a string"));
        }
        violations
    }
}

/// Plugin `git` has the repository-scope rule `commit-subject` (aspect `length`); plugin
/// `layers` has the project-scope rules `barrel-exports` (aspect `wildcard`, option `max`
/// forbidden) and `naming` (option `pattern` required).
fn declarations(_: &PluginName, source: &PluginSource, rule: &str) -> Option<RuleDeclaration> {
    match (source.plugin.as_str(), rule) {
        ("git", "commit-subject") => Some(RuleDeclaration {
            scope: RuleScope::Repository,
            aspects: vec!["length".to_owned()],
            options_schema: serde_json::json!({}),
        }),
        ("layers", "barrel-exports") => Some(RuleDeclaration {
            scope: RuleScope::Project,
            aspects: vec!["wildcard".to_owned()],
            options_schema: serde_json::json!({ "forbid": "max" }),
        }),
        ("layers", "naming") => Some(RuleDeclaration {
            scope: RuleScope::Project,
            aspects: vec!["case".to_owned()],
            options_schema: serde_json::json!({ "require": "pattern" }),
        }),
        _ => None,
    }
}

const LAYERS_PLUGIN: &str = "\n[plugins]\nlayers = { source = \"ElsiKora/Gate:layers@0.1.0\" }\n";

fn validate(files: &[(&str, &str)]) -> Result<(), String> {
    let repo = Repo::new();
    for (path, contents) in files {
        repo.write(path, contents);
    }
    repo.add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid files");
    workspace
        .validate(&declarations, &Accepting)
        .map_err(|error| {
            assert_eq!(error.kind(), ErrorKind::Config);
            error.message().to_owned()
        })
}

#[test]
fn validate_accepts_declared_rules_aspects_and_options() {
    validate(&[
        (
            "gate.toml",
            &format!("{WORKSPACE_ROOT}{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = {{ levels = {{ length = \"warning\" }} }}\n"),
        ),
        (
            "apps/a/gate.toml",
            &format!("{LAYERS_PLUGIN}\n[rules]\n\"layers/barrel-exports\" = {{ norms = {{ wildcard = \"N-1\" }}, options = {{ min = 1 }} }}\n"),
        ),
    ])
    .expect("valid");
}

#[test]
fn a_rule_off_in_every_aspect_needs_no_options_and_its_options_are_still_checked() {
    let project = |rule: &str| {
        validate(&[
            ("gate.toml", ROOT),
            (
                "apps/a/gate.toml",
                &format!("{LAYERS_PLUGIN}\n[rules]\n\"layers/naming\" = {rule}\n"),
            ),
        ])
    };
    project("\"off\"").expect("off needs no options");
    project("{ level = \"off\", levels = { case = \"off\" } }")
        .expect("off in every aspect needs no options");
    project("{ norm = \"N-1\" }\n\"layers/*\" = \"off\"")
        .expect("a rule its family turns off needs no options");

    let message = project("\"error\"").expect_err("on needs its option");
    assert_all(
        &message,
        &["\"layers/naming\" options", "pattern is required"],
    );
    let message = project("{ level = \"off\", levels = { case = \"warning\" } }")
        .expect_err("an aspect that is on needs the option");
    assert_all(&message, &["pattern is required"]);
    let message = project("{ level = \"off\", options = { max = 1 } }")
        .expect_err("options an off rule carries are checked");
    assert_all(&message, &["pattern is required"]);
}

#[test]
fn validate_rejects_an_unknown_rule() {
    let message = validate(&[
        ("gate.toml", ROOT),
        (
            "apps/a/gate.toml",
            &format!("{LAYERS_PLUGIN}\n[rules]\n\"layers/barel-exports\" = \"error\"\n"),
        ),
    ])
    .expect_err("unknown rule");
    assert_all(
        &message,
        &[
            "apps/a/gate.toml:6 [rules]",
            "plugin layers has no rule barel-exports",
        ],
    );
}

#[test]
fn validate_rejects_a_project_rule_in_a_workspace_root() {
    let message = validate(&[(
        "gate.toml",
        &format!(
            "{WORKSPACE_ROOT}{LAYERS_PLUGIN}\n[rules]\n\"layers/barrel-exports\" = \"error\"\n"
        ),
    )])
    .expect_err("project rule at the root");
    assert_all(
        &message,
        &[
            "gate.toml:11 [rules]",
            "is a project-scope rule",
            "[[profiles]]",
        ],
    );
}

#[test]
fn validate_accepts_a_project_rule_in_a_single_project_root() {
    validate(&[(
        "gate.toml",
        &root_with(&format!(
            "{LAYERS_PLUGIN}\n[rules]\n\"layers/barrel-exports\" = \"error\"\n"
        )),
    )])
    .expect("the root is the project");
}

#[test]
fn validate_rejects_a_repository_rule_in_a_profile() {
    let message = validate(&[
        (
            "gate.toml",
            &format!("{WORKSPACE_ROOT}{GIT_PLUGIN}\n[[profiles]]\nprojects = [\"apps/*\"]\nrules = {{ \"git/commit-subject\" = \"error\" }}\n"),
        ),
        ("apps/a/index.ts", "export {};\n"),
    ])
    .expect_err("repository rule in a profile");
    assert_all(
        &message,
        &[
            "[[profiles]] rules",
            "is a repository-scope rule",
            "project apps/a",
            "root gate.toml's [rules]",
        ],
    );
}

#[test]
fn an_exception_of_a_plugin_declared_only_elsewhere_is_refused_where_its_file_applies() {
    let a = format!("[project]\nname = \"a\"\n{LAYERS_PLUGIN}");
    let exception = "[[exception]]\nrule = \"layers/barrel-exports\"\ntarget = \"index.ts\"\nreason = \"old\"\n";

    let message = error(&[
        ("gate.toml", WORKSPACE_ROOT),
        ("apps/a/gate.toml", &a),
        ("gate.exceptions.toml", exception),
    ]);
    assert!(
        message.starts_with("gate.exceptions.toml:1: [[exception]] rule \"layers/barrel-exports\": plugin layers is not declared for the repository"),
        "{message}"
    );

    let message = error(&[
        ("gate.toml", WORKSPACE_ROOT),
        ("apps/a/gate.toml", &a),
        ("apps/b/gate.toml", "[project]\nname = \"b\"\n"),
        ("apps/b/gate.exceptions.toml", exception),
    ]);
    assert!(
        message.starts_with("apps/b/gate.exceptions.toml:1: [[exception]] rule \"layers/barrel-exports\": plugin layers is not declared for project b"),
        "{message}"
    );
}

#[test]
fn plugin_reads_are_paths_from_the_repository_root() {
    let message = error(&[
        ("gate.toml", WORKSPACE_ROOT),
        (
            "apps/a/gate.toml",
            "[project]\nname = \"a\"\n\n[plugins]\nlayers = { source = \"ElsiKora/Gate:layers@0.1.0\", reads = [\"../b\"] }\n",
        ),
    ]);
    assert_all(
        &message,
        &[
            "apps/a/gate.toml:",
            "\"../b\" is not a path relative to the repository root; write it from the top of the repository",
        ],
    );
}

#[test]
fn validate_rejects_an_unknown_aspect() {
    let message = validate(&[(
        "gate.toml",
        &root_with(&format!(
            "{GIT_PLUGIN}\n[rules]\n\"git/commit-subject\" = {{ levels = {{ width = \"off\" }} }}\n"
        )),
    )])
    .expect_err("unknown aspect");
    assert_all(
        &message,
        &[
            "gate.toml:8 [rules]",
            "has no aspect width",
            "its aspects are length",
        ],
    );
}

#[test]
fn validate_rejects_options_that_break_the_schema() {
    let message = validate(&[(
        "gate.toml",
        &root_with(&format!(
            "{LAYERS_PLUGIN}\n[rules]\n\"layers/barrel-exports\" = {{ options = {{ max = 3 }} }}\n"
        )),
    )])
    .expect_err("options");
    assert_all(
        &message,
        &["gate.toml:8 [rules]", "options: /max: not allowed"],
    );
}

#[test]
fn validate_rejects_an_exception_for_an_unknown_aspect() {
    let message = validate(&[
        ("gate.toml", &root_with(GIT_PLUGIN)),
        (
            "gate.exceptions.toml",
            "[[exception]]\nrule = \"git/commit-subject#width\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
        ),
    ])
    .expect_err("unknown aspect");
    assert_all(
        &message,
        &["gate.exceptions.toml:1:", "has no aspect width"],
    );
}

#[test]
fn schema_covers_every_section_and_has_no_null() {
    let schema = gate_host::config::json_schema();
    let properties = schema["properties"].as_object().expect("properties");
    for section in [
        "gate",
        "workspace",
        "project",
        "plugins",
        "rules",
        "generate",
        "steps",
        "suites",
        "exception-policy",
        "profiles",
        "run",
        "hooks",
        "ci",
        "tools",
    ] {
        assert!(properties.contains_key(section), "schema lacks {section}");
    }
    assert!(!properties.contains_key("exceptions"));
    assert!(!properties.contains_key("extends"));
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert!(!schema.to_string().contains("null"));

    let exceptions = gate_host::config::exceptions_json_schema();
    assert!(exceptions["properties"]["exception"].is_object());
    assert!(!exceptions.to_string().contains("null"));
}

#[test]
fn a_later_family_level_overrides_the_levels_earlier_layers_gave_its_rules() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &format!(
            "{WORKSPACE_ROOT}{LAYERS_PLUGIN}\n[[profiles]]\nprojects = [\"apps/*\"]\n\n[profiles.rules]\n\"layers/field\" = \"warning\"\n\"layers/kept\" = \"warning\"\n"
        ),
    )
    .write(
        "apps/a/gate.toml",
        "[rules]\n\"layers/*\" = \"off\"\n\"layers/kept\" = \"error\"\n",
    )
    .add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    let settings = workspace.settings("apps/a").expect("settings");
    assert_eq!(
        settings.setting(&RuleId::parse("layers/field").expect("id"), ""),
        Some(Setting::Off),
        "the project's family level overrides the profile's rule level"
    );
    assert_eq!(
        settings.setting(&RuleId::parse("layers/kept").expect("id"), ""),
        Some(Setting::On(Level::Error)),
        "a rule level in the same layer stays more specific than its family"
    );
}

#[test]
fn validate_puts_each_exception_in_the_file_of_its_rules_scope() {
    let message = validate(&[
        ("gate.toml", &format!("{WORKSPACE_ROOT}{LAYERS_PLUGIN}")),
        (
            "gate.exceptions.toml",
            "[[exception]]\nrule = \"layers/barrel-exports\"\ntarget = \"apps/a/x.ts\"\nreason = \"old\"\n",
        ),
        ("apps/a/index.ts", "export {};\n"),
    ])
    .expect_err("project rule excepted at a workspace root");
    assert_all(
        &message,
        &[
            "gate.exceptions.toml:1:",
            "is a project-scope rule",
            "beside the project's gate.toml",
        ],
    );

    let message = validate(&[
        ("gate.toml", &format!("{WORKSPACE_ROOT}{GIT_PLUGIN}")),
        ("apps/a/gate.toml", ""),
        (
            "apps/a/gate.exceptions.toml",
            "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
        ),
    ])
    .expect_err("repository rule excepted in a project");
    assert_all(
        &message,
        &[
            "apps/a/gate.exceptions.toml:1:",
            "is a repository-scope rule",
            "root gate.exceptions.toml",
        ],
    );
}

#[test]
fn the_roots_exceptions_belong_to_the_root_project_when_the_top_is_one() {
    let repo = Repo::new();
    repo.write("gate.toml", &root_with(GIT_PLUGIN))
        .write(
            "gate.exceptions.toml",
            "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"a.txt\"\nreason = \"old\"\n",
        )
        .add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid");
    let root = workspace
        .projects()
        .iter()
        .find(|project| project.root.is_empty())
        .expect("the top is a project");
    assert_eq!(
        workspace.exceptions()[0].project.as_deref(),
        Some(root.name.as_str())
    );
}

#[test]
fn exception_target_that_leaves_its_directory() {
    let message = error(&[
        ("gate.toml", &format!("{WORKSPACE_ROOT}{GIT_PLUGIN}")),
        ("apps/a/gate.toml", ""),
        (
            "apps/a/gate.exceptions.toml",
            "[[exception]]\nrule = \"git/commit-subject\"\ntarget = \"../b/x.ts\"\nreason = \"old\"\n",
        ),
    ]);
    assert_all(
        &message,
        &["apps/a/gate.exceptions.toml:1:", "leaves the directory"],
    );
}

/// Plugin `layers` declares `[plugins.layers] options` with `root` required and a string, and
/// `colour` forbidden; no other plugin declares a schema.
fn plugin_schema(name: &PluginName) -> Option<serde_json::Value> {
    (name.as_str() == "layers")
        .then(|| serde_json::json!({ "require": "root", "string": "root", "forbid": "colour" }))
}

/// Checks the plugin options of a workspace whose project `apps/a` declares `layers` with
/// `entry`, and whose `gate/layers.json` holds `file`.
fn validate_plugin_options(entry: &str, file: &str) -> Result<(), String> {
    let repo = Repo::new();
    repo.write("gate.toml", WORKSPACE_ROOT);
    repo.write(
        "apps/a/gate.toml",
        &format!("[plugins.layers]\nsource = \"ElsiKora/Gate:layers@0.1.0\"\n{entry}"),
    );
    repo.write("apps/a/gate/layers.json", file);
    repo.add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid files");
    workspace
        .validate_plugin_options(&plugin_schema, &Accepting)
        .map_err(|error| {
            assert_eq!(error.kind(), ErrorKind::Config);
            error.message().to_owned()
        })
}

#[test]
fn plugin_options_are_checked_against_the_plugins_schema() {
    validate_plugin_options("options = { root = \"src\" }\n", "{}").expect("valid inline");
    validate_plugin_options(
        "options-file = \"gate/layers.json\"\n",
        r#"{ "root": "src" }"#,
    )
    .expect("valid from the options file");

    let missing = validate_plugin_options("", "{}").expect_err("a required option is missing");
    assert_all(
        &missing,
        &[
            "apps/a/gate.toml:",
            "plugin layers options",
            "root is required",
        ],
    );

    let unknown = validate_plugin_options("options = { root = \"src\", colour = \"red\" }\n", "{}")
        .expect_err("an unknown option");
    assert_all(
        &unknown,
        &[
            "apps/a/gate.toml:",
            "plugin layers options",
            "/colour: not allowed",
        ],
    );

    let wrong = validate_plugin_options("options = { root = 5 }\n", "{}")
        .expect_err("an option of the wrong type");
    assert_all(
        &wrong,
        &[
            "apps/a/gate.toml:",
            "plugin layers options",
            "/root: not a string",
        ],
    );

    let from_file =
        validate_plugin_options("options-file = \"gate/layers.json\"\n", r#"{ "root": 5 }"#)
            .expect_err("an option of the wrong type in the options file");
    assert_all(
        &from_file,
        &["(from apps/a/gate/layers.json)", "/root: not a string"],
    );
}

#[test]
fn plugin_options_are_checked_in_the_root_and_whatever_the_rules_levels() {
    let check = |root: &str, project: &str| {
        let repo = Repo::new();
        repo.write("gate.toml", &format!("{WORKSPACE_ROOT}{root}"));
        repo.write("apps/a/gate.toml", project);
        repo.add();
        let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid files");
        workspace
            .validate_plugin_options(&plugin_schema, &Accepting)
            .map_err(|error| {
                assert_eq!(error.kind(), ErrorKind::Config);
                error.message().to_owned()
            })
    };
    let entry = |options: &str| {
        format!("\n[plugins.layers]\nsource = \"ElsiKora/Gate:layers@0.1.0\"\n{options}")
    };

    check(
        &entry("options = { root = \"src\" }\n"),
        "[project]\nname = \"a\"\n",
    )
    .expect("a valid root entry");
    let root = check(&entry(""), "[project]\nname = \"a\"\n").expect_err("the root entry");
    assert_all(
        &root,
        &["gate.toml:", "plugin layers options", "root is required"],
    );
    assert!(!root.contains("apps/a"), "{root}");

    let off = check(
        "",
        &format!(
            "{}\n[rules]\n\"layers/*\" = \"off\"\n",
            entry("options = { colour = \"red\" }\n")
        ),
    )
    .expect_err("the plugin's options belong to the plugin, not to its rules");
    assert_all(&off, &["apps/a/gate.toml:", "/colour: not allowed"]);
}

#[test]
fn a_plugin_that_declares_no_schema_is_not_checked() {
    let repo = Repo::new();
    repo.write(
        "gate.toml",
        &root_with(
            "\n[plugins.git]\nsource = \"ElsiKora/Gate:git@0.1.0\"\noptions = { anything = 1 }\n",
        ),
    );
    repo.add();
    let workspace = Workspace::open(repo.path(), Source::WorkTree).expect("valid files");
    workspace
        .validate_plugin_options(&plugin_schema, &Accepting)
        .expect("git did not load here, so its options are not known");
}

#[test]
fn a_tool_url_is_https_or_plain_http_to_this_machine_as_in_gate_lock() {
    let tool = |url: &str| {
        root_with(&format!(
            "\n[tools.lint]\nversion = \"1.0.0\"\ntargets = {{ aarch64-apple-darwin = {{ url = \"{url}\", sha256 = \"{}\" }} }}\n",
            "a".repeat(64)
        ))
    };
    for url in [
        "https://example.invalid/lint",
        "http://127.0.0.1:8080/lint",
        "http://localhost/lint",
        "http://[::1]:8080/lint",
    ] {
        let repo = Repo::new();
        repo.write("gate.toml", &tool(url));
        repo.add();
        Workspace::open(repo.path(), Source::WorkTree).expect(url);
    }
    for (url, reason) in [
        (
            "http://example.invalid/lint",
            "is not an https URL; gate downloads over https only (plain http only to this machine)",
        ),
        (
            "http://localhost.example/lint",
            "is not an https URL; gate downloads over https only (plain http only to this machine)",
        ),
        (
            "http://127.0.0.1@evil.example/lint",
            "names a user before its host (`@`); gate fetches no URL with user information",
        ),
    ] {
        let message = error(&[("gate.toml", &tool(url))]);
        assert_all(
            &message,
            &["[tools] lint.targets.aarch64-apple-darwin", reason],
        );
    }
}

#[test]
fn a_step_that_names_an_undeclared_tool_fails_at_load() {
    let message = error(&[(
        "gate.toml",
        &root_with("\n[steps]\nlint = { run = \"lint .\", tools = [\"lint\"] }\n"),
    )]);
    assert_all(
        &message,
        &[
            "step lint: tool lint is not declared",
            "add [tools.lint] to the root gate.toml or remove it from tools",
        ],
    );
}
