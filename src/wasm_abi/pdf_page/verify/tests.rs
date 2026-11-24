#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{BaselineGrid, FontAssetSlot, Profile, RenderError};

const SOURCE: &str = "# Statement\n\nTerm\n: A measured definition.\n\nA café and a note[^n]. [Known](#statement) [Missing](#absent).\n\n| Item | Amount |\n|---|---:|\n| Revenue | 120.50 |\n\n![Chart](chart.svg)\n\n```rust\nlet amount = 1;\n```\n\n[^n]: A retained footnote.\n";
const LOGO: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#369"/></svg>"##;
const TYPOGRAPHY: &str = "expansion,homogeneous,footnote-backlinks,theme-spacing,line-height=1.5,table-padding-x=0.6,table-padding-y=0.4";

fn native_options(profile: Profile, baseline_grid_pt: Option<f64>) -> WasmRenderOptions {
    let mut options = WasmRenderOptions::serif();
    options.title = Some("  Statement  ".into());
    options.author = Some("Author".into());
    options.metadata_epoch_seconds = Some(0);
    options.theme.page =
        crate::PageStyle::from_browser_geometry(&[720.0, 540.0, 36.0, 30.0, 32.0, 18.0])
            .unwrap()
            .unwrap();
    options.theme.page.baseline_grid =
        baseline_grid_pt.map(|points| BaselineGrid::from_points(points).unwrap());
    options.base_font_size = Some(12.0);
    options.heading_scale = Some(1.3);
    options.table_font_size = Some(9.0);
    options.font_scale = Some(1.125);
    options.lang = Some("de".into());
    options.toc = true;
    options.toc_depth = Some(2);
    options.page_numbers = true;
    options.code_line_numbers = true;
    options.profile = Some(profile);
    options.apply_typography_tokens(Some(TYPOGRAPHY)).unwrap();
    options.pdf_image_assets.push(crate::PdfImageAsset {
        destination: "chart.svg".into(),
        bytes: LOGO.to_vec(),
    });
    // The supplied sans face differs from the selected serif theme, and a
    // static weight pin must survive as a native verification finding.
    options
        .font_assets
        .set_slot(
            FontAssetSlot::BodyRegular,
            fmd_font::bundled::PLEX_REGULAR.to_vec(),
        )
        .unwrap();
    options
        .font_assets
        .set_slot_weight(FontAssetSlot::BodyRegular, 650)
        .unwrap();
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
fn configured_abi_matches_the_complete_native_report_with_assets_profiles_and_grid() {
    for (profile, grid) in [
        (Profile::CommonMarkGfm, None),
        (Profile::GfmPlus, Some(13.1254)),
    ] {
        let actual = verify_pdf_configured(
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
            fmd_font::bundled::PLEX_REGULAR.to_vec(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![650, 0, 0, 0, 0],
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
            Some(TYPOGRAPHY.into()),
            grid,
            Some(profile.as_str().into()),
        )
        .unwrap();
        let options = native_options(profile, grid);
        let document = crate::parse_markdown_with_profile(SOURCE, Some(profile));
        let expected = crate::verify::verify_pdf_result(&document, &options.pdf_options()).unwrap();
        assert_eq!(actual, crate::verify::to_json(&expected));
        assert_eq!(
            actual,
            report_json(&verify_source(SOURCE, &options).unwrap(), REPORT_LIMIT).unwrap()
        );
        assert_eq!(expected.anchors_unresolved, ["absent"]);
        assert!(expected.anchors_resolved > 0);
        assert!(
            expected
                .findings
                .iter()
                .any(|finding| finding.code == "font_weight_ignored_static")
        );
        assert!(
            !expected
                .findings
                .iter()
                .any(|finding| matches!(finding.code, "unresolved_image" | "unsupported_image"))
        );
        assert!(
            expected
                .pages
                .iter()
                .flat_map(|page| &page.runs)
                .any(|run| run.text.contains("retained footnote"))
        );
        // Unlike the older accessibility helper, this includes layout/asset
        // findings as well as the full body text layer.
        assert_ne!(
            actual,
            crate::verify::to_json(&crate::verify::filter_a11y(expected))
        );
    }
}

#[test]
fn configured_fit_target_changes_the_actual_text_layer() {
    let source = (0..13)
        .map(|index| format!("ROW{index:02}"))
        .collect::<Vec<_>>()
        .join("  \n");
    let geometry = vec![240.0, 144.0, 12.0, 24.0, 6.0, 0.0];
    let mut options = WasmRenderOptions::default();
    options.theme.page = crate::PageStyle::from_browser_geometry(&geometry)
        .unwrap()
        .unwrap();
    let original = verify_source(&source, &options).unwrap();
    assert!(original.page_count > 1);
    options.fit_to_pages = Some(1);
    let expected =
        crate::verify::verify_pdf_result(&crate::parse_markdown(&source), &options.pdf_options())
            .unwrap();
    assert_eq!(expected.page_count, 1);
    assert_eq!(expected.pages[0].runs.len(), 13);
    assert!(expected.pages[0].runs.iter().all(|run| run.size < 11.0));
    let actual = verify_pdf_configured(
        &source,
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        None,
        None,
        None,
        false,
        None,
        None,
        false,
        None,
        Some(1),
        false,
        geometry,
        vec![],
        false,
        false,
        false,
        vec![],
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(actual, crate::verify::to_json(&expected));
    assert_ne!(actual, crate::verify::to_json(&original));
}

#[test]
fn bounded_parser_findings_keep_source_byte_offsets_and_recompute_the_digest() {
    let hostile = format!("{}leaf{}", "[".repeat(32), "](/target)".repeat(32));
    let source = format!("# Café\r\n\r\n{hostile}\r\n\r\nA final paragraph.\r\n");
    let parsed = crate::parse_markdown_spanned(&source);
    let diagnostic = parsed
        .diagnostics
        .iter()
        .find(|item| item.code == "inline_work_limit")
        .unwrap();
    let finding = VerifyFinding {
        code: diagnostic.code,
        detail: format!(
            "{} (source bytes {}..{})",
            diagnostic.message, diagnostic.span.start, diagnostic.span.end
        ),
    };
    assert_eq!(diagnostic.span.slice(&source), Some(hostile.as_str()));
    let options = WasmRenderOptions::default();
    let base =
        crate::verify::verify_pdf_result(&parsed.into_document(), &options.pdf_options()).unwrap();
    let actual = verify_source(&source, &options).unwrap();
    assert_ne!(actual.digest, base.digest);
    assert_eq!(actual.verdict, "findings");
    assert!(actual.findings.contains(&finding));
    assert_eq!(
        actual,
        crate::verify::with_extra_findings(base, vec![finding])
    );
    assert!(
        actual
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text == "A final paragraph.")
    );
}

#[test]
fn strict_pagination_and_running_admission_keep_native_failures() {
    let source = "A  \nB  \nC";
    let mut options = WasmRenderOptions::default();
    options.base_font_size = Some(24.0);
    options.strict_pagination = true;
    options.theme.page.margins.top_pt = 360.0;
    options.theme.page.margins.bottom_pt = 360.0;
    let actual = verify_source(source, &options).unwrap_err();
    let published = wasm::render_pdf(source, &options).unwrap_err();
    assert!(matches!(actual, RenderError::Pagination(_)));
    assert_eq!(actual.code(), "pagination_infeasible");
    assert_eq!(actual.to_string(), published.to_string());
    let abi_error = super::super::super::render_error_message(&actual);
    assert!(abi_error.contains("\"code\":\"PAGINATION_INFEASIBLE\""));

    let mut running = WasmRenderOptions::default();
    running.theme.page.margins.top_pt = 0.0;
    running.running.header.left = Some("Document header".into());
    let actual = verify_source("Body text.", &running).unwrap_err();
    let published = wasm::render_pdf("Body text.", &running).unwrap_err();
    assert_eq!(actual.to_string(), published.to_string());
    assert!(
        actual
            .to_string()
            .contains("pdf_running_header_does_not_fit")
    );
}

#[test]
fn report_size_is_measured_in_utf8_bytes_after_json_escaping() {
    let report = verify_source(
        "# Café\n\nA quoted \"word\" and a backslash \\\\.",
        &WasmRenderOptions::default(),
    )
    .unwrap();
    let json = crate::verify::to_json(&report);
    assert!(json.len() > json.chars().count());
    assert_eq!(report_json(&report, json.len()).unwrap(), json);
    let error = report_json(&report, json.len() - 1).unwrap_err();
    assert!(matches!(error, RenderError::InvalidInput(_)));
    assert!(
        error
            .to_string()
            .contains("PDF verification report exceeds")
    );
}
