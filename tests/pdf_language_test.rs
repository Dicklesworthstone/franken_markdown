//! The PDF catalog must describe the document's actual language to readers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfASettings, PdfOptions, parse_markdown, render_pdf};

const DOCUMENT: &str = "# Language\n\nShort body text.\n";

fn catalog(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf)
        .split_once("1 0 obj\n")
        .expect("catalog object")
        .1
        .split_once("endobj")
        .expect("catalog terminator")
        .0
        .to_owned()
}

#[test]
fn tagged_pdf_catalog_uses_the_selected_language_and_regional_tag() {
    for lang in ["de", "fr-CA", "es", "nl", "zh-Hant", "ar", "und"] {
        let options = PdfOptions {
            lang: Some(lang.to_owned()),
            ..PdfOptions::default()
        };
        let pdf = render_pdf(DOCUMENT, &options).unwrap();
        let root = catalog(&pdf);
        assert!(root.contains("/Type /Catalog"));
        assert!(root.contains("/MarkInfo << /Marked true >>"));
        assert!(root.contains(&format!("/Lang ({lang})")), "{root}");
        assert_eq!(pdf, render_pdf(DOCUMENT, &options).unwrap());
    }
}

#[test]
fn absent_or_blank_language_preserves_the_existing_default_bytes() {
    let baseline = render_pdf(DOCUMENT, &PdfOptions::default()).unwrap();
    assert!(catalog(&baseline).contains("/Lang (en-US)"));
    for lang in ["", " \t\n ", "en-US", " en-US "] {
        let options = PdfOptions {
            lang: Some(lang.to_owned()),
            ..PdfOptions::default()
        };
        assert_eq!(
            baseline,
            render_pdf(DOCUMENT, &options).unwrap(),
            "{lang:?}"
        );
    }
}

#[test]
fn host_language_is_encoded_as_a_pdf_string_not_dictionary_syntax() {
    for (lang, token) in [
        (
            "de) /OpenAction << /S /JavaScript >> (\\",
            r"(de\) /OpenAction << /S /JavaScript >> \(\\)",
        ),
        ("français", "<FEFF006600720061006E00E7006100690073>"),
    ] {
        let options = PdfOptions {
            lang: Some(lang.to_owned()),
            ..PdfOptions::default()
        };
        let pdf = render_pdf(DOCUMENT, &options).unwrap();
        assert!(catalog(&pdf).contains(&format!("/Lang {token}")));
    }
}

#[test]
fn archival_pdf_preserves_document_language_with_its_catalog_extensions() {
    let options = PdfOptions {
        lang: Some("nl-NL".to_owned()),
        ..PdfOptions::default()
    };
    let pdf = franken_markdown::render_pdf_document_pdfa(
        &parse_markdown(DOCUMENT),
        &options,
        PdfASettings::a2b(),
    )
    .unwrap();
    let root = catalog(&pdf);
    assert!(root.contains("/Lang (nl-NL)"));
    assert!(root.contains("/Metadata "));
    assert!(root.contains("/OutputIntents "));
}
