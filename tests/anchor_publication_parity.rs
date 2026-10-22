//! Navigation must address the IDs that the exported document actually emits.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::book::validation::analyze_document_links;
use franken_markdown::doc_stats::compute_doc_stats;
use franken_markdown::search_index::{EntryKind, build_search_index};
use franken_markdown::{HtmlOptions, PdfOptions, html, parse_markdown, render_pdf};

#[test]
fn unicode_and_inline_markup_share_ids_across_publication_and_document_intelligence() {
    let source = "# Café\n\n# Café\n\n# Café-2\n\n# <em>Café</em>\n\n# 中文 Guide\n\n\
        [first](#caf%C3%A9) [second](#café-2) [literal](#caf%C3%A9-2-2) \
        [styled](#caf%C3%A9-3) [chinese](#%E4%B8%AD%E6%96%87-guide)";
    let doc = parse_markdown(source);
    let expected = ["café", "café-2", "café-2-2", "café-3", "中文-guide"];
    let html = html::render_fragment(&doc.blocks, &HtmlOptions::default());
    let indexed: Vec<_> = build_search_index(&doc)
        .entries
        .into_iter()
        .filter(|entry| entry.kind == EntryKind::Heading)
        .map(|entry| entry.anchor)
        .collect();
    assert_eq!(indexed, expected);
    let stats = compute_doc_stats(source, &doc);
    let outline: Vec<_> = stats
        .outline
        .iter()
        .map(|heading| heading.slug.as_str())
        .collect();
    assert_eq!(outline, expected);
    assert!(
        stats
            .findings
            .iter()
            .all(|finding| finding.code != "broken_internal_anchor")
    );
    let analysis = analyze_document_links(&doc).unwrap();
    assert_eq!(analysis.references.len(), expected.len());
    for (reference, id) in analysis.references.iter().zip(expected) {
        assert_eq!(reference.finding, None, "{id}");
        assert_eq!(
            analysis
                .resolve_fragment(&reference.destination)
                .unwrap()
                .unwrap()
                .id,
            id
        );
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "missing {id}: {html}"
        );
    }
}

#[test]
fn pdf_encoded_unicode_links_reach_real_destinations_and_decode_only_once() {
    let source = "# Café\n\n[valid](#caf%C3%A9) [double](#caf%25C3%25A9) \
        [case](#Caf%C3%A9) [invalid](#caf%ff)";
    let doc = parse_markdown(source);
    let audit = franken_markdown::pdf::audit_anchors(&doc);
    assert_eq!(audit.resolved, 1);
    assert_eq!(audit.unresolved.len(), 3);
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    let annotations: Vec<_> = text
        .split("endobj")
        .filter(|object| object.contains("/Subtype /Link"))
        .collect();
    assert_eq!(
        annotations.len(),
        1,
        "only the valid destination becomes active"
    );
    assert!(annotations[0].contains("/Dest ["));
    assert!(!annotations[0].contains("/URI"));
}

#[test]
fn pdf_wrapped_headings_keep_source_ids_and_outline_titles() {
    const TITLE: &str = "Supercalifragilisticexpialidocious";
    const ID: &str = "supercalifragilisticexpialidocious";
    let source = format!(
        "# {TITLE}\n\n[1](#{ID})\n\n## {TITLE}\n\n[2](#{ID}-2)\n\n\
         ## {TITLE}-2\n\n[3](#{ID}-2-2)"
    );
    for toc in [false, true] {
        let mut opts = PdfOptions {
            toc,
            ..PdfOptions::default()
        };
        opts.theme.page.size = franken_markdown::theme::PageSize {
            name: "wrapped-heading-regression",
            width_pt: 80.0,
            height_pt: 220.0,
        };
        opts.theme.page.margins = franken_markdown::theme::PageMargins {
            top_pt: 20.0,
            right_pt: 20.0,
            bottom_pt: 20.0,
            left_pt: 20.0,
        };
        let bytes = render_pdf(&source, &opts).unwrap();
        let pdf = String::from_utf8_lossy(&bytes);
        assert_eq!(pdf.matches(&format!("/Title ({TITLE})")).count(), 2);
        assert_eq!(pdf.matches(&format!("/Title ({TITLE}-2)")).count(), 1);

        let links: Vec<_> = pdf
            .split("endobj")
            .filter(|object| object.contains("/Subtype /Link"))
            .collect();
        if toc {
            assert!(links.len() > 3, "TOC entries must also remain active");
        } else {
            assert_eq!(links.len(), 3, "each source fragment must remain active");
        }
        let destinations: std::collections::BTreeSet<_> = links
            .iter()
            .map(|object| {
                object
                    .split_once("/Dest [")
                    .expect("internal destination")
                    .1
                    .split_once(']')
                    .unwrap()
                    .0
            })
            .collect();
        assert_eq!(
            destinations.len(),
            3,
            "wrapped duplicates and literal suffixes must remain distinct"
        );
    }
}

#[test]
fn svg_unicode_links_use_the_same_heading_identity() {
    let doc = parse_markdown("# Café\n\n[go](#caf%C3%A9)");
    let (bytes, _, warnings) = franken_markdown::svg::render_svg_with_diagnostics(
        &doc,
        &franken_markdown::svg::SvgOptions::default(),
    );
    assert!(
        warnings
            .iter()
            .all(|warning| warning.code != "svg_link_unresolved")
    );
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<view "));
    assert!(svg.contains("<a "));
}
