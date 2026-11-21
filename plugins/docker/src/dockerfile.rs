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
            if !settings.image_file(name) {
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

/// What an instruction copies or mounts from, a stage or an image: the value of `COPY --from=`,
/// and the `from=` of each `RUN --mount=`.
fn sources(instruction: &instructions::Instruction) -> Vec<String> {
    let (flags, _) = instructions::flags(&instruction.arguments);
    let mut sources = Vec::new();
    for (flag, value) in flags {
        let source = match flag.as_str() {
            "from" if instruction.keyword == "COPY" => Some(value),
            "mount" if instruction.keyword == "RUN" => value
                .split(',')
                .find_map(|part| part.strip_prefix("from=").map(str::to_owned)),
            _ => None,
        };
        sources.extend(source);
    }
    sources
}

/// One image file being checked.
struct Check<'c> {
    path: &'c str,
    parsed: &'c Parsed,
    settings: &'c Settings,
    labels: &'c [String],
}

impl Check<'_> {
    /// Every image the file pulls is pinned: each `FROM` that is not `scratch` or an earlier
    /// stage, and each `COPY --from=` and `RUN --mount=...,from=` that does not name a stage. An
    /// image is reported once, where it first appears.
    fn pinned(&self, report: &mut Report) {
        let mut reported = BTreeSet::new();
        for (index, stage) in self.parsed.stages.iter().enumerate() {
            let mut pulls = vec![("FROM", stage.line, stage.base.clone())];
            for instruction in &stage.instructions {
                let keyword = match instruction.keyword.as_str() {
                    "COPY" => "COPY --from",
                    "RUN" => "RUN --mount from",
                    _ => continue,
                };
                for source in sources(instruction) {
                    pulls.push((keyword, instruction.line, source));
                }
            }
            for (keyword, line, image) in pulls {
                if image.eq_ignore_ascii_case("scratch")
                    || self.parsed.stage(&image, index).is_some()
                    || !reported.insert(image.clone())
                {
                    continue;
                }
                let problem = match self.parsed.expand(&image) {
                    Some(expanded) => self.settings.unpinned(&expanded),
                    None => Some(format!("`{image}` uses a build argument without a default")),
                };
                if let Some(problem) = problem {
                    report
                        .finding(
                            "pinned",
                            Location::line(self.path, line),
                            format!("{keyword} {problem}"),
                        )
                        .target(format!("{}:{image}", self.path))
                        .help(
                            "pin the image to its digest, `<image>@sha256:<64 hex>`, with the tag in \
                             a comment; a build argument's default is pinned the same way",
                        )
                        .key(image);
                }
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
                for source in sources(instruction) {
                    if let Some(reference) = self.parsed.stage(&source, index) {
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
            // How a message names the stage: by its name, or by its number when it has none.
            let called = stage.name.as_ref().map_or_else(
                || format!("the unnamed runtime stage {index}"),
                |name| format!("the runtime stage {name}"),
            );

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
                        format!("{called} runs as root"),
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
                        format!("{called} does not set {}", missing.join(", ")),
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
                                .target(format!("{}:{name}", self.path))
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
                    .target(format!("{}:{}", self.path, argument.name))
                    .help("pass it to the step that needs it with `RUN --mount=type=secret`")
                    .key(argument.name.clone());
            }
        }
    }
}

/// The ignore file beside `path` is an allow-list or excludes each of `must_ignore`, and no `!` line
/// takes one of them back in.
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
    let patterns: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .collect();
    // An allow-list excludes everything first; the later lines decide, the last one that names a
    // pattern winning.
    let allow_list = matches!(patterns.first(), Some((_, "**" | "*")));
    for pattern in must_ignore {
        let mut excluded = allow_list;
        let mut taken_back = None;
        for (line, written) in &patterns {
            let (include, written) = written
                .strip_prefix('!')
                .map_or((false, *written), |rest| (true, rest.trim()));
            if normalized(written) == pattern.as_str() {
                excluded = !include;
                taken_back = include.then_some(*line);
            }
        }
        if excluded {
            continue;
        }
        let (location, message) = if let Some(line) = taken_back {
            (
                Location::line(ignore.as_str(), line),
                format!("{ignore} takes {pattern} back in with `!{pattern}`"),
            )
        } else {
            (
                Location::file(ignore.clone()),
                format!("{ignore} does not exclude {pattern}"),
            )
        };
        report
            .finding("ignore", location, message)
            .target(format!("{ignore}:{pattern}"))
            .help("exclude the pattern with a line of its own and no `!` line for it, or turn the file into an allow-list: `**` first")
            .key(pattern.clone());
    }
}

/// An ignore pattern without a leading `/` or `**/` and a trailing `/`, as `must-ignore` writes it.
fn normalized(pattern: &str) -> &str {
    pattern
        .trim_start_matches('/')
        .trim_start_matches("**/")
        .trim_end_matches('/')
}
