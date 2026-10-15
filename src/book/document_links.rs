//! Render-free navigation analysis for a single in-memory document.
//!
//! The parent module supplies the SAME admission, heading IDs, footnote queue,
//! fragment decoder and ambiguity rules used to validate published books.

use super::{
    Budget, LinkFinding, Reference, admit_blocks, invalid, navigation, resolve_fragment, scheme,
};
use crate::{Document, Result};

/// The emitted element addressed by an internal fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorKind {
    Heading,
    Footnote,
    FootnoteBackreference,
}

/// An emitted anchor and the top-level AST block containing its source.
///
/// `occurrences > 1` means the ID is ambiguous; consumers must not silently
/// navigate to the first occurrence. Nested nodes retain their enclosing
/// top-level block, not an invented precise source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentAnchor {
    pub id: String,
    pub block_index: usize,
    pub title: String,
    pub kind: AnchorKind,
    pub occurrences: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Link,
    Footnote,
}

/// One parsed link or footnote reference, in HTML emission order.
///
/// `destination` is the parser-decoded URL, or the footnote's logical ID.
/// `target_block_index` is absent for unresolved references and links to the
/// document root. For same-document references, `finding` distinguishes an
/// invalid target from a root link. With [`DocumentLinkAnalysis::with_file_references`],
/// file-addressed links have neither field set: their targets remain unchecked,
/// not validated roots. A host must resolve their destination against its own
/// explicitly authorized source set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentReference {
    pub block_index: usize,
    pub destination: String,
    pub kind: ReferenceKind,
    pub target_block_index: Option<usize>,
    pub finding: Option<LinkFinding>,
}

/// Complete, deterministic analysis with no rendering or resource access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLinkAnalysis {
    /// Sorted by ID. Only headings and referenced footnotes are emitted.
    pub anchors: Vec<DocumentAnchor>,
    pub references: Vec<DocumentReference>,
    /// Scheme-bearing and protocol-relative links, never requested.
    pub external: usize,
    /// File-addressed links, not checked without an explicitly supplied book.
    pub unchecked: usize,
}

impl DocumentLinkAnalysis {
    /// Resolve a destination's fragment against this exact analyzed document.
    ///
    /// A host must first select and authorize the target document. The path,
    /// query and scheme before `#` are deliberately NOT resolved or fetched.
    /// Missing/empty fragments address the document root (`Ok(None)`); other
    /// fragments use the book validator's single-pass percent/UTF-8 decoder.
    /// Only emitted, unambiguous anchors resolve, including referenced notes.
    /// No synthetic reference is inserted into the target document, and no
    /// parsing, rendering or font loading is repeated for subsequent lookups.
    ///
    /// Analyses produced by the public constructors have anchors sorted by ID;
    /// callers modifying the public `anchors` vector must preserve that order.
    ///
    /// # Errors
    /// Returns a stable `(code, message)` for `invalid_fragment`, `missing_anchor`,
    /// `ambiguous_anchor`, or `fragment_limit` (destination over 8192 bytes).
    /// Errors never allocate or echo a copy of the caller's destination.
    pub fn resolve_fragment(
        &self,
        destination: &str,
    ) -> std::result::Result<Option<&DocumentAnchor>, (&'static str, &'static str)> {
        if destination.len() > 8192 {
            return Err(("fragment_limit", "The link destination exceeds 8192 bytes."));
        }
        let destination =
            destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
        let Some((_, fragment)) = destination.split_once('#') else {
            return Ok(None);
        };
        if fragment.is_empty() {
            return Ok(None);
        }
        let id = super::decode(fragment).ok_or((
            "invalid_fragment",
            "This fragment is not valid percent-encoded UTF-8.",
        ))?;
        let index = self
            .anchors
            .binary_search_by(|anchor| anchor.id.as_str().cmp(id.as_str()))
            .map_err(|_| {
                (
                    "missing_anchor",
                    "The target chapter has no emitted heading or referenced-footnote anchor with this ID.",
                )
            })?;
        let anchor = &self.anchors[index];
        if anchor.occurrences != 1 {
            return Err((
                "ambiguous_anchor",
                "More than one emitted element uses this anchor; rename the colliding heading or note.",
            ));
        }
        Ok(Some(anchor))
    }

    /// Include emitted relative/absolute file links alongside internal navigation.
    ///
    /// This is a source-owned inventory for editors and host-authorized workspaces,
    /// not a filesystem checker. File destinations retain their parser-decoded
    /// spelling and enclosing block index. Both `target_block_index` and `finding`
    /// are `None` for these unchecked links; only the host can resolve them.
    /// Images, code, raw HTML, unreferenced notes and network URLs stay excluded.
    /// Explicit `file:` URLs are included but still counted as `external`, matching
    /// the original analysis counters. Existing same-document findings, anchor IDs,
    /// publication order and [`analyze_document_links`] behavior are unchanged.
    ///
    /// # Errors
    /// In addition to the ordinary analysis limits, rejects more than 16384
    /// inventoried references, any destination over 8192 bytes, or over 1 MiB of
    /// total destination text. Refusal never returns a partial inventory.
    pub fn with_file_references(doc: &Document) -> Result<Self> {
        analyze(doc, true)
    }
}

/// Analyze internal navigation without loading fonts, rendering pages, reading
/// files, or inventing a one-chapter filesystem workspace.
///
/// Forward/duplicate headings, referenced-note cycles, queries and encoded
/// fragments follow book/HTML publication. Code, raw HTML IDs and unreferenced
/// note bodies are not mined for references. File links are explicitly unchecked
/// rather than falsely reported missing when only an editor buffer is available.
/// Results borrow no AST memory and remain tied to this exact document revision.
///
/// # Errors
/// Uses the book validator's 250000-node, 128-depth and 64-MiB AST admission
/// limits. Findings are limited to 4096, 8192 destination bytes per finding,
/// and 256 KiB of finding destinations. A limit failure returns no partial report.
pub fn analyze_document_links(doc: &Document) -> Result<DocumentLinkAnalysis> {
    analyze(doc, false)
}

fn analyze(doc: &Document, include_files: bool) -> Result<DocumentLinkAnalysis> {
    admit_blocks(&doc.blocks, 0, &mut Budget::default())?;
    let nav = navigation(doc);
    let mut references = Vec::new();
    let mut external = 0;
    let mut unchecked = 0;
    let mut finding_count = 0;
    let mut finding_bytes = 0;
    let mut reference_bytes = 0;
    for reference in &nav.references {
        let (destination, block_index, kind, resolved) = match reference {
            Reference::Note(id, owner) => {
                let resolved = nav
                    .definitions
                    .get(id)
                    .map(|(_, index)| Some(*index))
                    .ok_or((
                        "missing_footnote",
                        "This parsed footnote reference has no definition in its chapter.",
                    ));
                (*id, *owner, ReferenceKind::Footnote, resolved)
            }
            Reference::Link(destination, owner) => {
                let dest =
                    destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
                let resolved = if dest.starts_with("//") || scheme(dest) {
                    external += 1;
                    if !include_files
                        || !dest
                            .get(..5)
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
                    {
                        continue;
                    }
                    Ok(None)
                } else if !dest.is_empty() && !dest.starts_with(['#', '?']) {
                    unchecked += 1;
                    if !include_files {
                        continue;
                    }
                    Ok(None)
                } else {
                    resolve_fragment(&nav, dest).map(|anchor| anchor.map(|entry| entry.block_index))
                };
                (*destination, *owner, ReferenceKind::Link, resolved)
            }
        };
        let (target_block_index, finding) = match resolved {
            Ok(target) => (target, None),
            Err((code, message)) => {
                if finding_count >= super::MAX_FINDINGS
                    || destination.len() > 8192
                    || destination.len() > (256 * 1024usize).saturating_sub(finding_bytes)
                {
                    return Err(invalid(
                        "finding count or destination text exceeds the report limit",
                    ));
                }
                finding_count += 1;
                finding_bytes += destination.len();
                (
                    None,
                    Some(LinkFinding {
                        code,
                        destination: destination.to_string(),
                        message,
                    }),
                )
            }
        };
        if include_files {
            if references.len() >= 16_384
                || destination.len() > 8192
                || destination.len() > (1024 * 1024usize).saturating_sub(reference_bytes)
            {
                return Err(invalid(
                    "reference inventory exceeds its count or text budget",
                ));
            }
            reference_bytes += destination.len();
        }
        references.push(DocumentReference {
            block_index,
            destination: destination.to_string(),
            kind,
            target_block_index,
            finding,
        });
    }
    Ok(DocumentLinkAnalysis {
        anchors: nav.anchors.into_values().collect(),
        references,
        external,
        unchecked,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod inventory_tests {
    use super::*;
    use crate::parse_markdown;

    #[test]
    fn file_inventory_preserves_existing_findings_anchors_and_counters() {
        let doc = parse_markdown(
            "# Here\n\n[ok](#here) [bad](#absent) [chapter](next.md#there) \
             [remote](https://example.com/#there) [file](FiLe:///docs/next.md#there)",
        );
        let ordinary = analyze_document_links(&doc).unwrap();
        let inventory = DocumentLinkAnalysis::with_file_references(&doc).unwrap();
        assert_eq!(ordinary.references.len(), 2);
        assert_eq!(inventory.references.len(), 4);
        assert_eq!(inventory.anchors, ordinary.anchors);
        assert_eq!(inventory.external, ordinary.external);
        assert_eq!(inventory.unchecked, ordinary.unchecked);
        assert_eq!(inventory.references[..2], ordinary.references);
        for reference in &inventory.references[2..] {
            assert_eq!(reference.kind, ReferenceKind::Link);
            assert_eq!(reference.target_block_index, None);
            assert_eq!(reference.finding, None);
        }
    }

    #[test]
    fn inventory_uses_the_publication_walk_and_parser_decoded_destinations() {
        let doc = parse_markdown(
            "# Top\n\n> [nested][ref]\n\n[ref]: guide.md?a=1&amp;b=2#top\n\n\
             ` [code](skip.md) ` ![asset](image.png)\n\n\
             ```md\n[fenced](skip.md)\n```\n\n\
             [^unused]: [hidden](skip.md)\n\n\
             [visible](guide.md#top)",
        );
        let inventory = DocumentLinkAnalysis::with_file_references(&doc).unwrap();
        let destinations: Vec<_> = inventory
            .references
            .iter()
            .map(|reference| reference.destination.as_str())
            .collect();
        assert_eq!(destinations, ["guide.md?a=1&b=2#top", "guide.md#top"]);
        assert_eq!(inventory.references[0].block_index, 1);
        assert!(inventory.references[1].block_index > 1);
    }

    #[test]
    fn inventory_keeps_repeated_references_and_published_footnotes() {
        let doc = parse_markdown(
            "# Here\n\n[a](next.md#there) [b](next.md#there) note[^n]\n\n\
             [^n]: [inside](next.md#there)",
        );
        let inventory = DocumentLinkAnalysis::with_file_references(&doc).unwrap();
        assert_eq!(inventory.references.len(), 4);
        assert_eq!(
            inventory.references[0].block_index,
            inventory.references[1].block_index
        );
        assert_eq!(inventory.references[2].kind, ReferenceKind::Footnote);
        assert!(inventory.references[2].target_block_index.is_some());
        assert_eq!(inventory.references[3].destination, "next.md#there");
    }

    #[test]
    fn network_schemes_and_protocol_relative_urls_never_enter_the_inventory() {
        let doc = parse_markdown(
            "[a](https://example.com) [b](//example.com/guide.md) \
             [c](mailto:me@example.com) [d](javascript:alert) [e](file:///guide.md)",
        );
        let inventory = DocumentLinkAnalysis::with_file_references(&doc).unwrap();
        assert_eq!(inventory.references.len(), 1);
        assert_eq!(inventory.references[0].destination, "file:///guide.md");
        assert_eq!(inventory.external, 5);
    }

    #[test]
    fn inventory_rejects_excessive_destination_text_without_changing_ordinary_analysis() {
        let doc = parse_markdown(&format!("[x]({}.md)", "a".repeat(8192)));
        assert!(analyze_document_links(&doc).is_ok());
        assert!(DocumentLinkAnalysis::with_file_references(&doc).is_err());
        let link = format!("[x]({}.md)\n\n", "a".repeat(5000));
        let doc = parse_markdown(&link.repeat(210));
        assert!(analyze_document_links(&doc).is_ok());
        assert!(DocumentLinkAnalysis::with_file_references(&doc).is_err());
    }

    #[test]
    fn inventory_rejects_excessive_reference_count_instead_of_truncating() {
        let doc = parse_markdown(&"[x](next.md)\n\n".repeat(16_385));
        assert!(analyze_document_links(&doc).is_ok());
        assert!(DocumentLinkAnalysis::with_file_references(&doc).is_err());
    }
}
