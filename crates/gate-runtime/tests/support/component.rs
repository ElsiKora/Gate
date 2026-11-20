//! Workspace packages built as `wasm32-wasip2` components for the tests, once per process for each
//! package and feature set. Tests run in parallel; a second `cargo build` into the same target could
//! replace a `.wasm` while another test's run reads it, so no call path in a process builds twice.
//! `cargo test` runs a test binary as one process, so that is once per binary. A runner that starts
//! one process per test (nextest) builds once per test instead, those builds in parallel into the
//! same target.
//!
//! Each build appends one line `<test binary> <package>[ <features>]` to `component-builds.log` in
//! `CARGO_TARGET_TMPDIR`, in one write, so a run can show how many builds each binary made.

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
    // A package name has no `+`, so the first `+` ends it and no two (package, features) pairs
    // share a directory.
    let target = if features.is_empty() {
        tmp.join(package)
    } else {
        tmp.join(format!("{package}+{features}"))
    };
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
    let line = if features.is_empty() {
        format!("{} {package}\n", env!("CARGO_CRATE_NAME"))
    } else {
        format!("{} {package} {features}\n", env!("CARGO_CRATE_NAME"))
    };
    log.write_all(line.as_bytes())
        .expect("the build log takes a line");
    let path = target.join(format!("wasm32-wasip2/release/{library}.wasm"));
    let bytes = std::fs::read(&path).expect("the component exists");
    (path, bytes)
}
