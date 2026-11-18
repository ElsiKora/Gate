//! The `gate` command line.

mod distribution;
mod list;

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand, ValueEnum};
use gate_host::cache::Cache;
use gate_host::config::RuleKey;
use gate_host::report::{render_finding, render_selection};
use gate_host::{
    ChangeRequest, CheckRequest, EXIT_FAILURE, Error, Fetcher, Format, PluginOverride, RuleFilter,
    RunOptions, SelectionFormat, Source, Workspace, doctor, run, sync, tools,
};

/// Gate: a modular repository linter and gate.
#[derive(Debug, Parser)]
#[command(name = "gate", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check the repository against its gate.toml files.
    Check(CheckArgs),
    /// Print the projects and the configured rules.
    List(ListArgs),
    /// Print what the configuration says about one rule, and where.
    Explain {
        /// The rule: <plugin>/<rule>, <plugin>/<rule>#<aspect> or <plugin>/*.
        rule: String,
    },
    /// Print the JSON Schema of gate.toml.
    Schema(SchemaArgs),
    /// Manage gate/baseline.json, the findings accepted as they are today.
    #[command(subcommand)]
    Baseline(BaselineCommand),
    /// Write the generated files: hooks, CI jobs, plugins' outputs.
    Sync(SyncArgs),
    /// Run a step or a suite in every scope the change selects.
    Run(RunCommandArgs),
    /// Check the working copy and the installation.
    Doctor,
    /// Write a starting gate.toml, the launcher bin/gate and gate.lock.
    Init,
    /// Resolve plugins and the core into gate.lock.
    Lock(LockArgs),
    /// Manage the shared cache of downloads and compiled plugins.
    #[command(subcommand)]
    Cache(CacheCommand),
}

#[derive(Debug, Args)]
struct LockArgs {
    /// Pin one plugin to a release asset URL without downloading it: <name>=<url>. Needs --sha256.
    #[arg(long, value_name = "NAME=URL", requires = "sha256")]
    set: Option<String>,
    /// The sha256 of the asset --set pins.
    #[arg(long, value_name = "HASH", requires = "set")]
    sha256: Option<String>,
    /// Refused: an override is for one gate check run and never enters gate.lock.
    #[arg(long = "plugin", value_name = "NAME=PATH", hide = true)]
    plugins: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    /// List the cache entries no gate.lock pins; --yes removes them.
    Prune(PruneArgs),
}

#[derive(Debug, Args)]
struct PruneArgs {
    /// Repositories whose gate.lock files, in every worktree, keep their entries; by default the
    /// current one.
    #[arg(value_name = "REPOSITORY")]
    repositories: Vec<PathBuf>,
    /// Remove the entries instead of listing them.
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is one command-line flag"
)]
struct CheckArgs {
    /// Check the index, and only the projects the staged change touches.
    #[arg(long, conflicts_with_all = ["rev", "push"])]
    staged: bool,
    /// Check the tree of a commit.
    #[arg(long, value_name = "REV", conflicts_with = "push")]
    rev: Option<String>,
    /// Check only the projects changed since --base (and those that depend on them).
    #[arg(long, requires = "base", conflicts_with = "push")]
    changed: bool,
    /// The revision --changed compares with.
    #[arg(long, value_name = "REV", requires = "changed")]
    base: Option<String>,
    /// Check a pushed range <base>..<tip> (a base of zeros is a new branch).
    #[arg(long, value_name = "RANGE")]
    push: Option<String>,
    /// The remote ref the --push range goes to (the pre-push hook and CI pass it).
    #[arg(long, value_name = "REF", requires = "push", value_parser = parse_target_ref)]
    target_ref: Option<String>,
    /// The output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    /// Replace one pinned plugin with a local build for this run: <name>=<path.wasm>. Repeatable.
    #[arg(long = "plugin", value_name = "NAME=PATH", value_parser = PluginOverride::parse)]
    plugins: Vec<PluginOverride>,
    /// Refuse every --plugin override (the generated CI jobs pass it).
    #[arg(long)]
    no_overrides: bool,
    /// Print only the projects the change selects (text, json or github-output); no rule runs.
    #[arg(long)]
    list_changed_projects: bool,
    /// The commit message file under review (the commit-msg hook passes "$1").
    #[arg(long, value_name = "FILE")]
    commit_message: Option<PathBuf>,
    /// Run only this rule, <plugin>/<rule>, or every rule of a plugin, <plugin>/*. Repeatable.
    #[arg(long = "rule", value_name = "ID", value_parser = RuleFilter::parse)]
    rules: Vec<RuleKey>,
    #[command(flatten)]
    run: RunArgs,
}

#[derive(Debug, Args)]
struct SyncArgs {
    /// Write nothing: report every generated file that differs from gate.toml, and exit 1 if any.
    #[arg(long)]
    check: bool,
    /// Replace one pinned plugin with a local build for this run: <name>=<path.wasm>. Repeatable.
    #[arg(long = "plugin", value_name = "NAME=PATH", value_parser = PluginOverride::parse)]
    plugins: Vec<PluginOverride>,
}

#[derive(Debug, Args)]
struct RunCommandArgs {
    /// The step or suite.
    name: String,
    /// Run only in the projects the staged change touches; files = "staged" steps get the staged
    /// paths.
    #[arg(long, conflicts_with_all = ["changed", "push"])]
    staged: bool,
    /// Run only in the projects changed since --base.
    #[arg(long, requires = "base", conflicts_with = "push")]
    changed: bool,
    /// The revision --changed compares with.
    #[arg(long, value_name = "REV", requires = "changed")]
    base: Option<String>,
    /// Run only in the projects a pushed range <base>..<tip> touches.
    #[arg(long, value_name = "RANGE")]
    push: Option<String>,
    /// The remote ref the --push range goes to (the pre-push hook and CI pass it).
    #[arg(long, value_name = "REF", requires = "push", value_parser = parse_target_ref)]
    target_ref: Option<String>,
    /// Read standard input once and give it to every step on its standard input (the pre-push
    /// hook passes the pushed refs).
    #[arg(long)]
    stdin: bool,
    /// Given to every step as $1, $2, ... (a hook passes its own arguments).
    #[arg(last = true, value_name = "ARGUMENT")]
    arguments: Vec<String>,
}

/// Options passed through to the run.
#[derive(Debug, Args)]
struct RunArgs {
    /// Print the time of each phase and plugin call to standard error.
    #[arg(long)]
    timings: bool,
    /// Warn when the run takes longer than this many seconds.
    #[arg(long, value_name = "SECONDS", value_parser = parse_budget)]
    budget: Option<Duration>,
    /// Make a run over --budget an error instead of a warning.
    #[arg(long, requires = "budget")]
    budget_fails: bool,
    /// Stop at the first error finding.
    #[arg(long)]
    fail_fast: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    /// The output format.
    #[arg(long, value_enum, default_value_t = ListFormat::Text)]
    format: ListFormat,
    /// Print the file, line and section each setting came from.
    #[arg(long)]
    origin: bool,
}

#[derive(Debug, Args)]
struct SchemaArgs {
    /// Print the schema of gate.exceptions.toml instead.
    #[arg(long)]
    exceptions: bool,
}

#[derive(Debug, Subcommand)]
enum BaselineCommand {
    /// Run a full check of the working tree and store its findings in gate/baseline.json.
    Write,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
    Github,
    /// `name=value` lines for `$GITHUB_OUTPUT` (with --list-changed-projects only).
    GithubOutput,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ListFormat {
    Text,
    Json,
}

/// A `--target-ref` value: a full ref, the form the pre-push hook and the generated CI pass.
fn parse_target_ref(value: &str) -> Result<String, String> {
    match value.strip_prefix("refs/") {
        Some(name) if !name.is_empty() && !name.ends_with('/') => Ok(value.to_owned()),
        _ => Err(format!(
            "{value:?} is not a full ref; pass the full remote ref, such as refs/heads/main"
        )),
    }
}

/// Refuses `--target-ref` without `--push`. clap drops `requires = "push"` when a present
/// argument conflicts with --push (--rev and --staged do), so it is checked here as well.
fn target_ref_needs_push(target_ref: Option<&str>, push: Option<&str>) -> Result<(), Error> {
    if target_ref.is_some() && push.is_none() {
        return Err(Error::config(
            "--target-ref names the remote ref a --push range updates; pass it with --push",
        ));
    }
    Ok(())
}

fn parse_budget(value: &str) -> Result<Duration, String> {
    let seconds: f64 = value
        .parse()
        .map_err(|_| format!("{value:?} is not a number of seconds"))?;
    if seconds.is_finite() && seconds > 0.0 {
        Ok(Duration::from_secs_f64(seconds))
    } else {
        Err(format!(
            "{value:?}: the budget must be a number of seconds above 0"
        ))
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check(arguments) => check(&arguments),
        Command::List(arguments) => list(&arguments),
        Command::Explain { rule } => explain(&rule),
        Command::Schema(arguments) => schema(&arguments),
        Command::Baseline(BaselineCommand::Write) => baseline_write(),
        Command::Sync(arguments) => sync(&arguments),
        Command::Run(arguments) => run(&arguments),
        Command::Doctor => doctor(),
        Command::Init => distribution::init().and_then(|text| print(&text)),
        Command::Lock(arguments) => distribution::lock(&distribution::LockRequest {
            set: arguments.set,
            sha256: arguments.sha256,
            overrides: arguments.plugins,
        })
        .and_then(|text| print(&text)),
        Command::Cache(CacheCommand::Prune(arguments)) => {
            distribution::prune(&arguments.repositories, arguments.yes)
                .and_then(|text| print(&text))
        }
    };

    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("gate: {error}");
            ExitCode::from(exit_byte(EXIT_FAILURE))
        }
    }
}

fn exit_byte(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(2)
}

fn write_out(text: &str) -> Result<(), Error> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(Error::from)
}

fn print(text: &str) -> Result<u8, Error> {
    write_out(text)?;
    Ok(0)
}

fn check(arguments: &CheckArgs) -> Result<u8, Error> {
    target_ref_needs_push(arguments.target_ref.as_deref(), arguments.push.as_deref())?;
    let commit_message = match &arguments.commit_message {
        None => None,
        Some(path) => Some(std::fs::read_to_string(path).map_err(|error| {
            Error::environment(format!(
                "--commit-message {}: cannot read the file: {error}",
                path.display()
            ))
        })?),
    };
    let changes = if let Some(range) = &arguments.push {
        ChangeRequest::Push {
            range: range.clone(),
            target_ref: arguments.target_ref.clone(),
        }
    } else if let Some(base) = arguments.base.as_ref().filter(|_| arguments.changed) {
        ChangeRequest::Since(base.clone())
    } else if arguments.staged {
        ChangeRequest::Staged
    } else {
        ChangeRequest::All
    };
    let source = if arguments.staged {
        Some(Source::Index)
    } else {
        arguments.rev.clone().map(Source::Commit)
    };
    let request = CheckRequest {
        source,
        changes,
        options: RunOptions {
            timings: arguments.run.timings,
            budget: arguments.run.budget,
            budget_fails: arguments.run.budget_fails,
            fail_fast: arguments.run.fail_fast,
        },
        overrides: arguments.plugins.clone(),
        no_overrides: arguments.no_overrides,
        list_changed_projects: arguments.list_changed_projects,
        commit_message,
        rules: RuleFilter::new(arguments.rules.clone()),
    };

    if arguments.list_changed_projects {
        let format = match arguments.format {
            OutputFormat::Text => SelectionFormat::Text,
            OutputFormat::Json => SelectionFormat::Json,
            OutputFormat::GithubOutput => SelectionFormat::GithubOutput,
            OutputFormat::Sarif | OutputFormat::Github => {
                return Err(Error::config(
                    "--list-changed-projects prints text, json or github-output; pick one of those with --format",
                ));
            }
        };
        let checked = gate_host::check(Path::new("."), &request)?;
        write_out(&render_selection(&checked.report.selection, format)?)?;
        return Ok(0);
    }

    let format = match arguments.format {
        OutputFormat::Text => Format::Text,
        OutputFormat::Json => Format::Json,
        OutputFormat::Sarif => Format::Sarif,
        OutputFormat::Github => Format::Github,
        OutputFormat::GithubOutput => {
            return Err(Error::config(
                "--format github-output lists the changed projects; add --list-changed-projects, or use text, json, sarif or github",
            ));
        }
    };

    let checked = gate_host::check(Path::new("."), &request)?;

    if arguments.run.timings {
        let line = checked
            .timings
            .iter()
            .map(|timing| format!("{} {} ms", timing.phase, timing.duration.as_millis()))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("timings: {line}");
        for call in &checked.reads {
            eprintln!("timings: {}", call_timing(call));
        }
    }

    write_out(&checked.report.render(format)?)?;
    Ok(exit_byte(checked.report.exit_code()))
}

/// Opens the working tree's workspace for a command that reads it, run by the core it pins.
fn workspace() -> Result<Workspace, Error> {
    let workspace = Workspace::open(Path::new("."), Source::WorkTree)?;
    gate_host::lock::check_core(&workspace, gate_host::lock::CORE_VERSION)?;
    Ok(workspace)
}

/// One plugin call of `--timings`: whether it ran or came from the result cache, its time, and
/// the host's share by kind of host call, the largest first.
fn call_timing(call: &gate_host::PluginReads) -> String {
    let scope = call.project.as_deref().unwrap_or("repository");
    if call.from_cache {
        return format!("call {} {scope}: cached", call.plugin);
    }
    let mut host: Vec<_> = call.time.host.iter().collect();
    host.sort_by(|left, right| right.1.time.cmp(&left.1.time).then(left.0.cmp(right.0)));
    let host = host
        .iter()
        .map(|(kind, spent)| {
            format!("{kind} {} ms in {}", spent.time.as_millis(), spent.calls)
        })
        .collect::<Vec<_>>();
    let mut line = format!(
        "call {} {scope}: ran {} ms",
        call.plugin,
        call.time.elapsed.as_millis()
    );
    if !host.is_empty() {
        line.push_str("; host ");
        line.push_str(&host.join(", "));
    }
    line
}

fn list(arguments: &ListArgs) -> Result<u8, Error> {
    let workspace = workspace()?;
    let text = match arguments.format {
        ListFormat::Text => list::text(&workspace, arguments.origin),
        ListFormat::Json => list::json(&workspace)?,
    };
    write_out(&text)?;
    Ok(0)
}

fn explain(rule: &str) -> Result<u8, Error> {
    let subject = list::Subject::parse(rule).map_err(Error::config)?;
    let workspace = workspace()?;
    write_out(&list::explain(&workspace, &subject, rule))?;
    Ok(0)
}

fn baseline_write() -> Result<u8, Error> {
    let written = gate_host::write_baseline(Path::new("."))?;
    let count = written.baseline.len();
    let noun = if count == 1 { "finding" } else { "findings" };
    let mut text = format!("wrote {}: {count} {noun}\n", gate_host::BASELINE_FILE);
    if !written.tracked {
        text.push_str("gate reads tracked files only; run git add ");
        text.push_str(gate_host::BASELINE_FILE);
        text.push('\n');
    }
    write_out(&text)?;
    Ok(0)
}

fn schema(arguments: &SchemaArgs) -> Result<u8, Error> {
    let schema = if arguments.exceptions {
        gate_host::config::exceptions_json_schema()
    } else {
        gate_host::config::json_schema()
    };
    let mut text = serde_json::to_string_pretty(&schema)
        .map_err(|error| Error::environment(error.to_string()))?;
    text.push('\n');
    write_out(&text)?;
    Ok(0)
}

fn sync(arguments: &SyncArgs) -> Result<u8, Error> {
    let workspace = Workspace::open(Path::new("."), Source::WorkTree)?;
    let generators = gate_host::PluginGenerators::load(&workspace, &arguments.plugins)?;
    let outputs = sync::plan(&workspace, &generators)?;
    let owned = sync::owned(&workspace, &generators)?;
    // What the generators reported fails the run as a rule at level error does.
    let mut reported = generators.take_findings();
    gate_host::attach_core_norms(&mut reported, workspace.repository());
    let reported_line = if reported.is_empty() {
        String::new()
    } else {
        format!(", {} reported by generators", reported.len())
    };

    if arguments.check {
        let mut findings = sync::check(&workspace, &outputs, &owned)?;
        gate_host::attach_core_norms(&mut findings, workspace.repository());
        let out_of_date = findings.len();
        findings.extend(reported);
        findings.sort();
        let mut text = findings.iter().map(render_finding).collect::<String>();
        let _ = writeln!(
            text,
            "gate sync --check: {} generated {}, {out_of_date} out of date{reported_line}{}",
            outputs.len(),
            if outputs.len() == 1 { "file" } else { "files" },
            override_lines(&generators)
        );
        write_out(&text)?;
        return Ok(u8::from(!findings.is_empty()));
    }

    let synced = sync::write(&workspace, &outputs, &owned)?;
    let mut text = reported.iter().map(render_finding).collect::<String>();
    for path in &synced.written {
        let _ = writeln!(text, "wrote {path}");
    }
    for path in &synced.removed {
        let _ = writeln!(text, "removed {path} (no longer generated)");
    }
    if let Some(previous) = &synced.hooks_path {
        let was = if previous.is_empty() {
            "unset".to_owned()
        } else {
            format!("was {previous}")
        };
        let _ = writeln!(
            text,
            "set core.hooksPath to {} ({was})",
            sync::HOOKS_DIRECTORY
        );
    }
    if !synced.untracked.is_empty() {
        let _ = writeln!(
            text,
            "gate reads tracked files only; run git add {}",
            synced.untracked.join(" ")
        );
    }
    let _ = writeln!(
        text,
        "gate sync: {} generated, {} written, {} removed{reported_line}{}",
        outputs.len(),
        synced.written.len(),
        synced.removed.len(),
        override_lines(&generators)
    );
    write_out(&text)?;
    Ok(u8::from(!reported.is_empty()))
}

/// `; override <line>` for each `--plugin` build the run uses, as the check report names them.
fn override_lines(generators: &gate_host::PluginGenerators<'_>) -> String {
    generators
        .overrides()
        .iter()
        .fold(String::new(), |mut text, line| {
            let _ = write!(text, "; override {line}");
            text
        })
}

fn run(arguments: &RunCommandArgs) -> Result<u8, Error> {
    target_ref_needs_push(arguments.target_ref.as_deref(), arguments.push.as_deref())?;
    let changes = if let Some(range) = &arguments.push {
        ChangeRequest::Push {
            range: range.clone(),
            target_ref: arguments.target_ref.clone(),
        }
    } else if let Some(base) = arguments.base.as_ref().filter(|_| arguments.changed) {
        ChangeRequest::Since(base.clone())
    } else if arguments.staged {
        ChangeRequest::Staged
    } else {
        ChangeRequest::All
    };
    let input = if arguments.stdin {
        let mut input = Vec::new();
        std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
            .map_err(|error| Error::environment(format!("standard input: {error}")))?;
        Some(input)
    } else {
        None
    };
    let workspace = Workspace::open(Path::new("."), Source::WorkTree)?;
    let request = run::RunRequest {
        name: arguments.name.clone(),
        changes,
        heavy_wrapper_env: std::env::var("GATE_HEAVY_WRAPPER").ok(),
        arguments: arguments.arguments.clone(),
        input,
    };
    let declared = &workspace.root_config().file().tools;
    let mut fetcher = None;
    let mut install = |names: &std::collections::BTreeSet<gate_host::config::Name>|
     -> gate_host::Result<Option<PathBuf>> {
        let wanted = declared
            .iter()
            .filter(|(name, _)| names.contains(*name))
            .map(|(name, tool)| (name.clone(), tool.clone()))
            .collect();
        let fetcher = match &mut fetcher {
            Some(fetcher) => fetcher,
            None => fetcher.insert(Fetcher::from_env()?),
        };
        Ok(tools::install(&wanted, fetcher)?.map(|installed| installed.bin))
    };

    let report = run::run(&workspace, &request, &mut install, &mut |line| {
        eprintln!("{line}");
    })?;

    let mut text = String::new();
    for step in &report.steps {
        let _ = writeln!(
            text,
            "{} {}: {} ({:.2} s)",
            step.scope,
            step.step,
            step.outcome,
            step.duration.as_secs_f64()
        );
    }
    for decision in report
        .selection
        .skipped
        .iter()
        .filter(|_| request.changes != ChangeRequest::All)
    {
        let _ = writeln!(text, "skipped {}: {}", decision.project, decision.reason);
    }
    let _ = writeln!(text, "{}", report.summary());
    write_out(&text)?;
    Ok(exit_byte(report.exit_code()))
}

fn doctor() -> Result<u8, Error> {
    let workspace = Workspace::open(Path::new("."), Source::WorkTree)?;
    let cache = Cache::from_env().ok();
    let checks = doctor::doctor(
        &workspace,
        cache.as_ref(),
        distribution::LAUNCHER.as_bytes(),
    );
    let mut text = checks.iter().fold(String::new(), |mut text, check| {
        let _ = writeln!(text, "{check}");
        text
    });
    let healthy = doctor::healthy(&checks);
    let _ = writeln!(
        text,
        "gate doctor: {}",
        if healthy {
            "ready"
        } else {
            "fix the lines marked fix"
        }
    );
    write_out(&text)?;
    Ok(u8::from(!healthy))
}
