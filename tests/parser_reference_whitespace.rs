//! Reference identity and title boundaries use Markdown's source whitespace.
//!
//! CommonMark 0.31.2 section 6.3 folds only spaces, tabs and line endings in
//! reference labels. Other Unicode whitespace remains significant content;
//! section 2.1 defines blank lines using only spaces and tabs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{
    ReferenceLabelForm, destination_spans, parse_inlines, reference_label_spans,
};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn link(label: &str, destination: &str, title: Option<&str>) -> Inline {
    Inline::Link {
        content: vec![text(label)],
        dest: destination.into(),
        title: title.map(str::to_owned),
    }
}

#[test]
fn distinct_unicode_whitespace_labels_keep_their_link_and_image_destinations() {
    for separator in ['\u{a0}', '\u{2003}'] {
        let label = format!("part{separator}one");
        let source = format!(
            "[part one]: /ascii\n[{label}]: /distinct\n\n[plain][part one] [special][{label}] ![image][{label}]\n"
        );
        assert_eq!(
            parse_markdown(&source).blocks,
            [Block::Paragraph(vec![
                link("plain", "/ascii", None),
                text(" "),
                link("special", "/distinct", None),
                text(" "),
                Inline::Image {
                    dest: "/distinct".into(),
                    title: None,
                    alt: "image".into(),
                },
            ])],
            "{source:?}"
        );
    }
}

#[test]
fn unicode_label_edges_and_whitespace_only_labels_remain_significant() {
    for label in ["\u{a0}part", "part\u{a0}", "\u{a0}"] {
        let source = format!(
            "[part]: /ascii\n[{label}]: /distinct\n\n[full][{label}] [{label}][] [{label}]\n"
        );
        assert_eq!(
            parse_markdown(&source).blocks,
            [Block::Paragraph(vec![
                link("full", "/distinct", None),
                text(" "),
                link(label, "/distinct", None),
                text(" "),
                link(label, "/distinct", None),
            ])],
            "label {label:?}"
        );
    }
}

#[test]
fn multiline_labels_fold_ascii_padding_and_unicode_case_without_aliasing() {
    let source = "[Straße\n \u{a0}\n ONE]: /first\n[strasse \u{a0} one]: /shadow\n[strasse one]: /other\n\n[label][STRASSE\t\u{a0}  ONE]\n";
    assert_eq!(
        parse_markdown(source).blocks,
        [Block::Paragraph(vec![link("label", "/first", None)])]
    );
    let spans = reference_label_spans(source);
    assert_eq!(spans.len(), 4);
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.key == "strasse \u{a0} one")
            .count(),
        3
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.key == "strasse one")
            .count(),
        1
    );
}

#[test]
fn reference_keys_and_exact_source_ranges_agree_across_parse_surfaces() {
    let source = "\u{feff}[part one]: /ascii\r\n[part\u{a0}one]: </unicode#part>\r\n\r\n> [ascii][part one] [special][part\u{a0}one]\r\n\r\n- [special][part\u{a0}one]\r\n";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    assert_eq!(
        parse_markdown_spanned_profiled(source).document.to_document(),
        document
    );
    let html =
        franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert_eq!(html.matches("href=\"/ascii\"").count(), 1, "{html}");
    assert_eq!(html.matches("href=\"/unicode#part\"").count(), 2, "{html}");
    let labels = reference_label_spans(source);
    assert_eq!(labels.len(), 5);
    for label in labels {
        assert_eq!(label.key.as_str(), &source[label.label.clone().unwrap()]);
        if label.form == ReferenceLabelForm::Full {
            assert_eq!(label.replacement, label.label);
        }
    }
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 2);
    assert_eq!(&source[destinations[0].range.clone()], "/ascii");
    assert_eq!(&source[destinations[1].range.clone()], "</unicode#part>");
    assert_eq!(
        &source[destinations[1].fragment_range(source).unwrap()],
        "part"
    );
}

#[test]
fn multiline_titles_do_not_treat_unicode_content_as_blank_lines() {
    for separator in ['\u{a0}', '\u{2003}', '\u{c}', '\u{2028}'] {
        let title = format!("first\n {separator}\nlast");
        let inline = format!("[label](/target \"{title}\")");
        assert_eq!(
            parse_inlines(&inline),
            [link("label", "/target", Some(&title))],
            "{inline:?}"
        );
        for opening in [" ", "\n"] {
            let source = format!("[ref]: /target{opening}\"{title}\"\n\n[label][ref]\n");
            assert_eq!(
                parse_markdown(&source).blocks,
                [Block::Paragraph(vec![link("label", "/target", Some(&title))])],
                "{source:?}"
            );
        }
    }
}

#[test]
fn ascii_blank_lines_still_reject_multiline_titles_and_empty_labels() {
    for blank in ["", " ", "\t", " \t "] {
        let source = format!("[ref]: /wrong \"first\n{blank}\nlast\"\n\n[ref]\n");
        assert!(destination_spans(&source).is_empty(), "{source:?}");
        assert!(reference_label_spans(&source).is_empty(), "{source:?}");
        let source = format!("[{blank}]: /wrong\n\n[{blank}]\n");
        assert!(destination_spans(&source).is_empty(), "{source:?}");
    }
}

#[test]
fn preserved_title_content_never_becomes_markup_or_an_editable_destination() {
    let source = "[ref]: /safe \"first\n\u{a0}\n<script>[hidden](/hidden)</script>&quot; onmouseover=&quot;x\"\n\n[label][ref]\n";
    let document = parse_markdown(source);
    let html =
        franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert!(html.contains("href=\"/safe\""), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
    assert!(!html.contains("<script>"), "{html}");
    assert!(!html.contains(" onmouseover=\""), "{html}");
    assert!(!html.contains("href=\"/hidden\""), "{html}");
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(&source[destinations[0].range.clone()], "/safe");
}
