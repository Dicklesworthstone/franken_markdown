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
                 \"Helvetica Neue\", Arial, \"Noto Sans\", sans-serif"
            }
            FontFamily::Serif => {
                "\"Source Serif 4\", Newsreader, \"Iowan Old Style\", \"Apple Garamond\", \
                 Georgia, Cambria, \"Times New Roman\", Times, serif"
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
}
