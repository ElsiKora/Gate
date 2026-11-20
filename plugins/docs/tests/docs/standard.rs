//! The standards look alike: every difference between kinds carries its reason, and the tables
//! hold exactly the differences the design decided.

use docs::standard::{Cell, ENTRIES, INDEXES, Kind, RESERVED, reserved};

#[test]
fn the_kinds_have_the_same_entries_in_the_same_order_but_the_two_absent_cells() {
    let mut absent = Vec::new();
    for entry in ENTRIES {
        for kind in Kind::ALL {
            if let Cell::Absent(reason) = entry.cell(kind) {
                assert!(
                    !reason.trim().is_empty(),
                    "{} in {}: a difference without a reason",
                    entry.name,
                    kind.name()
                );
                absent.push(format!("{} {}", kind.name(), entry.name));
            }
        }
    }
    assert_eq!(absent, ["monorepo-root modules", "monorepo-root ledger"]);
}

#[test]
fn only_a_library_requires_an_optional_entry_releases_and_says_why() {
    let mut required = Vec::new();
    for entry in ENTRIES {
        for kind in Kind::ALL {
            if let Cell::Required(reason) = entry.cell(kind) {
                assert!(
                    !reason.trim().is_empty(),
                    "{} in {}: a difference without a reason",
                    entry.name,
                    kind.name()
                );
                assert!(
                    entry.optional,
                    "{}: required where optional elsewhere",
                    entry.name
                );
                required.push(format!("{} {}", kind.name(), entry.name));
            }
        }
    }
    assert_eq!(required, ["library releases"]);
    let releases = ENTRIES
        .iter()
        .find(|entry| entry.name == "releases")
        .expect("releases");
    let requiring: Vec<&str> = Kind::ALL
        .iter()
        .filter(|kind| releases.required(**kind))
        .map(|kind| kind.name())
        .collect();
    assert_eq!(requiring, ["library"]);
}

#[test]
fn every_index_differs_between_kinds_only_where_a_reason_says_why() {
    for index in INDEXES {
        let titles: Vec<Vec<&str>> = Kind::ALL.iter().map(|kind| index.titles(*kind)).collect();
        // The sections most kinds share; when no two kinds agree, every kind differs.
        let shared = titles
            .iter()
            .max_by_key(|candidate| titles.iter().filter(|other| other == candidate).count())
            .filter(|candidate| titles.iter().filter(|other| other == candidate).count() > 1);
        for kind in Kind::ALL {
            let differs = shared != Some(&titles[kind.column()]);
            let reason = index.reasons[kind.column()];
            assert_eq!(
                differs,
                reason.is_some_and(|reason| !reason.trim().is_empty()),
                "{} in {}: a difference needs a reason, and a reason needs a difference",
                index.entry,
                kind.name()
            );
        }
    }
}

#[test]
fn system_shape_opens_every_architecture_index() {
    let architecture = INDEXES
        .iter()
        .find(|index| index.entry == "architecture")
        .expect("an architecture index");
    for kind in Kind::ALL {
        assert_eq!(
            architecture.titles(kind).first(),
            Some(&"System shape"),
            "{}",
            kind.name()
        );
    }
    assert_eq!(
        architecture.titles(Kind::Library),
        [
            "System shape",
            "Public API",
            "Versioning and release",
            "Compatibility"
        ]
    );
    assert_eq!(
        architecture.titles(Kind::Frontend),
        [
            "System shape",
            "Routing",
            "State",
            "Rendering",
            "API access"
        ]
    );
}

#[test]
fn every_index_belongs_to_an_entry_in_the_standard_order() {
    let order: Vec<&str> = ENTRIES.iter().map(|entry| entry.name).collect();
    let indexed: Vec<usize> = INDEXES
        .iter()
        .map(|index| {
            order
                .iter()
                .position(|name| *name == index.entry)
                .expect("an entry")
        })
        .collect();
    assert!(
        indexed.windows(2).all(|pair| pair[0] < pair[1]),
        "{indexed:?}"
    );
    for entry in ENTRIES {
        let has_index = INDEXES.iter().any(|index| index.entry == entry.name);
        assert_eq!(
            has_index,
            Kind::ALL.iter().any(|kind| entry.required(*kind)),
            "{}: every entry some kind requires has an index, and only those",
            entry.name
        );
    }
}

#[test]
fn no_entry_is_reserved_and_every_reserved_name_points_to_an_entry() {
    for entry in ENTRIES {
        assert!(reserved(entry.name).is_none(), "{} is reserved", entry.name);
    }
    for name in RESERVED {
        let target = name
            .goes_to
            .strip_prefix("docs/")
            .and_then(|rest| rest.split('/').next())
            .expect("a docs/ path");
        assert!(
            ENTRIES.iter().any(|entry| entry.name == target),
            "{} goes to {}, which is no entry",
            name.name,
            name.goes_to
        );
    }
    assert_eq!(
        reserved("concepts.md").map(|name| name.goes_to),
        Some("docs/architecture/")
    );
    assert_eq!(reserved("glossary").map(|name| name.goes_to), None);
}
