//! Workspace packages built as `wasm32-wasip2` components for the tests, once per test binary for
//! each package and feature set. Tests run in parallel; a second `cargo build` into the same target
//! could replace a `.wasm` while another test's run reads it, so no call path builds twice.
//!
//! Each build appends a line `<test binary> <package> <features>` to `component-builds.log` in
//! `CARGO_TARGET_TMPDIR`, so a run can show how many builds each binary made.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock, PoisonError};

/// A built component: its path and its bytes.
pub type Built = (PathBuf, Vec<u8>);

/// The component of the workspace package `package`, whose library is `library`, built with the
/// cargo `features` given (none when empty) in a target directory of its own, the first time it
/// is asked for; every later call answers the same build.
pub fn component(package: &str, library: &str, features: &str) -> &'static Built {
    type Cells = HashMap<(String, String), &'static OnceLock<Built>>;
    static CELLS: OnceLock<Mutex<Cells>> = OnceLock::new();
    let cell = *CELLS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry((package.to_owned(), features.to_owned()))
        .or_insert_with(|| &*Box::leak(Box::new(OnceLock::new())));
    cell.get_or_init(|| build(package, library, features))
}

fn build(package: &str, library: &str, features: &str) -> Built {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let target = tmp.join(format!("{package}{features}"));
    let mut arguments = vec![
        "build",
        "--locked",
        "--release",
        "-p",
        package,
        "--target",
        "wasm32-wasip2",
    ];
    if !features.is_empty() {
        arguments.extend(["--features", features]);
    }
    let status = Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(&arguments)
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .expect("cargo runs");
    assert!(
        status.success(),
        "{package} {features} builds for wasm32-wasip2"
    );
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(tmp.join("component-builds.log"))
        .expect("the build log opens");
    writeln!(log, "{} {package} {features}", env!("CARGO_CRATE_NAME"))
        .expect("the build log takes a line");
    let path = target.join(format!("wasm32-wasip2/release/{library}.wasm"));
    let bytes = std::fs::read(&path).expect("the component exists");
    (path, bytes)
}
