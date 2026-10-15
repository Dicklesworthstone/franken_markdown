//! Cached fragment lookup against actual publication analysis, without adding
//! synthetic references that would change which footnotes the target emits.
#![allow(clippy::unwrap_used)]

use franken_markdown::book::validation::{
    AnchorKind, DocumentLinkAnalysis, ReferenceKind, analyze_document_links,
};
use franken_markdown::parse_markdown;

fn analyze(source: &str) -> DocumentLinkAnalysis {
    analyze_document_links(&parse_markdown(source)).unwrap()
}

#[test]
fn target_lookup_distinguishes_root_valid_and_missing_fragments() {
    let target = analyze("# Topic\n\n> ## Nested");
    for destination in ["", "guide.md", "?view=print", "guide.md?view=print#"] {
        assert_eq!(target.resolve_fragment(destination), Ok(None));
    }
    let heading = target.resolve_fragment("guide.md?view=print#topic").unwrap().unwrap();
    assert_eq!(heading.id, "topic");
    assert_eq!(heading.block_index, 0);
    assert_eq!(target.resolve_fragment("#nested").unwrap().unwrap().block_index, 1);
    assert_eq!(target.resolve_fragment("#absent").unwrap_err().0, "missing_anchor");
}

#[test]
fn unicode_fragments_are_decoded_once_with_no_case_folding() {
    let target = analyze("# Café");
    assert_eq!(target.resolve_fragment("#caf%C3%A9").unwrap().unwrap().id, "café");
    assert!(target.resolve_fragment("#café").unwrap().is_some());
    for destination in ["#caf%25C3%25A9", "#Caf%C3%A9"] {
        assert_eq!(target.resolve_fragment(destination).unwrap_err().0, "missing_anchor");
    }
}

#[test]
fn malformed_encoding_and_encoded_controls_are_not_missing_anchor_claims() {
    let target = analyze("# Topic");
    for destination in ["#%", "#%0", "#%gg", "#%ff", "#%C0%AF", "#to%00pic", "#topic%0A"] {
        assert_eq!(target.resolve_fragment(destination).unwrap_err().0, "invalid_fragment", "{destination}");
    }
}

#[test]
fn duplicate_headings_resolve_their_own_published_ids() {
    let target = analyze("# Same\n\n# Same");
    assert_eq!(target.anchors.len(), 2);
    assert_ne!(target.anchors[0].id, target.anchors[1].id);
    for anchor in &target.anchors {
        let resolved = target.resolve_fragment(&format!("#{}", anchor.id)).unwrap().unwrap();
        assert_eq!(resolved.block_index, anchor.block_index);
        assert_eq!(resolved.kind, AnchorKind::Heading);
    }
}

#[test]
fn cached_lookup_does_not_publish_unreferenced_notes() {
    let target = analyze("note[^shown]\n\n[^shown]: Published\n\n[^hidden]: Not published");
    let note = target.anchors.iter().find(|a| a.kind == AnchorKind::Footnote).unwrap();
    assert!(target.resolve_fragment(&format!("#{}", note.id)).unwrap().is_some());
    let before = target.clone();
    assert_eq!(target.resolve_fragment("#fn-hidden").unwrap_err().0, "missing_anchor");
    assert_eq!(target, before);
}

#[test]
fn cached_lookup_matches_the_original_publication_findings() {
    let target = analyze(
        "# Topic\n\n[ok](#topic) [missing](#absent) [bad](#%ff) [root](#)\n\n\
         note[^n]\n\n[^n]: [inside](#topic)",
    );
    for reference in &target.references {
        if reference.kind != ReferenceKind::Link {
            continue;
        }
        let actual = target.resolve_fragment(&reference.destination);
        match &reference.finding {
            Some(finding) => assert_eq!(actual.unwrap_err().0, finding.code),
            None => assert_eq!(actual.unwrap().map(|a| a.block_index), reference.target_block_index),
        }
    }
}

#[test]
fn ambiguous_emitted_ids_are_rejected_not_selected_arbitrarily() {
    // Publication can record collisions between independently emitted anchor
    // classes. Exercise that public representation directly, not a font/parser mock.
    let mut target = analyze("# Topic");
    target.anchors[0].occurrences = 2;
    assert_eq!(target.resolve_fragment("#topic").unwrap_err().0, "ambiguous_anchor");
}

#[test]
fn long_destination_refusal_is_bounded_and_does_not_change_inventory_behavior() {
    let target = analyze("# Topic");
    let limit = format!("{}#topic", "x".repeat(8192 - 6));
    assert!(target.resolve_fragment(&limit).unwrap().is_some());
    assert_eq!(target.resolve_fragment(&(limit + "x")).unwrap_err().0, "fragment_limit");
    let inventory = DocumentLinkAnalysis::with_file_references(&parse_markdown("[link](b.md#topic)")).unwrap();
    assert_eq!(inventory.references[0].target_block_index, None);
    assert_eq!(inventory.references[0].finding, None);
}
