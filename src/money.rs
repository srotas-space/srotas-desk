/// Parses a rupee amount typed by the user (e.g. "80", "80.5", "80.50")
/// into whole paise. Parsed digit-by-digit rather than via `f64::parse`, so
/// there's no floating-point rounding anywhere near money, even at the
/// text-input boundary.
pub fn rupees_to_paise(input: &str) -> Option<i64> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let mut parts = input.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let frac = parts.next().unwrap_or("");

    if frac.len() > 2 || (whole.is_empty() && frac.is_empty()) {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let whole: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let frac: i64 = format!("{frac:0<2}").parse().ok()?;

    Some(whole * 100 + frac)
}

/// Formats paise as a rupee string, e.g. `-15050` -> "-₹150.50".
pub fn format_paise(paise: i64) -> String {
    let sign = if paise < 0 { "-" } else { "" };
    let paise_abs = paise.unsigned_abs();
    format!("{sign}₹{}.{:02}", paise_abs / 100, paise_abs % 100)
}

/// Same as `format_paise` but without the currency symbol, for pre-filling
/// a text input that the user will go on to edit (e.g. the edit-item form).
pub fn paise_to_input(paise: i64) -> String {
    format!("{}.{:02}", paise / 100, paise % 100)
}

/// Same as `format_paise`, but with an ASCII "Rs." prefix instead of the ₹
/// glyph. The PDF report uses this — its builtin fonts use an encoding
/// that doesn't include the rupee sign, so ₹ would render as a missing
/// glyph there.
pub fn format_paise_ascii(paise: i64) -> String {
    let sign = if paise < 0 { "-" } else { "" };
    let paise_abs = paise.unsigned_abs();
    format!("{sign}Rs. {}.{:02}", paise_abs / 100, paise_abs % 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whole_and_fractional_rupees() {
        assert_eq!(rupees_to_paise("80"), Some(8000));
        assert_eq!(rupees_to_paise("80.5"), Some(8050));
        assert_eq!(rupees_to_paise("80.50"), Some(8050));
        assert_eq!(rupees_to_paise(".5"), Some(50));
        assert_eq!(rupees_to_paise(""), None);
        assert_eq!(rupees_to_paise("80.555"), None);
        assert_eq!(rupees_to_paise("abc"), None);
    }

    #[test]
    fn formats_paise_as_rupees() {
        assert_eq!(format_paise(8050), "₹80.50");
        assert_eq!(format_paise(8000), "₹80.00");
        assert_eq!(format_paise(-15050), "-₹150.50");
        assert_eq!(format_paise(0), "₹0.00");
    }
}
