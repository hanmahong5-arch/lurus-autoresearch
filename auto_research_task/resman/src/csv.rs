/// Parse CSV text into rows of fields. RFC-4180 subset: comma delimiter,
/// double-quoted fields, "" escapes a literal quote inside a quoted field,
/// quoted fields may contain commas and newlines. Handles \n and \r\n.
/// Returns every row including the header; skips a final empty line.
pub(crate) fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes => {
                // Check for escaped quote: ""
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            }
            '"' => {
                in_quotes = true;
            }
            ',' if !in_quotes => {
                current_row.push(field.clone());
                field.clear();
            }
            '\r' if !in_quotes => {
                // Consume the following \n if present
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                current_row.push(field.clone());
                field.clear();
                rows.push(current_row.clone());
                current_row.clear();
            }
            '\n' if !in_quotes => {
                current_row.push(field.clone());
                field.clear();
                rows.push(current_row.clone());
                current_row.clear();
            }
            _ => {
                field.push(ch);
            }
        }
    }

    // Handle final field/row if text doesn't end with newline
    if !field.is_empty() || !current_row.is_empty() {
        current_row.push(field);
        rows.push(current_row);
    }

    // Skip trailing empty row (single empty field)
    if let Some(last) = rows.last() {
        if last.len() == 1 && last[0].is_empty() {
            rows.pop();
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_2x2_grid() {
        let rows = parse_csv("a,b\n1,2");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["a", "b"]);
        assert_eq!(rows[1], vec!["1", "2"]);
    }

    #[test]
    fn embedded_comma_in_quotes() {
        let rows = parse_csv("a,\"b,c\"\n1,2");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][1], "b,c");
    }

    #[test]
    fn escaped_quote_double() {
        let rows = parse_csv("\"he said \"\"hello\"\"\",b\n1,2");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "he said \"hello\"");
    }

    #[test]
    fn embedded_newline_inside_quotes() {
        let rows = parse_csv("\"line1\nline2\",b\n1,2");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0], "line1\nline2");
        assert_eq!(rows[0][1], "b");
        assert_eq!(rows[1], vec!["1", "2"]);
    }

    #[test]
    fn crlf_line_endings() {
        let rows = parse_csv("a,b\r\n1,2\r\n");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["a", "b"]);
        assert_eq!(rows[1], vec!["1", "2"]);
    }

    #[test]
    fn trailing_newline_no_empty_row() {
        let rows = parse_csv("a,b\n1,2\n");
        assert_eq!(rows.len(), 2, "trailing newline must not add empty row");
    }
}
