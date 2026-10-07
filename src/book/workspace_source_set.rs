//! Complete source-set transactions for retained books. Parsing against the
//! complete new chapter map prevents stale cross-file bindings after a rename,
//! removal, promotion from include-only resource, or reading-order change.

use super::{BookInput, BookWorkspace, MAX_SOURCE_BYTES, MAX_SOURCES, add_bytes, invalid, paths};
use crate::Result;

/// Receipt for replacing the complete chapter/include capture. Unlike a text
/// update, this operation may change chapter membership and reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSourceSetUpdate {
    pub revision: u32,
    pub source_length: usize,
    pub chapter_count: usize,
    pub resource_count: usize,
    pub changed: bool,
    /// Changed captures rebuild the chapter map and parse every chapter. Exact
    /// normalized no-ops retain the previous AST and report zero parser work.
    pub reparsed_chapter_count: usize,
}

impl BookSourceSetUpdate {
    /// Bounded receipt with no source text, paths or host asset payloads.
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema\":\"fmd-book-source-set-v1\",\"revision\":{},\"source_length\":{},\"chapter_count\":{},\"resource_count\":{},\"changed\":{},\"reparsed_chapter_count\":{}}}",
            self.revision, self.source_length, self.chapter_count, self.resource_count,
            self.changed, self.reparsed_chapter_count,
        )
    }
}

impl BookWorkspace {
    /// Replace the entire ordered chapter list and include-only resource list
    /// at an expected revision, retaining all presentation settings and assets.
    /// Omitted sources are removed from this in-memory capture only. This never
    /// writes, renames, or deletes a host file and never rewrites source text.
    ///
    /// Expansion policy is fixed by the original constructor. Parse-only books
    /// reject include-only resources; expanding books validate the final graph
    /// as one unit, allowing sources and their include references to change
    /// together. All source, path, output-name and expansion checks finish
    /// before publication. Changed captures rebuild all chapter bindings using
    /// the ordinary constructor; use `update_sources_at_revision` for selective
    /// reparsing when membership and reading order have not changed.
    ///
    /// Exact normalized captures are no-ops, including at the maximum revision.
    /// Source-set edits and text edits share the same optimistic revision, so an
    /// outstanding text edit cannot accidentally target a replaced collection.
    ///
    /// # Errors
    /// Rejects stale/exhausted revisions, empty/oversized sets, invalid or
    /// colliding paths, and missing/cyclic/over-budget includes. Every failure
    /// preserves the last usable book, source capture, revision and assets.
    pub fn replace_sources_at_revision(
        &mut self,
        chapters: &[BookInput],
        resources: &[BookInput],
        expected_revision: u32,
    ) -> Result<BookSourceSetUpdate> {
        self.replace_set_with_limit(chapters, resources, expected_revision, MAX_SOURCE_BYTES)
    }

    fn replace_set_with_limit(
        &mut self,
        chapters: &[BookInput],
        resources: &[BookInput],
        expected_revision: u32,
        limit: usize,
    ) -> Result<BookSourceSetUpdate> {
        // Permission precedes source validation, allocation and parsing, even
        // for a no-op. A changed source set invalidates every older edit ticket.
        if expected_revision != self.revision {
            return Err(invalid(
                "stale source revision; refresh the source capture before editing",
            ));
        }
        if chapters.is_empty()
            || chapters.len() > MAX_SOURCES
            || resources.len() > MAX_SOURCES.saturating_sub(chapters.len())
        {
            return Err(invalid(
                "expected at least one chapter and at most 4096 total sources",
            ));
        }
        if self.expanded.is_none() && !resources.is_empty() {
            return Err(invalid("include-only sources require an expanding book"));
        }
        // Admit raw text AND paths before canonicalizing keys or cloning any
        // source. The final capture's budget is independent of the old capture.
        let mut total = 0;
        for source in chapters.iter().chain(resources) {
            add_bytes(&mut total, source.path.len(), limit)?;
            add_bytes(&mut total, source.source.len(), limit)?;
        }
        if same_capture(&self.chapters, chapters)? && same_capture(&self.resources, resources)? {
            return Ok(self.source_set_report(false));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("source revision exhausted; create a new workspace"))?;
        let mut next = if self.expanded.is_some() {
            Self::from_sources(chapters, resources)?
        } else {
            Self::new(chapters)?
        };
        next.revision = revision;
        let report = next.source_set_report(true);
        // Everything fallible has finished. Move, never clone, all font/image
        // payloads and metadata; old ASTs/captures retire only after success.
        std::mem::swap(self.renderer.options_mut(), next.renderer.options_mut());
        *self = next;
        Ok(report)
    }

    fn source_set_report(&self, changed: bool) -> BookSourceSetUpdate {
        BookSourceSetUpdate {
            revision: self.revision,
            source_length: self.source_length(),
            chapter_count: self.chapters.len(),
            resource_count: self.resources.len(),
            changed,
            reparsed_chapter_count: if changed { self.chapters.len() } else { 0 },
        }
    }
}

fn same_capture(old: &[BookInput], new: &[BookInput]) -> Result<bool> {
    if old.len() != new.len() {
        return Ok(false);
    }
    for (before, after) in old.iter().zip(new) {
        let path = paths::source_path(&after.path)
            .ok_or_else(|| invalid("invalid source-set path"))?;
        if before.path != path || before.source != after.source {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "workspace_source_set_tests.rs"]
mod tests;
