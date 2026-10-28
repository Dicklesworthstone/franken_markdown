//! Accepted link destinations, titles, and reference labels own their source.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn link(label: &str, dest: &str, title: Option<&str>) -> Inline {
    Inline::Link {
        dest: dest.into(),
        title: title.map(str::to_owned),
        content: vec![Inline::Text(label.into())],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn backticks_in_destinations_and_titles_cannot_hide_later_links() {
    for (first, dest, title) in [
        ("[one](/a`x)", "/a`x", None),
        ("[one](</a`x>)", "/a`x", None),
        ("[one](/one \"`code\")", "/one", Some("`code")),
        ("[one](/one '`code')", "/one", Some("`code")),
        ("[one](/one (`code))", "/one", Some("`code")),
    ] {
        let source = format!("{first} [two](/two) `");
        assert_eq!(
            parse_inlines(&source),
            vec![
                link("one", dest, title),
                Inline::Text(" ".into()),
                link("two", "/two", None),
                Inline::Text(" `".into()),
            ],
            "{source:?}"
        );
    }
}

#[test]
fn image_destinations_and_titles_cannot_steal_the_enclosing_link_bracket() {
    for dest in [
        "image].png",
        "image[.png",
        "[image].png",
        "image[1].png",
        "images/café]図.png",
        "image`name.png",
    ] {
        for title in ["caption]", "caption[", "caption ` [ ]"] {
            let source = format!("[![alt]({dest} \"{title}\")](/viewer)");
            assert_eq!(
                parse_inlines(&source),
                vec![Inline::Link {
                    dest: "/viewer".into(),
                    title: None,
                    content: vec![Inline::Image {
                        dest: dest.into(),
                        title: Some(title.into()),
                        alt: "alt".into(),
                    }],
                }],
                "{source:?}"
            );
            assert!(fragment(&source).contains("<a href=\"/viewer\"><img "));
        }
    }
}

#[test]
fn accepted_images_flatten_child_links_without_disabling_their_outer_link() {
    let source = "[![before [inner](/inside \"`x\") after](image].png)](/outer)";
    assert_eq!(
        parse_inlines(source),
        vec![Inline::Link {
            dest: "/outer".into(),
            title: None,
            content: vec![Inline::Image {
                dest: "image].png".into(),
                title: None,
                alt: "before inner after".into(),
            }],
        }]
    );
    let targets: Vec<_> = destination_spans(source)
        .into_iter()
        .map(|span| span.dest)
        .collect();
    assert_eq!(targets, ["image].png", "/outer"]);
}

#[test]
fn rejected_nested_link_tails_remain_parseable_source() {
    for prefix in ["", "\\!"] {
        let source = format!("{prefix}[outer [inner](/inside)](/outside \"[other](/other)\")");
        let rendered = fragment(&source);
        assert!(rendered.contains("href=\"/inside\""), "{source:?}");
        assert!(rendered.contains("href=\"/other\""), "{source:?}");
        assert!(!rendered.contains("href=\"/outside\""), "{source:?}");
        assert!(!rendered.contains("<img"), "{source:?}");
        assert_eq!(rendered.matches("<a href=").count(), 2, "{source:?}");
        let targets: Vec<_> = destination_spans(&source)
            .into_iter()
            .map(|span| span.dest)
            .collect();
        assert_eq!(targets, ["/inside", "/other"], "{source:?}");
    }
}

#[test]
fn reference_links_and_angle_autolinks_still_defeat_an_outer_link() {
    for source in [
        "[outer [inner][ref]](/outside \"[other](/other)\")\n\n[ref]: /inside",
        "[outer <https://inside.test>](/outside \"[other](/other)\")",
    ] {
        let rendered = fragment(source);
        assert!(!rendered.contains("href=\"/outside\""), "{source:?}");
        assert!(rendered.contains("href=\"/other\""), "{source:?}");
        assert_eq!(rendered.matches("<a href=").count(), 2, "{source:?}");
    }
}

#[test]
fn full_reference_labels_are_literal_even_when_they_contain_markup_starters() {
    for label in ["ref `x", "ref $x", "ref *x", "réf \\] `x"] {
        let source = format!("[text][{label}] tail ` $ *\n\n[{label}]: /chosen\n");
        let document = parse_markdown(&source);
        let Block::Paragraph(inlines) = &document.blocks[0] else {
            panic!("expected reference paragraph: {source:?}: {document:?}");
        };
        assert_eq!(inlines[0], link("text", "/chosen", None), "{source:?}");
        assert_eq!(inlines[1], Inline::Text(" tail ` $ *".into()), "{source:?}");
        assert_eq!(inlines.len(), 2, "{source:?}: {inlines:?}");
    }
}

#[test]
fn malformed_following_labels_allow_shortcuts_but_unresolved_full_labels_do_not() {
    for suffix in ["[", "[broken", "[not[bar]]"] {
        let source = format!("[foo]{suffix}\n\n[foo]: /chosen\n");
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![Block::Paragraph(vec![
                link("foo", "/chosen", None),
                Inline::Text(suffix.into()),
            ])],
            "{source:?}"
        );
    }
    for suffix in ["[unknown]", "[ ]"] {
        let source = format!("[foo]{suffix}\n\n[foo]: /chosen\n");
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![Block::Paragraph(vec![Inline::Text(format!(
                "[foo]{suffix}"
            ))])],
            "{source:?}"
        );
    }
    let suffix = format!("[{}]", "a".repeat(1000));
    let source = format!("[foo]{suffix}\n\n[foo]: /chosen\n");
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![Block::Paragraph(vec![
            link("foo", "/chosen", None),
            Inline::Text(suffix),
        ])]
    );
}

#[test]
fn code_html_math_and_footnotes_keep_brackets_inside_their_atoms() {
    for (label, atom) in [
        ("`a]b`", Inline::Code("a]b".into())),
        ("$x]y$", Inline::Math("x]y".into())),
        ("$$x]y$$", Inline::DisplayMath("x]y".into())),
        ("<b title=\"]\">", Inline::Html("<b title=\"]\">".into())),
        ("[^n`q]", Inline::FootnoteRef { id: "n`q".into() }),
    ] {
        let source = format!("[{label}](/outer) [later](/later) `");
        assert_eq!(
            parse_inlines(&source),
            vec![
                Inline::Link {
                    dest: "/outer".into(),
                    title: None,
                    content: vec![atom],
                },
                Inline::Text(" ".into()),
                link("later", "/later", None),
                Inline::Text(" `".into()),
            ],
            "{source:?}"
        );
    }
    assert_eq!(
        parse_inlines("[^n]([later](/later))"),
        vec![
            Inline::FootnoteRef { id: "n".into() },
            Inline::Text("(".into()),
            link("later", "/later", None),
            Inline::Text(")".into()),
        ]
    );
}

#[test]
fn atomic_targets_preserve_safe_url_policy_and_title_attribute_escaping() {
    let source = "[blocked](javascript:alert(1) \"[decoy](/decoy) &quot; onmouseover=&quot;bad\") \
                  [safe](/safe \"<script>alert(1)</script>\") \
                  [![alt](javascript:alert(1) \"caption]\")](/viewer)";
    let rendered = fragment(source);
    assert!(!rendered.contains("href=\"javascript:"));
    assert!(!rendered.contains("src=\"javascript:"));
    assert!(!rendered.contains("href=\"/decoy\""));
    assert!(!rendered.contains(" onmouseover=\""));
    assert!(!rendered.contains("<script>"));
    assert!(rendered.contains("href=\"/safe\""));
    assert!(rendered.contains("href=\"/viewer\""));
    assert!(rendered.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[test]
fn unicode_destination_ranges_exclude_titles_and_retain_later_link_authority() {
    let source = "é [one](<images/café].png> \"caption ` ]\") [two](/über#part) `";
    let targets = destination_spans(source);
    assert_eq!(targets.len(), 2, "{targets:?}");
    assert_eq!(targets[0].dest, "images/café].png");
    assert_eq!(&source[targets[0].range.clone()], "<images/café].png>");
    assert_eq!(targets[1].dest, "/über#part");
    assert_eq!(&source[targets[1].range.clone()], "/über#part");
    assert_eq!(&source[targets[1].fragment_range(source).unwrap()], "part");
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
}

#[test]
fn all_container_and_cached_parse_entrypoints_retain_later_links() {
    let paragraph = "[first](/one \"`code\") [second](/two) `";
    let source = format!(
        "# {paragraph}\n\n> {paragraph}\n\n- {paragraph}\n\n{paragraph}\n\n{paragraph}\n\n{paragraph}"
    );
    let document = parse_markdown(&source);
    let rendered = fragment(&source);
    assert_eq!(rendered.matches("href=\"/one\"").count(), 6);
    assert_eq!(rendered.matches("href=\"/two\"").count(), 6);
    assert_eq!(parse_markdown_profiled(&source).document, document);
    assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    assert_eq!(destination_spans(&source).len(), 12);
}

#[test]
fn long_valid_sequences_keep_every_target_without_exhausting_speculative_work() {
    let source = "[a](/a`x) [b](/b \"title ` ]\") ".repeat(2048);
    let inlines = parse_inlines(&source);
    assert_eq!(
        inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::Link { .. }))
            .count(),
        4096
    );
    assert!(
        !inlines
            .iter()
            .any(|inline| matches!(inline, Inline::Code(_)))
    );
}

#[test]
fn bare_url_brackets_cannot_claim_targets_in_later_prose() {
    let body = "https://x.test/[a](/a \"[two](/two)\") [three](/three)";
    for (prefix, suffix, before) in [
        ("", "", false),
        ("[before](/before) ", "", true),
        ("[unmatched ", "", false),
        ("[unmatched ", "](/outside)", false),
    ] {
        let source = format!("{prefix}{body}{suffix}");
        let rendered = fragment(&source);
        assert!(
            rendered.contains("href=\"https://x.test/%5Ba%5D(/a\""),
            "{source:?}: {rendered}"
        );
        assert!(rendered.contains("href=\"/two\""), "{source:?}: {rendered}");
        assert!(
            rendered.contains("href=\"/three\""),
            "{source:?}: {rendered}"
        );
        assert!(!rendered.contains("href=\"/a\""), "{source:?}: {rendered}");
        assert!(
            !rendered.contains("href=\"/outside\""),
            "{source:?}: {rendered}"
        );
        assert_eq!(
            rendered.matches("<a href=").count(),
            if before { 4 } else { 3 },
            "{source:?}"
        );
        let targets: Vec<_> = destination_spans(&source)
            .into_iter()
            .map(|span| span.dest)
            .collect();
        let expected = if before {
            vec!["/before", "/two", "/three"]
        } else {
            vec!["/two", "/three"]
        };
        assert_eq!(targets, expected, "{source:?}");
    }
    // Bare autolinks are suppressed in explicit labels. An image's initial
    // bracket remains an image opener even if its text resembles a footnote.
    assert_eq!(
        parse_inlines("[https://x.test/a](/outer)"),
        vec![link("https://x.test/a", "/outer", None)]
    );
    assert_eq!(
        parse_inlines("![^n](image].png)"),
        vec![Inline::Image {
            dest: "image].png".into(),
            title: None,
            alt: "^n".into(),
        }]
    );
}

#[test]
fn long_mixed_urls_and_links_do_not_rescan_the_remaining_paragraph() {
    let source =
        "[first](/first) https://x.test/[a](/a \"[two](/two)\") [three](/three) ".repeat(2048);
    let inlines = parse_inlines(&source);
    assert_eq!(
        inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::Link { .. }))
            .count(),
        8192
    );
    assert!(
        !inlines
            .iter()
            .any(|inline| matches!(inline, Inline::Link { dest, .. } if dest == "/a"))
    );
}

#[test]
fn failed_long_targets_are_bounded_and_do_not_leak_destination_authority() {
    let hostile = format!("[discard](/discard) {}end", "[bad](".repeat(6000));
    assert_eq!(parse_inlines(&hostile), vec![Inline::Text(hostile.clone())]);
    let source = format!("{hostile}\n\n[good](/good)\n");
    let document = parse_markdown(&source);
    assert_eq!(
        document.blocks[0],
        Block::Paragraph(vec![Inline::Text(hostile)])
    );
    assert_eq!(
        document.blocks[1],
        Block::Paragraph(vec![link("good", "/good", None)])
    );
    let targets = destination_spans(&source);
    assert_eq!(targets.len(), 1, "{targets:?}");
    assert_eq!(targets[0].dest, "/good");
    assert_eq!(&source[targets[0].range.clone()], "/good");
}
