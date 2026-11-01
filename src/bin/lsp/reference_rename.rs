//! Document-local reference-label rename using parser-owned source locations.
//!
//! Full references change only their key. Collapsed/shortcut forms acquire an
//! explicit suffix so visible link text and image alt text remain unchanged.
//! Every proposed edit set must reproduce the entire original AST exactly.

use std::collections::BTreeMap;
use std::ops::Range;

use franken_markdown::parse::{ReferenceLabelForm, ReferenceLabelSpan, reference_label_spans};
use franken_markdown::{Block, Document, Inline, parse_markdown};

use super::{
    Buffer, Json, LineIndex, MAX_DOCUMENT_BYTES, MAX_SESSION_BYTES, Position, object, position,
    string,
};

type Failure = (i32, &'static str);
const MAX_LABEL_BYTES: usize = 4096;
const MAX_SYMBOLS: usize = 8192;
const MAX_EDITS: usize = 4096;
const MAX_EDIT_BYTES: usize = 2 * 1024 * 1024;

struct Edit {
    range: Range<usize>,
    text: String,
}

fn range(index: &LineIndex, source: &str, span: &Range<usize>) -> Json {
    object([
        ("start", position(index.position(source, span.start))),
        ("end", position(index.position(source, span.end))),
    ])
}

fn new_key(name: &str) -> Result<String, Failure> {
    if name.is_empty() || name.len() > MAX_LABEL_BYTES || name.chars().any(|ch| ch.is_control()) {
        return Err((
            -32602,
            "newName must be a bounded single-line reference label",
        ));
    }
    // The real parser decides label admission and normalization, including
    // Unicode expansion, literal escapes, whitespace and the 999-scalar cap.
    let source = format!("[probe][{name}]\n\n[{name}]: /fmd-reference-rename-target\n");
    let expected = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Link {
            dest: "/fmd-reference-rename-target".to_string(),
            title: None,
            content: vec![Inline::Text("probe".to_string())],
        }])],
    };
    let spans = reference_label_spans(&source);
    if parse_markdown(&source) != expected || spans.len() != 2 {
        return Err((-32602, "newName is not a parser-valid reference label"));
    }
    let definition = spans
        .iter()
        .find(|span| span.form == ReferenceLabelForm::Definition)
        .ok_or((-32602, "newName has no reference-definition identity"))?;
    if spans.iter().any(|span| span.key != definition.key) {
        return Err((-32602, "newName has ambiguous reference identity"));
    }
    Ok(definition.key.clone())
}

fn edit_for(
    span: &ReferenceLabelSpan,
    old_key: &str,
    new_key: &str,
    name: &str,
) -> Result<Option<Edit>, Failure> {
    let source_range = span.replacement.clone().ok_or((
        -32803,
        "reference label has a noncontiguous source range; rename cannot omit it",
    ))?;
    let text = match span.form {
        ReferenceLabelForm::Definition | ReferenceLabelForm::Full => name.to_string(),
        ReferenceLabelForm::Collapsed | ReferenceLabelForm::Shortcut if new_key == old_key => {
            return Ok(None);
        }
        ReferenceLabelForm::Collapsed | ReferenceLabelForm::Shortcut => format!("[{name}]"),
    };
    Ok(Some(Edit {
        range: source_range,
        text,
    }))
}

/// `None` delegates non-reference positions to the heading rename provider.
pub(super) fn request(
    method: &str,
    params: &Json,
    uri: &str,
    buffer: &Buffer,
    documents: &BTreeMap<String, Buffer>,
) -> Result<Option<Json>, Failure> {
    if !matches!(method, "textDocument/prepareRename" | "textDocument/rename") {
        return Ok(None);
    }
    let source = &buffer.text;
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "document exceeds the reference rename budget"));
    }
    if !buffer.synchronized {
        return Err((-32801, "document requires full-text resynchronization"));
    }
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let index = LineIndex::new(source);
    let offset = index
        .offset(source, requested)
        .map_err(|reason| (-32602, reason))?;
    let spans = reference_label_spans(source);
    let Some(selected) = spans.iter().find(|span| {
        span.label
            .as_ref()
            .is_some_and(|span| span.start <= offset && offset <= span.end)
    }) else {
        return Ok(None);
    };
    if spans.len() > MAX_SYMBOLS {
        return Err((-32803, "reference symbols exceed the rename budget"));
    }
    let key = &selected.key;
    let selected_range = selected
        .label
        .as_ref()
        .ok_or((-32803, "reference label has no exact source selection"))?;
    let selected_spans: Vec<_> = spans.iter().filter(|span| span.key == *key).collect();
    if selected_spans
        .iter()
        .any(|span| span.label.is_none() || span.replacement.is_none())
    {
        return Err((
            -32803,
            "reference label has noncontiguous source occurrences; rename cannot omit them",
        ));
    }
    if !selected_spans
        .iter()
        .any(|span| span.form == ReferenceLabelForm::Definition)
    {
        return Err((
            -32803,
            "reference definition has no complete source ownership",
        ));
    }
    if selected_spans.len() > MAX_EDITS {
        return Err((-32803, "reference rename exceeds its edit budget"));
    }
    if method == "textDocument/prepareRename" {
        return Ok(Some(object([
            ("range", range(&index, source, selected_range)),
            ("placeholder", string(&source[selected_range.clone()])),
        ])));
    }
    let name = params
        .get("newName")
        .and_then(Json::as_str)
        .ok_or((-32602, "rename requires string newName"))?;
    let renamed_key = new_key(name)?;
    if renamed_key != *key && spans.iter().any(|span| span.key == renamed_key) {
        return Err((
            -32803,
            "new reference label already exists in this document",
        ));
    }
    let mut edits = Vec::new();
    let mut edit_bytes = 0usize;
    for span in selected_spans {
        if let Some(edit) = edit_for(span, key, &renamed_key, name)?
            && source.get(edit.range.clone()) != Some(edit.text.as_str())
        {
            edit_bytes = edit_bytes.saturating_add(edit.text.len());
            if edit_bytes > MAX_EDIT_BYTES {
                return Err((
                    -32803,
                    "reference rename exceeds its replacement byte budget",
                ));
            }
            edits.push(edit);
        }
    }
    edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
    if edits
        .windows(2)
        .any(|pair| pair[0].range.end > pair[1].range.start)
    {
        return Err((-32803, "reference rename source ranges overlap"));
    }
    if edits.is_empty() {
        return Ok(Some(object([("documentChanges", Json::Array(Vec::new()))])));
    }
    let mut revised_len = source.len();
    for edit in &edits {
        revised_len = revised_len
            .checked_sub(edit.range.len())
            .and_then(|len| len.checked_add(edit.text.len()))
            .ok_or((-32803, "renamed document exceeds its byte budget"))?;
    }
    let other_bytes = documents
        .iter()
        .filter(|(name, _)| name.as_str() != uri)
        .try_fold(0usize, |sum, (_, buffer)| {
            sum.checked_add(buffer.text.len())
        })
        .ok_or((-32803, "workspace exceeds the reference rename budget"))?;
    if revised_len > MAX_DOCUMENT_BYTES
        || revised_len > MAX_SESSION_BYTES.saturating_sub(other_bytes)
    {
        return Err((
            -32803,
            "renamed document exceeds the document or session byte budget",
        ));
    }
    let mut revised = source.to_string();
    for edit in edits.iter().rev() {
        revised.replace_range(edit.range.clone(), &edit.text);
    }
    // This catches newly resolved formerly-literal references, altered code or
    // HTML, lost titles, image changes, and any incomplete source mapping.
    if parse_markdown(&revised) != parse_markdown(source) {
        return Err((
            -32803,
            "new reference label changes surrounding Markdown meaning",
        ));
    }
    let edits = edits
        .into_iter()
        .map(|edit| {
            object([
                ("range", range(&index, source, &edit.range)),
                ("newText", string(&edit.text)),
            ])
        })
        .collect();
    Ok(Some(object([(
        "documentChanges",
        Json::Array(vec![object([
            (
                "textDocument",
                object([
                    ("uri", string(uri)),
                    ("version", Json::Number(f64::from(buffer.version))),
                ]),
            ),
            ("edits", Json::Array(edits)),
        ])]),
    )])))
}

#[cfg(test)]
#[path = "reference_rename_tests.rs"]
mod tests;
