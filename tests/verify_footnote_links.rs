//! Verification follows the source note aliases that standalone PDFs publish.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::verify::{VerifyReport, verify_pdf_result};
use franken_markdown::{PdfOptions, Profile, parse_markdown};

fn verify(source: &str, options: &PdfOptions) -> VerifyReport {
    verify_pdf_result(&parse_markdown(source), options).unwrap()
}

fn assert_links(report: &VerifyReport, resolved: usize, unresolved: &[&str]) {
    assert_eq!(report.anchors_resolved, resolved, "{report:?}");
    assert_eq!(report.anchors_unresolved, unresolved, "{report:?}");
    let findings: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.code == "unresolved_anchor")
        .collect();
    assert_eq!(findings.len(), unresolved.len(), "{findings:?}");
    for (finding, target) in findings.iter().zip(unresolved) {
        assert!(
            finding.detail.contains(&format!("#{target}")),
            "preserve the authored fragment in the finding: {finding:?}"
        );
    }
}

#[test]
fn public_note_and_return_fragments_resolve_with_and_without_generated_backlinks() {
    let source = "Claim.[^n] [Read the note](#fn-n) [Return to the claim](#fnref-1)\n\n\
                  [^n]: Supporting evidence.\n";
    let document = parse_markdown(source);
    let original = document.clone();
    for profile in [None, Some(Profile::CommonMarkGfm)] {
        for footnote_backlinks in [false, true] {
            let options = PdfOptions {
                profile,
                footnote_backlinks,
                ..Default::default()
            };
            let report = verify_pdf_result(&document, &options).unwrap();
            assert_links(&report, 3 + usize::from(footnote_backlinks), &[]);
            assert_eq!(report.verdict, "clean");
        }
    }
    assert_eq!(document, original);
}

#[test]
fn queried_unicode_aliases_use_citation_order_instead_of_definition_order() {
    let source = "[Read the second note](?edition=2#fn-%CE%B2) First.[^a] Second.[^β] \
                  [Read the first note](#fn-a) [Second citation](#fnref-2)\n\n\
                  [^unused]: Unreferenced definition first.\n\n\
                  [^β]: Second cited definition.\n\n\
                  [^a]: First cited definition last.\n";
    assert_links(&verify(source, &PdfOptions::default()), 5, &[]);
}

#[test]
fn safe_html_and_nested_note_cycles_share_public_note_targets() {
    let source = "Source.[^a] <a href='#fn-a'>Read first note</a> [Nested note](#fn-b)\n\n\
                  <a href='?edition=2&amp;view=notes#fn-b'>Read second note</a>\n\n\
                  [^a]: First note cites another.[^b]\n\n\
                  [^b]: Second note returns to the first.[^a]\n";
    assert_links(&verify(source, &PdfOptions::default()), 6, &[]);
}

#[test]
fn ambiguous_note_ids_remain_unresolved_even_when_a_heading_matches() {
    let source = "# fn-n\n\n## fnref-1\n\n\
                  Cited.[^n] [Ambiguous note](#fn-n) [Ambiguous return](#fnref-1) \
                  [Unreferenced note](#fn-unused) [Missing note](#fn-missing)\n\n\
                  ## fn-heading\n\n[Ordinary heading](#fn-heading)\n\n\
                  [^n]: Referenced note.\n\n[^unused]: Unreferenced note.\n";
    assert_links(
        &verify(source, &PdfOptions::default()),
        2,
        &["fn-n", "fnref-1", "fn-unused", "fn-missing"],
    );
}

#[test]
fn percent_fragments_decode_once_and_findings_keep_the_original_spelling() {
    let source = "Claim.[^a%2Fb] [Encoded note](#fn-a%252Fb) \
                  [Different ID](#fn-a%2Fb) [Different ID again](#fn-a%2Fb) \
                  [Malformed encoding](#fn-a%ZZ)\n\n[^a%2Fb]: Percent-bearing note.\n";
    assert_links(
        &verify(source, &PdfOptions::default()),
        2,
        &["fn-a%2Fb", "fn-a%ZZ"],
    );
}

#[test]
fn an_uncited_note_does_not_gain_a_public_anchor_from_an_ordinary_link() {
    let source = "[Unused note](#fn-unused) [Absent citation](#fnref-1)\n\n\
                  [^unused]: Retained as an endnote.\n";
    assert_links(
        &verify(source, &PdfOptions::default()),
        0,
        &["fn-unused", "fnref-1"],
    );
}

#[test]
fn a_note_inside_an_ordinary_link_has_no_rendered_citation_to_return_to() {
    let source = "[Linked claim[^n]](https://example.test) \
                  [Read the note](#fn-n) [Absent citation](#fnref-1)\n\n\
                  [^n]: A note whose mark is inside another link.\n";
    let options = PdfOptions {
        footnote_backlinks: true,
        ..Default::default()
    };
    assert_links(&verify(source, &options), 1, &["fnref-1"]);
}
