//! `docker/dockerfile`: an image file is `Dockerfile`, pinned, staged, runs as a user that is not
//! root, carries the OCI labels and no health check, ignores what must never enter its context, and
//! writes no secret into a build argument or an environment variable.

use std::collections::BTreeSet;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Report, Rule, Run, Tree};

use crate::instructions::{self, Dockerfile as Parsed, labels, variables};
use crate::settings::Settings;

/// The rule.
#[derive(Debug, Default)]
pub struct Dockerfile;

/// The options of `docker/dockerfile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(
    crate = "gate_sdk::serde",
    default,
    deny_unknown_fields,
    rename_all = "kebab-case"
)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct Options {
    /// The patterns an ignore file that is not an allow-list excludes, each as one line.
    pub must_ignore: Vec<String>,
    /// The labels each runtime stage sets, and the only label names allowed.
    pub labels: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        let strings = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        Self {
            must_ignore: strings(&[".git", ".env", ".env.*", "*.env", "*.pem", "*.key"]),
            labels: strings(&[
                "org.opencontainers.image.source",
                "org.opencontainers.image.revision",
                "org.opencontainers.image.version",
            ]),
        }
    }
}

impl Rule for Dockerfile {
    type Options = Options;

    const NAME: &'static str = "dockerfile";
    const TITLE: &'static str = "Images are pinned, staged, run as a user and keep secrets out";
    const ASPECTS: &'static [&'static str] = &[
        "name",
        "pinned",
        "stages",
        "user",
        "healthcheck",
        "labels",
        "ignore",
        "secret",
    ];

    fn check(&self, run: &mut Run<'_, Options>) -> gate_sdk::Result<()> {
        let settings = Settings::read(run)?;
        let tree = run.tree();
        let paths = tree.paths();
        let options = run.options();
        let must_ignore = options.must_ignore.clone();
        let wanted = options.labels.clone();
        let report = run.report();

        for path in &paths {
            let name = file_name(path);
            if name == ".dockerignore" {
                report
                    .finding(
                        "name",
                        Location::file(path.clone()),
                        format!("{path} is a .dockerignore; its job is the ignore file beside each Dockerfile"),
                    )
                    .help("move its patterns to `<Dockerfile name>.dockerignore` beside the Dockerfile, and delete it")
                    .key(".dockerignore");
                continue;
            }
            if !is_image_file(name) {
                continue;
            }
            if name != "Dockerfile" {
                report
                    .finding(
                        "name",
                        Location::file(path.clone()),
                        format!("{path} is an image file not named Dockerfile"),
                    )
                    .help("name the file Dockerfile, one per directory")
                    .key(name.to_owned());
            }
            let Some(text) = tree.text(path) else {
                continue;
            };
            let parsed = instructions::parse(&text);
            let check = Check {
                path,
                parsed: &parsed,
                settings: &settings,
                labels: &wanted,
            };
            check.pinned(report);
            check.stages(report);
            check.runtime(report);
            check.instructions(report);
            ignore(report, tree, path, &must_ignore);
        }

        Ok(())
    }
}

/// The file name of a path.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether a file name is an image file: `Dockerfile` in any case, `Dockerfile.<x>` or
/// `<x>.Dockerfile`, and not an ignore file.
fn is_image_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, extension)| extension);
    extension != Some("dockerignore")
        && (lower == "dockerfile"
            || lower.starts_with("dockerfile.")
            || extension == Some("dockerfile"))
}

/// One image file being checked.
struct Check<'c> {
    path: &'c str,
    parsed: &'c Parsed,
    settings: &'c Settings,
    labels: &'c [String],
}

impl Check<'_> {
    fn pinned(&self, report: &mut Report) {
        for (index, stage) in self.parsed.stages.iter().enumerate() {
            if stage.base.eq_ignore_ascii_case("scratch")
                || self.parsed.stage(&stage.base, index).is_some()
            {
                continue;
            }
            let problem = match self.parsed.expand(&stage.base) {
                Some(image) => self.settings.unpinned(&image),
                None => Some(format!(
                    "`{}` uses a build argument without a default",
                    stage.base
                )),
            };
            if let Some(problem) = problem {
                report
                    .finding(
                        "pinned",
                        Location::line(self.path, stage.line),
                        format!("FROM {problem}"),
                    )
                    .target(format!("{}:{}", self.path, stage.base))
                    .help(
                        "pin the base to its digest, `FROM <image>@sha256:<64 hex>`, with the tag in \
                         a comment; a build argument's default is pinned the same way",
                    )
                    .key(stage.base.clone());
            }
        }
    }

    fn stages(&self, report: &mut Report) {
        if self.parsed.stages.len() < 2 {
            let line = self.parsed.stages.first().map_or(1, |stage| stage.line);
            report
                .finding(
                    "stages",
                    Location::line(self.path, line),
                    format!("{} has one stage", self.path),
                )
                .help("build in one stage and copy what runs into a second, so build tools stay out of the image")
                .key("stages");
        }
    }

    /// The stages no other stage builds from or copies from.
    fn runtime_stages(&self) -> Vec<usize> {
        let stages = &self.parsed.stages;
        let mut used = BTreeSet::new();
        for (index, stage) in stages.iter().enumerate() {
            if let Some(base) = self.parsed.stage(&stage.base, index) {
                used.insert(base);
            }
            for instruction in &stage.instructions {
                let (flags, _) = instructions::flags(&instruction.arguments);
                for (flag, value) in flags {
                    let from = match flag.as_str() {
                        "from" if instruction.keyword == "COPY" => Some(value.as_str()),
                        "mount" if instruction.keyword == "RUN" => {
                            value.split(',').find_map(|part| part.strip_prefix("from="))
                        }
                        _ => None,
                    };
                    if let Some(reference) = from.and_then(|from| self.parsed.stage(from, index)) {
                        used.insert(reference);
                    }
                }
            }
        }
        (0..stages.len())
            .filter(|index| !used.contains(index))
            .collect()
    }

    /// The stage chain of `index`, itself first, through `FROM <earlier stage>`.
    fn chain(&self, index: usize) -> Vec<usize> {
        let mut chain = vec![index];
        let mut current = index;
        while let Some(base) = self
            .parsed
            .stage(&self.parsed.stages[current].base, current)
        {
            chain.push(base);
            current = base;
        }
        chain
    }

    fn runtime(&self, report: &mut Report) {
        for index in self.runtime_stages() {
            let stage = &self.parsed.stages[index];
            let chain = self.chain(index);
            let name = stage
                .name
                .clone()
                .unwrap_or_else(|| format!("stage {index}"));

            let user = chain.iter().find_map(|link| {
                self.parsed.stages[*link]
                    .instructions
                    .iter()
                    .rev()
                    .find(|instruction| instruction.keyword == "USER")
                    .map(|instruction| instruction.arguments.trim().to_owned())
            });
            let root = user.as_deref().is_none_or(|user| {
                let account = user.split(':').next().unwrap_or(user);
                account == "root" || account == "0" || account.is_empty()
            });
            if root {
                report
                    .finding(
                        "user",
                        Location::line(self.path, stage.line),
                        format!("the runtime stage {name} runs as root"),
                    )
                    .target(format!("{}:{name}", self.path))
                    .help("end the stage with `USER <name>`, an account of the image that is not root")
                    .key(name.clone());
            }

            let set: BTreeSet<String> = chain
                .iter()
                .flat_map(|link| &self.parsed.stages[*link].instructions)
                .filter(|instruction| instruction.keyword == "LABEL")
                .flat_map(|instruction| labels(&instruction.arguments))
                .collect();
            let missing: Vec<&str> = self
                .labels
                .iter()
                .filter(|label| !set.contains(label.as_str()))
                .map(String::as_str)
                .collect();
            if !missing.is_empty() {
                report
                    .finding(
                        "labels",
                        Location::line(self.path, stage.line),
                        format!(
                            "the runtime stage {name} does not set {}",
                            missing.join(", ")
                        ),
                    )
                    .target(format!("{}:{name}", self.path))
                    .help("set the OCI labels with `LABEL`, their values from build arguments")
                    .key(format!("{name}#missing"));
            }
        }
    }

    fn instructions(&self, report: &mut Report) {
        for instruction in self
            .parsed
            .stages
            .iter()
            .flat_map(|stage| &stage.instructions)
        {
            match instruction.keyword.as_str() {
                "HEALTHCHECK" => {
                    report
                        .finding(
                            "healthcheck",
                            Location::line(self.path, instruction.line),
                            format!("{} declares a HEALTHCHECK", self.path),
                        )
                        .help("remove it: the orchestrator's service definition holds the health check")
                        .key("healthcheck");
                }
                "LABEL" => {
                    for label in labels(&instruction.arguments) {
                        if !self.labels.contains(&label) {
                            report
                                .finding(
                                    "labels",
                                    Location::line(self.path, instruction.line),
                                    format!("the label {label} is not one of the OCI labels the image sets"),
                                )
                                .target(format!("{}:{label}", self.path))
                                .help("carry the value in org.opencontainers.image.version or another of `labels`")
                                .key(label);
                        }
                    }
                }
                keyword @ ("ARG" | "ENV") => {
                    for (name, _) in variables(keyword, &instruction.arguments) {
                        if self.settings.secret(&name) {
                            report
                                .finding(
                                    "secret",
                                    Location::line(self.path, instruction.line),
                                    format!("{keyword} {name} puts a secret into the image's history"),
                                )
                                .target(format!("{}:{}", self.path, instruction.line))
                                .help("pass it to the step that needs it with `RUN --mount=type=secret`")
                                .key(name);
                        }
                    }
                }
                _ => {}
            }
        }
        for argument in &self.parsed.arguments {
            if self.settings.secret(&argument.name) {
                report
                    .finding(
                        "secret",
                        Location::line(self.path, argument.line),
                        format!(
                            "ARG {} puts a secret into the image's history",
                            argument.name
                        ),
                    )
                    .target(format!("{}:{}", self.path, argument.line))
                    .help("pass it to the step that needs it with `RUN --mount=type=secret`")
                    .key(argument.name.clone());
            }
        }
    }
}

/// The ignore file beside `path` is an allow-list or excludes each of `must_ignore`.
fn ignore(report: &mut Report, tree: Tree<'_>, path: &str, must_ignore: &[String]) {
    let ignore = format!("{path}.dockerignore");
    let Some(text) = tree.text(&ignore) else {
        report
            .finding(
                "ignore",
                Location::file(path.to_owned()),
                format!("{path} has no {} beside it", file_name(&ignore)),
            )
            .target(path.to_owned())
            .help("add the ignore file beside the Dockerfile, best as an allow-list: `**` first, then `!` each path the build copies")
            .key("missing");
        return;
    };
    let patterns: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if patterns.first() == Some(&"**") {
        return;
    }
    let excluded: BTreeSet<&str> = patterns
        .iter()
        .map(|pattern| {
            pattern
                .trim_start_matches('/')
                .trim_start_matches("**/")
                .trim_end_matches('/')
        })
        .collect();
    for pattern in must_ignore {
        if !excluded.contains(pattern.as_str()) {
            report
                .finding(
                    "ignore",
                    Location::file(ignore.clone()),
                    format!("{ignore} does not exclude {pattern}"),
                )
                .target(format!("{ignore}:{pattern}"))
                .help("add the pattern as a line, or turn the file into an allow-list: `**` first")
                .key(pattern.clone());
        }
    }
}
