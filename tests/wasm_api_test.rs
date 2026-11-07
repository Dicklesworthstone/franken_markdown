//! Browser/WASM-facing core API tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm::{
    WasmOutputFormat, WasmRenderOptions, capabilities_json, render_html, render_pdf,
};
use franken_markdown::{
    DarkModePolicy, FontAssetSlot, FontAssets, FontFamily, RenderError,
    fonts::{self, FontStyle},
};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn tiny_rgb_png() -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

    let rows = [0, 0x20, 0x70, 0xc0];
    let idat = franken_markdown::compress::zlib_compress(&rows);

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

#[test]
fn wasm_html_output_is_self_contained_bytes_with_diagnostics() {
    let output = render_html(
        "# Title\n\n[bad]:\n\nBody with **strong** text.",
        &WasmRenderOptions::default(),
    )
    .unwrap();

    assert_eq!(output.format, WasmOutputFormat::Html);
    assert_eq!(output.mime_type, "text/html; charset=utf-8");
    assert_eq!(output.extension, "html");
    assert_eq!(
        output.source_len,
        "# Title\n\n[bad]:\n\nBody with **strong** text.".len()
    );
    assert_eq!(output.diagnostics.len(), 1);
    assert_eq!(output.diagnostics[0].severity, "warning");
    assert!(output.diagnostics[0].message.contains("reference"));
    assert!(
        output
            .diagnostics_json()
            .contains("\"severity\":\"warning\"")
    );

    let html = output.html().unwrap();
    assert!(html.contains("<strong>strong</strong>"));
    assert!(html.contains("<style>"));
}

#[test]
fn wasm_options_accept_font_names_dark_mode_and_css_bytes() {
    let options = WasmRenderOptions::default()
        .with_font_name("serif")
        .unwrap()
        .with_dark_mode(DarkModePolicy::Disabled)
        .with_custom_css_bytes(b"body { color: #123456; }")
        .unwrap();
    let output = render_html("# Styled", &options).unwrap();
    let html = output.html().unwrap();

    assert!(html.contains("body { color: #123456; }"));
    assert!(!html.contains("@media (prefers-color-scheme: dark)"));
}

#[test]
fn wasm_serif_font_changes_default_stylesheet_without_custom_css() {
    let options = WasmRenderOptions::default()
        .with_font_name("serif")
        .unwrap();
    let output = render_html("# Styled", &options).unwrap();
    let html = output.html().unwrap();

    assert!(html.contains("Source Serif 4"));
    assert!(!html.contains("body { color: #123456; }"));
}

#[test]
fn wasm_options_reject_bad_font_and_non_utf8_css() {
    let err = WasmRenderOptions::default()
        .with_font_name("comic-sans")
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(err.to_string().contains("use 'sans' or 'serif'"));

    let err = WasmRenderOptions::default()
        .with_custom_css_bytes(&[0xff, 0xfe])
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(err.to_string().contains("custom CSS must be UTF-8"));

    let err = WasmRenderOptions::default()
        .with_pdf_image_asset("  ", tiny_rgb_png())
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(
        err.to_string()
            .contains("image asset destination must not be blank")
    );

    let err = WasmRenderOptions::default()
        .with_font_asset_name("body-display", b"not a font".to_vec())
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(err.to_string().contains("unknown font asset slot"));

    let err = WasmRenderOptions::default()
        .with_font_asset_bytes(FontAssetSlot::BodyRegular, b"not a font".to_vec())
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(
        err.to_string()
            .contains("body-regular font bytes are not a supported TrueType font")
    );
}

#[test]
fn wasm_html_accepts_browser_supplied_font_bytes() {
    let fallback = render_html("Body text", &WasmRenderOptions::default()).unwrap();
    let options = WasmRenderOptions::default()
        .with_font_asset_bytes(
            FontAssetSlot::BodyRegular,
            fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec(),
        )
        .unwrap();
    let custom = render_html("Body text", &options).unwrap();
    let html = custom.html().unwrap();

    assert_eq!(custom.format, WasmOutputFormat::Html);
    assert_ne!(
        custom.bytes, fallback.bytes,
        "custom body font bytes should change the embedded font subset"
    );
    assert!(html.contains("@font-face"));
    assert!(html.contains("font-family: \"FMD Body\""));
}

#[test]
fn wasm_render_rejects_directly_constructed_malformed_font_assets() {
    let options = WasmRenderOptions {
        font_assets: FontAssets {
            body_regular: Some(b"not a font".to_vec()),
            ..FontAssets::default()
        },
        ..WasmRenderOptions::default()
    };
    let err = render_html("Body", &options).unwrap_err();

    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(
        err.to_string()
            .contains("body-regular font bytes are not a supported TrueType font")
    );
}

#[test]
fn wasm_pdf_output_is_binary_deterministic_and_uses_epoch_option() {
    let options = WasmRenderOptions {
        title: Some("WASM PDF".to_string()),
        author: Some("fmd".to_string()),
        metadata_epoch_seconds: Some(1_700_000_000),
        code_line_numbers: true,
        ..WasmRenderOptions::default()
    };
    let a = render_pdf("# PDF\n\n```rust\nfn main() {}\n```", &options).unwrap();
    let b = render_pdf("# PDF\n\n```rust\nfn main() {}\n```", &options).unwrap();

    assert_eq!(a.format, WasmOutputFormat::Pdf);
    assert_eq!(a.mime_type, "application/pdf");
    assert_eq!(a.extension, "pdf");
    assert!(a.html().is_none());
    assert!(a.bytes.starts_with(b"%PDF-"));
    assert_eq!(a.bytes, b.bytes);
    assert!(String::from_utf8_lossy(&a.bytes).contains("D:20231114221320Z"));
}

#[test]
fn wasm_pdf_accepts_browser_supplied_image_bytes() {
    let options = WasmRenderOptions::default()
        .with_pdf_image_asset("diagram.png", tiny_rgb_png())
        .unwrap();
    let output = render_pdf("![Diagram](diagram.png)", &options).unwrap();
    let pdf = String::from_utf8_lossy(&output.bytes);

    assert_eq!(output.format, WasmOutputFormat::Pdf);
    assert!(pdf.contains("/Subtype /Image"));
    assert!(pdf.contains("/XObject << /Im1 "));
    assert!(pdf.contains("/S /Figure"));
    assert!(pdf.contains("/Alt (Diagram)"));
}

#[test]
fn wasm_pdf_surfaces_degraded_content_as_warning_diagnostics() {
    // A browser host must receive the same "content was degraded" signal the
    // native CLI prints: an image with no supplied asset (rendered as alt text)
    // and a hieroglyph with no glyph in any bundled face. Without this
    // the drop would be silent — contrary to the no-silent-drop doctrine.
    let output = render_pdf(
        "![chart](missing.png)\n\n\u{13000} body text",
        &WasmRenderOptions::default(),
    )
    .unwrap();
    assert_eq!(output.format, WasmOutputFormat::Pdf);
    assert!(!output.is_empty());

    let warnings: Vec<&str> = output
        .diagnostics
        .iter()
        .filter(|d| d.severity == "warning")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        warnings.iter().any(|m| m.contains("missing.png")),
        "unresolved-image warning missing: {warnings:?}"
    );
    assert!(
        warnings.iter().any(|m| m.contains("glyph")),
        "missing-glyph warning missing: {warnings:?}"
    );
    // The same warnings must appear in the serialized diagnostics JSON hosts read.
    let json = output.diagnostics_json();
    assert!(json.contains("\"severity\":\"warning\""));
    assert!(json.contains("missing.png"));
}

#[test]
fn wasm_pdf_accepts_browser_supplied_font_bytes() {
    let options = WasmRenderOptions::default()
        .with_font_asset_bytes(
            FontAssetSlot::BodyRegular,
            fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec(),
        )
        .unwrap();
    let fallback = render_pdf("Body text", &WasmRenderOptions::default()).unwrap();
    let a = render_pdf("Body text", &options).unwrap();
    let b = render_pdf("Body text", &options).unwrap();
    let pdf = String::from_utf8_lossy(&a.bytes);

    assert_eq!(a.format, WasmOutputFormat::Pdf);
    assert_eq!(a.bytes, b.bytes);
    assert_ne!(
        a.bytes, fallback.bytes,
        "custom body font bytes should change the embedded PDF subset"
    );
    assert!(pdf.contains("/FontFile2"));
    assert!(pdf.contains("/CIDFontType2"));
}

#[test]
fn wasm_capabilities_json_exposes_browser_safe_contract() {
    let json = capabilities_json();

    assert!(json.contains("\"schema\":\"fmd-wasm-capabilities-v1\""));
    assert!(json.contains("\"outputs\":[\"html\",\"pdf\",\"svg\",\"epub\",\"interactive-html\",\"diff-html\",\"book-site\",\"book-pdf\"]"));
    assert!(json.contains("\"filesystem\":false"));
    assert!(json.contains("\"process\":false"));
    assert!(json.contains("\"network\":false"));
    assert!(json.contains("\"threads\":false"));
    assert!(json.contains("\"font\":\"sans\""));
    assert!(json.contains("\"custom_css_utf8\":true"));
    assert!(json.contains("\"image_assets\":\"png_svg_v0_host_supplied_bytes\""));
    assert!(json.contains("\"font_assets\":\"ttf_v0_host_supplied_bytes\""));
    assert!(json.contains("\"font_outline_formats\":[\"truetype_glyf\",\"opentype_cff1\"]"));
    assert!(json.contains("\"font_slot_weight\":\"css_1_to_1000_variable_wght\""));
}

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

#[test]
fn wasm_builder_pins_variable_font_slot_weight() {
    let vf = franken_markdown::text::variable_triangle_fixture();
    let err = WasmRenderOptions::default()
        .with_font_slot_weight_name("not-a-slot", 650)
        .unwrap_err();
    log_check(
        "gk3v.3.wasm.slot",
        "unknown slot rejected",
        err.to_string().contains("unknown font asset slot"),
    );
    let err = WasmRenderOptions::default()
        .with_font_slot_weight(FontAssetSlot::BodyRegular, 0)
        .unwrap_err();
    log_check(
        "gk3v.3.wasm.weight",
        "weight 0 rejected",
        err.to_string().contains("out of range"),
    );

    let at_400 = WasmRenderOptions::default()
        .with_font_asset_bytes(FontAssetSlot::BodyRegular, vf.clone())
        .unwrap()
        .with_font_slot_weight_name("body-regular", 400)
        .unwrap();
    let at_650 = WasmRenderOptions::default()
        .with_font_asset_bytes(FontAssetSlot::BodyRegular, vf)
        .unwrap()
        .with_font_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    at_400.font_assets.validate().unwrap();
    let a = render_pdf("Hello variable", &at_400).unwrap();
    let b = render_pdf("Hello variable", &at_650).unwrap();
    let b2 = render_pdf("Hello variable", &at_650).unwrap();
    log_check(
        "gk3v.3.wasm.diff",
        "WASM PDF at 400 differs from 650",
        a.bytes != b.bytes,
    );
    log_check(
        "gk3v.3.wasm.det",
        "WASM PDF at 650 is deterministic",
        b.bytes == b2.bytes,
    );
    log_check(
        "gk3v.3.wasm.pdf",
        "WASM PDF is a PDF",
        b.bytes.starts_with(b"%PDF-"),
    );
}

// --- grn.2.8: small-module coverage for the wasm core surface ---------------

#[test]
fn wasm_output_format_spellings_are_stable() {
    assert_eq!(WasmOutputFormat::Html.as_str(), "html");
    assert_eq!(WasmOutputFormat::Pdf.as_str(), "pdf");
    assert_eq!(WasmOutputFormat::Html.extension(), "html");
    assert_eq!(WasmOutputFormat::Pdf.extension(), "pdf");
    assert_eq!(
        WasmOutputFormat::Html.mime_type(),
        "text/html; charset=utf-8"
    );
    assert_eq!(WasmOutputFormat::Pdf.mime_type(), "application/pdf");
}

#[test]
fn wasm_render_options_constructors_and_font_builder() {
    // sans() is the default; serif() flips the body font to the serif family.
    let sans = WasmRenderOptions::sans();
    assert_eq!(sans.theme.font, FontFamily::Sans);
    let serif = WasmRenderOptions::serif();
    assert_eq!(serif.theme.font, FontFamily::Serif);
    assert_ne!(serif.theme.font, sans.theme.font);

    // with_font_asset_name resolves a stable slot spelling and validates REAL bytes
    // (this is the success path that delegates to with_font_asset_bytes).
    let regular = fonts::body_bytes(FontFamily::Sans, FontStyle::Regular).to_vec();
    let opts = WasmRenderOptions::sans()
        .with_font_asset_name("body-regular", regular)
        .unwrap();
    assert!(!opts.font_assets.is_empty());
}

#[test]
fn wasm_render_output_accessors_reflect_format() {
    let html = render_html("# Hi\n\nbody", &WasmRenderOptions::sans()).unwrap();
    assert!(!html.is_empty());
    assert_eq!(html.len(), html.bytes.len());
    assert!(html.len() > 100, "a self-contained HTML doc is non-trivial");
    assert!(html.html().expect("html() Some for HTML").contains("<main"));

    let pdf = render_pdf("# Hi", &WasmRenderOptions::sans()).unwrap();
    assert!(!pdf.is_empty());
    assert!(pdf.html().is_none(), "html() must be None for PDF output");
}

#[test]
fn wasm_diagnostics_json_escapes_control_and_quote_characters() {
    // Build a result by hand (its fields are public) to drive the JSON escaper
    // across every branch: quote, backslash, newline, tab, CR, control char; the
    // two diagnostics also exercise the comma-join branch.
    let out = franken_markdown::wasm::WasmRenderOutput {
        format: WasmOutputFormat::Html,
        mime_type: WasmOutputFormat::Html.mime_type(),
        extension: WasmOutputFormat::Html.extension(),
        bytes: b"<main></main>".to_vec(),
        diagnostics: vec![
            franken_markdown::wasm::WasmDiagnostic {
                code: None,
                severity: "warning",
                start: 0,
                end: 3,
                message: "first".to_string(),
            },
            franken_markdown::wasm::WasmDiagnostic {
                code: Some("custom\"code\n\u{0001}"),
                severity: "error\"level\n\u{0001}",
                start: 4,
                end: 9,
                message: "say \"hi\"\n\twith \\ and \r and \u{0001} control".to_string(),
            },
        ],
        source_len: 13,
    };
    let json = out.diagnostics_json();
    assert!(json.starts_with('['));
    assert!(json.contains("\"severity\":\"warning\""));
    assert!(
        json.contains("\"code\":\"custom\\\"code\\n \""),
        "public diagnostic code must be escaped: {json}"
    );
    assert!(
        json.contains("\"severity\":\"error\\\"level\\n \""),
        "public diagnostic severity must be escaped: {json}"
    );
    assert!(json.contains("\\\"hi\\\""), "quotes escaped: {json}");
    assert!(json.contains("\\n"), "newline escaped: {json}");
    assert!(json.contains("\\t"), "tab escaped: {json}");
    assert!(json.contains("\\r"), "carriage return escaped: {json}");
    assert!(json.contains("\\\\"), "backslash escaped: {json}");
    assert!(json.contains("},{"), "diagnostics comma-separated: {json}");
    assert!(json.ends_with(']'));
}

#[test]
fn wasm_pdf_accepts_typography_overrides() {
    let opts_default = WasmRenderOptions::default();
    let opts_custom = WasmRenderOptions::default()
        .with_base_font_size(14.0)
        .with_heading_scale(1.33)
        .with_table_font_size(9.0);

    let doc = "# Heading\n\nParagraph text.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n";
    let a = render_pdf(doc, &opts_default).unwrap();
    let b = render_pdf(doc, &opts_custom).unwrap();
    let c = render_pdf(doc, &opts_custom).unwrap();

    assert_eq!(b.bytes, c.bytes, "WASM renders must be deterministic");
    assert_ne!(
        a.bytes, b.bytes,
        "Typography overrides must change PDF output"
    );
}

#[test]
fn typography_tokens_match_native_pdf_options_byte_for_byte() {
    let markdown = "# Title\n\nA long paragraph with enough words to wrap across several justified lines so the line breaker has real choices to make between alternative breakpoints and spacing.\n\nAnother paragraph follows here.\n";
    let mut wasm_options = franken_markdown::wasm::WasmRenderOptions::default();
    wasm_options
        .apply_typography_tokens(Some(
            "homogeneous, antiriver,pareto,optimal-pagination,expansion",
        ))
        .unwrap();
    let wasm_pdf = franken_markdown::wasm::render_pdf(markdown, &wasm_options).unwrap();
    let native_pdf = franken_markdown::render_pdf(
        markdown,
        &franken_markdown::PdfOptions {
            gradual_demerits: true,
            river_penalty: true,
            pareto_line_breaking: true,
            optimal_pagination: true,
            microtype: franken_markdown::layout::MicrotypeOptions {
                protrusion: false,
                max_expansion_per_mille: 15,
            },
            ..franken_markdown::PdfOptions::default()
        },
    )
    .unwrap();
    assert_eq!(wasm_pdf.bytes, native_pdf);
    // Absent or empty tokens keep the defaults.
    let mut plain = franken_markdown::wasm::WasmRenderOptions::default();
    plain.apply_typography_tokens(None).unwrap();
    plain.apply_typography_tokens(Some("")).unwrap();
    assert_eq!(
        franken_markdown::wasm::render_pdf(markdown, &plain)
            .unwrap()
            .bytes,
        franken_markdown::render_pdf(markdown, &franken_markdown::PdfOptions::default()).unwrap()
    );
    let err = plain.apply_typography_tokens(Some("kerning")).unwrap_err();
    assert!(err.contains("kerning"), "{err}");
}

#[test]
fn wasm_cff_host_fonts_match_native_html_and_pdf_for_every_slot() {
    use franken_markdown::{FontEmbedding, HtmlFontFormat, HtmlOptions, PdfOptions};
    let source = "Regular café Ω. **Bold café Ω.** *Italic café Ω.* \
        ***Bold italic café Ω.*** `Mono café Ω.`";
    let font = include_bytes!("fixtures/fonts/source-sans-3/SourceSans3-Regular.otf");
    let mut options = WasmRenderOptions {
        html_font_format: HtmlFontFormat::Ttf,
        metadata_epoch_seconds: Some(1_700_000_000),
        ..WasmRenderOptions::default()
    };
    for slot in [
        FontAssetSlot::BodyRegular,
        FontAssetSlot::BodyBold,
        FontAssetSlot::BodyItalic,
        FontAssetSlot::BodyBoldItalic,
        FontAssetSlot::MonoRegular,
    ] {
        options = options.with_font_asset_bytes(slot, font.to_vec()).unwrap();
    }
    let doc = franken_markdown::parse_markdown(source);
    for mode in [FontEmbedding::Subset, FontEmbedding::Full] {
        options.font_embedding = mode;
        let output = render_html(source, &options).unwrap();
        assert_eq!(
            output
                .html()
                .unwrap()
                .matches("data:font/otf;base64,")
                .count(),
            5
        );
        let native = franken_markdown::render_html_document(
            &doc,
            &HtmlOptions {
                font_assets: options.font_assets.clone(),
                html_font_format: HtmlFontFormat::Ttf,
                font_embedding: mode,
                ..HtmlOptions::default()
            },
        )
        .unwrap();
        assert_eq!(output.bytes, native.into_bytes());
    }
    let output = render_pdf(source, &options).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&output.bytes)
            .matches("/Subtype /CIDFontType0")
            .count(),
        5
    );
    let native = franken_markdown::render_pdf_document(
        &doc,
        &PdfOptions {
            font_assets: options.font_assets.clone(),
            metadata_epoch_seconds: options.metadata_epoch_seconds,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    assert_eq!(output.bytes, native);
}

const SOURCE_METADATA_DOCUMENT: &str = "---\ntitle: Metadata title\nauthor: Source author\nlang: de\ntoc: true\ntoc_depth: 2\n---\n# Heading\n\n## Child\n\n### Deep heading\n\nDocument body.\n";

#[test]
fn wasm_source_metadata_uses_native_frontmatter_with_bom_and_all_line_endings() {
    use franken_markdown::wasm::{WasmSourceMetadata, source_metadata};
    let expected = WasmSourceMetadata {
        title: Some("Metadata title".into()),
        author: Some("Source author".into()),
        lang: Some("de".into()),
        toc: Some(true),
        toc_depth: Some(2),
    };
    for newline in ["\n", "\r\n", "\r"] {
        for bom in ["", "\u{feff}"] {
            let source = format!("{bom}{}", SOURCE_METADATA_DOCUMENT.replace('\n', newline));
            assert_eq!(source_metadata(&source), expected);
        }
    }
    assert_eq!(
        source_metadata(SOURCE_METADATA_DOCUMENT).to_json(),
        "{\"title\":\"Metadata title\",\"author\":\"Source author\",\"lang\":\"de\",\"toc\":true,\"tocDepth\":2}"
    );
}

#[test]
fn wasm_source_metadata_keeps_native_scalar_grammar_and_explicit_false() {
    use franken_markdown::wasm::source_metadata;
    let source = "---\ntitle: >-\n  A \"quoted\" guide:\n  café = yes\nauthor: |+\n  First\n  Second\nlang=fr\ntoc=off\ntoc_depth=6\nunknown: ignored\n---\n# Body";
    let metadata = source_metadata(source);
    assert_eq!(
        metadata.title.as_deref(),
        Some("A \"quoted\" guide: café = yes")
    );
    assert_eq!(metadata.author.as_deref(), Some("First\nSecond\n"));
    assert_eq!(metadata.lang.as_deref(), Some("fr"));
    assert_eq!(metadata.toc, Some(false));
    assert_eq!(metadata.toc_depth, Some(6));
    assert_eq!(
        metadata.to_json(),
        "{\"title\":\"A \\\"quoted\\\" guide: café = yes\",\"author\":\"First\\nSecond\\n\",\"lang\":\"fr\",\"toc\":false,\"tocDepth\":6}"
    );
    let quoted = source_metadata("---\ntitle: '  Draft  '\nauthor: \"\"\n---\n# Body");
    assert_eq!(quoted.title.as_deref(), Some("  Draft  "));
    assert_eq!(quoted.author.as_deref(), Some(""));
}

#[test]
fn wasm_source_metadata_does_not_invent_settings_from_rules_or_invalid_frontmatter() {
    use franken_markdown::wasm::{WasmSourceMetadata, source_metadata};
    for source in [
        "# Ordinary document",
        "---\n\nAn ordinary paragraph.",
        "---\ntitle: Not closed\n",
        "---\ntitle: Incomplete\nordinary prose\n---\n# Body",
        "# Body\n\n---\ntitle: Too late\n---",
        "---\n# Only a comment\n---\n# Body",
        "---\nunknown: not an option\ntoc: perhaps\ntoc_depth: 7\n---\n# Body",
    ] {
        assert_eq!(
            source_metadata(source),
            WasmSourceMetadata::default(),
            "{source:?}"
        );
        assert_eq!(source_metadata(source).to_json(), "{}", "{source:?}");
    }
}

#[test]
fn wasm_source_metadata_json_preserves_text_instead_of_replacing_controls() {
    use franken_markdown::wasm::WasmSourceMetadata;
    let metadata = WasmSourceMetadata {
        title: Some("\"\\\0\u{0001}\u{001f}\u{007f} café 😀\n\r\t".into()),
        ..WasmSourceMetadata::default()
    };
    assert_eq!(
        metadata.to_json(),
        "{\"title\":\"\\\"\\\\\\u0000\\u0001\\u001f\u{007f} café 😀\\n\\r\\t\"}"
    );
}

#[cfg(feature = "wasm-bindgen")]
#[test]
fn wasm_source_metadata_abi_exposes_real_parser_defaults() {
    assert_eq!(
        franken_markdown::wasm_abi::source_metadata(SOURCE_METADATA_DOCUMENT),
        "{\"title\":\"Metadata title\",\"author\":\"Source author\",\"lang\":\"de\",\"toc\":true,\"tocDepth\":2}"
    );
    assert_eq!(
        franken_markdown::wasm_abi::source_metadata("---\n\nAn ordinary paragraph."),
        "{}"
    );
}

#[test]
fn wasm_source_metadata_drives_real_html_pdf_and_publication_options() {
    use franken_markdown::wasm::source_metadata;
    use franken_markdown::{HtmlOptions, PdfASettings, PdfOptions};

    let metadata = source_metadata(SOURCE_METADATA_DOCUMENT);
    let options = WasmRenderOptions {
        title: metadata.title,
        author: metadata.author,
        lang: metadata.lang,
        toc: metadata.toc.unwrap_or(false),
        toc_depth: metadata.toc_depth,
        custom_css: Some(String::new()),
        ..WasmRenderOptions::default()
    };
    // These are the independently specified settings the native CLI resolves
    // from this fixture. The source remains whole, preserving byte offsets.
    let native_html = HtmlOptions {
        title: Some("Metadata title".into()),
        lang: Some("de".into()),
        toc: true,
        toc_depth: Some(2),
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    };
    let native_pdf = PdfOptions {
        title: Some("Metadata title".into()),
        author: Some("Source author".into()),
        lang: Some("de".into()),
        toc: true,
        toc_depth: Some(2),
        ..PdfOptions::default()
    };
    let html = render_html(SOURCE_METADATA_DOCUMENT, &options).unwrap();
    assert_eq!(html.source_len, SOURCE_METADATA_DOCUMENT.len());
    let text = html.html().unwrap();
    assert!(text.contains("<title>Metadata title</title>"));
    assert!(text.contains("<html lang=\"de\">"));
    let toc = text
        .split_once("<nav class=\"toc\">")
        .unwrap()
        .1
        .split_once("</nav>")
        .unwrap()
        .0;
    assert!(toc.contains("href=\"#child\""));
    assert!(!toc.contains("href=\"#deep-heading\""));
    assert!(!text.contains("toc_depth:"));
    assert_eq!(
        html.bytes,
        franken_markdown::render_html(SOURCE_METADATA_DOCUMENT, &native_html)
            .unwrap()
            .into_bytes()
    );

    let pdf = render_pdf(SOURCE_METADATA_DOCUMENT, &options).unwrap();
    assert_eq!(pdf.source_len, SOURCE_METADATA_DOCUMENT.len());
    let text = String::from_utf8_lossy(&pdf.bytes);
    assert!(text.contains("/Title (Metadata title)"));
    assert!(text.contains("/Author (Source author)"));
    assert!(text.contains("/Lang (de)"));
    assert_eq!(
        pdf.bytes,
        franken_markdown::render_pdf(SOURCE_METADATA_DOCUMENT, &native_pdf).unwrap()
    );

    let doc = franken_markdown::parse_markdown(SOURCE_METADATA_DOCUMENT);
    let resolved_html = HtmlOptions {
        title: options.title.clone(),
        lang: options.lang.clone(),
        toc: options.toc,
        toc_depth: options.toc_depth,
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    };
    assert_eq!(
        franken_markdown::render_epub(&doc, &resolved_html).unwrap(),
        franken_markdown::render_epub(&doc, &native_html).unwrap()
    );
    let resolved_pdf = PdfOptions {
        title: options.title,
        author: options.author,
        lang: options.lang,
        toc: options.toc,
        toc_depth: options.toc_depth,
        ..PdfOptions::default()
    };
    let archival =
        franken_markdown::render_pdf_document_pdfa(&doc, &resolved_pdf, PdfASettings::a2b())
            .unwrap();
    assert_eq!(
        archival,
        franken_markdown::render_pdf_document_pdfa(&doc, &native_pdf, PdfASettings::a2b()).unwrap()
    );
    assert!(String::from_utf8_lossy(&archival).contains("/Metadata "));
}
