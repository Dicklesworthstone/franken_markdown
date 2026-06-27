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

    /// CSS body font stack. Until the embedded-subset-font subsystem lands, we
    /// use a high-quality system stack so output stays dependency-free and still
    /// looks excellent; embedded `@font-face` subsets replace this.
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
        page.size.name,
        json_num(page.size.width_pt),
        json_num(page.size.height_pt),
        json_num(page.margins.top_pt),
        json_num(page.margins.right_pt),
        json_num(page.margins.bottom_pt),
        json_num(page.margins.left_pt),
    )
}

fn json_num(value: f32) -> String {
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
