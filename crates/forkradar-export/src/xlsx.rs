//! Writer for real XLSX workbooks.
//!
//! The file is an OOXML package in a ZIP container. The writer produces one
//! worksheet with a bold header row and the values of a [`Table`]. Strings are
//! written as inline strings, so the file needs no shared string table.

use crate::table::{Cell, Table};
use crate::zip::ZipWriter;

/// Builds a named XML entity.
///
/// The name is passed in as text, because a literal entity in the source
/// would be read as a markup entity by some editors.
fn entity(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('&');
    out.push_str(name);
    out.push(';');
    out
}

/// Escapes the five characters that XML reserves.
fn escape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str(&entity("amp")),
            '<' => out.push_str(&entity("lt")),
            '>' => out.push_str(&entity("gt")),
            '"' => out.push_str(&entity("quot")),
            '\'' => out.push_str(&entity("apos")),
            _ => out.push(ch),
        }
    }
    out
}

/// Removes the characters that a worksheet name may not contain.
fn clean_sheet_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|ch| !matches!(ch, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "Blatt1".to_string()
    } else {
        trimmed.chars().take(31).collect()
    }
}

/// Converts a zero based column index into the spreadsheet notation.
fn column_name(mut index: usize) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name
}

/// Writes the table as an XLSX file.
pub fn to_xlsx(table: &Table) -> Vec<u8> {
    let sheet = clean_sheet_name(&table.sheet_name);
    let mut rows = String::new();

    rows.push_str("<row r=\"1\">");
    for (index, header) in table.headers.iter().enumerate() {
        let reference = format!("{}1", column_name(index));
        rows.push_str(&format!(
            "<c r=\"{}\" t=\"inlineStr\" s=\"1\"><is><t xml:space=\"preserve\">{}</t></is></c>",
            reference,
            escape_xml(header)
        ));
    }
    rows.push_str("</row>");

    for (row_index, row) in table.rows.iter().enumerate() {
        let number = row_index + 2;
        rows.push_str(&format!("<row r=\"{number}\">"));
        for (index, cell) in row.iter().enumerate() {
            let reference = format!("{}{}", column_name(index), number);
            match cell {
                Cell::Text(value) => rows.push_str(&format!(
                    "<c r=\"{}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    reference,
                    escape_xml(value)
                )),
                Cell::Integer(value) => {
                    rows.push_str(&format!("<c r=\"{}\"><v>{}</v></c>", reference, value))
                }
                Cell::Decimal(value) => {
                    let number = if value.is_finite() { *value } else { 0.0 };
                    rows.push_str(&format!(
                        "<c r=\"{}\"><v>{}</v></c>",
                        reference,
                        format_decimal(number)
                    ));
                }
            }
        }
        rows.push_str("</row>");
    }

    let content_types = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
        "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
        "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>",
        "<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
        "<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>",
        "</Types>"
    );

    let root_rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>",
        "</Relationships>"
    );

    let workbook = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" ",
            "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">",
            "<sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets>",
            "</workbook>"
        ),
        escape_xml(&sheet)
    );

    let workbook_rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/>",
        "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
        "</Relationships>"
    );

    let styles = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
        "<fonts count=\"2\">",
        "<font><sz val=\"11\"/><name val=\"Calibri\"/></font>",
        "<font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font>",
        "</fonts>",
        "<fills count=\"2\">",
        "<fill><patternFill patternType=\"none\"/></fill>",
        "<fill><patternFill patternType=\"gray125\"/></fill>",
        "</fills>",
        "<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>",
        "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>",
        "<cellXfs count=\"2\">",
        "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>",
        "<xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/>",
        "</cellXfs>",
        "</styleSheet>"
    );

    let worksheet = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
            "<sheetData>{}</sheetData>",
            "</worksheet>"
        ),
        rows
    );

    let mut writer = ZipWriter::new();
    writer.add("[Content_Types].xml", content_types.as_bytes());
    writer.add("_rels/.rels", root_rels.as_bytes());
    writer.add("xl/workbook.xml", workbook.as_bytes());
    writer.add("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes());
    writer.add("xl/styles.xml", styles.as_bytes());
    writer.add("xl/worksheets/sheet1.xml", worksheet.as_bytes());
    writer.finish()
}

/// Writes a decimal for the worksheet without trailing zeros.
fn format_decimal(value: f64) -> String {
    let text = format!("{value:.2}");
    let trimmed = text.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Table {
        let mut table = Table::new(
            "Funktionen",
            vec![
                "Titel".to_string(),
                "Stimmen".to_string(),
                "Score".to_string(),
            ],
        );
        table.push(vec![
            Cell::text("Schnellstart"),
            Cell::Integer(12),
            Cell::Decimal(87.5),
        ]);
        table.push(vec![
            Cell::text("Filter und Text"),
            Cell::Integer(0),
            Cell::Decimal(10.0),
        ]);
        table
    }

    #[test]
    fn the_workbook_is_a_zip_container() {
        let bytes = to_xlsx(&sample());
        assert_eq!(&bytes[0..4], &0x0403_4b50u32.to_le_bytes());
    }

    #[test]
    fn the_workbook_contains_the_required_ooxml_parts() {
        let bytes = to_xlsx(&sample());
        for part in [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/styles.xml",
            "xl/worksheets/sheet1.xml",
        ] {
            let needle = part.as_bytes();
            assert!(
                bytes.windows(needle.len()).any(|window| window == needle),
                "part {part} is missing"
            );
        }
    }

    #[test]
    fn the_worksheet_writes_text_and_numbers_in_separate_cells() {
        let bytes = to_xlsx(&sample());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("t=\"inlineStr\""));
        assert!(text.contains("<v>12</v>"));
        assert!(text.contains("<v>87.5</v>"));
    }

    #[test]
    fn special_characters_are_escaped() {
        let mut table = Table::new("Blatt", vec!["A".to_string()]);
        table.push(vec![Cell::text("a & b < c > d \" e ' f")]);
        let bytes = to_xlsx(&table);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(&entity("amp")));
        assert!(text.contains(&entity("lt")));
        assert!(text.contains(&entity("gt")));
        assert!(text.contains(&entity("quot")));
        assert!(text.contains(&entity("apos")));
        assert!(!text.contains("a & b < c"));
    }

    #[test]
    fn column_names_follow_the_spreadsheet_notation() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(51), "AZ");
        assert_eq!(column_name(52), "BA");
    }

    #[test]
    fn the_sheet_name_is_shortened_and_cleaned() {
        assert_eq!(clean_sheet_name("a/b:c*d?e[f]g"), "abcdefg");
        assert_eq!(clean_sheet_name("   "), "Blatt1");
        assert_eq!(clean_sheet_name(&"x".repeat(40)).len(), 31);
    }
}
