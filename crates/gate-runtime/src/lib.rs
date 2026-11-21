//! Runs Gate plugins: WebAssembly components under wasmtime, against the core's snapshot.
//!
//! A [`Runtime`] holds one engine (compiled-module cache, fuel limit) and the linker. A [`Plugin`] is
//! one loaded component and its declaration. [`Plugin::check`] runs a `check` call within a
//! [`Grant`], and returns the findings with the call's [`ReadSet`] and log. [`Runtime::check_all`]
//! runs many calls on a worker pool.
//!
//! The sandbox: a plugin reaches the snapshot only through `tree` (tracked, granted paths), git only
//! when every rule of the call declares `reads-history`, and the standard library's WASI imports
//! answer with fixed values (no environment, arguments, preopened directories or network, a constant
//! clock and randomness, stdout and stderr captured into the log).

mod error;
mod grant;
mod host;
mod read_set;
mod runtime;
mod state;

pub use self::error::{Error, ErrorKind};
pub use self::grant::{Grant, is_relative_glob};
pub use self::host::{Blob, History, HostError, Size, Tree};
pub use self::read_set::{GitQuery, ReadSet};
pub use self::runtime::{
    Call, CallOutcome, CallTime, Config, HostTime, INTERFACE, Invocation, LoadTime, Plugin, Runtime,
};

/// The result of a runtime operation.
pub type Result<T> = std::result::Result<T, Error>;
