//! Failed variable-font requests must not publish the uninstanced face.
//! The saved metric fixture has independent FontTools expectations; mutations
//! below retain a parseable sfnt and its wght axis while refusing instancing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::Font;
use franken_markdown::wasm::{self, WasmRenderOptions};
use franken_markdown::{
    FontAssetSlot, FontAssets, FontEmbedding, HtmlOptions, PdfASettings, PdfEmitOptions,
    PdfOptions, RenderError, RenderWarning, SvgOptions, parse_markdown, render_html,
    render_html_document_to_writer, render_pdf, render_pdf_document_emitted,
    render_pdf_document_pdfa, render_pdf_document_profiled, render_pdf_document_reported,
    render_pdf_document_with_diagnostics, render_pdf_with_diagnostics,
};

const VARIABLE: &[u8] = include_bytes!("../fmd-font/fonts/test-variable/FmdMappedMetricsVF.ttf");
const TEXT: &str = "AB **AB** *AB* ***AB*** `AB`";

fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let count = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
    (0..count)
        .find_map(|index| {
            let record = 12 + 16 * index;
            (&bytes[record..record + 4] == tag).then(|| {
                u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize
            })
        })
        .expect("fixture contains table")
}

fn malformed_metrics(tag: &[u8; 4]) -> Vec<u8> {
    let mut bytes = VARIABLE.to_vec();
    let table = table_offset(&bytes, tag);
    bytes[table..table + 2].copy_from_slice(&2u16.to_be_bytes());
    let font = Font::parse(bytes.clone()).unwrap();
    assert!(font.instance_bounds(*b"wght").is_some());
    assert!(font.instance(650.0).is_none());
    bytes
}

fn assets(slot: FontAssetSlot, bytes: Vec<u8>, weight: u16) -> FontAssets {
    FontAssets::default()
        .with_slot(slot, bytes)
        .unwrap()
        .with_slot_weight(slot, weight)
        .unwrap()
}

fn assert_instance_error<T>(result: franken_markdown::Result<T>, slot: FontAssetSlot, weight: u16) {
    let error = result.err().expect("failed instance must refuse output");
    assert!(matches!(error, RenderError::InvalidInput(_)), "{error}");
    let message = error.to_string();
    assert!(message.contains("font_instance:"), "{message}");
    assert!(message.contains(slot.as_str()), "{message}");
    assert!(message.contains(&weight.to_string()), "{message}");
}

#[test]
fn html_refuses_failed_instances_in_every_slot_and_embedding_mode_before_writing() {
    let document = parse_markdown(TEXT);
    for slot in FontAssetSlot::ALL {
        for embedding in [FontEmbedding::Subset, FontEmbedding::Full] {
            let options = HtmlOptions {
                font_assets: assets(slot, malformed_metrics(b"HVAR"), 650),
                font_embedding: embedding,
                ..HtmlOptions::default()
            };
            assert_instance_error(render_html(TEXT, &options), slot, 650);
            let mut sink = b"existing caller data".to_vec();
            assert_instance_error(
                render_html_document_to_writer(&document, &options, &mut sink),
                slot,
                650,
            );
            assert_eq!(sink, b"existing caller data", "validation precedes writes");
        }
    }
}

#[test]
fn pdf_refuses_failed_metric_instances_in_fitted_reported_and_archival_paths() {
    let document = parse_markdown("AB[^note] [Return](#fn-note)\n\n[^note]: **AB**");
    for table in [b"HVAR", b"MVAR"] {
        let options = PdfOptions {
            font_assets: assets(FontAssetSlot::BodyRegular, malformed_metrics(table), 650),
            fit_to_pages: Some(1),
            ..PdfOptions::default()
        };
        for emit in [PdfEmitOptions::default(), PdfEmitOptions::monolithic()] {
            assert_instance_error(
                render_pdf_document_emitted(&document, &options, emit),
                FontAssetSlot::BodyRegular,
                650,
            );
            assert_instance_error(
                render_pdf_document_reported(&document, &options, emit),
                FontAssetSlot::BodyRegular,
                650,
            );
            assert_instance_error(
                render_pdf_document_with_diagnostics(
                    &document,
                    &options,
                    PdfASettings::a2b_strict(),
                    emit,
                ),
                FontAssetSlot::BodyRegular,
                650,
            );
        }
        assert_instance_error(
            render_pdf_document_profiled(&document, &options),
            FontAssetSlot::BodyRegular,
            650,
        );
        assert_instance_error(
            render_pdf_document_pdfa(&document, &options, PdfASettings::a2b_strict()),
            FontAssetSlot::BodyRegular,
            650,
        );
    }
}

#[test]
fn native_browser_facades_preserve_the_same_typed_refusal() {
    let options = WasmRenderOptions {
        font_assets: assets(FontAssetSlot::BodyRegular, malformed_metrics(b"MVAR"), 650),
        ..WasmRenderOptions::default()
    };
    assert_instance_error(
        wasm::render_html(TEXT, &options),
        FontAssetSlot::BodyRegular,
        650,
    );
    assert_instance_error(
        wasm::render_pdf(TEXT, &options),
        FontAssetSlot::BodyRegular,
        650,
    );
}

#[test]
fn checked_editable_html_and_custom_stylesheets_cannot_hide_instance_failure() {
    let document = parse_markdown(TEXT);
    let options = HtmlOptions {
        font_assets: assets(FontAssetSlot::BodyRegular, malformed_metrics(b"HVAR"), 650),
        custom_css: Some(".fmd { color: navy }".into()),
        ..HtmlOptions::default()
    };
    assert_instance_error(render_html(TEXT, &options), FontAssetSlot::BodyRegular, 650);
    assert_instance_error(
        franken_markdown::render_interactive_html_checked(&document, TEXT, &options),
        FontAssetSlot::BodyRegular,
        650,
    );
    let defaults = HtmlOptions::default();
    assert_eq!(
        franken_markdown::render_interactive_html_checked(&document, TEXT, &defaults).unwrap(),
        franken_markdown::render_interactive_html(&document, TEXT, &defaults),
        "checked admission leaves fitting default output unchanged",
    );
}

#[test]
fn existing_svg_and_epub_refuse_the_same_uninstantiable_font() {
    let document = parse_markdown(TEXT);
    let fonts = assets(FontAssetSlot::BodyRegular, malformed_metrics(b"HVAR"), 650);
    for error in [
        franken_markdown::svg::render_svg_with_resources(
            &document,
            &SvgOptions::default(),
            &fonts,
            &[],
        )
        .err()
        .expect("SVG already refuses a failed instance"),
        franken_markdown::render_epub(
            &document,
            &HtmlOptions {
                font_assets: fonts,
                ..HtmlOptions::default()
            },
        )
        .err()
        .expect("EPUB already refuses a failed instance"),
    ] {
        assert!(matches!(error, RenderError::InvalidInput(_)), "{error}");
        assert!(error.to_string().contains("body-regular"), "{error}");
    }
}

#[test]
fn inherited_bold_must_instance_at_its_own_weight_before_publication() {
    let mut bytes = VARIABLE.to_vec();
    let os2 = table_offset(&bytes, b"OS/2");
    // The default location is valid; the fixture's positive hasc delta at 700
    // would overflow the signed OS/2 metric. Regular success cannot certify bold.
    bytes[os2 + 68..os2 + 70].copy_from_slice(&32_760i16.to_be_bytes());
    let font = Font::parse(bytes.clone()).unwrap();
    assert!(font.instance(400.0).is_some());
    assert!(font.instance(700.0).is_none());
    let fonts = assets(FontAssetSlot::BodyRegular, bytes, 400);
    assert_instance_error(
        render_html(
            TEXT,
            &HtmlOptions {
                font_assets: fonts.clone(),
                ..HtmlOptions::default()
            },
        ),
        FontAssetSlot::BodyBold,
        700,
    );
    assert_instance_error(
        render_pdf(
            TEXT,
            &PdfOptions {
                font_assets: fonts.clone(),
                ..PdfOptions::default()
            },
        ),
        FontAssetSlot::BodyBold,
        700,
    );
    let replacement = fonts
        .with_slot(
            FontAssetSlot::BodyBold,
            franken_markdown::fonts::body_bytes(
                franken_markdown::FontFamily::Sans,
                franken_markdown::fonts::FontStyle::Bold,
            )
            .to_vec(),
        )
        .unwrap();
    assert!(
        render_html(
            TEXT,
            &HtmlOptions {
                font_assets: replacement.clone(),
                ..HtmlOptions::default()
            }
        )
        .is_ok()
    );
    assert!(
        render_pdf(
            TEXT,
            &PdfOptions {
                font_assets: replacement,
                ..PdfOptions::default()
            }
        )
        .is_ok()
    );
}

#[test]
fn static_weight_pins_keep_bytes_and_the_existing_warning() {
    let fonts = FontAssets::default()
        .with_slot(
            FontAssetSlot::BodyRegular,
            franken_markdown::fonts::body_bytes(
                franken_markdown::FontFamily::Sans,
                franken_markdown::fonts::FontStyle::Regular,
            )
            .to_vec(),
        )
        .unwrap();
    let pinned = fonts
        .clone()
        .with_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
    let plain_html = render_html(
        TEXT,
        &HtmlOptions {
            font_assets: fonts.clone(),
            ..HtmlOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        plain_html,
        render_html(
            TEXT,
            &HtmlOptions {
                font_assets: pinned.clone(),
                ..HtmlOptions::default()
            }
        )
        .unwrap()
    );
    let plain_pdf = render_pdf(
        TEXT,
        &PdfOptions {
            font_assets: fonts,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let reported = render_pdf_with_diagnostics(
        TEXT,
        &PdfOptions {
            font_assets: pinned,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    assert_eq!(plain_pdf, reported.bytes);
    assert!(reported.warnings.iter().any(|warning| matches!(
        warning,
        RenderWarning::FontWeightIgnoredStatic { weight: 650, .. }
    )));
}

#[test]
fn failed_variable_instances_are_not_mislabeled_as_static_weight_warnings() {
    let options = PdfOptions {
        font_assets: assets(FontAssetSlot::BodyRegular, malformed_metrics(b"HVAR"), 650),
        ..PdfOptions::default()
    };
    let warnings = franken_markdown::render_warnings(&parse_markdown(TEXT), &options);
    assert!(
        !warnings
            .iter()
            .any(|warning| matches!(warning, RenderWarning::FontWeightIgnoredStatic { .. }))
    );
}

#[cfg(feature = "cli")]
#[test]
fn cli_refuses_all_affected_publications_before_replacing_files_or_writing_stdout() {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "fmd-variable-refusal-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let font = directory.join("bad.ttf");
    fs::write(&font, malformed_metrics(b"HVAR")).unwrap();
    let mapping = format!("body-regular={}", font.display());
    for format in ["html", "pdf", "both", "interactive-html"] {
        let output_path = directory.join(format!("{format}.html"));
        let companion = output_path.with_extension("pdf");
        fs::write(&output_path, b"previous publication").unwrap();
        fs::write(&companion, b"previous PDF").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .args([
                "--no-config",
                "--text",
                TEXT,
                "--pdf-font",
                &mapping,
                "--pdf-font-weight",
                "650",
                "--to",
                format,
                "--out",
            ])
            .arg(&output_path);
        let result = command.output().unwrap();
        assert_eq!(
            result.status.code(),
            Some(70),
            "{format}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty(), "{format}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("font_instance:"),
            "{format}"
        );
        assert_eq!(
            fs::read(output_path).unwrap(),
            b"previous publication",
            "{format}"
        );
        assert_eq!(fs::read(companion).unwrap(), b"previous PDF", "{format}");
    }
    for format in ["html", "pdf"] {
        let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args([
                "--no-config",
                "--text",
                TEXT,
                "--pdf-font",
                &mapping,
                "--pdf-font-weight",
                "650",
                "--to",
                format,
                "--out",
                "-",
            ])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(70), "{format}");
        assert!(
            result.stdout.is_empty(),
            "{format} must refuse before any document bytes"
        );
    }
}
