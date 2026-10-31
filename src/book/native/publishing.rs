//! Native book presentation admission and explicit publication images.
//! The shared book renderer owns typography, pagination, navigation and PDF/A.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

use clap::Args;

use super::{BookTarget, Failure, error};
use crate::config::RenderDefaults;
use crate::layout::MicrotypeOptions;
use crate::{
    HtmlFontFormat, PdfAMode, PdfASettings, PdfImageAsset, PdfOptions, PdfRunningBand,
    PdfRunningContent, PdfRunningImage, PdfRunningImagePosition,
};

/// One argument group lets source-only checking reject every publishing flag.
#[derive(Args)]
#[group(id = "book_publishing", multiple = true)]
pub(super) struct PublishingArgs {
    /// Generate contents; PDF books default on, HTML/EPUB default off.
    /// Use --toc=false to override a configured or frontmatter default.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    toc: Option<bool>,
    /// Embedded HTML font format. EPUB uses raw TrueType or CFF OpenType fonts.
    #[arg(long, value_parser = ["woff1", "woff2", "ttf"])]
    html_font_format: Option<String>,
    /// HTML/EPUB font programs: subset (compact default) or full (retain shaping).
    #[arg(long, value_parser = ["subset", "full"])]
    font_embedding: Option<String>,
    /// PDF optical-margin alignment or glyph expansion.
    #[arg(long, value_parser = ["off", "protrusion", "expansion"])]
    microtype: Option<String>,
    /// Enable gradual adjacent-line spacing demerits; accepts =false.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    typography_homogeneous: Option<bool>,
    /// Penalize aligned interword spaces on adjacent lines; accepts =false.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    typography_antiriver: Option<bool>,
    /// Enable bounded Pareto line breaking; accepts =false.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    typography_pareto: Option<bool>,
    /// Enable document-wide optimal pagination; accepts =false.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_optimal_pagination: Option<bool>,
    /// Require exact PDF page-boundary planning; fail instead of falling back.
    /// Implies optimal pagination; accepts =false to override config.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_strict_pagination: Option<bool>,
    /// Fit the assembled PDF, including contents and chapter breaks, to N pages.
    #[arg(long, visible_alias = "target-pages", value_name = "PAGES")]
    pub(super) fit_to_pages: Option<usize>,
    /// PDF base body size in points (6..=24); overrides the PDF font scale.
    #[arg(long, value_name = "POINTS")]
    pdf_base_font_size: Option<f64>,
    /// PDF heading size ratio (1.05..=2).
    #[arg(long, value_name = "RATIO")]
    pdf_heading_scale: Option<f64>,
    /// Nominal PDF table size in points (5..=24, capped by the body size).
    #[arg(long, value_name = "POINTS")]
    pdf_table_font_size: Option<f64>,
    /// Running page numbers, on by default for books; accepts =false.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_page_numbers: Option<bool>,
    /// PDF header left text. Tokens: {page} {pages} {title} {author} {date}.
    #[arg(long, value_name = "TEXT")]
    pdf_header_left: Option<String>,
    /// PDF header center text.
    #[arg(long, value_name = "TEXT")]
    pdf_header_center: Option<String>,
    /// PDF header right text.
    #[arg(long, value_name = "TEXT")]
    pdf_header_right: Option<String>,
    /// PDF footer left text.
    #[arg(long, value_name = "TEXT")]
    pdf_footer_left: Option<String>,
    /// PDF footer center text; overrides automatic page numbers.
    #[arg(long, value_name = "TEXT")]
    pdf_footer_center: Option<String>,
    /// PDF footer right text.
    #[arg(long, value_name = "TEXT")]
    pdf_footer_right: Option<String>,
    /// Header image key supplied with --pdf-image DEST=PATH.
    #[arg(long, value_name = "DEST")]
    pdf_header_image: Option<String>,
    /// Header image side, default left.
    #[arg(long, value_parser = ["left", "right"], requires = "pdf_header_image")]
    pdf_header_image_position: Option<String>,
    /// Header image height in points, reduced to fit its margin.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..), requires = "pdf_header_image")]
    pdf_header_image_height_pt: Option<u16>,
    /// Footer image key supplied with --pdf-image DEST=PATH.
    #[arg(long, value_name = "DEST")]
    pdf_footer_image: Option<String>,
    /// Footer image side, default left.
    #[arg(long, value_parser = ["left", "right"], requires = "pdf_footer_image")]
    pdf_footer_image_position: Option<String>,
    /// Footer image height in points, reduced to fit its margin.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..), requires = "pdf_footer_image")]
    pdf_footer_image_height_pt: Option<u16>,
    /// Draw a rule beneath the running header.
    #[arg(long)]
    pdf_header_rule: bool,
    /// Draw a rule above the running footer.
    #[arg(long)]
    pdf_footer_rule: bool,
    /// Leave the first PDF page without running bands or page number.
    #[arg(long)]
    pdf_running_skip_first: bool,
    /// Explicit image bytes for running logos or book-root image destinations.
    /// Repeat; later mappings replace earlier ones. Paths resolve from cwd.
    #[arg(long = "pdf-image", value_name = "DEST=PATH")]
    pdf_images: Vec<String>,
    /// PDF/A identification and output intent: 2b or off.
    #[arg(long = "pdf-a", value_name = "PROFILE")]
    pdf_a: Option<String>,
    /// Reject unsupported PDF/A URI actions. Requires --pdf-a 2b.
    #[arg(long = "pdf-a-strict")]
    pdf_a_strict: bool,
}

pub(super) struct Publishing {
    pub html_font_format: HtmlFontFormat,
    pub font_embedding: crate::FontEmbedding,
    pub pdf_a: PdfASettings,
    explicit_toc: Option<bool>,
    configured_toc: Option<bool>,
    pdf: PdfOptions,
    images: Vec<(String, PathBuf)>,
}

pub(super) fn prepare(
    args: &PublishingArgs,
    target: BookTarget,
    defaults: &RenderDefaults,
) -> Result<Publishing, Failure> {
    let running_requested = [
        &args.pdf_header_left,
        &args.pdf_header_center,
        &args.pdf_header_right,
        &args.pdf_footer_left,
        &args.pdf_footer_center,
        &args.pdf_footer_right,
        &args.pdf_header_image,
        &args.pdf_footer_image,
    ]
    .iter()
    .any(|value| value.is_some())
        || args.pdf_header_rule
        || args.pdf_footer_rule
        || args.pdf_running_skip_first;
    if !matches!(target, BookTarget::Pdf | BookTarget::Both)
        && (running_requested
            || args.microtype.is_some()
            || args.typography_homogeneous.is_some()
            || args.typography_antiriver.is_some()
            || args.typography_pareto.is_some()
            || args.pdf_optimal_pagination.is_some()
            || args.pdf_strict_pagination.is_some()
            || args.fit_to_pages.is_some()
            || args.pdf_base_font_size.is_some()
            || args.pdf_heading_scale.is_some()
            || args.pdf_table_font_size.is_some()
            || args.pdf_page_numbers.is_some()
            || args.pdf_a.is_some()
            || args.pdf_a_strict)
    {
        return Err(error(
            64,
            "unsupported_target_option",
            "PDF typography, pagination, running content and archival flags require --to pdf or --to both",
        ));
    }
    if args.html_font_format.is_some() && !matches!(target, BookTarget::Html | BookTarget::Both) {
        return Err(error(
            64,
            "unsupported_target_option",
            "--html-font-format requires --to html or --to both; EPUB uses raw TrueType or CFF OpenType fonts",
        ));
    }
    if args.font_embedding.is_some()
        && !matches!(
            target,
            BookTarget::Html | BookTarget::Both | BookTarget::Epub
        )
    {
        return Err(error(
            64,
            "unsupported_target_option",
            "--font-embedding requires --to html, --to both or --to epub",
        ));
    }
    if args.fit_to_pages == Some(0) {
        return Err(error(
            64,
            "usage_error",
            "--fit-to-pages must be at least 1",
        ));
    }
    let pdf_a = PdfASettings {
        mode: args
            .pdf_a
            .as_deref()
            .map(|value| {
                PdfAMode::parse(value).ok_or_else(|| {
                    error(64, "usage_error", "unknown --pdf-a profile; use 2b or off")
                })
            })
            .transpose()?
            .unwrap_or_default(),
        strict: args.pdf_a_strict,
    };
    if pdf_a.strict && !pdf_a.mode.is_a2b() {
        return Err(error(
            64,
            "usage_error",
            "--pdf-a-strict requires --pdf-a 2b",
        ));
    }
    let microtype = match args.microtype.as_deref().or(defaults.microtype.as_deref()) {
        Some("protrusion") => MicrotypeOptions::CONSERVATIVE,
        Some("expansion") => MicrotypeOptions {
            protrusion: false,
            max_expansion_per_mille: 15,
        },
        _ => MicrotypeOptions::DISABLED,
    };
    let pdf = PdfOptions {
        microtype,
        gradual_demerits: args
            .typography_homogeneous
            .or(defaults.typography_homogeneous)
            .unwrap_or(false),
        river_penalty: args
            .typography_antiriver
            .or(defaults.typography_antiriver)
            .unwrap_or(false),
        pareto_line_breaking: args
            .typography_pareto
            .or(defaults.typography_pareto)
            .unwrap_or(false),
        optimal_pagination: args
            .pdf_optimal_pagination
            .or(defaults.pdf_optimal_pagination)
            .unwrap_or(false),
        strict_pagination: args
            .pdf_strict_pagination
            .or(defaults.pdf_strict_pagination)
            .unwrap_or(false),
        page_numbers: args
            .pdf_page_numbers
            .or(defaults.pdf_page_numbers)
            .unwrap_or(true),
        fit_to_pages: args.fit_to_pages,
        base_font_size: bounded_size(args.pdf_base_font_size, "base font size", 6.0, 24.0)?,
        heading_scale: bounded_size(args.pdf_heading_scale, "heading scale", 1.05, 2.0)?,
        table_font_size: bounded_size(args.pdf_table_font_size, "table font size", 5.0, 24.0)?,
        running: PdfRunningContent {
            header: PdfRunningBand {
                left: args.pdf_header_left.clone(),
                center: args.pdf_header_center.clone(),
                right: args.pdf_header_right.clone(),
                rule: args.pdf_header_rule,
                image: running_image(
                    args.pdf_header_image.as_deref(),
                    args.pdf_header_image_position.as_deref(),
                    args.pdf_header_image_height_pt,
                )?,
            },
            footer: PdfRunningBand {
                left: args.pdf_footer_left.clone(),
                center: args.pdf_footer_center.clone(),
                right: args.pdf_footer_right.clone(),
                rule: args.pdf_footer_rule,
                image: running_image(
                    args.pdf_footer_image.as_deref(),
                    args.pdf_footer_image_position.as_deref(),
                    args.pdf_footer_image_height_pt,
                )?,
            },
            skip_first_page: args.pdf_running_skip_first,
        },
        ..PdfOptions::default()
    };
    if args.pdf_images.len() > 4096 {
        return Err(error(
            64,
            "usage_error",
            "at most 4096 --pdf-image mappings are allowed",
        ));
    }
    let images = args
        .pdf_images
        .iter()
        .map(|spec| {
            let (destination, path) = spec
                .split_once('=')
                .ok_or_else(|| error(64, "usage_error", "--pdf-image requires DEST=PATH"))?;
            if destination.trim().is_empty() || path.trim().is_empty() {
                return Err(error(
                    64,
                    "usage_error",
                    "--pdf-image needs a nonempty DEST and PATH",
                ));
            }
            Ok((destination.trim().to_owned(), PathBuf::from(path.trim())))
        })
        .collect::<Result<_, Failure>>()?;
    Ok(Publishing {
        html_font_format: args
            .html_font_format
            .as_deref()
            .or(defaults.html_font_format.as_deref())
            .and_then(HtmlFontFormat::parse)
            .unwrap_or_default(),
        font_embedding: args
            .font_embedding
            .as_deref()
            .or(defaults.font_embedding.as_deref())
            .and_then(crate::FontEmbedding::parse)
            .unwrap_or_default(),
        pdf_a,
        explicit_toc: args.toc,
        configured_toc: defaults.toc,
        pdf,
        images,
    })
}

fn bounded_size(
    value: Option<f64>,
    name: &str,
    min: f64,
    max: f64,
) -> Result<Option<f32>, Failure> {
    value
        .map(|value| {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(error(
                    64,
                    "usage_error",
                    format!("{name} must be finite and in {min}..={max}"),
                ));
            }
            Ok(value as f32)
        })
        .transpose()
}

fn running_image(
    destination: Option<&str>,
    position: Option<&str>,
    height_pt: Option<u16>,
) -> Result<Option<PdfRunningImage>, Failure> {
    destination
        .map(|destination| {
            if destination.trim().is_empty() {
                return Err(error(
                    64,
                    "usage_error",
                    "running image destination must not be blank",
                ));
            }
            Ok(PdfRunningImage {
                dest: destination.trim().to_owned(),
                position: if position == Some("right") {
                    PdfRunningImagePosition::Right
                } else {
                    PdfRunningImagePosition::Left
                },
                height_pt,
            })
        })
        .transpose()
}

impl Publishing {
    pub(super) fn toc(&self, frontmatter: Option<bool>, fallback: bool) -> bool {
        self.explicit_toc
            .or(frontmatter)
            .or(self.configured_toc)
            .unwrap_or(fallback)
    }

    pub(super) fn apply_pdf(&self, options: &mut PdfOptions) {
        options.microtype = self.pdf.microtype;
        options.gradual_demerits = self.pdf.gradual_demerits;
        options.river_penalty = self.pdf.river_penalty;
        options.pareto_line_breaking = self.pdf.pareto_line_breaking;
        options.optimal_pagination = self.pdf.optimal_pagination;
        options.strict_pagination = self.pdf.strict_pagination;
        options.page_numbers = self.pdf.page_numbers;
        options.fit_to_pages = self.pdf.fit_to_pages;
        options.base_font_size = self.pdf.base_font_size.or(options.base_font_size);
        options.heading_scale = self.pdf.heading_scale;
        options.table_font_size = self.pdf.table_font_size;
        options.running = self.pdf.running.clone();
    }

    pub(super) fn check_page_budget(&self, pages: u64) -> Result<(), Failure> {
        if let Some(target) = self.pdf.fit_to_pages
            && pages > target as u64
        {
            return Err(error(
                70,
                "page_budget_unmet",
                format!(
                    "the assembled book still needs {pages} pages after fitting, exceeding --fit-to-pages {target}; increase the budget or shorten the source"
                ),
            ));
        }
        Ok(())
    }

    /// Explicit assets override auto-loaded destinations. Protect every input,
    /// even a file superseded by a later mapping, before any publication writes.
    pub(super) fn load_images(
        &self,
        images: &mut Vec<PdfImageAsset>,
        protected: &mut BTreeSet<PathBuf>,
        max_image_bytes: u64,
    ) -> Result<(), Failure> {
        const MAX_TOTAL: usize = 128 * 1024 * 1024;
        let limit = max_image_bytes.min(32 * 1024 * 1024);
        let mut total: usize = images.iter().map(|asset| asset.bytes.len()).sum();
        let mut read_total = 0usize;
        for (destination, path) in &self.images {
            let label = format!("image {destination:?} from {}", path.display());
            let metadata = std::fs::metadata(path)
                .map_err(|e| error(66, "image_error", format!("reading {label}: {e}")))?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
                return Err(error(
                    66,
                    "image_error",
                    format!("{label} must be a regular file of 1..={limit} bytes"),
                ));
            }
            let canonical = path
                .canonicalize()
                .map_err(|e| error(66, "image_error", format!("resolving {label}: {e}")))?;
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
                .map_err(|e| error(66, "image_error", format!("reading {label}: {e}")))?;
            if bytes.is_empty() || bytes.len() as u64 > limit {
                return Err(error(
                    66,
                    "image_error",
                    format!("{label} must contain 1..={limit} bytes"),
                ));
            }
            read_total = read_total.saturating_add(bytes.len());
            let existing = images
                .iter()
                .position(|asset| asset.destination == *destination);
            let old_size = existing.map_or(0, |index| images[index].bytes.len());
            total = total.saturating_sub(old_size).saturating_add(bytes.len());
            if total > MAX_TOTAL
                || read_total > MAX_TOTAL
                || (existing.is_none() && images.len() >= 4096)
            {
                return Err(error(
                    66,
                    "image_error",
                    "book images exceed the 128 MiB or 4096-resource limit",
                ));
            }
            protected.insert(canonical);
            let asset = PdfImageAsset::new(destination, bytes);
            if let Some(index) = existing {
                images[index] = asset;
            } else {
                images.push(asset);
            }
        }
        Ok(())
    }
}
