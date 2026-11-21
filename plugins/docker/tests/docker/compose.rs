//! `docker/compose`, one aspect at a time, each with a passing twin.

use crate::fixture::{Fixture, texts};

const RULE: &str = "compose";

fn image() -> String {
    format!("postgres@sha256:{}", "b".repeat(64))
}

/// The rows of `aspect` for `text` as the file `name`.
fn aspect(name: &str, text: &str, aspect: &str) -> Vec<String> {
    Fixture::new(&[(name, text)])
        .rows(RULE)
        .into_iter()
        .filter(|row| row.starts_with(&format!("{aspect} ")))
        .collect()
}

/// One service of the end-to-end purpose, as the standard writes it.
fn good() -> String {
    format!(
        "services:\n  app-e2e-postgres:\n    image: {}\n    environment:\n      POSTGRES_PASSWORD: ${{POSTGRES_PASSWORD}}\n    ports:\n      - \"127.0.0.1::5432\"\n",
        image()
    )
}

#[test]
fn the_standard_compose_file_passes() {
    assert_eq!(
        Fixture::new(&[("docker-compose.e2e.yml", &good())]).rows(RULE),
        Vec::<String>::new()
    );
}

#[test]
fn name_reports_files_without_a_purpose_and_passes_a_scenario() {
    let text = good();
    let fixture = Fixture::new(&[
        ("compose.yaml", "services: {}\n"),
        ("docker-compose.yml", "services: {}\n"),
        ("docker-compose.dev.yml", "services: {}\n"),
        ("docker-compose.e2e.yaml", "services: {}\n"),
        ("docker-compose.e2e.yml", &text),
        ("docker-compose.load-test.e2e.yml", &text),
    ]);
    assert_eq!(
        fixture.rows(RULE),
        [
            "name compose.yaml:0 compose.yaml",
            "name docker-compose.dev.yml:0 docker-compose.dev.yml",
            "name docker-compose.e2e.yaml:0 docker-compose.e2e.yaml",
            "name docker-compose.yml:0 docker-compose.yml"
        ]
    );
}

#[test]
fn service_names_carry_owner_purpose_and_role() {
    let text = format!(
        "services:\n  postgres:\n    image: {0}\n  app-postgres:\n    image: {0}\n  appE2ePostgres:\n    image: {0}\n  app-development-postgres:\n    image: {0}\n  app-e2e-postgres:\n    image: {0}\n",
        image()
    );
    assert_eq!(
        aspect("docker-compose.e2e.yml", &text, "service"),
        [
            "service docker-compose.e2e.yml:2 postgres#name",
            "service docker-compose.e2e.yml:4 app-postgres#name",
            "service docker-compose.e2e.yml:6 appE2ePostgres#name",
            "service docker-compose.e2e.yml:8 app-development-postgres#name"
        ]
    );
}

#[test]
fn container_name_is_reported() {
    let text = format!("{}    container_name: fixed\n", good());
    assert_eq!(
        aspect("docker-compose.e2e.yml", &text, "container-name"),
        ["container-name docker-compose.e2e.yml:8 app-e2e-postgres"]
    );
}

#[test]
fn pinned_reports_a_tag_a_free_variable_and_an_unpinned_default() {
    let text = format!(
        "services:\n  app-e2e-a:\n    image: postgres:17.7-alpine\n  app-e2e-b:\n    image: ${{IMAGE}}\n  app-e2e-c:\n    image: ${{IMAGE:-postgres:17}}\n  app-e2e-d:\n    image: {}\n  app-e2e-e:\n    image: ${{IMAGE:?set the image}}\n  app-e2e-f:\n    build: .\n",
        image()
    );
    assert_eq!(
        aspect("docker-compose.e2e.yml", &text, "pinned"),
        [
            "pinned docker-compose.e2e.yml:3 app-e2e-a",
            "pinned docker-compose.e2e.yml:5 app-e2e-b",
            "pinned docker-compose.e2e.yml:7 app-e2e-c"
        ]
    );
}

#[test]
fn secret_reports_literals_outside_end_to_end_files_without_the_value() {
    let text = "services:\n  app-development-postgres:\n    build: .\n    environment:\n      POSTGRES_PASSWORD: literal-value-1\n      API_TOKEN: ${API_TOKEN:-literal-value-2}\n      DB_PASSWORD: ${DB_PASSWORD}\n      POSTGRES_PASSWORD_FILE: /run/secrets/x\n      PLAIN: literal\n    secrets:\n      - db\n  app-development-cache:\n    build: .\n    environment:\n      - REDIS_PASSWORD=literal-value-3\n      - REDIS_HOST=cache\n";
    let findings = Fixture::new(&[("docker-compose.development.yml", text)])
        .check_with(RULE, "{}", "")
        .expect("runs");
    let rows: Vec<String> = crate::fixture::rows(&findings)
        .into_iter()
        .filter(|row| row.starts_with("secret "))
        .collect();
    assert_eq!(
        rows,
        [
            "secret docker-compose.development.yml:15 app-development-cache.REDIS_PASSWORD",
            "secret docker-compose.development.yml:5 app-development-postgres.POSTGRES_PASSWORD",
            "secret docker-compose.development.yml:6 app-development-postgres.API_TOKEN"
        ]
    );
    let all = texts(&findings);
    assert!(!all.contains("literal-value"), "{all}");

    for name in ["docker-compose.e2e.yml", "docker-compose.load.e2e.yml"] {
        let text = text.replace("development", "e2e");
        assert_eq!(
            aspect(name, &text, "secret"),
            Vec::<String>::new(),
            "{name}"
        );
    }
}

#[test]
fn healthcheck_is_needed_by_a_service_others_wait_for_as_healthy() {
    let text = "services:\n  app-e2e-postgres:\n    build: .\n  app-e2e-api:\n    build: .\n    depends_on:\n      app-e2e-postgres:\n        condition: service_healthy\n      app-e2e-cache:\n        condition: service_started\n  app-e2e-cache:\n    build: .\n";
    assert_eq!(
        aspect("docker-compose.e2e.yml", text, "healthcheck"),
        ["healthcheck docker-compose.e2e.yml:2 app-e2e-postgres"]
    );
    let checked = text.replace(
        "  app-e2e-postgres:\n    build: .\n",
        "  app-e2e-postgres:\n    build: .\n    healthcheck:\n      test: [\"CMD\", \"pg_isready\"]\n",
    );
    assert_eq!(
        aspect("docker-compose.e2e.yml", &checked, "healthcheck"),
        Vec::<String>::new()
    );
}

#[test]
fn port_is_published_on_the_loopback_address_only() {
    let text = "services:\n  app-e2e-postgres:\n    build: .\n    ports:\n      - \"5432:5432\"\n      - \"0.0.0.0:5432:5432\"\n      - \"127.0.0.1::5432\"\n      - \"[::1]:5432:5432\"\n";
    assert_eq!(
        aspect("docker-compose.e2e.yml", text, "port"),
        [
            "port docker-compose.e2e.yml:5 app-e2e-postgres#5432:5432",
            "port docker-compose.e2e.yml:6 app-e2e-postgres#0.0.0.0:5432:5432"
        ]
    );
}

/// The help of the `port` finding of each entry of `ports`, in order; `None` for an entry that
/// passes.
fn port_helps(ports: &[&str]) -> Vec<Option<String>> {
    ports
        .iter()
        .map(|port| {
            let text = format!(
                "services:\n  app-e2e-postgres:\n    build: .\n    ports:\n      - {port}\n"
            );
            let findings = Fixture::new(&[("docker-compose.e2e.yml", &text)])
                .check_with(RULE, "{}", "")
                .expect("the rule runs");
            findings
                .iter()
                .find(|finding| finding.aspect == "port")
                .map(|finding| finding.help.clone().unwrap_or_default())
        })
        .collect()
}

#[test]
fn the_port_help_keeps_the_host_and_container_ports() {
    let suggest = |published: &str| {
        Some(format!(
            "publish on 127.0.0.1 (or [::1]) with the same ports: `\"{published}\"`"
        ))
    };
    assert_eq!(
        port_helps(&[
            "\"5432:5432\"",
            "\"5432\"",
            "\"8000-8010:8000-8010\"",
            "\"9000-9010\"",
            "\"0.0.0.0:5432:5432\"",
            "\"0.0.0.0::5432\"",
            "\"[::]:6379:6379\"",
            "\"53:53/udp\"",
            "\"${DB_PORT:-5432}:5432\"",
            "\"::1:5432:5432\"",
        ]),
        [
            suggest("127.0.0.1:5432:5432"),
            suggest("127.0.0.1::5432"),
            suggest("127.0.0.1:8000-8010:8000-8010"),
            suggest("127.0.0.1::9000-9010"),
            suggest("127.0.0.1:5432:5432"),
            suggest("127.0.0.1::5432"),
            suggest("127.0.0.1:6379:6379"),
            suggest("127.0.0.1:53:53/udp"),
            suggest("127.0.0.1:${DB_PORT:-5432}:5432"),
            // An IPv6 address without brackets is read no other way: the forms themselves.
            Some(
                "publish on 127.0.0.1 (or [::1]) with the same ports: `\"127.0.0.1:<host>:<container>\"`, or `\"127.0.0.1::<container>\"` for a random host port"
                    .to_owned()
            ),
        ]
    );
    // The suggestions themselves pass, and the long form is told to set host_ip.
    assert_eq!(
        port_helps(&[
            "\"127.0.0.1:5432:5432\"",
            "\"127.0.0.1::9000-9010\"",
            "\"127.0.0.1:${DB_PORT:-5432}:5432\"",
            "\"127.0.0.1::${DB_PORT:-5432}\"",
        ]),
        [None, None, None, None]
    );
    let long = port_helps(&["target: 5432\n        published: \"5432\""]);
    assert!(
        long[0]
            .as_deref()
            .is_some_and(|help| help.starts_with("set `host_ip: 127.0.0.1`")),
        "{long:?}"
    );
}

#[test]
fn a_long_form_port_is_on_the_loopback_address_only_with_a_loopback_host_ip() {
    let text = "services:\n  app-e2e-postgres:\n    build: .\n    ports:\n      - target: 5432\n        published: \"5432\"\n        host_ip: 127.0.0.1\n      - target: 5433\n        published: \"5433\"\n      - target: 5434\n        host_ip: 0.0.0.0\n      - target: 5435\n        host_ip: \"::1\"\n";
    assert_eq!(
        aspect("docker-compose.e2e.yml", text, "port"),
        [
            "port docker-compose.e2e.yml:10 app-e2e-postgres#5434",
            "port docker-compose.e2e.yml:8 app-e2e-postgres#5433"
        ]
    );
}

#[test]
fn a_file_that_is_not_yaml_stops_the_run() {
    let error = Fixture::new(&[("docker-compose.e2e.yml", "services: [\n")])
        .check_with(RULE, "{}", "")
        .expect_err("invalid YAML");
    assert!(
        error
            .message()
            .contains("docker-compose.e2e.yml: while parsing"),
        "{error}"
    );
}
