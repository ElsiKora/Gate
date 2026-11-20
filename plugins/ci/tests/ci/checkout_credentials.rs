use crate::fixture::{Fixture, aspects, lines};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn workflow() -> String {
    format!(
        "\
jobs:
  build:
    steps:
      - uses: actions/checkout@{SHA} # v4.2.2
        with:
          persist-credentials: false
      - uses: actions/checkout@{SHA} # v4.2.2
      - uses: actions/checkout@{SHA} # v4.2.2
        with:
          persist-credentials: true
      - uses: example/checkout@{SHA} # v1.0.0
"
    )
}

#[test]
fn a_checkout_that_keeps_its_credentials_is_reported() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", &workflow())]);
    let findings = fixture.check("checkout-credentials", "{}").expect("runs");
    assert_eq!(aspects(&findings), ["- 7 build:2", "- 8 build:3"]);
    assert_eq!(
        lines(&findings)[0],
        ".github/workflows/ci.yml:7 job `build` checks out with its credentials kept in the clone"
    );
    assert_eq!(
        findings[0].target.as_deref(),
        Some(".github/workflows/ci.yml:build")
    );
}

#[test]
fn the_actions_option_names_the_checkout_actions() {
    let fixture = Fixture::new(&[(".github/workflows/ci.yml", &workflow())]);
    let findings = fixture
        .check(
            "checkout-credentials",
            r#"{"actions": ["example/checkout"]}"#,
        )
        .expect("runs");
    assert_eq!(aspects(&findings), ["- 11 build:4"]);
}
