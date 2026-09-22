//! Target-aware file configuration over the same options as the text tools.
//!
//! A paired export consumes the union, not the intersection, of its targets'
//! fields. Other exports reject fields with no consumer before reading a file.

use super::{
    FILE_FIELDS, Field, HTML_FIELDS, JsonValue, PDF_FIELDS, ToolError, boolean,
    html_options, invalid_options, pdf_options, string, theme, validate_arguments,
};
use crate::{HtmlOptions, PdfASettings, PdfOptions, SvgOptions};
use std::collections::BTreeMap;

const SVG_WIDTH: Field = (
    "maxWidthPt", "number", "SVG poster width in points (144..14400); SVG only",
);

/// Both discovery and dispatch use this deduplicated union. Target-specific
/// descriptions are derived from the very same applicability predicate.
pub(super) fn fields() -> Vec<Field> {
    let mut fields = BTreeMap::new();
    for &field in FILE_FIELDS.iter().chain(HTML_FIELDS).chain(PDF_FIELDS) {
        if field.0 != "markdown" {
            fields.entry(field.0).or_insert(field);
        }
    }
    fields.insert(SVG_WIDTH.0, SVG_WIDTH);
    fields.into_values().collect()
}

pub(super) fn supported_targets(name: &str) -> String {
    ["html", "pdf", "both", "epub", "svg"].into_iter()
        .filter(|target| supports(target, name)).collect::<Vec<_>>().join(", ")
}

fn supports(target: &str, name: &str) -> bool {
    if FILE_FIELDS.iter().any(|field| field.0 == name) {
        return true;
    }
    if name == "markdown" {
        return false;
    }
    let html = HTML_FIELDS.iter().any(|field| field.0 == name);
    let pdf = PDF_FIELDS.iter().any(|field| field.0 == name);
    match target {
        "html" => html,
        "pdf" => pdf,
        "both" => html || pdf,
        // EPUB is safe XHTML with its own font container. These HTML switches
        // have no effect there and must not masquerade as publication options.
        "epub" => html && !matches!(name, "allowRawHtml" | "interactiveHtml" | "htmlFontFormat"),
        // The poster presently uses a fixed type ladder; do not claim that a
        // Theme fontScale field changes that ladder merely by passing it on.
        "svg" => matches!(name, "font" | "maxWidthPt"),
        _ => false,
    }
}

pub(crate) struct PreparedOptions {
    pub target: &'static str,
    pub html: HtmlOptions,
    pub pdf: PdfOptions,
    pub pdf_a: PdfASettings,
    pub svg: SvgOptions,
    pub interactive: bool,
}

/// Admit the entire option set before source I/O, asset decoding or rendering.
/// The resource payloads themselves are still decoded once by file_render.
pub(crate) fn prepare(args: &JsonValue) -> Result<PreparedOptions, ToolError> {
    validate_arguments(args, &fields(), &["path"])?;
    let value = string(args, "to").unwrap_or("html").trim().to_ascii_lowercase();
    let target = match value.as_str() {
        "html" => "html", "pdf" => "pdf", "both" => "both",
        "epub" => "epub", "svg" => "svg",
        _ => return Err(invalid_options(format!("Unsupported output target: '{value}'"), "unsupported_target")),
    };
    if let Some(object) = args.as_object() {
        for name in object.keys() {
            if !supports(target, name) {
                return Err(invalid_options(
                    format!("'{name}' is not supported for '{target}'; supported targets: {}", supported_targets(name)),
                    "unsupported_target_option",
                ));
            }
        }
    }
    let html = if matches!(target, "html" | "both" | "epub") {
        html_options(args)?
    } else {
        HtmlOptions::default()
    };
    let (pdf, pdf_a) = if matches!(target, "pdf" | "both") {
        pdf_options(args)?
    } else {
        (PdfOptions::default(), PdfASettings::OFF)
    };
    let svg = if target == "svg" {
        let width = args.get("maxWidthPt").and_then(JsonValue::as_f64).unwrap_or(612.0);
        if !width.is_finite() || !(144.0..=14400.0).contains(&width) {
            return Err(invalid_options("maxWidthPt must be 144..14400 finite points", "invalid_svg_width"));
        }
        SvgOptions { theme: theme(args)?, max_width_pt: width as f32 }
    } else {
        SvgOptions::default()
    };
    Ok(PreparedOptions { target, html, pdf, pdf_a, svg, interactive: boolean(args, "interactiveHtml") })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::mcp::parse_json;

    #[test]
    fn file_fields_are_unique_and_have_at_least_one_real_consumer() {
        let fields = fields();
        let names: std::collections::BTreeSet<_> = fields.iter().map(|field| field.0).collect();
        assert_eq!(names.len(), fields.len());
        assert!(!names.contains("markdown"));
        for name in names { assert!(!supported_targets(name).is_empty(), "{name}"); }
        for field in HTML_FIELDS.iter().chain(PDF_FIELDS) {
            if field.0 != "markdown" { assert!(supports("both", field.0), "{}", field.0); }
        }
    }

    #[test]
    fn paired_options_use_each_real_renderer_builder() {
        let args = parse_json(r#"{"path":"doc.md","to":" BOTH ","font":"serif","fontScale":"125%","title":"  Draft  ","lang":"de","toc":true,"tocDepth":2,"darkMode":"disabled","customCss":"","author":"Writer","microtype":"expansion","pdfA":"2b","pdfAStrict":true}"#).unwrap();
        let options = prepare(&args).unwrap();
        assert_eq!(options.target, "both");
        assert_eq!(options.html.title.as_deref(), Some("  Draft  "));
        assert_eq!(options.pdf.title, options.html.title);
        assert_eq!(options.html.custom_css.as_deref(), Some(""));
        assert_eq!(options.pdf.author.as_deref(), Some("Writer"));
        assert_eq!(options.pdf.toc_depth, options.html.toc_depth);
        assert_eq!(options.pdf_a, PdfASettings::a2b_strict());
        assert!(options.pdf.microtype.max_expansion_per_mille > 0);
        assert!(!options.pdf.microtype.protrusion);
    }

    #[test]
    fn options_without_a_target_consumer_are_refused_even_when_false() {
        for (target, field) in [("pdf", "customCss"), ("html", "author"),
            ("epub", "interactiveHtml"), ("epub", "allowRawHtml"),
            ("svg", "fontScale"), ("svg", "toc"), ("html", "maxWidthPt")] {
            let value = match field { "toc" | "allowRawHtml" | "interactiveHtml" => "false", "maxWidthPt" => "612", _ => "\"value\"" };
            let args = parse_json(&format!("{{\"path\":\"missing.md\",\"to\":\"{target}\",\"{field}\":{value}}}")).unwrap();
            let error = prepare(&args).err().unwrap();
            assert_eq!(error.2, "unsupported_target_option", "{field} for {target}");
        }
    }

    #[test]
    fn svg_width_does_not_silently_fall_back_or_overflow() {
        for width in [144.0, 595.28, 14400.0] {
            let args = parse_json(&format!("{{\"path\":\"doc.md\",\"to\":\"svg\",\"maxWidthPt\":{width}}}")).unwrap();
            assert_eq!(prepare(&args).unwrap().svg.max_width_pt, width as f32);
        }
        for width in [0.0, 143.99, 14400.01, f64::MAX] {
            let args = parse_json(&format!("{{\"path\":\"doc.md\",\"to\":\"svg\",\"maxWidthPt\":{width}}}")).unwrap();
            assert!(prepare(&args).is_err());
        }
    }
}
