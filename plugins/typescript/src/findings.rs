//! A collector that reports each distinct finding once, in the order found.

use gate_sdk::{Location, Run};

/// One finding before it is reported.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Pending {
    aspect: &'static str,
    path: String,
    /// The one-based line, or `None` for the whole file.
    line: Option<u32>,
    message: String,
    /// The stable key of what the finding is about; see `gate_sdk::Finding::key`.
    key: Option<String>,
}

/// The findings of a run in the order they were found, each distinct one kept once: two checks
/// that find the same thing report it once.
#[derive(Debug, Default)]
pub struct Findings {
    pending: Vec<Pending>,
}

impl Findings {
    /// Adds a finding unless an identical one is already collected.
    pub fn push(
        &mut self,
        aspect: &'static str,
        path: impl Into<String>,
        line: Option<u32>,
        message: impl Into<String>,
    ) {
        self.add(Pending {
            aspect,
            path: path.into(),
            line,
            message: message.into(),
            key: None,
        });
    }

    fn add(&mut self, finding: Pending) {
        if !self.pending.contains(&finding) {
            self.pending.push(finding);
        }
    }

    /// Reports the collected findings on `run`, each with the help `help` gives for its aspect.
    pub fn report<O>(self, run: &mut Run<'_, O>, help: impl Fn(&str) -> &'static str) {
        for finding in self.pending {
            let help = help(finding.aspect);
            let location = match finding.line.and_then(|line| usize::try_from(line).ok()) {
                Some(line) => Location::line(&finding.path, line),
                None => Location::file(&finding.path),
            };
            let reported = run
                .report()
                .finding(finding.aspect, location, finding.message)
                .help(help);
            if let Some(key) = finding.key {
                reported.key(key);
            }
        }
    }
}
