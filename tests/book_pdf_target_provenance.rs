//! Canonical PDF identities and parser-owned spellings remain separate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{
    BookInput, BookPdfDestination, BookRenderer, BookSourceLocation, BookSourceRangeKind,
    BookWorkspace, LinkSourcePrecision,
};
use franken_markdown::{PdfASettings, PdfOptions};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn original_token<'a>(target: &BookPdfDestination, path: &str, source: &'a str) -> &'a str {
    assert_eq!(target.source.precision, LinkSourcePrecision::Destination);
    let BookSourceLocation::Mapped(ranges) = &target.source.source else {
        panic!("original token ownership must be retained: {target:?}");
    };
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].path, path);
    &source[ranges[0].start..ranges[0].end]
}

#[test]
fn different_spellings_of_one_target_keep_each_parser_token_and_reference_definition() {
    const SOURCE: &str = concat!(
        "# Start\n\n[Relative](target.md#target) [Dot](./target.md#target) ",
        "[Root](/guide/target.md#target) [Encoded](target%2Emd#target) ",
        "[Short](target#target) [Missing](target.md#absent) [Defined][r]\n\n",
        "[r]: <./target.md#target> \"Target\"\n",
    );
    let renderer = BookRenderer::new(&[
        input("guide/start.md", SOURCE),
        input("guide/target.md", "# Target\n"),
    ])
    .unwrap();
    let pdf = renderer.render_pdf().unwrap();
    let report = renderer.verify_pdf().unwrap();
    assert_eq!(report.destinations.len(), 7);
    for (index, (parsed, token)) in [
        ("target.md#target", "target.md#target"),
        ("./target.md#target", "./target.md#target"),
        ("/guide/target.md#target", "/guide/target.md#target"),
        ("target%2Emd#target", "target%2Emd#target"),
        ("target#target", "target#target"),
        ("target.md#absent", "target.md#absent"),
        ("./target.md#target", "<./target.md#target>"),
    ]
    .into_iter()
    .enumerate()
    {
        let target = &report.destinations[index];
        assert_eq!(target.parsed_destination.as_deref(), Some(parsed));
        assert_eq!(original_token(target, "guide/start.md", SOURCE), token);
        if index == 5 {
            assert_eq!(target.destination, "/guide/target.md#absent");
            assert_eq!(target.status, "unresolved");
            assert_eq!(target.page, None);
            assert_eq!(target.y, None);
        } else {
            assert_eq!(target.destination, "/guide/target.md#target");
            assert_eq!(target.status, "resolved");
            assert_eq!(target.page, report.destinations[0].page);
            assert_eq!(target.y, report.destinations[0].y);
            assert!(target.page.is_some_and(|page| page > 1));
        }
    }
    assert_eq!(
        report.layout.anchors_unresolved,
        ["guide/start.md: /guide/target.md#absent"]
    );
    assert_eq!(report.navigation.report().finding_count(), 1);
    assert_eq!(
        report
            .layout
            .findings
            .iter()
            .filter(|finding| finding.code == "missing_anchor")
            .count(),
        1
    );
    assert!(
        !report
            .layout
            .findings
            .iter()
            .any(|finding| finding.code == "unresolved_pdf_destination")
    );
    assert_eq!(renderer.render_pdf().unwrap(), pdf);
    assert_eq!(
        renderer.clone().verify_pdf().unwrap().to_json().unwrap(),
        report.to_json().unwrap()
    );
}

#[test]
fn included_urls_report_rebased_parser_targets_and_original_definition_tokens() {
    const ROOT: &str = "# Start\n\n[From root][r]\n\n{{#include ../parts/body.md}}\n";
    const PART: &str = "[Included](next.md#next)\n\n[r]: <next.md#next>\n";
    let renderer = BookRenderer::from_sources(
        &[
            input("guide/start.md", ROOT),
            input("parts/next.md", "# Next\n"),
        ],
        &[input("parts/body.md", PART)],
    )
    .unwrap();
    let report = renderer.verify_pdf().unwrap();
    assert_eq!(report.destinations.len(), 2);
    for (target, token) in report
        .destinations
        .iter()
        .zip(["<next.md#next>", "next.md#next"])
    {
        assert_eq!(target.destination, "/parts/next.md#next");
        assert_eq!(
            target.parsed_destination.as_deref(),
            Some("../parts/next.md#next")
        );
        assert_eq!(target.status, "resolved");
        assert_eq!(original_token(target, "parts/body.md", PART), token);
        let BookSourceLocation::Mapped(ranges) = &target.source.source else {
            unreachable!()
        };
        assert_eq!(ranges[0].kind, BookSourceRangeKind::Rewritten);
    }
}

#[test]
fn html_targets_decode_entities_without_guessing_ambiguous_lowered_ownership() {
    const SOURCE: &str = "<p><a href='./target.md?mode=one&amp;x=1#target'>Unique</a></p>\n\n<p><a href='target.md#target'>First</a><a href='./target.md#target'>Second</a></p>\n";
    let renderer = BookRenderer::new(&[
        input("guide/start.md", SOURCE),
        input("guide/target.md", "# Target\n"),
    ])
    .unwrap();
    let report = renderer.verify_pdf().unwrap();
    assert_eq!(report.destinations.len(), 3);
    let unique = &report.destinations[0];
    assert_eq!(unique.destination, "/guide/target.md?mode=one&x=1#target");
    assert_eq!(
        unique.parsed_destination.as_deref(),
        Some("./target.md?mode=one&x=1#target")
    );
    assert_eq!(
        original_token(unique, "guide/start.md", SOURCE),
        "'./target.md?mode=one&amp;x=1#target'"
    );
    for target in &report.destinations[1..] {
        assert_eq!(target.destination, "/guide/target.md#target");
        assert_eq!(target.parsed_destination, None);
        assert_eq!(
            target.source.source,
            BookSourceLocation::Unavailable("ownership_unavailable")
        );
        assert_eq!(target.status, "resolved");
    }
    assert!(
        report
            .to_json()
            .unwrap()
            .contains("\"parsedDestination\":null")
    );
}

#[test]
fn chapter_local_note_labels_and_generated_returns_keep_parser_identity() {
    let renderer = BookRenderer::new(&[
        input("one.md", "# Same\n\nFirst[^n].\n\n[^n]: First note.\n"),
        input("two.md", "# Same\n\nSecond[^n].\n\n[^n]: Second note.\n"),
    ])
    .unwrap();
    let options = PdfOptions {
        footnote_backlinks: true,
        ..PdfOptions::default()
    };
    let report = renderer
        .verify_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    assert_eq!(report.destinations.len(), 4);
    for target in &report.destinations {
        assert_eq!(target.destination, "n");
        assert_eq!(target.parsed_destination.as_deref(), Some("n"));
        assert!(matches!(target.kind, "footnote" | "footnote_backreference"));
        assert_eq!(target.status, "resolved");
        assert!(target.page.is_some_and(|page| page > 0));
        assert!(target.y.is_some_and(f32::is_finite));
        let BookSourceLocation::Mapped(ranges) = &target.source.source else {
            panic!("note citation must retain its chapter");
        };
        assert!(ranges.iter().all(|range| range.path == target.chapter));
    }
    assert_eq!(report.destinations[0].chapter, "one.md");
    assert_eq!(report.destinations[2].chapter, "two.md");
    assert!(report.destinations[0].page < report.destinations[2].page);
}

#[test]
fn optional_target_budget_is_shared_across_chapters_without_losing_navigation_or_ranges() {
    // Each spelling fits the individual 8192-byte ceiling. The complete
    // two-chapter capture crosses the independent 1 MiB spelling allowance;
    // canonical targets and rendered link text stay small.
    let parsed = format!("{}target.md#target", "./".repeat(4080));
    assert!(parsed.len() <= 8192);
    let source = format!("# Start\n\n{}", format!("[Jump]({parsed}) ").repeat(65));
    let renderer = BookRenderer::new(&[
        input("one.md", &source),
        input("two.md", &source),
        input("target.md", "# Target\n"),
    ])
    .unwrap();
    let navigation_before = renderer.validate_links_with_sources().unwrap();
    let report = renderer.verify_pdf().unwrap();
    assert_eq!(report.navigation, navigation_before);
    assert_eq!(report.navigation.report().finding_count(), 0);
    assert_eq!(report.destinations.len(), 130);
    let retained = report
        .destinations
        .iter()
        .filter(|target| target.parsed_destination.is_some())
        .count();
    assert_eq!(retained, (1024 * 1024) / parsed.len());
    assert!(retained < report.destinations.len());
    for target in &report.destinations {
        assert_eq!(target.destination, "/target.md#target");
        assert_eq!(target.status, "resolved");
        assert_eq!(original_token(target, &target.chapter, &source), parsed);
        if let Some(value) = &target.parsed_destination {
            assert_eq!(value, &parsed);
        }
    }
    assert!(
        report
            .to_json()
            .unwrap()
            .contains("\"parsedDestination\":null")
    );
}

#[test]
fn workspace_recaptures_limited_targets_when_remaining_allowance_changes() {
    fn spelling(length: usize) -> String {
        let base = "target.md#target";
        let value = format!("{}{}", "./".repeat((length - base.len()) / 2), base);
        assert_eq!(value.len(), length);
        value
    }
    let long = spelling(8191);
    let first = format!("# First\n\n{}", format!("[Jump]({long}) ").repeat(128));
    // The first chapter leaves 128 bytes. The middle target initially cannot
    // fit after the large target, while the final 27-byte target still can.
    let second = format!(
        "# Second\n\n[Large]({}) [Middle]({}) [Small]({})\n",
        spelling(101),
        spelling(71),
        spelling(27)
    );
    let mut workspace = BookWorkspace::new(&[
        input("one.md", &first),
        input("two.md", &second),
        input("target.md", "# Target\n"),
    ])
    .unwrap();
    let initial = workspace.verify_pdf().unwrap();
    let middle = initial
        .destinations
        .iter()
        .filter(|target| target.chapter == "two.md")
        .nth(1)
        .unwrap();
    assert_eq!(middle.parsed_destination, None);
    // A smaller allowance changes which entries fit: the 101-byte target no
    // longer fits, so the formerly discarded 71-byte target must be recovered.
    let expanded_first = format!("{first} [Extra](target.md#target) [Extra](target.md#target)\n");
    let middle_spelling = spelling(71);
    for updated in [
        expanded_first.as_str(),
        "# First\n\n[Jump](target.md#target)\n",
    ] {
        let receipt = workspace
            .update_sources(&[input("one.md", updated)])
            .unwrap();
        assert_eq!(receipt.reparsed_chapters, [0, 1]);
        let report = workspace.verify_pdf().unwrap();
        let fresh = BookRenderer::new(&[
            input("one.md", updated),
            input("two.md", &second),
            input("target.md", "# Target\n"),
        ])
        .unwrap()
        .verify_pdf()
        .unwrap();
        assert_eq!(report.to_json().unwrap(), fresh.to_json().unwrap());
        let middle = report
            .destinations
            .iter()
            .filter(|target| target.chapter == "two.md")
            .nth(1)
            .unwrap();
        assert_eq!(
            middle.parsed_destination.as_deref(),
            Some(middle_spelling.as_str())
        );
    }
}
