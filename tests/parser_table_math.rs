//! Inline formulas must own their pipes before table columns are partitioned.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, Table, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned,
};

fn table(document: &Document) -> &Table {
    let Block::Table(table) = &document.blocks[0] else {
        panic!("expected table: {document:?}");
    };
    table
}

fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text(value.into())]
}

fn assert_table_source(source: &str) -> Document {
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    document
}

#[test]
fn text_and_display_math_pipes_preserve_following_columns() {
    for formula in [
        "$P(A|B)$",
        "$x | y$",
        r"$$\left|x\right|$$",
        r"$\lVert x|y\rVert$",
        "$[fake](/hidden)|x$",
    ] {
        let source = format!(
            "| Formula | Result | Link |\n| --- | --- | --- |\n| {formula} | retained-data | [go](/visible) |\n"
        );
        let document = assert_table_source(&source);
        let parsed = table(&document);
        assert_eq!(parsed.rows.len(), 1);
        assert_eq!(parsed.rows[0].len(), 3);
        assert_eq!(parsed.rows[0][0], parse_inlines(formula), "{formula}");
        assert_eq!(parsed.rows[0][1], text("retained-data"), "{formula}");
        assert_eq!(parsed.rows[0][2], parse_inlines("[go](/visible)"));
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{formula}");
        assert_eq!(spans[0].dest, "/visible");
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert!(html.contains("<td>retained-data</td>"));
        assert!(html.contains("href=\"/visible\""));
        assert!(!html.contains("href=\"/hidden\""));
    }
}

#[test]
fn math_in_headers_uses_the_same_column_boundaries_as_body_rows() {
    for (open, close) in [("| ", " |"), ("", "")] {
        let source =
            format!("{open}$P(A|B)$ | $$x|y$${close}\n| --- | --- |\n| $C|D$ | retained-data |\n");
        let document = assert_table_source(&source);
        assert_eq!(
            table(&document).head,
            [
                vec![Inline::Math("P(A|B)".into())],
                vec![Inline::DisplayMath("x|y".into())]
            ]
        );
        assert_eq!(
            table(&document).rows,
            [vec![
                vec![Inline::Math("C|D".into())],
                text("retained-data")
            ]]
        );
    }
}

#[test]
fn math_inside_links_and_emphasis_keeps_its_complete_cell() {
    for source in [
        "**$P(A|B)$**",
        "[$P(A|B)$](/target)",
        "![$P(A|B)$](image.png)",
    ] {
        let document = assert_table_source(&format!(
            "| Formula | Result |\n| --- | --- |\n| {source} | retained-data |\n"
        ));
        assert_eq!(
            table(&document).rows[0],
            [parse_inlines(source), text("retained-data")]
        );
    }
}

#[test]
fn code_and_backslash_escapes_keep_precedence_over_math_openers() {
    for formula in [
        r"`$a|b$`",
        r"``$a`|b$``",
        r"\$not-math\|still-text\$",
        r"$a\$|b$",
        r"$a\\$",
        r"$`a|b`$",
        r"$a\|b$",
        r"$$a\$$|b$$",
        r"\$$a|b$",
    ] {
        let document = assert_table_source(&format!(
            "| Formula | Result |\n| --- | --- |\n| {formula} | retained-data |\n"
        ));
        assert_eq!(
            table(&document).rows[0],
            [parse_inlines(formula), text("retained-data")],
            "{formula:?}"
        );
    }
}

#[test]
fn invalid_math_edges_do_not_hide_real_cell_separators() {
    for (left, right) in [
        ("$", "remaining"),
        ("$12.50", "$19.95"),
        ("$ 12.50", "$ 19.95"),
        ("$ x", "y$"),
        ("$\tx", "y$"),
        ("$x", "y $"),
        ("$$ x", "y $$"),
        (r"\$x", r"y\$"),
        ("$unclosed", "remaining"),
    ] {
        let source = format!("| Left | Right |\n| --- | --- |\n| {left} | {right} |\n");
        let document = assert_table_source(&source);
        assert_eq!(
            table(&document).rows[0],
            [parse_inlines(left), parse_inlines(right)],
            "{source:?}"
        );
    }
}

#[test]
fn long_and_mixed_dollar_runs_follow_existing_inline_rules() {
    for formula in ["$$$x|y$$", "$$$$x|y$$", "$$x|y$", "$x$$y|z$", "$x$$$y|z$"] {
        let document = assert_table_source(&format!(
            "| Formula | Result |\n| --- | --- |\n| {formula} | retained-data |\n"
        ));
        assert_eq!(
            table(&document).rows[0],
            [parse_inlines(formula), text("retained-data")],
            "{formula:?}"
        );
    }
}

#[test]
fn unicode_crlf_and_destination_spans_survive_math_owned_pipes() {
    let source = "Pré $A|B$ | Δ\r\n--- | ---\r\n$\\text{café}|图$ [go](<dir/café\\|图.png>) | end\r\n# Next | heading\r\n";
    let document = assert_table_source(source);
    assert_eq!(table(&document).rows[0][1], text("end"));
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks.len(), 2);
    let end = source.find("\r\n# Next").unwrap();
    assert_eq!(spanned.blocks[0].span.slice(source), Some(&source[..end]));
    assert_eq!(
        spanned.blocks[1].span.slice(source),
        Some("# Next | heading")
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "dir/café|图.png");
    assert_eq!(&source[spans[0].range.clone()], "<dir/café\\|图.png>");
}

#[test]
fn repeated_math_cells_share_prose_semantics_without_polluting_code_escapes() {
    let formula = "repeated prefix $P(A|B)$";
    let prose = format!("{formula}\n\n").repeat(3);
    let source = format!(
        "{prose}| {formula} | Result |\n| --- | --- |\n| {formula} | a |\n| {formula} | b |\n| {formula} | c |\n\n{prose}"
    );
    let document = assert_table_source(&source);
    let expected = parse_inlines(formula);
    assert_eq!(document.blocks.len(), 7);
    for block in &document.blocks {
        match block {
            Block::Paragraph(inlines) => assert_eq!(inlines, &expected),
            Block::Table(table) => {
                assert_eq!(table.head[0], expected);
                for row in &table.rows {
                    assert_eq!(row[0], expected);
                }
            }
            other => panic!("unexpected block: {other:?}"),
        }
    }
}

#[test]
fn math_tables_in_container_blocks_keep_their_following_source() {
    let source = "> | Formula | Result |\n> | --- | --- |\n> | $P(A|B)$ | retained-data |\n\nAfter [go](/after)\n";
    let document = assert_table_source(source);
    let Block::BlockQuote(blocks) = &document.blocks[0] else {
        panic!("{document:?}")
    };
    let Block::Table(parsed) = &blocks[0] else {
        panic!("{blocks:?}")
    };
    assert_eq!(
        parsed.rows[0],
        [vec![Inline::Math("P(A|B)".into())], text("retained-data")]
    );
    assert_eq!(
        document.blocks[1],
        Block::Paragraph(parse_inlines("After [go](/after)"))
    );
    assert_eq!(destination_spans(source)[0].dest, "/after");
}

#[test]
fn many_unmatched_math_openers_preserve_every_column() {
    let count = 512;
    let head = vec!["h"; count].join(" | ");
    let delimiter = vec!["---"; count].join(" | ");
    let row = vec!["$12.50"; count].join(" | ");
    let source = format!("{head}\n{delimiter}\n{row}\n\nAfter [go](/after)\n");
    let document = assert_table_source(&source);
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(table(&document).rows[0].len(), count);
    for cell in &table(&document).rows[0] {
        assert_eq!(cell, &text("$12.50"));
    }
    assert_eq!(destination_spans(&source)[0].dest, "/after");
}

#[test]
fn currency_columns_cannot_capture_a_later_formula_closer() {
    for separator in [" | ", "|"] {
        for formula in ["$P(A|B)$", "$2|x$", "$|x|$", "$$P(A|B)$$"] {
            for prices in [["$12.50", "$19.95"], ["$-12.50", "$+19.95"]] {
                let row = [prices[0], prices[1], formula, "retained"].join(separator);
                let source =
                    format!("Price A | Price B | Formula | Result\n--- | --- | --- | ---\n{row}\n");
                let document = assert_table_source(&source);
                assert_eq!(
                    table(&document).rows[0],
                    [
                        text(prices[0]),
                        text(prices[1]),
                        parse_inlines(formula),
                        text("retained")
                    ],
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn compact_currency_headers_and_adjacent_absolute_value_formulas_keep_columns() {
    let source = "$12.50|$19.95|$|x|$|Result\n---|---|---|---\n$|x|$|$|y|$|$2|z$|retained\n";
    let document = assert_table_source(source);
    assert_eq!(
        table(&document).head,
        [
            text("$12.50"),
            text("$19.95"),
            parse_inlines("$|x|$"),
            text("Result")
        ]
    );
    assert_eq!(
        table(&document).rows[0],
        [
            parse_inlines("$|x|$"),
            parse_inlines("$|y|$"),
            parse_inlines("$2|z$"),
            text("retained")
        ]
    );
}

#[test]
fn a_pipe_adjacent_formula_opener_cannot_close_the_previous_cell() {
    for previous in ["$unclosed", "$12.50", "$-12.50"] {
        for formula in ["$P(A|B)$", r"$\alpha|x$", "$π|θ$"] {
            let source =
                format!("Left|Formula|Result\n---|---|---\n{previous}|{formula}|retained\n");
            let document = assert_table_source(&source);
            assert_eq!(
                table(&document).rows[0],
                [text(previous), parse_inlines(formula), text("retained")],
                "{source:?}"
            );
        }
    }
}

#[test]
fn ambiguous_numeric_math_endings_prefer_separate_table_cells() {
    // In prose this is legal math ending in a pipe. In a table the same source
    // can be a price next to an unmatched dollar: retain the column boundary.
    assert_eq!(parse_inlines("$12.50|$"), [Inline::Math("12.50|".into())]);
    let source = "Price|Literal|Result\n---|---|---\n$12.50|$|retained\n";
    assert_eq!(
        table(&assert_table_source(source)).rows[0],
        [text("$12.50"), text("$"), text("retained")]
    );

    // Escaping the ambiguous pipe makes the intended formula explicit.
    let source = "Formula|Result\n---|---\n$12.50\\|$|retained\n";
    assert_eq!(
        table(&assert_table_source(source)).rows[0],
        [parse_inlines(r"$12.50\|$"), text("retained")]
    );
}

#[test]
fn table_admission_does_not_change_ordinary_inline_math_grammar() {
    for (source, expected) in [
        (
            "$x$2",
            vec![Inline::Math("x".into()), Inline::Text("2".into())],
        ),
        ("$x $y$", vec![Inline::Math("x $y".into())]),
        (
            "$12.50|$19.95",
            vec![Inline::Math("12.50|".into()), Inline::Text("19.95".into())],
        ),
        (
            "$|x|$suffix",
            vec![Inline::Math("|x|".into()), Inline::Text("suffix".into())],
        ),
        ("$2|x$", vec![Inline::Math("2|x".into())]),
        ("$|x|$", vec![Inline::Math("|x|".into())]),
    ] {
        assert_eq!(parse_inlines(source), expected, "{source:?}");
    }
}
