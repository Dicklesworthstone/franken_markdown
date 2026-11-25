//! Public source-copy regressions. Expected ranges come from the original
//! Markdown spelling, never a second renderer or a source-map search result.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::{
    Block, DocumentSourceMap, HeadlessFlowConsumer, Inline, ParseDiagnostic, Profile,
    SourceMapError, SourceOrigin, SourceSpan, TextSelectionRange, parse_markdown_spanned,
    parse_markdown_spanned_with_profile,
};

fn source_map(source: &str) -> DocumentSourceMap {
    parse_markdown_spanned(source).source_map(source).unwrap()
}

fn assert_exact_source(
    map: &DocumentSourceMap,
    source: &str,
    rendered: TextSelectionRange,
    expected: SourceSpan,
) {
    let ranges = map.copy_exact_source_ranges(rendered).unwrap();
    assert_eq!(ranges.len(), 1, "source: {source:?}, ranges: {ranges:?}");
    assert_eq!(ranges[0].origin, SourceOrigin::Primary);
    assert_eq!(ranges[0].span, expected, "source: {source:?}");
    assert_eq!(
        map.copy_exact_source_slices(rendered, source).unwrap(),
        [expected.slice(source).unwrap()],
        "source: {source:?}",
    );
}

#[test]
fn transformed_scalars_copy_the_complete_original_token() {
    for (source, rendered) in [
        ("&#x1f600;", "😀"),
        ("&#128512;", "😀"),
        ("&copy;", "©"),
        ("&amp;", "&"),
        ("&#0;", "�"),
        (r"\*", "*"),
        (r"\\", r"\"),
    ] {
        let map = source_map(source);
        assert_eq!(map.rendered_text(), format!("{rendered}\n\n"));
        assert_exact_source(
            &map,
            source,
            TextSelectionRange::new(0, rendered.len()),
            SourceSpan::new(0, source.len()),
        );
    }
}

#[test]
fn an_entity_cannot_borrow_a_later_identical_literal() {
    let source = "&copy;**©**";
    let map = source_map(source);
    assert_eq!(map.rendered_text(), "©©\n\n");
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(0, 2),
        SourceSpan::new(0, 6),
    );
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(2, 4),
        SourceSpan::new(8, 10),
    );
}

#[test]
fn following_text_is_never_owned_by_an_earlier_opaque_inline() {
    for (source, tail) in [
        (r#"![ALT](image.png " target") target"#, " target"),
        (r#"[label](path(with)paren " target") target"#, " target"),
        ("$x target$ target", " target"),
        (r#"<span title=" target"> target"#, " target"),
        ("`a\r\nb`a b", "a b"),
    ] {
        let map = source_map(source);
        assert!(
            map.rendered_text().ends_with(&format!("{tail}\n\n")),
            "source: {source:?}",
        );
        let end = map.rendered_len() - 2;
        assert_exact_source(
            &map,
            source,
            TextSelectionRange::new(end - tail.len(), end),
            SourceSpan::new(source.len() - tail.len(), source.len()),
        );
    }
}

#[test]
fn normalized_crlf_keeps_entity_offsets_in_the_original_utf8_source() {
    let source = "# 🌍\r\n\r\n&#x1f600;\r\n";
    let map = source_map(source);
    let start = map.rendered_text().find('😀').unwrap();
    let source_start = "# 🌍\r\n\r\n".len();
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(start, start + "😀".len()),
        SourceSpan::new(source_start, source_start + "&#x1f600;".len()),
    );
    assert_eq!(map.headings()[0].source_span.slice(source), Some("# 🌍"));
}

#[test]
fn multi_scalar_entities_keep_one_atomic_source_token() {
    for (source, rendered) in [("&NotEqualTilde;", "≂̸"), ("&fjlig;", "fj")] {
        let map = source_map(source);
        assert_eq!(map.rendered_text(), format!("{rendered}\n\n"));
        // Selecting either scalar still selects the one indivisible source
        // spelling. No overlapping sibling ranges or partial entity bytes.
        for (start, scalar) in rendered.char_indices() {
            assert_exact_source(
                &map,
                source,
                TextSelectionRange::new(start, start + scalar.len_utf8()),
                SourceSpan::new(0, source.len()),
            );
        }
        assert_exact_source(
            &map,
            source,
            TextSelectionRange::new(0, rendered.len()),
            SourceSpan::new(0, source.len()),
        );
    }
}

#[test]
fn normalized_nul_still_copies_the_original_source_byte() {
    let source = "\0";
    let map = source_map(source);
    assert_eq!(map.rendered_text(), "�\n\n");
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(0, "�".len()),
        SourceSpan::new(0, 1),
    );
}

#[test]
fn line_breaks_own_their_complete_original_spelling() {
    for (source, rendered, spelling) in [
        ("α\r\nβ", " ", "\r\n"),
        ("α  \r\nβ", "\n", "  \r\n"),
        ("α\\\r\nβ", "\n", "\\\r\n"),
    ] {
        let map = source_map(source);
        assert_eq!(map.rendered_text(), format!("α{rendered}β\n\n"));
        assert_exact_source(
            &map,
            source,
            TextSelectionRange::new("α".len(), "α".len() + rendered.len()),
            SourceSpan::new("α".len(), "α".len() + spelling.len()),
        );
    }
}

#[test]
fn container_prefixes_do_not_become_inline_source() {
    let source = "> α\r\n> &copy;\r\n> β";
    let map = source_map(source);
    assert_eq!(map.rendered_text(), "α © β\n\n");
    let start = map.rendered_text().find('©').unwrap();
    let source_start = "> α\r\n> ".len();
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(start, start + "©".len()),
        SourceSpan::new(source_start, source_start + "&copy;".len()),
    );
    for element in map
        .elements()
        .iter()
        .filter(|element| !element.is_generated)
    {
        for span in element.source_ranges.spans() {
            assert!(source.is_char_boundary(span.start));
            assert!(source.is_char_boundary(span.end));
            assert!(!span.slice(source).unwrap().contains('>'));
        }
    }
}

#[test]
fn the_supplied_authoring_grammar_owns_source_mapping() {
    let source = "*Term\r\n: definition*";
    let commonmark = parse_markdown_spanned_with_profile(source, Some(Profile::CommonMarkGfm))
        .source_map(source)
        .unwrap();
    assert_eq!(commonmark.rendered_text(), "Term : definition\n\n");
    let definition = commonmark.rendered_text().find(": definition").unwrap();
    assert_exact_source(
        &commonmark,
        source,
        TextSelectionRange::new(definition, definition + ": definition".len()),
        SourceSpan::new("*Term\r\n".len(), source.len() - 1),
    );

    let plus = parse_markdown_spanned_with_profile(source, Some(Profile::GfmPlus))
        .source_map(source)
        .unwrap();
    assert_eq!(plus.rendered_text(), "*Term\n  : definition*\n\n");
    let definition = plus.rendered_text().find("definition*").unwrap();
    assert_exact_source(
        &plus,
        source,
        TextSelectionRange::new(definition, definition + "definition*".len()),
        SourceSpan::new("*Term\r\n: ".len(), source.len()),
    );
}

#[test]
fn rejected_outer_links_do_not_leave_speculative_source_owners() {
    let source = "[outer [target](a)](b)target";
    let map = source_map(source);
    assert_eq!(map.rendered_text(), "[outer target](b)target\n\n");
    let tail = "](b)target";
    let end = map.rendered_len() - 2;
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(end - tail.len(), end),
        SourceSpan::new(source.len() - tail.len(), source.len()),
    );
}

#[test]
fn reference_definitions_never_substitute_for_invocation_text() {
    let source = "[id]: /target \"target\"\r\n\r\n[target][id]target";
    let map = source_map(source);
    assert_eq!(map.rendered_text(), "targettarget\n\n");
    let label_start = "[id]: /target \"target\"\r\n\r\n[".len();
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new(0, "target".len()),
        SourceSpan::new(label_start, label_start + "target".len()),
    );
    assert_exact_source(
        &map,
        source,
        TextSelectionRange::new("target".len(), "targettarget".len()),
        SourceSpan::new(source.len() - "target".len(), source.len()),
    );
}

#[test]
fn generated_list_and_note_markers_do_not_claim_editable_source_ranges() {
    for (source, count) in [("7) item\r\n8) next", 2), ("[^n]:  note\r\n\r\nUse[^n]", 1)] {
        let map = source_map(source);
        let markers: Vec<_> = map
            .elements()
            .iter()
            .filter(|element| element.is_generated)
            .collect();
        assert_eq!(markers.len(), count);
        for marker in markers {
            assert!(marker.source_ranges.spans().is_empty());
            assert!(
                map.copy_exact_source_ranges(marker.rendered_range)
                    .unwrap()
                    .is_empty()
            );
            assert!(
                map.copy_exact_source_slices(marker.rendered_range, source)
                    .unwrap()
                    .is_empty()
            );
            assert!(
                !map.copy_enclosing_source(marker.rendered_range, source)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[test]
fn headless_flow_reuses_the_exact_source_tokens() {
    let source = "&#x1f600;";
    let output = HeadlessFlowConsumer::default()
        .consume_source(source)
        .unwrap();
    assert_eq!(output.lines.len(), 1);
    assert_eq!(output.lines[0].rendered_text, "😀");
    assert_eq!(
        output.lines[0].source_span,
        SourceSpan::new(0, source.len())
    );
    assert_exact_source(
        &output.source_map,
        source,
        output.lines[0].rendered_range,
        SourceSpan::new(0, source.len()),
    );
}

#[test]
fn stale_source_or_mutated_ast_is_refused_instead_of_searching_for_a_match() {
    let source = "alpha";
    let parsed = parse_markdown_spanned(source);
    // Equal length is not evidence of capture identity.
    assert_eq!(
        parsed.source_map("omega"),
        Err(SourceMapError::SourceMismatch)
    );
    assert_eq!(
        parsed.source_map("a shorter or longer capture"),
        Err(SourceMapError::SourceMismatch)
    );
    let mut mutated = parsed.clone();
    mutated.blocks[0].node = Block::Paragraph(vec![Inline::Text("omega".into())]);
    assert_eq!(
        mutated.source_map(source),
        Err(SourceMapError::SourceMismatch)
    );
}

#[test]
fn altered_top_level_spans_are_not_accepted_as_parser_coordinates() {
    let source = "# Heading\r\n\r\nparagraph";
    let parsed = parse_markdown_spanned(source);
    let mut moved = parsed.clone();
    moved.blocks[0].span.start += 1;
    assert_eq!(
        moved.source_map(source),
        Err(SourceMapError::SourceMismatch)
    );
    let mut widened = parsed.clone();
    widened.blocks[0].span.end = source.len();
    assert_eq!(
        widened.source_map(source),
        Err(SourceMapError::SourceMismatch)
    );
    let mut reordered = parsed;
    reordered.blocks.swap(0, 1);
    assert_eq!(
        reordered.source_map(source),
        Err(SourceMapError::SourceMismatch)
    );
}

#[test]
fn host_diagnostics_do_not_change_source_identity() {
    let source = "# Heading\r\n\r\n&copy;";
    let mut parsed = parse_markdown_spanned(source);
    let expected = parsed.source_map(source).unwrap();
    parsed.diagnostics.push(ParseDiagnostic::warning(
        SourceSpan::new(0, "# Heading".len()),
        "Host-supplied editorial warning",
    ));
    assert_eq!(parsed.source_map(source).unwrap(), expected);
}

#[test]
fn supported_page_break_promotion_retains_the_source_capture() {
    let source = "Before\r\n\r\n<!-- pagebreak -->\r\n\r\nAfter";
    let mut parsed = parse_markdown_spanned(source);
    let expected = parsed.source_map(source).unwrap();
    assert!(matches!(parsed.blocks[1].node, Block::HtmlBlock(_)));
    parsed.blocks[1].node = Block::PageBreak;
    assert_eq!(parsed.source_map(source).unwrap(), expected);
}

#[test]
fn empty_links_and_images_do_not_steal_the_previous_blocks_token() {
    for source in ["before\n\n[](target)tail", "before\n\n![](target)tail"] {
        let map = source_map(source);
        assert_eq!(map.rendered_text(), "before\n\ntail\n\n");
        let tail = map
            .elements()
            .iter()
            .find(|element| element.rendered_text == "tail")
            .unwrap();
        assert_exact_source(
            &map,
            source,
            tail.rendered_range,
            SourceSpan::new(source.len() - "tail".len(), source.len()),
        );
    }
}
