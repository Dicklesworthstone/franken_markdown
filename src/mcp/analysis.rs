//! Source-only document intelligence and revision review for MCP clients.

use std::borrow::Cow;

use super::{
    AUTHORING_PROFILE, ERROR_INPUT_TOO_LARGE, Field, JsonValue, ToolError, authoring_profile,
    invalid_options, markdown, string, wrap_text_content,
};
use crate::ast::Document;
use crate::{HtmlOptions, Profile};

pub(super) const STATS_FIELDS: &[Field] = &[
    ("markdown", "string", "Markdown source text to analyze"),
    AUTHORING_PROFILE,
];

pub(super) const DIFF_FIELDS: &[Field] = &[
    ("oldMarkdown", "string", "Old / base Markdown source text"),
    (
        "newMarkdown",
        "string",
        "New / updated Markdown source text",
    ),
    AUTHORING_PROFILE,
    (
        "format",
        "string",
        "Result format: 'json' for typed semantic changes (default), or 'html' for a visual Before/Current review",
    ),
];

// Explicit analysis uses publication transforms. An omitted profile preserves
// the legacy stats/diff treatment of ordinary page-break comments.
fn visible_document(doc: &Document, profile: Option<Profile>) -> Cow<'_, Document> {
    if profile.is_some() {
        crate::apply_profile(doc, profile)
    } else {
        Cow::Borrowed(doc)
    }
}

pub(super) fn stats(args: &JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    let profile = authoring_profile(args)?;
    let source = markdown(args, limit)?;
    let document = crate::parse_markdown_with_profile(source, profile);
    let document = visible_document(&document, profile);
    let stats = crate::doc_stats::compute_doc_stats(source, &document);
    Ok(wrap_text_content(&stats.to_json()))
}

pub(super) fn diff(args: &JsonValue, limit: u64) -> Result<JsonValue, ToolError> {
    let profile = authoring_profile(args)?;
    let format = string(args, "format").unwrap_or("json");
    if !matches!(format, "json" | "html") {
        return Err(invalid_options(
            "format must be 'json' or 'html'",
            "invalid_diff_format",
        ));
    }
    let old_source = string(args, "oldMarkdown").ok_or_else(|| {
        invalid_options(
            "Missing required argument: 'oldMarkdown'",
            "missing_old_markdown",
        )
    })?;
    let new_source = string(args, "newMarkdown").ok_or_else(|| {
        invalid_options(
            "Missing required argument: 'newMarkdown'",
            "missing_new_markdown",
        )
    })?;
    // Charge both originals before parsing either. Independent per-revision
    // checks would let one request retain twice the configured source budget.
    let source_bytes = (old_source.len() as u64).checked_add(new_source.len() as u64);
    if source_bytes.is_none_or(|bytes| bytes > limit) {
        return Err((
            ERROR_INPUT_TOO_LARGE,
            format!("Combined Markdown input length exceeds limit {limit}"),
            "input_too_large",
        ));
    }
    let old = crate::parse_markdown_with_profile(old_source, profile);
    let new = crate::parse_markdown_with_profile(new_source, profile);
    let old = visible_document(&old, profile);
    let new = visible_document(&new, profile);
    let output = if format == "html" {
        crate::diff::compute_diff(&old, &new, "Before", "Current").to_html_with_options(
            &HtmlOptions {
                profile,
                ..HtmlOptions::default()
            },
        )
    } else {
        crate::diff::report_json(&crate::diff::diff_documents(&old, &new))
    };
    Ok(wrap_text_content(&output))
}
