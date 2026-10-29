#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;

const SOURCE: &str = "# Statement\n\nA café and a note[^n].\n\n| Item | Amount |\n|---|---:|\n| Revenue | 120.50 |\n\n![Chart](chart.svg)\n\n[^n]: A retained footnote.\n";
const LOGO: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#369"/></svg>"##;

fn options() -> WasmRenderOptions {
    let mut options = WasmRenderOptions::serif();
    options.title = Some("  Statement  ".into());
    options.author = Some("Author".into());
    options.metadata_epoch_seconds = Some(0);
    options.theme.page =
        crate::PageStyle::from_browser_geometry(&[720.0, 540.0, 36.0, 30.0, 32.0, 18.0])
            .unwrap()
            .unwrap();
    options.base_font_size = Some(12.0);
    options.heading_scale = Some(1.3);
    options.table_font_size = Some(9.0);
    options.font_scale = Some(1.125);
    options.lang = Some("de".into());
    options.toc = true;
    options.toc_depth = Some(2);
    options.page_numbers = true;
    options.code_line_numbers = true;
    options
        .apply_typography_tokens(Some("expansion,homogeneous"))
        .unwrap();
    options.pdf_image_assets.push(crate::PdfImageAsset {
        destination: "chart.svg".into(),
        bytes: LOGO.to_vec(),
    });
    options.running.header.left = Some("{title}".into());
    options.running.header.rule = true;
    options.running.header.image = Some(crate::PdfRunningImage {
        dest: "chart.svg".into(),
        position: crate::PdfRunningImagePosition::Right,
        height_pt: Some(12),
    });
    options.running.footer.center = Some("{page} / {pages}".into());
    options
}

#[test]
fn archival_admission_is_exact_and_does_not_silently_change_profiles() {
    assert_eq!(archival_settings("off", false).unwrap(), PdfASettings::OFF);
    assert_eq!(archival_settings("2b", false).unwrap(), PdfASettings::a2b());
    assert_eq!(
        archival_settings("2b", true).unwrap(),
        PdfASettings::a2b_strict()
    );
    for (profile, strict) in [
        ("off", true),
        ("2B", false),
        ("", false),
        ("3b", true),
        ("PDF/A-2b", false),
    ] {
        assert!(archival_settings(profile, strict).is_err());
    }
}

#[test]
fn direct_archive_matches_the_native_document_writer_not_a_book() {
    let options = options();
    let doc = crate::parse_markdown_spanned(SOURCE).into_document();
    let plain = wasm::render_pdf(SOURCE, &options).unwrap();
    for settings in [PdfASettings::a2b(), PdfASettings::a2b_strict()] {
        let expected =
            crate::render_pdf_document_pdfa(&doc, &options.pdf_options(), settings).unwrap();
        let actual = render_archival(SOURCE, &options, settings).unwrap();
        assert_eq!(actual.bytes, expected);
        assert_eq!(actual, render_archival(SOURCE, &options, settings).unwrap());
        assert_eq!(actual.diagnostics, plain.diagnostics);
        assert_eq!(actual.source_len, SOURCE.len());
        assert_eq!(actual.mime_type, "application/pdf");
        assert_eq!(actual.extension, "pdf");
        let text = String::from_utf8_lossy(&actual.bytes);
        assert!(text.contains("/Metadata"));
        assert!(text.contains("/OutputIntents"));
        assert!(text.contains("/MediaBox [0 0 720 540]"));
        assert!(text.contains("1970-01-01"));
    }
    assert_eq!(wasm::render_pdf(SOURCE, &options).unwrap(), plain);
}

#[test]
fn archival_footer_keeps_rejecting_insufficient_margin() {
    let mut options = options();
    // The original success fixture supplied 24pt below a footer whose baseline
    // and scaled font ascent need 27.2pt. Keep that actual rejection covered;
    // the successful native/ABI parity fixture reserves 32pt instead.
    options.theme.page =
        crate::PageStyle::from_browser_geometry(&[720.0, 540.0, 36.0, 30.0, 24.0, 18.0])
            .unwrap()
            .unwrap();
    for settings in [
        PdfASettings::OFF,
        PdfASettings::a2b(),
        PdfASettings::a2b_strict(),
    ] {
        let error = render_archival(SOURCE, &options, settings).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("pdf_running_footer_does_not_fit"),
            "{error}"
        );
    }
}

#[test]
fn off_is_byte_and_diagnostic_identical_to_the_existing_wasm_route() {
    for source in [
        SOURCE,
        "",
        "# Broken\n\n![Missing](missing.png)\n\n```rust\nfn x() {}",
    ] {
        let options = options();
        assert_eq!(
            render_archival(source, &options, PdfASettings::OFF).unwrap(),
            wasm::render_pdf(source, &options).unwrap()
        );
    }
}

#[test]
fn archival_emission_keeps_missing_resource_and_parser_diagnostics() {
    let source = "# café\n\n![Missing](missing.png)\n\n```rust\nfn x() {}";
    let options = WasmRenderOptions::default();
    let plain = wasm::render_pdf(source, &options).unwrap();
    let archive = render_archival(source, &options, PdfASettings::a2b()).unwrap();
    assert!(!archive.diagnostics.is_empty());
    assert_eq!(archive.diagnostics, plain.diagnostics);
    assert_eq!(archive.source_len, source.len());
}

#[test]
fn strict_failure_is_not_downgraded_and_does_not_poison_later_exports() {
    let source = "# Local\n\n[File](file:///private/document)\n";
    let options = WasmRenderOptions::default();
    let before = wasm::render_pdf(source, &options).unwrap();
    let error = render_archival(source, &options, PdfASettings::a2b_strict()).unwrap_err();
    assert!(error.to_string().contains("pdf_a_file_uri"));
    assert!(render_archival(source, &options, PdfASettings::a2b()).is_ok());
    assert_eq!(wasm::render_pdf(source, &options).unwrap(), before);
}

#[test]
fn exported_archival_abi_keeps_all_document_settings_and_running_image_slots() {
    assert_archival_abi_settings(None);
}

#[test]
fn exported_archival_abi_preserves_grid_with_page_running_images_and_typography() {
    assert_archival_abi_settings(Some(13.1254));
}

fn assert_archival_abi_settings(baseline_grid_pt: Option<f64>) {
    let actual = render_pdf_configured_archival(
        SOURCE,
        Some("serif".into()),
        None,
        Some("  Statement  ".into()),
        Some("Author".into()),
        Some(0.0),
        false,
        true,
        vec!["chart.svg".into()],
        LOGO.to_vec(),
        vec![LOGO.len() as u32],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        Some(12.0),
        Some(1.3),
        Some(9.0),
        true,
        Some(1.125),
        Some("de".into()),
        true,
        Some(2),
        None,
        false,
        vec![720.0, 540.0, 36.0, 30.0, 32.0, 18.0],
        ["{title}", "", "", "", "{page} / {pages}", ""]
            .map(String::from)
            .to_vec(),
        true,
        false,
        false,
        vec!["chart.svg".into(), String::new()],
        vec![1, 0],
        vec![12, 0],
        Some("expansion,homogeneous".into()),
        "2b",
        true,
        baseline_grid_pt,
    )
    .unwrap();
    let mut expected_options = options();
    expected_options.theme.page.baseline_grid =
        baseline_grid_pt.map(|spacing| crate::BaselineGrid::from_points(spacing).unwrap());
    let expected = render_archival(SOURCE, &expected_options, PdfASettings::a2b_strict()).unwrap();
    assert_eq!(actual.bytes(), expected.bytes);
    assert_eq!(actual.diagnostics_json(), expected.diagnostics_json());
    assert_eq!(actual.source_length(), SOURCE.len());
}

#[test]
fn raw_abi_resource_admission_refuses_ambiguous_or_broken_packs() {
    let empty: [&[u8]; 5] = [&[]; 5];
    assert!(admit_inputs("", &[], &[], &[], empty).is_ok());
    assert!(admit_inputs("x", &["a".into()], &[7], &[1], empty).is_ok());
    for (destinations, bytes, lengths) in [
        (vec!["a".into()], vec![7], vec![]),
        (vec!["a".into()], vec![7], vec![0]),
        (vec!["a".into()], vec![7], vec![2]),
        (vec!["a".into()], vec![], vec![33 * 1024 * 1024]),
        (vec!["a".into(), "a".into()], vec![7, 8], vec![1, 1]),
        (vec![" ".into()], vec![7], vec![1]),
        (vec!["x".repeat(8193)], vec![7], vec![1]),
        (vec!["x".into(); 4097], vec![], vec![1; 4097]),
    ] {
        assert!(admit_inputs("x", &destinations, &bytes, &lengths, empty).is_err());
    }
    let destinations: Vec<String> = (0..9)
        .map(|i| format!("{}{}", i, "x".repeat(8191)))
        .collect();
    assert!(admit_inputs("x", &destinations, &[7; 9], &[1; 9], empty).is_err());
}
