//! Preview/source heading identity must match actual publication allocation.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::{HeadlessFlowConsumer, HtmlOptions, parse_markdown_spanned};

fn published_ids(html: &str) -> Vec<&str> {
    html.split("<h")
        .filter_map(|tail| {
            let (level, rest) = tail.split_at_checked(1)?;
            if !matches!(level, "1" | "2" | "3" | "4" | "5" | "6") {
                return None;
            }
            rest.strip_prefix(" id=\"")?.split_once('"').map(|(id, _)| id)
        })
        .collect()
}

fn check(source: &str, published: &[&str], primary: &[&str]) {
    let parsed = parse_markdown_spanned(source);
    let map = parsed.source_map(source).unwrap();
    let html = franken_markdown::html::render_fragment(
        &parsed.to_document().blocks,
        &HtmlOptions::default(),
    );
    assert_eq!(published_ids(&html), published, "{source:?}: {html}");
    assert_eq!(
        map.headings()
            .iter()
            .map(|heading| heading.slug.as_str())
            .collect::<Vec<_>>(),
        primary,
        "{source:?}"
    );
    for heading in map.headings() {
        assert!(published.contains(&heading.slug.as_str()));
        assert_eq!(map.resolve_source_anchor(&heading.slug), Some(heading));
        assert_eq!(heading.source_span, parsed.blocks[heading.block_index].span);
        assert!(heading.source_span.slice(source).is_some());
        assert!(map.rendered_text().is_char_boundary(heading.rendered_offset));
    }
}

#[test]
fn literal_suffixes_cannot_alias_generated_heading_ids() {
    check(
        "# A\n\n# A\n\n# A-2\n\n# A\n\n# A-2\n",
        &["a", "a-2", "a-2-2", "a-3", "a-2-3"],
        &["a", "a-2", "a-2-2", "a-3", "a-2-3"],
    );
    check(
        "# A-2\n\n# A\n\n# A\n",
        &["a-2", "a", "a-3"],
        &["a-2", "a", "a-3"],
    );
    check(
        "# !!!\n\n# Section-2\n\n# ???\n\n# Section\n",
        &["section", "section-2", "section-3", "section-4"],
        &["section", "section-2", "section-3", "section-4"],
    );
}

#[test]
fn nested_body_headings_reserve_ids_without_inventing_primary_ranges() {
    check(
        "> # A\n\n- ## A\n\n# A\n\n> - ### A\n\n# A-2\n",
        &["a", "a-2", "a-3", "a-4", "a-2-2"],
        &["a-3", "a-2-2"],
    );
    let source = "> # A\n\n# A\n";
    let parsed = parse_markdown_spanned(source);
    let map = parsed.source_map(source).unwrap();
    assert!(map.resolve_source_anchor("a").is_none());
    assert_eq!(
        map.resolve_source_anchor("a-2").unwrap().source_span.slice(source),
        Some("# A")
    );
}

#[test]
fn note_headings_allocate_after_body_in_first_reference_order() {
    let source = concat!(
        "[^first]: # A\n\n",
        "[^second]: # A\n\n",
        "# A\n\n",
        "Use[^second] then[^first].\n\n",
        "# A\n\n",
        "# A-2\n",
    );
    check(
        source,
        &["a", "a-2", "a-2-2", "a-3", "a-4"],
        &["a", "a-2", "a-2-2"],
    );
}

#[test]
fn omitted_and_duplicate_notes_keep_source_slots_without_claiming_ids() {
    let source = concat!(
        "[^unused]: # A\n\n",
        "[^used]: # A\n\n",
        "[^used]: # A\n\n",
        "> # A\n\n",
        "# A\n\n",
        "Use[^used].\n\n",
        "# A-2\n",
    );
    check(
        source,
        &["a", "a-2", "a-2-2", "a-3"],
        &["a-2", "a-2-2"],
    );
}

#[test]
fn unicode_setext_and_line_endings_preserve_exact_primary_source_ownership() {
    let original = "\u{feff}> # Élan\n\nÉlan\n=====\n\n# Élan-2\n\n# Élan\n";
    for ending in ["\n", "\r\n", "\r"] {
        let source = original.replace('\n', ending);
        check(
            &source,
            &["élan", "élan-2", "élan-2-2", "élan-3"],
            &["élan-2", "élan-2-2", "élan-3"],
        );
    }
}

#[test]
fn headless_preview_resolves_the_published_collision_target() {
    let source = "> # A\n\n# A\n\n# A-2\n\n# A\n";
    let output = HeadlessFlowConsumer::default().consume_source(source).unwrap();
    let heading = output.source_map.resolve_source_anchor("a-2-2").unwrap();
    assert_eq!(heading.title, "A-2");
    assert_eq!(heading.source_span.slice(source), Some("# A-2"));
    assert_eq!(
        output
            .source_map
            .resolve_source_anchor("a-3")
            .unwrap()
            .source_span
            .slice(source),
        Some("# A")
    );
}
