//! Shared theme/style model for HTML, PDF, CLI JSON, and WASM callers.
//!
//! The model is deliberately typed and dependency-free. It is "serializable
//! enough" through stable hand-rolled JSON snippets without pulling in `serde`
//! or a config stack, and it keeps visual decisions in one place so HTML and
//! PDF can converge on the same typography, colour, spacing, and page contract.

/// The default body font family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontFamily {
    /// A clean, highly-readable sans-serif (the default).
    #[default]
    Sans,
    /// A beautiful serif for long-form reading.
    Serif,
}

impl FontFamily {
    /// Parse a CLI/config string (`sans`/`serif`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sans" | "sans-serif" | "sansserif" => Some(Self::Sans),
            "serif" => Some(Self::Serif),
            _ => None,
        }
    }

    /// Stable config/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sans => "sans",
            Self::Serif => "serif",
        }
    }
}

/// Monospace font family used for inline and fenced code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MonoFontFamily {
    /// High-quality documentation-code stack.
    #[default]
    Documentation,
}

impl MonoFontFamily {
    /// Stable config/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Documentation => "documentation",
        }
    }
}

/// Dark-mode CSS policy for all-in-one HTML output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DarkModePolicy {
    /// Emit a `prefers-color-scheme: dark` override.
    #[default]
    Auto,
    /// Emit only the light/default palette.
    Disabled,
}

impl DarkModePolicy {
    /// Stable config/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Disabled => "disabled",
        }
    }
}

/// Code token palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CodeTheme {
    /// GitHub/Cursor-like light tokens with dark-mode counterparts.
    #[default]
    GitHub,
}

impl CodeTheme {
    /// Stable config/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
        }
    }
}

/// Colour tokens shared by HTML and the PDF style layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeColors {
    pub fg: String,
    pub fg_muted: String,
    pub bg: String,
    pub bg_subtle: String,
    pub border: String,
    pub border_muted: String,
    pub code_bg: String,
    pub stripe: String,
    pub quote_fg: String,
    pub quote_bar: String,
    pub accent: String,
}

impl ThemeColors {
    /// Light Cursor/GitHub-like palette.
    #[must_use]
    pub fn light() -> Self {
        Self {
            fg: "#1f2328".to_string(),
            fg_muted: "#59636e".to_string(),
            bg: "#ffffff".to_string(),
            bg_subtle: "#f6f8fa".to_string(),
            border: "#d1d9e0".to_string(),
            border_muted: "#e6e8eb".to_string(),
            code_bg: "#f6f8fa".to_string(),
            stripe: "#f6f8fa".to_string(),
            quote_fg: "#59636e".to_string(),
            quote_bar: "#d1d9e0".to_string(),
            accent: "#0969da".to_string(),
        }
    }

    /// Dark palette paired with [`Self::light`].
    #[must_use]
    pub fn dark() -> Self {
        Self {
            fg: "#e6edf3".to_string(),
            fg_muted: "#9198a1".to_string(),
            bg: "#0d1117".to_string(),
            bg_subtle: "#161b22".to_string(),
            border: "#2f3742".to_string(),
            border_muted: "#21262d".to_string(),
            code_bg: "#161b22".to_string(),
            stripe: "#12171e".to_string(),
            quote_fg: "#9198a1".to_string(),
            quote_bar: "#2f3742".to_string(),
            accent: "#4493f8".to_string(),
        }
    }
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::light()
    }
}

/// Spacing and density tokens shared across renderers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeSpacing {
    /// Base font size in CSS px; PDF treats this as the visual baseline token.
    pub base_px: u16,
    /// Readable content measure in CSS px.
    pub max_width_px: u16,
    /// Body line-height/leading multiplier.
    pub line_height: f32,
    /// Default corner radius in px for tables/code/images.
    pub radius_px: u16,
    /// Table cell vertical padding in em.
    pub table_cell_padding_y_em: f32,
    /// Table cell horizontal padding in em.
    pub table_cell_padding_x_em: f32,
}

impl Default for ThemeSpacing {
    fn default() -> Self {
        Self {
            base_px: 16,
            max_width_px: 760,
            line_height: 1.7,
            radius_px: 8,
            table_cell_padding_y_em: 0.55,
            table_cell_padding_x_em: 0.85,
        }
    }
}

impl ThemeSpacing {
    /// Return a copy with base font size and readable measure scaled by a typographic scale.
    #[must_use]
    pub fn with_font_scale(mut self, scale: FontScale) -> Self {
        let factor = scale.scale_factor();
        self.base_px = scale.html_base_px().round() as u16;
        self.max_width_px = ((760.0 * factor).round() as u16).max(400);
        self
    }
}

/// PDF/page size in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    /// Stable name for CLI/config/WASM surfaces.
    pub name: &'static str,
    pub width_pt: f32,
    pub height_pt: f32,
}

impl PageSize {
    /// US Letter.
    pub const LETTER: Self = Self {
        name: "letter",
        width_pt: 612.0,
        height_pt: 792.0,
    };
}

impl Default for PageSize {
    fn default() -> Self {
        Self::LETTER
    }
}

/// Page margins in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageMargins {
    pub top_pt: f32,
    pub right_pt: f32,
    pub bottom_pt: f32,
    pub left_pt: f32,
}

impl Default for PageMargins {
    fn default() -> Self {
        Self {
            top_pt: 72.0,
            right_pt: 72.0,
            bottom_pt: 72.0,
            left_pt: 72.0,
        }
    }
}

/// Page style contract for PDF and future paged WASM/native renderers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PageStyle {
    pub size: PageSize,
    pub margins: PageMargins,
}

/// A render theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Body font family.
    pub font: FontFamily,
    /// Monospace font family.
    pub mono_font: MonoFontFamily,
    /// Light/default colour palette.
    pub colors: ThemeColors,
    /// Dark colour palette used when [`Self::dark_mode`] is [`DarkModePolicy::Auto`].
    pub dark_colors: ThemeColors,
    /// Spacing, measure, leading, radius, and table-density tokens.
    pub spacing: ThemeSpacing,
    /// Page contract used by PDF and future paged renderers.
    pub page: PageStyle,
    /// Code token palette.
    pub code_theme: CodeTheme,
    /// Dark-mode CSS policy.
    pub dark_mode: DarkModePolicy,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: FontFamily::Sans,
            mono_font: MonoFontFamily::Documentation,
            colors: ThemeColors::light(),
            dark_colors: ThemeColors::dark(),
            spacing: ThemeSpacing::default(),
            page: PageStyle::default(),
            code_theme: CodeTheme::GitHub,
            dark_mode: DarkModePolicy::Auto,
        }
    }
}

impl Theme {
    /// The default sans theme.
    #[must_use]
    pub fn sans() -> Self {
        Self::default()
    }

    /// A serif variant of the default theme.
    #[must_use]
    pub fn serif() -> Self {
        Self::default().with_font(FontFamily::Serif)
    }

    /// Return a copy with a different body font family.
    #[must_use]
    pub fn with_font(mut self, font: FontFamily) -> Self {
        self.font = font;
        self
    }

    /// Return a copy with a different dark-mode policy.
    #[must_use]
    pub fn with_dark_mode(mut self, dark_mode: DarkModePolicy) -> Self {
        self.dark_mode = dark_mode;
        self
    }

    /// Return a copy with a uniform typographic font scale applied to spacing tokens.
    #[must_use]
    pub fn with_font_scale(mut self, scale: FontScale) -> Self {
        self.spacing = self.spacing.with_font_scale(scale);
        self
    }

    /// Stable dependency-free JSON representation for CLI/config/WASM surfaces.
    #[must_use]
    pub fn to_config_json(&self) -> String {
        format!(
            "{{\"font\":\"{}\",\"mono_font\":\"{}\",\"code_theme\":\"{}\",\
             \"dark_mode\":\"{}\",\"colors\":{},\"dark_colors\":{},\"spacing\":{},\"page\":{}}}",
            self.font.as_str(),
            self.mono_font.as_str(),
            self.code_theme.as_str(),
            self.dark_mode.as_str(),
            colors_json(&self.colors),
            colors_json(&self.dark_colors),
            spacing_json(&self.spacing),
            page_json(&self.page),
        )
    }

    /// CSS body font stack used as the fallback tail after the embedded
    /// `@font-face` subsets (which the HTML emitter now inlines): a high-quality
    /// system stack that keeps output dependency-free and attractive if a font
    /// fails to load.
    #[must_use]
    pub(crate) fn body_font_stack(&self) -> &'static str {
        match self.font {
            FontFamily::Sans => {
                "Inter, -apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, \
                 \"Helvetica Neue\", Arial, \"Noto Sans\", \"Noto Sans CJK SC\", \
                 \"Noto Sans CJK JP\", \"Noto Sans CJK KR\", \"PingFang SC\", \
                 \"Hiragino Sans\", \"Malgun Gothic\", sans-serif"
            }
            FontFamily::Serif => {
                "\"Source Serif 4\", Newsreader, \"Iowan Old Style\", \"Apple Garamond\", \
                 Georgia, Cambria, \"Times New Roman\", Times, \"Noto Serif CJK SC\", \
                 \"Noto Serif CJK JP\", \"Noto Serif CJK KR\", \"Source Han Serif\", \
                 \"Songti SC\", \"YuMincho\", serif"
            }
        }
    }

    /// CSS monospace stack for code.
    #[must_use]
    pub(crate) fn mono_font_stack(&self) -> &'static str {
        match self.mono_font {
            MonoFontFamily::Documentation => {
                "\"JetBrains Mono\", \"IBM Plex Mono\", \"SFMono-Regular\", \"SF Mono\", \
                 Menlo, Consolas, \"Liberation Mono\", monospace"
            }
        }
    }
}

fn colors_json(colors: &ThemeColors) -> String {
    format!(
        "{{\"fg\":\"{}\",\"fg_muted\":\"{}\",\"bg\":\"{}\",\"bg_subtle\":\"{}\",\
         \"border\":\"{}\",\"border_muted\":\"{}\",\"code_bg\":\"{}\",\"stripe\":\"{}\",\
         \"quote_fg\":\"{}\",\"quote_bar\":\"{}\",\"accent\":\"{}\"}}",
        json_escape(&colors.fg),
        json_escape(&colors.fg_muted),
        json_escape(&colors.bg),
        json_escape(&colors.bg_subtle),
        json_escape(&colors.border),
        json_escape(&colors.border_muted),
        json_escape(&colors.code_bg),
        json_escape(&colors.stripe),
        json_escape(&colors.quote_fg),
        json_escape(&colors.quote_bar),
        json_escape(&colors.accent),
    )
}

fn spacing_json(spacing: &ThemeSpacing) -> String {
    format!(
        "{{\"base_px\":{},\"max_width_px\":{},\"line_height\":{},\"radius_px\":{},\
         \"table_cell_padding_y_em\":{},\"table_cell_padding_x_em\":{}}}",
        spacing.base_px,
        spacing.max_width_px,
        json_num(spacing.line_height),
        spacing.radius_px,
        json_num(spacing.table_cell_padding_y_em),
        json_num(spacing.table_cell_padding_x_em),
    )
}

fn page_json(page: &PageStyle) -> String {
    format!(
        "{{\"size\":{{\"name\":\"{}\",\"width_pt\":{},\"height_pt\":{}}},\
         \"margins\":{{\"top_pt\":{},\"right_pt\":{},\"bottom_pt\":{},\"left_pt\":{}}}}}",
        json_escape(page.size.name),
        json_num(page.size.width_pt),
        json_num(page.size.height_pt),
        json_num(page.margins.top_pt),
        json_num(page.margins.right_pt),
        json_num(page.margins.bottom_pt),
        json_num(page.margins.left_pt),
    )
}

fn json_num(value: f32) -> String {
    // A non-finite value would serialize to the bare tokens `NaN`/`inf`/`-inf`,
    // which are invalid JSON. A library caller can place such a value in a
    // directly-constructed Theme, so fold it to `0` (matching the HTML writer's
    // `css_num`) rather than emit a document that no JSON parser accepts.
    if !value.is_finite() {
        return "0".to_string();
    }
    let mut s = format!("{value:.3}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_string() } else { s }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::json_num;

    #[test]
    fn json_num_folds_non_finite_to_zero_and_keeps_finite() {
        // A non-finite float would otherwise serialize to the invalid JSON tokens
        // `NaN`/`inf`/`-inf`; fold them to `0` so `to_config_json` always parses.
        assert_eq!(json_num(f32::NAN), "0");
        assert_eq!(json_num(f32::INFINITY), "0");
        assert_eq!(json_num(f32::NEG_INFINITY), "0");
        // Finite values are unchanged (trailing zeros trimmed).
        assert_eq!(json_num(72.0), "72");
        assert_eq!(json_num(1.5), "1.5");
        assert_eq!(json_num(0.0), "0");
    }
}

/// Materialized typographic scale for one PDF render, in points.
///
/// Single source of truth for PDF heading/body/code/table sizes. The default
/// reproduces the historical hard-coded ladder byte-for-byte; explicit
/// overrides harmonize the whole hierarchy around them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeScale {
    /// Heading sizes for levels 1..=6 (H1 first).
    pub h: [f32; 6],
    /// Body paragraph size.
    pub body: f32,
    /// Monospace code size.
    pub code: f32,
    /// Nominal table cell size before adaptive scaling.
    pub table: f32,
}

/// Clamp bounds for caller-supplied typography overrides.
pub const BASE_FONT_SIZE_MIN_PT: f32 = 6.0;
pub const BASE_FONT_SIZE_MAX_PT: f32 = 24.0;
pub const HEADING_SCALE_MIN: f32 = 1.05;
pub const HEADING_SCALE_MAX: f32 = 2.0;
pub const TABLE_FONT_SIZE_FLOOR_PT: f32 = 5.0;

impl Default for TypeScale {
    fn default() -> Self {
        Self {
            h: [24.0, 19.0, 16.0, 13.5, 12.0, 11.0],
            body: 11.0,
            code: 9.5,
            table: 10.0,
        }
    }
}

impl TypeScale {
    /// Resolve effective sizes from optional `PdfOptions` overrides.
    ///
    /// * All-`None` reproduces [`TypeScale::default`] exactly so existing
    ///   renders stay byte-identical.
    /// * `base_font_size` alone rescales every entry by its ratio to the
    ///   11 pt default body.
    /// * Adding a `heading_scale` rebuilds the heading ladder geometrically
    ///   from an H1 anchor of `(24/11) x base`; each next level divides by
    ///   the ratio and floors at [`BASE_FONT_SIZE_MIN_PT`], giving the Major
    ///   Third / Perfect Fourth hierarchies the typography bead calls for.
    /// * `table_font_size` overrides the nominal table size directly, clamped
    ///   into `[TABLE_FONT_SIZE_FLOOR_PT, base]`.
    #[must_use]
    pub fn resolve(
        base_font_size: Option<f32>,
        heading_scale: Option<f32>,
        table_font_size: Option<f32>,
    ) -> Self {
        let d = Self::default();
        let base = base_font_size
            .filter(|x| x.is_finite())
            .unwrap_or(d.body)
            .clamp(BASE_FONT_SIZE_MIN_PT, BASE_FONT_SIZE_MAX_PT);
        let proportion = base / d.body;
        let table = table_font_size
            .filter(|x| x.is_finite())
            .unwrap_or(d.table * proportion)
            .clamp(TABLE_FONT_SIZE_FLOOR_PT, base);
        match heading_scale
            .filter(|x| x.is_finite())
            .map(|r| r.clamp(HEADING_SCALE_MIN, HEADING_SCALE_MAX))
        {
            None => Self {
                h: d.h.map(|size| size * proportion),
                body: base,
                code: d.code * proportion,
                table,
            },
            Some(ratio) => {
                let mut prev = d.h[0] * proportion;
                let mut h = [prev; 6];
                for slot in h.iter_mut().skip(1) {
                    prev = (prev / ratio).max(BASE_FONT_SIZE_MIN_PT);
                    *slot = prev;
                }
                Self {
                    h,
                    body: base,
                    code: d.code * proportion,
                    table,
                }
            }
        }
    }
}

/// Named typographic scale presets for uniform, anti-aliased type sizing across HTML and PDF.
///
/// Each preset scales the entire typographic ladder (body, headings H1–H6, code,
/// tables, line heights, and layout measure) proportionally without subpixel
/// aliasing or layout distortion.
///
/// Presets:
/// - `ExtraSmall` (`xs`, `0.75x`): 12px HTML / 8.25pt PDF
/// - `Small` (`sm`, `compact`, `0.875x`): 14px HTML / 9.625pt PDF
/// - `Medium` (`md`, `default`, `normal`, `1.0x`): 16px HTML / 11.0pt PDF
/// - `Large` (`lg`, `large`, `1.125x`): 18px HTML / 12.375pt PDF
/// - `ExtraLarge` (`xl`, `1.25x`): 20px HTML / 13.75pt PDF
/// - `Huge` (`2xl`, `1.5x`): 24px HTML / 16.5pt PDF
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypeScalePreset {
    /// 0.75x scale (12px HTML / 8.25pt PDF).
    ExtraSmall,
    /// 0.875x scale (14px HTML / 9.625pt PDF).
    Small,
    /// 1.0x scale (16px HTML / 11.0pt PDF).
    #[default]
    Medium,
    /// 1.125x scale (18px HTML / 12.375pt PDF).
    Large,
    /// 1.25x scale (20px HTML / 13.75pt PDF).
    ExtraLarge,
    /// 1.5x scale (24px HTML / 16.5pt PDF).
    Huge,
}

impl TypeScalePreset {
    /// All named presets in increasing scale order.
    pub const ALL: [Self; 6] = [
        Self::ExtraSmall,
        Self::Small,
        Self::Medium,
        Self::Large,
        Self::ExtraLarge,
        Self::Huge,
    ];

    /// Parse stable preset spelling or shorthand (`xs`, `sm`, `compact`, `md`, `normal`, `default`, `lg`, `xl`, `2xl`, `huge`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "xs" | "x-small" | "extra-small" | "extrasmall" | "tiny" => Some(Self::ExtraSmall),
            "sm" | "small" | "compact" => Some(Self::Small),
            "md" | "medium" | "normal" | "default" | "regular" | "standard" => Some(Self::Medium),
            "lg" | "large" | "comfortable" => Some(Self::Large),
            "xl" | "x-large" | "extra-large" | "extralarge" => Some(Self::ExtraLarge),
            "2xl" | "xxl" | "huge" | "display" => Some(Self::Huge),
            _ => None,
        }
    }

    /// Stable CLI/config/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExtraSmall => "xs",
            Self::Small => "sm",
            Self::Medium => "md",
            Self::Large => "lg",
            Self::ExtraLarge => "xl",
            Self::Huge => "2xl",
        }
    }

    /// Descriptive human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ExtraSmall => "Extra Small (75%)",
            Self::Small => "Small / Compact (87.5%)",
            Self::Medium => "Medium / Default (100%)",
            Self::Large => "Large (112.5%)",
            Self::ExtraLarge => "Extra Large (125%)",
            Self::Huge => "Huge (150%)",
        }
    }

    /// Proportional scale factor.
    #[must_use]
    pub const fn scale_factor(self) -> f32 {
        match self {
            Self::ExtraSmall => 0.75,
            Self::Small => 0.875,
            Self::Medium => 1.0,
            Self::Large => 1.125,
            Self::ExtraLarge => 1.25,
            Self::Huge => 1.5,
        }
    }

    /// Crisp HTML root font size in CSS pixels (snapped to whole integer pixels to prevent text aliasing).
    #[must_use]
    pub const fn html_base_px(self) -> u16 {
        match self {
            Self::ExtraSmall => 12,
            Self::Small => 14,
            Self::Medium => 16,
            Self::Large => 18,
            Self::ExtraLarge => 20,
            Self::Huge => 24,
        }
    }

    /// Nominal PDF body font size in points.
    #[must_use]
    pub const fn pdf_base_pt(self) -> f32 {
        match self {
            Self::ExtraSmall => 8.25,
            Self::Small => 9.625,
            Self::Medium => 11.0,
            Self::Large => 12.375,
            Self::ExtraLarge => 13.75,
            Self::Huge => 16.5,
        }
    }
}

/// Typographic scale configuration, either a named preset or a custom multiplier / size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontScale {
    /// One of the standard named presets.
    Preset(TypeScalePreset),
    /// Custom positive multiplier (e.g. 1.25 = 125%).
    Factor(f32),
}

impl Default for FontScale {
    fn default() -> Self {
        Self::Preset(TypeScalePreset::Medium)
    }
}

impl FontScale {
    /// Parse a font scale string (preset name, percentage like `125%`, decimal like `1.2`, or `px`/`pt` values).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if let Some(preset) = TypeScalePreset::parse(trimmed) {
            return Some(Self::Preset(preset));
        }
        if let Some(pct) = trimmed.strip_suffix('%') {
            if let Ok(val) = pct.trim().parse::<f32>() {
                if val.is_finite() && val > 0.0 {
                    return Some(Self::from_factor(val / 100.0));
                }
            }
        }
        if let Some(px) = trimmed.strip_suffix("px") {
            if let Ok(val) = px.trim().parse::<f32>() {
                if val.is_finite() && val > 0.0 {
                    return Some(Self::from_factor(val / 16.0));
                }
            }
        }
        if let Some(pt) = trimmed.strip_suffix("pt") {
            if let Ok(val) = pt.trim().parse::<f32>() {
                if val.is_finite() && val > 0.0 {
                    return Some(Self::from_factor(val / 11.0));
                }
            }
        }
        if let Ok(val) = trimmed.parse::<f32>() {
            if val.is_finite() && val > 0.0 {
                return Some(Self::from_factor(val));
            }
        }
        None
    }

    /// Construct a font scale from a custom float factor.
    #[must_use]
    pub fn from_factor(factor: f32) -> Self {
        let clamped = factor.clamp(0.5, 3.0);
        for preset in TypeScalePreset::ALL {
            if (preset.scale_factor() - clamped).abs() < 1e-4 {
                return Self::Preset(preset);
            }
        }
        Self::Factor(clamped)
    }

    /// Proportional scale factor.
    #[must_use]
    pub fn scale_factor(self) -> f32 {
        match self {
            Self::Preset(p) => p.scale_factor(),
            Self::Factor(f) => f.clamp(0.5, 3.0),
        }
    }

    /// HTML base font size in pixels, snapped to clean whole pixels to prevent antialiasing blur.
    #[must_use]
    pub fn html_base_px(self) -> f32 {
        match self {
            Self::Preset(p) => p.html_base_px() as f32,
            Self::Factor(f) => {
                let raw = 16.0 * f.clamp(0.5, 3.0);
                raw.round().max(8.0)
            }
        }
    }

    /// PDF base font size in points (clamped to [6, 24]).
    #[must_use]
    pub fn pdf_base_pt(self) -> f32 {
        match self {
            Self::Preset(p) => p.pdf_base_pt(),
            Self::Factor(f) => {
                (11.0 * f.clamp(0.5, 3.0)).clamp(BASE_FONT_SIZE_MIN_PT, BASE_FONT_SIZE_MAX_PT)
            }
        }
    }

    /// Scale a `Theme` in place (spacing base_px, max_width_px, etc.).
    #[must_use]
    pub fn apply_to_theme(self, mut theme: Theme) -> Theme {
        let factor = self.scale_factor();
        theme.spacing.base_px = self.html_base_px().round() as u16;
        theme.spacing.max_width_px = ((760.0 * factor).round() as u16).max(400);
        theme
    }
}

#[cfg(test)]
mod type_scale_tests {
    use super::*;

    #[test]
    fn all_none_reproduces_legacy_ladder_exactly() {
        assert_eq!(TypeScale::resolve(None, None, None), TypeScale::default());
    }

    #[test]
    fn non_finite_overrides_fall_back_to_defaults() {
        assert_eq!(
            TypeScale::resolve(Some(f32::NAN), Some(f32::INFINITY), Some(f32::NEG_INFINITY)),
            TypeScale::default()
        );
    }

    #[test]
    fn base_override_rescales_whole_hierarchy_proportionally() {
        let s = TypeScale::resolve(Some(16.5), None, None);
        assert_eq!(s.body, 16.5);
        assert!((s.h[0] - 36.0).abs() < 1e-4); // 24 * 1.5
        assert!((s.code - 14.25).abs() < 1e-4); // 9.5 * 1.5
        assert!((s.table - 15.0).abs() < 1e-4); // 10 * 1.5
    }

    #[test]
    fn heading_scale_builds_monotone_geometric_ladder() {
        let s = TypeScale::resolve(Some(10.0), Some(1.25), None);
        // H1 anchor is (24/11) x 10.
        assert!((s.h[0] - 240.0 / 11.0).abs() < 1e-4);
        for pair in s.h.windows(2) {
            assert!(pair[0] > pair[1], "headings must strictly decrease");
        }
        assert!((s.h[1] - s.h[0] / 1.25).abs() < 1e-4);
        // Table defaults track the base proportion within documented bounds.
        assert!((s.table - 10.0 * (10.0 / 11.0)).abs() < 1e-4);
    }

    #[test]
    fn overrides_clamp_into_documented_bounds() {
        let tiny = TypeScale::resolve(Some(1.0), Some(9.9), Some(0.5));
        assert_eq!(tiny.body, BASE_FONT_SIZE_MIN_PT);
        assert_eq!(tiny.h[3], BASE_FONT_SIZE_MIN_PT);
        assert_eq!(tiny.table, TABLE_FONT_SIZE_FLOOR_PT);
        let huge = TypeScale::resolve(Some(99.0), Some(0.0), Some(99.0));
        assert_eq!(huge.body, BASE_FONT_SIZE_MAX_PT);
        assert!(
            (huge.h[0] - TypeScale::default().h[0] * (BASE_FONT_SIZE_MAX_PT / 11.0)).abs() < 1e-3
        );
    }

    #[test]
    fn type_scale_presets_parse_and_scale_cleanly() {
        assert_eq!(TypeScalePreset::parse("xs"), Some(TypeScalePreset::ExtraSmall));
        assert_eq!(TypeScalePreset::parse("sm"), Some(TypeScalePreset::Small));
        assert_eq!(TypeScalePreset::parse("compact"), Some(TypeScalePreset::Small));
        assert_eq!(TypeScalePreset::parse("md"), Some(TypeScalePreset::Medium));
        assert_eq!(TypeScalePreset::parse("default"), Some(TypeScalePreset::Medium));
        assert_eq!(TypeScalePreset::parse("lg"), Some(TypeScalePreset::Large));
        assert_eq!(TypeScalePreset::parse("xl"), Some(TypeScalePreset::ExtraLarge));
        assert_eq!(TypeScalePreset::parse("2xl"), Some(TypeScalePreset::Huge));

        for preset in TypeScalePreset::ALL {
            let scale = FontScale::Preset(preset);
            assert!(scale.html_base_px() >= 12.0 && scale.html_base_px() <= 24.0);
            assert!(scale.pdf_base_pt() >= 8.0 && scale.pdf_base_pt() <= 17.0);
            let theme = scale.apply_to_theme(Theme::default());
            assert_eq!(theme.spacing.base_px as f32, scale.html_base_px());
        }
    }

    #[test]
    fn font_scale_parsing_handles_percentages_and_numbers() {
        assert_eq!(FontScale::parse("125%"), Some(FontScale::Preset(TypeScalePreset::ExtraLarge)));
        assert_eq!(FontScale::parse("75%"), Some(FontScale::Preset(TypeScalePreset::ExtraSmall)));
        assert_eq!(FontScale::parse("1.5"), Some(FontScale::Preset(TypeScalePreset::Huge)));
        assert_eq!(FontScale::parse("18px"), Some(FontScale::Preset(TypeScalePreset::Large)));
    }
}
