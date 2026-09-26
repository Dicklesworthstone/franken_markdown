//! Render-free navigation analysis for a single in-memory document.
//!
//! The parent module supplies the SAME admission, heading IDs, footnote queue,
//! fragment decoder and ambiguity rules used to validate published books.

use super::{Budget, LinkFinding, Reference, admit_blocks, invalid, navigation, resolve_fragment, scheme};
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

/// One parsed same-document link or footnote reference, in HTML emission order.
///
/// `destination` is the parser-decoded URL, or the footnote's logical ID.
/// `target_block_index` is absent for unresolved references and links to the
/// document root. `finding` distinguishes an invalid target from a root link.
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
    admit_blocks(&doc.blocks, 0, &mut Budget::default())?;
    let nav = navigation(doc);
    let mut references = Vec::new();
    let mut external = 0;
    let mut unchecked = 0;
    let mut finding_count = 0;
    let mut finding_bytes = 0;
    for reference in &nav.references {
        let (destination, block_index, kind, resolved) = match reference {
            Reference::Note(id, owner) => {
                let resolved = nav.definitions.get(id).map(|(_, index)| Some(*index))
                    .ok_or(("missing_footnote", "This parsed footnote reference has no definition in its chapter."));
                (*id, *owner, ReferenceKind::Footnote, resolved)
            }
            Reference::Link(destination, owner) => {
                let dest = destination.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
                if dest.starts_with("//") || scheme(dest) {
                    external += 1;
                    continue;
                }
                if !dest.is_empty() && !dest.starts_with(['#', '?']) {
                    unchecked += 1;
                    continue;
                }
                let resolved = resolve_fragment(&nav, dest).map(|anchor| anchor.map(|entry| entry.block_index));
                (*destination, *owner, ReferenceKind::Link, resolved)
            }
        };
        let (target_block_index, finding) = match resolved {
            Ok(target) => (target, None),
            Err((code, message)) => {
                if finding_count >= super::MAX_FINDINGS || destination.len() > 8192
                    || destination.len() > (256 * 1024usize).saturating_sub(finding_bytes)
                {
                    return Err(invalid("finding count or destination text exceeds the report limit"));
                }
                finding_count += 1;
                finding_bytes += destination.len();
                (None, Some(LinkFinding { code, destination: destination.to_string(), message }))
            }
        };
        references.push(DocumentReference {
            block_index, destination: destination.to_string(), kind, target_block_index, finding,
        });
    }
    Ok(DocumentLinkAnalysis {
        anchors: nav.anchors.into_values().collect(), references, external, unchecked,
    })
}
