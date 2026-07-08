//! Focused parser conformance regressions. Tests may unwrap for brevity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::parse_inlines;
use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, parse_markdown_spanned, render_html,
    scan_markdown_line,
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
fn alphabetic_pipe_table_before_reference_keeps_reference_boundary() {
    let out = html(
        "Name | Value\n\
         --- | ---\n\
         alpha | beta\n\n\
         [id]: /ok\n\n\
         See [id].",
    );

    assert!(out.contains("<table>"));
    assert!(out.contains("<a href=\"/ok\">id</a>"));
    assert!(!out.contains("[id]:"));
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
fn multiline_paragraph_inline_handoff_preserves_soft_breaks_and_references() {
    let doc = parse_markdown("Title\n===\n[ref]: /dest \"T\"\n\n*a\n[ref]*");

    assert_eq!(
        doc.blocks,
        vec![
            Block::Heading {
                level: 1,
                inlines: vec![Inline::Text("Title".to_string())],
            },
            Block::Paragraph(vec![Inline::Emphasis(vec![
                Inline::Text("a".to_string()),
                Inline::SoftBreak,
                Inline::Link {
                    dest: "/dest".to_string(),
                    title: Some("T".to_string()),
                    content: vec![Inline::Text("ref".to_string())],
                },
            ])]),
        ]
    );
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
fn profiled_parser_counts_paragraph_handoff_allocations() {
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
    assert_eq!(joined_paragraph.allocations, 4);
}

#[test]
fn profiled_parser_reference_collection_skips_work_when_no_definition_candidate_exists() {
    let src = "# Title\n\nplain paragraph\n\n    indented code\n";
    let profiled = parse_markdown_profiled(src);

    assert_eq!(profiled.document, parse_markdown(src));
    let reference_stage = profiled
        .stages
        .iter()
        .find(|stage| stage.stage == "reference_collection")
        .expect("profiled parse should report reference collection");
    assert_eq!(reference_stage.count, 0);
    assert_eq!(reference_stage.allocations, 0);
}

#[test]
fn profiled_parser_plain_inline_fast_path_skips_tokenizer_without_losing_autolinks() {
    let profiled = parse_markdown_profiled("plain words only");
    let inline_stage = profiled
        .stages
        .iter()
        .find(|stage| stage.stage == "inline_parse")
        .expect("plain paragraph should still report inline parse");
    assert_eq!(inline_stage.count, "plain words only".chars().count());
    assert_eq!(inline_stage.allocations, 1);
    assert_eq!(
        profiled.document.blocks,
        vec![Block::Paragraph(vec![Inline::Text(
            "plain words only".to_string()
        )])]
    );

    let autolinked = html("See www.example.org");
    assert!(autolinked.contains("<a href=\"http://www.example.org\">www.example.org</a>"));
}

#[test]
fn profiled_full_inline_parse_without_emphasis_skips_resolver_state() {
    let src = "before `code` &amp; <https://example.test> user@example.test <!-- note --> after";
    let profiled = parse_markdown_profiled(src);
    let inline_stage = profiled
        .stages
        .iter()
        .find(|stage| stage.stage == "inline_parse")
        .expect("full inline parse should report inline stage");

    assert_eq!(inline_stage.count, src.chars().count());
    assert_eq!(
        inline_stage.allocations, 11,
        "no-emphasis full parse should count only char/token/output state plus output nodes"
    );
    assert_eq!(
        profiled.document.blocks,
        vec![Block::Paragraph(vec![
            Inline::Text("before ".to_string()),
            Inline::Code("code".to_string()),
            Inline::Text(" & ".to_string()),
            Inline::Link {
                dest: "https://example.test".to_string(),
                title: None,
                content: vec![Inline::Text("https://example.test".to_string())],
            },
            Inline::Text(" ".to_string()),
            Inline::Link {
                dest: "mailto:user@example.test".to_string(),
                title: None,
                content: vec![Inline::Text("user@example.test".to_string())],
            },
            Inline::Text(" ".to_string()),
            Inline::Html("<!-- note -->".to_string()),
            Inline::Text(" after".to_string()),
        ])]
    );
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
    // Three delimiters: strong is consumed first (inner), emphasis is the leftover
    // outer wrapper — CommonMark takes the delimiters nearest the content first.
    assert!(html("***x***").contains("<p><em><strong>x</strong></em></p>"));
    // Five delimiters: two strong wrappers (inner) with a single outer emphasis.
    assert!(html("*****x*****").contains("<em><strong><strong>x</strong></strong></em>"));
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

// ---------------------------------------------------------------------------
// grn.2.5 mock-free coverage: real Markdown strings exercising parser edge
// cases. Each test names the construct it targets.
// ---------------------------------------------------------------------------

/// Parse `md` and assert it produced exactly one top-level block, returning it.
fn only_block(md: &str) -> Block {
    let mut doc = parse_markdown(md);
    assert_eq!(doc.blocks.len(), 1, "expected exactly one block for {md:?}");
    doc.blocks.remove(0)
}

/// Pull the inline children of a single-paragraph document.
fn paragraph_inlines(md: &str) -> Vec<Inline> {
    match only_block(md) {
        Block::Paragraph(inlines) => inlines,
        other => panic!("expected a paragraph for {md:?}, got {other:?}"),
    }
}

#[test]
fn spanned_html_block_spans_every_non_blank_line() {
    // The spanned-block collector walks an HTML block until the next blank line,
    // so a multi-line `<div>` yields one span covering all of its lines.
    let src = "<div>\nstill html\nmore html\n\nafter";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(
        doc.blocks[0].span.slice(src).unwrap(),
        "<div>\nstill html\nmore html"
    );
    assert!(matches!(doc.blocks[0].node, Block::HtmlBlock(_)));
    assert_eq!(doc.blocks[1].span.slice(src).unwrap(), "after");
}

#[test]
fn spanned_paragraph_span_stops_at_interrupting_block_starter() {
    // A heading on the line directly after prose interrupts the paragraph; the
    // spanned collector must end the paragraph span before the heading.
    let src = "intro paragraph\n# Heading\n\nbody";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "intro paragraph");
    assert!(matches!(doc.blocks[0].node, Block::Paragraph(_)));
    assert_eq!(doc.blocks[1].span.slice(src).unwrap(), "# Heading");
    assert!(matches!(
        doc.blocks[1].node,
        Block::Heading { level: 1, .. }
    ));
    assert_eq!(doc.blocks[2].span.slice(src).unwrap(), "body");
}

#[test]
fn ascii_letter_paragraph_lines_do_not_swallow_later_interrupts() {
    let doc = parse_markdown("alpha\nbeta\n# Heading\n\nbody");

    assert_eq!(doc.blocks.len(), 3);
    match &doc.blocks[0] {
        Block::Paragraph(inlines) => assert_eq!(
            inlines,
            &[
                Inline::Text("alpha".into()),
                Inline::SoftBreak,
                Inline::Text("beta".into())
            ]
        ),
        other => panic!("expected initial prose paragraph, got {other:?}"),
    }
    assert!(matches!(doc.blocks[1], Block::Heading { level: 1, .. }));
    match &doc.blocks[2] {
        Block::Paragraph(inlines) => assert_eq!(inlines, &[Inline::Text("body".into())]),
        other => panic!("expected trailing body paragraph, got {other:?}"),
    }
}

#[test]
fn plain_multiline_paragraph_preserves_soft_and_hard_breaks() {
    assert_eq!(
        paragraph_inlines("alpha \nbeta  \ngamma"),
        vec![
            Inline::Text("alpha".into()),
            Inline::SoftBreak,
            Inline::Text("beta".into()),
            Inline::HardBreak,
            Inline::Text("gamma".into()),
        ]
    );
}

#[test]
fn multiline_paragraph_with_inline_syntax_uses_full_inline_parser() {
    assert_eq!(
        paragraph_inlines("alpha\n*beta*"),
        vec![
            Inline::Text("alpha".into()),
            Inline::SoftBreak,
            Inline::Emphasis(vec![Inline::Text("beta".into())]),
        ]
    );
}

#[test]
fn reference_definition_with_empty_angle_destination_is_rejected() {
    // `<>` is an empty angle destination, so the line is not a valid reference
    // definition and stays as visible paragraph text; `[a]` does not resolve.
    let out = html("[a]: <>\n\nUse [a].");

    assert!(out.contains("[a]: &lt;&gt;"));
    assert!(out.contains("Use [a]."));
    assert!(!out.contains("<a href"));
}

#[test]
fn reference_definition_accepts_parenthesized_title() {
    // A parenthesized title on the same line as the definition resolves.
    let out = html("[ref]: /url (Paren title)\n\nSee [ref].");

    assert!(out.contains("<a href=\"/url\" title=\"Paren title\">ref</a>"));
}

#[test]
fn reference_definition_with_unterminated_title_quote_stays_text() {
    // The opening title quote is never closed, so the definition is malformed and
    // remains literal text instead of resolving a link.
    let out = html("[ref]: /url \"open\n\nSee [ref].");

    assert!(out.contains("/url \"open"));
    assert!(!out.contains("<a href=\"/url\""));
}

#[test]
fn reference_definition_with_trailing_junk_after_title_stays_text() {
    // Trailing non-space content after a complete title invalidates the
    // definition.
    let out = html("[ref]: /url \"Title\" junk\n\nSee [ref].");

    assert!(out.contains("[ref]: /url"));
    assert!(!out.contains("<a href=\"/url\""));
}

#[test]
fn atx_heading_requires_a_space_after_the_hashes() {
    // `#text` (no space) is not a heading; it stays paragraph text.
    assert_eq!(
        paragraph_inlines("#nospace"),
        vec![Inline::Text("#nospace".to_string())]
    );
}

#[test]
fn backtick_fence_info_string_may_not_contain_a_backtick() {
    // ```` ```js`x ```` has a backtick inside its info string, so it is not a
    // fenced code block — it is paragraph text.
    assert!(matches!(only_block("```js`x"), Block::Paragraph(_)));
}

#[test]
fn indented_code_block_keeps_internal_blank_lines() {
    // Blank lines between indented code lines stay inside the code block, and a
    // following non-indented line ends it.
    let blocks = parse_markdown("    a\n\n\n    b\nx").blocks;
    assert_eq!(blocks.len(), 2);
    match &blocks[0] {
        Block::CodeBlock { lang, code } => {
            assert_eq!(*lang, None);
            assert_eq!(code, "a\n\n\nb\n");
        }
        other => panic!("expected code block, got {other:?}"),
    }
    assert_eq!(
        blocks[1],
        Block::Paragraph(vec![Inline::Text("x".to_string())])
    );
}

#[test]
fn blockquote_heading_is_not_lazily_continued() {
    // The previous quoted line is a heading (not an open paragraph), so a
    // following bare line ends the quote instead of lazily continuing it.
    let out = html("> # Quoted heading\nplain line");

    assert!(
        out.contains("<blockquote>\n<h1 id=\"quoted-heading\">Quoted heading</h1>\n</blockquote>")
    );
    assert!(out.contains("<p>plain line</p>"));
}

#[test]
fn html_block_recognizes_comments_declarations_and_processing_instructions() {
    // A comment that closes on its own line is a single HTML block; per
    // CommonMark (type 2) it ends at the line containing `-->`, so any text
    // after that line is its own block.
    assert!(matches!(
        only_block("<!-- a comment -->"),
        Block::HtmlBlock(_)
    ));
    let after = parse_markdown("<!-- a comment -->\nstill text");
    assert_eq!(
        after.blocks.len(),
        2,
        "text after the close marker is separate"
    );
    assert!(matches!(after.blocks[0], Block::HtmlBlock(_)));

    // A comment whose `-->` is on a later line runs across the intervening
    // lines — including blank lines — until the close marker (the fix for the
    // previous bug that split type 1/2 blocks at the first blank line).
    let spanning = parse_markdown("<!-- a\n\nb -->");
    assert_eq!(
        spanning.blocks.len(),
        1,
        "comment must span across the blank line to -->"
    );
    assert!(matches!(spanning.blocks[0], Block::HtmlBlock(_)));

    assert!(matches!(only_block("<!DOCTYPE html>"), Block::HtmlBlock(_)));
    assert!(matches!(
        only_block("<?php echo 1; ?>"),
        Block::HtmlBlock(_)
    ));
}

#[test]
fn html_block_tag_classification_handles_late_alphabet_and_unknown_tags() {
    // Tags late in the block-tag table are recognized as HTML blocks.
    assert!(matches!(
        only_block("<table>\n<tr><td>x</td></tr>\n</table>"),
        Block::HtmlBlock(_)
    ));
    assert!(matches!(
        only_block("<ul>\n<li>x</li>\n</ul>"),
        Block::HtmlBlock(_)
    ));
    // An unknown tag name forces the classifier to evaluate every alternative and
    // fall through to "not a block tag", so the line is an ordinary paragraph.
    assert!(matches!(
        only_block("<videocustomtag>not a block</videocustomtag>"),
        Block::Paragraph(_)
    ));
}

#[test]
fn list_with_multiple_blank_lines_between_items_is_loose() {
    // Two blank lines between same-level items still produce one loose list.
    match only_block("- a\n\n\n- b") {
        Block::List(list) => {
            assert!(!list.tight, "blank-separated items should loosen the list");
            assert_eq!(list.items.len(), 2);
        }
        other => panic!("expected a list, got {other:?}"),
    }
}

#[test]
fn pipe_line_followed_by_non_delimiter_is_not_a_table() {
    // The second line has no `-`, so it is not a delimiter row: the two pipe lines
    // form a single paragraph, not a table.
    assert!(matches!(only_block("a | b\nc | d"), Block::Paragraph(_)));
    assert!(!html("a | b\nc | d").contains("<table>"));
}

#[test]
fn public_parse_inlines_entry_point_resolves_emphasis() {
    let inlines = parse_inlines("plain *em* text");

    assert_eq!(
        inlines,
        vec![
            Inline::Text("plain ".to_string()),
            Inline::Emphasis(vec![Inline::Text("em".to_string())]),
            Inline::Text(" text".to_string()),
        ]
    );
}

#[test]
fn unclosed_code_span_backtick_is_literal_text() {
    // No closing backtick: the run degrades to literal text, no code span.
    let inlines = parse_inlines("a `b c");
    assert_eq!(inlines, vec![Inline::Text("a `b c".to_string())]);
    assert!(!inlines.iter().any(|i| matches!(i, Inline::Code(_))));
}

#[test]
fn code_span_strips_one_surrounding_padding_space() {
    // A single space immediately inside both delimiters is stripped.
    assert_eq!(parse_inlines("` a `"), vec![Inline::Code("a".to_string())]);
}

#[test]
fn strikethrough_delimiter_edge_cases() {
    // Valid strikethrough, including a lone interior `~` that is not a closer.
    assert_eq!(
        parse_inlines("~~a~b~~"),
        vec![Inline::Strikethrough(vec![Inline::Text("a~b".to_string())])]
    );
    // A space right after the opener prevents a strikethrough run.
    let spaced = parse_inlines("~~ x~~");
    assert!(!spaced.iter().any(|i| matches!(i, Inline::Strikethrough(_))));
    // An unterminated run stays literal.
    let unterminated = parse_inlines("~~foo");
    assert_eq!(unterminated, vec![Inline::Text("~~foo".to_string())]);
}

#[test]
fn inline_link_with_trailing_junk_after_title_is_literal() {
    // `[a](b "t" x)` has stray content after the title before `)`, so it is not a
    // link.
    let out = html("[a](b \"t\" x)");
    assert!(!out.contains("<a href"));
    assert!(out.contains("[a](b"));
}

#[test]
fn angle_link_destination_escapes_and_rejects() {
    // Backslash-escaped punctuation inside an angle destination is unescaped.
    assert!(html("[esc](<a\\)b>)").contains("<a href=\"a)b\">esc</a>"));
    // A literal `<` inside the angle destination is invalid: not a link.
    assert!(!html("[x](<a<b>)").contains("<a href"));
}

#[test]
fn bare_link_destination_rejects_angle_and_unbalanced_parens() {
    // A `<` inside a bare destination invalidates the link.
    assert!(!html("[x](a<b)").contains("<a href"));
    // Unbalanced opening parens leave no valid destination.
    assert!(!html("[x](a(b").contains("<a href"));
    assert!(html("[x](a(b").contains("[x](a(b"));
}

#[test]
fn link_title_spanning_a_newline_is_not_a_link() {
    // The title quote is interrupted by a line break, so the link does not form.
    let out = html("[x](/u \"a\nb\")");
    assert!(!out.contains("href=\"/u\""));
    assert!(out.contains("[x](/u"));
}

#[test]
fn empty_and_numeric_only_character_references_stay_literal() {
    // `&;` is empty; `&#;` and `&#x;` have empty numeric bodies. None decode.
    assert_eq!(
        parse_inlines("&; &#; &#x;"),
        vec![Inline::Text("&; &#; &#x;".to_string())]
    );
}

#[test]
fn named_character_references_decode_the_full_table() {
    assert_eq!(
        parse_inlines("&gt; &apos; &reg; &ndash; &mdash; &quot; &nbsp; &trade;"),
        vec![Inline::Text(
            "> ' \u{ae} \u{2013} \u{2014} \" \u{a0} \u{2122}".to_string()
        )]
    );
}

#[test]
fn bare_www_autolink_requires_more_than_the_prefix() {
    // A bare `www.` with nothing after the prefix is not autolinked.
    let out = html("see www. here");
    assert!(!out.contains("<a href"));
    assert!(out.contains("www."));
}

#[test]
fn bare_email_domain_may_not_end_in_dash_or_underscore() {
    // The domain ends in `-`, so it is not treated as an email autolink.
    let out = html("mail x@y- here");
    assert!(!out.contains("mailto:"));
}

#[test]
fn inline_html_comment_is_recognized_mid_paragraph() {
    let inlines = paragraph_inlines("see <!-- note --> end");
    assert!(
        inlines
            .iter()
            .any(|i| matches!(i, Inline::Html(h) if h == "<!-- note -->")),
        "expected an inline HTML comment node, got {inlines:?}"
    );
}

#[test]
fn link_text_may_contain_an_escaped_closing_bracket() {
    // The escaped `]` inside the link text is skipped by bracket matching.
    let out = html("[foo\\]bar](/url)");
    assert!(out.contains("<a href=\"/url\">foo]bar</a>"));
}

#[test]
fn image_alt_text_flattens_every_inline_kind() {
    // The alt text is built by flattening every inline variant to plain text:
    // emphasis/strong/strike, code, nested link, nested image, raw HTML, and a
    // soft break (the embedded newline) all contribute.
    let src = "![start *em* **str** ~~strike~~ `code` [link](/l) ![img](/i) <span>h</span> and\nmore](/dest)";
    match only_block(src) {
        Block::Paragraph(inlines) => match inlines.as_slice() {
            [Inline::Image { dest, title, alt }] => {
                assert_eq!(dest, "/dest");
                assert_eq!(*title, None);
                assert_eq!(
                    alt,
                    "start em str strike code link img <span>h</span> and more"
                );
            }
            other => panic!("expected a single image, got {other:?}"),
        },
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Regression tests for the 2026-06-30 "no fakes" parser fixes: each construct
// below previously dropped/faked output and now renders for real.
// ---------------------------------------------------------------------------

#[test]
fn numeric_references_fold_invalid_code_points_to_replacement_char() {
    // U+0000, surrogates, and out-of-range scalars must become U+FFFD — never a
    // raw NUL byte or literal leak (CommonMark sanitization).
    let out = html("a[&#0;]b &#xD800; &#99999999; &#x110000;");
    assert!(
        !out.contains('\u{0}'),
        "no raw NUL byte may reach the output"
    );
    assert!(out.contains('\u{FFFD}'), "invalid refs decode to U+FFFD");
    // A valid numeric reference still decodes normally.
    assert!(html("&#9731;").contains('\u{2603}')); // ☃
}

#[test]
fn opaque_scheme_and_email_autolinks_are_linkified() {
    // tel:/urn: lack `://` and `@` yet are valid CommonMark URI autolinks.
    assert!(html("<tel:+15551234>").contains("href=\"tel:+15551234\""));
    assert!(html("<mailto:a@b.com>").contains("href=\"mailto:a@b.com\""));
    // Email autolinks get a mailto: destination.
    assert!(html("<a@b.com>").contains("href=\"mailto:a@b.com\""));
    // http(s) autolinks keep working.
    assert!(html("<http://x.com>").contains("href=\"http://x.com\""));
    // A plain `<` that is not an autolink is not turned into a link.
    assert!(!html("a < b").contains("<a "));
}

#[test]
fn full_html5_named_entities_decode() {
    // Entities far outside the old 11-entry stub now resolve.
    assert!(html("&hellip;").contains('\u{2026}')); // …
    assert!(html("&auml;").contains('\u{e4}')); // ä
    assert!(html("&aring;").contains('\u{e5}')); // å
    // A multi-code-point entity resolves to both scalars.
    let amacr = html("&nLeftrightarrow;");
    assert!(amacr.contains('\u{21ce}'));
    // An unknown entity stays literal (escaped), not silently dropped.
    assert!(html("&notarealentity;").contains("&amp;notarealentity;"));
}

#[test]
fn tab_indented_lines_form_indented_code_blocks() {
    // A single leading tab is four columns of indentation -> indented code, and
    // tabs inside the content are preserved verbatim.
    let out = html("\tcode_one\n\tcode_two");
    assert!(
        out.contains("<pre><code>"),
        "tab indent must start a code block"
    );
    assert!(out.contains("code_one"));
    let preserved = html("\tfoo\tbar");
    assert!(preserved.contains("foo\tbar"), "interior tabs stay literal");
}

#[test]
fn entity_references_inside_link_destinations_decode_once() {
    // `&amp;` in a destination decodes to `&`, then the emitter escapes it once
    // — no double-escaping into `&amp;amp;`.
    let out = html("[x](http://e.com?a=1&amp;b=2)");
    assert!(out.contains("href=\"http://e.com?a=1&amp;b=2\""));
    assert!(
        !out.contains("&amp;amp;"),
        "destination must not double-escape"
    );
}

#[test]
fn tab_indented_list_and_blockquote_lines_do_not_panic() {
    // Regression: list_marker sliced `&line[indent..]` where `indent` is a
    // tab-expanded COLUMN count, using it as a BYTE index — this panicked on any
    // tab-indented list/blockquote/paragraph-interrupt line.
    for src in [
        "- a\n\tx",
        "> \tx\ny",
        "1. a\n\tz",
        "- a\n  - b\n\t\tq",
        "- a\n\t\tx",
        "\t- x",
        "para\n\t- item",
    ] {
        // Rendering must not panic (the parse is shared by HTML and PDF).
        let out = html(src);
        assert!(out.contains("<body"), "rendered HTML for {src:?}");
    }
}
