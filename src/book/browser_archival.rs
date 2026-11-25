//! Export-local PDF/A settings for the browser's retained book.
//! The core book writer, not a post-processing PDF converter, owns emission.

use wasm_bindgen::prelude::*;

use super::super::{FmdBook, to_js};
use crate::book::{BookPdfRenderReport, BookRenderer};
use crate::wasm_abi::{FmdRenderResult, book_pdf_render_result};
use crate::{PageStyle, PdfASettings, PdfOptions, RenderError, Result};

#[wasm_bindgen]
impl FmdBook {
    /// Emit the retained book with an explicit PDF/A profile and optional paper
    /// geometry (width, height, top, right, bottom, left, in points). Empty
    /// geometry keeps the stored paper. Supported profiles are `off` and `2b`.
    /// Strict mode requires `2b` and uses the existing writer's URI policy.
    ///
    /// Neither successful nor rejected exports change sources, fonts, images,
    /// presentation settings or the source revision. This selects emission
    /// settings; it does not independently certify PDF/A conformance.
    ///
    /// # Errors
    /// Invalid profiles, strictness/profile combinations, paper geometry and
    /// failures from the chapter-aware PDF/A writer are returned to the host.
    #[wasm_bindgen(js_name = renderPdfArchival)]
    pub fn render_pdf_archival(
        &self,
        profile: &str,
        strict: bool,
        page_geometry: Vec<f64>,
    ) -> std::result::Result<Vec<u8>, JsValue> {
        render_archival(&self.renderer, profile, strict, &page_geometry).map_err(to_js)
    }

    /// Emit the same archival book PDF with bounded rendering diagnostics from
    /// its actual layout. Warnings do not claim independent PDF/A validation.
    /// The returned FmdRenderResult is owned separately from this retained book.
    ///
    /// # Errors
    /// Returns the same policy, geometry and writer failures as renderPdfArchival.
    #[wasm_bindgen(js_name = renderPdfArchivalReported)]
    pub fn render_pdf_archival_reported(
        &self,
        profile: &str,
        strict: bool,
        page_geometry: Vec<f64>,
    ) -> std::result::Result<FmdRenderResult, JsValue> {
        render_archival_reported(&self.renderer, profile, strict, &page_geometry)
            .map(|report| book_pdf_render_result(report, self.source_length()))
            .map_err(to_js)
    }

    /// Verify the retained book's actual assembled PDF publication. The JSON
    /// contains positioned page text, chapter-scoped destinations, original
    /// source ranges, findings and pagination provenance. It is bounded to
    /// 8 MiB and is never truncated or replaced by a source-only link check.
    ///
    /// Profile, strictness and optional geometry have exactly the same meaning
    /// as renderPdfArchival. All other settings, fonts and images come from
    /// this book. Verification does not change its source revision or settings
    /// and does not independently certify PDF/A or PDF/UA conformance.
    ///
    /// # Errors
    /// Propagates publication admission, strict pagination and report limits.
    #[wasm_bindgen(js_name = verifyPdf)]
    pub fn verify_pdf(
        &self,
        profile: &str,
        strict: bool,
        page_geometry: Vec<f64>,
    ) -> std::result::Result<String, JsValue> {
        verify_publication(&self.renderer, profile, strict, &page_geometry).map_err(to_js)
    }
}

// Keep admission and the actual writer call independent of JsValue so the
// native test target exercises rejection without invoking a browser import.
fn render_archival(
    renderer: &BookRenderer,
    profile: &str,
    strict: bool,
    geometry: &[f64],
) -> Result<Vec<u8>> {
    let (options, settings) = archival_options(renderer, profile, strict, geometry)?;
    renderer.render_pdf_with_options(&options, settings)
}

fn render_archival_reported(
    renderer: &BookRenderer,
    profile: &str,
    strict: bool,
    geometry: &[f64],
) -> Result<BookPdfRenderReport> {
    let (options, settings) = archival_options(renderer, profile, strict, geometry)?;
    renderer.render_pdf_with_options_reported(&options, settings)
}

fn verify_publication(
    renderer: &BookRenderer,
    profile: &str,
    strict: bool,
    geometry: &[f64],
) -> Result<String> {
    let (options, settings) = archival_options(renderer, profile, strict, geometry)?;
    renderer
        .verify_pdf_with_options(&options, settings)?
        .to_json()
}

fn archival_options(
    renderer: &BookRenderer,
    profile: &str,
    strict: bool,
    geometry: &[f64],
) -> Result<(PdfOptions, PdfASettings)> {
    let settings = match (profile, strict) {
        ("off", false) => PdfASettings::OFF,
        ("2b", false) => PdfASettings::a2b(),
        ("2b", true) => PdfASettings::a2b_strict(),
        ("off", true) => {
            return Err(RenderError::InvalidInput(
                "pdfAStrict requires pdfA: 2b".into(),
            ));
        }
        _ => {
            return Err(RenderError::InvalidInput("pdfA must be off or 2b".into()));
        }
    };
    let page = PageStyle::from_browser_geometry(geometry)
        .map_err(|message| RenderError::InvalidInput(message.to_string()))?;
    let mut options = renderer.options().pdf_options();
    if let Some(mut page) = page {
        page.baseline_grid = options.theme.page.baseline_grid;
        options.theme.page = page;
    }
    Ok((options, settings))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::book::{BookInput, BookSourceLocation, BookWorkspace, LinkSourcePrecision};

    fn workspace() -> BookWorkspace {
        let mut book = BookWorkspace::from_sources(
            &[
                BookInput {
                    path: "guide/one.md".into(),
                    source: "# One\n\n[Next](../two.md#two)\n\n{{#include ../shared.txt}}\n".into(),
                },
                BookInput {
                    path: "two.md".into(),
                    source: "# Two\n\n[Back](guide/one.md#one)\n".into(),
                },
            ],
            &[BookInput {
                path: "shared.txt".into(),
                source: "Shared text and a note[^n].\n\n[^n]: Note text.\n".into(),
            }],
        )
        .unwrap();
        book.options_mut().title = Some("Archived book".into());
        book.options_mut().metadata_epoch_seconds = Some(1_700_000_000);
        book.options_mut().toc = true;
        book
    }

    #[test]
    fn archival_off_preserves_existing_browser_pdf_bytes_and_state() {
        let book = workspace();
        let before = book.render_pdf().unwrap();
        let source_length = book.source_length();
        let revision = book.source_revision();
        assert_eq!(render_archival(&book, "off", false, &[]).unwrap(), before);
        assert_eq!(book.source_length(), source_length);
        assert_eq!(book.source_revision(), revision);
        assert_eq!(book.render_pdf().unwrap(), before);
    }

    #[test]
    fn exported_archival_method_uses_the_existing_chapter_aware_writer() {
        let raw = FmdBook {
            renderer: workspace(),
        };
        for strict in [false, true] {
            let expected = raw
                .renderer
                .render_pdf_pdfa(if strict {
                    PdfASettings::a2b_strict()
                } else {
                    PdfASettings::a2b()
                })
                .unwrap();
            let actual = raw.render_pdf_archival("2b", strict, vec![]).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(
                actual,
                raw.render_pdf_archival("2b", strict, vec![]).unwrap()
            );
            let text = String::from_utf8_lossy(&actual);
            assert!(text.contains("/OutputIntents"));
            assert!(text.contains("/Metadata"));
        }
    }

    #[test]
    fn archival_geometry_is_export_local_and_keeps_chapter_navigation() {
        let book = workspace();
        let before = book.render_pdf().unwrap();
        let page = book.options().theme.page;
        for geometry in [
            [720.0, 540.0, 18.0, 24.0, 30.0, 36.0],
            [540.0, 720.0, 36.0, 30.0, 24.0, 18.0],
        ] {
            let mut options = book.options().pdf_options();
            options.theme.page = PageStyle::from_browser_geometry(&geometry)
                .unwrap()
                .unwrap();
            let expected = book
                .render_pdf_with_options(&options, PdfASettings::a2b_strict())
                .unwrap();
            let actual = render_archival(&book, "2b", true, &geometry).unwrap();
            assert_eq!(actual, expected);
            assert!(
                String::from_utf8_lossy(&actual)
                    .contains(&format!("/MediaBox [0 0 {} {}]", geometry[0], geometry[1]))
            );
        }
        assert_eq!(book.options().theme.page, page);
        assert_eq!(book.render_pdf().unwrap(), before);
    }

    #[test]
    fn invalid_archival_requests_do_not_change_the_retained_book() {
        let book = workspace();
        let before = book.render_pdf().unwrap();
        for (profile, strict) in [("off", true), ("", false), ("2B", true), ("3b", false)] {
            assert!(matches!(
                render_archival(&book, profile, strict, &[]),
                Err(RenderError::InvalidInput(_))
            ));
        }
        for geometry in [
            vec![612.0],
            vec![f64::NAN; 6],
            vec![612.0, 792.0, 72.0, 270.000001, 72.0, 270.0],
        ] {
            assert!(render_archival(&book, "2b", true, &geometry).is_err());
        }
        assert_eq!(book.render_pdf().unwrap(), before);
    }

    #[test]
    fn strict_writer_failure_is_not_replaced_by_an_ordinary_export() {
        let book = BookWorkspace::new(&[BookInput {
            path: "one.md".into(),
            source: "# One\n\n[Local](file:///private/document)\n".into(),
        }])
        .unwrap();
        let before = book.render_pdf().unwrap();
        assert!(render_archival(&book, "2b", true, &[]).is_err());
        assert_eq!(book.render_pdf().unwrap(), before);
        assert!(render_archival(&book, "2b", false, &[]).is_ok());
    }

    fn configured_verification_workspace() -> BookWorkspace {
        let mut book = workspace();
        book.update_sources(&[BookInput {
            path: "shared.txt".into(),
            source: concat!(
                "Shared text and a note[^n].\n\n",
                "Term\n: Definition with **emphasis**.\n\n",
                "![Chart](chart.svg)\n\n[^n]: Retained note text.\n",
            )
            .into(),
        }])
        .unwrap();
        book.set_authoring_profile(Some(crate::Profile::CommonMarkGfm))
            .unwrap();
        book.set_image(
            "chart.svg",
            br#"<svg width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec(),
        )
        .unwrap();
        let options = book.options_mut();
        options.author = Some("Retained author".into());
        options.theme.page.baseline_grid = Some(crate::BaselineGrid::from_points(12.0).unwrap());
        options.use_theme_spacing = true;
        options.pdf_line_height = Some(1.7);
        options.base_font_size = Some(11.0);
        options.heading_scale = Some(1.2);
        options.page_numbers = true;
        options.footnote_backlinks = true;
        options.optimal_pagination = true;
        options.running.header.left = Some("{chapter}".into());
        options
            .font_assets
            .set_slot(
                crate::FontAssetSlot::BodyRegular,
                crate::fonts::body_bytes(
                    crate::FontFamily::Serif,
                    crate::fonts::FontStyle::Regular,
                )
                .to_vec(),
            )
            .unwrap();
        book
    }

    #[test]
    fn verification_export_matches_native_retained_options_assets_and_dialect() {
        let raw = FmdBook {
            renderer: configured_verification_workspace(),
        };
        let geometry = vec![480.0, 540.0, 36.0, 42.0, 30.0, 24.0];
        let mut options = raw.renderer.options().pdf_options();
        let mut page = PageStyle::from_browser_geometry(&geometry)
            .unwrap()
            .unwrap();
        page.baseline_grid = options.theme.page.baseline_grid;
        options.theme.page = page;
        for (profile, strict, settings) in [
            ("off", false, PdfASettings::OFF),
            ("2b", true, PdfASettings::a2b_strict()),
        ] {
            let expected = raw
                .renderer
                .verify_pdf_with_options(&options, settings)
                .unwrap();
            let actual = raw.verify_pdf(profile, strict, geometry.clone()).unwrap();
            assert_eq!(actual, expected.to_json().unwrap());
            assert!(expected.pagination.page_breaks.requested_optimal);
            assert!(expected.destinations.iter().any(|target| {
                target.kind == "footnote_backreference" && target.status == "resolved"
            }));
            assert!(
                expected
                    .layout
                    .pages
                    .iter()
                    .flat_map(|page| &page.runs)
                    .any(|run| { run.text.contains("Retained note text") })
            );
            assert!(!expected.layout.findings.iter().any(|finding| {
                matches!(finding.code, "unresolved_image" | "unsupported_image")
            }));
        }
        // A geometry override must preserve the retained grid. Dropping it
        // changes physical baselines even though the source links still pass.
        let gridded = raw.verify_pdf("off", false, geometry).unwrap();
        options.theme.page.baseline_grid = None;
        assert_ne!(
            gridded,
            raw.renderer
                .verify_pdf_with_options(&options, PdfASettings::OFF)
                .unwrap()
                .to_json()
                .unwrap()
        );
        let preview = String::from_utf8(raw.render_chapter_preview(0.0).unwrap()).unwrap();
        assert!(
            !preview.contains("<dl>"),
            "retained CommonMark dialect must survive"
        );
    }

    #[test]
    fn verification_success_and_admission_failures_preserve_the_retained_book() {
        let book = configured_verification_workspace();
        let bytes = book.render_pdf().unwrap();
        let options = book.options().clone();
        let sources = book.source_diagnostics().to_json();
        let revision = book.source_revision();
        let source_length = book.source_length();
        assert_eq!(
            verify_publication(&book, "off", false, &[]).unwrap(),
            book.verify_pdf().unwrap().to_json().unwrap()
        );
        assert!(
            verify_publication(&book, "2b", true, &[540.0, 720.0, 36.0, 30.0, 24.0, 18.0]).is_ok()
        );
        for (profile, strict) in [("off", true), ("", false), ("2B", true), ("3b", false)] {
            assert!(matches!(
                verify_publication(&book, profile, strict, &[]),
                Err(RenderError::InvalidInput(_))
            ));
        }
        for geometry in [
            vec![612.0],
            vec![f64::NAN; 6],
            vec![612.0, 792.0, 72.0, 300.0, 72.0, 300.0],
        ] {
            assert!(matches!(
                verify_publication(&book, "off", false, &geometry),
                Err(RenderError::InvalidInput(_))
            ));
        }
        assert_eq!(book.source_revision(), revision);
        assert_eq!(book.source_length(), source_length);
        assert_eq!(book.source_diagnostics().to_json(), sources);
        assert_eq!(book.options().theme, options.theme);
        assert_eq!(book.options().font_assets, options.font_assets);
        assert_eq!(book.options().pdf_image_assets, options.pdf_image_assets);
        assert_eq!(book.options().running, options.running);
        assert_eq!(book.options().profile, options.profile);
        assert_eq!(book.options().title, options.title);
        assert_eq!(book.options().author, options.author);
        assert_eq!(book.render_pdf().unwrap(), bytes);
    }

    // Read actual page and annotation dictionaries through the emitted xref.
    // Font/content stream bytes cannot masquerade as PDF objects in this check.
    fn emitted_navigation(bytes: &[u8]) -> (usize, Vec<(usize, f32)>) {
        fn references(dictionary: &str, key: &str) -> Vec<usize> {
            let Some((_, tail)) = dictionary.split_once(key) else {
                return Vec::new();
            };
            let fields: Vec<_> = tail.split_once(']').unwrap().0.split_whitespace().collect();
            assert_eq!(fields.len() % 3, 0);
            fields
                .chunks_exact(3)
                .map(|reference| {
                    assert_eq!(&reference[1..], &["0", "R"]);
                    reference[0].parse().unwrap()
                })
                .collect()
        }
        let marker = b"startxref\n";
        let start = bytes
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap();
        let xref: usize = std::str::from_utf8(&bytes[start + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut rows = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(rows.next(), Some("xref"));
        let count: usize = rows
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(rows.next().unwrap().trim(), "0000000000 65535 f");
        let mut offsets: Vec<usize> = rows
            .take(count - 1)
            .map(|row| {
                let fields: Vec<_> = row.split_whitespace().collect();
                assert_eq!(&fields[1..], &["00000", "n"]);
                fields[0].parse().unwrap()
            })
            .collect();
        offsets.push(xref);
        let objects: Vec<_> = offsets
            .windows(2)
            .enumerate()
            .map(|(index, range)| {
                let object = &bytes[range[0]..range[1]];
                assert!(object.starts_with(format!("{} 0 obj\n", index + 1).as_bytes()));
                let end = object
                    .windows(8)
                    .position(|part| part == b"\nstream\n")
                    .unwrap_or(object.len());
                std::str::from_utf8(&object[..end]).unwrap()
            })
            .collect();
        let page_tree = objects
            .iter()
            .find(|object| object.contains("/Type /Pages "))
            .unwrap();
        let pages = references(page_tree, "/Kids [");
        let mut destinations = Vec::new();
        for &page in &pages {
            let dictionary = objects[page - 1];
            assert!(dictionary.contains("/Type /Page "));
            for annotation in references(dictionary, "/Annots [") {
                let dictionary = objects[annotation - 1];
                assert!(dictionary.contains("/Subtype /Link"));
                assert!(!dictionary.contains("/S /URI"));
                let tail = dictionary.split_once("/Dest [").unwrap().1;
                let fields: Vec<_> = tail.split_once(']').unwrap().0.split_whitespace().collect();
                assert_eq!(&fields[1..5], &["0", "R", "/XYZ", "null"]);
                let page_object: usize = fields[0].parse().unwrap();
                let physical_page = pages.iter().position(|&id| id == page_object).unwrap() + 1;
                let y: f32 = fields[5].parse().unwrap();
                destinations.push((physical_page, y));
            }
        }
        (pages.len(), destinations)
    }

    #[test]
    fn verification_reports_emitted_physical_pages_and_chapter_scoped_targets() {
        let mut renderer = BookWorkspace::new(&[
            BookInput {
                path: "one.md".into(),
                source: "# Same\n\n[LOCAL](#same) [CROSS](two.md#same)".into(),
            },
            BookInput {
                path: "two.md".into(),
                source: "# Same\n\n[SELF](#same) [BACK](one.md#same)".into(),
            },
        ])
        .unwrap();
        renderer.options_mut().toc = true;
        let raw = FmdBook { renderer };
        let report = raw.renderer.verify_pdf().unwrap();
        assert_eq!(
            raw.verify_pdf("off", false, vec![]).unwrap(),
            report.to_json().unwrap()
        );
        let bytes = raw.render_pdf().unwrap();
        let (pages, emitted) = emitted_navigation(&bytes);
        assert_eq!(report.layout.page_count, pages);
        assert_eq!(report.layout.pages.len(), pages);
        assert_eq!(report.destinations.len(), 4);
        assert!(
            emitted.len() > 4,
            "generated contents links precede authored links"
        );
        let authored = &emitted[emitted.len() - 4..];
        for (target, &(page, y)) in report.destinations.iter().zip(authored) {
            assert_eq!(target.status, "resolved");
            assert_eq!(target.page, Some(page));
            assert!((target.y.unwrap() - y).abs() <= 0.011);
        }
        assert!(authored[0].0 > 1, "contents occupies a physical page");
        assert_eq!(authored[0], authored[3]);
        assert_eq!(authored[1], authored[2]);
        assert!(
            authored[0].0 < authored[1].0,
            "duplicate headings stay chapter-scoped"
        );
    }

    #[test]
    fn verification_keeps_include_provenance_and_dormant_pdf_note_findings() {
        let included = "[Missing](#missing-included)\n\n[^unused]: [Dormant](#missing-note)\n";
        let raw = FmdBook {
            renderer: BookWorkspace::from_sources(
                &[BookInput {
                    path: "guide/one.md".into(),
                    source: "# One\n\n{{#include ../parts/body.md}}\n".into(),
                }],
                &[BookInput {
                    path: "parts/body.md".into(),
                    source: included.into(),
                }],
            )
            .unwrap(),
        };
        let report = raw.renderer.verify_pdf().unwrap();
        let json = raw.verify_pdf("off", false, vec![]).unwrap();
        assert_eq!(json, report.to_json().unwrap());
        assert_eq!(report.navigation.report().finding_count(), 1);
        assert_eq!(report.destinations.len(), 2);
        for target in &report.destinations {
            assert_eq!(target.status, "unresolved");
            assert_eq!(target.page, None);
            assert_eq!(target.y, None);
            assert_eq!(target.source.precision, LinkSourcePrecision::Destination);
            let BookSourceLocation::Mapped(ranges) = &target.source.source else {
                panic!("included source destination must retain original ownership");
            };
            assert_eq!(ranges.len(), 1);
            assert_eq!(ranges[0].path, "parts/body.md");
            assert_eq!(
                &included[ranges[0].start..ranges[0].end],
                target.destination
            );
        }
        assert!(report.layout.findings.iter().any(|finding| {
            finding.code == "unresolved_pdf_destination" && finding.detail.contains("#missing-note")
        }));
        assert!(
            report
                .layout
                .pages
                .iter()
                .flat_map(|page| &page.runs)
                .any(|run| run.text.contains("Dormant"))
        );
        assert!(!json.contains("fmd-book-pdf-nav/"));
    }

    #[test]
    fn verification_preserves_strict_pagination_and_archival_writer_errors() {
        let mut book = BookWorkspace::new(&[BookInput {
            path: "one.md".into(),
            source: "A  \nB  \nC".into(),
        }])
        .unwrap();
        book.options_mut().strict_pagination = true;
        book.options_mut().base_font_size = Some(24.0);
        let before = book.render_pdf().unwrap();
        let geometry = [400.0, 144.0, 36.0, 36.0, 36.0, 36.0];
        let mut options = book.options().pdf_options();
        options.theme.page = PageStyle::from_browser_geometry(&geometry)
            .unwrap()
            .unwrap();
        let publication = book
            .render_pdf_with_options(&options, PdfASettings::OFF)
            .unwrap_err();
        let verification = verify_publication(&book, "off", false, &geometry).unwrap_err();
        match (&publication, &verification) {
            (RenderError::Pagination(expected), RenderError::Pagination(actual)) => {
                assert_eq!(actual, expected)
            }
            _ => panic!("typed pagination failure lost: {publication:?}; {verification:?}"),
        }
        assert_eq!(verification.code(), "pagination_infeasible");
        assert_eq!(book.render_pdf().unwrap(), before);
        assert_eq!(book.source_revision(), 0);

        let book = BookWorkspace::new(&[BookInput {
            path: "one.md".into(),
            source: "# One\n\n[Local](file:///private/document)\n".into(),
        }])
        .unwrap();
        let before = book.render_pdf().unwrap();
        let publication = book
            .render_pdf_pdfa(PdfASettings::a2b_strict())
            .unwrap_err();
        let verification = verify_publication(&book, "2b", true, &[]).unwrap_err();
        assert!(
            matches!(&verification, RenderError::InvalidInput(message) if message.starts_with("pdf_a_file_uri:"))
        );
        assert_eq!(verification.to_string(), publication.to_string());
        assert!(verify_publication(&book, "2b", false, &[]).is_ok());
        assert_eq!(book.render_pdf().unwrap(), before);
        assert_eq!(book.source_revision(), 0);
    }

    #[test]
    fn verification_uses_the_latest_include_revision_without_stale_findings() {
        let chapters = [BookInput {
            path: "guide/one.md".into(),
            source: "# One\n\n{{#include ../shared.md}}\n".into(),
        }];
        let mut raw = FmdBook {
            renderer: BookWorkspace::from_sources(
                &chapters,
                &[BookInput {
                    path: "shared.md".into(),
                    source: "Before. [Broken](#absent)\n".into(),
                }],
            )
            .unwrap(),
        };
        raw.renderer.options_mut().toc = true;
        raw.renderer.options_mut().page_numbers = true;
        let before = raw.verify_pdf("off", false, vec![]).unwrap();
        assert!(before.contains("#absent"));
        let changed = "After. [Working](#one)\n";
        raw.update_sources(vec!["shared.md".into()], vec![changed.into()], 0.0)
            .unwrap();
        let actual = raw.verify_pdf("off", false, vec![]).unwrap();
        assert_ne!(actual, before);
        assert!(!actual.contains("#absent"));
        assert_eq!(raw.source_revision(), 1);
        let mut fresh = BookWorkspace::from_sources(
            &chapters,
            &[BookInput {
                path: "shared.md".into(),
                source: changed.into(),
            }],
        )
        .unwrap();
        *fresh.options_mut() = raw.renderer.options().clone();
        let expected = fresh.verify_pdf().unwrap();
        assert_eq!(actual, expected.to_json().unwrap());
        assert_eq!(expected.layout.verdict, "clean");
        assert_eq!(expected.destinations[0].status, "resolved");
        assert_eq!(raw.verify_pdf("off", false, vec![]).unwrap(), actual);
        assert_eq!(raw.source_revision(), 1);
        assert_eq!(raw.render_pdf().unwrap(), fresh.render_pdf().unwrap());
    }
}
