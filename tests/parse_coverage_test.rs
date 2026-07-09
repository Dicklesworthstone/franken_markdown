//! Integration coverage tests that drive previously-missed branch arms in
//! `src/parse/mod.rs` through the public API. Every test pins exact output:
//! inline-level cases assert the exact `parse_inlines` AST, block-level cases
//! assert exact rendered-HTML fragments, and the span/diagnostic cases assert
//! exact spanned-document slices and diagnostics. No mocks, no `is_ok()`.
//!
//! Coverage focus areas (branch arms are named in the per-line report):
//!   * emphasis flanking, delimiter pairing, rule of three, mixed `*`/`_`;
//!   * code spans (maximal runs, space stripping, unclosed);
//!   * links/images (angle dest, nested parens, escapes, no-link-in-link,
//!     reference shortcut/collapsed/full, titles);
//!   * autolinks (URI/email/opaque scheme) and GFM bare URL/email;
//!   * character references (named/decimal/hex, NUL/surrogate/overflow, bad);
//!   * ATX/setext heading edges, thematic breaks, fenced code info strings;
//!   * HTML block types 1-7 (comment/CDATA/PI/declaration/raw-text/block-tag);
//!   * lists (unordered/ordered/task, loose/tight, nested, non-interrupting);
//!   * pipe tables (alignment, escaped/code pipes, cell over/underflow);
//!   * reference-definition collection edges (flat, nested, between tables);
//!   * spanned parsing (block spans, paragraph interrupts, diagnostics);
//!   * the inline-parse cache (profiled cache hit, capacity guard).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::parse_inlines;
use franken_markdown::{
    DiagnosticSeverity, HtmlOptions, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, render_html,
};

fn html(src: &str) -> String {
    render_html(src, &HtmlOptions::default()).unwrap()
}

fn html_raw(src: &str) -> String {
    render_html(
        src,
        &HtmlOptions {
            allow_raw_html: true,
            ..HtmlOptions::default()
        },
    )
    .unwrap()
}

fn text(t: &str) -> Inline {
    Inline::Text(t.to_string())
}

// ---- emphasis / strong / delimiter resolution -------------------------------

#[test]
fn simple_star_emphasis_and_strong_pin_exact_ast() {
    assert_eq!(
        parse_inlines("*em*"),
        vec![Inline::Emphasis(vec![text("em")])]
    );
    assert_eq!(
        parse_inlines("**strong**"),
        vec![Inline::Strong(vec![text("strong")])]
    );
}

#[test]
fn intraword_underscore_stays_literal_but_edge_underscore_emphasizes() {
    // `_` intraword rule: `foo_bar_baz` never opens/closes (right & left both
    // flanked by alphanumerics), while `_foo_` opens and closes at word edges.
    assert_eq!(parse_inlines("foo_bar_baz"), vec![text("foo_bar_baz")]);
    assert_eq!(
        parse_inlines("_foo_"),
        vec![Inline::Emphasis(vec![text("foo")])]
    );
}

#[test]
fn intraword_star_emphasis_is_allowed() {
    assert_eq!(
        parse_inlines("a*b*c"),
        vec![text("a"), Inline::Emphasis(vec![text("b")]), text("c")]
    );
}

#[test]
fn triple_delimiters_nest_strong_inside_emphasis() {
    // `***both***`: the delimiters nearest the content pair first (strong inner),
    // the surplus single delimiters wrap it (emphasis outer).
    assert_eq!(
        parse_inlines("***both***"),
        vec![Inline::Emphasis(vec![Inline::Strong(vec![text("both")])])]
    );
}

#[test]
fn quadruple_delimiters_pair_into_nested_strong() {
    assert_eq!(
        parse_inlines("****x****"),
        vec![Inline::Strong(vec![Inline::Strong(vec![text("x")])])]
    );
}

#[test]
fn rule_of_three_pairs_inner_run_when_summed_length_is_multiple_of_three() {
    // `foo***bar***baz`: both runs are 3 (each %3==0), so the rule-of-three
    // exception permits the match; strong nests inside emphasis.
    assert_eq!(
        parse_inlines("foo***bar***baz"),
        vec![
            text("foo"),
            Inline::Emphasis(vec![Inline::Strong(vec![text("bar")])]),
            text("baz"),
        ]
    );
}

#[test]
fn mixed_star_delimiters_across_an_inner_underscore_pair() {
    // The closer `*` must walk back over the already-resolved `_bar_` node to find
    // its `*` opener (exercises the opener-walk char match / alive checks).
    assert_eq!(
        parse_inlines("*foo _bar_ baz*"),
        vec![Inline::Emphasis(vec![
            text("foo "),
            Inline::Emphasis(vec![text("bar")]),
            text(" baz"),
        ])]
    );
}

#[test]
fn strong_star_wrapping_an_inner_underscore_emphasis() {
    assert_eq!(
        parse_inlines("**a _b_ c**"),
        vec![Inline::Strong(vec![
            text("a "),
            Inline::Emphasis(vec![text("b")]),
            text(" c"),
        ])]
    );
}

#[test]
fn parenthesized_punctuation_emphasis_resolves_nested_pairs() {
    assert_eq!(
        parse_inlines("*(*foo*)*"),
        vec![Inline::Emphasis(vec![
            text("("),
            Inline::Emphasis(vec![text("foo")]),
            text(")"),
        ])]
    );
}

#[test]
fn repeated_strong_runs_pair_independently() {
    assert_eq!(
        parse_inlines("a**b**c**d**e"),
        vec![
            text("a"),
            Inline::Strong(vec![text("b")]),
            text("c"),
            Inline::Strong(vec![text("d")]),
            text("e"),
        ]
    );
}

#[test]
fn unicode_symbol_adjacent_to_delimiter_suppresses_emphasis() {
    // A currency symbol (Unicode S) counts as punctuation for flanking, so the
    // run is neither left- nor right-flanking and stays literal.
    assert_eq!(parse_inlines("*£*bravo"), vec![text("*£*bravo")]);
    assert_eq!(parse_inlines("*$*alpha"), vec![text("*$*alpha")]);
}

// ---- strikethrough ----------------------------------------------------------

#[test]
fn double_tilde_strikes_but_single_tilde_stays_literal() {
    assert_eq!(
        parse_inlines("~~gone~~"),
        vec![Inline::Strikethrough(vec![text("gone")])]
    );
    assert_eq!(parse_inlines("~one~"), vec![text("~one~")]);
}

#[test]
fn strikethrough_with_leading_space_after_open_does_not_form() {
    assert!(html("~~ no~~").contains("<p>~~ no~~</p>"));
}

// ---- code spans -------------------------------------------------------------

#[test]
fn code_span_closes_only_on_a_maximal_backtick_run_of_equal_length() {
    // `` `foo``bar`` `` : the inner ``` `` ``` opener spans to the matching `` ``,
    // so a shorter run in the middle cannot close it early.
    assert_eq!(
        parse_inlines("`` `foo``bar`` ``"),
        vec![
            Inline::Code(" `foo".into()),
            text("bar"),
            Inline::Code(" ".into())
        ]
    );
}

#[test]
fn code_span_strips_one_leading_and_trailing_space_unless_all_spaces() {
    assert_eq!(parse_inlines("` a `"), vec![Inline::Code("a".into())]);
    // All-spaces span is preserved verbatim (the strip guard requires non-blank).
    assert_eq!(parse_inlines("`   `"), vec![Inline::Code("   ".into())]);
}

#[test]
fn unclosed_code_span_is_literal_backtick_text() {
    assert_eq!(parse_inlines("`foo"), vec![text("`foo")]);
}

#[test]
fn code_span_collapses_internal_newline_to_space() {
    assert_eq!(parse_inlines("`a\nb`"), vec![Inline::Code("a b".into())]);
}

// ---- links / images ---------------------------------------------------------

#[test]
fn inline_link_with_and_without_title() {
    assert_eq!(
        parse_inlines("[t](/u)"),
        vec![Inline::Link {
            dest: "/u".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
    assert_eq!(
        parse_inlines("[t](/u \"ti\")"),
        vec![Inline::Link {
            dest: "/u".into(),
            title: Some("ti".into()),
            content: vec![text("t")],
        }]
    );
}

#[test]
fn link_title_may_use_parentheses() {
    assert!(html("[t](/u (ti))").contains("<a href=\"/u\" title=\"ti\">t</a>"));
}

#[test]
fn angle_bracket_link_destination_allows_spaces() {
    assert_eq!(
        parse_inlines("[t](<a b>)"),
        vec![Inline::Link {
            dest: "a b".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
}

#[test]
fn angle_bracket_link_destination_rejects_newline() {
    // A newline inside `<...>` aborts the destination, so nothing forms.
    assert!(html("[t](<a\nb>)").contains("[t](&lt;a"));
}

#[test]
fn bare_link_destination_balances_nested_parentheses() {
    assert_eq!(
        parse_inlines("[t](/a(b)c)"),
        vec![Inline::Link {
            dest: "/a(b)c".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
}

#[test]
fn bare_link_destination_with_unbalanced_paren_does_not_form() {
    assert!(html("[t](/a(b)").contains("<p>[t](/a(b)</p>"));
}

#[test]
fn no_link_inside_a_link_the_inner_link_wins() {
    assert_eq!(
        parse_inlines("[a [b](/u) c](/v)"),
        vec![
            text("[a "),
            Inline::Link {
                dest: "/u".into(),
                title: None,
                content: vec![text("b")],
            },
            text(" c](/v)"),
        ]
    );
}

#[test]
fn escaped_closing_bracket_inside_link_text() {
    assert_eq!(
        parse_inlines("[a\\]b](/u)"),
        vec![Inline::Link {
            dest: "/u".into(),
            title: None,
            content: vec![text("a]b")],
        }]
    );
}

#[test]
fn link_with_invalid_trailing_title_char_does_not_form() {
    assert!(html("[t](/u x)").contains("<p>[t](/u x)</p>"));
}

#[test]
fn inline_image_flattens_alt_text() {
    assert_eq!(
        parse_inlines("![alt](/i.png)"),
        vec![Inline::Image {
            dest: "/i.png".into(),
            title: None,
            alt: "alt".into(),
        }]
    );
}

// ---- reference links / definitions ------------------------------------------

#[test]
fn reference_link_shortcut_collapsed_and_full_forms_all_resolve() {
    assert!(html("[foo]\n\n[foo]: /u").contains("<a href=\"/u\">foo</a>"));
    assert!(html("[foo][]\n\n[foo]: /u").contains("<a href=\"/u\">foo</a>"));
    assert!(html("[txt][foo]\n\n[foo]: /u").contains("<a href=\"/u\">txt</a>"));
}

#[test]
fn unresolved_reference_stays_literal() {
    let out = html("[nope]\n\nplain");
    assert!(out.contains("<p>[nope]</p>"));
    assert!(out.contains("<p>plain</p>"));
}

#[test]
fn reference_definition_title_delimiters_all_parse() {
    assert!(html("[a]\n\n[a]: /u \"T\"").contains("<a href=\"/u\" title=\"T\">a</a>"));
    assert!(html("[a]\n\n[a]: /u (T)").contains("<a href=\"/u\" title=\"T\">a</a>"));
    assert!(html("[a]\n\n[a]: /u 'T'").contains("<a href=\"/u\" title=\"T\">a</a>"));
}

#[test]
fn reference_definition_angle_destination_and_next_line_title() {
    assert!(html("[a]\n\n[a]: <u v>").contains("<a href=\"u v\">a</a>"));
    assert!(html("[a]\n\n[a]: /u\n\"Title\"").contains("<a href=\"/u\" title=\"Title\">a</a>"));
}

#[test]
fn malformed_reference_definitions_stay_as_paragraph_text() {
    // Unclosed angle dest, empty dest, and a trailing non-title tail each cause
    // `parse_reference_definition` to bail, so the line renders literally.
    assert!(html("[a]: <u\n\nplain").contains("<p>[a]: &lt;u</p>"));
    assert!(html("[a]:\n\nplain").contains("<p>[a]:</p>"));
    assert!(html("[a]: /u xx\n\nplain").contains("<p>[a]: /u xx</p>"));
}

#[test]
fn indented_four_columns_reference_definition_is_code_not_a_definition() {
    let out = html("    [a]: /u\n\n[a]");
    assert!(out.contains("<pre><code>[a]: /u\n</code></pre>"));
    assert!(out.contains("<p>[a]</p>"));
}

// ---- autolinks --------------------------------------------------------------

#[test]
fn uri_email_and_opaque_scheme_autolinks() {
    assert_eq!(
        parse_inlines("<http://example.com>"),
        vec![Inline::Link {
            dest: "http://example.com".into(),
            title: None,
            content: vec![text("http://example.com")],
        }]
    );
    assert_eq!(
        parse_inlines("<foo@bar.com>"),
        vec![Inline::Link {
            dest: "mailto:foo@bar.com".into(),
            title: None,
            content: vec![text("foo@bar.com")],
        }]
    );
    // An opaque scheme without `://` is still a valid URI autolink.
    assert_eq!(
        parse_inlines("<tel:12345>"),
        vec![Inline::Link {
            dest: "tel:12345".into(),
            title: None,
            content: vec![text("tel:12345")],
        }]
    );
}

#[test]
fn autolink_with_non_alpha_scheme_start_does_not_form() {
    assert_eq!(parse_inlines("<1http://x>"), vec![text("<1http://x>")]);
}

// ---- GFM bare URL / email autolinks -----------------------------------------

#[test]
fn bare_http_url_autolinks_between_boundaries() {
    assert_eq!(
        parse_inlines("see http://example.com now"),
        vec![
            text("see "),
            Inline::Link {
                dest: "http://example.com".into(),
                title: None,
                content: vec![text("http://example.com")],
            },
            text(" now"),
        ]
    );
}

#[test]
fn bare_www_url_gets_http_scheme_prepended() {
    assert_eq!(
        parse_inlines("see www.example.com now"),
        vec![
            text("see "),
            Inline::Link {
                dest: "http://www.example.com".into(),
                title: None,
                content: vec![text("www.example.com")],
            },
            text(" now"),
        ]
    );
}

#[test]
fn bare_url_trims_trailing_sentence_punctuation() {
    assert_eq!(
        parse_inlines("see http://example.com. done"),
        vec![
            text("see "),
            Inline::Link {
                dest: "http://example.com".into(),
                title: None,
                content: vec![text("http://example.com")],
            },
            text(". done"),
        ]
    );
}

#[test]
fn bare_url_after_open_paren_boundary_trims_unmatched_closer() {
    assert_eq!(
        parse_inlines("(http://example.com)"),
        vec![
            text("("),
            Inline::Link {
                dest: "http://example.com".into(),
                title: None,
                content: vec![text("http://example.com")],
            },
            text(")"),
        ]
    );
}

#[test]
fn bare_email_autolinks_and_trims_trailing_dot() {
    assert_eq!(
        parse_inlines("mail foo@bar.com now"),
        vec![
            text("mail "),
            Inline::Link {
                dest: "mailto:foo@bar.com".into(),
                title: None,
                content: vec![text("foo@bar.com")],
            },
            text(" now"),
        ]
    );
    assert_eq!(
        parse_inlines("mail foo@bar.com. done"),
        vec![
            text("mail "),
            Inline::Link {
                dest: "mailto:foo@bar.com".into(),
                title: None,
                content: vec![text("foo@bar.com")],
            },
            text(". done"),
        ]
    );
}

#[test]
fn bare_email_without_a_dotted_domain_does_not_form() {
    assert_eq!(
        parse_inlines("mail foo@bar now"),
        vec![text("mail foo@bar now")]
    );
}

// ---- character references ---------------------------------------------------

#[test]
fn named_and_numeric_character_references_decode() {
    assert_eq!(parse_inlines("&amp; &copy;"), vec![text("& ©")]);
    assert_eq!(parse_inlines("&#65;"), vec![text("A")]);
    assert_eq!(parse_inlines("&#x41;"), vec![text("A")]);
    // Uppercase hex marker `&#X..;` is accepted too.
    assert!(html("&#X41;").contains("<p>A</p>"));
}

#[test]
fn invalid_character_references_stay_literal() {
    assert_eq!(
        parse_inlines("&nope; &#; &#xZZ;"),
        vec![text("&nope; &#; &#xZZ;")]
    );
}

#[test]
fn nul_surrogate_and_overflow_numeric_references_fold_to_replacement() {
    assert_eq!(parse_inlines("&#0;"), vec![text("\u{FFFD}")]);
    assert_eq!(parse_inlines("&#xD800;"), vec![text("\u{FFFD}")]);
    assert_eq!(parse_inlines("&#9999999999;"), vec![text("\u{FFFD}")]);
}

// ---- breaks / escapes / inline html -----------------------------------------

#[test]
fn hard_breaks_from_trailing_spaces_or_backslash() {
    assert_eq!(
        parse_inlines("a  \nb"),
        vec![text("a"), Inline::HardBreak, text("b")]
    );
    assert_eq!(
        parse_inlines("a\\\nb"),
        vec![text("a"), Inline::HardBreak, text("b")]
    );
}

#[test]
fn backslash_escapes_ascii_punctuation() {
    assert_eq!(parse_inlines("\\*not emph\\*"), vec![text("*not emph*")]);
}

#[test]
fn inline_html_tags_and_comments_are_captured_as_html_nodes() {
    assert_eq!(
        parse_inlines("a <span>x</span> b"),
        vec![
            text("a "),
            Inline::Html("<span>".into()),
            text("x"),
            Inline::Html("</span>".into()),
            text(" b"),
        ]
    );
    assert_eq!(
        parse_inlines("a <!-- c --> b"),
        vec![text("a "), Inline::Html("<!-- c -->".into()), text(" b")]
    );
}

// ---- ATX / setext headings, thematic breaks, fences -------------------------

#[test]
fn atx_heading_trailing_hashes_close_only_when_space_separated() {
    // `# heading ##` -> closing sequence stripped; `# heading #x` -> not a
    // closing sequence (a non-hash follows), so `#x` stays in the content.
    assert!(html("# heading ##").contains("<h1 id=\"heading\">heading</h1>"));
    assert!(html("# heading #x").contains("<h1 id=\"heading-x\">heading #x</h1>"));
}

#[test]
fn atx_heading_of_only_hashes_is_an_empty_heading() {
    assert!(html("###").contains("<h3 id=\"section\"></h3>"));
}

#[test]
fn seven_hashes_and_hash_without_space_are_not_headings() {
    assert!(html("####### too many").contains("<p>####### too many</p>"));
    assert!(html("#notheading").contains("<p>#notheading</p>"));
}

#[test]
fn setext_underlines_may_contain_spaces_between_markers() {
    assert!(html("Title\n--- ---").contains("<h2 id=\"title\">Title</h2>"));
    assert!(html("Title\n= = =").contains("<h1 id=\"title\">Title</h1>"));
}

#[test]
fn thematic_breaks_from_each_marker_and_the_too_short_reject() {
    assert!(html("* * *").contains("<hr>"));
    assert!(html("___").contains("<hr>"));
    assert!(html("--").contains("<p>--</p>"));
}

#[test]
fn backtick_fence_info_string_containing_a_backtick_is_not_a_fence() {
    // A ``` info string may not contain a backtick, so the opener is rejected and
    // the line becomes paragraph text (with an inline code span).
    let out = html("```rb`ruby\ncode\n```");
    assert!(out.contains("<code>rb</code>ruby"));
}

#[test]
fn tilde_fence_info_string_may_contain_a_backtick() {
    let out = html("~~~ ok`tick\ncode\n~~~");
    assert!(out.contains("<pre><code class=\"language-ok`tick\">code\n</code></pre>"));
}

#[test]
fn indented_fence_strips_matching_leading_columns_from_content() {
    assert!(html("  ```\n  code\n  ```").contains("<pre><code>code\n</code></pre>"));
}

#[test]
fn unclosed_fence_still_emits_the_code_collected_so_far() {
    assert!(html("```\ncode never closed").contains("<pre><code>code never closed\n</code></pre>"));
}

// ---- HTML blocks (types 1-7) ------------------------------------------------

#[test]
fn html_block_types_render_raw_when_raw_is_allowed() {
    // Type 2 comment, type 5 CDATA, type 3 PI, type 4 declaration, and the bare
    // `<!` type all round-trip verbatim in raw mode, each followed by a paragraph.
    for (src, needle) in [
        ("<!-- comment\nline2 -->\n\npara", "<!-- comment\nline2 -->"),
        ("<![CDATA[\ndata\n]]>\n\npara", "<![CDATA[\ndata\n]]>"),
        ("<?php\ncode ?>\n\npara", "<?php\ncode ?>"),
        ("<!DOCTYPE html>\n\npara", "<!DOCTYPE html>"),
        ("<!bad>\n\npara", "<!bad>"),
    ] {
        let out = html_raw(src);
        assert!(out.contains(needle), "missing {needle:?} in {out}");
        assert!(out.contains("<p>para</p>"), "missing para in {out}");
    }
}

#[test]
fn raw_text_html_blocks_end_at_their_closing_tag() {
    for (src, needle) in [
        (
            "<script>\nlet x=1;\n</script>\n\npara",
            "<script>\nlet x=1;\n</script>",
        ),
        ("<style>\n.x{}\n</style>\n\npara", "<style>\n.x{}\n</style>"),
        (
            "<textarea>\nhi\n</textarea>\n\npara",
            "<textarea>\nhi\n</textarea>",
        ),
    ] {
        let out = html_raw(src);
        assert!(out.contains(needle), "missing {needle:?} in {out}");
        assert!(out.contains("<p>para</p>"));
    }
}

#[test]
fn raw_text_pre_block_spans_across_a_blank_line() {
    // A type-1 `<pre>` block continues past a blank line to its `</pre>` marker,
    // rather than terminating at the blank (which a type-6/7 block would).
    let out = html_raw("<pre>\ntext\n\nmore\n</pre>\n\npara");
    assert!(out.contains("<pre>\ntext\n\nmore\n</pre>"), "{out}");
    assert!(out.contains("<p>para</p>"));
}

#[test]
fn block_level_tag_html_blocks_terminate_at_a_blank_line() {
    let out = html_raw("<div>\ncontent\n</div>\n\npara");
    assert!(out.contains("<div>\ncontent\n</div>"), "{out}");
    assert!(out.contains("<p>para</p>"));
}

#[test]
fn block_tag_matching_is_case_insensitive() {
    let out = html_raw("<DIV>\ncontent\n</DIV>\n\npara");
    assert!(out.contains("<DIV>\ncontent\n</DIV>"), "{out}");
    assert!(out.contains("<p>para</p>"));
}

#[test]
fn a_tag_that_only_shares_a_prefix_with_a_raw_text_element_is_not_a_block() {
    // `<scripture>` shares the `script` prefix but is not `<script>`; it stays an
    // inline-in-paragraph rather than a raw-text HTML block.
    let out = html_raw("<scripture>not raw</scripture>\n\npara");
    assert!(
        out.contains("<p><scripture>not raw</scripture></p>"),
        "{out}"
    );
}

// ---- lists ------------------------------------------------------------------

#[test]
fn unordered_lists_accept_dash_star_and_plus_markers() {
    for src in ["- a\n- b", "* a\n* b", "+ a\n+ b"] {
        let out = html(src);
        assert!(out.contains("<ul>\n<li>a</li>\n<li>b</li>\n</ul>"), "{out}");
    }
}

#[test]
fn ordered_lists_with_dot_and_paren_and_custom_start() {
    assert!(html("1. a\n2. b").contains("<ol>\n<li>a</li>\n<li>b</li>\n</ol>"));
    assert!(html("1) a\n2) b").contains("<ol>\n<li>a</li>\n<li>b</li>\n</ol>"));
    assert!(html("3. a\n4. b").contains("<ol start=\"3\">\n<li>a</li>\n<li>b</li>\n</ol>"));
}

#[test]
fn a_bare_marker_makes_an_empty_first_item() {
    assert!(html("-\n- b").contains("<ul>\n<li></li>\n<li>b</li>\n</ul>"));
}

#[test]
fn a_blank_line_between_items_makes_the_list_loose() {
    assert!(html("- a\n\n- b").contains("<li><p>a</p>\n</li>\n<li><p>b</p>\n</li>"));
}

#[test]
fn a_second_paragraph_at_the_content_column_makes_one_item_loose() {
    assert!(html("- a\n\n  b").contains("<li><p>a</p>\n<p>b</p>\n</li>"));
}

#[test]
fn a_deeper_indented_marker_makes_a_nested_sublist() {
    assert!(html("- a\n  - b").contains("<li>a\n<ul>\n<li>b</li>\n</ul>\n</li>"));
}

#[test]
fn task_list_checkboxes_render_checked_unchecked_and_bare() {
    let out = html("- [x] done\n- [ ] todo");
    assert!(
        out.contains("<input type=\"checkbox\" disabled checked> done"),
        "{out}"
    );
    assert!(
        out.contains("<input type=\"checkbox\" disabled> todo"),
        "{out}"
    );
    assert!(html("- [x]").contains("<input type=\"checkbox\" disabled checked>"));
}

#[test]
fn a_tab_after_a_marker_is_valid_item_padding() {
    assert!(html("-\tabc").contains("<ul>\n<li>abc</li>\n</ul>"));
}

#[test]
fn ordered_marker_starting_above_one_cannot_interrupt_a_paragraph() {
    // `2.` cannot interrupt, so it is absorbed into the paragraph; `1.` can.
    assert!(html("text\n2. item").contains("<p>text\n2. item</p>"));
    let interrupt = html("text\n1. item");
    assert!(interrupt.contains("<p>text</p>"));
    assert!(interrupt.contains("<ol>\n<li>item</li>\n</ol>"));
}

// ---- tables -----------------------------------------------------------------

#[test]
fn pipe_table_with_alignments_renders_styled_cells() {
    let out = html("| a | b | c |\n| :- | :-: | -: |\n| 1 | 2 | 3 |");
    assert!(
        out.contains("<th style=\"text-align:left\">a</th>"),
        "{out}"
    );
    assert!(out.contains("<th style=\"text-align:center\">b</th>"));
    assert!(out.contains("<th style=\"text-align:right\">c</th>"));
    assert!(out.contains("<td style=\"text-align:left\">1</td>"));
}

#[test]
fn empty_delimiter_cells_keep_the_existing_table_shape() {
    let out = html("| a | b | c |\n| - || - |\n| 1 | 2 | 3 |");

    assert!(out.contains("<table>"), "{out}");
    assert!(out.contains("<tr><th>a</th><th>b</th><th>c</th></tr>"));
    assert!(out.contains("<tr><td>1</td><td>2</td><td>3</td></tr>"));
}

#[test]
fn table_cells_split_on_unescaped_pipes_outside_code_spans() {
    assert!(
        html("| a | b |\n| - | - |\n| `x|y` | 2 |").contains("<td><code>x|y</code></td><td>2</td>")
    );
    assert!(html("| a | b |\n| - | - |\n| x\\|y | 2 |").contains("<td>x|y</td><td>2</td>"));
}

#[test]
fn table_rows_are_padded_or_truncated_to_the_header_column_count() {
    // An overflow cell is dropped; a short row is padded with an empty cell.
    assert!(html("| a | b |\n| - | - |\n| 1 | 2 | 3 |").contains("<td>1</td><td>2</td></tr>"));
    assert!(html("| a | b |\n| - | - |\n| 1 |").contains("<td>1</td><td></td></tr>"));
}

#[test]
fn a_table_with_no_body_rows_still_renders_a_header() {
    let out = html("| a | b |\n| - | - |");
    assert!(out.contains("<tr><th>a</th><th>b</th></tr>"), "{out}");
    assert!(out.contains("<tbody>\n</tbody>"));
}

#[test]
fn a_header_delimiter_column_mismatch_is_not_a_table() {
    assert!(html("| a | b |\n| - |").contains("<p>| a | b |\n| - |</p>"));
}

// ---- indented code ----------------------------------------------------------

#[test]
fn indented_code_blocks_span_internal_blank_lines_and_accept_tabs() {
    assert!(html("    code line\n    more").contains("<pre><code>code line\nmore\n</code></pre>"));
    assert!(html("    a\n\n    b").contains("<pre><code>a\n\nb\n</code></pre>"));
    assert!(html("\tcode").contains("<pre><code>code\n</code></pre>"));
}

// ---- blockquotes ------------------------------------------------------------

#[test]
fn blockquote_lazy_continuation_and_interrupt() {
    // A plain line lazily continues the quote's open paragraph; a heading ends it.
    assert!(
        html("> quote\ncontinuation")
            .contains("<blockquote>\n<p>quote\ncontinuation</p>\n</blockquote>")
    );
    let interrupt = html("> quote\n# H");
    assert!(interrupt.contains("<blockquote>\n<p>quote</p>\n</blockquote>"));
    assert!(interrupt.contains("<h1 id=\"h\">H</h1>"));
}

#[test]
fn nested_blockquotes_recurse() {
    assert!(
        html("> > deep")
            .contains("<blockquote>\n<blockquote>\n<p>deep</p>\n</blockquote>\n</blockquote>")
    );
}

// ---- reference-definition collection edges ----------------------------------

#[test]
fn a_definition_between_two_tables_keeps_both_table_boundaries() {
    let out = html(
        "| a | b |\n| - | - |\n| 1 | 2 |\n[x]: /y\n| c | d |\n| - | - |\n| 3 | 4 |\n\nuse [x]",
    );
    // Two separate tables (the second must not be swallowed as body rows) plus a
    // paragraph whose link resolves via the stripped definition.
    assert!(out.contains("<th>a</th><th>b</th>"), "{out}");
    assert!(out.contains("<th>c</th><th>d</th>"), "{out}");
    assert!(out.contains("<p>use <a href=\"/y\">x</a></p>"), "{out}");
}

#[test]
fn a_definition_inside_a_default_html_block_is_not_collected() {
    // The `[foo]: /url` line lives inside a raw HTML block, so it is not a
    // definition and the later use stays literal.
    let out = html("<div>\n[foo]: /url\n</div>\n\n[foo]");
    assert!(!out.contains("href=\"/url\""), "{out}");
    assert!(out.contains("<p>[foo]</p>"), "{out}");
}

#[test]
fn a_definition_nested_in_a_blockquote_resolves_a_use_in_the_same_quote() {
    assert!(html("> [a]: /x\n> use [a]").contains("<p>use <a href=\"/x\">a</a></p>"));
}

#[test]
fn a_refdef_lazily_continuing_a_blockquote_paragraph_is_not_a_definition() {
    let out = html("> quote\n[x]: /y\n\nuse [x]");
    assert!(!out.contains("href=\"/y\""), "{out}");
    assert!(out.contains("[x]: /y"), "{out}");
    assert!(out.contains("<p>use [x]</p>"), "{out}");
}

// ---- multi-line paragraph inline handling -----------------------------------

#[test]
fn plain_multiline_paragraph_uses_soft_breaks() {
    assert!(
        html("line one\nline two\nline three").contains("<p>line one\nline two\nline three</p>")
    );
}

#[test]
fn multiline_paragraph_with_trailing_double_space_is_a_hard_break() {
    assert!(html("line one  \nline two").contains("<p>line one<br>\nline two</p>"));
}

// ---- spanned parsing (span collector + diagnostics) -------------------------

#[test]
fn spanned_paragraph_interrupted_by_a_block_starter_splits_into_two_spans() {
    // The span collector's paragraph loop must break on a block starter with no
    // intervening blank line: an HTML block and a heading each split the span.
    let html_src = "para text\n<div>\nx\n</div>\n";
    let doc = parse_markdown_spanned(html_src);
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].span.slice(html_src).unwrap(), "para text");
    assert_eq!(
        doc.blocks[1].span.slice(html_src).unwrap(),
        "<div>\nx\n</div>"
    );

    let head_src = "para text\n# H\n";
    let head = parse_markdown_spanned(head_src);
    assert_eq!(head.blocks.len(), 2);
    assert_eq!(head.blocks[0].span.slice(head_src).unwrap(), "para text");
    assert_eq!(head.blocks[1].span.slice(head_src).unwrap(), "# H");
}

#[test]
fn spanned_definition_between_tables_preserves_the_table_boundary_span() {
    // Exercises the spanned strip-consumed-source-lines path that inserts a blank
    // separator between two tables around a stripped definition.
    let src =
        "| a | b |\n| - | - |\n| 1 | 2 |\n[x]: /y\n| c | d |\n| - | - |\n| 3 | 4 |\n\nuse [x]";
    let doc = parse_markdown_spanned(src);
    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(
        doc.blocks[0].span.slice(src).unwrap(),
        "| a | b |\n| - | - |\n| 1 | 2 |"
    );
    assert_eq!(
        doc.blocks[1].span.slice(src).unwrap(),
        "| c | d |\n| - | - |\n| 3 | 4 |"
    );
    assert_eq!(doc.blocks[2].span.slice(src).unwrap(), "use [x]");
}

#[test]
fn malformed_reference_definition_yields_a_warning_diagnostic() {
    let src = "[bad]:\n";
    let doc = parse_markdown_spanned(src);
    assert_eq!(doc.blocks.len(), 1);
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(doc.diagnostics[0].span.slice(src).unwrap(), "[bad]:");
    assert!(
        doc.diagnostics[0]
            .message
            .contains("malformed link reference")
    );
}

#[test]
fn unclosed_fence_yields_a_warning_diagnostic_to_end_of_source() {
    let src = "```\nnope\n";
    let doc = parse_markdown_spanned(src);
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert!(doc.diagnostics[0].message.contains("unclosed fenced"));
    assert_eq!(doc.diagnostics[0].span.slice(src).unwrap(), "```\nnope\n");
}

// ---- inline-parse cache -----------------------------------------------------

#[test]
fn profiled_parse_reuses_the_inline_cache_for_repeated_paragraphs() {
    // Two identical cacheable paragraphs (>=16 bytes, needing full inline parse)
    // hit the profiled cache-get path on the second occurrence; the AST is stable.
    let src = "alpha *emphasis* beta gamma\n\nalpha *emphasis* beta gamma\n";
    let profile = parse_markdown_profiled(src);
    assert_eq!(profile.document.blocks.len(), 2);
    let expected = Block::Paragraph(vec![
        text("alpha "),
        Inline::Emphasis(vec![text("emphasis")]),
        text(" beta gamma"),
    ]);
    assert_eq!(profile.document.blocks[0], expected);
    assert_eq!(profile.document.blocks[1], expected);
    // The profiled run records inline-parse stages.
    assert!(profile.stages.iter().any(|s| s.stage == "inline_parse"));
}

#[test]
fn the_inline_cache_capacity_guard_does_not_corrupt_output() {
    // More than the cache's 512-entry cap of distinct cacheable paragraphs forces
    // the insert guard to bail on later entries; every paragraph must still parse
    // correctly (the cache is an optimization, never a correctness dependency).
    let mut src = String::new();
    for n in 0..600 {
        src.push_str(&format!("item number {n} has *emphasis* here\n\n"));
    }
    let doc = parse_markdown(&src);
    assert_eq!(doc.blocks.len(), 600);
    let first = Block::Paragraph(vec![
        text("item number 0 has "),
        Inline::Emphasis(vec![text("emphasis")]),
        text(" here"),
    ]);
    assert_eq!(doc.blocks[0], first);
    let last = Block::Paragraph(vec![
        text("item number 599 has "),
        Inline::Emphasis(vec![text("emphasis")]),
        text(" here"),
    ]);
    assert_eq!(doc.blocks[599], last);
}

// ---- residual-arm targets ---------------------------------------------------

#[test]
fn a_thematic_break_interrupts_a_paragraph_in_block_and_span_parsing() {
    // The paragraph-interrupt check must fire on a thematic break with no blank
    // line before it, both in the block parser and the span collector.
    let out = html("word\n***");
    assert!(out.contains("<p>word</p>"), "{out}");
    assert!(out.contains("<hr>"), "{out}");

    let src = "word\n***\n";
    let doc = parse_markdown_spanned(src);
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "word");
    assert_eq!(doc.blocks[1].span.slice(src).unwrap(), "***");
}

#[test]
fn a_fenced_block_interrupts_a_paragraph_span() {
    let src = "word\n```\ncode\n```\n";
    let doc = parse_markdown_spanned(src);
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "word");
    assert_eq!(doc.blocks[1].span.slice(src).unwrap(), "```\ncode\n```");
}

#[test]
fn an_ordered_marker_with_more_than_nine_digits_is_not_a_list() {
    // A 10-digit run exceeds the ordered-marker digit cap, so `1234567890.` stays
    // paragraph text; the reference definition ahead of it still resolves.
    let out = html("[r]: /u\n\n1234567890. x\n\nuse [r]");
    assert!(out.contains("<p>1234567890. x</p>"), "{out}");
    assert!(out.contains("<a href=\"/u\">r</a>"), "{out}");
}

#[test]
fn a_reference_definition_line_whose_first_bracket_is_not_the_label_is_literal() {
    // `[x] and [y]: /u` — the first `]` is not followed by `:`, so this is not a
    // definition; the whole line stays paragraph text and `[y]` never resolves.
    let out = html("[x] and [y]: /u\n\nuse [y]");
    assert!(out.contains("<p>[x] and [y]: /u</p>"), "{out}");
    assert!(out.contains("<p>use [y]</p>"), "{out}");
    assert!(!out.contains("<a "), "{out}");
}

#[test]
fn a_marker_run_shorter_than_three_is_not_a_thematic_break() {
    // `- -` has only two dashes, so `is_thematic_break` rejects it (its count>=3
    // guard); it parses as a list whose item is itself a bare-marker list.
    assert!(html("- -").contains("<ul>\n<li><ul>\n<li></li>\n</ul>\n</li>\n</ul>"));
}

#[test]
fn an_ordered_list_may_start_at_zero() {
    assert!(
        html("0. item\n1. two").contains("<ol start=\"0\">\n<li>item</li>\n<li>two</li>\n</ol>")
    );
}

#[test]
fn a_reference_definition_two_line_title_at_end_of_document_is_consumed() {
    // The definition and its following title line are both consumed even at EOF,
    // leaving no rendered block; a later use resolves with the title.
    let doc = parse_markdown("[a]: /u\n\"T\"");
    assert_eq!(doc.blocks.len(), 0);
    assert!(html("[a]: /u\n\"T\"\n\n[a]").contains("<a href=\"/u\" title=\"T\">a</a>"));
}

#[test]
fn indented_code_extends_across_multiple_blank_lines_and_ends_at_eof() {
    // A run of blank lines inside an indented code block is preserved as long as
    // more indented code follows; a trailing blank simply ends the block.
    assert!(html("    a\n\n\n    b\n\nend").contains("<pre><code>a\n\n\nb\n</code></pre>"));
    assert!(html("text\n\n    code\n\n").contains("<pre><code>code\n</code></pre>"));
}

#[test]
fn a_nested_ordered_sublist_keeps_its_non_one_start_in_one_list() {
    // The 2nd/3rd items of an indented ordered sublist (start 2, 3) must not be
    // split into separate lists even though their start is not 1.
    let out = html("1. a\n   2. b\n   3. c");
    assert!(
        out.contains("<ol start=\"2\">\n<li>b</li>\n<li>c</li>\n</ol>"),
        "{out}"
    );
}

#[test]
fn a_blank_then_a_nested_marker_makes_a_sublist_not_a_loose_item() {
    // After a blank line a deeper marker begins a sublist; the outer list stays
    // tight (the loosen check requires a non-marker line at the content column).
    let out = html("- a\n\n  - b");
    assert!(
        out.contains("<ul>\n<li>a\n<ul>\n<li>b</li>\n</ul>\n</li>\n</ul>"),
        "{out}"
    );
}

#[test]
fn a_reference_label_may_contain_a_pipe() {
    assert!(html("[a|b]: /u\n\n[a|b]").contains("<a href=\"/u\">a|b</a>"));
}

#[test]
fn a_reference_definition_label_with_nested_or_escaped_brackets_resolves() {
    // `find_closing_bracket` must balance a nested `]` (depth>0) and skip an
    // escaped `\]` when locating the label's closing bracket.
    assert!(html("[a[b]c]: /u\n\n[a[b]c]").contains("<a href=\"/u\">a[b]c</a>"));
    assert!(html("[a\\]b]: /u\n\n[a\\]b]").contains("<a href=\"/u\">a]b</a>"));
}

#[test]
fn a_pipe_row_followed_by_a_dashless_delimiter_is_not_a_table() {
    // `| : |` contains no `-`, so `is_table_delimiter` rejects it and the two
    // lines stay a paragraph.
    assert!(html("| a |\n| : |").contains("<p>| a |\n| : |</p>"));
}

#[test]
fn a_collapsed_reference_label_over_the_length_cap_does_not_resolve() {
    // A collapsed `[text][]` whose text exceeds the 999-char label cap is rejected
    // in O(1); it stays literal (no link) rather than being looked up.
    let big = format!("[{}][]\n\nplain", "a".repeat(1000));
    let doc = parse_markdown(&big);
    assert_eq!(doc.blocks.len(), 2);
    assert!(matches!(doc.blocks[0], Block::Paragraph(_)));
    assert!(!html(&big).contains("<a "));
}

// ---- link destinations / titles: escape and entity decoding -----------------

#[test]
fn angle_bracket_destination_decodes_entities_and_escapes() {
    assert_eq!(
        parse_inlines("[t](<a&amp;b>)"),
        vec![Inline::Link {
            dest: "a&b".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
    assert_eq!(
        parse_inlines("[t](<a\\>b>)"),
        vec![Inline::Link {
            dest: "a>b".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
}

#[test]
fn bare_destination_decodes_entities_and_escapes() {
    assert_eq!(
        parse_inlines("[t](/a&amp;b)"),
        vec![Inline::Link {
            dest: "/a&b".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
    assert_eq!(
        parse_inlines("[t](/a\\)b)"),
        vec![Inline::Link {
            dest: "/a)b".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
}

#[test]
fn link_title_decodes_backslash_escapes() {
    assert_eq!(
        parse_inlines("[t](/u \"a\\\"b\")"),
        vec![Inline::Link {
            dest: "/u".into(),
            title: Some("a\"b".into()),
            content: vec![text("t")],
        }]
    );
}

// ---- delimiter leftovers / mixed runs ---------------------------------------

#[test]
fn a_longer_closer_leaves_a_leftover_delimiter_as_text() {
    // `*a**`: a double closer pairs one delimiter with the single opener; the
    // surplus `*` degrades to literal text.
    assert_eq!(
        parse_inlines("*a**"),
        vec![Inline::Emphasis(vec![text("a")]), text("*")]
    );
    // `**a*`: mirror case — leftover opener `*` is literal, then emphasis.
    assert_eq!(
        parse_inlines("**a*"),
        vec![text("*"), Inline::Emphasis(vec![text("a")])]
    );
}

#[test]
fn emphasis_wrapping_a_strong_run_and_vice_versa() {
    assert_eq!(
        parse_inlines("*a **b** c*"),
        vec![Inline::Emphasis(vec![
            text("a "),
            Inline::Strong(vec![text("b")]),
            text(" c"),
        ])]
    );
    assert_eq!(
        parse_inlines("**a *b* c**"),
        vec![Inline::Strong(vec![
            text("a "),
            Inline::Emphasis(vec![text("b")]),
            text(" c"),
        ])]
    );
    // An intraword `_` inside a `*` emphasis stays literal.
    assert_eq!(
        parse_inlines("*a_b*"),
        vec![Inline::Emphasis(vec![text("a_b")])]
    );
}

// ---- strikethrough closer requires no preceding space -----------------------

#[test]
fn a_strikethrough_closer_preceded_by_a_space_does_not_close() {
    assert_eq!(parse_inlines("~~a ~~"), vec![text("~~a ~~")]);
}

// ---- HTTPS bare URL (distinct scheme operand) -------------------------------

#[test]
fn a_bare_https_url_autolinks_with_its_scheme_preserved() {
    assert_eq!(
        parse_inlines("see https://example.com now"),
        vec![
            text("see "),
            Inline::Link {
                dest: "https://example.com".into(),
                title: None,
                content: vec![text("https://example.com")],
            },
            text(" now"),
        ]
    );
}

// ---- email autolink validation edges ----------------------------------------

#[test]
fn email_autolink_local_part_edges() {
    // Empty local part: not an email; also not tag-like, so it stays literal.
    assert_eq!(parse_inlines("<@bar.com>"), vec![text("<@bar.com>")]);
    // A local part using an allowed special character (`.`) forms a valid email.
    assert_eq!(
        parse_inlines("<a.b@ex.com>"),
        vec![Inline::Link {
            dest: "mailto:a.b@ex.com".into(),
            title: None,
            content: vec![text("a.b@ex.com")],
        }]
    );
    // A disallowed local-part character rejects the email; the `<...>` is instead
    // captured as opaque inline HTML.
    assert_eq!(
        parse_inlines("<a\"b@ex.com>"),
        vec![Inline::Html("<a\"b@ex.com>".into())]
    );
}

#[test]
fn email_autolink_domain_edges_all_reject() {
    // Empty domain, an empty dot-separated label, a label starting or ending with
    // `-`, and a label with a disallowed character each reject the email; the
    // token is captured as inline HTML rather than a mailto link.
    for src in [
        "<foo@>",
        "<a@ex..com>",
        "<a@-ex.com>",
        "<a@ex-.com>",
        "<a@ex_.com>",
    ] {
        assert_eq!(
            parse_inlines(src),
            vec![Inline::Html(src.to_string())],
            "{src}"
        );
    }
    // A domain label longer than 63 chars is rejected.
    let long_label = format!("<a@{}.com>", "x".repeat(64));
    assert_eq!(
        parse_inlines(&long_label),
        vec![Inline::Html(long_label.clone())]
    );
}

// ---- link destination / title: non-punct escape and non-entity `&` ----------

#[test]
fn a_backslash_before_a_non_punctuation_char_is_literal_in_destinations_and_titles() {
    // In angle and bare destinations and in titles, `\` only escapes ASCII
    // punctuation; before a letter it stays a literal backslash.
    assert_eq!(
        parse_inlines("[t](<a\\zb>)"),
        vec![Inline::Link {
            dest: "a\\zb".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
    assert_eq!(
        parse_inlines("[t](/u \"a\\zb\")"),
        vec![Inline::Link {
            dest: "/u".into(),
            title: Some("a\\zb".into()),
            content: vec![text("t")],
        }]
    );
}

#[test]
fn an_ampersand_that_is_not_a_character_reference_is_literal_in_an_angle_destination() {
    assert_eq!(
        parse_inlines("[t](<a&b>)"),
        vec![Inline::Link {
            dest: "a&b".into(),
            title: None,
            content: vec![text("t")],
        }]
    );
}

// ---- inline HTML comment scanning edges -------------------------------------

#[test]
fn an_inline_comment_with_an_interior_double_dash_ends_only_at_the_real_close() {
    assert_eq!(
        parse_inlines("x <!-- b -- c --> y"),
        vec![
            text("x "),
            Inline::Html("<!-- b -- c -->".into()),
            text(" y"),
        ]
    );
}

#[test]
fn an_unclosed_inline_comment_stays_literal() {
    assert_eq!(parse_inlines("x <!-- open"), vec![text("x <!-- open")]);
}

// ---- HTML block classifier edges --------------------------------------------

#[test]
fn a_bang_declaration_without_a_letter_is_a_blank_terminated_block() {
    // `<!1>` is not a type-4 declaration (no ASCII letter after `<!`), so it falls
    // to the historical bare-`<!` blank-terminated block form.
    let out = html_raw("<!1>\n\npara");
    assert!(out.contains("<!1>"), "{out}");
    assert!(out.contains("<p>para</p>"), "{out}");
}

#[test]
fn a_hyphenated_unknown_tag_is_classified_but_is_not_a_block_tag() {
    // `<my-tag>` exercises the hyphen branch of tag-name scanning; it is not a
    // recognized block tag, so it stays inline-in-paragraph.
    let out = html_raw("<my-tag>\ncontent");
    assert!(out.contains("<p><my-tag>\ncontent</p>"), "{out}");
}

#[test]
fn a_marker_terminated_html_block_that_never_closes_runs_to_end_of_document() {
    let out = html_raw("<!-- open\nnever closes");
    assert!(out.contains("<!-- open\nnever closes"), "{out}");
    let doc = parse_markdown("<!-- open\nnever closes");
    assert_eq!(doc.blocks.len(), 1);
    assert!(matches!(doc.blocks[0], Block::HtmlBlock(_)));
}

// ---- table cell with an escaped backslash -----------------------------------

#[test]
fn a_table_cell_with_a_double_backslash_keeps_one_literal_backslash() {
    let out = html("| a\\\\b | c |\n| - | - |\n| 1 | 2 |");
    assert!(out.contains("<th>a\\b</th><th>c</th>"), "{out}");
}

// ---- mixed / nested delimiter resolution (opener-walk paths) -----------------

#[test]
fn emphasis_of_the_same_char_nests_when_the_inner_run_is_flanked() {
    // `*a *b* c*`: the inner `*b*` resolves first; the outer closer then walks back
    // past that resolved node to reach the outer opener.
    assert_eq!(
        parse_inlines("*a *b* c*"),
        vec![Inline::Emphasis(vec![
            text("a "),
            Inline::Emphasis(vec![text("b")]),
            text(" c"),
        ])]
    );
}

#[test]
fn star_emphasis_nested_inside_underscore_emphasis_then_a_trailing_pair() {
    assert_eq!(
        parse_inlines("_a *b* c_ *d*"),
        vec![
            Inline::Emphasis(vec![
                text("a "),
                Inline::Emphasis(vec![text("b")]),
                text(" c"),
            ]),
            text(" "),
            Inline::Emphasis(vec![text("d")]),
        ]
    );
}

#[test]
fn underscore_emphasis_wrapping_a_star_pair_with_a_deeper_underscore() {
    assert_eq!(
        parse_inlines("a *b _c_ d* e"),
        vec![
            text("a "),
            Inline::Emphasis(vec![
                text("b "),
                Inline::Emphasis(vec![text("c")]),
                text(" d"),
            ]),
            text(" e"),
        ]
    );
}

#[test]
fn adjacent_strong_emphasis_and_strike_runs_resolve_independently() {
    assert_eq!(
        parse_inlines("x **a** *b* ~~c~~ y"),
        vec![
            text("x "),
            Inline::Strong(vec![text("a")]),
            text(" "),
            Inline::Emphasis(vec![text("b")]),
            text(" "),
            Inline::Strikethrough(vec![text("c")]),
            text(" y"),
        ]
    );
}

#[test]
fn emphasis_may_wrap_a_code_span_node() {
    // The code span becomes an opaque node between the delimiters; emphasis
    // resolution must collect it as content.
    assert_eq!(
        parse_inlines("*a`b`c*"),
        vec![Inline::Emphasis(vec![
            text("a"),
            Inline::Code("b".into()),
            text("c"),
        ])]
    );
}

// ---- table delimiter cell validation ----------------------------------------

#[test]
fn a_delimiter_row_with_a_colon_only_cell_is_not_a_valid_table() {
    // The row `| - | : |` contains a `-` (so the fast reject passes) but its second
    // cell has an empty dash-core after stripping the colons, so it fails the
    // per-cell check and the two lines stay a paragraph.
    let out = html("| a | b |\n| - | : |");
    assert!(out.contains("<p>| a | b |\n| - | : |</p>"), "{out}");
}
