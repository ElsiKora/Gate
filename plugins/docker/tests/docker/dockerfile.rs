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
        ("tools/Containerfile", &text),
        ("tools/Containerfile.dockerignore", IGNORE),
    ]);
    assert_eq!(
        fixture.rows(RULE),
        [
            "name Dockerfile.api:0 Dockerfile.api",
            "name tools/Containerfile:0 Containerfile",
            "name worker.Dockerfile:0 worker.Dockerfile"
        ]
    );
}

#[test]
fn a_file_is_an_image_file_only_by_the_image_files_names_compared_case_sensitively() {
    let text = good();
    let fixture = Fixture::new(&[
        ("dockerfile.md", "# notes\n"),
        ("rules/dockerfile.md", "# notes\n"),
        ("src/dockerfile.ts", "export {};\n"),
        ("src/Dockerfile.test.ts", "export {};\n"),
        ("tools/dockerfile", &text),
        ("a.b.Dockerfile", &text),
        ("api.containerfile", &text),
    ]);
    assert_eq!(fixture.rows(RULE), Vec::<String>::new());

    let named = r#"{"image-files": ["Dockerfile", "*.containerfile"]}"#;
    let findings = fixture.check_with(RULE, "{}", named).expect("runs");
    assert_eq!(
        rows(&findings),
        [
            "ignore api.containerfile:0 missing",
            "name api.containerfile:0 api.containerfile"
        ]
    );
}

#[test]
fn an_image_copied_or_mounted_from_needs_a_digest_and_a_stage_name_passes() {
    let digest = digest();
    let text = format!(
        "FROM node@sha256:{digest} AS build\n\
         RUN --mount=type=cache,target=/cache,from=busybox:1 true\n\
         RUN --mount=type=bind,from=build,target=/b true\n\
         FROM node@sha256:{digest} AS runtime\n\
         COPY --from=nginx:1.27 /etc/nginx /etc/nginx\n\
         COPY --from=nginx:1.27 /usr/share/nginx /usr/share/nginx\n\
         COPY --from=build /app /app\n\
         COPY --from=0 /app /app\n\
         COPY --from=nginx@sha256:{digest} /etc/x /etc/x\n\
         USER node\n"
    );
    assert_eq!(
        aspect(&text, "pinned"),
        [
            "pinned Dockerfile:2 busybox:1",
            "pinned Dockerfile:5 nginx:1.27"
        ]
    );
}

#[test]
fn ignore_takes_a_star_allow_list_and_reports_a_pattern_taken_back_in() {
    let text = good();
    let star = Fixture::new(&[
        ("Dockerfile", &text),
        ("Dockerfile.dockerignore", "*\n!src\n"),
    ]);
    assert_eq!(star.rows(RULE), Vec::<String>::new());

    let taken_back = Fixture::new(&[
        ("Dockerfile", &text),
        ("Dockerfile.dockerignore", "**\n!src\n!.env\n"),
    ]);
    assert_eq!(
        taken_back.rows(RULE),
        ["ignore Dockerfile.dockerignore:3 .env"]
    );

    let listed = ".git\n.env\n.env.*\n*.env\n*.pem\n*.key\n";
    let excluded_then_back = Fixture::new(&[
        ("Dockerfile", &text),
        ("Dockerfile.dockerignore", &format!("{listed}!*.pem\n")),
    ]);
    assert_eq!(
        excluded_then_back.rows(RULE),
        ["ignore Dockerfile.dockerignore:7 *.pem"]
    );

    let back_then_excluded = Fixture::new(&[
        ("Dockerfile", &text),
        ("Dockerfile.dockerignore", &format!("{listed}!.env\n.env\n")),
    ]);
    assert_eq!(back_then_excluded.rows(RULE), Vec::<String>::new());
}

#[test]
fn secret_names_match_whole_words_and_a_metadata_last_word_passes() {
    let text = format!(
        "{}ARG AUTHOR\nARG NEXTAUTH_URL\nARG TOKEN_TTL\nARG POSTGRES_PASSWORD_FILE\nARG SECRET_NAME\n\
         ARG NEXTAUTH_SECRET\nARG GITHUB_TOKEN\nARG AWS_SECRET_ACCESS_KEY\nARG SECRET_KEY_BASE\nARG AUTH\n",
        good()
    );
    assert_eq!(
        aspect(&text, "secret"),
        [
            "secret Dockerfile:14 NEXTAUTH_SECRET",
            "secret Dockerfile:15 GITHUB_TOKEN",
            "secret Dockerfile:16 AWS_SECRET_ACCESS_KEY",
            "secret Dockerfile:17 SECRET_KEY_BASE",
            "secret Dockerfile:18 AUTH"
        ]
    );
}

#[test]
fn a_secret_mounted_into_one_step_passes_where_the_same_build_argument_is_reported() {
    let digest = digest();
    let stage = |step: &str| {
        format!(
            "FROM node@sha256:{digest} AS build\n{step}\nFROM node@sha256:{digest} AS runtime\n\
             LABEL org.opencontainers.image.source=\"x\" org.opencontainers.image.revision=\"y\" \
             org.opencontainers.image.version=\"z\"\nUSER node\n"
        )
    };
    let mounted = stage("RUN --mount=type=secret,id=NPM_TOKEN,env=NPM_TOKEN npm ci");
    let argument = stage("ARG NPM_TOKEN\nRUN NPM_TOKEN=$NPM_TOKEN npm ci");
    assert_eq!(aspect(&mounted, "secret"), Vec::<String>::new());
    assert_eq!(
        aspect(&argument, "secret"),
        ["secret Dockerfile:2 NPM_TOKEN"]
    );
}
