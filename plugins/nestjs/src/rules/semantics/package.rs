//! Package options: an absent package option means the layout block's package.

/// The package an optional package option names, or the layout's `fallback` when it is absent.
///
/// # Errors
/// When the option is present but empty.
pub(crate) fn resolve(
    option: &str,
    value: Option<&str>,
    fallback: &str,
) -> gate_sdk::Result<String> {
    match value {
        None => Ok(fallback.to_owned()),
        Some("") => Err(gate_sdk::Error::new(format!(
            "the option `{option}` is empty; name a package, or leave the option out to use the layout's package ({fallback})"
        ))),
        Some(package) => Ok(package.to_owned()),
    }
}
