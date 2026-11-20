//! The standard: what every kind shares, and the few differences, each with its reason. The rules
//! read these constants, so the table a reviewer reads is the table the rules apply.

use crate::settings::Kind;

/// A row of the standard where kinds differ.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Row {
    /// `private: true`.
    Private,
    /// `exports`, `files`, `sideEffects` and `repository`.
    Published,
    /// The `dev` script.
    Dev,
    /// `module` and `moduleResolution` in `tsconfig`.
    Resolution,
}

/// One difference: the kinds whose cell is not "same", what the cell says, and why.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Difference {
    pub(crate) row: Row,
    pub(crate) kinds: &'static [Kind],
    pub(crate) cell: &'static str,
    pub(crate) reason: &'static str,
}

/// Every cell of the standard that is not "same". A kind not named in a row has the common cell.
pub(crate) const DIFFERENCES: &[Difference] = &[
    Difference {
        row: Row::Private,
        kinds: &[Kind::Library],
        cell: "a publishConfig, unless private: true marks a package of the workspace",
        reason: "a published package says where it is published; a workspace package is never \
                 published",
    },
    Difference {
        row: Row::Published,
        kinds: &[Kind::Library],
        cell: "exports, files, sideEffects and repository; exports alone when private",
        reason: "they say what a consumer may import and what the tarball holds; a workspace \
                 package is still imported through its exports",
    },
    Difference {
        row: Row::Dev,
        kinds: &[Kind::Frontend],
        cell: "a dev script",
        reason: "a development server is part of a frontend's job; the other kinds have no single \
                 such command",
    },
    Difference {
        row: Row::Resolution,
        kinds: &[Kind::Frontend],
        cell: "module ESNext and moduleResolution Bundler",
        reason: "a bundler resolves a frontend's imports, so Bundler is the resolution that \
                 matches what runs",
    },
];

/// The difference of `row` for `kind`, when its cell is not the common one.
pub(crate) fn difference(kind: Kind, row: Row) -> Option<&'static Difference> {
    DIFFERENCES
        .iter()
        .find(|difference| difference.row == row && difference.kinds.contains(&kind))
}

/// The `package.json` fields every project has.
pub(crate) const FIELDS: [&str; 4] = ["name", "version", "license", "type"];

/// The value of `type` in every kind.
pub(crate) const MODULE_TYPE: &str = "module";

/// The fields a published package has and no other project does.
pub(crate) const PUBLISHED: [&str; 4] = ["exports", "files", "sideEffects", "repository"];

/// The scripts every project has, by job: a bare name checks, `<name>:fix` writes.
pub(crate) const SCRIPTS: [&str; 5] = ["build", "lint", "lint:fix", "typecheck", "test"];

/// The formatting scripts, which only a project that is not a workspace member has.
pub(crate) const FORMAT_SCRIPTS: [&str; 2] = ["format", "format:fix"];

/// Why a workspace member has no formatting scripts.
pub(crate) const FORMAT_REASON: &str = "formatting is repository-wide, with one configuration at the \
     root; the root's `format` and `format:fix` cover every member";

/// Names whose job has a standard name, and that name.
pub(crate) const SYNONYMS: &[(&str, &str)] = &[
    ("lint:types", "typecheck"),
    ("type-check", "typecheck"),
    ("tsc", "typecheck"),
    ("check-types", "typecheck"),
    ("format:check", "format"),
    ("prettier:check", "format"),
    ("prettier", "format"),
    ("prettier:fix", "format:fix"),
    ("prettier:write", "format:fix"),
    ("format:write", "format:fix"),
    ("lint:ts", "lint"),
    ("eslint", "lint"),
    ("lint:ts:fix", "lint:fix"),
    ("eslint:fix", "lint:fix"),
];

/// The one package manager.
pub(crate) const PACKAGE_MANAGER: &str = "pnpm";

/// The lockfiles of the other package managers.
pub(crate) const OTHER_LOCKFILES: [&str; 5] = [
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
];

/// The Prettier settings of every repository: Prettier's defaults but for these keys.
pub(crate) const PRETTIER: [(&str, &str); 3] = [
    ("printWidth", "120"),
    ("proseWrap", "\"preserve\""),
    ("trailingComma", "\"all\""),
];

/// The shared `ESLint` configuration every repository's root configuration imports.
pub(crate) const SHARED_ESLINT: &str = "@elsikora/eslint-config";

/// The `tsconfig` flags every configuration has on, through the base.
pub(crate) const STRICT_FLAGS: [&str; 10] = [
    "strict",
    "noUncheckedIndexedAccess",
    "exactOptionalPropertyTypes",
    "noImplicitOverride",
    "noImplicitReturns",
    "noFallthroughCasesInSwitch",
    "useUnknownInCatchVariables",
    "forceConsistentCasingInFileNames",
    "isolatedModules",
    "verbatimModuleSyntax",
];

/// The flags `strict` turns on; turning one off weakens `strict`.
pub(crate) const STRICT_FAMILY: [&str; 7] = [
    "noImplicitAny",
    "strictNullChecks",
    "strictFunctionTypes",
    "strictBindCallApply",
    "strictPropertyInitialization",
    "noImplicitThis",
    "alwaysStrict",
];

/// `module` and `moduleResolution` of a kind.
pub(crate) fn resolution(kind: Kind) -> (&'static str, &'static str) {
    if difference(kind, Row::Resolution).is_some() {
        ("ESNext", "Bundler")
    } else {
        ("NodeNext", "NodeNext")
    }
}

#[cfg(test)]
mod tests {
    use super::{DIFFERENCES, Row, SCRIPTS, SYNONYMS, difference};
    use crate::settings::Kind;

    /// The kinds stay alike: a difference names a kind, says what differs and why, and no kind has
    /// two cells in one row.
    #[test]
    fn every_difference_has_its_reason_and_one_cell_per_kind() {
        for difference in DIFFERENCES {
            assert_ne!(difference.kinds, &[] as &[Kind], "{:?}", difference.row);
            assert!(difference.reason.len() > 20, "{:?}", difference.row);
            assert!(difference.cell.len() > 5, "{:?}", difference.row);
        }
        for kind in Kind::ALL {
            for row in [Row::Private, Row::Published, Row::Dev, Row::Resolution] {
                let cells = DIFFERENCES
                    .iter()
                    .filter(|difference| difference.row == row && difference.kinds.contains(&kind))
                    .count();
                assert!(cells <= 1, "{kind:?} {row:?}");
            }
        }
        assert_eq!(
            Kind::ALL
                .iter()
                .filter(|kind| difference(**kind, Row::Dev).is_some())
                .count(),
            1
        );
    }

    #[test]
    fn a_synonym_names_a_standard_script_and_is_not_one() {
        for (synonym, standard) in SYNONYMS {
            assert!(
                SCRIPTS.contains(standard) || ["format", "format:fix"].contains(standard),
                "{synonym}"
            );
            assert!(!SCRIPTS.contains(synonym), "{synonym}");
        }
    }
}
