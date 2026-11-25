//! Opt-in accepted-inline provenance. The tokenizer owns recognition; these
//! records only retain its source coordinates and final readable characters.

use crate::span::{ProvenanceRelation, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineSourceKind {
    Text,
    Code,
    Image,
    Math,
    DisplayMath,
    Html,
    Footnote,
    SoftBreak,
    HardBreak,
}

#[derive(Debug, Clone)]
pub(crate) struct InlineSourceAtom {
    pub kind: InlineSourceKind,
    pub text: String,
    pub ranges: Vec<SourceSpan>,
    pub relation: ProvenanceRelation,
    /// End of the complete accepted token, including its non-reading syntax.
    pub end: usize,
}

const MAX_ATOMS: usize = 262_144;
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;

pub(super) struct InlineSourceCapture {
    pub atoms: Vec<InlineSourceAtom>,
    pub exhausted: bool,
    bytes: usize,
    merge_tail: bool,
}

impl InlineSourceCapture {
    pub fn new() -> Self {
        Self {
            atoms: Vec::new(),
            exhausted: false,
            bytes: 0,
            merge_tail: false,
        }
    }

    pub fn refuse(&mut self) {
        self.exhausted = true;
        self.atoms.clear();
    }

    /// The tokenizer visits literal characters one at a time. Grow the current
    /// run directly so opt-in source capture does not allocate a String and a
    /// range vector for every ordinary character.
    pub fn literal_chars(
        &mut self,
        chars: &[char],
        span: SourceSpan,
        merge: bool,
    ) -> Option<usize> {
        if self.exhausted {
            return None;
        }
        let bytes = chars.iter().map(|ch| ch.len_utf8()).sum::<usize>();
        if bytes > MAX_CAPTURE_BYTES.saturating_sub(self.bytes) {
            self.refuse();
            return None;
        }
        if merge
            && self.merge_tail
            && let Some(last) = self.atoms.last_mut()
            && last.kind == InlineSourceKind::Text
            && last.relation == ProvenanceRelation::Literal
            && last.ranges.len() == 1
            && last.ranges[0].end == span.start
            && last.end == span.start
        {
            self.bytes += bytes;
            last.text.extend(chars.iter());
            last.ranges[0].end = span.end;
            last.end = span.end;
            return Some(self.atoms.len() - 1);
        }
        self.push(
            InlineSourceAtom {
                kind: InlineSourceKind::Text,
                text: chars.iter().collect(),
                ranges: vec![span],
                relation: ProvenanceRelation::Literal,
                end: span.end,
            },
            merge,
        )
    }

    pub fn push(&mut self, atom: InlineSourceAtom, merge: bool) -> Option<usize> {
        if self.exhausted {
            return None;
        }
        let relation_bytes = match &atom.relation {
            ProvenanceRelation::Entity { raw_entity, .. } => raw_entity.len(),
            ProvenanceRelation::DecodedText { source, decoded } => source.len() + decoded.len(),
            ProvenanceRelation::StrippedDelimiter { delimiter } => delimiter.len(),
            _ => 0,
        };
        let bytes = atom.text.len().saturating_add(relation_bytes);
        if bytes > MAX_CAPTURE_BYTES.saturating_sub(self.bytes) {
            self.refuse();
            return None;
        }
        self.bytes += bytes;
        if merge
            && self.merge_tail
            && atom.kind == InlineSourceKind::Text
            && atom.relation == ProvenanceRelation::Literal
            && let Some(last) = self.atoms.last_mut()
            && last.kind == atom.kind
            && last.relation == ProvenanceRelation::Literal
            && last.ranges.len() == 1
            && atom.ranges.len() == 1
            && last.ranges[0].end == atom.ranges[0].start
            && last.end == atom.ranges[0].start
        {
            last.text.push_str(&atom.text);
            last.ranges[0].end = atom.ranges[0].end;
            last.end = atom.end;
            return Some(self.atoms.len() - 1);
        }
        if self.atoms.len() == MAX_ATOMS {
            self.refuse();
            return None;
        }
        let range_bytes = atom
            .ranges
            .len()
            .saturating_mul(std::mem::size_of::<SourceSpan>());
        if range_bytes > MAX_CAPTURE_BYTES.saturating_sub(self.bytes) {
            self.refuse();
            return None;
        }
        self.bytes += range_bytes;
        let index = self.atoms.len();
        self.merge_tail = merge;
        self.atoms.push(atom);
        Some(index)
    }

    pub fn rollback(&mut self, length: usize) {
        self.atoms.truncate(length);
        self.merge_tail = false;
    }

    pub fn checkpoint(&mut self) -> usize {
        self.merge_tail = false;
        self.atoms.len()
    }

    /// Only tokenizer-owned literal ASCII whitespace or backslash suffixes are
    /// removed. Entities/escaped characters can never become break syntax.
    pub fn trim_literal_suffix(&mut self, mut count: usize) {
        if self.exhausted {
            return;
        }
        while count > 0 {
            let Some(last) = self.atoms.last_mut() else {
                self.refuse();
                return;
            };
            if last.kind != InlineSourceKind::Text
                || last.relation != ProvenanceRelation::Literal
                || last.ranges.len() != 1
                || last.text.len() != last.ranges[0].len()
            {
                self.refuse();
                return;
            }
            let trim = count.min(last.text.len());
            let length = last.text.len() - trim;
            if !last.text.is_char_boundary(length) {
                self.refuse();
                return;
            }
            last.text.truncate(length);
            last.ranges[0].end -= trim;
            last.end -= trim;
            count -= trim;
            if last.text.is_empty() {
                self.atoms.pop();
            }
        }
    }

    pub fn consume_delimiter(&mut self, index: usize, count: usize, opener: bool) {
        if self.exhausted {
            return;
        }
        let Some(atom) = self.atoms.get_mut(index) else {
            self.refuse();
            return;
        };
        if atom.kind != InlineSourceKind::Text
            || atom.ranges.len() != 1
            || count > atom.text.len()
            || atom.ranges[0].len() != atom.text.len()
        {
            self.refuse();
            return;
        }
        atom.text.truncate(atom.text.len() - count);
        if opener {
            atom.ranges[0].end -= count;
        } else {
            atom.ranges[0].start += count;
        }
        // Keep the complete token end even when its remaining text is shorter.
    }
}
