//! Multi-document publication over an explicit in-memory source set.
//!
//! The book core owns paths, include expansion, citations and navigation. This
//! adapter only admits the MCP request and routes it to those real renderers.

use std::collections::BTreeMap;

use crate::book::{BookInput, BookRenderer, render_book_site};
use crate::book::validation::LinkReport;
use crate::mcp::ERROR_INPUT_ERROR;
use crate::{HtmlOptions, PdfASettings, PdfOptions};

use super::{
    ERROR_INPUT_TOO_LARGE, ERROR_RENDER_FAILED, Field, HTML_FIELDS, JsonValue as Json,
    PDF_FIELDS, ToolError, base64_encode, boolean, html_options, invalid_options, pdf_options,
    resources, string, wrap_text_content,
};

const MAX_SOURCES: usize = 4096;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;
pub(super) const MAX_OUTPUT_BYTES: u64 = 32 * 1024 * 1024;
const DEFAULT_OUTPUT_BYTES: u64 = 16 * 1024 * 1024;

pub(super) const SOURCE_FIELDS: &[Field] = &[
    ("chapters", "array", "Ordered chapter objects with logical path and markdown; no file reads"),
    ("includeSources", "array", "Additional path/markdown sources available only to explicit includes"),
];
const PUBLICATION_FIELDS: &[Field] = &[
    ("strictLinks", "boolean", "Reject broken chapter/anchor/note navigation before rendering; does not check external URLs, assets or output conformance"),
    ("to", "string", "Book output: pdf (default), epub, site (ZIP), or all"),
    ("maxOutputBytes", "integer", "Aggregate artifact bytes before base64, 1..33554432; default 16777216"),
];

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(fields.into_iter().map(|(key, value)| (key.to_string(), value)).collect())
}
fn text(value: &str) -> Json {
    Json::String(value.to_string())
}
fn number(value: usize) -> Json {
    Json::Number(value as f64)
}

pub(super) fn fields() -> Vec<Field> {
    let mut fields = BTreeMap::new();
    for &field in SOURCE_FIELDS.iter().chain(PUBLICATION_FIELDS).chain(HTML_FIELDS).chain(PDF_FIELDS) {
        // Both PDF modes now use the chapter-aware book pipeline.
        if !matches!(field.0, "markdown" | "interactiveHtml") {
            fields.entry(field.0).or_insert(field);
        }
    }
    fields.into_values().collect()
}

pub(super) fn schema(name: &str) -> Option<Json> {
    match name {
        "chapters" | "includeSources" => Some(object([
            ("type", text("array")),
            ("minItems", number(usize::from(name == "chapters"))),
            ("maxItems", number(MAX_SOURCES)),
            ("items", object([
                ("type", text("object")),
                ("additionalProperties", Json::Bool(false)),
                ("required", Json::Array(vec![text("path"), text("markdown")])),
                ("properties", object([
                    ("path", object([
                        ("type", text("string")),
                        ("minLength", number(1)),
                        ("maxLength", number(MAX_PATH_BYTES)),
                        ("description", text("Logical book-relative path, at most 4096 UTF-8 bytes; never read from disk")),
                    ])),
                    ("markdown", object([("type", text("string"))])),
                ])),
            ])),
        ])),
        "to" => Some(object([
            ("type", text("string")),
            ("enum", Json::Array(["pdf", "epub", "site", "all"].map(text).to_vec())),
            ("default", text("pdf")),
        ])),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format { Pdf, Epub, Site }
impl Format {
    fn name(self) -> &'static str {
        match self { Self::Pdf => "pdf", Self::Epub => "epub", Self::Site => "site" }
    }
    fn mime(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Epub => "application/epub+zip",
            Self::Site => "application/zip",
        }
    }
    fn supports(self, name: &str) -> bool {
        let listed = |fields: &[Field]| fields.iter().any(|field| field.0 == name);
        match self {
            Self::Pdf => listed(PDF_FIELDS),
            Self::Site => listed(HTML_FIELDS),
            Self::Epub => listed(HTML_FIELDS) && !matches!(name, "allowRawHtml" | "htmlFontFormat"),
        }
    }
}

struct Publication {
    formats: &'static [Format],
    output_limit: usize,
    html: HtmlOptions,
    pdf: PdfOptions,
    pdf_a: PdfASettings,
}
impl Publication {
    fn prepare(args: &Json) -> Result<Self, ToolError> {
        let formats: &'static [Format] = match string(args, "to").unwrap_or("pdf") {
            "pdf" => &[Format::Pdf],
            "epub" => &[Format::Epub],
            "site" => &[Format::Site],
            "all" => &[Format::Pdf, Format::Epub, Format::Site],
            _ => return Err(invalid_options("Book to must be pdf, epub, site, or all", "unsupported_target")),
        };
        if let Some(fields) = args.as_object() {
            for name in fields.keys() {
                if SOURCE_FIELDS.iter().chain(PUBLICATION_FIELDS).any(|field| field.0 == name.as_str()) {
                    continue;
                }
                if !formats.iter().any(|format| format.supports(name)) {
                    return Err(invalid_options(
                        format!("'{name}' has no consumer in the selected book output(s)"),
                        "unsupported_target_option",
                    ));
                }
            }
        }
        let output_limit = args.get("maxOutputBytes")
            .map(Json::as_u64).unwrap_or(Some(DEFAULT_OUTPUT_BYTES))
            .filter(|limit| (1..=MAX_OUTPUT_BYTES).contains(limit))
            .and_then(|limit| usize::try_from(limit).ok())
            .ok_or_else(|| invalid_options("maxOutputBytes must be 1..33554432", "invalid_output_limit"))?;
        let html = if formats.iter().any(|format| *format != Format::Pdf) {
            html_options(args)?
        } else { HtmlOptions::default() };
        let (pdf, pdf_a) = if formats.contains(&Format::Pdf) {
            pdf_options(args)?
        } else { (PdfOptions::default(), PdfASettings::OFF) };
        Ok(Self { formats, output_limit, html, pdf, pdf_a })
    }
}

fn input_error(message: impl std::fmt::Display) -> ToolError {
    (ERROR_INPUT_ERROR, format!("Book source error: {message}"), "invalid_book_sources")
}

fn array<'a>(args: &'a Json, name: &str) -> Result<&'a [Json], ToolError> {
    match args.get(name) {
        Some(Json::Array(items)) => Ok(items),
        None if name == "includeSources" => Ok(&[]),
        _ => Err(input_error(format!("{name} must be an array"))),
    }
}

/// Validate the complete source collection before cloning any Markdown, asset
/// decoding, include expansion or parsing. The core then owns path semantics.
fn sources(args: &Json, limit: u64) -> Result<(Vec<BookInput>, Vec<BookInput>), ToolError> {
    let chapters = array(args, "chapters")?;
    let includes = array(args, "includeSources")?;
    if chapters.is_empty() || chapters.len() > MAX_SOURCES
        || includes.len() > MAX_SOURCES.saturating_sub(chapters.len())
    {
        return Err(input_error("expected 1..4096 chapters and at most 4096 total sources"));
    }
    let limit = limit.min(MAX_SOURCE_BYTES);
    let mut total = 0u64;
    for item in chapters.iter().chain(includes) {
        let fields = item.as_object().ok_or_else(|| input_error("each source must be an object"))?;
        if fields.len() != 2 || fields.keys().any(|key| !matches!(key.as_str(), "path" | "markdown")) {
            return Err(input_error("each source requires exactly path and markdown"));
        }
        let path = string(item, "path").ok_or_else(|| input_error("source path must be a string"))?;
        let markdown = string(item, "markdown").ok_or_else(|| input_error("source markdown must be a string"))?;
        if path.is_empty() || path.len() > MAX_PATH_BYTES || path.chars().any(char::is_control) {
            return Err(input_error("source path must have 1..4096 UTF-8 bytes without controls"));
        }
        for bytes in [path.len(), markdown.len()] {
            if bytes as u64 > limit.saturating_sub(total) {
                return Err((ERROR_INPUT_TOO_LARGE,
                    format!("Book source text and paths exceed the {limit}-byte limit"), "input_too_large"));
            }
            total += bytes as u64;
        }
    }
    let own = |items: &[Json]| items.iter().map(|item| BookInput {
        // Already type-checked above; use no unchecked indexing or panic path.
        path: string(item, "path").unwrap_or_default().to_string(),
        source: string(item, "markdown").unwrap_or_default().to_string(),
    }).collect();
    Ok((own(chapters), own(includes)))
}

fn renderer(args: &Json, limit: u64) -> Result<BookRenderer, ToolError> {
    let (chapters, includes) = sources(args, limit)?;
    BookRenderer::from_sources(&chapters, &includes).map_err(input_error)
}

fn link_analysis_error(error: crate::RenderError) -> ToolError {
    (ERROR_RENDER_FAILED, format!("Book link analysis did not complete: {error}"),
        "book_link_analysis_failed")
}

fn link_report(renderer: &BookRenderer) -> Result<LinkReport, ToolError> {
    renderer.validate_links().map_err(link_analysis_error)
}

/// Source-only navigation verification. Findings are successful report data;
/// an admission/analysis failure is an error, never an empty clean report.
pub(super) fn verify(args: &Json, limit: u64) -> Result<Json, ToolError> {
    let renderer = renderer(args, limit)?;
    let report = link_report(&renderer)?;
    let json = report.to_json().map_err(link_analysis_error)?;
    Ok(wrap_text_content(&json))
}

fn enforce_links(renderer: &BookRenderer) -> Result<(), ToolError> {
    let report = link_report(renderer)?;
    if let Some((chapter, issue)) = report.chapters.iter()
        .find_map(|chapter| chapter.findings.first().map(|issue| (chapter, issue)))
    {
        // Keep a bounded actionable preview in the error; the complete,
        // machine-readable findings are available through fmd.verify_book.
        let path: String = chapter.path.chars().take(96).collect();
        let destination: String = issue.destination.chars().take(128).collect();
        return Err((ERROR_RENDER_FAILED, format!(
            "Book navigation has {} finding(s). First: {} in {path:?}, target {destination:?}. Run fmd.verify_book with the same chapters/includeSources for the complete report.",
            report.finding_count(), issue.code), "book_links_failed"));
    }
    Ok(())
}

pub(super) fn render(args: &Json, limit: u64) -> Result<Json, ToolError> {
    let mut publication = Publication::prepare(args)?;
    let renderer = renderer(args, limit)?;
    if boolean(args, "strictLinks") {
        enforce_links(&renderer)?;
    }
    let assets = resources::parse(args)?;
    // Transfer ownership between format options rather than copying the image
    // and font payloads for a multi-format request.
    publication.pdf.font_assets = assets.fonts;
    publication.pdf.image_assets = assets.images;
    let mut outputs = Vec::new();
    let mut bytes_written = 0usize;
    for &format in publication.formats {
        if format != Format::Pdf && publication.html.image_assets.is_empty()
            && publication.html.font_assets.is_empty()
        {
            publication.html.image_assets = std::mem::take(&mut publication.pdf.image_assets);
            publication.html.font_assets = std::mem::take(&mut publication.pdf.font_assets);
        }
        let bytes = match format {
            Format::Pdf => renderer.render_pdf_with_options(&publication.pdf, publication.pdf_a),
            Format::Epub => crate::epub::render_book_epub(renderer.book(), &publication.html),
            Format::Site => render_book_site(renderer.book(), &publication.html),
        }.map_err(|error| (ERROR_RENDER_FAILED, format!("Book {} render failed: {error}", format.name()), "render_failed"))?;
        if bytes.len() > publication.output_limit.saturating_sub(bytes_written) {
            return Err((ERROR_RENDER_FAILED,
                format!("Book output exceeds the {}-byte aggregate artifact limit", publication.output_limit),
                "output_too_large"));
        }
        bytes_written += bytes.len();
        let mut output = object([
            ("format", text(format.name())),
            ("mimeType", text(format.mime())),
            ("encoding", text("base64")),
            ("bytes", number(bytes.len())),
            ("data", Json::String(base64_encode(&bytes))),
        ]);
        // This describes emission settings, not an independent validation
        // result. Omitted/OFF keeps the existing receipt byte-for-byte.
        match &mut output {
            Json::Object(fields) if format == Format::Pdf && publication.pdf_a.mode.is_a2b() => {
                fields.insert("pdfA".into(), text(publication.pdf_a.mode.as_str()));
                fields.insert("pdfAStrict".into(), Json::Bool(publication.pdf_a.strict));
            }
            _ => {}
        }
        outputs.push(output);
    }
    let result = object([
        ("schema", text("fmd-mcp-book-publication-v1")),
        ("chapters", number(renderer.book().chapters.len())),
        ("sourceBytes", number(renderer.source_length())),
        ("artifactBytes", number(bytes_written)),
        ("outputs", Json::Array(outputs)),
    ]);
    Ok(wrap_text_content(&result.to_json_string()))
}
