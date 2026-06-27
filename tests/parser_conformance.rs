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
