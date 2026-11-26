//! Dollar amounts must not consume prose or a later formula (GitHub issue #22).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, HtmlOptions, Inline, Profile, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_with_profile,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn math(value: &str) -> Inline {
    Inline::Math(value.into())
}

fn assert_prose(source: &str, expected: Vec<Inline>) {
    assert_eq!(parse_inlines(source), expected, "{source:?}");
    let document = parse_markdown(source);
    assert_eq!(document.blocks, [Block::Paragraph(expected)], "{source:?}");
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    for profile in [Profile::CommonMarkGfm, Profile::GfmPlus] {
        assert_eq!(
            parse_markdown_with_profile(source, Some(profile)),
            document,
            "{profile:?}: {source:?}"
        );
    }
}

#[test]
fn financial_ranges_and_punctuation_preceded_prices_stay_literal() {
    for source in [
        "Screen of $5 million–$50 million.",
        "$5m–$50m",
        "$54,000 fee ($4,500 × 12)",
        "$5−$1,808.36",
        "$5/$10/$15",
        "$5+$10",
        "$12.50|$19.95",
        "$x$2",
    ] {
        assert_prose(source, vec![text(source)]);
    }
}

#[test]
fn currency_does_not_consume_emphasis_or_a_later_real_formula() {
    let source = "Screen of $5 million–$50 million. Thus $0.470 becomes **$0.260 of UFCF**. \
                  Costs $5 and $x = 1$.";
    assert_prose(
        source,
        vec![
            text("Screen of $5 million–$50 million. Thus $0.470 becomes "),
            Inline::Strong(vec![text("$0.260 of UFCF")]),
            text(". Costs $5 and "),
            math("x = 1"),
            text("."),
        ],
    );
    let document = parse_markdown(source);
    let html = franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert!(html.contains("Screen of $5 million–$50 million. Thus $0.470 becomes "));
    assert!(html.contains("<strong>$0.260 of UFCF</strong>. Costs $5 and "));
    assert_eq!(html.matches("<math ").count(), 1, "{html}");
}

#[test]
fn invalid_closers_leave_later_formulas_available() {
    for (source, expected) in [
        (
            "Costs $5 and $x = 1$.",
            vec![text("Costs $5 and "), math("x = 1"), text(".")],
        ),
        ("$x $y$", vec![text("$x "), math("y")]),
        ("$5 and $2+2=4$", vec![text("$5 and "), math("2+2=4")]),
        (
            "$5 and $x$ then $10 and $y$",
            vec![text("$5 and "), math("x"), text(" then $10 and "), math("y")],
        ),
    ] {
        assert_prose(source, expected);
    }
}

#[test]
fn numeric_math_and_nondigit_suffixes_keep_their_delimiters() {
    for formula in ["2", "2+2=4", "-3.5", "2|x", "|x|", r"50\,\mathrm{USD}"] {
        let source = format!("Value ${formula}$.");
        assert_prose(&source, vec![text("Value "), math(formula), text(".")]);
    }
    // The currency guard is deliberately ASCII-only.
    for suffix in ["suffix", "−2", "²", "٢"] {
        assert_prose(&format!("$x${suffix}"), vec![math("x"), text(suffix)]);
    }
    assert_prose("$\u{a0}x\u{a0}$", vec![math("\u{a0}x\u{a0}")]);
}

#[test]
fn escaped_and_braced_dollars_remain_math_content() {
    for formula in [
        r"a\$5 + b",
        r"\text{Price $5} + x",
        r"\text{Price {USD $5} and $10} + x",
        r"\text{escaped \} Price $5} + x",
        r"a\\",
    ] {
        assert_prose(&format!("${formula}$"), vec![math(formula)]);
    }
    assert_prose(r"Price \$5 and $x$", vec![text("Price $5 and "), math("x")]);
    // Escaped braces and groups opened before a candidate are not TeX groups
    // belonging to that candidate, so they cannot hide the next formula.
    assert_prose(r"$\{price $x$", vec![text("${price "), math("x")]);
    assert_prose(
        "{Cost $5 and $x$}",
        vec![text("{Cost $5 and "), math("x"), text("}")],
    );
}

#[test]
fn display_math_and_literal_code_keep_their_existing_ownership() {
    assert_prose(
        "Value $$x$$2 and $$a $5 + b$$.",
        vec![
            text("Value "),
            Inline::DisplayMath("x".into()),
            text("2 and "),
            Inline::DisplayMath("a $5 + b".into()),
            text("."),
        ],
    );
    assert_prose(
        "`$5–$50 and $x$` then $y$",
        vec![
            Inline::Code("$5–$50 and $x$".into()),
            text(" then "),
            math("y"),
        ],
    );
}

#[test]
fn currency_prose_keeps_real_links_and_original_source_ranges() {
    let source = "Pré $5–$50 **$30,000** [guide](/café) costs $5 and $x = 1$.\r\n";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/café");
    assert_eq!(&source[destinations[0].range.clone()], "/café");
    let html = franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert!(html.contains("<strong>$30,000</strong>"));
    assert!(html.contains("<a href=\"/caf%C3%A9\">guide</a>"));
    assert_eq!(html.matches("<math ").count(), 1, "{html}");
}

#[test]
fn repeated_prices_preserve_following_links_and_real_math_within_the_budget() {
    let prices = "$5–$50 ".repeat(4096);
    let source = format!("{prices}[guide](/target) and $x$.");
    assert_prose(
        &source,
        vec![
            text(&prices),
            Inline::Link {
                dest: "/target".into(),
                title: None,
                content: vec![text("guide")],
            },
            text(" and "),
            math("x"),
            text("."),
        ],
    );
    assert_eq!(destination_spans(&source).len(), 1);
    assert!(parse_markdown_spanned(&source).diagnostics.is_empty());
}
