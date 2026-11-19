//! Finding locations, and a collector that reports each distinct finding once.

use gate_sdk::{Location, Run};

/// A location in `path`: the line when there is one, the whole file otherwise.
#[must_use]
pub fn location(path: &str, line: Option<usize>) -> Location {
    match line {
        Some(line) => Location::line(path, line),
        None => Location::file(path),
    }
}

/// One finding before it is reported: aspect, path, line and message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pending {
    /// The aspect, or empty.
    pub aspect: &'static str,
    /// The file.
    pub path: String,
    /// The one-based line, or `None` for the whole file.
    pub line: Option<u32>,
    /// The message.
    pub message: String,
    /// The stable key of what the finding is about, when the message embeds a path or an option
    /// value; see `gate_sdk::Finding::key`.
    pub key: Option<String>,
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

    /// Adds a finding with the stable key `key` unless an identical one is already collected.
    pub fn push_keyed(
        &mut self,
        aspect: &'static str,
        path: impl Into<String>,
        line: Option<u32>,
        key: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.add(Pending {
            aspect,
            path: path.into(),
            line,
            message: message.into(),
            key: Some(key.into()),
        });
    }

    fn add(&mut self, finding: Pending) {
        if !self.pending.contains(&finding) {
            self.pending.push(finding);
        }
    }

    /// Reports the collected findings on `run`, each with the help `help` gives for its aspect.
    pub fn report<O>(self, run: &mut Run<'_, O, crate::Nest>, help: impl Fn(&str) -> &'static str) {
        for finding in self.pending {
            let help = help(finding.aspect);
            let location = location(
                &finding.path,
                finding.line.and_then(|line| usize::try_from(line).ok()),
            );
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
