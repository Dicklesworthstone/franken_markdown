//! Native file rendering for MCP. The core still receives only an AST/options.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::tools::{ToolError, wrap_text_content};
use super::tools::resources::{self, with_svg_warnings};
use super::{ERROR_INPUT_ERROR, ERROR_INPUT_TOO_LARGE, ERROR_INVALID_OPTIONS, ERROR_RENDER_FAILED};
use super::{JsonValue, base64_encode};
use crate::file_write::{OutputFile, write_outputs_staged};
use crate::PdfAMode;

struct Source {
    text: String,
    canonical_path: PathBuf,
    #[cfg(unix)]
    metadata: fs::Metadata,
}

fn input_error(message: String, reason: &'static str) -> ToolError {
    (ERROR_INPUT_ERROR, message, reason)
}

fn read_bounded<R: Read>(reader: R, limit: u64) -> Result<String, ToolError> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| input_error(format!("Cannot read Markdown: {error}"), "io_error"))?;
    if bytes.len() as u64 > limit {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!("Markdown file exceeds limit {limit}"),
            "input_too_large",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| input_error("Markdown file is not valid UTF-8".to_string(), "invalid_utf8"))
}

fn read_source(path: &Path, limit: u64) -> Result<Source, ToolError> {
    let canonical_path = fs::canonicalize(path).map_err(|error| {
        let reason = if error.kind() == std::io::ErrorKind::NotFound {
            "file_not_found"
        } else {
            "io_error"
        };
        input_error(format!("Cannot resolve '{}': {error}", path.display()), reason)
    })?;
    // Reject known directories/devices/FIFOs before opening a potentially
    // blocking stream, then check the descriptor again after opening it.
    let metadata = fs::metadata(&canonical_path)
        .map_err(|error| input_error(format!("Cannot stat '{}': {error}", path.display()), "io_error"))?;
    if !metadata.is_file() {
        return Err(input_error("Input must be a regular file".to_string(), "not_regular_file"));
    }
    let file = File::open(&canonical_path)
        .map_err(|error| input_error(format!("Cannot open '{}': {error}", path.display()), "io_error"))?;
    let metadata = file.metadata()
        .map_err(|error| input_error(format!("Cannot inspect input: {error}"), "io_error"))?;
    if !metadata.is_file() {
        return Err(input_error("Input must be a regular file".to_string(), "not_regular_file"));
    }
    if metadata.len() > limit {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!("File size {} exceeds limit {limit}", metadata.len()),
            "input_too_large",
        ));
    }
    // The actual read is bounded too: metadata can be stale if a file grows.
    let text = read_bounded(file, limit)?;
    Ok(Source {
        text,
        canonical_path,
        #[cfg(unix)]
        metadata,
    })
}

fn aliases_source(source: &Source, destination: &Path) -> bool {
    if fs::canonicalize(destination).ok().as_ref() == Some(&source.canonical_path) {
        return true;
    }
    // Also protect the original entry if it was removed after the source read.
    let parent = destination.parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let (Ok(parent), Some(name)) = (fs::canonicalize(parent), destination.file_name()) {
        if parent.join(name) == source.canonical_path {
            return true;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(metadata) = fs::metadata(destination) {
            return metadata.dev() == source.metadata.dev() && metadata.ino() == source.metadata.ino();
        }
    }
    false
}

struct Artifact {
    format: &'static str,
    mime: &'static str,
    bytes: Vec<u8>,
    binary: bool,
}

impl Artifact {
    fn text(&self) -> Result<String, ToolError> {
        if self.binary {
            Ok(base64_encode(&self.bytes))
        } else {
            String::from_utf8(self.bytes.clone()).map_err(|_| (
                ERROR_RENDER_FAILED,
                "Renderer returned invalid UTF-8".to_string(),
                "render_failed",
            ))
        }
    }
}

pub(super) fn render_file(args: &JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    let path = args.get("path").and_then(JsonValue::as_str).ok_or_else(|| (
        ERROR_INVALID_OPTIONS,
        "Missing required argument: 'path'".to_string(),
        "missing_path",
    ))?;
    if path.is_empty() {
        return Err((ERROR_INVALID_OPTIONS, "Input path must not be empty".to_string(), "missing_path"));
    }
    let prepared = super::tools::file_options::prepare(args)?;
    let target = prepared.target;
    let mut html_options = prepared.html;
    let mut pdf_options = prepared.pdf;
    let out = args.get("out").and_then(JsonValue::as_str);
    if out == Some("") {
        return Err((ERROR_INVALID_OPTIONS, "Output path must not be empty".to_string(), "invalid_output_path"));
    }
    let source = read_source(Path::new(path), limit)?;
    let destinations: Vec<PathBuf> = match out {
        None => Vec::new(),
        Some(out) if target == "both" => vec![Path::new(out).with_extension("html"), Path::new(out).with_extension("pdf")],
        Some(out) => vec![PathBuf::from(out)],
    };
    for destination in &destinations {
        if aliases_source(&source, destination) {
            return Err((ERROR_INVALID_OPTIONS, format!("Output '{}' would overwrite the Markdown source", destination.display()), "output_overwrites_input"));
        }
    }
    let assets = resources::parse(args)?;
    let title = Path::new(path).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    // Explicit empty or whitespace-padded titles are caller data, not defaults.
    if html_options.title.is_none() { html_options.title = Some(title.clone()); }
    if pdf_options.title.is_none() { pdf_options.title = Some(title); }
    if target == "both" {
        html_options.font_assets = assets.fonts.clone();
        html_options.image_assets = assets.images.clone();
        pdf_options.font_assets = assets.fonts;
        pdf_options.image_assets = assets.images;
    } else if matches!(target, "html" | "epub") {
        html_options.font_assets = assets.fonts;
        html_options.image_assets = assets.images;
    } else {
        pdf_options.font_assets = assets.fonts;
        pdf_options.image_assets = assets.images;
    }
    // Parse once, including for paired HTML/PDF output.
    let document = crate::parse_markdown(&source.text);
    let render_error = |error: crate::RenderError| (ERROR_RENDER_FAILED, format!("Render failed: {error}"), "render_failed");
    let mut artifacts = Vec::new();
    let mut svg_warnings = Vec::new();
    if matches!(target, "html" | "both") {
        let html = if prepared.interactive {
            crate::interactive::render_interactive_html(&document, &source.text, &html_options)
        } else {
            crate::render_html_document(&document, &html_options).map_err(render_error)?
        };
        artifacts.push(Artifact { format: "html", mime: "text/html", bytes: html.into_bytes(), binary: false });
    }
    if matches!(target, "pdf" | "both") {
        let pdf = if prepared.pdf_a.mode == PdfAMode::Off {
            crate::render_pdf_document(&document, &pdf_options)
        } else {
            crate::render_pdf_document_pdfa(&document, &pdf_options, prepared.pdf_a)
        }.map_err(render_error)?;
        artifacts.push(Artifact { format: "pdf", mime: "application/pdf", bytes: pdf, binary: true });
    }
    if target == "epub" {
        artifacts.push(Artifact { format: "epub", mime: "application/epub+zip", bytes: crate::render_epub(&document, &html_options).map_err(render_error)?, binary: true });
    }
    if target == "svg" {
        let (bytes, _, warnings) = crate::svg::render_svg_with_resources(
            &document, &prepared.svg, &pdf_options.font_assets, &pdf_options.image_assets,
        ).map_err(render_error)?;
        svg_warnings = warnings;
        artifacts.push(Artifact { format: "svg", mime: "image/svg+xml", bytes, binary: false });
    }
    if !destinations.is_empty() {
        // Every render and source-identity check finishes before any file write.
        let outputs: Vec<_> = destinations.iter().zip(&artifacts)
            .map(|(path, artifact)| OutputFile { path, bytes: &artifact.bytes }).collect();
        write_outputs_staged(&outputs).map_err(|error| input_error(
            format!("Cannot write '{}': {}", error.path.display(), error.source), "write_error"
        ))?;
        let written: Vec<_> = artifacts.iter().zip(&destinations)
            .map(|(artifact, path)| format!("Rendered {} written to {}", artifact.format.to_ascii_uppercase(), path.display())).collect();
        return Ok(with_svg_warnings(wrap_text_content(&written.join("\n")), svg_warnings));
    }
    if let [artifact] = artifacts.as_slice() {
        return Ok(with_svg_warnings(wrap_text_content(&artifact.text()?), svg_warnings));
    }
    // Existing single-format responses stay unchanged. The new paired response
    // carries both payloads with explicit format, media type and encoding.
    let outputs = artifacts.iter().map(|artifact| {
        Ok(JsonValue::Object(BTreeMap::from([
            ("format".to_string(), JsonValue::String(artifact.format.to_string())),
            ("mimeType".to_string(), JsonValue::String(artifact.mime.to_string())),
            ("encoding".to_string(), JsonValue::String(if artifact.binary { "base64" } else { "utf-8" }.to_string())),
            ("data".to_string(), JsonValue::String(artifact.text()?)),
        ])))
    }).collect::<Result<Vec<_>, ToolError>>()?;
    Ok(wrap_text_content(&JsonValue::Object(BTreeMap::from([
        ("outputs".to_string(), JsonValue::Array(outputs)),
    ])).to_json_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn actual_read_limit_does_not_depend_on_metadata() {
        let mut reader = Cursor::new(b"123456789");
        assert_eq!(read_bounded(&mut reader, 4).unwrap_err().2, "input_too_large");
        assert_eq!(reader.position(), 5);
        assert_eq!(read_bounded(Cursor::new(b"1234"), 4).unwrap(), "1234");
        assert_eq!(read_bounded(Cursor::new(b""), 0).unwrap(), "");
        assert_eq!(read_bounded(Cursor::new(b"x"), 0).unwrap_err().2, "input_too_large");
    }

    #[test]
    fn bounded_reader_rejects_invalid_utf8() {
        assert_eq!(read_bounded(Cursor::new([0xff]), 1).unwrap_err().2, "invalid_utf8");
    }
}
