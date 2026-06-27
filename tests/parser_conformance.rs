//! Focused parser conformance regressions. Tests may unwrap for brevity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, render_html};

fn html(md: &str) -> String {
    render_html(md, &HtmlOptions::default()).unwrap()
}

fn html_allowing_raw(md: &str) -> String {
    render_html(
        md,
        &HtmlOptions {
            allow_raw_html: true,
            ..HtmlOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn setext_equals_underline_renders_level_one_heading() {
    let out = html("Main Title\n==========\n\nbody");

    assert!(out.contains("<h1 id=\"main-title\">Main Title</h1>"));
    assert!(out.contains("<p>body</p>"));
}

#[test]
fn setext_dash_underline_renders_level_two_heading_not_rule() {
    let out = html("Section Title\n-------------\n\nbody");

    assert!(out.contains("<h2 id=\"section-title\">Section Title</h2>"));
    assert!(!out.contains("<hr>"));
}

#[test]
fn setext_single_dash_after_paragraph_is_heading() {
    let out = html("Tiny\n-\n");

    assert!(out.contains("<h2 id=\"tiny\">Tiny</h2>"));
    assert!(!out.contains("<hr>"));
}

#[test]
fn thematic_break_without_paragraph_stays_thematic_break() {
    let out = html("---\n\nbody");

    assert!(out.contains("<hr>"));
    assert!(out.contains("<p>body</p>"));
}

#[test]
fn indented_dash_line_is_not_setext_underline() {
    let out = html("Not a heading\n    ---\n");

    assert!(!out.contains("<h2 id=\"not-a-heading\">"));
    assert!(out.contains("<p>Not a heading"));
}

#[test]
fn full_reference_link_definitions_are_collected_and_not_rendered() {
    let out = html("[docs]: https://example.com/docs \"Docs\"\n\nRead [the docs][docs].");

    assert!(out.contains("<a href=\"https://example.com/docs\" title=\"Docs\">the docs</a>"));
    assert!(!out.contains("[docs]:"));
}

#[test]
fn collapsed_and_shortcut_reference_links_resolve() {
    let out = html("[Guide]: https://example.com/guide\n\nRead [Guide][] and [Guide].");

    assert_eq!(
        out.matches("<a href=\"https://example.com/guide\">Guide</a>")
            .count(),
        2
    );
}

#[test]
fn reference_labels_are_case_insensitive_and_collapse_whitespace() {
    let out = html("[Multi   Word]: /ok\n\nSee [this][multi word].");

    assert!(out.contains("<a href=\"/ok\">this</a>"));
}

#[test]
fn first_reference_definition_wins() {
    let out = html("[id]: /first\n[id]: /second\n\nSee [id].");

    assert!(out.contains("<a href=\"/first\">id</a>"));
    assert!(!out.contains("/second"));
}

#[test]
fn malformed_reference_definitions_remain_visible_text() {
    let out = html("[bad]:\n\n[good]: /ok extra garbage\n\nUse [bad] and [good].");

    assert!(out.contains("<p>[bad]:</p>"));
    assert!(out.contains("[good]: /ok extra garbage"));
    assert!(out.contains("Use [bad] and [good]."));
}

#[test]
fn reference_images_resolve_alt_dest_and_title() {
    let out = html("[logo]: /logo.png 'Logo mark'\n\n![Project Logo][logo]");

    assert!(out.contains("<img src=\"/logo.png\" alt=\"Project Logo\" title=\"Logo mark\">"));
}

#[test]
fn lazy_list_item_continuation_stays_in_the_item() {
    let out = html("- first line\ncontinued without indentation\n- second");

    assert!(out.contains("<li>first line\ncontinued without indentation</li>"));
    assert!(out.contains("<li>second</li>"));
}

#[test]
fn nested_unordered_lists_render_as_nested_lists() {
    let out = html("- parent\n  - child\n- sibling");

    assert!(out.contains("<p>parent</p>\n<ul>\n<li>child</li>"));
    assert!(out.contains("<li>sibling</li>"));
}

#[test]
fn nested_ordered_lists_preserve_start_numbers() {
    let out = html("1. parent\n   3. child");

    assert!(out.contains("<ol start=\"3\">\n<li>child</li>"));
}

#[test]
fn task_list_items_preserve_lazy_continuation_text() {
    let out = html("- [x] done\ncontinues here");

    assert!(out.contains(
        "<li class=\"task\"><input type=\"checkbox\" disabled checked> done\ncontinues here</li>"
    ));
}

#[test]
fn blockquote_lists_can_contain_nested_lists() {
    let out = html("> - quoted\n>   - nested");

    assert!(out.contains("<blockquote>\n<ul>"));
    assert!(out.contains("<p>quoted</p>\n<ul>\n<li>nested</li>"));
}

#[test]
fn html_blocks_escape_by_default_and_pass_through_when_allowed() {
    let md = "<div class=\"note\">\n<strong>trusted</strong>\n</div>";
    let escaped = html(md);

    assert!(escaped.contains("&lt;div class=\"note\"&gt;"));
    assert!(escaped.contains("&lt;strong&gt;trusted&lt;/strong&gt;"));
    assert!(!escaped.contains("<div class=\"note\">"));

    let raw = html_allowing_raw(md);
    assert!(raw.contains("<div class=\"note\">\n<strong>trusted</strong>\n</div>\n"));
}

#[test]
fn inline_html_escapes_by_default_and_passes_through_when_allowed() {
    let md = "A <span class=\"pill\">trusted</span> word.";
    let escaped = html(md);

    assert!(escaped.contains("A &lt;span class=\"pill\"&gt;trusted&lt;/span&gt; word."));
    assert!(!escaped.contains("<span class=\"pill\">"));

    let raw = html_allowing_raw(md);
    assert!(raw.contains("A <span class=\"pill\">trusted</span> word."));
}

#[test]
fn malformed_angle_bracket_text_stays_escaped_text() {
    let out = html("2 < 3 and <not closed");

    assert!(out.contains("2 &lt; 3 and &lt;not closed"));
}

#[test]
fn uri_and_email_autolinks_render_with_commonmark_display_text() {
    let out = html("Visit <https://example.com/docs?q=1> or <team@example.com>.");

    assert!(
        out.contains("<a href=\"https://example.com/docs?q=1\">https://example.com/docs?q=1</a>")
    );
    assert!(out.contains("<a href=\"mailto:team@example.com\">team@example.com</a>"));
    assert!(!out.contains(">mailto:team@example.com</a>"));
}

#[test]
fn character_references_decode_named_decimal_and_hex_forms() {
    let out = html("AT&amp;T &copy; &#169; &#xA9; &#x1F680; &notaref;");

    assert!(out.contains("AT&amp;T"));
    assert!(out.contains("\u{a9} \u{a9} \u{a9} \u{1f680}"));
    assert!(out.contains("&amp;notaref;"));
}

#[test]
fn gfm_bare_urls_autolink_with_punctuation_outside() {
    let out = html("See https://example.com/a?b=1&c=2, then www.example.org.");

    assert!(out.contains(
        "<a href=\"https://example.com/a?b=1&amp;c=2\">https://example.com/a?b=1&amp;c=2</a>, then"
    ));
    assert!(out.contains("<a href=\"http://www.example.org\">www.example.org</a>."));
}

#[test]
fn gfm_bare_urls_require_a_reasonable_left_boundary() {
    let out = html("prefixhttps://example.com should stay text");

    assert!(!out.contains("<a href=\"https://example.com\""));
    assert!(out.contains("prefixhttps://example.com should stay text"));
}

#[test]
fn inline_links_support_balanced_destinations_and_title_forms() {
    let out = html(
        "[wiki](https://example.test/wiki/Markdown_(syntax)) \
         [nested](https://example.test/a(b(c)d)e) \
         [angle](<https://example.test/a b?q=1> 'Angle title') \
         [paren](dest (Paren title)) \
         [escaped](foo\\)bar \"T\\\"x\")",
    );

    assert!(out.contains("<a href=\"https://example.test/wiki/Markdown_(syntax)\">wiki</a>"));
    assert!(out.contains("<a href=\"https://example.test/a(b(c)d)e\">nested</a>"));
    assert!(
        out.contains("<a href=\"https://example.test/a b?q=1\" title=\"Angle title\">angle</a>")
    );
    assert!(out.contains("<a href=\"dest\" title=\"Paren title\">paren</a>"));
    assert!(out.contains("<a href=\"foo)bar\" title=\"T&quot;x\">escaped</a>"));
}

#[test]
fn inline_images_share_robust_destination_and_title_parsing() {
    let out = html("![alt](<images/final diagram.svg> 'Final diagram')");

    assert!(
        out.contains("<img src=\"images/final diagram.svg\" alt=\"alt\" title=\"Final diagram\">")
    );
}

#[test]
fn malformed_inline_link_destinations_remain_literal_text() {
    let out = html("[bad](foo(bar) and [angle](<broken)");

    assert!(out.contains("[bad](foo(bar) and [angle](&lt;broken)"));
    assert!(!out.contains("<a href=\"foo(bar\""));
    assert!(!out.contains("<a href=\"broken\""));
}

#[test]
fn top_level_indented_code_blocks_strip_one_code_indent() {
    let out = html("    let x = 1;\n        let y = 2;\n    <tag>\n\nnext");

    assert!(out.contains("<pre><code>let x = 1;\n    let y = 2;\n&lt;tag&gt;\n</code></pre>"));
    assert!(out.contains("<p>next</p>"));
}

#[test]
fn list_item_indentation_still_belongs_to_the_list_item() {
    let out = html("- item\n    still item text");

    assert!(out.contains("<li>item\n  still item text</li>"));
    assert!(!out.contains("<pre><code>still item text"));
}
