//! Parser-owned completion of footnote and explicit reference-link labels.
//!
//! Lexical brackets only nominate a position. Footnote navigation's inventory
//! proves note ownership; parser-owned reference and destination tokens prove
//! reference suffixes. Probes preserve existing label text and delimiters, and
//! their number is independent of the number of offered definitions.

use std::collections::BTreeSet;
use std::ops::Range;

use franken_markdown::SourceSpan;
use franken_markdown::parse::{
    ReferenceLabelForm, ReferenceLabelSpan, destination_spans, reference_label_spans,
    split_frontmatter,
};

use super::{
    Json, LineIndex, MAX_DOCUMENT_BYTES, Position, footnotes, number, object, position, string,
};

type Failure = (i32, &'static str);
const MAX_LABEL_BYTES: usize = 4096;
const MAX_SYMBOLS: usize = 8192;
const MAX_COMPLETIONS: usize = 256;
const MAX_PROBE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Footnote,
    Reference,
}

struct Token {
    kind: Kind,
    open: usize,
    label: Range<usize>,
    closed: bool,
}

struct Label {
    spelling: String,
    key: String,
}

fn escaped(source: &str, at: usize) -> bool {
    source.as_bytes()[..at]
        .iter()
        .rev()
        .take_while(|&&byte| byte == b'\\')
        .count()
        % 2
        != 0
}

fn candidate(source: &str, offset: usize) -> Option<Token> {
    let line_start = source[..offset].rfind(['\r', '\n']).map_or(0, |at| at + 1);
    let mut start = offset.saturating_sub(MAX_LABEL_BYTES + 2).max(line_start);
    while !source.is_char_boundary(start) {
        start += 1;
    }
    let open = source[start..offset]
        .char_indices()
        .rev()
        .find_map(|(at, ch)| (ch == '[' && !escaped(source, start + at)).then_some(start + at))?;
    let kind = if source.as_bytes().get(open + 1) == Some(&b'^') {
        Kind::Footnote
    } else if open > 0 && source.as_bytes()[open - 1] == b']' && !escaped(source, open - 1) {
        Kind::Reference
    } else {
        return None;
    };
    let label_start = open + if kind == Kind::Footnote { 2 } else { 1 };
    if offset < label_start {
        return None;
    }
    let line_end = source[offset..]
        .find(['\r', '\n'])
        .map_or(source.len(), |at| offset + at);
    let mut end = line_end;
    let mut closed = false;
    for (at, ch) in source[label_start..line_end].char_indices() {
        let at = label_start + at;
        if at - label_start > MAX_LABEL_BYTES {
            return None;
        }
        if ch == ']' && (kind == Kind::Footnote || !escaped(source, at)) {
            end = at;
            closed = true;
            break;
        }
        if ch == '[' && (kind == Kind::Footnote || !escaped(source, at)) {
            return None;
        }
    }
    if offset > end {
        return None;
    }
    // An unfinished marker owns the word under the caret, not following prose.
    // Closed reference labels can contain spaces; unfinished multiword prefixes
    // are retained, but text after the next whitespace stays outside the edit.
    if !closed {
        end = source[offset..end]
            .char_indices()
            .find_map(|(at, ch)| ch.is_whitespace().then_some(offset + at))
            .unwrap_or(end);
    }
    let label = source.get(label_start..end)?;
    if label.len() > MAX_LABEL_BYTES
        || label.chars().any(char::is_control)
        || kind == Kind::Footnote && (label.len() > 128 || label.chars().any(char::is_whitespace))
    {
        return None;
    }
    Some(Token {
        kind,
        open,
        label: label_start..end,
        closed,
    })
}

fn list(items: Vec<Json>, incomplete: bool) -> Json {
    object([
        ("isIncomplete", Json::Bool(incomplete)),
        ("items", Json::Array(items)),
    ])
}

fn range(index: &LineIndex, source: &str, span: &Range<usize>) -> Json {
    object([
        ("start", position(index.position(source, span.start))),
        ("end", position(index.position(source, span.end))),
    ])
}

fn reference_key(label: &str) -> Option<String> {
    if label.trim().is_empty() {
        return Some(String::new());
    }
    // Ask the real parser for its pinned Unicode folding and whitespace rules.
    let probe = format!("[{label}]: /fmd-label-prefix\n");
    let spans = reference_label_spans(&probe);
    let [span] = spans.as_slice() else {
        return None;
    };
    (span.form == ReferenceLabelForm::Definition && span.label == Some(1..1 + label.len()))
        .then(|| span.key.clone())
}

fn prefix_key(prefix: &str) -> Option<String> {
    let mut key = reference_key(prefix)?;
    if !key.is_empty() && prefix.chars().next_back().is_some_and(char::is_whitespace) {
        key.push(' ');
    }
    Some(key)
}

fn reserve_label(source: &str) -> Result<String, Failure> {
    let folded = source.to_ascii_lowercase();
    (0..32)
        .map(|attempt| format!("fmd-lsp-label-{attempt}"))
        .find(|label| !folded.contains(label.as_str()))
        .ok_or((-32803, "cannot reserve a bounded completion probe"))
}

fn note_labels(source: &str, token: &Token) -> Result<Option<Vec<Label>>, Failure> {
    let raw = &source[token.label.clone()];
    let name = if raw.is_empty() {
        reserve_label(source)?
    } else {
        raw.to_string()
    };
    let mut amended = String::with_capacity(source.len() + name.len() + 1);
    amended.push_str(&source[..token.label.start]);
    amended.push_str(&name);
    if !token.closed {
        amended.push(']');
    }
    amended.push_str(&source[token.label.end..]);
    if amended.len() > MAX_PROBE_BYTES {
        return Err((-32803, "footnote completion probe exceeds its byte budget"));
    }
    footnotes::completion_labels(
        &amended,
        SourceSpan::new(token.label.start, token.label.start + name.len()),
    )
    .map(|labels| {
        labels.map(|labels| {
            labels
                .into_iter()
                .map(|spelling| Label {
                    key: spelling.clone(),
                    spelling,
                })
                .collect()
        })
    })
}

fn selected_use(span: &ReferenceLabelSpan, token: &Token) -> bool {
    match span.form {
        ReferenceLabelForm::Full => span.label.as_ref() == Some(&token.label),
        ReferenceLabelForm::Collapsed => {
            token.closed && span.replacement == Some(token.open..token.label.end + 1)
        }
        ReferenceLabelForm::Shortcut => span.replacement == Some(token.open..token.open),
        ReferenceLabelForm::Definition => false,
    }
}

fn shifted(
    range: &Range<usize>,
    token: &Token,
    body: usize,
    prefix: usize,
    replacement: usize,
) -> Option<Range<usize>> {
    let range = if range.end <= token.label.start {
        range.clone()
    } else if range.start >= token.label.end {
        range.start - token.label.len() + replacement..range.end - token.label.len() + replacement
    } else {
        return None;
    };
    if range.start >= body {
        Some(range.start + prefix..range.end + prefix)
    } else if range.end <= body {
        Some(range)
    } else {
        None
    }
}

fn coordinates(range: &Option<Range<usize>>) -> Option<(usize, usize)> {
    range.as_ref().map(|range| (range.start, range.end))
}

fn prove_reference(
    source: &str,
    token: &Token,
    spans: &[ReferenceLabelSpan],
    body: usize,
) -> Result<bool, Failure> {
    let raw = &source[token.label.clone()];
    let name = if raw.trim().is_empty() {
        reserve_label(source)?
    } else {
        raw.to_string()
    };
    let Some(key) = reference_key(&name) else {
        return Ok(false);
    };
    // Insert after actual frontmatter, before any source block. Appending to the
    // end could put the definition inside an unclosed fence or HTML block.
    let prefix = format!("[{name}]: /fmd-label-completion\n\n");
    let replacement = name.len() + usize::from(!token.closed);
    let mut amended = String::with_capacity(source.len() + prefix.len() + replacement);
    amended.push_str(&source[..body]);
    amended.push_str(&prefix);
    amended.push_str(&source[body..token.label.start]);
    amended.push_str(&name);
    if !token.closed {
        amended.push(']');
    }
    amended.push_str(&source[token.label.end..]);
    if amended.len() > MAX_PROBE_BYTES {
        return Err((-32803, "reference completion probe exceeds its byte budget"));
    }
    let probed = reference_label_spans(&amended);
    if probed.len() > MAX_SYMBOLS + 2 {
        return Err((-32803, "reference completion symbols exceed their budget"));
    }
    let selected = token.label.start + prefix.len()..token.label.start + prefix.len() + name.len();
    if !probed.iter().any(|span| {
        span.form == ReferenceLabelForm::Full
            && span.label.as_ref() == Some(&selected)
            && span.key == key
    }) {
        return Ok(false);
    }
    // Resolving a nested label must not deactivate an existing outer link.
    // Retain every other parser-owned reference use and definition, including
    // image references, rather than trusting bracket-shaped text alone.
    let actual: BTreeSet<_> = probed
        .iter()
        .map(|span| {
            (
                span.form,
                span.key.as_str(),
                coordinates(&span.label),
                coordinates(&span.replacement),
            )
        })
        .collect();
    for span in spans.iter().filter(|span| !selected_use(span, token)) {
        let transform = |range: &Option<Range<usize>>| -> Option<Option<(usize, usize)>> {
            range.as_ref().map_or(Some(None), |range| {
                shifted(range, token, body, prefix.len(), replacement)
                    .map(|range| Some((range.start, range.end)))
            })
        };
        let (Some(label), Some(replace)) = (transform(&span.label), transform(&span.replacement))
        else {
            return Ok(false);
        };
        if !actual.contains(&(span.form, span.key.as_str(), label, replace)) {
            return Ok(false);
        }
    }
    let original_destinations = destination_spans(source);
    let probed_destinations = destination_spans(&amended);
    if original_destinations.len() > MAX_SYMBOLS || probed_destinations.len() > MAX_SYMBOLS + 1 {
        return Err((
            -32803,
            "reference completion destinations exceed their budget",
        ));
    }
    if probed_destinations.len() != original_destinations.len() + 1 {
        return Ok(false);
    }
    let destinations: BTreeSet<_> = probed_destinations
        .iter()
        .map(|span| (span.dest.as_str(), span.range.start, span.range.end))
        .collect();
    for span in original_destinations {
        let Some(range) = shifted(&span.range, token, body, prefix.len(), replacement) else {
            return Ok(false);
        };
        if !destinations.contains(&(span.dest.as_str(), range.start, range.end)) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// None delegates unrelated positions to the existing fragment-completion route.
pub(super) fn request(params: &Json, source: &str) -> Result<Option<Json>, Failure> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err((-32803, "document exceeds the label completion budget"));
    }
    let requested = Position::parse(params.get("position").ok_or((-32602, "missing position"))?)
        .map_err(|reason| (-32602, reason))?;
    let index = LineIndex::new(source);
    let offset = index
        .offset(source, requested)
        .map_err(|reason| (-32602, reason))?;
    let Some(token) = candidate(source, offset) else {
        return Ok(None);
    };
    let (frontmatter, body_source) = split_frontmatter(source);
    let body = if frontmatter.is_some() {
        source.len() - body_source.len()
    } else {
        usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8()
    };
    if token.open < body {
        return Ok(Some(list(Vec::new(), false)));
    }
    let typed = &source[token.label.start..offset];
    let (labels, prefix, mut incomplete) = if token.kind == Kind::Footnote {
        let Some(labels) = note_labels(source, &token)? else {
            return Ok(Some(list(Vec::new(), false)));
        };
        (labels, typed.to_string(), false)
    } else {
        let spans = reference_label_spans(source);
        if spans.len() > MAX_SYMBOLS {
            return Err((-32803, "reference completion symbols exceed their budget"));
        }
        let Some(prefix) = prefix_key(typed) else {
            return Ok(Some(list(Vec::new(), false)));
        };
        let mut seen = BTreeSet::new();
        let mut labels = Vec::new();
        let mut incomplete = false;
        for span in spans
            .iter()
            .filter(|span| span.form == ReferenceLabelForm::Definition)
        {
            if !span.key.starts_with(&prefix) || !seen.insert(span.key.clone()) {
                continue;
            }
            let Some(spelling) = span
                .label
                .as_ref()
                .and_then(|range| source.get(range.clone()))
            else {
                incomplete = true;
                continue;
            };
            if spelling.len() > MAX_LABEL_BYTES || spelling.chars().any(char::is_control) {
                incomplete = true;
                continue;
            }
            labels.push(Label {
                spelling: spelling.to_string(),
                key: span.key.clone(),
            });
        }
        if labels.is_empty() {
            return Ok(Some(list(Vec::new(), incomplete)));
        }
        if !prove_reference(source, &token, &spans, body)? {
            return Ok(Some(list(Vec::new(), false)));
        }
        (labels, prefix, incomplete)
    };
    let edit_range = range(&index, source, &token.label);
    let mut items = Vec::new();
    for label in labels
        .into_iter()
        .filter(|label| label.key.starts_with(&prefix))
    {
        if items.len() == MAX_COMPLETIONS {
            incomplete = true;
            break;
        }
        let display = if token.kind == Kind::Footnote {
            format!("[^{}]", label.spelling)
        } else {
            label.spelling.clone()
        };
        let insertion = format!("{}{}", label.spelling, if token.closed { "" } else { "]" });
        items.push(object([
            ("label", string(&display)),
            ("kind", number(18)),
            (
                "detail",
                string(if token.kind == Kind::Footnote {
                    "Footnote definition"
                } else {
                    "Reference definition"
                }),
            ),
            ("sortText", string(&format!("{:04}", items.len()))),
            (
                "filterText",
                string(&format!("{}{}", typed, &label.key[prefix.len()..])),
            ),
            ("insertTextFormat", number(1)),
            (
                "textEdit",
                object([
                    ("range", edit_range.clone()),
                    ("newText", string(&insertion)),
                ]),
            ),
        ]));
    }
    Ok(Some(list(items, incomplete)))
}

#[cfg(test)]
#[path = "label_completion_tests.rs"]
mod tests;
