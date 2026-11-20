//! `CallState`: the rules of one `check` call share one state, built from the call before its
//! first rule, and the next call starts from a new one.

use std::cell::Cell;

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{
    Call, CallState, CheckRequest, Finding, GitHost, Host, Location, Result, Rule, RuleRun, Run,
    TreeHost,
};

/// Options that take no key.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
struct NoOptions {}

/// The state of the test plugin: the visits of the call's rules.
struct Visits;

/// What one call owns: its project, and the visits counted so far.
struct Tally<'h> {
    project: &'h str,
    visits: Cell<u32>,
}

/// What the rules see: the owner, borrowed.
struct Seen<'o> {
    tally: &'o Tally<'o>,
}

impl CallState for Visits {
    type Owner<'h> = Tally<'h>;
    type State<'o> = Seen<'o>;

    fn own<'h>(call: &Call<'h>) -> Tally<'h> {
        Tally {
            project: call.project(),
            visits: Cell::new(0),
        }
    }

    fn state<'o>(owner: &'o Tally<'_>) -> Seen<'o> {
        Seen { tally: owner }
    }
}

/// Counts one visit, and reports the project and the count it sees.
#[derive(Default)]
struct Count;

impl Rule<Visits> for Count {
    type Options = NoOptions;
    const NAME: &'static str = "count";
    const TITLE: &'static str = "Counts the visits of its call";

    fn check(&self, run: &mut Run<'_, NoOptions, Visits>) -> Result<()> {
        run.with_call(|run, seen| {
            let visits = seen.tally.visits.get() + 1;
            seen.tally.visits.set(visits);
            run.report().finding(
                "",
                Location::line("a.txt", 1),
                format!("{} visits {visits}", seen.tally.project),
            );
            Ok(())
        })
    }
}

gate_sdk::plugin! {
    name: "visits",
    version: "0.0.0",
    options: NoOptions,
    call: Visits,
    rules: [Count],
    generators: [],
    presets: [],
}

struct Empty;

impl TreeHost for Empty {
    fn list(&self, _prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn read(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }

    fn changed(&self) -> Option<Vec<String>> {
        None
    }
}

impl Host for Empty {
    fn git(&self) -> Option<&dyn GitHost> {
        None
    }

    fn log(&self, _message: &str) {}
}

/// One call for `project` that runs the rule `times` times.
fn call(project: &str, times: usize) -> Vec<String> {
    let request = CheckRequest {
        project: project.to_owned(),
        rules: (0..times)
            .map(|_| RuleRun {
                name: "count".to_owned(),
                options: "{}".to_owned(),
                options_file: None,
                exceptions: Vec::new(),
            })
            .collect(),
        plugin_options: "{}".to_owned(),
        languages: "{}".to_owned(),
        norms: Vec::new(),
        rule_decisions: Vec::new(),
        config_file: String::new(),
    };
    gate_sdk::check(&PLUGIN, &Empty, &request)
        .expect("the call runs")
        .into_iter()
        .map(|finding: Finding| finding.message)
        .collect()
}

#[test]
fn the_rules_of_one_call_share_its_state_and_the_next_call_starts_anew() {
    assert_eq!(
        call("api", 3),
        ["api visits 1", "api visits 2", "api visits 3"]
    );
    // A second call, of another project, does not see the first one's state.
    assert_eq!(call("web", 2), ["web visits 1", "web visits 2"]);
}
