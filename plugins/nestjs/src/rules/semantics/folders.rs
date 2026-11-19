//! Folder options of the semantic rules that must name a folder below the project root: the root
//! itself (`""`) would make the option say nothing, or everything.

use crate::shared::paths::{self, Form};

/// Checks that the folder `value` of `option` is normalized ([`Form::Folder`]) and not empty.
///
/// # Errors
/// When the folder is not normalized, or is the project root.
pub fn check_below_root(option: &str, value: &str) -> gate_sdk::Result<()> {
    if value.is_empty() {
        return Err(gate_sdk::Error::new(format!(
            "option `{option}` is \"\", the project root; write a folder below it, such as \"src/shared\""
        )));
    }

    paths::check(option, value, Form::Folder)
}

/// Checks every folder of a list option with [`check_below_root`], naming the entry by its index.
///
/// # Errors
/// At the first folder that is not normalized or is the project root.
pub fn check_all_below_root(option: &str, values: &[String]) -> gate_sdk::Result<()> {
    for (index, value) in values.iter().enumerate() {
        check_below_root(&format!("{option}[{index}]"), value)?;
    }

    Ok(())
}
