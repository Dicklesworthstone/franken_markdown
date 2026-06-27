//! Theme: the typography + colour model shared by the HTML emitter and the
//! (in-build-out) PDF layout engine. One theme drives both outputs so the HTML
//! preview and the PDF stay visually consistent.

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
}

/// A render theme.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Body font family.
    pub font: FontFamily,
    /// Base font size in px (HTML) / pt (PDF, later).
    pub base_px: u16,
    /// Readable content column width in px.
    pub max_width_px: u16,
    /// Accent colour for links and selected emphasis.
    pub accent: String,
    /// Include a `prefers-color-scheme: dark` variant in the default CSS.
    pub dark_mode: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: FontFamily::Sans,
            base_px: 16,
            max_width_px: 760,
            accent: "#0969da".to_string(),
            dark_mode: true,
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
        Self {
            font: FontFamily::Serif,
            ..Self::default()
        }
    }

    /// CSS body font stack. Until the embedded-subset-font subsystem lands (a
    /// bead), we use a high-quality system stack so output stays dependency-free
    /// and still looks excellent; embedded `@font-face` subsets replace this.
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
        "\"JetBrains Mono\", \"IBM Plex Mono\", \"SFMono-Regular\", \"SF Mono\", \
         Menlo, Consolas, \"Liberation Mono\", monospace"
    }
}
