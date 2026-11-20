//! Environment coverage: a project's example environment file lists exactly the variables its code
//! reads. Each rule finds its own reads, by its own means, and hands them to [`check_example`], which
//! parses the example file, compares and reports under the aspects of [`ASPECTS`]. A rule that
//! calls it declares those aspects and the options `example-file` and `external-keys`.
//!
//! No finding holds a value of the example file: a line is read for its key only.

use std::collections::{BTreeMap, BTreeSet};

use gate_sdk::schemars::JsonSchema;
use gate_sdk::serde::Deserialize;
use gate_sdk::{Location, Report, Span};

/// The aspects [`check_example`] reports, in the order a rule's page lists them.
pub const ASPECTS: [&str; 5] = [
    "undeclared",
    "unread",
    "dynamic",
    "example-missing",
    "example-syntax",
];

/// The default of a rule's `example-file` option.
pub const EXAMPLE_FILE: &str = ".env.example";

/// One read of an environment variable that a rule found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvRead {
    /// The variable's name; `None` when the rule cannot name it (the read is `dynamic`).
    pub key: Option<String>,
    /// The file of the read, relative to the call's root.
    pub path: String,
    /// Where in the file.
    pub span: Span,
    /// The text of the expression that names the key, for a `dynamic` read's message and key.
    pub expression: String,
}

/// A key something other than the code the rule reads reads (the runtime itself, a script, an
/// image), with what reads it; it counts as read. The item of a rule's `external-keys` option.
#[derive(Clone, Debug, Deserialize, JsonSchema, Eq, PartialEq)]
#[serde(crate = "gate_sdk::serde", deny_unknown_fields)]
#[schemars(crate = "gate_sdk::schemars")]
pub struct ExternalKey {
    /// The variable's name.
    pub key: String,
    /// What reads it, for the reader of the configuration.
    pub reader: String,
}

/// One key the example file lists, and its one-based line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExampleLine {
    /// The variable's name.
    pub key: String,
    /// Its one-based line.
    pub line: usize,
}

/// The example file's lines: each key with its line, and the lines that are no assignment.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Example {
    /// The lines with a key, in file order.
    pub keys: Vec<ExampleLine>,
    /// The one-based lines that are neither blank, a comment, nor `KEY=value`.
    pub invalid: Vec<usize>,
}

/// Parses an example environment file: `KEY=value`, `export KEY=value`, `KEY=` and quoted values
/// are lines with a key; blank lines and lines starting with `#` are skipped. A key is a letter or
/// `_` followed by letters, digits and `_`. The values are never kept.
#[must_use]
pub fn parse_example(text: &str) -> Example {
    let mut example = Example::default();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let assignment = line.strip_prefix("export ").map_or(line, str::trim_start);
        match assignment.split_once('=') {
            Some((key, _)) if is_key(key.trim_end()) => example.keys.push(ExampleLine {
                key: key.trim_end().to_owned(),
                line: number,
            }),
            _ => example.invalid.push(number),
        }
    }
    example
}

/// Whether `name` is an environment variable name: a letter or `_`, then letters, digits and `_`.
#[must_use]
pub fn is_key(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Compares the example file at `example_path` (`example`: its text, `None` when it is not tracked)
/// with `reads` and `external`, and reports:
///
/// - `undeclared`: a key a read names has no line, once per file that reads it, at its first read;
/// - `unread`: a line names a key no read names and `external` does not list, at the line;
/// - `dynamic`: a read whose key the rule cannot name, at the read;
/// - `example-missing`: the example file is not tracked and some read names a key, once;
/// - `example-syntax`: a line of the example file that is no assignment, at the line.
///
/// The key of a finding is the variable's name (for `dynamic`, the expression's text; for
/// `example-missing`, the example's path); no message, help, target or key holds a value.
pub fn check_example(
    report: &mut Report,
    example_path: &str,
    example: Option<&str>,
    reads: &[EnvRead],
    external: &[ExternalKey],
) {
    for read in reads.iter().filter(|read| read.key.is_none()) {
        report
            .finding(
                "dynamic",
                Location::span(read.path.clone(), read.span),
                format!(
                    "the environment variable `{}` reads is not named by a literal, so it cannot \
                     be checked against {example_path}",
                    read.expression
                ),
            )
            .target(format!("{}:{}", read.path, read.span.line))
            .help(
                "name the variable with a string literal, a `const` bound to one, or a loop over a \
                 literal array of names in the same file",
            )
            .key(read.expression.clone());
    }

    let named: BTreeMap<&str, Vec<&EnvRead>> =
        reads.iter().fold(BTreeMap::new(), |mut named, read| {
            if let Some(key) = &read.key {
                named.entry(key.as_str()).or_default().push(read);
            }
            named
        });

    let Some(text) = example else {
        if !named.is_empty() {
            report
                .finding(
                    "example-missing",
                    Location::file(example_path),
                    format!(
                        "{example_path} is not tracked, and the code reads {} environment \
                         variable(s)",
                        named.len()
                    ),
                )
                .target(example_path.to_owned())
                .help(format!(
                    "add {example_path} with one `KEY=` line, and a placeholder value, for each \
                     variable the code reads"
                ))
                .key(example_path.to_owned());
        }
        return;
    };

    let example = parse_example(text);
    for line in &example.invalid {
        report
            .finding(
                "example-syntax",
                Location::line(example_path, *line),
                format!(
                    "line {line} of {example_path} is neither `KEY=value`, a comment nor blank"
                ),
            )
            .target(format!("{example_path}:{line}"))
            .help("write the line as `KEY=value` or `export KEY=value`, or as a `#` comment")
            .key(format!("line {line}"));
    }

    let listed: BTreeSet<&str> = example.keys.iter().map(|line| line.key.as_str()).collect();
    for (key, reads) in &named {
        if listed.contains(key) {
            continue;
        }
        let mut files = BTreeSet::new();
        for read in reads.iter().filter(|read| files.insert(read.path.as_str())) {
            report
                .finding(
                    "undeclared",
                    Location::span(read.path.clone(), read.span),
                    format!("{key} is read here but {example_path} has no line for it"),
                )
                .target(format!("{}:{key}", read.path))
                .help(format!(
                    "add `{key}=` with a placeholder value to {example_path}, or stop reading it"
                ))
                .key((*key).to_owned());
        }
    }

    let outside: BTreeSet<&str> = external.iter().map(|item| item.key.as_str()).collect();
    for line in &example.keys {
        let key = line.key.as_str();
        if named.contains_key(key) || outside.contains(key) {
            continue;
        }
        report
            .finding(
                "unread",
                Location::line(example_path, line.line),
                format!("{example_path} lists {key}, which nothing reads"),
            )
            .target(format!("{example_path}:{key}"))
            .help(
                "remove the line, or, when something other than the code reads it, list it in \
                 `external-keys` with what reads it",
            )
            .key(key.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUE: &str = "s3cr3t-value";

    fn read(key: Option<&str>, path: &str, line: u32) -> EnvRead {
        EnvRead {
            key: key.map(str::to_owned),
            path: path.to_owned(),
            span: Span::line(line),
            expression: key.map_or_else(|| "name".to_owned(), |key| format!("\"{key}\"")),
        }
    }

    fn check(example: Option<&str>, reads: &[EnvRead], external: &[ExternalKey]) -> Vec<String> {
        let mut report = Report::new("rule");
        check_example(&mut report, ".env.example", example, reads, external);
        report
            .into_findings()
            .into_iter()
            .map(|finding| {
                for text in [
                    Some(&finding.message),
                    finding.help.as_ref(),
                    finding.target.as_ref(),
                    finding.key.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    assert!(!text.contains(VALUE), "a value in {text:?}");
                }
                let line = match &finding.location {
                    Some(Location::File(file)) => file.span.map_or(0, |span| span.line),
                    _ => 0,
                };
                format!(
                    "{} {}:{line} {}",
                    finding.aspect,
                    finding
                        .location
                        .as_ref()
                        .and_then(Location::path)
                        .unwrap_or(""),
                    finding.key.unwrap_or_default()
                )
            })
            .collect()
    }

    #[test]
    fn the_parser_keeps_keys_and_lines_and_reports_other_lines_without_their_text() {
        let text = format!(
            "# a comment\n\nA={VALUE}\nexport B=\"{VALUE}\"\n  C=\nD='quoted value'\nnot a line\n1X=y\n"
        );
        let example = parse_example(&text);
        let keys: Vec<(&str, usize)> = example
            .keys
            .iter()
            .map(|line| (line.key.as_str(), line.line))
            .collect();
        assert_eq!(keys, [("A", 3), ("B", 4), ("C", 5), ("D", 6)]);
        assert_eq!(example.invalid, [7, 8]);
        assert_eq!(
            check(Some(&text), &[], &[])[..2],
            [
                "example-syntax .env.example:7 line 7".to_owned(),
                "example-syntax .env.example:8 line 8".to_owned()
            ]
        );
    }

    #[test]
    fn a_key_read_without_a_line_is_undeclared_once_per_file() {
        let reads = [
            read(Some("A"), "src/a.ts", 3),
            read(Some("A"), "src/a.ts", 9),
            read(Some("A"), "src/b.ts", 2),
            read(Some("B"), "src/a.ts", 4),
        ];
        let example = format!("B={VALUE}\n");
        assert_eq!(
            check(Some(&example), &reads, &[]),
            ["undeclared src/a.ts:3 A", "undeclared src/b.ts:2 A"]
        );
        let declared = format!("A={VALUE}\nB=\n");
        assert_eq!(check(Some(&declared), &reads, &[]), Vec::<String>::new());
    }

    #[test]
    fn a_line_nothing_reads_is_unread_unless_external_keys_list_it() {
        let example = format!("A={VALUE}\nNODE_OPTIONS={VALUE}\n");
        let reads = [read(Some("A"), "src/a.ts", 1)];
        assert_eq!(
            check(Some(&example), &reads, &[]),
            ["unread .env.example:2 NODE_OPTIONS"]
        );
        let external = [ExternalKey {
            key: "NODE_OPTIONS".to_owned(),
            reader: "Node.js".to_owned(),
        }];
        assert_eq!(
            check(Some(&example), &reads, &external),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_read_without_a_key_is_dynamic_and_a_named_one_is_not() {
        let reads = [read(None, "src/a.ts", 5), read(Some("A"), "src/a.ts", 6)];
        assert_eq!(
            check(Some("A=\n"), &reads, &[]),
            ["dynamic src/a.ts:5 name"]
        );
        assert_eq!(check(Some("A=\n"), &reads[1..], &[]), Vec::<String>::new());
    }

    #[test]
    fn a_missing_example_is_reported_once_when_a_read_names_a_key() {
        let reads = [
            read(Some("A"), "src/a.ts", 1),
            read(Some("B"), "src/b.ts", 1),
        ];
        assert_eq!(
            check(None, &reads, &[]),
            ["example-missing .env.example:0 .env.example"]
        );
        assert_eq!(check(None, &[], &[]), Vec::<String>::new());
        assert_eq!(
            check(None, &[read(None, "src/a.ts", 2)], &[]),
            ["dynamic src/a.ts:2 name"]
        );
    }
}
