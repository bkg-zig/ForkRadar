//! Tabular data and its CSV representation.
//!
//! The exporter works on a neutral table. Every view that offers an export
//! converts its rows into this table, so the CSV and the XLSX file always
//! contain the same values in the same order.

/// A single cell of an export table.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Free text.
    Text(String),
    /// A whole number.
    Integer(i64),
    /// A decimal number. The value is written with at most two decimals and
    /// without trailing zeros.
    Decimal(f64),
}

impl Cell {
    /// Builds a text cell.
    pub fn text(value: impl Into<String>) -> Self {
        Cell::Text(value.into())
    }

    /// Builds a text cell from an optional value. `None` becomes an empty
    /// cell, so a missing value never turns into the word "null".
    pub fn optional_text(value: Option<impl Into<String>>) -> Self {
        match value {
            Some(value) => Cell::Text(value.into()),
            None => Cell::Text(String::new()),
        }
    }

    /// Writes the cell as text.
    pub fn as_text(&self) -> String {
        match self {
            Cell::Text(value) => value.clone(),
            Cell::Integer(value) => value.to_string(),
            Cell::Decimal(value) => format_decimal(*value),
        }
    }

    /// Writes the cell as a number, when it holds one.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Cell::Text(_) => None,
            Cell::Integer(value) => Some(*value as f64),
            Cell::Decimal(value) => Some(*value),
        }
    }
}

/// Writes a decimal without trailing zeros.
fn format_decimal(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let text = format!("{value:.2}");
    let trimmed = text.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    if trimmed == "-0" {
        String::new()
    } else {
        trimmed.to_string()
    }
}

/// A table with a header row.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Name of the worksheet in the XLSX file.
    pub sheet_name: String,
    /// Titles of the columns.
    pub headers: Vec<String>,
    /// The rows below the header.
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    /// Builds an empty table.
    pub fn new(sheet_name: impl Into<String>, headers: Vec<String>) -> Self {
        Self {
            sheet_name: sheet_name.into(),
            headers,
            rows: Vec::new(),
        }
    }

    /// Adds a row. Missing cells are filled with empty text, additional cells
    /// are dropped, so a malformed row can never shift the columns.
    pub fn push(&mut self, row: Vec<Cell>) {
        let mut row = row;
        row.resize(self.headers.len(), Cell::Text(String::new()));
        row.truncate(self.headers.len());
        self.rows.push(row);
    }

    /// Number of rows without the header.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Reports whether the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Writes the table as CSV with a byte order mark, so that Excel and
    /// LibreOffice read the German umlauts as UTF-8.
    pub fn to_csv(&self) -> Vec<u8> {
        let mut out = vec![0xef, 0xbb, 0xbf];
        out.extend_from_slice(&self.to_csv_body());
        out
    }

    /// Writes the table as CSV without the byte order mark.
    pub fn to_csv_without_bom(&self) -> Vec<u8> {
        self.to_csv_body()
    }

    /// Builds the CSV body with the line endings of RFC 4180.
    fn to_csv_body(&self) -> Vec<u8> {
        let mut out = String::new();
        let header: Vec<String> = self.headers.iter().map(|h| escape_field(h)).collect();
        out.push_str(&header.join(","));
        out.push_str("\r\n");
        for row in &self.rows {
            let cells: Vec<String> = row
                .iter()
                .map(|cell| escape_field(&cell.as_text()))
                .collect();
            out.push_str(&cells.join(","));
            out.push_str("\r\n");
        }
        out.into_bytes()
    }
}

/// Escapes one CSV field according to RFC 4180.
fn escape_field(value: &str) -> String {
    let needs_quotes = value
        .chars()
        .any(|ch| ch == ',' || ch == '"' || ch == '\n' || ch == '\r');
    if !needs_quotes {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_uses_the_line_endings_of_rfc_4180() {
        let mut table = Table::new("Blatt", vec!["A".to_string(), "B".to_string()]);
        table.push(vec![Cell::Integer(1), Cell::text("x")]);
        let body = String::from_utf8(table.to_csv_without_bom()).unwrap();
        assert_eq!(body, "A,B\r\n1,x\r\n");
    }

    #[test]
    fn csv_quotes_fields_with_separators_quotes_and_line_breaks() {
        let mut table = Table::new("Blatt", vec!["A".to_string()]);
        table.push(vec![Cell::text("a,b")]);
        table.push(vec![Cell::text("say \"hi\"")]);
        table.push(vec![Cell::text("line\nbreak")]);
        let body = String::from_utf8(table.to_csv_without_bom()).unwrap();
        assert_eq!(
            body,
            "A\r\n\"a,b\"\r\n\"say \"\"hi\"\"\"\r\n\"line\nbreak\"\r\n"
        );
    }

    #[test]
    fn csv_starts_with_the_byte_order_mark() {
        let table = Table::new("Blatt", vec!["A".to_string()]);
        let bytes = table.to_csv();
        assert_eq!(&bytes[..3], &[0xef, 0xbb, 0xbf]);
    }

    #[test]
    fn decimals_lose_their_trailing_zeros() {
        assert_eq!(Cell::Decimal(87.40).as_text(), "87.4");
        assert_eq!(Cell::Decimal(87.0).as_text(), "87");
        assert_eq!(Cell::Decimal(87.456).as_text(), "87.46");
    }

    #[test]
    fn missing_cells_are_filled_and_extra_cells_are_dropped() {
        let mut table = Table::new("Blatt", vec!["A".to_string(), "B".to_string()]);
        table.push(vec![Cell::text("only")]);
        table.push(vec![Cell::text("a"), Cell::text("b"), Cell::text("c")]);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[1].len(), 2);
        assert_eq!(table.rows[0][1], Cell::Text(String::new()));
    }
}
