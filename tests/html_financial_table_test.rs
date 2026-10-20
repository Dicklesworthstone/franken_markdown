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
