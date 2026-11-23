//! Semantic and layout-contract checks for explicitly opted-in financial HTML.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, parse_markdown, render_html};

const STATEMENT: &str = "| Item | Amount |\n| --- | ---. |\n\
    | Revenue | [1,234.5](https://example.com/revenue \"Revenue detail\") |\n\
    | Costs | (234.50) |\n\
    | | --- |\n\
    | Count | 17 |\n\
    | Rate | *2.5%* |\n\
    | Adjustment | **20**.125 |\n\
    | Total | **1,000.000** |\n\
    | | === |\n";

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

fn track_widths(html: &str) -> Vec<f32> {
    html.split("grid-template-columns:max-content ")
        .skip(1)
        .map(|tail| tail.split_once("em;").unwrap().0.parse().unwrap())
        .collect()
}

fn without_tags(html: &str) -> String {
    let mut output = String::new();
    let mut inside = false;
    for character in html.chars() {
        match character {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => output.push(character),
            _ => {}
        }
    }
    output
}

#[test]
fn financial_rows_align_decimal_origins_without_changing_text_styles_or_links() {
    let html = fragment(STATEMENT);
    let widths = track_widths(&html);
    assert_eq!(widths.len(), 6);
    assert!(
        widths[0] > 1.0,
        "a multi-digit fraction needs actual font advances"
    );
    assert!(widths.iter().all(|width| *width == widths[0]));
    assert_eq!(
        html.matches("<tr>").count(),
        7,
        "one header and six data rows"
    );
    assert_eq!(html.matches("<td").count(), 12);
    assert_eq!(html.matches("<th scope=\"col\"").count(), 2);
    assert!(!html.contains(">---</"));
    assert!(!html.contains(">===</"));
    assert_eq!(
        html.matches("border-bottom:1px solid currentColor").count(),
        1
    );
    assert_eq!(
        html.matches("border-bottom:3px double currentColor")
            .count(),
        1
    );
    assert_eq!(
        html.matches("href=\"https://example.com/revenue\" title=\"Revenue detail\"")
            .count(),
        1
    );
    assert!(html.contains("<strong>20</strong></span><span class=\"fmd-decimal-after\">.125"));
    assert!(html.contains("<em><span class=\"fmd-decimal\""));
    assert!(html.contains(">2</span><span class=\"fmd-decimal-after\">.5%</span></span></em>"));
    assert!(html.contains("<strong><span class=\"fmd-decimal\""));
    assert!(
        html.contains(">1,000</span><span class=\"fmd-decimal-after\">.000</span></span></strong>")
    );
    assert!(html.contains(">17</span><span class=\"fmd-decimal-after\"></span>"));
    let visible = without_tags(&html);
    for line in [
        "Revenue1,234.5",
        "Costs(234.50)",
        "Count17",
        "Rate2.5%",
        "Adjustment20.125",
        "Total1,000.000",
    ] {
        assert!(visible.contains(line), "{line}: {visible}");
    }
    assert!(!visible.contains("==="));
    assert_eq!(fragment(STATEMENT), html);
}

#[test]
fn column_tracks_are_independent_and_rules_attach_to_selected_total_cells() {
    let html = fragment(
        "| Item | Money | Rate |\n| --- | ---. | ---. |\n| A | 1.0 | 2.12345% |\n| Total | 2.5 | 8.25% |\n| --- | === | --- |\n",
    );
    let widths = track_widths(&html);
    assert_eq!(widths.len(), 4);
    assert_eq!(widths[0], widths[2]);
    assert_eq!(widths[1], widths[3]);
    assert!(widths[1] > widths[0]);
    let total = html.split("<tr>").nth(3).unwrap();
    assert!(total.starts_with("<td style=\"border-bottom:1px solid currentColor;\">Total</td>"));
    assert!(
        total.contains("<td style=\"text-align:right;border-bottom:3px double currentColor;\">")
    );
    assert!(
        total.contains("<td style=\"text-align:right;border-bottom:1px solid currentColor;\">")
    );
    assert_eq!(html.matches("<tr>").count(), 3);
}

#[test]
fn numeric_year_headers_remain_labels_and_do_not_widen_body_decimal_tracks() {
    let source = "| Item | 2025 | 2026.123456789 |\n| --- | ---. | ---. |\n| A | 1.2 | 2.3 |\n";
    let html = fragment(source);
    let labels = fragment(
        &source
            .replace("2025", "Previous")
            .replace("2026.123456789", "Current"),
    );
    assert_eq!(track_widths(&html), track_widths(&labels));
    assert_eq!(
        track_widths(&html).len(),
        2,
        "headers must have no decimal grid"
    );
    assert!(html.contains("<th scope=\"col\" style=\"text-align:right;\">2025</th>"));
    assert!(html.contains("<th scope=\"col\" style=\"text-align:right;\">2026.123456789</th>"));
    assert!(
        !html
            .split_once("</thead>")
            .unwrap()
            .0
            .contains("fmd-decimal")
    );
}

#[test]
fn a_whole_cell_link_keeps_one_keyboard_stop_and_continuous_accessible_text() {
    let html = fragment(
        "| Item | Amount |\n| --- | ---. |\n| Linked | [**12.50**](#total \"Audit\") |\n| Unsafe | [12.50](<javascript:alert(1)>) |\n",
    );
    assert_eq!(html.matches("<a ").count(), 1);
    assert_eq!(html.matches("</a>").count(), 1);
    assert!(
        html.contains("<a href=\"#total\" title=\"Audit\"><strong><span class=\"fmd-decimal\"")
    );
    assert!(
        html.contains(">12</span><span class=\"fmd-decimal-after\">.50</span></span></strong></a>")
    );
    assert!(without_tags(&html).contains("Linked12.50"));
    assert!(without_tags(&html).contains("Unsafe12.50"));
    assert!(!html.contains("javascript:"));
}

#[test]
fn non_numeric_cells_keep_ordinary_content_and_do_not_change_numeric_widths() {
    let html = fragment(
        "| Item | Amount |\n| --- | ---. |\n| Numeric | 12.3 |\n| Unknown | N/A |\n| Code | `123.45678` |\n| Version | v1.23456 |\n| Mixed | 12kg |\n| Styled marker | **===** |\n",
    );
    assert_eq!(track_widths(&html).len(), 1);
    assert!(html.contains("<td style=\"text-align:right;\">N/A</td>"));
    assert!(html.contains("<td style=\"text-align:right;\"><code>123.45678</code></td>"));
    assert!(html.contains("<td style=\"text-align:right;\">v1.23456</td>"));
    assert!(html.contains("<td style=\"text-align:right;\">12kg</td>"));
    assert!(html.contains("<td style=\"text-align:right;\"><strong>===</strong></td>"));
    assert_eq!(html.matches("<tr>").count(), 7);
}

#[test]
fn marker_rows_before_any_data_stay_visible_and_repeated_rules_do_not_add_rows() {
    let html = fragment(
        "| Item | Amount |\n| --- | ---. |\n| | === |\n| Total | 1.5 |\n| | --- |\n| | === |\n",
    );
    assert_eq!(html.matches("<tr>").count(), 3);
    assert!(html.contains("<td style=\"text-align:right;\">===</td>"));
    assert_eq!(
        html.matches("border-bottom:3px double currentColor")
            .count(),
        1
    );
    assert!(!html.contains("border-bottom:1px solid currentColor"));
}

#[test]
fn ordinary_gfm_tables_retain_their_exact_markup_and_literal_marker_rows() {
    let source = "| Name | Amount |\n| --- | ---: |\n| Total | **1.25** |\n| | === |\n";
    let expected = "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n<table>\n<thead>\n<tr><th>Name</th><th style=\"text-align:right\">Amount</th></tr>\n</thead>\n<tbody>\n<tr><td>Total</td><td style=\"text-align:right\"><strong>1.25</strong></td></tr>\n<tr><td></td><td style=\"text-align:right\">===</td></tr>\n</tbody>\n</table>\n</div>\n";
    assert_eq!(fragment(source), expected);
    let plain = render_html(source, &HtmlOptions::default()).unwrap();
    assert!(!plain.contains("fmd-decimal"));
    assert!(!plain.contains("fmd-financial-table"));
}

#[test]
fn custom_stylesheets_remain_unchanged_and_do_not_erase_financial_geometry() {
    let css = "body { color: navy; font-family: serif; } td { padding: .5em; }";
    let opts = HtmlOptions {
        custom_css: Some(css.into()),
        ..HtmlOptions::default()
    };
    let html = render_html(STATEMENT, &opts).unwrap();
    assert_eq!(
        html.split_once("<style>\n")
            .unwrap()
            .1
            .split_once("</style>")
            .unwrap()
            .0,
        css
    );
    assert_eq!(track_widths(&html).len(), 6);
    assert!(html.contains("border-bottom:3px double currentColor"));
}

#[test]
fn cited_amounts_keep_decimal_tracks_and_source_owned_note_links() {
    let source = "Lead[^earlier].\n\n\
        | Item | Amount |\n| --- | ---. |\n\
        | Revenue | [1,234.5](https://example.com/revenue)[^audit] |\n\
        | Costs | **(234.50)[^audit]** |\n\
        | Count | 17[^other][^audit] |\n\
        | Plain | 7.125 |\n\
        | | === |\n\n\
        [^earlier]: Prior context.\n\n[^audit]: Audited statement.\n\n[^other]: Unit count.\n";
    let html = fragment(source);
    let widths = track_widths(&html);
    assert_eq!(widths.len(), 4, "citations must not disable numeric alignment");
    assert!(widths.iter().all(|width| *width == widths[0]));
    assert!(html.contains("class=\"fmd-decimal-before\"><a href=\"https://example.com/revenue\">1,234</a>"));
    assert!(html.contains(".5</a><sup class=\"footnote-ref\" id=\"fnref-2\"><a href=\"#fn-audit\">2</a></sup>"));
    assert!(html.contains(">17</span><span class=\"fmd-decimal-after\"><sup class=\"footnote-ref\" id=\"fnref-3\">"));
    assert_eq!(html.matches("id=\"fnref-2\"").count(), 1);
    assert_eq!(html.matches("href=\"#fn-audit\"").count(), 3);
    assert_eq!(html.matches("Audited statement.").count(), 1);
    assert_eq!(html.matches("border-bottom:3px double currentColor").count(), 1);
    let visible = without_tags(&html);
    for text in ["Revenue1,234.52", "Costs(234.50)2", "Count1732", "Plain7.125"] {
        assert!(visible.contains(text), "missing {text}: {visible}");
    }
    assert_eq!(fragment(source), html);

    let plain = fragment("| Item | Amount |\n| --- | ---. |\n| A | 17 |\n| B | 7.125 |\n");
    assert!(widths[0] > track_widths(&plain)[0], "citation glyphs need space in the shared suffix track");
}

#[test]
fn citation_measurement_predicts_first_use_without_consuming_reference_anchors() {
    let prefix = (1..=9).map(|n| format!("Earlier[^n{n}] ")).collect::<String>();
    let definitions = (1..=10).map(|n| format!("[^n{n}]: Note {n}.\n\n")).collect::<String>();
    let table = "| Label[^header] | Amount |\n| --- | ---. |\n| A | 12.5[^n10] |\n| B[^label] | 7.2[^next] |\n";
    let source = format!("{prefix}\n\n{table}\n{definitions}[^header]: Header note.\n\n[^label]: Row note.\n\n[^next]: Next note.\n");
    let html = fragment(&source);
    assert_eq!(track_widths(&html).len(), 2);
    for (number, id) in [(10, "header"), (11, "n10"), (12, "label"), (13, "next")] {
        assert_eq!(html.matches(&format!("id=\"fnref-{number}\"")).count(), 1);
        assert!(html.contains(&format!("href=\"#fn-{id}\">{number}</a></sup>")));
    }
    let single = fragment("| Item | Amount |\n| --- | ---. |\n| A | 12.5[^one] |\n| B | 7.2 |\n\n[^one]: One.\n");
    assert!(track_widths(&html)[0] > track_widths(&single)[0], "two-digit citations must reserve their complete width");

    let opts = HtmlOptions::default();
    let document = parse_markdown(&source);
    let complete = franken_markdown::render_html_document(&document, &opts).unwrap();
    let mut streamed = Vec::new();
    franken_markdown::render_html_document_to_writer(&document, &opts, &mut streamed).unwrap();
    assert_eq!(streamed, complete.as_bytes());
}

#[test]
fn unresolved_interior_and_authored_superscript_marks_remain_nonnumeric() {
    let html = fragment("| Item | Amount |\n| --- | ---. |\n\
        | Numeric | 12.5[^yes] |\n| Undefined | 12.5[^missing] |\n\
        | Middle | 12[^yes].5 |\n| Tail | 12.5[^yes] USD |\n\
        | Authored | 12.5² |\n| Code | `12.5`[^yes] |\n\n[^yes]: Verified.\n");
    assert_eq!(track_widths(&html).len(), 1);
    let visible = without_tags(&html);
    for text in ["Undefined12.5[^missing]", "Middle121.5", "Tail12.51 USD", "Authored12.5²", "Code12.51"] {
        assert!(visible.contains(text), "{text}: {visible}");
    }
}

#[test]
fn whole_cell_cited_links_release_link_ownership_for_later_cells_and_prose() {
    let source = "| Item | Amount |\n| --- | ---. |\n\
        | Linked | [12.5[^inside]](/amount) |\n\
        | Later | 7.25[^outside] |\n\
        | More | [6.75](/next) |\n\n\
        After [details](/tail) and another citation[^inside].\n\n\
        [^inside]: First evidence.\n\n[^outside]: Second evidence.\n";
    let html = fragment(source);
    assert_eq!(track_widths(&html).len(), 3);
    for destination in ["/amount", "/next", "/tail"] {
        assert_eq!(html.matches(&format!("href=\"{destination}\"")).count(), 1);
    }

    let outer = html.split_once("<a href=\"/amount\">").unwrap().1;
    let outer = outer.split_once("</a>").unwrap().0;
    assert!(outer.contains("class=\"fmd-decimal\""));
    assert!(outer.contains("<sup class=\"footnote-ref\" id=\"fnref-1\">1</sup>"));
    assert!(!outer.contains("<a "), "the amount link owns its note mark");
    assert!(html.contains("id=\"fnref-2\"><a href=\"#fn-outside\">2</a></sup>"));
    assert!(html.contains("another citation<sup class=\"footnote-ref\"><a href=\"#fn-inside\">1</a></sup>"));
    assert_eq!(html.matches("id=\"fnref-1\"").count(), 1);
    assert_eq!(html.matches("id=\"fnref-2\"").count(), 1);

    let document = parse_markdown(source);
    let options = HtmlOptions::default();
    let complete = franken_markdown::render_html_document(&document, &options).unwrap();
    let mut streamed = Vec::new();
    franken_markdown::render_html_document_to_writer(&document, &options, &mut streamed).unwrap();
    assert_eq!(streamed, complete.as_bytes());
    assert!(complete.contains("href=\"/tail\""));
}

#[test]
fn deeply_styled_header_notes_reserve_the_actual_two_digit_body_citation() {
    let prefix = (1..=8)
        .map(|n| format!("Earlier[^n{n}] "))
        .collect::<String>();
    let definitions = (1..=8)
        .map(|n| format!("[^n{n}]: Earlier note {n}.\n\n"))
        .collect::<String>();
    let source = format!(
        "{prefix}\n\n| Label[^header] | Amount |\n| --- | ---. |\n\
         | A | 12.5[^amount] |\n| B | 7.2 |\n\n\
         {definitions}[^header]: Header note.\n\n[^amount]: Amount evidence.\n"
    );
    let shallow = fragment(&source);
    let mut document = parse_markdown(&source);
    let table = document.blocks.iter_mut().find_map(|block| match block {
        franken_markdown::Block::Table(table) => Some(table),
        _ => None,
    }).unwrap();
    // Construct a deep public AST explicitly, without depending on how a
    // delimiter run is paired. Header notes do not need numeric admission.
    let mut header = std::mem::take(&mut table.head[0]);
    for _ in 0..130 {
        header = vec![franken_markdown::Inline::Strong(header)];
    }
    table.head[0] = header;
    let deep = franken_markdown::html::render_fragment(
        &document.blocks,
        &HtmlOptions::default(),
    );
    assert_eq!(track_widths(&deep).len(), 2);
    assert!(deep.contains("href=\"#fn-header\">9</a></sup>"));
    assert!(deep.contains("href=\"#fn-amount\">10</a></sup>"));
    assert_eq!(deep.matches("id=\"fnref-9\"").count(), 1);
    assert_eq!(deep.matches("id=\"fnref-10\"").count(), 1);
    assert_eq!(
        track_widths(&deep), track_widths(&shallow),
        "header nesting must not change the amount's actual two-digit suffix width"
    );
}
