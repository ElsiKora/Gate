//! Path options: every path an option names is relative to the project root and written in its
//! normalized form, so that rules can join and compare paths as plain strings. A path in another
//! form is a configuration error that shows the normalized form.

/// What a path option names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Form {
    /// A file: not empty, without a trailing `/`.
    File,
    /// A folder: without a trailing `/`; empty for the project root.
    Folder,
    /// A path prefix matched against paths as a string: a trailing `/` is kept when written.
    Prefix,
    /// One folder or file name, matched as a whole path segment: not empty, without `/`.
    Name,
}

/// The normalized form of `value`, or why it has none: no `.` segment, no empty segment (a
/// doubled or leading `/`), and no `..` segment that a parent does not absorb.
///
/// # Errors
/// When `value` leaves the project root, the error says so.
pub fn normalized(value: &str, form: Form) -> Result<String, String> {
    let mut segments: Vec<&str> = Vec::new();

    for segment in value.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err("leaves the project root".to_owned());
                }
            }
            name => segments.push(name),
        }
    }

    let joined = segments.join("/");

    match form {
        Form::File if joined.is_empty() => Err("names no file".to_owned()),
        Form::Name if joined.is_empty() || joined.contains('/') => {
            Err("is not one folder or file name".to_owned())
        }
        Form::Prefix if value.ends_with('/') && !joined.is_empty() => Ok(format!("{joined}/")),
        _ => Ok(joined),
    }
}

/// Checks that the path `value` of the option `option` is written in its normalized form.
///
/// # Errors
/// When it is not, the error names the option, the value and the normalized form to write.
pub fn check(option: &str, value: &str, form: Form) -> gate_sdk::Result<()> {
    match normalized(value, form) {
        Ok(normal) if normal == value => Ok(()),
        Ok(normal) if form == Form::Name => Err(gate_sdk::Error::new(format!(
            "option `{option}` is {value:?}, which is not a plain name; write {normal:?}"
        ))),
        Ok(normal) => Err(gate_sdk::Error::new(format!(
            "option `{option}` is {value:?}, which is not a normalized path relative to the project root; write {normal:?}"
        ))),
        Err(reason) if form == Form::Name => Err(gate_sdk::Error::new(format!(
            "option `{option}` is {value:?}, which {reason}; write a single name without `/`"
        ))),
        Err(reason) => Err(gate_sdk::Error::new(format!(
            "option `{option}` is {value:?}, which {reason}; write a path relative to the project root"
        ))),
    }
}

/// Checks every path of a list option, naming the entry by its index.
///
/// # Errors
/// At the first path that is not normalized.
pub fn check_all<'v>(
    option: &str,
    values: impl IntoIterator<Item = &'v String>,
    form: Form,
) -> gate_sdk::Result<()> {
    for (index, value) in values.into_iter().enumerate() {
        check(&format!("{option}[{index}]"), value, form)?;
    }

    Ok(())
}
