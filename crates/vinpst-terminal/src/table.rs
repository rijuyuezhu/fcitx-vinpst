use unicode_width::UnicodeWidthStr;

const COLUMN_GAP: usize = 2;

/// Print a table whose columns are aligned by terminal display width.
pub fn print_table<const N: usize>(
    headers: [&str; N],
    rows: impl IntoIterator<Item = [String; N]>,
) {
    print!("{}", render_table(headers, rows));
}

/// Print rows without a header, aligned by terminal display width.
pub fn print_rows<const N: usize>(rows: impl IntoIterator<Item = [String; N]>) {
    print!("{}", render_rows(rows));
}

fn render_table<const N: usize>(
    headers: [&str; N],
    rows: impl IntoIterator<Item = [String; N]>,
) -> String {
    let rows = rows.into_iter().collect::<Vec<_>>();
    let mut widths = headers.map(UnicodeWidthStr::width);
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let mut output = String::new();
    append_row(&mut output, headers.into_iter(), &widths);
    for row in &rows {
        append_row(&mut output, row.iter().map(String::as_str), &widths);
    }
    output
}

fn render_rows<const N: usize>(rows: impl IntoIterator<Item = [String; N]>) -> String {
    let rows = rows.into_iter().collect::<Vec<_>>();
    let mut widths = [0; N];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let mut output = String::new();
    for row in &rows {
        append_row(&mut output, row.iter().map(String::as_str), &widths);
    }
    output
}

fn append_row<'a, const N: usize>(
    output: &mut String,
    cells: impl Iterator<Item = &'a str>,
    widths: &[usize; N],
) {
    for (index, cell) in cells.enumerate() {
        output.push_str(cell);
        if index + 1 < N {
            let padding = widths[index] - UnicodeWidthStr::width(cell) + COLUMN_GAP;
            output.extend(std::iter::repeat_n(' ', padding));
        }
    }
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::render_table;
    use unicode_width::UnicodeWidthStr;

    fn display_prefix_width(line: &str, cell: &str) -> usize {
        let byte_offset = line
            .find(cell)
            .expect("cell should be present in table row");
        UnicodeWidthStr::width(&line[..byte_offset])
    }

    #[test]
    fn aligns_columns_by_terminal_display_width() {
        let output = render_table(
            ["ID", "TITLE", "STATUS"],
            [
                [
                    "ascii".to_owned(),
                    "MTranServer 代理".to_owned(),
                    "installed".to_owned(),
                ],
                ["中文".to_owned(), "短".to_owned(), "available".to_owned()],
            ],
        );
        let lines = output.lines().collect::<Vec<_>>();

        assert_eq!(display_prefix_width(lines[0], "TITLE"), 7);
        assert_eq!(display_prefix_width(lines[1], "MTranServer 代理"), 7);
        assert_eq!(display_prefix_width(lines[2], "短"), 7);
        assert_eq!(display_prefix_width(lines[0], "STATUS"), 25);
        assert_eq!(display_prefix_width(lines[1], "installed"), 25);
        assert_eq!(display_prefix_width(lines[2], "available"), 25);
        assert!(!output.contains('\t'));
    }
}
