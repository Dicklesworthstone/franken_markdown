//! Explicit, reproducible PDF inputs for native verification. Presentation
//! config and network image loading never supply the verification layout;
//! the caller resolves the authoring grammar before admitting these options.

use std::path::Path;
use std::process::ExitCode;

use clap::Args;

use super::host_fonts::{HostFontError, HostFonts};
use super::protected_paths::ProtectedInputs;
use super::{FontArg, MicrotypeArg, PdfImageError, fail_json};
use crate::{Document, PdfOptions, Profile};

#[derive(Args)]
pub(super) struct VerifyLayoutArgs {
    #[command(flatten)]
    running: super::PdfRunningArgs,
    /// Explicit document title for the {title} running-content token.
    #[arg(long)]
    title: Option<String>,
    /// Explicit author for the {author} running-content token.
    #[arg(long)]
    author: Option<String>,
    /// UTC Unix timestamp for {date}. Verification ignores SOURCE_DATE_EPOCH.
    #[arg(long, value_name = "SECONDS")]
    metadata_epoch_seconds: Option<u64>,
    /// Body font family for the PDF layout (default: sans).
    #[arg(long, value_enum)]
    font: Option<FontArg>,
    /// Type size preset, percentage or multiplier, as in rendering.
    #[arg(long, value_name = "SCALE|PRESET", visible_alias = "type-size")]
    font_scale: Option<String>,
    /// Explicit hyphenation language; configured language is not inherited.
    #[arg(long)]
    lang: Option<String>,
    /// PDF paper: letter, a4, a5, legal, tabloid, or WIDTHxHEIGHT in points.
    #[arg(long, value_name = "SIZE")]
    page_size: Option<String>,
    /// PDF top margin in points (default: 72).
    #[arg(long, value_name = "POINTS")]
    margin_top_pt: Option<f64>,
    /// PDF right margin in points (default: 72).
    #[arg(long, value_name = "POINTS")]
    margin_right_pt: Option<f64>,
    /// PDF bottom margin in points (default: 72).
    #[arg(long, value_name = "POINTS")]
    margin_bottom_pt: Option<f64>,
    /// PDF left margin in points (default: 72).
    #[arg(long, value_name = "POINTS")]
    margin_left_pt: Option<f64>,
    /// Body baseline grid in points (1..=144; 0 disables it).
    #[arg(long, value_name = "POINTS")]
    pdf_baseline_grid_pt: Option<f64>,
    /// Fit and verify a positive page budget; fail if it cannot be met.
    #[arg(long, value_name = "PAGES", visible_alias = "target-pages")]
    fit_to_pages: Option<usize>,
    /// Include the generated table of contents in the checked layout.
    #[arg(long)]
    toc: bool,
    /// Maximum heading depth in the generated contents (default: 3).
    #[arg(long)]
    toc_depth: Option<u8>,
    /// PDF body size in points, with the renderer's [6, 24] clamp.
    #[arg(long)]
    pdf_base_font_size: Option<f32>,
    /// Heading geometric scale, with the renderer's [1.05, 2.0] clamp.
    #[arg(long)]
    pdf_heading_scale: Option<f32>,
    /// Nominal table font size in points, as in PDF rendering.
    #[arg(long)]
    pdf_table_font_size: Option<f32>,
    /// Include code line numbers in PDF measurement.
    #[arg(long)]
    pdf_line_numbers: bool,
    /// Optical margin protrusion or glyph expansion (default: off).
    #[arg(long, value_enum)]
    microtype: Option<MicrotypeArg>,
    /// Use gradual adjacent demerits for paragraph breaking.
    #[arg(long)]
    typography_homogeneous: bool,
    /// Penalize aligned whitespace channels between adjacent lines.
    #[arg(long)]
    typography_antiriver: bool,
    /// Use bounded multi-objective paragraph breaking.
    #[arg(long)]
    typography_pareto: bool,
    /// Use document-wide optimal page planning.
    #[arg(long)]
    pdf_optimal_pagination: bool,
    /// Require exact page planning; return the renderer's typed refusal.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_strict_pagination: Option<bool>,
    /// Link numbered notes back to their first citation.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_footnote_backlinks: Option<bool>,
    /// Continue long notes across page feet; requires greedy pagination.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_split_footnotes: Option<bool>,
    #[command(flatten)]
    spacing: super::pdf_spacing::PdfSpacingArgs,
    /// Load relative local images under the input file's directory. Opt-in;
    /// remote URLs are never fetched. Stdin requires explicit --pdf-image.
    #[arg(long)]
    load_local_images: bool,
    /// Supply exact image bytes as DEST=PATH (repeatable, also with stdin).
    /// Overrides automatic local images; never downloads a URL.
    #[arg(long = "pdf-image", value_name = "DEST=PATH")]
    pdf_images: Vec<String>,
    /// Maximum bytes per explicit or automatically loaded local image.
    #[arg(long, default_value_t = super::DEFAULT_MAX_PDF_IMAGE_BYTES)]
    max_pdf_image_bytes: u64,
    /// Supply a TrueType/CFF1 face as SLOT=PATH, as in rendering (repeatable).
    #[arg(long = "pdf-font", value_name = "SLOT=PATH")]
    pdf_fonts: Vec<String>,
    /// Select a zero-based TTC/OTC face as INDEX or SLOT=INDEX.
    #[arg(long = "pdf-font-face", value_name = "INDEX|SLOT=INDEX")]
    pdf_font_faces: Vec<String>,
    /// Pin a host font's weight as WEIGHT or SLOT=WEIGHT (1..=1000).
    #[arg(long = "pdf-font-weight", value_name = "WEIGHT|SLOT=WEIGHT")]
    pdf_font_weights: Vec<String>,
}

pub(super) struct AdmittedLayout {
    pub options: PdfOptions,
    fonts: HostFonts,
}

impl VerifyLayoutArgs {
    /// Validate options without opening document/resource files. The same
    /// precision-safe page, font and scale parsers serve native publication.
    pub fn admit(
        &self,
        input: &Path,
        profile: Option<Profile>,
        json: bool,
    ) -> Result<AdmittedLayout, ExitCode> {
        if self.load_local_images && input == Path::new("-") {
            return Err(fail_json(
                64,
                "usage_error",
                "--load-local-images needs a file input; use --pdf-image DEST=PATH with stdin",
                json,
            ));
        }
        if self.fit_to_pages == Some(0) {
            return Err(fail_json(
                64,
                "invalid_page_budget",
                "--fit-to-pages must be at least 1",
                json,
            ));
        }
        let fonts = HostFonts::parse(
            &self.pdf_fonts,
            &self.pdf_font_weights,
            &self.pdf_font_faces,
        )
        .map_err(|message| fail_json(64, "usage_error", &message, json))?;
        let scale = super::parse_font_scale_value(self.font_scale.as_deref(), json)?;
        let mut theme = crate::Theme::default();
        if let Some(font) = self.font {
            theme = theme.with_font(font.into());
        }
        if let Some(scale) = scale {
            theme = theme.with_font_scale(scale);
        }
        crate::config::page::configure(
            &mut theme,
            self.page_size.as_deref(),
            [
                self.margin_top_pt,
                self.margin_right_pt,
                self.margin_bottom_pt,
                self.margin_left_pt,
            ],
        )
        .map_err(|message| fail_json(64, "invalid_page", &message, json))?;
        crate::config::page::configure_baseline_grid(
            &mut theme,
            self.pdf_baseline_grid_pt,
            None,
            self.fit_to_pages,
        )
        .map_err(|error| fail_json(64, error.code(), &error.to_string(), json))?;
        let use_theme_spacing = self
            .spacing
            .enabled(None)
            .map_err(|message| fail_json(64, "invalid_theme_spacing", message, json))?;
        self.spacing.apply(&mut theme.spacing);
        let options = PdfOptions {
            theme,
            profile,
            title: self.title.clone(),
            author: self.author.clone(),
            metadata_epoch_seconds: self.metadata_epoch_seconds,
            page_numbers: self.running.pdf_page_numbers,
            running: super::pdf_running_content(&self.running),
            lang: self.lang.clone(),
            base_font_size: self.pdf_base_font_size.or(scale.map(|s| s.pdf_base_pt())),
            heading_scale: self.pdf_heading_scale,
            table_font_size: self.pdf_table_font_size,
            toc: self.toc,
            toc_depth: self.toc_depth,
            code_line_numbers: self.pdf_line_numbers,
            fit_to_pages: self.fit_to_pages,
            microtype: self.microtype.unwrap_or_default().into(),
            gradual_demerits: self.typography_homogeneous,
            river_penalty: self.typography_antiriver,
            pareto_line_breaking: self.typography_pareto,
            optimal_pagination: self.pdf_optimal_pagination,
            strict_pagination: self.pdf_strict_pagination.unwrap_or(false),
            footnote_backlinks: self.pdf_footnote_backlinks.unwrap_or(false),
            split_footnotes: self.pdf_split_footnotes.unwrap_or(false),
            use_theme_spacing,
            ..PdfOptions::default()
        };
        Ok(AdmittedLayout { options, fonts })
    }

    /// Capture explicit resources and optional source-relative images once.
    /// Every admitted path, including a missing or overridden local image,
    /// remains protected against the only possible write: the link cache.
    pub fn load(
        &self,
        admitted: &mut AdmittedLayout,
        doc: &Document,
        input: &Path,
        protected: &mut ProtectedInputs,
        cache: Option<&Path>,
        json: bool,
    ) -> Result<(), ExitCode> {
        let lowered = crate::safe_html::lower(doc);
        let mut destinations = Vec::new();
        super::collect_image_destinations(&lowered.blocks, &mut destinations);
        // Running logo keys may contain '=' and need not occur in Markdown.
        // Only body destinations authorize automatic local reads.
        let mut explicit_destinations = destinations.clone();
        explicit_destinations.extend(super::running_image_destinations(&self.running));
        let base = self
            .load_local_images
            .then(|| super::auto_pdf_image_base_dir(input.to_str(), None))
            .flatten();
        let mut paths = admitted
            .fonts
            .paths()
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        for spec in &self.pdf_images {
            let (_, path) = super::parse_pdf_image_spec(spec, &explicit_destinations)
                .map_err(|message| fail_json(64, "usage_error", &message, json))?;
            paths.push(path);
        }
        if let Some(base) = base.as_deref() {
            paths.extend(
                destinations
                    .iter()
                    .filter_map(|dest| super::auto_pdf_image_path(dest.trim(), base)),
            );
        }
        for path in &paths {
            protected.insert(path);
        }
        check_cache(protected, cache, json)?;
        admitted.options.font_assets = admitted.fonts.load(&mut paths).map_err(|error| {
            let (code, kind, message) = match error {
                HostFontError::Usage(message) => (64, "usage_error", message),
                HostFontError::Input(message) => (66, "input_error", message),
            };
            fail_json(code, kind, &message, json)
        })?;
        admitted.options.image_assets = super::read_pdf_image_assets_recording(
            &self.pdf_images,
            self.max_pdf_image_bytes,
            &explicit_destinations,
            &mut paths,
        )
        .map_err(|error| {
            let (code, kind, message) = match error {
                PdfImageError::Usage(message) => (64, "usage_error", message),
                PdfImageError::Input(message) => (66, "input_error", message),
            };
            fail_json(code, kind, &message, json)
        })?;
        if let Some(base) = base.as_deref() {
            super::append_auto_image_assets_recording(
                doc,
                base,
                &mut admitted.options.image_assets,
                self.max_pdf_image_bytes,
                "verify",
                &mut paths,
            )
            .map_err(|message| fail_json(66, "input_error", &message, json))?;
        }
        for path in &paths {
            protected.insert(path);
        }
        check_cache(protected, cache, json)
    }
}

pub(super) fn check_cache(
    protected: &ProtectedInputs,
    cache: Option<&Path>,
    json: bool,
) -> Result<(), ExitCode> {
    if let Some(cache) = cache
        && let Some(source) = protected.source_for(cache)
    {
        return Err(fail_json(
            64,
            "usage_error",
            &format!(
                "refusing to overwrite verify input {} with link cache {}; choose a different --links-cache path",
                source.display(),
                cache.display(),
            ),
            json,
        ));
    }
    Ok(())
}
