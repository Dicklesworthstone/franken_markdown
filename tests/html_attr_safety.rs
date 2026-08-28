//! Attribute-context escaping for values that originate in Markdown.
//!
//! GFM footnote identifiers may contain `"` (anything except whitespace and
//! brackets). Those ids are written into `id="…"` and `href="…"` attributes, so
//! they must use attribute escaping — text-node escaping leaves quotes intact
//! and would break out of the attribute.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

fn log_check(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log_check(id, subject, "PASS");
    } else {
        log_check(id, subject, "FAIL");
        panic!("{id} failed for `{subject}`: {detail}");
    }
}

fn render(src: &str) -> String {
    franken_markdown::render_html_document(
        &franken_markdown::parse_markdown(src),
        &franken_markdown::HtmlOptions::default(),
    )
    .expect("html render")
}

fn render_doc(doc: &franken_markdown::Document) -> String {
    franken_markdown::render_html_document(doc, &franken_markdown::HtmlOptions::default())
        .expect("html render")
}

#[test]
fn constructed_footnote_id_quote_is_attribute_escaped() {
    // Parser rejects `"<&` in footnote ids; the HTML emitter must still
    // attribute-escape a programmatically built AST.
    let id = "a\"b".to_string();
    let doc = franken_markdown::Document {
        blocks: vec![
            franken_markdown::ast::Block::Paragraph(vec![
                franken_markdown::ast::Inline::FootnoteRef { id: id.clone() },
            ]),
            franken_markdown::ast::Block::FootnoteDefinition {
                id,
                blocks: vec![franken_markdown::ast::Block::Paragraph(vec![
                    franken_markdown::ast::Inline::Text("the note".into()),
                ])],
            },
        ],
    };
    let html = render_doc(&doc);
    assert_ok(
        "fn-quote-no-breakout",
        "id attribute",
        !html.contains("id=\"fn-a\""),
        &html,
    );
    assert_ok(
        "fn-quote-escaped",
        "id attribute",
        html.contains("id=\"fn-a&quot;b\""),
        &html,
    );
    assert_ok(
        "fn-quote-href",
        "href attribute",
        html.contains("href=\"#fn-a&quot;b\""),
        &html,
    );
}

#[test]
fn unmatched_footnote_ref_quote_stays_in_text() {
    let html = render("dangling[^a\"b] here\n");
    assert_ok(
        "fn-unmatched-literal",
        "text",
        html.contains("[^a\"b]") && !html.contains("<sup class=\"footnote-ref\""),
        &html,
    );
}
