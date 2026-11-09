//! Original-source navigation follows parser ownership, not equal URL strings.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{
    BookInput, BookRenderer, BookSourceLocation, BookSourceRange, BookSourceRangeKind,
    BookWorkspace, LinkSourcePrecision, LocatedLinkReport,
};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn ranges(report: &LocatedLinkReport, chapter: usize, finding: usize) -> &[BookSourceRange] {
    let location = report.source_location(chapter, finding).unwrap();
    assert_eq!(location.precision, LinkSourcePrecision::Destination);
    match &location.source {
        BookSourceLocation::Mapped(ranges) => ranges,
        other => panic!("expected original destination: {other:?}"),
    }
}

#[test]
fn equal_inline_and_reference_urls_keep_actual_token_identity_and_first_definition() {
    let source = concat!(
        "# Start\n\n[first][r] ![image][r]\n\n",
        "[direct](missing.md#x) [second][s] [third][r]\n\n",
        "[r]: missing.md#x\n[r]: ignored.md#x\n[s]: missing.md#x\n",
    );
    let book = BookRenderer::new(&[input("start.md", source)]).unwrap();
    assert!(book.source_diagnostics().is_empty());
    let located = book.validate_links_with_sources().unwrap();
    assert_eq!(located.report(), &book.validate_links().unwrap());
    assert_eq!(located.report().finding_count(), 4);
    let first = source.find("[r]: missing").unwrap() + "[r]: ".len();
    let direct = source.find("[direct](").unwrap() + "[direct](".len();
    let second = source.find("[s]: missing").unwrap() + "[s]: ".len();
    for (finding, start) in [first, direct, second, first].into_iter().enumerate() {
        assert_eq!(
            ranges(&located, 0, finding),
            &[BookSourceRange {
                path: "start.md".into(),
                start,
                end: start + "missing.md#x".len(),
                kind: BookSourceRangeKind::Exact,
            }]
        );
    }
    assert!(!located.to_json().contains("ignored.md"));
    assert!(
        !book
            .validate_links()
            .unwrap()
            .to_json()
            .unwrap()
            .contains("\"source\"")
    );
    assert_eq!(located, book.clone().validate_links_with_sources().unwrap());
}

#[test]
fn repeated_nested_includes_and_rewritten_urls_select_the_defining_source() {
    let part = "prefix 😀\r\n<!-- ANCHOR: x -->\r\n[broken](missing.md?x=1#part)\r\n<!-- ANCHOR_END: x -->\r\n";
    let root = "# Start\n\n{{#include ../parts/outer.md}}\n\n{{#include ../parts/link.txt:x}}\n";
    let book = BookRenderer::from_sources(
        &[input("manual/start.md", root)],
        &[
            input("parts/outer.md", "{{#include link.txt:x}}\n"),
            input("parts/link.txt", part),
        ],
    )
    .unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report(), &book.validate_links().unwrap());
    assert_eq!(report.report().finding_count(), 2);
    let start = part.find("missing.md").unwrap();
    for finding in 0..2 {
        assert_eq!(
            ranges(&report, 0, finding),
            &[BookSourceRange {
                path: "parts/link.txt".into(),
                start,
                end: start + "missing.md?x=1#part".len(),
                kind: BookSourceRangeKind::Rewritten,
            }]
        );
    }
}

#[test]
fn reference_definition_in_an_include_is_selected_instead_of_the_citation() {
    let root = "# Start\n\n[one][r] [two][] [two]\n\n{{#include definitions.txt}}\n";
    let definitions = "[r]: missing.md#r\n[two]: <missing.md#two>\n[r]: shadowed.md\n";
    let book = BookRenderer::from_sources(
        &[input("start.md", root)],
        &[input("definitions.txt", definitions)],
    )
    .unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 3);
    for (index, token) in ["missing.md#r", "<missing.md#two>", "<missing.md#two>"]
        .into_iter()
        .enumerate()
    {
        let location = &ranges(&report, 0, index)[0];
        assert_eq!(location.path, "definitions.txt");
        assert_eq!(&definitions[location.start..location.end], token);
    }
}

#[test]
fn literal_image_and_abandoned_outer_links_cannot_steal_a_real_link_location() {
    let source = concat!(
        "\u{feff}---\r\ntitle: Sources\r\n---\r\n\r\n# 😀\r\n\r\n",
        "`[code](#absent)` ![alt [hidden](#hidden)](image.png)\r\n\r\n",
        "[outer [inner](#absent)](#ignored) **[strong](#absent)**\r\n\r\n",
        "```md\r\n[code](#absent)\r\n```\r\n",
    );
    let book = BookRenderer::new(&[input("start.md", source)]).unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 2);
    for (index, prefix) in ["[inner](", "[strong]("].into_iter().enumerate() {
        let start = source.find(prefix).unwrap() + prefix.len();
        let location = &ranges(&report, 0, index)[0];
        assert_eq!(location.start, start);
        assert_eq!(&source[location.start..location.end], "#absent");
    }
    assert_eq!(book.source_diagnostics().diagnostic_count(), 0);
}

#[test]
fn published_note_order_uses_each_note_body_and_ignores_unused_definitions() {
    let source = "# Start\n\nCall[^a]\n\n[^unused]: [hidden](#absent)\n\n[^b]: [second](#absent)\n\n[^a]: [first](#absent) nested[^b]\n";
    let book = BookRenderer::new(&[input("start.md", source)]).unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 2);
    for (index, prefix) in ["[first](", "[second]("].into_iter().enumerate() {
        assert_eq!(
            ranges(&report, 0, index)[0].start,
            source.find(prefix).unwrap() + prefix.len()
        );
    }
}

#[test]
fn supported_html_anchor_tokens_exclude_images_and_survive_canonicalization() {
    let source = "# Start\n\n<div><a href=\"next.md#one\">one</a><img src=\"next.md#image\"><a href='next.md#two'>two</a></div>\n\nInline <a href=\"#three\">three</a>.\n";
    let book =
        BookRenderer::new(&[input("start.md", source), input("next.md", "# Next\n")]).unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 3);
    for (index, token) in ["\"next.md#one\"", "'next.md#two'", "\"#three\""]
        .into_iter()
        .enumerate()
    {
        let location = &ranges(&report, 0, index)[0];
        assert_eq!(&source[location.start..location.end], token);
    }
    assert_eq!(report.report(), &book.validate_links().unwrap());
    assert_eq!(report, book.clone().validate_links_with_sources().unwrap());
}

#[test]
fn nul_replacement_and_authoring_profiles_preserve_original_byte_endpoints() {
    let source = "# Start\r\n\r\n前\0 [broken](#bad\0id)\r\n";
    for profile in [
        None,
        Some(franken_markdown::Profile::CommonMarkGfm),
        Some(franken_markdown::Profile::GfmPlus),
    ] {
        let book = BookRenderer::new_with_profile(&[input("start.md", source)], profile).unwrap();
        let report = book.validate_links_with_sources().unwrap();
        assert_eq!(report.report().finding_count(), 1);
        let location = &ranges(&report, 0, 0)[0];
        assert_eq!(&source[location.start..location.end], "#bad\0id");
        assert_eq!(location.start, source.find("#bad").unwrap());
    }
}

#[test]
fn mapping_only_source_edits_refresh_links_with_no_parser_warnings_or_reparse() {
    let before = "<!-- ANCHOR: x -->\n[broken](#absent)\n<!-- ANCHOR_END: x -->\n";
    let after = format!("Outside 😀\r\n{before}");
    let mut book = BookWorkspace::from_sources(
        &[input("start.md", "# Start\n\n{{#include part.txt:x}}\n")],
        &[input("part.txt", before)],
    )
    .unwrap();
    let old = book.validate_links_with_sources().unwrap();
    assert!(book.source_diagnostics().is_empty());
    let update = book.update_sources(&[input("part.txt", &after)]).unwrap();
    assert!(update.reparsed_chapters.is_empty());
    let new = book.validate_links_with_sources().unwrap();
    assert_eq!(old.report(), new.report());
    assert_eq!(
        ranges(&new, 0, 0)[0].start,
        ranges(&old, 0, 0)[0].start + "Outside 😀\r\n".len()
    );
    assert!(
        book.update_sources(&[input("part.txt", "{{#include missing.txt}}")])
            .is_err()
    );
    assert_eq!(new, book.validate_links_with_sources().unwrap());
    book.set_authoring_profile(Some(franken_markdown::Profile::CommonMarkGfm))
        .unwrap();
    assert_eq!(new, book.validate_links_with_sources().unwrap());
}

#[test]
fn provenance_exhaustion_preserves_every_base_finding_and_explicitly_refuses_locations() {
    let path = format!("{}.md", "a".repeat(200));
    let source = "[broken](#absent)\n\n".repeat(2000);
    let book = BookRenderer::new(&[input(&path, &source)]).unwrap();
    let located = book.validate_links_with_sources().unwrap();
    assert_eq!(located.report(), &book.validate_links().unwrap());
    assert_eq!(located.report().finding_count(), 2000);
    assert_eq!(ranges(&located, 0, 0)[0].start, "[broken](".len());
    assert_eq!(
        located.source_location(0, 1999).unwrap().source,
        BookSourceLocation::Unavailable("report_provenance_limit")
    );
    assert!(located.to_json().len() <= 6 * 1024 * 1024);
}
