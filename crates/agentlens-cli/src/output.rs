use agentlens_core::tokens::Tokens;
use anyhow::Result;
use comfy_table::{Cell, CellAlignment, Table, presets::UTF8_FULL_CONDENSED};

use crate::args::Format;

/// How a command prints: the output format and whether token counts are rounded (`--rounded`).
/// Only text tables are affected; JSON and CSV always carry the exact values.
#[derive(Clone, Copy, Debug)]
pub struct Display {
    pub format: Format,
    pub rounded: bool,
}

impl Display {
    /// A token figure: `1,234` or `1.23 k` when rounded; `~` marks an estimate.
    pub fn tokens(self, t: Tokens) -> String {
        tokens(t, self.rounded)
    }

    /// A plain token count that is always measured (no `~`).
    pub fn count(self, n: u64) -> String {
        token_count(n, self.rounded)
    }
}

/// `1,234` for measured values, `~1,234` for estimated ones; rounded as `1.23 k` on request.
fn tokens(t: Tokens, rounded: bool) -> String {
    format!(
        "{}{}",
        if t.measured { "" } else { "~" },
        token_count(t.count, rounded)
    )
}

fn token_count(n: u64, rounded: bool) -> String {
    if rounded {
        rounded_number(i128::from(n))
    } else {
        number(n)
    }
}

const UNITS: [(f64, &str); 3] = [(1e3, "k"), (1e6, "M"), (1e9, "B")];

/// `2.11 k`, `1.55 M`, `3.20 B`; below 1,000 the number as it is. Same rule as `rounded()` in
/// `src/app/core/format.ts` (two decimals, `999,999` is `1.00 M`).
pub fn rounded_number(n: i128) -> String {
    let abs = n.unsigned_abs();
    if abs < 1000 {
        return n.to_string();
    }
    let mut i = UNITS.len() - 1;
    while i > 0 && (abs as f64) < UNITS[i].0 {
        i -= 1;
    }
    let mut value = fixed2(abs as f64 / UNITS[i].0);
    if value.parse::<f64>().is_ok_and(|v| v >= 1000.0) && i < UNITS.len() - 1 {
        i += 1;
        value = fixed2(abs as f64 / UNITS[i].0);
    }
    format!("{}{} {}", if n < 0 { "-" } else { "" }, value, UNITS[i].1)
}

/// Two decimals, exact ties rounded up like JavaScript's `toFixed` (Rust rounds them to even), so
/// the CLI and the application show the same text. Only ties that are exact in binary (multiples of
/// 1/8, e.g. `1.125`) matter; other values are not ties and both languages round them alike.
fn fixed2(v: f64) -> String {
    let eighths = v * 8.0;
    let tie = eighths.fract() == 0.0 && (eighths as u64) % 2 == 1;
    format!("{:.2}", if tie { v + 0.001 } else { v })
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
    Ok(agentlens_core::export::csv::write(header, rows)?)
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
        assert_eq!(tokens(Tokens::estimated(1500), false), "~1,500");
        assert_eq!(tokens(Tokens::measured(1500), false), "1,500");
    }

    #[test]
    fn rounded_boundaries() {
        assert_eq!(rounded_number(0), "0");
        assert_eq!(rounded_number(999), "999");
        assert_eq!(rounded_number(1000), "1.00 k");
        assert_eq!(rounded_number(2110), "2.11 k");
        assert_eq!(rounded_number(999_994), "999.99 k");
        assert_eq!(rounded_number(999_999), "1.00 M");
        assert_eq!(rounded_number(1_000_000), "1.00 M");
        assert_eq!(rounded_number(1_550_000), "1.55 M");
        assert_eq!(rounded_number(999_999_999), "1.00 B");
        assert_eq!(rounded_number(3_200_000_000), "3.20 B");
        assert_eq!(rounded_number(2_000_000_000_000), "2000.00 B");
    }

    #[test]
    fn rounded_ties_round_up_like_the_application() {
        assert_eq!(rounded_number(1125), "1.13 k");
        assert_eq!(rounded_number(1005), "1.00 k");
    }

    #[test]
    fn rounded_negative_numbers_keep_the_sign() {
        assert_eq!(rounded_number(-5), "-5");
        assert_eq!(rounded_number(-1_550_000), "-1.55 M");
    }

    #[test]
    fn rounded_tokens_keep_the_estimate_mark() {
        assert_eq!(tokens(Tokens::estimated(1_550_000), true), "~1.55 M");
        assert_eq!(tokens(Tokens::measured(1_550_000), true), "1.55 M");
        assert_eq!(tokens(Tokens::measured(1_550_000), false), "1,550,000");
        assert_eq!(token_count(42, true), "42");
    }
}
