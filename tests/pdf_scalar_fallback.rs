//! A missing superscript/subscript glyph may use its historical base glyph,
//! but selecting that glyph must not rewrite searchable or extractable text.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use franken_markdown::pdf::{RenderWarning, render_warnings, verification_text_layer};
use franken_markdown::{
    FontAssets, FontFamily, PdfEmitOptions, PdfImageAsset, PdfOptions, fonts, parse_markdown,
    render_pdf, render_pdf_document_emitted,
};

fn options() -> PdfOptions {
    let mut options = PdfOptions {
        page_numbers: false,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    options.theme.font = FontFamily::Serif;
    options
}

/// Decode the original UTF-16BE source from actual marked-content operators.
/// CIDs and ToUnicode alone cannot distinguish `i` from an approximated `ᵢ`.
fn actual_text(pdf: &[u8]) -> Vec<String> {
    let mut result = Vec::new();
    for page in pdf_content::page_content_streams(pdf) {
        for operation in pdf_content::operations(&page) {
            if operation.operator != "BDC" {
                continue;
            }
            if let Some(index) = operation
                .operands
                .iter()
                .position(|&word| word == "/ActualText")
            {
                let value = operation.operands[index + 1];
                let hex = value.strip_prefix('<').unwrap().strip_suffix('>').unwrap();
                let (words, remainder) = hex.as_bytes().as_chunks::<4>();
                assert!(remainder.is_empty());
                let units: Vec<_> = words
                    .iter()
                    .map(|word| {
                        u16::from_str_radix(std::str::from_utf8(word).unwrap(), 16).unwrap()
                    })
                    .collect();
                assert_eq!(units.first(), Some(&0xfeff));
                result.push(String::from_utf16(&units[1..]).unwrap());
            }
        }
    }
    result
}

#[test]
fn fallback_keeps_exact_original_spelling_and_identical_glyph_paint() {
    let source = "Source xᵢ nᵗʰ Mᵃˣ aₐ + bₑ + cₒ + xₓ 10⁻⁵";
    let base = "Source xi nth Max aa + be + co + xx 10-5";
    let opts = options();
    let pdf = render_pdf(source, &opts).unwrap();
    let reference = render_pdf(base, &opts).unwrap();
    pdf_content::assert_same_paint(&pdf, &reference);
    assert_eq!(actual_text(&pdf), [source]);
    assert!(actual_text(&reference).is_empty());
    let layer = verification_text_layer(&parse_markdown(source), &opts).unwrap();
    assert_eq!(layer.pages[0].runs[0].text, source);
    assert!(render_warnings(&parse_markdown(source), &opts).is_empty());
}

#[test]
fn fallback_keeps_source_roles_across_styled_words_tables_and_notes() {
    let source = "# xᵢ\n\n**xᵢ** *nᵗʰ* ***Mᵃˣ*** [xₓ](https://example.com) ~~aₐ~~\n\n> bₑ\n\n- cₒ\n\n| xᵢ | aₐ |\n| --- | --- |\n| nᵗʰ | Mᵃˣ |\n\nNote[^one].\n\n[^one]: Note xᵢ.\n";
    let base = source
        .replace('ᵢ', "i")
        .replace('ᵗ', "t")
        .replace('ʰ', "h")
        .replace('ᵃ', "a")
        .replace('ˣ', "x")
        .replace('ₓ', "x")
        .replace('ₐ', "a")
        .replace('ₑ', "e")
        .replace('ₒ', "o");
    let opts = options();
    let pdf = render_pdf(source, &opts).unwrap();
    let reference = render_pdf(&base, &opts).unwrap();
    pdf_content::assert_same_paint(&pdf, &reference);
    let retained = actual_text(&pdf).join("");
    for expected in ["xᵢ", "nᵗʰ", "Mᵃˣ", "xₓ", "aₐ", "bₑ", "cₒ"] {
        assert!(
            retained.contains(expected),
            "missing original source {expected:?}"
        );
    }
    let objects = String::from_utf8_lossy(&pdf);
    assert!(objects.contains("/URI (https://example.com)"));
    for tag in ["/S /Table", "/S /TH", "/S /TD", "/S /Note", "/S /Reference"] {
        assert!(objects.contains(tag), "missing semantic role {tag}");
    }
}

#[test]
fn literal_code_wraps_at_the_fallback_glyph_width_without_losing_source() {
    let mut opts = options();
    opts.theme.page.size.width_pt = 200.0;
    opts.theme.page.margins.left_pt = 20.0;
    opts.theme.page.margins.right_pt = 20.0;
    let code = "xᵢᵗʰ".repeat(24);
    let source = format!("`xᵢ  nᵗʰ`\n\n```text\n{code}\n```\n");
    let base = source
        .replace('ᵢ', "i")
        .replace('ᵗ', "t")
        .replace('ʰ', "h");
    let pdf = render_pdf(&source, &opts).unwrap();
    pdf_content::assert_same_paint(&pdf, &render_pdf(&base, &opts).unwrap());
    let layer = verification_text_layer(&parse_markdown(&source), &opts).unwrap();
    let mut extracted_code = String::new();
    for run in layer.pages.iter().flat_map(|page| &page.runs) {
        assert!(run.overshoot.is_none(), "overfull source run: {run:?}");
        if run.kind == "code" {
            extracted_code.push_str(&run.text);
        }
    }
    assert_eq!(extracted_code, code);
    let marked = actual_text(&pdf).join("");
    assert!(marked.contains("xᵢ  nᵗʰ"));
    assert!(marked.ends_with(&code));
    assert!(render_warnings(&parse_markdown(&source), &opts).is_empty());
}

#[test]
fn running_bands_and_embedded_svg_keep_their_own_original_text() {
    let mut opts = options();
    opts.running.header.left = Some("Band xᵢ".to_owned());
    opts.image_assets.push(PdfImageAsset::new(
        "label.svg",
        b"<svg xmlns='http://www.w3.org/2000/svg' width='140' height='40'><text x='10' y='25' font-family='serif' font-size='18'>SVG x&#x1d62;</text></svg>".to_vec(),
    ));
    let pdf = render_pdf("Body xᵢ.\n\n![Label](label.svg)", &opts).unwrap();
    let marked = actual_text(&pdf);
    for expected in ["Body xᵢ.", "Band xᵢ", "SVG xᵢ"] {
        assert!(marked.iter().any(|text| text == expected), "{marked:?}");
    }
}

#[test]
fn a_real_source_glyph_wins_and_unavailable_replacements_still_warn() {
    let mut sans = options();
    sans.theme.font = FontFamily::Sans;
    let source = "2² 3³";
    let font = fonts::body_font(FontFamily::Sans, fonts::FontStyle::Regular).unwrap();
    assert_ne!(font.glyph_index('²'), 0);
    assert_ne!(font.glyph_index('³'), 0);
    assert!(actual_text(&render_pdf(source, &sans).unwrap()).is_empty());

    let mut restricted = options();
    restricted.font_assets = FontAssets {
        body_regular: Some(font.subset(&['x', ' ']).unwrap()),
        ..FontAssets::default()
    };
    let warnings = render_warnings(&parse_markdown("xᵢ"), &restricted);
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            RenderWarning::MissingGlyphs { count: 1, sample } if sample == "ᵢ"
        )),
        "{warnings:?}"
    );
    assert!(actual_text(&render_pdf("xᵢ", &restricted).unwrap()).is_empty());
}

#[test]
fn mixed_source_spellings_remain_deterministic_in_every_shared_emitter() {
    let source = "xi xᵢ xi xᵢ\n\nCafe\u{0301} xᵢ.\n";
    let opts = options();
    let doc = parse_markdown(source);
    let pdf = render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::default()).unwrap();
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap()
    );
    assert_eq!(pdf, render_pdf(source, &opts).unwrap());
    let browser = franken_markdown::wasm::render_pdf(
        source,
        &franken_markdown::wasm::WasmRenderOptions {
            theme: opts.theme.clone(),
            metadata_epoch_seconds: opts.metadata_epoch_seconds,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(pdf, browser.bytes);
    assert_eq!(actual_text(&pdf), ["xi xᵢ xi xᵢ", "Cafe\u{0301} xᵢ."]);

    // The bundled serif face has a and á, but neither ᵃ nor the standalone
    // acute mark. Approximation must precede canonical composition, while
    // the two original source scalars still belong to this occurrence.
    let compound = "ᵃ\u{0301}";
    let compound_pdf = render_pdf(compound, &opts).unwrap();
    pdf_content::assert_same_paint(&compound_pdf, &render_pdf("á", &opts).unwrap());
    assert_eq!(actual_text(&compound_pdf), [compound]);
    assert!(render_warnings(&parse_markdown(compound), &opts).is_empty());
}
