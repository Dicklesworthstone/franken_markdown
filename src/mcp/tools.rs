//! MCP tool declarations and execution share the same argument contract.

use std::collections::BTreeMap;
use std::time::Instant;

use super::{
    ERROR_INPUT_TOO_LARGE, ERROR_INVALID_OPTIONS, ERROR_RENDER_FAILED, INVALID_PARAMS,
    JsonValue, METHOD_NOT_FOUND, base64_encode,
};
use crate::layout::MicrotypeOptions;
use crate::{
    DarkModePolicy, FontFamily, FontScale, HtmlFontFormat, HtmlOptions, PdfAMode,
    PdfASettings, PdfOptions, Theme,
};

pub(super) type ToolError = (i32, String, &'static str);
type Field = (&'static str, &'static str, &'static str);
type ToolSpec = (&'static str, &'static str, &'static [Field], &'static [&'static str]);
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

const HTML_FIELDS: &[Field] = &[
    ("markdown", "string", "Markdown source text to render"),
    ("font", "string", "Body font family ('sans' or 'serif')"),
    ("darkMode", "string", "Dark mode policy ('auto' or 'disabled')"),
    ("customCss", "string", "Custom stylesheet CSS replacing default theme"),
    ("title", "string", "Document title metadata"),
    ("lang", "string", "Document language tag (e.g. 'en', 'de')"),
    ("allowRawHtml", "boolean", "Pass raw HTML through without escaping"),
    ("toc", "boolean", "Generate a table of contents"),
    ("tocDepth", "integer", "Maximum table-of-contents heading depth (1..6)"),
    ("htmlFontFormat", "string", "Font format ('woff1', 'woff2', or 'ttf')"),
    ("interactiveHtml", "boolean", "Generate self-hosting interactive HTML"),
    ("fontScale", "string", "Typographic scale preset or multiplier ('sm', '125%')"),
];
const PDF_FIELDS: &[Field] = &[
    ("markdown", "string", "Markdown source text to render"),
    ("font", "string", "Body font family ('sans' or 'serif')"),
    ("title", "string", "Document title metadata"),
    ("author", "string", "Document author metadata"),
    ("lang", "string", "Document language tag for hyphenation"),
    ("fontScale", "string", "Typographic scale preset or multiplier"),
    ("fitToPages", "integer", "Positive adaptive page budget"),
    ("microtype", "string", "Microtypography ('off', 'protrusion', 'expansion', 'all')"),
    ("typographyHomogeneous", "boolean", "Gradual adjacent demerits in the line breaker"),
    ("codeLineNumbers", "boolean", "Render line numbers in code blocks"),
    ("pageNumbers", "boolean", "Render running page numbers in the bottom margin"),
    ("toc", "boolean", "Generate a table of contents"),
    ("tocDepth", "integer", "Maximum table-of-contents heading depth (1..6)"),
    ("pdfA", "string", "PDF/A profile ('2b' or 'off')"),
    ("pdfAStrict", "boolean", "Fail closed on non-conformable PDF/A; requires pdfA='2b'"),
    ("metadataEpochSeconds", "integer", "Deterministic nonnegative UNIX epoch timestamp"),
];
const VERIFY_FIELDS: &[Field] = &[
    ("markdown", "string", "Markdown source text to verify"),
    ("a11y", "boolean", "Restrict findings to the accessibility audit"),
];
const FILE_FIELDS: &[Field] = &[
    ("path", "string", "Local path to a UTF-8 Markdown regular file"),
    ("to", "string", "Output format ('html', 'pdf', 'both', 'epub', 'svg')"),
    ("out", "string", "Optional output path; 'both' replaces its extension with .html and .pdf"),
];
const TOOLS: &[ToolSpec] = &[
    ("fmd.render_html", "Render Markdown to self-contained HTML with inlined fonts and styles.", HTML_FIELDS, &["markdown"]),
    ("fmd.render_pdf", "Render Markdown to deterministic PDF with embedded subsets, returned as base64.", PDF_FIELDS, &["markdown"]),
    ("fmd.verify", "Audit text, internal anchors, accessibility, and horizontal overflow.", VERIFY_FIELDS, &["markdown"]),
    ("fmd.capabilities", "Discover the stable feature contract and theme model.", &[], &[]),
    ("fmd.render_file", "Render a local Markdown file. HTML/SVG return text; PDF/EPUB return base64. Without out, both returns a JSON outputs array with format, mimeType, encoding and data. With out, all artifacts are rendered and staged before replacement; the source is protected.", FILE_FIELDS, &["path"]),
];

fn integer_bounds(name: &str) -> (u64, u64) {
    match name {
        "tocDepth" => (1, 6),
        "fitToPages" => (1, MAX_SAFE_INTEGER.min(usize::MAX as u64)),
        _ => (0, MAX_SAFE_INTEGER),
    }
}

pub fn tools_list_result() -> JsonValue {
    let tools = TOOLS.iter().map(|(name, description, fields, required)| {
        let properties = fields.iter().map(|(name, kind, description)| {
            let mut property = BTreeMap::from([
                ("type".to_string(), JsonValue::String((*kind).to_string())),
                ("description".to_string(), JsonValue::String((*description).to_string())),
            ]);
            if *kind == "integer" {
                let (min, max) = integer_bounds(name);
                property.insert("minimum".to_string(), JsonValue::Number(min as f64));
                property.insert("maximum".to_string(), JsonValue::Number(max as f64));
            }
            ((*name).to_string(), JsonValue::Object(property))
        }).collect();
        let mut schema = BTreeMap::from([
            ("type".to_string(), JsonValue::String("object".to_string())),
            ("properties".to_string(), JsonValue::Object(properties)),
            ("additionalProperties".to_string(), JsonValue::Bool(false)),
        ]);
        if !required.is_empty() {
            schema.insert("required".to_string(), JsonValue::Array(required.iter()
                .map(|name| JsonValue::String((*name).to_string())).collect()));
        }
        JsonValue::Object(BTreeMap::from([
            ("name".to_string(), JsonValue::String((*name).to_string())),
            ("description".to_string(), JsonValue::String((*description).to_string())),
            ("inputSchema".to_string(), JsonValue::Object(schema)),
        ]))
    }).collect();
    JsonValue::Object(BTreeMap::from([("tools".to_string(), JsonValue::Array(tools))]))
}

fn invalid_options(message: impl Into<String>, reason: &'static str) -> ToolError {
    (ERROR_INVALID_OPTIONS, message.into(), reason)
}

fn validate_arguments(args: &JsonValue, fields: &[Field], required: &[&str]) -> Result<(), ToolError> {
    let args = args.as_object().ok_or_else(|| (
        INVALID_PARAMS, "Tool arguments must be an object".to_string(), "invalid_params",
    ))?;
    for (name, value) in args {
        let (_, kind, _) = fields.iter().find(|(key, _, _)| *key == name.as_str()).ok_or_else(|| (
            INVALID_PARAMS, format!("Unknown tool argument: '{name}'"), "invalid_params",
        ))?;
        let valid = match *kind {
            "string" => matches!(value, JsonValue::String(_)),
            "boolean" => matches!(value, JsonValue::Bool(_)),
            "integer" => {
                let (min, max) = integer_bounds(name);
                value.as_u64().is_some_and(|value| (min..=max).contains(&value))
            }
            _ => false,
        };
        if !valid {
            return Err((INVALID_PARAMS, format!("Invalid '{name}': expected {kind} in the advertised range"), "invalid_params"));
        }
    }
    for name in required {
        if !args.contains_key(*name) {
            let reason = if *name == "markdown" { "missing_markdown" } else { "missing_path" };
            return Err(invalid_options(format!("Missing required argument: '{name}'"), reason));
        }
    }
    Ok(())
}

fn string<'a>(args: &'a JsonValue, name: &str) -> Option<&'a str> {
    args.get(name).and_then(JsonValue::as_str)
}

fn boolean(args: &JsonValue, name: &str) -> bool {
    args.get(name).and_then(JsonValue::as_bool).unwrap_or(false)
}

fn theme(args: &JsonValue) -> Result<Theme, ToolError> {
    let mut theme = Theme::default();
    if let Some(font) = string(args, "font") {
        theme.font = FontFamily::parse(font).ok_or_else(||
            invalid_options(format!("Invalid font '{font}'"), "invalid_font"))?;
    }
    if let Some(scale) = string(args, "fontScale") {
        let scale = FontScale::parse(scale).ok_or_else(||
            invalid_options(format!("Invalid fontScale '{scale}'"), "invalid_font_scale"))?;
        theme = theme.with_font_scale(scale);
    }
    Ok(theme)
}

fn toc_depth(args: &JsonValue) -> Result<Option<u8>, ToolError> {
    args.get("tocDepth").map(|value| {
        value.as_u64().filter(|depth| (1..=6).contains(depth))
            .and_then(|depth| u8::try_from(depth).ok())
            .ok_or_else(|| invalid_options("tocDepth must be an integer from 1 to 6", "invalid_toc_depth"))
    }).transpose()
}

fn html_options(args: &JsonValue) -> Result<HtmlOptions, ToolError> {
    let mut theme = theme(args)?;
    if let Some(policy) = string(args, "darkMode") {
        theme.dark_mode = match policy.trim().to_ascii_lowercase().as_str() {
            "auto" => DarkModePolicy::Auto,
            "disabled" => DarkModePolicy::Disabled,
            _ => return Err(invalid_options("darkMode must be 'auto' or 'disabled'", "invalid_dark_mode")),
        };
    }
    let html_font_format = string(args, "htmlFontFormat").map(|format| {
        HtmlFontFormat::parse(format).ok_or_else(|| invalid_options(
            "htmlFontFormat must be 'woff1', 'woff2', or 'ttf'", "invalid_html_font_format",
        ))
    }).transpose()?.unwrap_or_default();
    Ok(HtmlOptions {
        theme,
        title: string(args, "title").map(str::to_string),
        custom_css: string(args, "customCss").map(str::to_string),
        lang: string(args, "lang").map(str::to_string),
        allow_raw_html: boolean(args, "allowRawHtml"),
        toc: boolean(args, "toc"),
        toc_depth: toc_depth(args)?,
        html_font_format,
        ..Default::default()
    })
}

fn pdf_options(args: &JsonValue) -> Result<(PdfOptions, PdfASettings), ToolError> {
    let microtype = match string(args, "microtype").unwrap_or("off").trim().to_ascii_lowercase().as_str() {
        "off" => MicrotypeOptions::DISABLED,
        "protrusion" | "all" => MicrotypeOptions::CONSERVATIVE,
        "expansion" => MicrotypeOptions { protrusion: false, ..MicrotypeOptions::CONSERVATIVE },
        _ => return Err(invalid_options("microtype must be 'off', 'protrusion', 'expansion', or 'all'", "invalid_microtype")),
    };
    let fit_to_pages = args.get("fitToPages").map(|value| {
        value.as_u64().filter(|pages| *pages > 0)
            .and_then(|pages| usize::try_from(pages).ok())
            .ok_or_else(|| invalid_options("fitToPages must be a positive supported page count", "invalid_fit_to_pages"))
    }).transpose()?;
    let mode = string(args, "pdfA").map(|mode|
        PdfAMode::parse(mode).ok_or_else(|| invalid_options("pdfA must be '2b' or 'off'", "invalid_pdf_a"))
    ).transpose()?.unwrap_or_default();
    let strict = boolean(args, "pdfAStrict");
    if strict && mode == PdfAMode::Off {
        return Err(invalid_options("pdfAStrict requires pdfA='2b'", "invalid_pdf_a"));
    }
    Ok((PdfOptions {
        theme: theme(args)?,
        title: string(args, "title").map(str::to_string),
        author: string(args, "author").map(str::to_string),
        lang: string(args, "lang").map(str::to_string),
        fit_to_pages,
        gradual_demerits: boolean(args, "typographyHomogeneous"),
        code_line_numbers: boolean(args, "codeLineNumbers"),
        page_numbers: boolean(args, "pageNumbers"),
        toc: boolean(args, "toc"),
        toc_depth: toc_depth(args)?,
        metadata_epoch_seconds: args.get("metadataEpochSeconds").and_then(JsonValue::as_u64),
        microtype,
        ..Default::default()
    }, PdfASettings { mode, strict }))
}

fn markdown(args: &JsonValue, limit: u64) -> Result<&str, ToolError> {
    let markdown = string(args, "markdown").ok_or_else(||
        invalid_options("Missing required argument: 'markdown'", "missing_markdown"))?;
    if markdown.len() as u64 > limit {
        return Err((ERROR_INPUT_TOO_LARGE, format!("Markdown input length {} exceeds limit {limit}", markdown.len()), "input_too_large"));
    }
    Ok(markdown)
}

pub fn handle_tool_call(params: Option<&JsonValue>, max_input_bytes: u64) -> Result<JsonValue, ToolError> {
    let params = params.and_then(JsonValue::as_object).ok_or_else(|| (
        INVALID_PARAMS, "Expected params object with name and arguments".to_string(), "invalid_params",
    ))?;
    let name = params.get("name").and_then(JsonValue::as_str).ok_or_else(|| (
        INVALID_PARAMS, "Missing tool name string".to_string(), "invalid_params",
    ))?;
    let (_, _, fields, required) = TOOLS.iter().find(|(tool, _, _, _)| *tool == name).ok_or_else(|| (
        METHOD_NOT_FOUND, format!("Unknown tool: '{name}'"), "method_not_found",
    ))?;
    let default_args = JsonValue::Object(BTreeMap::new());
    let args = params.get("arguments").unwrap_or(&default_args);
    validate_arguments(args, fields, required)?;
    let started = Instant::now();
    let result = execute(name, args, max_input_bytes);
    eprintln!("fmd mcp tool '{name}' completed in {:?}", started.elapsed());
    result
}

fn execute(name: &str, args: &JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    let render_error = |error: crate::RenderError| (
        ERROR_RENDER_FAILED, format!("Render failed: {error}"), "render_failed",
    );
    match name {
        "fmd.render_html" => {
            let source = markdown(args, limit)?;
            let options = html_options(args)?;
            let doc = crate::parse_markdown(source);
            let html = if boolean(args, "interactiveHtml") {
                crate::interactive::render_interactive_html(&doc, source, &options)
            } else {
                crate::render_html_document(&doc, &options).map_err(render_error)?
            };
            Ok(wrap_text_content(&html))
        }
        "fmd.render_pdf" => {
            let source = markdown(args, limit)?;
            let (options, settings) = pdf_options(args)?;
            let doc = crate::parse_markdown(source);
            let pdf = if settings.mode == PdfAMode::Off {
                crate::render_pdf_document(&doc, &options)
            } else {
                crate::render_pdf_document_pdfa(&doc, &options, settings)
            }.map_err(render_error)?;
            Ok(wrap_text_content(&base64_encode(&pdf)))
        }
        "fmd.verify" => {
            let source = markdown(args, limit)?;
            let doc = crate::parse_markdown(source);
            let report = crate::verify::verify_pdf(&doc, &PdfOptions::default()).ok_or_else(|| (
                ERROR_RENDER_FAILED, "Verification failed: cannot load fonts".to_string(), "verify_failed",
            ))?;
            let report = if boolean(args, "a11y") { crate::verify::filter_a11y(report) } else { report };
            Ok(wrap_text_content(&crate::verify::to_json(&report)))
        }
        "fmd.capabilities" => {
            let json = format!(
                "{{\"tool\":\"fmd\",\"version\":\"{}\",\"contract_version\":\"0.1.0\",\"outputs\":[\"html\",\"pdf\",\"both\",\"epub\",\"svg\"],\"theme_model\":{{\"status\":\"structured_v1\",\"default\":{}}},\"features\":{{\"html\":\"available\",\"pdf\":\"available_v0_embedded_subset_fonts\",\"mcp\":\"available_stdio_jsonrpc\"}}}}",
                env!("CARGO_PKG_VERSION"), Theme::default().to_config_json()
            );
            Ok(wrap_text_content(&json))
        }
        "fmd.render_file" => super::file_render::render_file(args, limit),
        _ => Err((METHOD_NOT_FOUND, format!("Unknown tool: '{name}'"), "method_not_found")),
    }
}

pub(super) fn wrap_text_content(text: &str) -> JsonValue {
    let content = JsonValue::Object(BTreeMap::from([
        ("type".to_string(), JsonValue::String("text".to_string())),
        ("text".to_string(), JsonValue::String(text.to_string())),
    ]));
    JsonValue::Object(BTreeMap::from([
        ("content".to_string(), JsonValue::Array(vec![content])),
        ("isError".to_string(), JsonValue::Bool(false)),
    ]))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::mcp::parse_json;

    #[test]
    fn html_options_apply_previously_ignored_fields() {
        let args = parse_json(r#"{"darkMode":"disabled","toc":true,"tocDepth":2,"htmlFontFormat":"woff2","font":"serif"}"#).unwrap();
        let opts = html_options(&args).unwrap();
        assert_eq!(opts.theme.dark_mode, DarkModePolicy::Disabled);
        assert_eq!(opts.theme.font, FontFamily::Serif);
        assert_eq!(opts.toc_depth, Some(2));
        assert_eq!(opts.html_font_format, HtmlFontFormat::Woff2);
        assert!(opts.toc);
    }

    #[test]
    fn pdf_microtype_modes_reach_the_renderer_options() {
        for (mode, protrusion, expansion) in [
            ("off", false, false), ("expansion", false, true),
            ("protrusion", true, true), ("all", true, true),
        ] {
            let args = parse_json(&format!("{{\"microtype\":\"{mode}\"}}")).unwrap();
            let (opts, _) = pdf_options(&args).unwrap();
            assert_eq!(opts.microtype.protrusion, protrusion);
            assert_eq!(opts.microtype.max_expansion_per_mille > 0, expansion);
        }
    }

    #[test]
    fn strict_pdfa_cannot_silently_render_non_archival_pdf() {
        assert!(pdf_options(&parse_json(r#"{"pdfAStrict":true}"#).unwrap()).is_err());
        assert!(pdf_options(&parse_json(r#"{"pdfAStrict":true,"pdfA":"off"}"#).unwrap()).is_err());
        let (_, settings) = pdf_options(&parse_json(r#"{"pdfAStrict":true,"pdfA":"2b"}"#).unwrap()).unwrap();
        assert_eq!(settings, PdfASettings::a2b_strict());
    }
}
