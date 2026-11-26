//! Preserve authored heading and note targets when safe HTML adds export-only headings.
//!
//! Generated EPUB navigation addresses the lowered document directly. Authored
//! links instead name the original document's anchors. The lowering sidecar
//! connects these two inventories without matching titles or searching HTML.

use std::collections::BTreeMap;

use crate::{Document, RenderError, Result};

#[derive(Default)]
pub(super) struct HeadingLinks(BTreeMap<String, String>);

impl HeadingLinks {
    pub(super) fn for_document(source: &Document) -> Result<Self> {
        let (lowered, origins) = crate::safe_html::lower_with_heading_origins(source);
        Self::from_lowered(source, &lowered, origins.as_deref())
    }

    pub(super) fn from_lowered(
        source: &Document,
        lowered: &Document,
        origins: Option<&[Option<usize>]>,
    ) -> Result<Self> {
        let original = crate::html::heading_identifiers(source);
        let mut changed = BTreeMap::new();
        if let Some(origins) = origins {
            let emitted = crate::html::heading_identifiers(lowered);
            if origins.len() != emitted.headings.len() {
                return Err(invalid());
            }
            for (origin, target) in origins.iter().zip(emitted.headings) {
                let Some(ordinal) = origin else {
                    continue;
                };
                let Some(original_id) = original.headings.get(*ordinal) else {
                    return Err(invalid());
                };
                let (Some(original_id), Some(target)) = (original_id, target) else {
                    // An unreferenced/duplicate note heading has no emitted anchor.
                    continue;
                };
                if *original_id != target
                    && !original.other_anchors.contains(original_id)
                    && !emitted.other_anchors.contains(&target)
                {
                    // A source ID shared with a note/backreference is ambiguous.
                    // Never turn that ambiguity into a silently selected heading.
                    changed.insert(original_id.clone(), target);
                }
            }
            // Added headings can also occupy a note's already-disambiguated source
            // ID. Match those targets by their source note identity, never by their
            // numeric spelling or by rendered text, so a link to a note cannot be
            // captured by an export-only heading (or a different note).
            let note_targets: BTreeMap<_, _> = emitted
                .footnotes
                .iter()
                .map(|note| (note.source_id.as_str(), note))
                .collect();
            for note in original.footnotes {
                if let Some(target) = note_targets.get(note.source_id.as_str()) {
                    if note.body != target.body {
                        changed.insert(note.body, target.body.clone());
                    }
                    if note.reference != target.reference {
                        changed.insert(note.reference, target.reference.clone());
                    }
                }
            }
        }
        // Resolve authored aliases through the original inventory before any
        // export-only HTML headings consume IDs. Preserve its exact-ID and
        // note collision decisions, then follow the canonical lowering map.
        for (alias, canonical) in original.aliases.entries() {
            let target = changed
                .get(canonical)
                .cloned()
                .unwrap_or_else(|| canonical.to_string());
            changed.entry(alias.to_string()).or_insert(target);
        }
        Ok(Self(changed))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(super) fn byte_len(&self) -> Option<usize> {
        self.0.iter().try_fold(0usize, |bytes, (source, target)| {
            bytes.checked_add(source.len())?.checked_add(target.len())
        })
    }

    /// Fragments are case-sensitive and decoded exactly once. The query and
    /// everything preceding '#' remain opaque; unchanged/invalid fragments
    /// retain their original spelling. Generated IDs need no extra escaping
    /// here: the ordinary link renderer owns URL/attribute serialization.
    pub(super) fn rewrite(&self, destination: &str) -> Option<String> {
        if self.0.is_empty() {
            return None;
        }
        let (prefix, fragment) = destination.split_once('#')?;
        let id = crate::anchors::decode_fragment(fragment)?;
        let target = self.0.get(&id)?;
        Some(format!("{prefix}#{target}"))
    }

    pub(super) fn rewrite_local(&self, destination: &mut String) {
        let trimmed = destination.trim();
        if trimmed.starts_with(['#', '?'])
            && let Some(rewritten) = self.rewrite(trimmed)
        {
            *destination = rewritten;
        }
    }
}

fn invalid() -> RenderError {
    RenderError::InvalidInput("epub: heading provenance does not match lowered document".into())
}
