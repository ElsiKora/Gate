//! Versions and the ranges `engines.node` writes: exact versions, comparators (`>=`, `>`, `<=`,
//! `<`, `=`), partial versions (`24`, `24.x`), caret, tilde, `*`, hyphen ranges and `||`.

/// A version `major.minor.patch`; pre-release and build parts are not compared.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Version {
    pub(crate) major: u64,
    pub(crate) minor: u64,
    pub(crate) patch: u64,
}

/// Whether `text` is one exact version: `x.y.z`, optionally with a pre-release and build part.
pub(crate) fn is_exact(text: &str) -> bool {
    let core = text.split(['-', '+']).next().unwrap_or_default();
    let rest = &text[core.len()..];
    let mut parts = core.split('.');
    let numbers = (0..3).all(|_| parts.next().is_some_and(number));
    numbers
        && parts.next().is_none()
        && rest.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '+' | '.')
        })
}

fn number(part: &str) -> bool {
    !part.is_empty()
        && part.chars().all(|character| character.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'))
}

/// `text` as an exact version: what [`is_exact`] accepts, the same in every rule, read as its
/// `x.y.z`.
pub(crate) fn exact(text: &str) -> Option<Version> {
    if !is_exact(text) {
        return None;
    }
    let core = text.split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.');
    let version = Version {
        major: parts.next().filter(|part| number(part))?.parse().ok()?,
        minor: parts.next().filter(|part| number(part))?.parse().ok()?,
        patch: parts.next().filter(|part| number(part))?.parse().ok()?,
    };
    parts.next().is_none().then_some(version)
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

#[derive(Clone, Copy, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

impl Comparator {
    fn admits(self, version: Version) -> bool {
        match self.op {
            Op::Greater => version > self.version,
            Op::GreaterOrEqual => version >= self.version,
            Op::Less => version < self.version,
            Op::LessOrEqual => version <= self.version,
        }
    }
}

/// A range: alternatives joined by `||`, each a set of comparators that all hold.
#[derive(Debug)]
pub(crate) struct Range(Vec<Vec<Comparator>>);

impl Range {
    /// Parses `text`; `Ok(None)` when it is not a range this module reads.
    ///
    /// # Errors
    /// A configuration error naming `text` when a version in it is so large that its next major,
    /// minor or patch does not fit in 64 bits.
    pub(crate) fn parse(text: &str) -> gate_sdk::Result<Option<Self>> {
        let mut alternatives = Vec::new();
        for alternative in text.split("||") {
            match alternative_of(alternative.trim()) {
                Ok(Some(set)) => alternatives.push(set),
                Ok(None) => return Ok(None),
                Err(Overflow) => {
                    return Err(gate_sdk::Error::config(format!(
                        "engines.node \"{text}\" has a version part too large to step past; \
                         write the Node range the project runs on, such as \">=24.11.0 <25\""
                    )));
                }
            }
        }
        Ok(Some(Self(alternatives)))
    }

    /// Whether `version` is in the range.
    pub(crate) fn admits(&self, version: Version) -> bool {
        self.0
            .iter()
            .any(|set| set.iter().all(|comparator| comparator.admits(version)))
    }

    /// Whether the range admits a version of a major below `major`.
    pub(crate) fn admits_below(&self, major: u64) -> bool {
        self.0.iter().any(|set| {
            let lowest = set
                .iter()
                .filter_map(|comparator| match comparator.op {
                    Op::GreaterOrEqual => Some(comparator.version),
                    Op::Greater => Some(Version {
                        patch: comparator.version.patch.saturating_add(1),
                        ..comparator.version
                    }),
                    Op::Less | Op::LessOrEqual => None,
                })
                .max()
                .unwrap_or(Version {
                    major: 0,
                    minor: 0,
                    patch: 0,
                });
            lowest.major < major && set.iter().all(|comparator| comparator.admits(lowest))
        })
    }
}

/// A version part whose next value does not fit in 64 bits.
#[derive(Debug)]
struct Overflow;

/// One alternative's comparators; `Ok(None)` when it is not one this module reads.
fn alternative_of(text: &str) -> Result<Option<Vec<Comparator>>, Overflow> {
    if text.is_empty() || text == "*" || text == "x" {
        return Ok(Some(Vec::new()));
    }
    if let Some((low, high)) = text.split_once(" - ") {
        let (Some(mut set), Some(upper)) = (
            bound(Op::GreaterOrEqual, low.trim())?,
            bound(Op::LessOrEqual, high.trim())?,
        ) else {
            return Ok(None);
        };
        set.extend(upper);
        return Ok(Some(set));
    }
    let mut set = Vec::new();
    let mut tokens = text.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        // An operator written apart from its version (`>= 24`).
        let token = if matches!(token, ">" | ">=" | "<" | "<=" | "=" | "^" | "~") {
            let Some(version) = tokens.next() else {
                return Ok(None);
            };
            format!("{token}{version}")
        } else {
            token.to_owned()
        };
        let Some(found) = comparators(&token)? else {
            return Ok(None);
        };
        set.extend(found);
    }
    Ok(Some(set))
}

fn comparators(token: &str) -> Result<Option<Vec<Comparator>>, Overflow> {
    for (prefix, op) in [
        (">=", Op::GreaterOrEqual),
        ("<=", Op::LessOrEqual),
        (">", Op::Greater),
        ("<", Op::Less),
    ] {
        if let Some(version) = token.strip_prefix(prefix) {
            return bound(op, version);
        }
    }
    if let Some(version) = token.strip_prefix('^') {
        let Some((low, parts)) = partial(version) else {
            return Ok(None);
        };
        let high = match (low.major, parts) {
            (0, 1) => next_major(low)?,
            (0, _) if low.minor == 0 && parts == 3 => Version {
                patch: low.patch.checked_add(1).ok_or(Overflow)?,
                ..low
            },
            (0, _) => next_minor(low)?,
            _ => next_major(low)?,
        };
        return Ok(Some(between(low, high)));
    }
    if let Some(version) = token.strip_prefix('~') {
        let Some((low, parts)) = partial(version) else {
            return Ok(None);
        };
        let high = if parts == 1 {
            next_major(low)?
        } else {
            next_minor(low)?
        };
        return Ok(Some(between(low, high)));
    }
    let version = token.strip_prefix('=').unwrap_or(token);
    let Some((low, parts)) = partial(version) else {
        return Ok(None);
    };
    Ok(Some(match parts {
        1 => between(low, next_major(low)?),
        2 => between(low, next_minor(low)?),
        _ => vec![
            Comparator {
                op: Op::GreaterOrEqual,
                version: low,
            },
            Comparator {
                op: Op::LessOrEqual,
                version: low,
            },
        ],
    }))
}

/// A comparator `op` with a possibly partial version: `<25` is `<25.0.0`, `<=24` is `<25.0.0`,
/// `>24` is `>=25.0.0`.
fn bound(op: Op, version: &str) -> Result<Option<Vec<Comparator>>, Overflow> {
    let Some((low, parts)) = partial(version.trim()) else {
        return Ok(None);
    };
    let after = || match parts {
        1 => next_major(low),
        2 => next_minor(low),
        _ => Ok(low),
    };
    let comparator = match (op, parts) {
        (Op::LessOrEqual, 1 | 2) => Comparator {
            op: Op::Less,
            version: after()?,
        },
        (Op::Greater, 1 | 2) => Comparator {
            op: Op::GreaterOrEqual,
            version: after()?,
        },
        _ => Comparator { op, version: low },
    };
    Ok(Some(vec![comparator]))
}

/// A version with one to three parts, the missing ones (or `x`, `*`) as zero, and how many
/// parts were given.
fn partial(text: &str) -> Option<(Version, usize)> {
    let text = text.strip_prefix('v').unwrap_or(text);
    let core = text.split(['-', '+']).next()?;
    let mut numbers = Vec::new();
    for part in core.split('.') {
        if matches!(part, "x" | "X" | "*") {
            break;
        }
        numbers.push(part.parse::<u64>().ok()?);
    }
    if numbers.is_empty() || numbers.len() > 3 {
        return None;
    }
    let parts = numbers.len();
    numbers.resize(3, 0);
    Some((
        Version {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
        },
        parts,
    ))
}

fn next_major(version: Version) -> Result<Version, Overflow> {
    Ok(Version {
        major: version.major.checked_add(1).ok_or(Overflow)?,
        minor: 0,
        patch: 0,
    })
}

fn next_minor(version: Version) -> Result<Version, Overflow> {
    Ok(Version {
        minor: version.minor.checked_add(1).ok_or(Overflow)?,
        patch: 0,
        ..version
    })
}

fn between(low: Version, high: Version) -> Vec<Comparator> {
    vec![
        Comparator {
            op: Op::GreaterOrEqual,
            version: low,
        },
        Comparator {
            op: Op::Less,
            version: high,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{Range, exact, is_exact};

    fn version(text: &str) -> super::Version {
        exact(text).expect("an exact version")
    }

    #[test]
    fn exact_versions_and_the_rest() {
        for text in ["1.2.3", "0.0.0", "10.20.30-rc.1", "1.2.3+build.5"] {
            assert!(is_exact(text), "{text}");
            assert!(exact(text).is_some(), "{text}");
        }
        for text in [
            "^1.2.3",
            "~1.2.3",
            "*",
            "latest",
            "1.2",
            "01.2.3",
            "1.2.3 - 2",
            ">=1.2.3",
            "1.2.x",
        ] {
            assert!(!is_exact(text), "{text}");
            assert!(exact(text).is_none(), "{text}");
        }
    }

    #[test]
    fn a_pre_release_is_exact_and_reads_as_its_version() {
        let read = exact("24.11.0-rc.1+build.5").expect("exact");
        assert_eq!((read.major, read.minor, read.patch), (24, 11, 0));
    }

    #[test]
    fn ranges_admit_what_npm_admits() {
        let cases = [
            (">=24.11.0 <25", "24.11.0", true),
            (">=24.11.0 <25", "24.10.9", false),
            (">=24.11.0 <25", "25.0.0", false),
            (">=26", "26.13.1", true),
            ("^24", "24.99.0", true),
            ("^24", "25.0.0", false),
            ("~24.1", "24.1.9", true),
            ("~24.1", "24.2.0", false),
            ("24.x", "24.3.1", true),
            ("22 || 24", "24.0.0", true),
            ("22 || 24", "23.0.0", false),
            ("20.0.0 - 24", "24.9.9", true),
            (">= 24", "24.0.0", true),
            ("<=24", "24.5.0", true),
            (">24", "24.5.0", false),
            ("*", "1.0.0", true),
        ];
        for (range, at, admitted) in cases {
            let parsed = Range::parse(range)
                .ok()
                .flatten()
                .unwrap_or_else(|| panic!("{range} parses"));
            assert_eq!(parsed.admits(version(at)), admitted, "{range} at {at}");
        }
        assert!(matches!(Range::parse(">=foo"), Ok(None)));
    }

    #[test]
    fn a_version_part_too_large_to_step_is_a_configuration_error() {
        for range in [
            "<=18446744073709551615",
            "^18446744073709551615",
            "~1.18446744073709551615",
        ] {
            let error = Range::parse(range).expect_err(range);
            assert!(error.is_config(), "{range}");
            assert!(
                error.message().contains(range),
                "{range}: {}",
                error.message()
            );
        }
    }

    #[test]
    fn an_earlier_major_is_seen_in_any_alternative() {
        let below = |range: &str| {
            Range::parse(range)
                .ok()
                .flatten()
                .expect("parses")
                .admits_below(24)
        };
        assert!(!below(">=24.11.0 <25"));
        assert!(below(">=22"));
        assert!(below("20 || >=24"));
        assert!(below("<27"));
        assert!(!below("^24"));
        assert!(!below(">23"));
    }
}
