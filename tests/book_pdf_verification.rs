//! Full-book verification must retain publication identity and original sources.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, BookSourceLocation, LinkSourcePrecision};
use franken_markdown::{PdfASettings, PdfImageAsset, PdfOptions};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

#[test]
fn full_verification_is_repeatable_and_keeps_ordinary_book_state() {
    let book = BookRenderer::new(&[
        input("one.md", "# One\n\n[Second](two.md#two).\n"),
        input("two.md", "# Two\n\n[First](one.md#one).\n"),
    ])
    .unwrap();
    let source_before = book.validate_links_with_sources().unwrap().to_json();
    let bytes_before = book.render_pdf().unwrap();
    let report = book.verify_pdf().unwrap();
    assert_eq!(report.layout.verdict, "clean");
    assert_eq!(report.navigation.to_json(), source_before);
    assert!(
        report
            .destinations
            .iter()
            .all(|link| link.status == "resolved")
    );
    assert_eq!(report.destinations.len(), 2);
    assert_eq!(
        report.to_json().unwrap(),
        book.verify_pdf().unwrap().to_json().unwrap()
    );
    assert_eq!(book.render_pdf().unwrap(), bytes_before);
    assert_eq!(
        book.validate_links_with_sources().unwrap().to_json(),
        source_before
    );
    assert_eq!(
        report.pagination,
        book.render_pdf_reported().unwrap().pagination
    );
}

#[test]
fn distinct_unresolved_links_keep_their_original_include_destinations_and_ranges() {
    let root = "# Start\n\n{{#include ../parts/body.md}}\n";
    let included = "[First](#missing-one) [Second][missing]\n\n[missing]: #missing-two\n";
    let book = BookRenderer::from_sources(
        &[input("guide/start.md", root)],
        &[input("parts/body.md", included)],
    )
    .unwrap();
    let source_report = book.validate_links_with_sources().unwrap().to_json();
    let report = book.verify_pdf().unwrap();
    assert_eq!(report.navigation.to_json(), source_report);
    assert_eq!(report.destinations.len(), 2);
    for (reference, expected) in report
        .destinations
        .iter()
        .zip(["#missing-one", "#missing-two"])
    {
        assert_eq!(reference.chapter, "guide/start.md");
        assert_eq!(reference.destination, expected);
        assert_eq!(reference.status, "unresolved");
        assert_eq!(reference.page, None);
        assert_eq!(reference.y, None);
        assert_eq!(reference.source.precision, LinkSourcePrecision::Destination);
        let BookSourceLocation::Mapped(ranges) = &reference.source.source else {
            panic!("included link must retain its captured source");
        };
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].path, "parts/body.md");
        assert_eq!(&included[ranges[0].start..ranges[0].end], expected);
    }
    assert_eq!(report.layout.verdict, "findings");
    assert_eq!(report.layout.anchors_unresolved.len(), 2);
    let json = report.to_json().unwrap();
    assert!(json.contains("#missing-one") && json.contains("#missing-two"));
    assert!(!json.contains("fmd-book-pdf-nav/"));
}

#[test]
fn dormant_pdf_endnotes_have_pdf_findings_without_changing_html_navigation() {
    let source = "# Start\n\nBody.\n\n[^unused]: [Dormant](#missing-note-target)\n";
    let book = BookRenderer::new(&[input("one.md", source)]).unwrap();
    // HTML/source navigation excludes unreferenced notes, while PDF preserves
    // them as endnotes. Full verification must inspect the published text.
    assert_eq!(book.validate_links().unwrap().finding_count(), 0);
    let report = book.verify_pdf().unwrap();
    assert_eq!(report.navigation.report().finding_count(), 0);
    let reference = report
        .destinations
        .iter()
        .find(|reference| reference.destination == "#missing-note-target")
        .expect("PDF endnote reference is retained");
    assert_eq!(reference.status, "unresolved");
    let BookSourceLocation::Mapped(ranges) = &reference.source.source else {
        panic!("dormant note source is still owned");
    };
    assert!(ranges.iter().any(|range| {
        range.path == "one.md" && &source[range.start..range.end] == "#missing-note-target"
    }));
    assert!(report.layout.findings.iter().any(|finding| {
        finding.code == "unresolved_pdf_destination"
            && finding.detail.contains("#missing-note-target")
    }));
    assert!(
        report
            .layout
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text.contains("Dormant"))
    );
}

#[test]
fn external_urls_and_nonchapter_resources_are_never_fabricated_as_missing_anchors() {
    let book = BookRenderer::new(&[input(
        "one.md",
        "# Start\n\n[Web](https://example.com/page#part) [Archive](assets/file.zip) [Root](#start).\n",
    )])
    .unwrap();
    let report = book.verify_pdf().unwrap();
    assert_eq!(report.layout.verdict, "clean");
    assert_eq!(report.layout.anchors_resolved, 1);
    assert_eq!(
        report
            .destinations
            .iter()
            .filter(|link| link.status == "unchecked")
            .count(),
        2
    );
    assert!(report.layout.anchors_unresolved.is_empty());
}

#[test]
fn strict_pagination_and_archival_admission_keep_the_publishers_specific_failure() {
    let book = BookRenderer::new(&[input("one.md", "A  \nB  \nC")]).unwrap();
    let mut options = PdfOptions {
        strict_pagination: true,
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 400.0;
    options.theme.page.size.height_pt = 112.0;
    options.theme.page.margins = franken_markdown::PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    let publication = book
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap_err();
    let verification = book
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap_err();
    assert_eq!(publication.code(), "pagination_infeasible");
    assert_eq!(verification.code(), publication.code());

    let mut invalid_archival = PdfASettings::OFF;
    invalid_archival.strict = true;
    let options = PdfOptions::default();
    assert_eq!(
        book.verify_pdf_with_options(&options, invalid_archival)
            .unwrap_err()
            .code(),
        book.render_pdf_with_options(&options, invalid_archival)
            .unwrap_err()
            .code(),
    );
}

#[test]
fn resource_admission_and_chapter_relative_image_findings_match_publication() {
    let book =
        BookRenderer::new(&[input("guide/one.md", "# One\n\n![Chart](chart.png)\n")]).unwrap();
    let mut options = PdfOptions::default();
    options
        .image_assets
        .push(PdfImageAsset::new("guide/chart.png", vec![0, 1, 2, 3]));
    let report = book
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert!(report.layout.findings.iter().any(|finding| {
        finding.code == "unsupported_image" && finding.detail.contains("guide/chart.png")
    }));
    assert!(
        !report
            .layout
            .findings
            .iter()
            .any(|finding| finding.code == "missing_image")
    );
    options.image_assets[0].destination.clear();
    let verify_error = book
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap_err();
    let render_error = book
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap_err();
    assert_eq!(verify_error.to_string(), render_error.to_string());
}

#[test]
fn report_limit_counts_escaped_output_bytes_and_never_returns_partial_json() {
    let book = BookRenderer::new(&[input("one.md", "A")]).unwrap();
    let mut report = book.verify_pdf().unwrap();
    let run = report
        .layout
        .pages
        .iter_mut()
        .flat_map(|page| &mut page.runs)
        .find(|run| run.text == "A")
        .unwrap();
    // Raw bytes fit under 8 MiB, but JSON must escape each control byte.
    run.text = "\u{1}".repeat(1_500_000);
    let error = report.to_json().unwrap_err();
    assert!(error.to_string().contains("book_pdf_verification_limit"));
}

#[test]
fn ambiguous_repeated_html_destinations_do_not_invent_precise_source_ownership() {
    let book = BookRenderer::new(&[input(
        "one.md",
        "<p><a href='#missing'>First</a><a href='#missing'>Second</a></p>\n",
    )])
    .unwrap();
    let report = book.verify_pdf().unwrap();
    assert_eq!(report.destinations.len(), 2);
    for reference in &report.destinations {
        assert_eq!(reference.destination, "#missing");
        assert_eq!(reference.status, "unresolved");
        assert!(matches!(
            &reference.source.source,
            BookSourceLocation::Unavailable("ownership_unavailable")
        ));
    }
    assert_eq!(report.navigation.report().finding_count(), 2);
}

#[test]
fn return_navigation_requires_a_citation_with_its_own_note_link() {
    let source = "# Start\n\n[Ordinary link with a citation[^hidden]](https://example.com).\n\n[^hidden]: A complete retained note.\n";
    let book = BookRenderer::new(&[input("one.md", source)]).unwrap();
    let without = PdfOptions::default();
    let with = PdfOptions {
        footnote_backlinks: true,
        ..PdfOptions::default()
    };
    // A nested citation retains its enclosing action; there is no separate
    // note link or generated return label to verify. This uses actual PDF
    // bytes as a negative control, independently of the report inventory.
    assert_eq!(
        book.render_pdf_with_options(&with, PdfASettings::OFF)
            .unwrap(),
        book.render_pdf_with_options(&without, PdfASettings::OFF)
            .unwrap(),
    );
    let report = book
        .verify_pdf_with_options(&with, PdfASettings::OFF)
        .unwrap();
    assert_eq!(
        report.layout.verdict, "clean",
        "{:?}",
        report.layout.findings
    );
    assert_eq!(report.destinations.len(), 2);
    assert_eq!(report.destinations[0].status, "unchecked");
    assert_eq!(report.destinations[1].kind, "footnote");
    assert_eq!(report.destinations[1].status, "resolved");
    assert!(
        report
            .destinations
            .iter()
            .all(|reference| reference.kind != "footnote_backreference")
    );
}

#[test]
fn repeated_note_returns_keep_a_linked_source_and_missing_author_targets() {
    let nested = "[Nested citation[^n]](https://example.com).";
    let linked = "Actual linked citation[^n].";
    let source = format!(
        "# Start\n\n{nested}\n\n{linked}\n\nAnother linked citation[^n]. [Missing](#absent).\n\n[^n]: Retained note.\n"
    );
    let book = BookRenderer::new(&[input("one.md", &source)]).unwrap();
    let options = PdfOptions {
        footnote_backlinks: true,
        ..PdfOptions::default()
    };
    let report = book
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let returns: Vec<_> = report
        .destinations
        .iter()
        .filter(|reference| reference.kind == "footnote_backreference")
        .collect();
    assert_eq!(returns.len(), 1, "one generated label per note");
    assert_eq!(returns[0].status, "resolved");
    let BookSourceLocation::Mapped(ranges) = &returns[0].source.source else {
        panic!("linked citation provenance must be retained");
    };
    assert_eq!(ranges.len(), 1);
    assert_eq!(&source[ranges[0].start..ranges[0].end], linked);
    assert_eq!(
        report
            .destinations
            .iter()
            .filter(|reference| reference.kind == "footnote")
            .count(),
        3
    );
    assert_eq!(report.layout.anchors_unresolved, ["one.md: #absent"]);
    assert_eq!(
        report
            .destinations
            .iter()
            .filter(|reference| reference.status == "unresolved")
            .count(),
        1
    );
    assert_eq!(report.layout.verdict, "findings");
}

#[test]
fn note_only_cycles_and_forward_citations_keep_their_real_return_destinations() {
    let source = "# Start\n\nBody.\n\n[^first]: Forward citation[^second].\n\n[^second]: Back citation[^first]. [Nested-only citation[^hidden]](https://example.com).\n\n[^hidden]: Nested note.\n";
    let book = BookRenderer::new(&[input("one.md", source)]).unwrap();
    let options = PdfOptions {
        footnote_backlinks: true,
        ..PdfOptions::default()
    };
    let report = book
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert_eq!(
        report.layout.verdict, "clean",
        "{:?}",
        report.layout.findings
    );
    let returns: Vec<_> = report
        .destinations
        .iter()
        .filter(|reference| reference.kind == "footnote_backreference")
        .collect();
    assert_eq!(returns.len(), 2);
    assert_eq!(
        returns
            .iter()
            .map(|reference| reference.destination.as_str())
            .collect::<Vec<_>>(),
        ["second", "first"]
    );
    for reference in returns {
        assert_eq!(reference.status, "resolved");
        assert!(reference.page.is_some() && reference.y.is_some());
    }
    assert!(
        report
            .destinations
            .iter()
            .filter(|reference| reference.kind == "footnote")
            .all(|reference| reference.status == "resolved")
    );
}

#[test]
fn discarded_duplicate_definitions_do_not_invent_published_references() {
    let source = "# Start\n\nBody[^n].\n\n[^n]: Selected first definition.\n\n[^n]: [Discarded link](#missing).\n";
    let book = BookRenderer::new(&[input("one.md", source)]).unwrap();
    assert_eq!(book.validate_links().unwrap().finding_count(), 0);
    let report = book.verify_pdf().unwrap();
    assert_eq!(
        report.layout.verdict, "clean",
        "{:?}",
        report.layout.findings
    );
    assert_eq!(report.destinations.len(), 1);
    assert_eq!(report.destinations[0].kind, "footnote");
    assert_eq!(report.destinations[0].destination, "n");
    assert_eq!(report.destinations[0].status, "resolved");
    let text = report
        .layout
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<String>();
    assert!(text.contains("Selected first definition"));
    assert!(!text.contains("Discarded link"));

    let source = "# Start\n\nBody[^n].\n\n[^n]: [Selected link](#missing).\n\n[^n]: [Discarded link](#missing).\n";
    let book = BookRenderer::new(&[input("one.md", source)]).unwrap();
    let report = book.verify_pdf().unwrap();
    assert_eq!(report.layout.verdict, "findings");
    let unresolved: Vec<_> = report
        .destinations
        .iter()
        .filter(|reference| reference.status == "unresolved")
        .collect();
    assert_eq!(unresolved.len(), 1, "the published broken link remains");
    assert_eq!(unresolved[0].destination, "#missing");
    let BookSourceLocation::Mapped(ranges) = &unresolved[0].source.source else {
        panic!("the first definition retains exact source ownership");
    };
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start, source.find("#missing").unwrap());
    assert_eq!(&source[ranges[0].start..ranges[0].end], "#missing");
}
