//! `docker/compose`: a compose file is named for its purpose, its services for their owner,
//! purpose and role; its images are pinned, it writes no secret outside an end-to-end file, it
//! publishes ports on the loopback address only, and a service others wait for as healthy declares
//! its health check.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::yaml::{self, Node, Value};
use gate_sdk::{Error, Location, Report, Rule, Run};

use crate::settings::Settings;

/// The rule.
#[derive(Debug, Default)]
pub struct Compose;

/// The options of `docker/compose`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The purposes a compose file and its services name.
    pub purposes: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            purposes: vec!["development".to_owned(), "e2e".to_owned()],
        }
    }
}

impl Rule for Compose {
    type Options = Options;

    const NAME: &'static str = "compose";
    const TITLE: &'static str =
        "Compose files are named for their purpose, pinned, local and keep secrets out";
    const ASPECTS: &'static [&'static str] = &[
        "name",
        "service",
        "container-name",
        "pinned",
        "secret",
        "port",
        "healthcheck",
    ];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let options = run.options();
        let purposes = options.purposes.clone();
        let report = run.report();

        for path in tree.paths() {
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            if !is_compose_file(&name) {
                continue;
            }
            let purpose = purpose(&name, &purposes);
            if purpose.is_none() {
                report
                    .finding(
                        "name",
                        Location::file(path.clone()),
                        format!(
                            "{path} is not named docker-compose.[<scenario>.]<purpose>.yml with a purpose of {}",
                            purposes.join(", ")
                        ),
                    )
                    .help("name the file for its purpose, such as docker-compose.e2e.yml")
                    .key(name.clone());
            }
            let Some(text) = tree.text(&path) else {
                continue;
            };
            let root = yaml::parse(&text).map_err(|error| {
                Error::new(format!(
                    "{path}: {error}; fix the YAML so that docker compose can read the file"
                ))
            })?;
            let file = File {
                path: &path,
                purpose: purpose.as_deref(),
                end_to_end: matches!(
                    name.rsplitn(3, '.').collect::<Vec<_>>().as_slice(),
                    ["yml", "e2e", _]
                ),
                settings: &settings,
            };
            let services = root
                .get("services")
                .and_then(Node::mapping)
                .unwrap_or_default();
            for (key, service) in services {
                let Some(service_name) = key.scalar() else {
                    continue;
                };
                file.service(report, service_name, key.line, service);
            }
            file.health(report, services);
        }

        Ok(())
    }
}

/// Whether a file name is a compose file: `compose` or `docker-compose`, then `.yml` or `.yaml`,
/// with anything between.
fn is_compose_file(name: &str) -> bool {
    (name.starts_with("docker-compose.") || name.starts_with("compose."))
        && matches!(
            name.rsplit_once('.').map(|(_, extension)| extension),
            Some("yml" | "yaml")
        )
}

/// The purpose of a well-named compose file: `docker-compose.<purpose>.yml` or
/// `docker-compose.<scenario>.<purpose>.yml`, the scenario kebab-case.
fn purpose(name: &str, purposes: &[String]) -> Option<String> {
    let middle = name.strip_prefix("docker-compose.")?.strip_suffix(".yml")?;
    let (scenario, purpose) = match middle.rsplit_once('.') {
        Some((scenario, purpose)) => (Some(scenario), purpose),
        None => (None, middle),
    };
    if scenario.is_some_and(|scenario| !kebab(scenario)) {
        return None;
    }
    purposes
        .iter()
        .any(|item| item == purpose)
        .then(|| purpose.to_owned())
}

/// Whether `text` is kebab-case: lowercase letters and digits in parts joined by `-`.
fn kebab(text: &str) -> bool {
    !text.is_empty()
        && text.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// One compose file being checked.
struct File<'f> {
    path: &'f str,
    purpose: Option<&'f str>,
    end_to_end: bool,
    settings: &'f Settings,
}

impl File<'_> {
    fn service(&self, report: &mut Report, name: &str, line: usize, service: &Node) {
        let path = self.path;
        if let Some(purpose) = self.purpose {
            let parts: Vec<&str> = name.split('-').collect();
            if !kebab(name) || parts.len() < 3 || parts[1] != purpose {
                report
                    .finding(
                        "service",
                        Location::line(path, line),
                        format!("the service {name} is not named <owner>-{purpose}-<role>"),
                    )
                    .target(format!("{path}:{name}"))
                    .help(format!(
                        "name it `app-{purpose}-<role>`, or after whose service it is: two files in one directory share a compose project"
                    ))
                    .key(format!("{name}#name"));
            }
        }
        if let Some(container) = service.get("container_name") {
            report
                .finding(
                    "container-name",
                    Location::line(path, container.line),
                    format!("the service {name} sets container_name"),
                )
                .target(format!("{path}:{name}"))
                .help("remove it: a fixed name collides between two worktrees or two runs")
                .key(name.to_owned());
        }
        if service.get("build").is_none()
            && let Some(image) = service.get("image")
        {
            let text = image.scalar().unwrap_or("");
            if let Some(problem) = self.image(text) {
                report
                    .finding("pinned", Location::line(path, image.line), problem)
                    .target(format!("{path}:{name}"))
                    .help("pin the image to its digest, `<image>@sha256:<64 hex>`, or require it from the caller with `${NAME:?...}`")
                    .key(name.to_owned());
            }
        }
        if !self.end_to_end
            && let Some(environment) = service.get("environment")
        {
            self.secrets(report, name, environment);
        }
        for port in service
            .get("ports")
            .and_then(Node::sequence)
            .unwrap_or_default()
        {
            if !loopback(port) {
                report
                    .finding(
                        "port",
                        Location::line(path, port.line),
                        format!("the service {name} publishes a port beyond the loopback address"),
                    )
                    .target(format!("{path}:{name}"))
                    .help(loopback_help(port))
                    .key(format!(
                        "{name}#{}",
                        port.scalar()
                            .or_else(|| port.get("published").and_then(Node::scalar))
                            .or_else(|| port.get("target").and_then(Node::scalar))
                            .unwrap_or("port")
                    ));
            }
        }
    }

    /// Why `image` is not acceptable; `None` when it is pinned or required from the caller.
    fn image(&self, image: &str) -> Option<String> {
        let Some(inner) = image
            .strip_prefix("${")
            .and_then(|rest| rest.strip_suffix('}'))
        else {
            return self.settings.unpinned(image);
        };
        match operator(inner) {
            (":?" | "?", _) => None,
            (":-" | "-", default) => self.settings.unpinned(default),
            _ => Some(format!(
                "`{image}` takes any image the environment names; require it with `${{NAME:?...}}`"
            )),
        }
    }

    /// The secret-named entries of `environment` with a literal value, or a literal default.
    fn secrets(&self, report: &mut Report, service: &str, environment: &Node) {
        let entries: Vec<(String, Option<String>, usize)> = match &environment.value {
            Value::Mapping(entries) => entries
                .iter()
                .filter_map(|(key, value)| {
                    Some((
                        key.scalar()?.to_owned(),
                        value.scalar().map(str::to_owned),
                        key.line,
                    ))
                })
                .collect(),
            Value::Sequence(items) => items
                .iter()
                .filter_map(|item| {
                    let (key, value) = item.scalar()?.split_once('=')?;
                    Some((key.to_owned(), Some(value.to_owned()), item.line))
                })
                .collect(),
            _ => Vec::new(),
        };
        for (key, value, line) in entries {
            if !self.settings.secret(&key) {
                continue;
            }
            if value.as_deref().is_some_and(literal) {
                report
                    .finding(
                        "secret",
                        Location::line(self.path, line),
                        format!("the service {service} has a literal value for {key}"),
                    )
                    .target(format!("{}:{service}.{key}", self.path))
                    .help("take it from the environment, `${NAME}`, or from a compose secret; an end-to-end file (`*.e2e.yml`) may write local throwaway credentials")
                    .key(format!("{service}.{key}"));
            }
        }
    }

    /// The services others wait for as healthy that declare no health check.
    fn health(&self, report: &mut Report, services: &[(Node, Node)]) {
        let mut waited = BTreeSet::new();
        for (_, service) in services {
            for (target, condition) in service
                .get("depends_on")
                .and_then(Node::mapping)
                .unwrap_or_default()
            {
                let healthy = condition
                    .get("condition")
                    .and_then(Node::scalar)
                    .is_some_and(|condition| condition == "service_healthy");
                if let (true, Some(target)) = (healthy, target.scalar()) {
                    waited.insert(target.to_owned());
                }
            }
        }
        for (key, service) in services {
            let Some(name) = key.scalar() else {
                continue;
            };
            let declared = service
                .get("healthcheck")
                .is_some_and(|check| check.get("disable").and_then(Node::scalar) != Some("true"));
            if waited.contains(name) && !declared {
                report
                    .finding(
                        "healthcheck",
                        Location::line(self.path, key.line),
                        format!("other services wait for {name} as healthy, and it declares no healthcheck"),
                    )
                    .target(format!("{}:{name}", self.path))
                    .help("declare a `healthcheck` that says when the service is ready")
                    .key(name.to_owned());
            }
        }
    }
}

/// Whether a value is written in: no interpolation, or a `${NAME:-literal}` default.
fn literal(value: &str) -> bool {
    match value
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
    {
        Some(inner) => matches!(operator(inner), (":-" | "-", default) if !default.is_empty()),
        None => !value.is_empty() && !value.starts_with('$'),
    }
}

/// The operator of the inside of `${NAME<operator><word>}` (`:-`, `-`, `:?`, `?`, `:+`, `+`, or
/// empty) and its word.
fn operator(inner: &str) -> (&str, &str) {
    let end = inner
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(inner.len());
    let rest = &inner[end..];
    for operator in [":-", ":?", ":+", "-", "?", "+"] {
        if let Some(word) = rest.strip_prefix(operator) {
            return (operator, word);
        }
    }
    ("", rest)
}

/// Whether a `ports` entry publishes on the loopback address only.
fn loopback(port: &Node) -> bool {
    let local = |address: &str| matches!(address, "127.0.0.1" | "::1" | "[::1]");
    if let Some(host) = port.get("host_ip") {
        return host.scalar().is_some_and(local);
    }
    if port.mapping().is_some() {
        return false;
    }
    let text = port.scalar().unwrap_or("");
    if let Some(rest) = text.strip_prefix('[') {
        return rest
            .split_once(']')
            .is_some_and(|(address, _)| local(address));
    }
    let parts: Vec<&str> = text.split(':').collect();
    parts.len() == 3 && local(parts[0])
}

/// What to write instead of `port` to publish it on the loopback address with the same host and
/// container ports: the short form behind `127.0.0.1`, or `host_ip` for the long form.
fn loopback_help(port: &Node) -> String {
    if port.mapping().is_some() {
        return "set `host_ip: 127.0.0.1` (or `\"::1\"`) on the port, keeping its published and target ports".to_owned();
    }
    let text = port.scalar().unwrap_or("");
    // The part after an address, if the entry names one: `[<ipv6>]:` or the first of three parts.
    let ports = match text.strip_prefix('[') {
        Some(rest) => rest.split_once("]:").map_or(text, |(_, ports)| ports),
        None => match outside_braces(text).as_slice() {
            [address, _, _] => &text[address.len() + 1..],
            _ => text,
        },
    };
    // `<host>:<container>` keeps both; a container port alone keeps its random host port.
    let published = if outside_braces(ports).len() > 1 {
        format!("127.0.0.1:{ports}")
    } else {
        format!("127.0.0.1::{ports}")
    };
    format!("publish on 127.0.0.1 (or [::1]) with the same ports: `\"{published}\"`")
}

/// `text` split at each `:` outside a `${...}` interpolation.
fn outside_braces(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0_usize, 0);
    for (at, character) in text.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_values_and_ports_take_the_standards_forms() {
        let purposes = ["development".to_owned(), "e2e".to_owned()];
        assert_eq!(
            purpose("docker-compose.e2e.yml", &purposes).as_deref(),
            Some("e2e")
        );
        assert_eq!(
            purpose("docker-compose.load-test.e2e.yml", &purposes).as_deref(),
            Some("e2e")
        );
        for name in [
            "docker-compose.yml",
            "docker-compose.dev.yml",
            "docker-compose.e2e.yaml",
            "docker-compose.Load.e2e.yml",
        ] {
            assert_eq!(purpose(name, &purposes), None, "{name}");
        }
        assert!(is_compose_file("compose.yaml") && is_compose_file("docker-compose.yml"));
        assert!(!is_compose_file("compose-notes.md"));
        assert!(literal("x") && literal("${A:-x}") && literal("${A-x}"));
        assert!(
            !literal("${A}") && !literal("$A") && !literal("${A:?must-be-set}") && !literal("")
        );
    }
}
