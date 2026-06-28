//! Focused parser conformance regressions. Tests may unwrap for brevity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, render_html, scan_markdown_line,
};

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
fn scalar_line_scanner_is_conservative_for_markdown_starters() {
    let heading = scan_markdown_line("  ## Title");
    assert!(heading.maybe_heading_marker);
    assert_eq!(heading.first_special_byte, Some(2));

    let table_header = scan_markdown_line("| a | `b|c` |");
    assert!(table_header.contains_pipe);
    assert!(table_header.contains_backtick);

    let delimiter = scan_markdown_line("|---|:---:|---:|");
    assert!(delimiter.maybe_table_delimiter);
    assert!(delimiter.contains_pipe);

    let setext_equals = scan_markdown_line("==========");
    assert!(setext_equals.maybe_setext_underline);
    assert_eq!(setext_equals.first_special_byte, Some(0));

    let reference = scan_markdown_line("\t[label]: /dest");
    assert!(reference.maybe_reference);

    let raw_html = scan_markdown_line("text <span>ok</span>");
    assert!(raw_html.maybe_html);
    assert!(raw_html.maybe_autolink);

    let fence = scan_markdown_line("   ```rust");
    assert!(fence.maybe_fence);
    assert!(fence.contains_backtick);

    let list = scan_markdown_line("123. ordered");
    assert!(list.maybe_list_marker);

    let plain = scan_markdown_line("plain words only");
    assert_eq!(plain, Default::default());
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
fn multiline_reference_titles_resolve_and_do_not_render() {
    let out = html(
        "[double]: /double\n  \"Double title\"\n\
         [single]: /single\n'Single title'\n\
         [paren]: /paren\n(Paren title)\n\n\
         [double] [single] [paren]",
    );

    assert!(out.contains("<a href=\"/double\" title=\"Double title\">double</a>"));
    assert!(out.contains("<a href=\"/single\" title=\"Single title\">single</a>"));
    assert!(out.contains("<a href=\"/paren\" title=\"Paren title\">paren</a>"));
    assert!(!out.contains("<p>&quot;Double title&quot;</p>"));
    assert!(!out.contains("<p>'Single title'</p>"));
    assert!(!out.contains("<p>(Paren title)</p>"));
}

#[test]
fn multiline_reference_titles_preserve_first_definition_wins() {
    let out = html("[id]: /first\n\"First\"\n[id]: /second\n\"Second\"\n\nSee [id].");

    assert!(out.contains("<a href=\"/first\" title=\"First\">id</a>"));
    assert!(!out.contains("/second"));
    assert!(!out.contains("Second"));
}

#[test]
fn malformed_multiline_reference_title_remains_visible_text() {
    let out = html("[id]: /ok\n\"unterminated\n\nSee [id].");

    assert!(out.contains("<a href=\"/ok\">id</a>"));
    assert!(out.contains("<p>\"unterminated</p>"));
}

#[test]
fn four_space_reference_title_line_is_not_consumed() {
    let out = html("[id]: /ok\n    \"not a title\"\n\nSee [id].");

    assert!(out.contains("<a href=\"/ok\">id</a>"));
    assert!(out.contains("<pre><code>\"not a title\"\n</code></pre>"));
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

    assert!(out.contains("<li>parent\n<ul>\n<li>child</li>"));
    assert!(out.contains("<li>sibling</li>"));
}

#[test]
fn nested_ordered_lists_preserve_start_numbers() {
    let out = html("1. parent\n   3. child");

    assert!(out.contains("<ol start=\"3\">\n<li>child</li>"));
}

#[test]
fn ordered_list_start_other_than_one_does_not_interrupt_paragraph() {
    let out = html("The year\n1986. was memorable");

    assert!(out.contains("<p>The year\n1986. was memorable</p>"));
    assert!(!out.contains("<ol"));
}

#[test]
fn ordered_list_start_other_than_one_can_start_a_block() {
    let out = html("1986. was memorable");

    assert!(out.contains("<ol start=\"1986\">"));
    assert!(out.contains("<li>was memorable</li>"));
}

#[test]
fn lazy_ordered_marker_start_other_than_one_stays_in_list_paragraph() {
    let out = html("- The year\n1986. was memorable");

    assert!(out.contains("<li>The year\n1986. was memorable</li>"));
    assert!(!out.contains("<ol start=\"1986\">"));
}

#[test]
fn empty_list_markers_are_valid_empty_items() {
    let unordered = html("-\n- filled");
    let ordered = html("1.\n2. filled");

    assert!(unordered.contains("<ul>\n<li></li>\n<li>filled</li>\n</ul>"));
    assert!(ordered.contains("<ol>\n<li></li>\n<li>filled</li>\n</ol>"));
}

#[test]
fn tab_after_list_marker_starts_list_item_text() {
    let out = html("-\ttabbed\n1.\tordered");

    assert!(out.contains("<ul>\n<li>tabbed</li>\n</ul>"));
    assert!(out.contains("<ol>\n<li>ordered</li>\n</ol>"));
}

#[test]
fn list_markers_without_padding_stay_paragraph_text() {
    let out = html("-not a list\n1.not a list");

    assert!(out.contains("<p>-not a list\n1.not a list</p>"));
    assert!(!out.contains("<ul>"));
    assert!(!out.contains("<ol>"));
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
    assert!(out.contains("<li>quoted\n<ul>\n<li>nested</li>"));
}

#[test]
fn gfm_table_requires_header_and_delimiter_width_to_match() {
    let out = html("Name | Score\n--- | --- | ---\nthis stays paragraph");

    assert!(!out.contains("<table>"));
    assert!(out.contains("<p>Name | Score\n--- | --- | ---\nthis stays paragraph</p>"));
}

#[test]
fn gfm_table_body_rows_still_pad_and_truncate_to_header_width() {
    let out = html("Name | Score\n--- | ---\nalpha |\nbeta | 20 | ignored");

    assert!(out.contains("<table>"));
    assert!(out.contains("<tr><td>alpha</td><td></td></tr>"));
    assert!(out.contains("<tr><td>beta</td><td>20</td></tr>"));
    assert!(!out.contains("ignored"));
}

#[test]
fn gfm_table_pipes_inside_code_spans_stay_in_the_cell() {
    let out = html("Name | Expr\n--- | ---\nalpha | `a|b`\nbeta | ``x|y``");

    assert!(out.contains("<tr><td>alpha</td><td><code>a|b</code></td></tr>"));
    assert!(out.contains("<tr><td>beta</td><td><code>x|y</code></td></tr>"));
}

#[test]
fn gfm_table_escaped_pipes_still_stay_in_the_cell() {
    let out = html("Name | Expr\n--- | ---\nalpha | a \\| b");

    assert!(out.contains("<tr><td>alpha</td><td>a | b</td></tr>"));
}

#[test]
fn gfm_table_escaped_backticks_do_not_hide_cell_pipes() {
    let out = html("A | B\n--- | ---\n\\` | right");

    assert!(out.contains("<tr><td>`</td><td>right</td></tr>"));
    assert!(!out.contains("<td>` | right</td><td></td>"));
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
fn intraword_underscores_remain_literal_text() {
    let out = html("foo_bar_baz foo__bar__baz foo___bar___baz");

    assert!(out.contains("<p>foo_bar_baz foo__bar__baz foo___bar___baz</p>"));
    assert!(!out.contains("<em>"));
    assert!(!out.contains("<strong>"));
}

#[test]
fn underscore_emphasis_still_works_at_word_boundaries() {
    let out = html("_em_ and __strong__ and _foo_bar_");

    assert!(out.contains("<em>em</em>"));
    assert!(out.contains("<strong>strong</strong>"));
    assert!(out.contains("<em>foo_bar</em>"));
}

#[test]
fn unmatched_closing_emphasis_delimiters_remain_literal_text() {
    for (md, expected) in [
        ("a*", "<p>a*</p>"),
        ("a**", "<p>a**</p>"),
        ("a_", "<p>a_</p>"),
        ("a__", "<p>a__</p>"),
    ] {
        let out = html(md);
        assert!(out.contains(expected), "{md:?} should render as {expected}");
    }
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
fn profiled_parser_matches_normal_ast_and_reports_required_stages() {
    let src = "# Profiled\n\n\
               A paragraph with **strong** text and [ref][id].\n\n\
               [id]: https://example.com \"Example\"\n\n\
               | Name | Value |\n|---|---:|\n| alpha | 1 |\n\n\
               - [x] task\n  - nested\n\n\
               ```rust\nfn main() {}\n```\n";

    let normal = parse_markdown(src);
    let profiled = parse_markdown_profiled(src);

    assert_eq!(profiled.document, normal);

    let stages: BTreeSet<&str> = profiled.stages.iter().map(|stage| stage.stage).collect();
    for required in [
        "line_split",
        "reference_collection",
        "block_parse_total",
        "inline_parse",
        "heading_block",
        "paragraph_block",
        "table_block",
        "list_block",
        "fenced_code_block",
    ] {
        assert!(stages.contains(required), "missing parser stage {required}");
    }

    assert!(
        profiled.stages.iter().any(|stage| stage.allocations > 0),
        "parser profiling should report approximate allocation/object counts"
    );
}

#[test]
fn profiled_parser_does_not_charge_single_line_paragraphs_for_join_allocation() {
    let profiled = parse_markdown_profiled("alpha\n\nbeta");
    let paragraph_allocations: Vec<usize> = profiled
        .stages
        .iter()
        .filter(|stage| stage.stage == "paragraph_block")
        .map(|stage| stage.allocations)
        .collect();

    assert_eq!(paragraph_allocations, vec![2, 2]);

    let joined = parse_markdown_profiled("alpha\nbeta");
    let joined_paragraph = joined
        .stages
        .iter()
        .find(|stage| stage.stage == "paragraph_block")
        .expect("multi-line paragraph should report a paragraph stage");
    assert_eq!(joined_paragraph.allocations, 5);
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
fn gfm_bare_urls_leave_unmatched_closing_parens_outside() {
    let out = html("See (https://example.com/a).");

    assert!(out.contains("(<a href=\"https://example.com/a\">https://example.com/a</a>)."));
    assert!(!out.contains("href=\"https://example.com/a)\""));
}

#[test]
fn gfm_bare_urls_keep_balanced_parentheses_inside() {
    let out = html("See https://example.com/a(b).");

    assert!(out.contains("<a href=\"https://example.com/a(b)\">https://example.com/a(b)</a>."));
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

#[test]
fn four_space_atx_heading_is_indented_code_not_a_heading() {
    let out = html("    # not a heading\n\nnext");

    assert!(out.contains("<pre><code># not a heading\n</code></pre>"));
    assert!(!out.contains("<h1"));
    assert!(out.contains("<p>next</p>"));
}

#[test]
fn four_space_fence_is_indented_code_not_a_fenced_code_block() {
    let out = html("    ```rust\n    let x = 1;\n    ```\n");

    assert!(out.contains("<pre><code>```rust\nlet x = 1;\n```\n</code></pre>"));
    assert!(!out.contains("class=\"language-rust\""));
}

#[test]
fn four_space_thematic_break_is_indented_code_not_a_rule() {
    let out = html("    ---\n\nnext");

    assert!(out.contains("<pre><code>---\n</code></pre>"));
    assert!(!out.contains("<hr>"));
    assert!(out.contains("<p>next</p>"));
}

#[test]
fn atx_closing_hashes_must_be_space_separated() {
    let out = html("# title#\n\n# title ###\n\n# ###\n");

    assert!(out.contains("<h1 id=\"title\">title#</h1>"));
    assert!(out.contains("<h1 id=\"title-2\">title</h1>"));
    assert!(out.contains("<h1 id=\"section\"></h1>"));
}

#[test]
fn fenced_code_closer_allows_three_spaces_but_not_four() {
    let out = html("```text\ninside\n   ```\n\n```text\ninside\n    ```\nstill code\n");

    assert!(out.contains("<pre><code class=\"language-text\">inside\n</code></pre>"));
    assert!(out.contains("inside\n    ```\nstill code\n</code></pre>"));
}

#[test]
fn blockquote_paragraph_continues_lazily() {
    let out = html("> quoted line one\nlazy continuation");

    assert!(out.contains("<blockquote>\n<p>quoted line one\nlazy continuation</p>\n</blockquote>"));
}

#[test]
fn blockquote_lazy_continuation_stops_at_blank_or_block_starter() {
    let blanked = html("> quoted\nlazy\n\noutside");
    assert!(blanked.contains("<p>quoted\nlazy</p>"));
    assert!(blanked.contains("</blockquote>\n<p>outside</p>"));

    let heading = html("> quoted\n# Heading");
    assert!(heading.contains("<blockquote>\n<p>quoted</p>\n</blockquote>"));
    assert!(heading.contains("<h1 id=\"heading\">Heading</h1>"));
}

#[test]
fn bare_email_becomes_mailto_autolink() {
    assert!(html("Contact a@b.com today").contains("<a href=\"mailto:a@b.com\">a@b.com</a>"));
    assert!(
        html("me.first+tag@sub.example.org")
            .contains("<a href=\"mailto:me.first+tag@sub.example.org\">")
    );
}

#[test]
fn bare_email_autolink_is_conservative() {
    // No dot in the domain is not treated as an email.
    assert!(!html("user@localhost stays plain").contains("mailto:"));
    // A trailing sentence period is not part of the address.
    let out = html("Mail a@b.com.");
    assert!(out.contains("<a href=\"mailto:a@b.com\">a@b.com</a>"));
    assert!(out.contains("</a>."));
}

#[test]
fn even_length_emphasis_runs_are_bold_not_italic() {
    // Four delimiters pair entirely into strong (bold), never nested emphasis.
    assert!(html("****x****").contains("<p><strong><strong>x</strong></strong></p>"));
    // Three delimiters keep the strong-outer / emphasis-inner shape (unchanged).
    assert!(html("***x***").contains("<p><strong><em>x</em></strong></p>"));
    // Five delimiters: bold wrappers with a single inner emphasis (odd leftover).
    assert!(html("*****x*****").contains("<strong><strong><em>x</em></strong></strong>"));
}

#[test]
fn blank_separated_blocks_loosen_only_their_own_list() {
    // A second paragraph at the item's content column loosens that item's list.
    assert!(html("- a\n\n  b").contains("<li><p>a</p>\n<p>b</p>\n</li>"));
    // A blank inside a sub-list item loosens the INNER list, leaving the outer
    // list tight.
    let nested = html("- a\n  - b\n\n    cont");
    assert!(nested.contains("<li>a\n<ul>"));
    assert!(nested.contains("<li><p>b</p>\n<p>cont</p>"));
}
