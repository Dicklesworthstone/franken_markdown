//! Reported book exports reuse the ordinary native writer and its result ABI.
//! No chapter source is reparsed and no diagnostic-only layout is performed.

use wasm_bindgen::prelude::*;

use super::{FmdBook, to_js};
use crate::book::{BookPdfRenderReport, BookRenderer};
use crate::wasm_abi::{FmdRenderResult, book_pdf_render_result};
use crate::{PageStyle, PdfASettings, RenderError, Result};

#[wasm_bindgen]
impl FmdBook {
    /// Render a PDF plus bounded, coded fidelity diagnostics from that exact
    /// emission. A clean audited render returns `[]`; unavailable chapter
    /// source coordinates use `0..0`. Parser findings remain sourceDiagnostics.
    ///
    /// The caller owns the returned FmdRenderResult and must free it after
    /// copying its bytes and diagnostics. Freeing it does not dispose the book.
    ///
    /// # Errors
    /// Returns the same validation and writer failures as renderPdf.
    #[wasm_bindgen(js_name = renderPdfReported)]
    pub fn render_pdf_reported(&self) -> std::result::Result<FmdRenderResult, JsValue> {
        self.renderer
            .render_pdf_reported()
            .map(|report| book_pdf_render_result(report, self.source_length()))
            .map_err(to_js)
    }

    /// Apply export-local paper geometry and return the PDF's actual fidelity
    /// diagnostics. Empty geometry uses retained settings; the book's source,
    /// presentation, grid, fonts and assets remain unchanged after this call.
    ///
    /// # Errors
    /// Returns the same failures as renderPdfWithPage.
    #[wasm_bindgen(js_name = renderPdfWithPageReported)]
    pub fn render_pdf_with_page_reported(
        &self,
        page_geometry: Vec<f64>,
    ) -> std::result::Result<FmdRenderResult, JsValue> {
        render_with_geometry(&self.renderer, &page_geometry)
            .map(|report| book_pdf_render_result(report, self.source_length()))
            .map_err(to_js)
    }
}

fn render_with_geometry(renderer: &BookRenderer, geometry: &[f64]) -> Result<BookPdfRenderReport> {
    let page = PageStyle::from_browser_geometry(geometry)
        .map_err(|message| RenderError::InvalidInput(message.to_string()))?;
    let Some(mut page) = page else {
        return renderer.render_pdf_reported();
    };
    let mut options = renderer.options().pdf_options();
    page.baseline_grid = options.theme.page.baseline_grid;
    options.theme.page = page;
    renderer.render_pdf_with_options_reported(&options, PdfASettings::OFF)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::book::BookInput;

    #[test]
    fn reported_geometry_matches_bytes_and_keeps_retained_settings() {
        let book = BookRenderer::new(&[BookInput {
            path: "one.md".into(),
            source: "# One\n\n![missing](figure.png)\n".into(),
        }])
        .unwrap();
        let before = book.render_pdf().unwrap();
        let page = book.options().theme.page;
        for geometry in [&[][..], &[540.0, 720.0, 36.0, 30.0, 24.0, 18.0][..]] {
            let report = render_with_geometry(&book, geometry).unwrap();
            assert_eq!(
                report.bytes,
                super::super::render_pdf_with_geometry(&book, geometry).unwrap()
            );
            assert_eq!(report.warnings[0].code(), "unresolved_image");
            assert_eq!(book.options().theme.page, page);
        }
        assert!(render_with_geometry(&book, &[1.0]).is_err());
        assert_eq!(book.render_pdf().unwrap(), before);
    }
}
