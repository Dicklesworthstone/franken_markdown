//! EPUB-local font resources shared by document and book export.
//!
//! Explicit host faces or Full embedding opt into primary faces. Default
//! Subset keeps Latin archives font-free; covered CJK text adds only its enabled
//! fallback face. The rendered repertoire is collected once per publication.

use std::collections::BTreeSet;

use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::text::{EmbeddingFormat, Font};
use franken_markdown::{
    FontAssetSlot, FontEmbedding, FontFamily, HtmlOptions, RenderError, Result,
};

use super::{ZipWriter, fnv1a64};

const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_FONT_BYTES: usize = 128 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHARACTERS: usize = 65_536;
const CSS_PATH: &str = "embedded-fonts.css";
const FONT_LINK: &str =
    "<link rel=\"stylesheet\" type=\"text/css\" href=\"embedded-fonts.css\"/>\n";
const STYLE_LINK: &str = "<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\n";

fn invalid(message: impl std::fmt::Display) -> RenderError {
    RenderError::InvalidInput(format!("epub_fonts: {message}"))
}

/// Validate before cloning any host options or entering the HTML renderer.
/// Public FontAssets fields must not bypass the same validation as set_slot.
pub(super) fn enabled(opts: &HtmlOptions) -> Result<bool> {
    let mut total = 0usize;
    let mut supplied = false;
    for slot in FontAssetSlot::ALL {
        if let Some(bytes) = opts.font_assets.slot_bytes(slot) {
            supplied = true;
            total = total
                .checked_add(bytes.len())
                .ok_or_else(|| invalid("font size overflow"))?;
            if bytes.len() > MAX_FONT_BYTES || total > MAX_TOTAL_FONT_BYTES {
                return Err(invalid(
                    "host fonts exceed 32 MiB per slot or 128 MiB in total",
                ));
            }
        }
    }
    opts.font_assets.validate()?;
    Ok(supplied || opts.font_embedding == FontEmbedding::Full)
}

/// Collect emitted text, including MathML symbols and generated footnote
/// labels. ASCII syntax in markup is harmless extra coverage, and avoids a
/// second HTML parser. Call after data images have become archive references.
#[derive(Default)]
pub(super) struct Repertoire {
    enabled: bool,
    cjk: Option<&'static Font>,
    bytes: usize,
    characters: BTreeSet<char>,
}

impl Repertoire {
    pub(super) fn new(enabled: bool) -> Self {
        let mut result = Self {
            enabled,
            // The broad fallback is cached once; publications borrow it.
            cjk: fonts::cjk_font()
                .and_then(|loaded| loaded.ok())
                .map(|(font, _)| font),
            ..Self::default()
        };
        if enabled {
            // Browser-generated list markers, generated labels and the five
            // XML named entities must remain available even without text nodes.
            result.characters.extend(' '..='~');
            result.characters.extend(['\u{a0}', '•', '◦', '▪', '↩']);
        }
        result
    }

    pub(super) fn add(&mut self, text: &str) -> Result<()> {
        if !self.enabled && self.cjk.is_none() {
            return Ok(());
        }
        if self.enabled && text.len() > MAX_TEXT_BYTES.saturating_sub(self.bytes) {
            return Err(invalid("font repertoire text exceeds 64 MiB"));
        }
        let mut relevant = self.enabled;
        for ch in text.chars() {
            relevant |= self.insert(ch)?;
        }
        // Numeric XML entities otherwise contribute only their ASCII spelling.
        // Names are not decoded as arbitrary HTML entities: emitted XHTML uses
        // XML's five named entities, already covered by the ASCII seed above.
        for tail in text.split("&#").skip(1) {
            let Some(end) = tail.bytes().take(10).position(|b| b == b';') else {
                continue;
            };
            let number = &tail[..end];
            let (digits, radix) = number.strip_prefix('x').map_or((number, 10), |s| (s, 16));
            if !digits.is_empty()
                && digits.bytes().all(|b| {
                    if radix == 16 {
                        b.is_ascii_hexdigit()
                    } else {
                        b.is_ascii_digit()
                    }
                })
            {
                if let Some(ch) = u32::from_str_radix(digits, radix)
                    .ok()
                    .and_then(char::from_u32)
                {
                    relevant |= self.insert(ch)?;
                }
            }
        }
        if relevant {
            if text.len() > MAX_TEXT_BYTES.saturating_sub(self.bytes) {
                return Err(invalid("font repertoire text exceeds 64 MiB"));
            }
            self.bytes += text.len();
        }
        Ok(())
    }

    fn insert(&mut self, ch: char) -> Result<bool> {
        // A default publication needs only actual CJK glyphs, not the ASCII
        // markup/Latin seed that would force primary font resources into it.
        if !self.enabled
            && self
                .cjk
                .as_ref()
                .is_none_or(|font| ch.is_ascii() || font.glyph_index(ch) == 0)
        {
            return Ok(false);
        }
        if self.characters.contains(&ch) {
            return Ok(true);
        }
        if self.characters.len() == MAX_CHARACTERS {
            return Err(invalid(
                "font repertoire exceeds 65536 distinct Unicode characters",
            ));
        }
        self.characters.insert(ch);
        Ok(true)
    }

    pub(super) fn finish(self, opts: &HtmlOptions) -> Result<Package> {
        let cjk = self.cjk.filter(|font| {
            self.characters
                .iter()
                .any(|&ch| !ch.is_ascii() && font.glyph_index(ch) != 0)
        });
        if !self.enabled && cjk.is_none() {
            return Ok(Package::default());
        }
        let keep: Vec<char> = self.characters.into_iter().collect();
        let mut package = Package::default();
        if self.enabled {
            for slot in FontAssetSlot::ALL {
                let style = match slot {
                    FontAssetSlot::BodyRegular | FontAssetSlot::MonoRegular => FontStyle::Regular,
                    FontAssetSlot::BodyBold => FontStyle::Bold,
                    FontAssetSlot::BodyItalic => FontStyle::Italic,
                    FontAssetSlot::BodyBoldItalic => FontStyle::BoldItalic,
                };
                let font = match opts.font_assets.resolved_bytes(slot) {
                    Some(bytes) => {
                        let font = Font::parse(bytes.to_vec()).map_err(invalid)?;
                        if font.instance_bounds(*b"wght").is_some() {
                            font.instance(f32::from(opts.font_assets.effective_weight(slot)))
                                .ok_or_else(|| {
                                    invalid(format!(
                                        "cannot instance {} at the requested weight",
                                        slot.as_str()
                                    ))
                                })?
                        } else {
                            font
                        }
                    }
                    None if slot == FontAssetSlot::MonoRegular => {
                        fonts::load_mono(style).map_err(invalid)?
                    }
                    None => fonts::load_body(opts.theme.font, style).map_err(invalid)?,
                };
                let mono = slot == FontAssetSlot::MonoRegular;
                let family = if mono { "FmdEpubMono" } else { "FmdEpubBody" };
                let italic = matches!(
                    slot,
                    FontAssetSlot::BodyItalic | FontAssetSlot::BodyBoldItalic
                );
                // CSS describes semantic slots (normal/bold). Weight pins select
                // the outlines in that slot, as on the PDF path, not a new style.
                package.face(
                    &font,
                    &keep,
                    family,
                    slot.default_weight(),
                    italic,
                    opts.font_embedding,
                )?;
            }
            let symbols = Font::parse(fonts::symbol_bytes().to_vec()).map_err(invalid)?;
            package.face(
                &symbols,
                &keep,
                "FmdEpubSymbols",
                400,
                false,
                opts.font_embedding,
            )?;
        }
        let has_cjk = cjk.is_some();
        if let Some(font) = cjk {
            let cjk_keep: Vec<char> = keep
                .iter()
                .copied()
                .filter(|&ch| !ch.is_ascii() && font.glyph_index(ch) != 0)
                .collect();
            package.face(
                font,
                &cjk_keep,
                "FmdEpubCjk",
                400,
                false,
                opts.font_embedding,
            )?;
        }
        let fallback = match opts.theme.font {
            FontFamily::Sans => "sans-serif",
            FontFamily::Serif => "serif",
        };
        if self.enabled {
            let cjk_family = if has_cjk { "\"FmdEpubCjk\"," } else { "" };
            package.css.push_str(&format!(
                "body{{font-family:\"FmdEpubBody\",\"FmdEpubSymbols\",{cjk_family}{fallback};}}\n\
                 pre,code,kbd,samp{{font-family:\"FmdEpubMono\",\"FmdEpubSymbols\",{cjk_family}monospace;}}\n"
            ));
        } else {
            // This subset has no ASCII/Latin cmap entries: reading-system
            // families still paint those characters in default publications.
            package.css.push_str(&format!(
                "body{{font-family:\"FmdEpubCjk\",{fallback};}}\n\
                 pre,code,kbd,samp{{font-family:\"FmdEpubCjk\",monospace;}}\n"
            ));
        }
        Ok(package)
    }
}

struct Resource {
    href: String,
    bytes: Vec<u8>,
    format: EmbeddingFormat,
}

#[derive(Default)]
pub(super) struct Package {
    css: String,
    resources: Vec<Resource>,
    payload_bytes: usize,
}

impl Package {
    fn face(
        &mut self,
        font: &Font,
        keep: &[char],
        family: &str,
        weight: u16,
        italic: bool,
        embedding: FontEmbedding,
    ) -> Result<()> {
        // Full embedding retains the complete cmap and layout program for
        // the reading system. Compact embedding keeps the established bytes;
        // CFF uses the strict API so unsupported used outlines cannot become
        // a successful blank subset.
        let (bytes, format) = if embedding == FontEmbedding::Full {
            let format = if font.has_glyf_outlines() {
                EmbeddingFormat::TrueType
            } else {
                EmbeddingFormat::OpenTypeCff
            };
            (font.as_sfnt().to_vec(), format)
        } else if font.has_glyf_outlines() {
            (
                font.subset(keep)
                    .ok_or_else(|| invalid("font subsetting failed"))?,
                EmbeddingFormat::TrueType,
            )
        } else {
            let subset = font.try_subset(keep).map_err(invalid)?;
            (subset.bytes, subset.format)
        };
        let (extension, css_format) = match format {
            EmbeddingFormat::TrueType => ("ttf", "truetype"),
            EmbeddingFormat::OpenTypeCff => ("otf", "opentype"),
        };
        if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
            return Err(invalid(if embedding == FontEmbedding::Full {
                "full font is empty or exceeds 32 MiB"
            } else {
                "font subset is empty or exceeds 32 MiB"
            }));
        }
        let index = match self
            .resources
            .iter()
            .position(|resource| resource.format == format && resource.bytes == bytes)
        {
            Some(index) => index,
            None => {
                if bytes.len() > MAX_TOTAL_FONT_BYTES.saturating_sub(self.payload_bytes) {
                    return Err(invalid(if embedding == FontEmbedding::Full {
                        "full fonts exceed 128 MiB"
                    } else {
                        "font subsets exceed 128 MiB"
                    }));
                }
                self.payload_bytes += bytes.len();
                let index = self.resources.len();
                self.resources.push(Resource {
                    href: format!("fonts/font-{}.{extension}", index + 1),
                    bytes,
                    format,
                });
                index
            }
        };
        let style = if italic { "italic" } else { "normal" };
        self.css.push_str(&format!(
            "@font-face{{font-family:\"{family}\";font-style:{style};font-weight:{weight};src:url(\"{}\") format(\"{css_format}\");}}\n",
            self.resources[index].href,
        ));
        Ok(())
    }

    pub(super) fn byte_len(&self) -> usize {
        self.payload_bytes + self.css.len()
    }

    pub(super) fn check_output(&self, sizes: impl IntoIterator<Item = usize>) -> Result<()> {
        if self.resources.is_empty() {
            return Ok(());
        }
        let mut total = self.byte_len();
        for size in sizes {
            total = total
                .checked_add(size)
                .ok_or_else(|| invalid("publication size overflow"))?;
            if total > 256 * 1024 * 1024 {
                return Err(invalid("font-enabled publication exceeds 256 MiB"));
            }
        }
        Ok(())
    }

    /// Extend the renderer's own package shell. No arbitrary XML is parsed.
    pub(super) fn manifest(&self, opf: &mut String) -> Result<()> {
        if self.resources.is_empty() {
            return Ok(());
        }
        let at = opf
            .find("</manifest>")
            .ok_or_else(|| invalid("package manifest boundary missing"))?;
        let mut entries =
            format!("<item id=\"epub-font-css\" href=\"{CSS_PATH}\" media-type=\"text/css\"/>\n");
        for (index, resource) in self.resources.iter().enumerate() {
            let media_type = match resource.format {
                EmbeddingFormat::TrueType => "font/ttf",
                EmbeddingFormat::OpenTypeCff => "font/otf",
            };
            entries.push_str(&format!(
                "<item id=\"epub-font-{}\" href=\"{}\" media-type=\"{media_type}\"/>\n",
                index + 1,
                resource.href
            ));
        }
        opf.insert_str(at, &entries);
        Ok(())
    }

    /// Custom CSS remains verbatim and last in the cascade. Without custom
    /// CSS the font-family rules follow the historical serif/mono defaults.
    /// Navigation documents also need the shared stylesheet and font faces.
    pub(super) fn link(&self, xhtml: &mut String, custom_css: bool) -> Result<()> {
        if self.resources.is_empty() {
            return Ok(());
        }
        if let Some(at) = xhtml.find(STYLE_LINK) {
            xhtml.insert_str(
                if custom_css {
                    at
                } else {
                    at + STYLE_LINK.len()
                },
                FONT_LINK,
            );
        } else {
            let at = xhtml
                .find("</head>")
                .ok_or_else(|| invalid("XHTML head boundary missing"))?;
            let links = if custom_css {
                format!("{FONT_LINK}{STYLE_LINK}")
            } else {
                format!("{STYLE_LINK}{FONT_LINK}")
            };
            xhtml.insert_str(at, &links);
        }
        Ok(())
    }

    /// Length-delimited fingerprint joins the existing content-derived EPUB
    /// identifier; it is not a cryptographic signature or authenticity check.
    pub(super) fn fingerprint(&self) -> Option<String> {
        if self.resources.is_empty() {
            return None;
        }
        let mut a = 0xcbf2_9ce4_8422_2325;
        let mut b = 0x9e37_79b9_7f4a_7c15;
        let mut part = |bytes: &[u8]| {
            let length = (bytes.len() as u64).to_le_bytes();
            a = fnv1a64(fnv1a64(a, &length), bytes);
            b = fnv1a64(fnv1a64(b, &length), bytes);
        };
        part(self.css.as_bytes());
        for resource in &self.resources {
            part(resource.href.as_bytes());
            part(&resource.bytes);
        }
        Some(format!("{a:016x}{b:016x}"))
    }

    pub(super) fn write(&self, zip: &mut ZipWriter) {
        if self.resources.is_empty() {
            return;
        }
        zip.add_deflated(&format!("OEBPS/{CSS_PATH}"), self.css.as_bytes());
        for resource in &self.resources {
            zip.add_deflated(&format!("OEBPS/{}", resource.href), &resource.bytes);
        }
    }
}

#[cfg(test)]
#[path = "font_tests.rs"]
mod tests;
