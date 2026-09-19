//! Browser adapter for reusable multi-document rendering.
//!
//! The host selects source files and supplies asset bytes. This adapter never
//! fetches URLs or opens files. A single parsed book supports repeated PDF,
//! EPUB, and HTML-site ZIP exports without parsing the chapters again.

use wasm_bindgen::prelude::*;

use super::{BookInput, BookRenderer};
use crate::{DarkModePolicy, FontAssetSlot, FontFamily, RenderError};

/// A parsed book retained across browser exports.
///
/// Construct with parallel arrays of book-relative paths and Markdown sources
/// in reading order. Call `free()` when the book is no longer needed, as with
/// other wasm-bindgen classes. Render methods return `Uint8Array` payloads:
/// PDF (`application/pdf`), EPUB (`application/epub+zip`), and site ZIP
/// (`application/zip`).
#[wasm_bindgen]
pub struct FmdBook {
    renderer: BookRenderer,
}

#[wasm_bindgen]
impl FmdBook {
    /// Parse all chapters once, with bounded source bytes and validated paths.
    ///
    /// # Errors
    /// Rejects mismatched arrays, empty books, invalid paths, collisions,
    /// or the source limits enforced by the pure book renderer.
    #[wasm_bindgen(constructor)]
    pub fn new(paths: Vec<String>, sources: Vec<String>) -> Result<FmdBook, JsValue> {
        let inputs = book_inputs(paths, sources).map_err(JsValue::from_str)?;
        let renderer = BookRenderer::new(&inputs).map_err(to_js)?;
        Ok(Self { renderer })
    }

    /// Expand includes from the selected chapters and optional include-only
    /// resources, then retain one parsed book for every export format.
    /// Resources never become chapters. The original constructor remains
    /// parse-only so hosts can deliberately supply already expanded source.
    ///
    /// # Errors
    /// Rejects mismatched arrays, invalid or duplicate paths, missing sources,
    /// cycles, selectors, and whole-book source/expansion budgets.
    #[wasm_bindgen(js_name = fromSources)]
    pub fn from_sources(
        paths: Vec<String>,
        sources: Vec<String>,
        include_paths: Vec<String>,
        include_sources: Vec<String>,
    ) -> Result<FmdBook, JsValue> {
        let chapters = book_inputs(paths, sources).map_err(JsValue::from_str)?;
        let resources = if include_paths.is_empty() && include_sources.is_empty() {
            Vec::new()
        } else {
            book_inputs(include_paths, include_sources).map_err(JsValue::from_str)?
        };
        let renderer = BookRenderer::from_sources(&chapters, &resources).map_err(to_js)?;
        Ok(Self { renderer })
    }

    /// Number of chapters, in reading order.
    #[wasm_bindgen(getter, js_name = chapterCount)]
    #[must_use]
    pub fn chapter_count(&self) -> usize {
        self.renderer.book().chapters.len()
    }

    /// Original Markdown size in UTF-8 bytes, excluding logical filenames.
    /// Includes include-only resource bytes when constructed by `fromSources`.
    #[wasm_bindgen(getter, js_name = sourceLength)]
    #[must_use]
    pub fn source_length(&self) -> usize {
        self.renderer.source_length()
    }

    /// Set shared metadata. Absent or blank values restore renderer defaults.
    /// Per-chapter frontmatter language overrides the book's language in HTML
    /// and EPUB. Author metadata is used by the PDF path.
    #[wasm_bindgen(js_name = setMetadata)]
    pub fn set_metadata(
        &mut self,
        title: Option<String>,
        author: Option<String>,
        language: Option<String>,
    ) {
        let options = self.renderer.options_mut();
        options.title = nonblank(title);
        options.author = nonblank(author);
        options.lang = nonblank(language);
    }

    /// Replace HTML/EPUB CSS. `undefined` restores the built-in stylesheet;
    /// an empty string deliberately requests no stylesheet rules.
    #[wasm_bindgen(js_name = setCustomCss)]
    pub fn set_custom_css(&mut self, css: Option<String>) {
        self.renderer.options_mut().custom_css = css;
    }

    /// Choose the shared font family and HTML dark-mode policy. `auto` follows
    /// the browser's preferred color scheme; `light` or `disabled` emits only
    /// the default light palette. EPUB uses its own or caller-supplied CSS.
    ///
    /// # Errors
    /// Invalid names leave the previous theme unchanged.
    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&mut self, font: &str, dark_mode: &str) -> Result<(), JsValue> {
        let (font, dark) = theme_settings(font, dark_mode).map_err(JsValue::from_str)?;
        let theme = self.renderer.options().theme.clone()
            .with_font(font).with_dark_mode(dark);
        self.renderer.options_mut().theme = theme;
        Ok(())
    }

    /// Configure table-of-contents generation and PDF running page numbers.
    /// EPUB's required navigation document is generated independently of this
    /// optional in-content table of contents.
    #[wasm_bindgen(js_name = setNavigation)]
    pub fn set_navigation(&mut self, table_of_contents: bool, page_numbers: bool) {
        let options = self.renderer.options_mut();
        options.toc = table_of_contents;
        options.page_numbers = page_numbers;
    }

    /// Set a finite, positive uniform font scale (1.0 means no scaling).
    ///
    /// # Errors
    /// Non-finite, zero, negative, or unrepresentable scales are rejected
    /// without changing the current value.
    #[wasm_bindgen(js_name = setFontScale)]
    pub fn set_font_scale(&mut self, scale: f64) -> Result<(), JsValue> {
        let scale = font_scale(scale).map_err(JsValue::from_str)?;
        self.renderer.options_mut().font_scale = Some(scale);
        Ok(())
    }

    /// Add or replace one image keyed by its book-relative destination.
    /// For `guide/start.md` containing `figure.svg`, supply
    /// `guide/figure.svg`. Different chapters may use the same basename.
    ///
    /// # Errors
    /// Rejects invalid keys and image budgets. No partial replacement occurs
    /// when validation fails. The core does not fetch missing images.
    #[wasm_bindgen(js_name = setImage)]
    pub fn set_image(&mut self, destination: &str, bytes: Vec<u8>) -> Result<(), JsValue> {
        self.renderer.set_image(destination, bytes).map_err(to_js)
    }

    /// Supply a validated font for one shared PDF/HTML renderer slot. EPUB
    /// font styling remains controlled by its stylesheet.
    ///
    /// # Errors
    /// Unknown slots or unsupported font bytes leave the existing slot intact.
    #[wasm_bindgen(js_name = setFont)]
    pub fn set_font(&mut self, slot: &str, bytes: Vec<u8>) -> Result<(), JsValue> {
        let slot = font_slot(slot).map_err(JsValue::from_str)?;
        self.renderer.options_mut().font_assets.set_slot(slot, bytes).map_err(to_js)
    }

    /// Pin a variable font's CSS weight in the range 1 through 1000.
    /// Static fonts ignore the weight pin.
    ///
    /// # Errors
    /// Unknown slots or out-of-range weights do not mutate the font settings.
    #[wasm_bindgen(js_name = setFontWeight)]
    pub fn set_font_weight(&mut self, slot: &str, weight: u32) -> Result<(), JsValue> {
        let slot = font_slot(slot).map_err(JsValue::from_str)?;
        let weight = u16::try_from(weight)
            .map_err(|_| JsValue::from_str("font weight must be in 1..=1000"))?;
        self.renderer.options_mut().font_assets.set_slot_weight(slot, weight).map_err(to_js)
    }

    /// Export a continuous PDF with isolated citations and chapter assets.
    ///
    /// # Errors
    /// Returns asset validation or PDF rendering failures as JavaScript errors.
    #[wasm_bindgen(js_name = renderPdf)]
    pub fn render_pdf(&self) -> Result<Vec<u8>, JsValue> {
        self.renderer.render_pdf().map_err(to_js)
    }

    /// Export a multi-chapter EPUB with ordered spine and packaged images.
    ///
    /// # Errors
    /// Returns asset validation or EPUB rendering failures as JavaScript errors.
    #[wasm_bindgen(js_name = renderEpub)]
    pub fn render_epub(&self) -> Result<Vec<u8>, JsValue> {
        self.renderer.render_epub().map_err(to_js)
    }

    /// Export an HTML-site ZIP with navigation and chapter-addressed search.
    ///
    /// # Errors
    /// Returns path, resource-budget, or HTML rendering failures.
    #[wasm_bindgen(js_name = renderSite)]
    pub fn render_site(&self) -> Result<Vec<u8>, JsValue> {
        self.renderer.render_site().map_err(to_js)
    }

    /// Check local HTML navigation on the retained, expanded book. Returns
    /// fmd-book-link-report-v1 JSON without rendering pages or inspecting assets.
    /// External URLs and non-chapter downloads are counted, not verified.
    ///
    /// # Errors
    /// Rejects invalid books or validation/report budget overruns.
    #[wasm_bindgen(js_name = validateLinks)]
    pub fn validate_links(&self) -> Result<String, JsValue> {
        self.renderer.validate_links().and_then(|report| report.to_json()).map_err(to_js)
    }
}

fn to_js(error: RenderError) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn book_inputs(paths: Vec<String>, sources: Vec<String>) -> Result<Vec<BookInput>, &'static str> {
    if paths.len() != sources.len() {
        return Err("book paths and source arrays must have equal lengths");
    }
    if paths.is_empty() || paths.len() > 4096 {
        return Err("expected between 1 and 4096 book chapters");
    }
    Ok(paths.into_iter().zip(sources).map(|(path, source)| BookInput { path, source }).collect())
}

fn nonblank(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

fn theme_settings(font: &str, dark_mode: &str) -> Result<(FontFamily, DarkModePolicy), &'static str> {
    let font = FontFamily::parse(font).ok_or("unknown font family; use sans or serif")?;
    let dark = match dark_mode.trim().to_ascii_lowercase().as_str() {
        "auto" | "system" => DarkModePolicy::Auto,
        "light" | "disabled" => DarkModePolicy::Disabled,
        _ => return Err("unknown dark mode; use auto, light, or disabled"),
    };
    Ok((font, dark))
}

fn font_slot(slot: &str) -> Result<FontAssetSlot, &'static str> {
    FontAssetSlot::parse(slot).ok_or(
        "unknown font slot; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular",
    )
}

fn font_scale(scale: f64) -> Result<f32, &'static str> {
    let narrowed = scale as f32;
    if !scale.is_finite() || !narrowed.is_finite() || narrowed <= 0.0 {
        return Err("font scale must be finite, positive, and representable as f32");
    }
    Ok(narrowed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn input_arrays_preserve_order_and_reject_mismatches() {
        let inputs = book_inputs(
            vec!["z.md".into(), "a.md".into()],
            vec!["# Z".into(), "# A".into()],
        ).unwrap();
        assert_eq!(inputs[0].path, "z.md");
        assert_eq!(inputs[1].source, "# A");
        assert!(book_inputs(vec!["one.md".into()], vec![]).is_err());
        assert!(book_inputs(vec![], vec![]).is_err());
    }

    #[test]
    fn scales_reject_nan_infinity_and_narrowing_overflow_or_underflow() {
        assert_eq!(font_scale(1.125), Ok(1.125));
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, f64::MAX, f64::MIN_POSITIVE] {
            assert!(font_scale(value).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn metadata_preserves_nonblank_values_verbatim() {
        assert_eq!(nonblank(None), None);
        assert_eq!(nonblank(Some(" \t".into())), None);
        assert_eq!(nonblank(Some("  A title  ".into())), Some("  A title  ".into()));
        assert!(font_slot("body-regular").is_ok());
        assert!(font_slot("missing").is_err());
    }

    #[test]
    fn theme_names_map_only_to_supported_policies() {
        assert_eq!(theme_settings("sans", "auto").unwrap().1, DarkModePolicy::Auto);
        assert_eq!(theme_settings("serif", "light").unwrap().1, DarkModePolicy::Disabled);
        assert!(theme_settings("missing", "auto").is_err());
        assert!(theme_settings("sans", "missing").is_err());
    }
}
