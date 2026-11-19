//! Lines in the text of a JSON options file, found without a JSON parser that keeps positions:
//! the line of each element of an array and the line of the first occurrence of a text.

/// The one-based line of each element of the array that the first key `key` with an array value
/// holds, in order. Elements are strings, objects, arrays or the first character of a scalar after
/// the opening bracket or a comma; an empty list when there is no such key.
#[must_use]
pub fn element_lines(text: &str, key: &str) -> Vec<u32> {
    let bytes = text.as_bytes();
    let needle = format!("\"{key}\"");
    let mut lines = Vec::new();
    let mut index = 0;
    let mut line = 1_u32;
    let mut array: Option<usize> = None;
    let mut depth = 0_usize;
    let mut expect_element = false;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == b'\n' {
            line += 1;
        }

        if byte == b'"' {
            let start = index;
            index += 1;

            while index < bytes.len() && bytes[index] != b'"' {
                if bytes[index] == b'\\' {
                    index += 1;
                }
                index += 1;
            }

            if array.is_none() && text.get(start..=index) == Some(needle.as_str()) {
                let rest = text.get(index + 1..).unwrap_or_default().trim_start();

                if rest
                    .strip_prefix(':')
                    .is_some_and(|value| value.trim_start().starts_with('['))
                {
                    array = Some(depth);
                }
            } else if array.is_some_and(|base| depth == base + 1) && expect_element {
                lines.push(line);
                expect_element = false;
            }

            index += 1;
            continue;
        }

        match byte {
            b'[' | b'{' => {
                if array.is_some_and(|base| depth == base + 1) && expect_element {
                    lines.push(line);
                    expect_element = false;
                }

                depth += 1;

                if byte == b'[' && array.is_some_and(|base| depth == base + 1) {
                    expect_element = true;
                }
            }
            b']' | b'}' => {
                depth = depth.saturating_sub(1);

                if array.is_some_and(|base| depth == base) {
                    return lines;
                }
            }
            b',' if array.is_some_and(|base| depth == base + 1) => expect_element = true,
            _ => {}
        }

        index += 1;
    }

    lines
}

/// The one-based line of the first occurrence of `needle` in `text`.
#[must_use]
pub fn text_line(text: &str, needle: &str) -> Option<u32> {
    let offset = text.find(needle)?;

    u32::try_from(text[..offset].matches('\n').count() + 1).ok()
}
