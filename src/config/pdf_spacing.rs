//! Validation shared by native PDF spacing flags and persistent defaults.

/// Validate at input precision before narrowing to renderer coordinates.
pub(crate) fn bounded_number(text: &str, min: f64, max: f64) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| (min..=max).contains(value))
        .ok_or_else(|| format!("expected a finite number from {min} through {max}"))
}

pub(crate) fn parse_value(key: &str, text: &str) -> Result<f64, String> {
    let (min, max) = if key == "pdf_line_height" {
        (1.0, 4.0)
    } else {
        (0.0, 8.0)
    };
    bounded_number(text, min, max).map_err(|message| format!("{key}: {message}"))
}
