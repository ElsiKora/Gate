//! `hello/marker` on the fixture cases under `tests/fixtures`, through `gate-test`. Run with
//! `GATE_BLESS=1` to rewrite each case's `expected.txt` from what the rule reports.

#[test]
fn marker() {
    gate_test::check_fixtures(
        &gate_plugin_hello::PLUGIN,
        "marker",
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"),
    )
    .unwrap_or_else(|failure| panic!("{failure}"));
}
