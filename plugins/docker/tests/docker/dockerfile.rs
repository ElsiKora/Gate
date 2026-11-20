//! `docker/dockerfile`, one aspect at a time, each with a passing twin.

use crate::fixture::{Fixture, rows, texts};

const RULE: &str = "dockerfile";

fn digest() -> String {
    "a".repeat(64)
}

/// A Dockerfile that passes every aspect: a pinned build stage, a runtime stage that inherits
/// nothing and sets its user and labels.
fn good() -> String {
    let digest = digest();
    format!(
        "ARG VERSION=0.0.0\n\
         FROM node@sha256:{digest} AS build\n\
         RUN --mount=type=secret,id=npm_token npm ci\n\
         FROM node@sha256:{digest} AS runtime\n\
         COPY --from=build /app /app\n\
         LABEL org.opencontainers.image.source=\"x\" org.opencontainers.image.revision=\"y\" \\\n\
               org.opencontainers.image.version=\"$VERSION\"\n\
         USER node\n"
    )
}

const IGNORE: &str = "**\n!src\n";

/// The rows of `aspect` for a tree with `Dockerfile` and its allow-list ignore file.
fn aspect(dockerfile: &str, aspect: &str) -> Vec<String> {
    let fixture = Fixture::new(&[
        ("Dockerfile", dockerfile),
        ("Dockerfile.dockerignore", IGNORE),
    ]);
    fixture
        .rows(RULE)
        .into_iter()
        .filter(|row| row.starts_with(&format!("{aspect} ")))
        .collect()
}

#[test]
fn the_standard_dockerfile_passes() {
    let fixture = Fixture::new(&[("Dockerfile", &good()), ("Dockerfile.dockerignore", IGNORE)]);
    assert_eq!(fixture.rows(RULE), Vec::<String>::new());
}

#[test]
fn pinned_reports_a_tag_an_unpinned_argument_default_and_an_unlisted_registry() {
    let digest = digest();
    let text = format!(
        "ARG BASE=node:24-alpine\n\
         FROM node:24-alpine AS build\n\
         FROM ${{BASE}} AS other\n\
         FROM ${{MISSING}} AS third\n\
         FROM build AS runtime\n\
         FROM scratch AS empty\n\
         FROM node@sha256:{digest} AS pinned\n"
    );
    assert_eq!(
        aspect(&text, "pinned"),
        [
            "pinned Dockerfile:2 node:24-alpine",
            "pinned Dockerfile:3 ${BASE}",
            "pinned Dockerfile:4 ${MISSING}"
        ]
    );

    let pinned =
        format!("FROM node@sha256:{digest} AS a\nFROM mirror.gcr.io/node@sha256:{digest}\n");
    let fixture = Fixture::new(&[("Dockerfile", &pinned)]);
    let listed = r#"{"registries": ["mirror.gcr.io"]}"#;
    let findings = fixture.check_with(RULE, "{}", listed).expect("runs");
    let pinned_rows: Vec<String> = rows(&findings)
        .into_iter()
        .filter(|row| row.starts_with("pinned "))
        .collect();
    assert_eq!(
        pinned_rows,
        [format!("pinned Dockerfile:1 node@sha256:{digest}")]
    );
}

#[test]
fn stages_reports_one_stage_and_two_pass() {
    let digest = digest();
    assert_eq!(
        aspect(&format!("FROM node@sha256:{digest}\nUSER node\n"), "stages"),
        ["stages Dockerfile:1 stages"]
    );
    assert_eq!(aspect(&good(), "stages"), Vec::<String>::new());
}

#[test]
fn user_reports_each_runtime_stage_without_a_user_that_is_not_root() {
    let digest = digest();
    let from = format!("FROM node@sha256:{digest}");
    let text = format!(
        "{from} AS base\nUSER node\n\
         {from} AS build\n\
         FROM base AS inherits\n\
         {from} AS none\n\
         {from} AS root\nUSER root\n\
         {from} AS zero\nUSER 0:0\n\
         FROM build AS copier\nCOPY --from=build /a /a\nUSER app\n"
    );
    assert_eq!(
        aspect(&text, "user"),
        [
            "user Dockerfile:5 none",
            "user Dockerfile:6 root",
            "user Dockerfile:8 zero"
        ]
    );
}

#[test]
fn healthcheck_reports_any_healthcheck() {
    let checked = format!("{}HEALTHCHECK CMD curl -f http://localhost/\n", good());
    let none = format!("{}HEALTHCHECK NONE\n", good());
    assert_eq!(
        aspect(&checked, "healthcheck"),
        ["healthcheck Dockerfile:9 healthcheck"]
    );
    assert_eq!(
        aspect(&none, "healthcheck"),
        ["healthcheck Dockerfile:9 healthcheck"]
    );
    assert_eq!(aspect(&good(), "healthcheck"), Vec::<String>::new());
}

#[test]
fn labels_reports_a_missing_oci_label_and_any_other_label() {
    let digest = digest();
    let text = format!(
        "FROM node@sha256:{digest} AS base\n\
         LABEL org.opencontainers.image.source=x org.opencontainers.image.revision=y org.opencontainers.image.version=z\n\
         FROM base AS inherits\nUSER node\n\
         FROM node@sha256:{digest} AS partial\nUSER node\n\
         LABEL org.opencontainers.image.source=x org.opencontainers.image.revision=y\n\
         LABEL org.example.release-id=1\n"
    );
    let found = aspect(&text, "labels");
    assert_eq!(
        found,
        [
            "labels Dockerfile:5 partial#missing",
            "labels Dockerfile:8 org.example.release-id"
        ]
    );
    let fixture = Fixture::new(&[("Dockerfile", &text)]);
    let findings = fixture.check_with(RULE, "{}", "").expect("runs");
    assert!(
        texts(&findings).contains("does not set org.opencontainers.image.version"),
        "{}",
        texts(&findings)
    );
}

#[test]
fn ignore_reports_a_missing_file_and_a_missing_pattern_and_a_dockerignore_under_name() {
    let text = good();
    let missing = Fixture::new(&[("api/Dockerfile", &text)]);
    assert_eq!(missing.rows(RULE), ["ignore api/Dockerfile:0 missing"]);

    let partial = Fixture::new(&[
        ("Dockerfile", &text),
        (
            "Dockerfile.dockerignore",
            ".git\n.env.*\n*.env\n**/*.pem\n/*.key\n",
        ),
    ]);
    assert_eq!(
        partial.rows(RULE),
        ["ignore Dockerfile.dockerignore:0 .env"]
    );

    let excluding = Fixture::new(&[
        ("Dockerfile", &text),
        (
            "Dockerfile.dockerignore",
            "# keep out\n.git\n.env\n.env.*\n*.env\n*.pem\n*.key\n",
        ),
    ]);
    assert_eq!(excluding.rows(RULE), Vec::<String>::new());

    let root = Fixture::new(&[
        ("Dockerfile", &text),
        ("Dockerfile.dockerignore", IGNORE),
        (".dockerignore", ".git\n"),
        ("api/.dockerignore", ".git\n"),
    ]);
    assert_eq!(
        root.rows(RULE),
        [
            "name .dockerignore:0 .dockerignore",
            "name api/.dockerignore:0 .dockerignore"
        ]
    );
}

#[test]
fn secret_reports_secret_named_arguments_and_variables_without_their_values() {
    let text = format!(
        "ARG NPM_TOKEN\n{}ENV DB_PASSWORD=hunter2-value\nARG api_key\nARG RELEASE_ID\n",
        good()
    );
    assert_eq!(
        aspect(&text, "secret"),
        [
            "secret Dockerfile:1 NPM_TOKEN",
            "secret Dockerfile:10 DB_PASSWORD",
            "secret Dockerfile:11 api_key"
        ]
    );
    let fixture = Fixture::new(&[("Dockerfile", &text), ("Dockerfile.dockerignore", IGNORE)]);
    let findings = fixture.check_with(RULE, "{}", "").expect("runs");
    assert!(
        !texts(&findings).contains("hunter2-value"),
        "{}",
        texts(&findings)
    );
}

#[test]
fn name_reports_image_files_not_named_dockerfile() {
    let text = good();
    let fixture = Fixture::new(&[
        ("Dockerfile.api", &text),
        ("Dockerfile.api.dockerignore", IGNORE),
        ("worker.Dockerfile", &text),
        ("worker.Dockerfile.dockerignore", IGNORE),
        ("tools/dockerfile", &text),
        ("tools/dockerfile.dockerignore", IGNORE),
    ]);
    assert_eq!(
        fixture.rows(RULE),
        [
            "name Dockerfile.api:0 Dockerfile.api",
            "name tools/dockerfile:0 dockerfile",
            "name worker.Dockerfile:0 worker.Dockerfile"
        ]
    );
}
