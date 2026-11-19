//! Managed blocks inside hand-written Markdown, and tables printed the way Prettier prints them.

/// Prettier's `printWidth` that the tables are laid out for.
pub(crate) const PRINT_WIDTH: usize = 100;

/// Replaces the lines between `<!-- gate:begin NAME -->` and `<!-- gate:end NAME -->`.
///
/// # Errors
/// The one-based line and the reason when the markers are missing, repeated or reversed.
pub(crate) fn replace(
    text: &str,
    name: &str,
    inner: &str,
) -> Result<String, (Option<usize>, String)> {
    let begin = format!("<!-- gate:begin {name} -->");
    let end = format!("<!-- gate:end {name} -->");
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let begins = positions(&lines, &begin);
    let ends = positions(&lines, &end);

    match (begins.as_slice(), ends.as_slice()) {
        ([start], [stop]) if start < stop => {
            // The block is written with the begin marker's line ending, so a CRLF file stays CRLF.
            let newline = if lines[*start].ends_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let mut output: String = lines[..=*start].concat();
            output.push_str(newline);
            output.push_str(&inner.replace('\n', newline));
            if !inner.is_empty() {
                output.push_str(newline);
            }
            output.push_str(&lines[*stop..].concat());
            Ok(output)
        }
        ([], []) => Err((
            None,
            format!("the managed block `{begin}` … `{end}` is missing"),
        )),
        _ => Err((
            begins.first().or(ends.first()).map(|index| index + 1),
            format!(
                "the managed block {name} needs exactly one `{begin}` line followed by one `{end}` line"
            ),
        )),
    }
}

fn positions(lines: &[&str], marker: &str) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_end_matches(['\n', '\r']) == marker)
        .map(|(index, _)| index)
        .collect()
}

/// A pipe table as Prettier prints Markdown tables with `proseWrap: "never"`: aligned while the
/// aligned width fits [`PRINT_WIDTH`], compact otherwise.
pub(crate) fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = header
        .iter()
        .map(|cell| cell.chars().count().max(3))
        .collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.chars().count());
            }
        }
    }

    let aligned_width = widths.iter().sum::<usize>() + 3 * widths.len().saturating_sub(1) + 4;
    let aligned = aligned_width <= PRINT_WIDTH;
    let render = |cells: &[String]| -> String {
        let padded: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let width = if aligned {
                    widths.get(index).copied().unwrap_or(0)
                } else {
                    0
                };
                format!(
                    "{cell}{}",
                    " ".repeat(width.saturating_sub(cell.chars().count()))
                )
            })
            .collect();
        format!("| {} |\n", padded.join(" | "))
    };

    let header: Vec<String> = header.iter().map(|cell| (*cell).to_owned()).collect();
    let rule: Vec<String> = widths
        .iter()
        .map(|width| "-".repeat(if aligned { *width } else { 3 }))
        .collect();
    let mut output = render(&header);
    output.push_str(&render(&rule));
    for row in rows {
        output.push_str(&render(row));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{PRINT_WIDTH, replace, table};

    #[test]
    fn a_table_stays_aligned_up_to_the_print_width_and_compacts_beyond() {
        let fits = "x".repeat(PRINT_WIDTH - 10);
        let aligned = table(
            &["A", "B"],
            &[
                vec!["a".to_owned(), "b".to_owned()],
                vec![fits.clone(), "c".to_owned()],
            ],
        );
        let compact = table(
            &["A", "B"],
            &[
                vec!["a".to_owned(), "b".to_owned()],
                vec![format!("{fits}x"), "c".to_owned()],
            ],
        );
        assert!(
            aligned
                .lines()
                .all(|line| line.chars().count() == PRINT_WIDTH)
        );
        assert_eq!(
            compact.lines().take(3).collect::<Vec<_>>(),
            ["| A | B |", "| --- | --- |", "| a | b |"]
        );
    }

    #[test]
    fn only_the_lines_between_one_pair_of_markers_are_replaced() {
        let text = "before\n<!-- gate:begin x -->\nold\n<!-- gate:end x -->\nafter\n";
        assert_eq!(
            replace(text, "x", "new\n"),
            Ok("before\n<!-- gate:begin x -->\n\nnew\n\n<!-- gate:end x -->\nafter\n".to_owned())
        );
        assert_eq!(
            replace(
                "before\r\n<!-- gate:begin x -->\r\nold\r\n<!-- gate:end x -->\r\nafter\r\n",
                "x",
                "a\nb\n"
            ),
            Ok(
                "before\r\n<!-- gate:begin x -->\r\n\r\na\r\nb\r\n\r\n<!-- gate:end x -->\r\nafter\r\n"
                    .to_owned()
            ),
            "a CRLF file keeps CRLF"
        );
        assert_eq!(
            replace(text, "x", ""),
            Ok("before\n<!-- gate:begin x -->\n\n<!-- gate:end x -->\nafter\n".to_owned())
        );
        assert_eq!(
            replace("text\n", "x", "").map_err(|error| error.0),
            Err(None)
        );
        assert_eq!(
            replace("<!-- gate:end x -->\n<!-- gate:begin x -->\n", "x", "")
                .map_err(|error| error.0),
            Err(Some(2))
        );
        assert_eq!(
            replace(
                "<!-- gate:begin x -->\n<!-- gate:begin x -->\n<!-- gate:end x -->\n",
                "x",
                ""
            )
            .map_err(|error| error.0),
            Err(Some(1))
        );
    }
}
