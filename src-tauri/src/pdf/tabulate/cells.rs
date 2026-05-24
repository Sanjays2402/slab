//! Cell typing — decide whether a string is a number, a date, or text.
//!
//! This is what makes the output feel like a real spreadsheet instead of a
//! dump of strings: Excel-formula-ready numbers, date-formatted cells, and
//! preserved free text.

use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Blank,
    Number(f64),
    /// Excel-serial date (days since 1899-12-30 to handle the 1900 leap-year bug).
    Date(f64),
    Text(String),
}

/// Excel's epoch is 1899-12-30 (1900 leap-year quirk). The offset from
/// the Unix epoch (1970-01-01) is 25569 days.
const EXCEL_EPOCH_OFFSET_DAYS: i64 = 25_569;

/// Classify a trimmed cell string into a typed `Cell`.
pub fn classify(s: &str) -> Cell {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Cell::Blank;
    }
    if let Some(n) = parse_number(trimmed) {
        return Cell::Number(n);
    }
    if let Some(serial) = parse_date(trimmed) {
        return Cell::Date(serial);
    }
    Cell::Text(trimmed.to_string())
}

fn parse_number(s: &str) -> Option<f64> {
    // Accounting parens: "(1,200)" → -1200.
    let mut t = s.to_string();
    let negate_parens = t.starts_with('(') && t.ends_with(')') && t.len() >= 2;
    if negate_parens {
        t = t[1..t.len() - 1].to_string();
    }
    // Strip currency symbols and stray spaces / non-breaking spaces.
    for sym in ['$', '€', '£', '¥', '₹', ' ', '\u{00A0}'] {
        t = t.replace(sym, "");
    }
    let percent = t.ends_with('%');
    if percent {
        t.pop();
    }
    // Try US format ("1,234.50") and EU format ("1.234,50"). When the string
    // contains BOTH '.' and ',', use the position of the last separator to
    // pick which one is the decimal — whichever appears last wins.
    let has_dot = t.contains('.');
    let has_comma = t.contains(',');
    let candidates: Vec<String> = if has_dot && has_comma {
        let last_dot = t.rfind('.').unwrap_or(0);
        let last_comma = t.rfind(',').unwrap_or(0);
        if last_comma > last_dot {
            // EU: "1.234,50" → "1234.50"
            vec![t.replace('.', "").replace(',', ".")]
        } else {
            // US: "1,234.50" → "1234.50"
            vec![t.replace(',', "")]
        }
    } else if has_comma {
        // Ambiguous single-comma: try both US (thousands) and EU (decimal).
        vec![t.replace(',', ""), t.replace(',', ".")]
    } else {
        vec![t.clone()]
    };
    for c in candidates {
        if c.is_empty() {
            continue;
        }
        if let Ok(mut n) = c.parse::<f64>() {
            if !n.is_finite() {
                continue;
            }
            if negate_parens {
                n = -n;
            }
            if percent {
                n /= 100.0;
            }
            return Some(n);
        }
    }
    None
}

fn parse_date(s: &str) -> Option<f64> {
    const FORMATS: &[&str] = &[
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%m/%d/%Y",
        "%m-%d-%Y",
        "%d/%m/%Y",
        "%d-%m-%Y",
        "%d %b %Y",
        "%d %B %Y",
        "%b %d, %Y",
        "%B %d, %Y",
    ];
    for fmt in FORMATS {
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(to_excel_serial(d));
        }
    }
    None
}

fn to_excel_serial(d: NaiveDate) -> f64 {
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let delta = d.signed_duration_since(epoch).num_days();
    (delta + EXCEL_EPOCH_OFFSET_DAYS) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_numeric() {
        assert!(matches!(classify("42"), Cell::Number(n) if (n - 42.0).abs() < 1e-9));
        assert!(matches!(classify("-17"), Cell::Number(n) if (n + 17.0).abs() < 1e-9));
    }

    #[test]
    fn currency_and_thousands_strip() {
        assert!(matches!(classify("$1,234.50"), Cell::Number(n) if (n - 1234.5).abs() < 1e-6));
        assert!(matches!(classify("(1,200)"), Cell::Number(n) if (n + 1200.0).abs() < 1e-6));
    }

    #[test]
    fn eu_format_thousands_dot_decimal_comma() {
        assert!(matches!(classify("€2.500,75"), Cell::Number(n) if (n - 2500.75).abs() < 1e-6));
    }

    #[test]
    fn percentages_are_numeric() {
        assert!(matches!(classify("12.5%"), Cell::Number(n) if (n - 0.125).abs() < 1e-6));
        assert!(matches!(classify("100%"), Cell::Number(n) if (n - 1.0).abs() < 1e-6));
    }

    #[test]
    fn iso_dates_are_dates() {
        match classify("2026-05-24") {
            Cell::Date(serial) => assert!((40000.0..60000.0).contains(&serial)),
            other => panic!("expected Cell::Date, got {:?}", other),
        }
    }

    #[test]
    fn us_dates_are_dates() {
        assert!(matches!(classify("5/24/2026"), Cell::Date(_)));
        assert!(matches!(classify("05/24/2026"), Cell::Date(_)));
    }

    #[test]
    fn long_month_dates_are_dates() {
        assert!(matches!(classify("May 24, 2026"), Cell::Date(_)));
        assert!(matches!(classify("24 May 2026"), Cell::Date(_)));
    }

    #[test]
    fn empty_and_whitespace_are_blank() {
        assert!(matches!(classify(""), Cell::Blank));
        assert!(matches!(classify("   "), Cell::Blank));
        assert!(matches!(classify("\t\n"), Cell::Blank));
    }

    #[test]
    fn arbitrary_text_is_text() {
        assert!(matches!(classify("Revenue Q1"), Cell::Text(ref s) if s == "Revenue Q1"));
        assert!(matches!(classify("N/A"), Cell::Text(ref s) if s == "N/A"));
    }

    #[test]
    fn excel_serial_for_known_date() {
        // 1970-01-01 → 25569 in Excel's 1900-system.
        match classify("1970-01-01") {
            Cell::Date(serial) => assert!((serial - 25569.0).abs() < 1e-9),
            other => panic!("expected Cell::Date, got {:?}", other),
        }
    }
}
