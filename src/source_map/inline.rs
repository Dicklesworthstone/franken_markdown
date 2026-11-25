//! Consume accepted parser tokens, never search decoded text in source syntax.

use std::collections::VecDeque;

use crate::parse::{InlineSourceAtom, InlineSourceKind, inline_source_capture};
use crate::span::{ProvenanceRelation, SourceSpan, SpannedDocument};

use super::SourceMapError;

pub(super) struct InlineSources {
    atoms: VecDeque<InlineSourceAtom>,
}

impl InlineSources {
    pub(super) fn capture(
        document: &SpannedDocument,
        source: &str,
    ) -> Result<Self, SourceMapError> {
        if document.source_len != source.len()
            || document
                .blocks
                .iter()
                .any(|block| block.span.slice(source).is_none())
        {
            return Err(SourceMapError::SourceMismatch);
        }

        // SpannedDocument predates authoring profiles and has no grammar field.
        // Prove the entire AST and its block spans against a supported grammar
        // before admitting any source token. Display-string equality alone is
        // not sufficient. This is structural agreement, not revision identity.
        for profile in [None, Some(crate::Profile::CommonMarkGfm)] {
            let (parsed, atoms) = inline_source_capture(source, profile);
            if same_blocks(&parsed, document) {
                return atoms
                    .map(|atoms| Self {
                        atoms: atoms.into(),
                    })
                    .ok_or(SourceMapError::InlineSourceUnavailable);
            }
        }
        Err(SourceMapError::SourceMismatch)
    }

    /// An AST Text leaf may contain several differently spelled source runs.
    /// Keep transforms atomic, while retaining the normal coarse literal runs
    /// instead of allocating a separate rendered element for every character.
    pub(super) fn take(
        &mut self,
        kind: InlineSourceKind,
        text: &str,
        block: SourceSpan,
        source: &str,
    ) -> Result<Vec<InlineSourceAtom>, SourceMapError> {
        let mut remaining = text;
        let mut accepted: Vec<InlineSourceAtom> = Vec::new();
        if text.is_empty()
            && !self
                .atoms
                .front()
                .is_some_and(|atom| atom.kind == kind && atom.text.is_empty())
        {
            return if kind == InlineSourceKind::Text {
                Ok(accepted)
            } else {
                Err(SourceMapError::SourceMismatch)
            };
        }
        loop {
            let mut atom = self
                .atoms
                .pop_front()
                .ok_or(SourceMapError::SourceMismatch)?;
            if atom.kind != kind
                || atom.end > block.end
                || atom.ranges.iter().any(|range| {
                    range.start < block.start
                        || range.end > block.end
                        || range.slice(source).is_none()
                })
            {
                return Err(SourceMapError::SourceMismatch);
            }
            if remaining.starts_with(&atom.text) {
                remaining = &remaining[atom.text.len()..];
            } else if kind == InlineSourceKind::Text
                && atom.text.starts_with(remaining)
                && atom.relation == ProvenanceRelation::Literal
                && atom.ranges.len() == 1
                && atom.ranges[0].slice(source) == Some(atom.text.as_str())
            {
                // A resolved inline boundary can divide a coalesced literal
                // token. Byte-identical text is the only case where clipping
                // a range is justified without another parser-owned boundary.
                let suffix = atom.text.split_off(remaining.len());
                let split = atom.ranges[0].start + remaining.len();
                let tail = InlineSourceAtom {
                    kind,
                    text: suffix,
                    ranges: vec![SourceSpan::new(split, atom.ranges[0].end)],
                    relation: ProvenanceRelation::Literal,
                    end: atom.end,
                };
                atom.ranges[0].end = split;
                atom.end = split;
                self.atoms.push_front(tail);
                remaining = "";
            } else {
                return Err(SourceMapError::SourceMismatch);
            }
            if kind == InlineSourceKind::Text
                && atom.relation == ProvenanceRelation::Literal
                && atom.ranges.len() == 1
                && let Some(previous) = accepted.last_mut()
                && previous.relation == ProvenanceRelation::Literal
                && previous.ranges.len() == 1
                && previous.ranges[0].end == atom.ranges[0].start
            {
                // An unresolved delimiter can remain a separate parser token
                // inside one Text leaf. Adjacent literal bytes still form one
                // exact source run; transformed tokens remain indivisible.
                previous.text.push_str(&atom.text);
                previous.ranges[0].end = atom.ranges[0].end;
                previous.end = previous.end.max(atom.end);
            } else {
                accepted.push(atom);
            }
            if remaining.is_empty() {
                return Ok(accepted);
            }
            if kind != InlineSourceKind::Text {
                return Err(SourceMapError::SourceMismatch);
            }
        }
    }

    pub(super) fn finish(&self) -> Result<(), SourceMapError> {
        if self.atoms.is_empty() {
            Ok(())
        } else {
            Err(SourceMapError::SourceMismatch)
        }
    }
}

fn same_blocks(parsed: &SpannedDocument, supplied: &SpannedDocument) -> bool {
    if parsed.source_len != supplied.source_len
        || !parsed
            .blocks
            .iter()
            .map(|block| block.span)
            .eq(supplied.blocks.iter().map(|block| block.span))
    {
        return false;
    }
    if parsed.blocks == supplied.blocks {
        return true;
    }
    // Hosts may already have promoted authored page-break comments. That
    // supported output transform leaves the original source spans intact.
    crate::apply_profile(&parsed.to_document(), Some(crate::Profile::GfmPlus))
        .blocks
        .iter()
        .eq(supplied.blocks.iter().map(|block| &block.node))
}
