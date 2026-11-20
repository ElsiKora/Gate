//! `repository/large-files`: sizes only, `large` and `huge` apart, `extra` declared with a job.

use crate::{Fixture, rows};

const RULE: &str = "large-files";
const LIMITS: &str = r#"{"large-bytes": 10, "huge-bytes": 20}"#;

#[test]
fn a_file_over_large_bytes_is_large_and_over_huge_bytes_only_huge() {
    let fixture = Fixture::new(&[
        ("at-large.bin", &[1u8; 10][..]),
        ("over-large.bin", &[1u8; 11][..]),
        ("at-huge.bin", &[1u8; 20][..]),
        ("over-huge.bin", &[1u8; 21][..]),
    ]);
    let findings = fixture.check(RULE, LIMITS).expect("runs");
    assert_eq!(
        rows(&findings),
        [
            "huge over-huge.bin:0 size",
            "large at-huge.bin:0 size",
            "large over-large.bin:0 size"
        ]
    );
    assert_eq!(fixture.reads(), 0, "the rule reads sizes only");
}

#[test]
fn a_declared_file_passes_and_a_declared_path_that_is_not_tracked_is_extra_missing() {
    let fixture = Fixture::new(&[("brand/logo.png", &[1u8; 30][..])]);
    let options = r#"{"large-bytes": 10, "huge-bytes": 20, "extra": [
        {"path": "brand/logo.png", "job": "the logo the site serves"},
        {"path": "brand/gone.png", "job": "an old logo"}
    ]}"#;
    let findings = fixture.check(RULE, options).expect("runs");
    assert_eq!(rows(&findings), ["extra-missing :0 brand/gone.png"]);

    let error = fixture
        .check(RULE, r#"{"extra": [{"path": "brand/logo.png"}]}"#)
        .expect_err("an entry without a job");
    assert!(error.message().contains("missing field `job`"), "{error}");
    let error = fixture
        .check(RULE, r#"{"large-bytes": 30, "huge-bytes": 20}"#)
        .expect_err("huge below large");
    assert!(
        error
            .message()
            .contains("huge-bytes (20) is below large-bytes (30)"),
        "{error}"
    );
}

#[test]
fn the_defaults_are_one_and_five_mebibytes() {
    let mebibyte = 1_048_576;
    let fixture = Fixture::new(&[
        ("one.bin", &vec![0u8; mebibyte][..]),
        ("two.bin", &vec![0u8; mebibyte + 1][..]),
        ("six.bin", &vec![0u8; 5 * mebibyte + 1][..]),
    ]);
    let findings = fixture.check(RULE, "{}").expect("runs");
    assert_eq!(
        rows(&findings),
        ["huge six.bin:0 size", "large two.bin:0 size"]
    );
}
