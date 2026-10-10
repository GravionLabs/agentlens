use agentlens_core::tokens::Tokens;
use anyhow::Result;
use comfy_table::{Cell, CellAlignment, Table, presets::UTF8_FULL_CONDENSED};

/// `1,234` for measured values, `~1,234` for estimated ones.
pub fn tokens(t: Tokens) -> String {
    format!("{}{}", if t.measured { "" } else { "~" }, number(t.count))
}

pub fn number(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A table with right-aligned columns from the `right_from` index on.
pub fn table(header: &[&str], rows: Vec<Vec<String>>, right_from: usize) -> Table {
    let mut t = Table::new();
    t.load_style(UTF8_FULL_CONDENSED);
    t.set_header(header);
    for row in rows {
        t.add_row(row.into_iter().enumerate().map(|(i, c)| {
            let cell = Cell::new(c);
            if i >= right_from {
                cell.set_alignment(CellAlignment::Right)
            } else {
                cell
            }
        }));
    }
    t
}

pub fn csv(header: &[&str], rows: Vec<Vec<String>>) -> Result<String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(header)?;
    for row in rows {
        w.write_record(row)?;
    }
    Ok(String::from_utf8(w.into_inner()?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_get_thousands_separators() {
        assert_eq!(number(0), "0");
        assert_eq!(number(999), "999");
        assert_eq!(number(1000), "1,000");
        assert_eq!(number(1234567), "1,234,567");
    }

    #[test]
    fn estimates_are_marked() {
        assert_eq!(tokens(Tokens::estimated(1500)), "~1,500");
        assert_eq!(tokens(Tokens::measured(1500)), "1,500");
    }
}
