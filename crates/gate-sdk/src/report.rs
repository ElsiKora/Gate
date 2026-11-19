//! Findings and where they point.

/// A one-based line and column range; the end is inclusive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Span {
    /// The first line.
    pub line: u32,
    /// The first column of the first line, counted in characters.
    pub column: u32,
    /// The last line.
    pub end_line: u32,
    /// The last column of the last line, counted in characters.
    pub end_column: u32,
}

impl Span {
    /// Line `line` from column 1 to column 1: a span that names the line without knowing its
    /// length. Reports show the line; an annotation marks its first column.
    #[must_use]
    pub fn line(line: u32) -> Self {
        Self {
            line,
            column: 1,
            end_line: line,
            end_column: 1,
        }
    }
}

/// A tracked path relative to the call's root, and optionally where in it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileLocation {
    /// The path, relative to the call's root (`../` for a granted path outside it).
    pub path: String,
    /// Where in the file; `None` for the whole file.
    pub span: Option<Span>,
}

/// Where a finding is: in a file, or on a commit for a rule that reads history.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Location {
    /// In a file.
    File(FileLocation),
    /// On a commit, by its full id; reports and exceptions show the full id.
    Commit(String),
}

impl Location {
    /// A whole file.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self::File(FileLocation {
            path: path.into(),
            span: None,
        })
    }

    /// One line of a file (one-based); a line past `u32::MAX` is clamped.
    #[must_use]
    pub fn line(path: impl Into<String>, line: usize) -> Self {
        let line = u32::try_from(line).unwrap_or(u32::MAX);
        Self::File(FileLocation {
            path: path.into(),
            span: Some(Span::line(line)),
        })
    }

    /// A range in a file.
    #[must_use]
    pub fn span(path: impl Into<String>, span: Span) -> Self {
        Self::File(FileLocation {
            path: path.into(),
            span: Some(span),
        })
    }

    /// A commit, by its full id.
    #[must_use]
    pub fn commit(id: impl Into<String>) -> Self {
        Self::Commit(id.into())
    }

    /// The file's path; `None` for a commit.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::File(file) => Some(&file.path),
            Self::Commit(_) => None,
        }
    }
}

/// One violation. The rule id, level and norm are added by the host from the declaration and
/// configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    /// The rule's name inside the plugin; set by the SDK.
    pub rule: String,
    /// A named part of the rule; empty when the rule has one.
    pub aspect: String,
    pub message: String,
    pub location: Option<Location>,
    /// What the finding is about, in the rule's documented grammar; exceptions match it.
    pub target: Option<String>,
    /// What to do about it.
    pub help: Option<String>,
}

impl Finding {
    /// Sets what the finding is about, in the rule's documented grammar (for example
    /// `src/user.entity.ts:UserEntity.parent`); exceptions match it.
    pub fn target(&mut self, target: impl Into<String>) -> &mut Self {
        self.target = Some(target.into());
        self
    }

    /// Sets what to do about it.
    pub fn help(&mut self, help: impl Into<String>) -> &mut Self {
        self.help = Some(help.into());
        self
    }
}

/// The findings of one rule run, in the order reported; the host sorts the report.
#[derive(Debug, Default)]
pub struct Report {
    rule: String,
    findings: Vec<Finding>,
}

impl Report {
    /// An empty report for the rule named `rule`.
    #[must_use]
    pub fn new(rule: impl Into<String>) -> Self {
        Self {
            rule: rule.into(),
            findings: Vec::new(),
        }
    }

    /// Reports one finding of `aspect` (one of the rule's declared aspects, or empty) at `location`
    /// (a [`Location`], or `None` for a finding about no file); set its target and help on the
    /// returned finding.
    pub fn finding(
        &mut self,
        aspect: &str,
        location: impl Into<Option<Location>>,
        message: impl Into<String>,
    ) -> &mut Finding {
        self.findings.push(Finding {
            rule: self.rule.clone(),
            aspect: aspect.to_owned(),
            message: message.into(),
            location: location.into(),
            target: None,
            help: None,
        });
        let last = self.findings.len() - 1;
        &mut self.findings[last]
    }

    /// The findings so far.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// The findings, consumed.
    #[must_use]
    pub fn into_findings(self) -> Vec<Finding> {
        self.findings
    }
}
