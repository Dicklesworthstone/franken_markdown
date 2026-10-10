//! Link-label precedence and bounded recursion across every parser entry point.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::parse_inlines;
use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, parse_markdown_spanned, render_html,
};

fn nested_links(depth: usize) -> String {
    format!("{}leaf{}", "[".repeat(depth), "](/target)".repeat(depth))
}

#[test]
fn repeatedly_rejected_nested_links_preserve_literal_source() {
    // Each outer link is rejected because its text contains an inner link. The
    // parser previously parsed those same inner candidates again at every '[':
    // just 32 levels took exponential work despite linear bracket matching.
    for depth in [32, 256, 4096] {
        let source = nested_links(depth);
        assert_eq!(parse_inlines(&source), vec![Inline::Text(source.clone())]);
        let html = render_html(&source, &HtmlOptions::default()).unwrap();
        assert!(html.contains(&format!("<p>{source}</p>")));
        assert!(!html.contains("<a href=\"/target\""));
    }
}

#[test]
fn nested_images_do_not_overflow_a_small_thread_stack() {
    // Image descriptions recursively parse and then flatten their children, so
    // the emphasis AST depth cap never protected this path. The former parser
    // overflowed the CLI stack on 1024 image descriptions (about 10 KiB).
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let source = format!("{}leaf{}", "![".repeat(2048), "](/image)".repeat(2048));
            assert_eq!(parse_inlines(&source), vec![Inline::Text(source.clone())]);
            let html = render_html(&source, &HtmlOptions::default()).unwrap();
            assert!(html.contains(&format!("<p>{source}</p>")));
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn nested_reference_links_share_the_same_work_bound() {
    let paragraph = format!("{}leaf{}", "[".repeat(32), "][ref]".repeat(32));
    let source = format!("[ref]: /target\n\n{paragraph}");
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![Block::Paragraph(vec![Inline::Text(paragraph)])]
    );
}

#[test]
fn exhausted_inline_work_does_not_affect_later_or_cached_paragraphs() {
    let hostile = nested_links(32);
    let source =
        format!("{hostile}\n\n[good](/ok)\n\n{hostile}\n\n[good](/ok)\n\n{hostile}\n\n[good](/ok)");
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 6);
    for (index, block) in document.blocks.iter().enumerate() {
        let expected = if index % 2 == 0 {
            Inline::Text(hostile.clone())
        } else {
            Inline::Link {
                dest: "/ok".into(),
                title: None,
                content: vec![Inline::Text("good".into())],
            }
        };
        assert_eq!(*block, Block::Paragraph(vec![expected]));
    }
    assert_eq!(parse_markdown_profiled(&source).document, document);
    let spanned = parse_markdown_spanned(&source);
    let blocks: Vec<_> = spanned.blocks.into_iter().map(|block| block.node).collect();
    assert_eq!(blocks, document.blocks);
}

#[test]
fn multiline_inline_runs_have_independent_recursion_budgets() {
    let hostile = nested_links(32);
    let paragraph = format!("before\n{hostile}\nafter");
    let source = format!("{paragraph}\n\n[good](/ok)");
    let document = parse_markdown(&source);
    assert_eq!(
        document.blocks[0],
        Block::Paragraph(vec![Inline::Text(paragraph)])
    );
    assert!(matches!(
        &document.blocks[1],
        Block::Paragraph(inlines) if matches!(&inlines[0], Inline::Link { dest, .. } if dest == "/ok")
    ));
}

#[test]
fn ordinary_nested_markup_and_long_flat_documents_keep_their_structure() {
    let source = "[outer **[inner](/inside)**](/outside) [![image](pic.png)](/page)";
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("[outer <strong><a href=\"/inside\">inner</a></strong>](/outside)"));
    assert!(html.contains("<a href=\"/page\"><img src=\"pic.png\" alt=\"image\"></a>"));

    let flat = "[**label**](/page) ".repeat(2048);
    let inlines = parse_inlines(&flat);
    assert_eq!(
        inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::Link { .. }))
            .count(),
        2048
    );
}

#[test]
fn explicit_link_destinations_win_over_bare_urls_and_emails_in_their_labels() {
    for label in [
        "https://example.com",
        "http://example.com/path?one=1&two=2",
        "www.example.com",
        "support@example.com",
        "write support@example.com or visit https://example.com",
    ] {
        let source = format!("[{label}](/chosen-destination \"Chosen title\")");
        assert_eq!(
            parse_inlines(&source),
            vec![Inline::Link {
                dest: "/chosen-destination".into(),
                title: Some("Chosen title".into()),
                content: vec![Inline::Text(label.into())],
            }],
            "bare autolinks must not invalidate an explicit link: {source}",
        );
    }
}

#[test]
fn reference_link_labels_keep_bare_urls_and_emails_as_text() {
    for label in [
        "https://example.com",
        "www.example.com",
        "support@example.com",
    ] {
        for (definition, use_site) in [
            ("[ref]: /chosen".to_string(), format!("[{label}][ref]")),
            (format!("[{label}]: /chosen"), format!("[{label}][]")),
            (format!("[{label}]: /chosen"), format!("[{label}]")),
        ] {
            let source = format!("{definition}\n\n{use_site}");
            assert_eq!(
                parse_markdown(&source).blocks,
                vec![Block::Paragraph(vec![Inline::Link {
                    dest: "/chosen".into(),
                    title: None,
                    content: vec![Inline::Text(label.into())],
                }])],
                "reference link lost its destination: {source}",
            );
        }
    }
}

#[test]
fn rich_link_labels_preserve_formatting_without_creating_nested_autolinks() {
    let source = "[**visit https://example.com** and ~~write support@example.com~~](/contact)";
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains(
        "<a href=\"/contact\"><strong>visit https://example.com</strong> and <del>write support@example.com</del></a>",
    ));
    assert_eq!(html.matches("<a href=").count(), 1);

    let reference = format!(
        "[ref]: /contact\n\n{}[ref]",
        source.trim_end_matches("(/contact)")
    );
    assert_eq!(parse_markdown(source), parse_markdown(&reference));
}

#[test]
fn explicit_nested_links_and_angle_autolinks_still_defeat_an_outer_link() {
    for (source, inner_destination) in [
        ("[outer [inner](/inside)](/outside)", "/inside"),
        (
            "[outer <https://inside.example>](/outside)",
            "https://inside.example",
        ),
        (
            "[outer ~~<inside@example.com>~~](/outside)",
            "mailto:inside@example.com",
        ),
    ] {
        let html = render_html(source, &HtmlOptions::default()).unwrap();
        assert!(html.contains(&format!("<a href=\"{inner_destination}\">")));
        assert!(!html.contains("href=\"/outside\""));
        assert!(html.contains("](/outside)"));
    }
}

#[test]
fn image_descriptions_keep_url_text_and_can_still_be_wrapped_in_links() {
    for label in ["https://example.com", "support@example.com"] {
        let source = format!("[![{label}](image.svg)](/chosen)");
        assert_eq!(
            parse_inlines(&source),
            vec![Inline::Link {
                dest: "/chosen".into(),
                title: None,
                content: vec![Inline::Image {
                    dest: "image.svg".into(),
                    title: None,
                    alt: label.into(),
                }],
            }],
        );
        let reference = format!("[image]: image.svg\n\n[![{label}][image]](/chosen)");
        assert_eq!(parse_markdown(&source), parse_markdown(&reference));
    }
}

#[test]
fn bare_autolinks_remain_active_after_successful_and_rejected_link_labels() {
    let source = "[https://label.example](/chosen) https://outside.example\n\n\
                  [outer [inner](/inside)](/outside) support@example.com\n\n\
                  [https://label.example](/chosen) https://outside.example\n\n\
                  [https://label.example](/chosen) https://outside.example";
    let document = parse_markdown(source);
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert_eq!(html.matches("href=\"/chosen\"").count(), 3);
    assert_eq!(html.matches("href=\"https://outside.example\"").count(), 3);
    assert_eq!(
        html.matches("href=\"mailto:support@example.com\"").count(),
        1
    );
    assert!(!html.contains("href=\"/outside\""));
    assert_eq!(document.blocks[0], document.blocks[2]);
    assert_eq!(document.blocks[0], document.blocks[3]);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(
        parse_markdown_spanned(source)
            .blocks
            .into_iter()
            .map(|block| block.node)
            .collect::<Vec<_>>(),
        document.blocks,
    );
}
