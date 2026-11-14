//! Image descriptions are plain text, including when authored with raw HTML.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, Document, FontAssets, HtmlOptions, Inline, PdfImageAsset, PdfOptions, parse_markdown,
    parse_markdown_profiled, parse_markdown_spanned, render_pdf, render_pdf_document,
};

fn image(alt: &str) -> Inline {
    Inline::Image {
        dest: "diagram.png".into(),
        title: None,
        alt: alt.into(),
    }
}

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, parsed);
    assert_eq!(parse_markdown_spanned(source).to_document(), parsed);
    parsed
}

fn assert_description(source: &str, expected: &str) {
    assert_eq!(
        document(source).blocks,
        [Block::Paragraph(vec![image(expected)])],
        "{source:?}"
    );
}

#[test]
fn descriptions_keep_visible_text_without_tags_attributes_or_comments() {
    for (description, expected) in [
        ("<em>useful</em>", "useful"),
        ("<span title='hidden'>visible</span>", "visible"),
        ("before<!-- hidden -->after", "beforeafter"),
        ("before<!-->after<!--->tail", "beforeaftertail"),
        ("before<?hidden?>after", "beforeafter"),
        ("before<!X hidden>after", "beforeafter"),
        ("before<![CDATA[hidden]]>after", "beforeafter"),
        ("<b>café</b> &amp; **details**", "café & details"),
        ("before<br>after", "beforeafter"),
    ] {
        assert_description(&format!("![{description}](diagram.png)"), expected);
    }
}

#[test]
fn escaped_markup_code_and_malformed_tags_remain_literal_description_text() {
    for (description, expected) in [
        (r"\<em>literal\</em>", "<em>literal</em>"),
        ("&lt;em&gt;literal&lt;/em&gt;", "<em>literal</em>"),
        ("`<em>literal</em>`", "<em>literal</em>"),
        ("`<!-- literal -->`", "<!-- literal -->"),
        ("< span>literal</ span>", "< span>literal</ span>"),
        ("<span/ >literal", "<span/ >literal"),
        ("<em>text</em> and `<b>`", "text and <b>"),
    ] {
        assert_description(&format!("![{description}](diagram.png)"), expected);
    }
}

#[test]
fn reference_images_use_the_same_plain_description_as_inline_images() {
    for source in [
        "![<em>café</em>](diagram.png)",
        "![<em>café</em>][picture]\n\n[picture]: diagram.png\n",
        "![<em>café</em>][]\n\n[<em>café</em>]: diagram.png\n",
        "![<em>café</em>]\n\n[<em>café</em>]: diagram.png\n",
    ] {
        assert_description(source, "café");
    }
}

#[test]
fn nested_images_and_links_contribute_only_their_visible_descriptions() {
    assert_description(
        "![<b>outer</b> [<i>linked</i>](/target) ![<u>inner</u>](/nested.png)](diagram.png)",
        "outer linked inner",
    );
    assert_description(
        "![<b>outer</b> [<i>linked</i>][target]](diagram.png)\n\n[target]: /target\n",
        "outer linked",
    );
}

#[test]
fn markup_only_descriptions_do_not_disguise_missing_alternative_text() {
    for description in [
        "<em></em>",
        "<!-- hidden -->",
        "<span title='not alternative text'></span>",
        "<img src='nested.png' alt='not Markdown description text'>",
    ] {
        assert_description(&format!("![{description}](diagram.png)"), "");
    }
}

#[test]
fn image_descriptions_do_not_change_the_surrounding_raw_html_policy() {
    let source = "<b>outside</b> ![<em>inside</em>](diagram.png)";
    let parsed = document(source);
    assert_eq!(
        parsed.blocks,
        [Block::Paragraph(vec![
            Inline::Html("<b>".into()),
            Inline::Text("outside".into()),
            Inline::Html("</b>".into()),
            Inline::Text(" ".into()),
            image("inside"),
        ])]
    );
    for allow_raw_html in [false, true] {
        let options = HtmlOptions {
            allow_raw_html,
            ..HtmlOptions::default()
        };
        let html = franken_markdown::html::render_fragment(&parsed.blocks, &options);
        assert!(html.contains("outside"));
        assert!(html.contains("<img src=\"diagram.png\" alt=\"inside\">"));
        assert!(!html.contains("alt=\"&lt;em&gt;"));
    }
}

#[test]
fn html_pdf_and_svg_share_the_plain_description() {
    let source = "![A <em>clear</em> <!-- hidden -->diagram](diagram.png)";
    let expected = Document {
        blocks: vec![Block::Paragraph(vec![image("A clear diagram")])],
    };
    let parsed = document(source);
    assert_eq!(parsed, expected);

    for allow_raw_html in [false, true] {
        let options = HtmlOptions {
            allow_raw_html,
            ..HtmlOptions::default()
        };
        assert_eq!(
            franken_markdown::html::render_fragment(&parsed.blocks, &options),
            "<p><img src=\"diagram.png\" alt=\"A clear diagram\"></p>\n"
        );
    }

    // Missing-image fallback text and metadata must consume the same semantic
    // description as HTML, without retaining raw attributes or comments.
    let pdf_options = PdfOptions::default();
    assert_eq!(
        render_pdf(source, &pdf_options).unwrap(),
        render_pdf_document(&expected, &pdf_options).unwrap()
    );
    let image_asset = PdfImageAsset::new(
        "diagram.png",
        b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'/>".to_vec(),
    );
    let (svg, _, warnings) = franken_markdown::svg::render_svg_with_resources(
        &parsed,
        &franken_markdown::svg::SvgOptions::default(),
        &FontAssets::default(),
        &[image_asset],
    )
    .unwrap();
    assert!(warnings.is_empty());
    let svg = String::from_utf8(svg).unwrap();
    assert!(svg.contains("aria-label=\"A clear diagram\""));
    assert!(!svg.contains("&lt;em&gt;"));
    assert!(!svg.contains("hidden"));
}
