//! Live cross-document diagnostics over the client's explicitly opened buffers.
//! Sources are parsed once per accepted revision, never once per inbound link.
//! Every publication replaces the whole local + workspace diagnostic set.

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::SourceSpan;
use franken_markdown::book::validation::{DocumentLinkAnalysis, ReferenceKind};

use super::diagnostics::{self, MAX_DIAGNOSTICS, Snapshot};
use super::{Buffer, Json, LineIndex, workspace_links};

type Issue = (&'static str, &'static str);
const MAX_REFERENCE_WORK: usize = 32_768;
const INCOMPLETE: Issue = (
    "workspace_link_analysis_incomplete",
    "Cross-document link analysis could not complete within its limits; unopened files and external URLs are not checked.",
);
const UNSYNCHRONIZED: Issue = (
    "workspace_target_unsynchronized",
    "The opened target requires full-text resynchronization; its anchors cannot be verified against stale text.",
);

struct Entry {
    snapshot: Snapshot,
    // None means this version has not been published. Local findings are
    // immutable within a version, so only workspace additions need comparison.
    sent: Option<Vec<Json>>,
}

#[derive(Default)]
pub(super) struct Workspace {
    entries: BTreeMap<String, Entry>,
    #[cfg(test)]
    rebuilds: usize,
}

impl Workspace {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Reconcile open/change/close and rejected-edit transitions. Buffer text
    /// remains authoritative. Missing or unsynchronized records are discarded
    /// before any dependent can inspect the old anchor set.
    pub fn refresh(
        &mut self,
        documents: &BTreeMap<String, Buffer>,
        changed: Option<&str>,
    ) -> Vec<Json> {
        self.entries.retain(|uri, _| {
            documents.get(uri).is_some_and(|buffer| buffer.synchronized)
        });
        for (uri, buffer) in documents {
            if !buffer.synchronized {
                continue;
            }
            if self.entries.get(uri).is_some_and(|entry| {
                entry.snapshot.version == buffer.version
            }) {
                continue;
            }
            self.entries.insert(uri.clone(), Entry {
                snapshot: diagnostics::snapshot(buffer),
                sent: None,
            });
            #[cfg(test)]
            { self.rebuilds += 1; }
        }
        // Admission is global and deterministic: do not arbitrarily omit the
        // final URI's edges while implying earlier reports are complete.
        let work: usize = self.entries.values().filter_map(|entry| {
            entry.snapshot.links.as_ref().map(|links| links.references.len())
        }).sum();
        let over_budget = work > MAX_REFERENCE_WORK;
        let updates: Vec<_> = self.entries.iter().filter_map(|(uri, entry)| {
            let buffer = documents.get(uri)?;
            let extra = additions(uri, buffer, &entry.snapshot, documents, &self.entries, over_budget);
            (entry.sent.as_ref() != Some(&extra)).then_some((uri.clone(), extra))
        }).collect();
        let mut primary = Vec::new();
        let mut others = Vec::new();
        for (uri, extra) in updates {
            let (Some(entry), Some(buffer)) = (self.entries.get_mut(&uri), documents.get(&uri)) else {
                continue;
            };
            let mut findings = entry.snapshot.local.clone();
            findings.extend(extra.iter().cloned());
            entry.sent = Some(extra);
            let report = diagnostics::publication(&uri, buffer, findings);
            if changed == Some(uri.as_str()) {
                primary.push(report);
            } else {
                others.push(report);
            }
        }
        // Keep the changed document's report first, preserving the existing
        // single-document notification contract. Other updates use URI order.
        primary.extend(others);
        primary
    }
}

fn has_truncation(snapshot: &Snapshot) -> bool {
    snapshot.local.iter().any(|finding| {
        finding.get("code").and_then(Json::as_str) == Some("diagnostics_truncated")
    })
}

fn addressed_fragment(destination: &str) -> Option<(&str, &str)> {
    let destination =
        destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    if destination.starts_with(['#', '?']) {
        return None;
    }
    let (path_and_query, fragment) = destination.split_once('#')?;
    if fragment.is_empty() {
        return None;
    }
    let path = path_and_query.split('?').next()?;
    (!path.is_empty()).then_some((path, fragment))
}

fn target<'a>(
    origin: &str,
    path: &str,
    documents: &BTreeMap<String, Buffer>,
    entries: &'a BTreeMap<String, Entry>,
) -> Result<Option<&'a DocumentLinkAnalysis>, Issue> {
    match workspace_links::resolve(documents, origin, path) {
        // Absent and ambiguous identities are unchecked, not missing files.
        Ok(None) => Ok(None),
        Err(_) => Err(UNSYNCHRONIZED),
        Ok(Some((uri, _))) => entries.get(uri)
            .and_then(|entry| entry.snapshot.links.as_ref())
            .map(Some)
            .ok_or(INCOMPLETE),
    }
}

fn issue_for(analysis: &DocumentLinkAnalysis, destination: &str) -> Option<Issue> {
    let (code, message) = analysis.resolve_fragment(destination).err()?;
    Some((match code {
        "missing_anchor" => "workspace_missing_anchor",
        "invalid_fragment" => "workspace_invalid_fragment",
        "ambiguous_anchor" => "workspace_ambiguous_anchor",
        _ => return Some(INCOMPLETE),
    }, message))
}

fn additions(
    uri: &str,
    buffer: &Buffer,
    snapshot: &Snapshot,
    documents: &BTreeMap<String, Buffer>,
    entries: &BTreeMap<String, Entry>,
    over_budget: bool,
) -> Vec<Json> {
    if has_truncation(snapshot) {
        return Vec::new();
    }
    let source = buffer.text.as_str();
    let index = LineIndex::new(source);
    let incomplete = || vec![diagnostics::finding(
        source, &index, SourceSpan::new(0, 0), 2, INCOMPLETE.1, Some(INCOMPLETE.0),
    )];
    if !snapshot.inventory_complete {
        // Preserve a prior local-analysis refusal rather than multiplying it.
        return if snapshot.links.is_some() { incomplete() } else { Vec::new() };
    }
    let Some(links) = &snapshot.links else {
        return incomplete();
    };
    if over_budget {
        return if links.references.iter().any(|reference| {
            reference.kind == ReferenceKind::Link
                && addressed_fragment(&reference.destination).is_some()
        }) { incomplete() } else { Vec::new() };
    }
    let mut seen = BTreeSet::new();
    let mut targets = BTreeMap::new();
    let mut findings = Vec::new();
    for reference in &links.references {
        if reference.kind != ReferenceKind::Link {
            continue;
        }
        let destination = reference.destination
            .trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
        let Some((path, _)) = addressed_fragment(destination) else {
            continue;
        };
        if !seen.insert((reference.block_index, destination)) {
            continue;
        }
        // Resolve each distinct file path once for this source, regardless of
        // how many fragments or repeated uses address that file.
        let resolved = *targets.entry(path).or_insert_with(|| target(uri, path, documents, entries));
        let issue = match resolved {
            Ok(None) => None,
            Ok(Some(analysis)) => issue_for(analysis, destination),
            Err(issue) => Some(issue),
        };
        let Some((code, message)) = issue else { continue; };
        let Some(span) = snapshot.spans.get(reference.block_index).copied()
            .filter(|span| source.get(span.start..span.end).is_some())
        else {
            return incomplete();
        };
        let mut chars = destination.chars();
        let mut display: String = chars.by_ref().take(160).collect();
        if chars.next().is_some() { display.push('…'); }
        findings.push(diagnostics::finding(source, &index, span, 2,
            &format!("{message} Target: {display}. The range covers its enclosing Markdown block."),
            Some(code)));
        // Keep one extra only to trigger the shared truncation diagnostic.
        // Parser/local findings always have priority in the final envelope.
        if findings.len() > MAX_DIAGNOSTICS.saturating_sub(snapshot.local.len()) {
            break;
        }
    }
    findings
}

#[cfg(test)]
#[path = "workspace_diagnostics_tests.rs"]
mod tests;
