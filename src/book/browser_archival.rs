//! Export-local PDF/A settings for the browser's retained book.
//! The core book writer, not a post-processing PDF converter, owns emission.

use wasm_bindgen::prelude::*;

use super::super::{FmdBook, to_js};
use crate::book::BookRenderer;
use crate::{PageStyle, PdfASettings, RenderError, Result};

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
}

// Keep admission and the actual writer call independent of JsValue so the
// native test target exercises rejection without invoking a browser import.
fn render_archival(
    renderer: &BookRenderer,
    profile: &str,
    strict: bool,
    geometry: &[f64],
) -> Result<Vec<u8>> {
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
    let Some(mut page) = page else {
        return renderer.render_pdf_pdfa(settings);
    };
    let mut options = renderer.options().pdf_options();
    page.baseline_grid = options.theme.page.baseline_grid;
    options.theme.page = page;
    renderer.render_pdf_with_options(&options, settings)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::book::{BookInput, BookWorkspace};

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
}
