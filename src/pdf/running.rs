//! Opt-in running page chrome (GH #13): left/center/right text slots in the
//! header and footer margin bands, a closed token set, and optional rules.
//!
//! Chrome is resolved against the final page geometry before layout-dependent
//! work, expanded per page once the page count is known (so every glyph it
//! draws is part of the document's font subset), and painted after pagination
//! as `/Artifact` pagination content. It never feeds layout, pagination or the
//! structure tree. Default options resolve to `None`, so default output bytes
//! are untouched.

use super::{
    EmbeddedFace, EmbeddedFaceLookup, F_BODY, Faces, Fill, LinkAnnotation, PageGeom, Palette, Seg,
    ShapedRunCache, append_rgb_stroke_line_operator, civil_from_unix_days, draw_seg,
    fallback_slot_runs, seg_text_hash, text_composition,
};
use crate::ast::Document;
use crate::{PdfOptions, PdfRunningBand, RenderError};

/// Minimum horizontal clearance between neighbouring slots.
const SLOT_GAP_PT: f32 = 12.0;
/// Share of the content width a side slot may take when a center slot exists.
const SIDE_SHARE_WITH_CENTER: f32 = 0.4;
/// Clearance between band text and its rule.
const RULE_GAP_PT: f32 = 3.0;
/// Rule stroke width (a hairline that survives print).
const RULE_WIDTH_PT: f32 = 0.5;
/// The band text baseline never sits closer than this to the paper edge
/// (the historical folio position: `max(bottom / 2, 18)`).
const MIN_EDGE_OFFSET_PT: f32 = 18.0;
/// Chrome size relative to the body size: 9 pt at the 11 pt default, the
/// historical folio size.
const SIZE_RATIO: f32 = 9.0 / 11.0;

/// One parsed template piece. Static tokens are substituted at resolve time;
/// only the page-dependent ones survive.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Page,
    Pages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BandKind {
    Header,
    Footer,
}

impl BandKind {
    const fn marker(self) -> &'static str {
        match self {
            Self::Header => "/Artifact <</Type /Pagination /Subtype /Header>> BDC\n",
            Self::Footer => "/Artifact <</Type /Pagination /Subtype /Footer>> BDC\n",
        }
    }
}

/// Static values for the closed token set.
struct TokenValues {
    title: String,
    author: String,
    date: String,
}

#[derive(Debug)]
struct BandSpec {
    kind: BandKind,
    /// Left, center, right. An empty vector is an absent slot.
    slots: [Vec<Piece>; 3],
    baseline: f32,
    rule_y: Option<f32>,
}

/// Running chrome resolved against one render's options and page geometry.
#[derive(Debug)]
pub(super) struct RunningSpec {
    bands: Vec<BandSpec>,
    skip_first_page: bool,
    size: f32,
}

/// One drawn slot: its fitted text, split into per-face pieces so glyphs the
/// body face lacks (CJK, math and arrow symbols) use the same fallback faces
/// as body text instead of drawing `.notdef`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChromeRun {
    x: f32,
    width: f32,
    text: String,
    /// `(slot, text, width)` in drawing order; concatenated texts == `text`.
    pieces: Vec<(u8, String, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
struct BandPaint {
    kind: BandKind,
    baseline: f32,
    rule_y: Option<f32>,
    runs: Vec<ChromeRun>,
}

/// Every page's expanded, fitted chrome, indexed by physical page.
#[derive(Debug, Default)]
pub(super) struct ChromePages {
    pages: Vec<Vec<BandPaint>>,
    size: f32,
    left: f32,
    content_w: f32,
}

impl ChromePages {
    /// All text this render will draw, with the font slot carrying it, for
    /// font subsetting.
    pub(super) fn texts(&self) -> impl Iterator<Item = (u8, &str)> {
        self.pages.iter().flatten().flat_map(|band| {
            band.runs.iter().flat_map(|run| {
                run.pieces
                    .iter()
                    .map(|(slot, text, _)| (*slot, text.as_str()))
            })
        })
    }
}

/// Parse a slot template into pieces, substituting the static tokens.
/// Control characters (a slot is one line) become spaces. Unknown `{tokens}`
/// and unbalanced braces stay literal.
fn parse_template(template: &str, values: &TokenValues) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let token = after
            .find('}')
            .map(|close| (&after[..close], &after[close + 1..]));
        let piece = match token {
            Some(("page", tail)) => Some((Some(Piece::Page), tail)),
            Some(("pages", tail)) => Some((Some(Piece::Pages), tail)),
            Some(("title", tail)) => {
                text.push_str(&values.title);
                Some((None, tail))
            }
            Some(("author", tail)) => {
                text.push_str(&values.author);
                Some((None, tail))
            }
            Some(("date", tail)) => {
                text.push_str(&values.date);
                Some((None, tail))
            }
            _ => None,
        };
        match piece {
            Some((dynamic, tail)) => {
                if let Some(dynamic) = dynamic {
                    if !text.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut text)));
                    }
                    pieces.push(dynamic);
                }
                rest = tail;
            }
            None => {
                text.push('{');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    for piece in &mut pieces {
        if let Piece::Text(text) = piece {
            if text.chars().any(char::is_control) {
                *text = text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
            }
        }
    }
    // A slot that expands to nothing but whitespace draws nothing.
    let blank = pieces
        .iter()
        .all(|piece| matches!(piece, Piece::Text(text) if text.trim().is_empty()));
    if blank { Vec::new() } else { pieces }
}

fn expand(pieces: &[Piece], page: usize, pages: usize) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(text) => out.push_str(text),
            Piece::Page => out.push_str(&page.to_string()),
            Piece::Pages => out.push_str(&pages.to_string()),
        }
    }
    out
}

/// `YYYY-MM-DD` (UTC) for `{date}`.
fn iso_date(epoch_seconds: u64) -> String {
    const MAX_EPOCH: u64 = 253_402_300_799; // 9999-12-31T23:59:59Z
    let days = (epoch_seconds.min(MAX_EPOCH) / 86_400) as i64;
    let (year, month, day) = civil_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn nonempty(slot: Option<&String>) -> Option<&str> {
    slot.map(String::as_str).filter(|slot| !slot.is_empty())
}

/// Resolve the effective chrome for a render, or `None` when nothing draws.
///
/// `page_numbers` is sugar for `footer.center = "{page}"` when the footer
/// center slot is empty. A band the caller configured explicitly must fit its
/// margin (a render error names the fix); the footer produced purely by the
/// `page_numbers` sugar keeps the historical folio's lenient placement.
pub(super) fn resolve(
    doc: &Document,
    opts: &PdfOptions,
    faces: &Faces,
    page: PageGeom,
) -> crate::Result<Option<RunningSpec>> {
    let running = &opts.running;
    let sugar = opts.page_numbers && nonempty(running.footer.center.as_ref()).is_none();
    if running.is_empty() && !sugar {
        return Ok(None);
    }
    let values = TokenValues {
        title: opts
            .title
            .clone()
            .filter(|title| !title.is_empty())
            .or_else(|| crate::html::first_heading_text(doc))
            .unwrap_or_default(),
        author: opts.author.clone().unwrap_or_default(),
        date: opts
            .metadata_epoch_seconds
            .map(iso_date)
            .unwrap_or_default(),
    };
    let size = opts.type_scale().body * SIZE_RATIO;
    let font = faces.get(F_BODY);
    let upem = if font.units_per_em == 0 {
        1000.0
    } else {
        f32::from(font.units_per_em)
    };
    let ascent = (f32::from(font.ascent) / upem * size).max(0.0);
    let descent = (-f32::from(font.descent) / upem * size).max(0.0);

    let mut bands = Vec::with_capacity(2);
    if !running.header.is_empty() {
        let offset = (page.top / 2.0).max(MIN_EDGE_OFFSET_PT);
        // Cap height hangs from the band midline toward the body, mirroring
        // the footer baseline that sits on its midline.
        let baseline = page.height - offset - ascent * 0.7;
        let text_bottom = baseline - descent;
        let rule_y = running.header.rule.then_some(text_bottom - RULE_GAP_PT);
        let lowest = rule_y.map_or(text_bottom, |y| y - RULE_WIDTH_PT / 2.0);
        let needed = page.height - lowest;
        if needed > page.top || baseline + ascent > page.height {
            return Err(does_not_fit("header", "top", page.top, needed));
        }
        bands.push(band_spec(
            BandKind::Header,
            &running.header,
            None,
            &values,
            baseline,
            rule_y,
        ));
    }
    if !running.footer.is_empty() || sugar {
        let baseline = (page.bottom / 2.0).max(MIN_EDGE_OFFSET_PT);
        let text_top = baseline + ascent;
        let rule_y = running.footer.rule.then_some(text_top + RULE_GAP_PT);
        let highest = rule_y.map_or(text_top, |y| y + RULE_WIDTH_PT / 2.0);
        let explicit = !running.footer.is_empty();
        if explicit && (highest > page.bottom || baseline - descent < 0.0) {
            return Err(does_not_fit("footer", "bottom", page.bottom, highest));
        }
        let center = sugar.then_some("{page}");
        bands.push(band_spec(
            BandKind::Footer,
            &running.footer,
            center,
            &values,
            baseline,
            rule_y,
        ));
    }
    // A band whose slots all expand to nothing and that has no rule draws
    // nothing; drop it so it neither marks content nor costs bytes.
    bands.retain(|band| band.rule_y.is_some() || band.slots.iter().any(|slot| !slot.is_empty()));
    if bands.is_empty() {
        return Ok(None);
    }
    Ok(Some(RunningSpec {
        bands,
        skip_first_page: running.skip_first_page,
        size,
    }))
}

fn band_spec(
    kind: BandKind,
    band: &PdfRunningBand,
    center_override: Option<&str>,
    values: &TokenValues,
    baseline: f32,
    rule_y: Option<f32>,
) -> BandSpec {
    let slot =
        |template: Option<&str>| template.map_or_else(Vec::new, |t| parse_template(t, values));
    BandSpec {
        kind,
        slots: [
            slot(nonempty(band.left.as_ref())),
            slot(center_override.or_else(|| nonempty(band.center.as_ref()))),
            slot(nonempty(band.right.as_ref())),
        ],
        baseline,
        rule_y,
    }
}

fn does_not_fit(band: &str, side: &str, margin: f32, needed: f32) -> RenderError {
    RenderError::InvalidInput(format!(
        "pdf_running_{band}_does_not_fit: the running {band} needs {needed:.1}pt but the {side} \
         margin is {margin:.1}pt; increase the {side} margin (native config margin_{side}_pt, \
         MCP page.margins.{side}Pt, or PageStyle margins) or remove the {band} \
         (--pdf-{band}-* flags / PdfOptions.running.{band})"
    ))
}

impl RunningSpec {
    /// Expand and fit every page's chrome. `page_count` must be the count
    /// the emitter will produce (same lines, geometry and break plan).
    pub(super) fn layout_pages(
        &self,
        faces: &Faces,
        page: PageGeom,
        page_count: usize,
    ) -> ChromePages {
        let ellipsis = if faces.get(F_BODY).glyph_index('\u{2026}') != 0 {
            "\u{2026}"
        } else {
            "..."
        };
        let fitter = Fitter {
            faces,
            size: self.size,
            ellipsis,
        };
        let mut pages = Vec::with_capacity(page_count);
        for page_idx in 0..page_count {
            if page_idx == 0 && self.skip_first_page {
                pages.push(Vec::new());
                continue;
            }
            let bands = self
                .bands
                .iter()
                .map(|band| BandPaint {
                    kind: band.kind,
                    baseline: band.baseline,
                    rule_y: band.rule_y,
                    runs: fitter.fit_band(band, page_idx + 1, page_count, page),
                })
                .collect();
            pages.push(bands);
        }
        ChromePages {
            pages,
            size: self.size,
            left: page.left,
            content_w: page.content_w,
        }
    }
}

struct Fitter<'a> {
    faces: &'a Faces,
    size: f32,
    ellipsis: &'static str,
}

impl Fitter<'_> {
    /// Per-face pieces of `text` with their widths.
    fn pieces(&self, text: &str) -> Vec<(u8, String, f32)> {
        fallback_slot_runs(self.faces, F_BODY, text)
            .into_iter()
            .map(|(slot, piece)| {
                let width = self.faces.shaped_width_points(slot, &piece, self.size);
                (slot, piece, width)
            })
            .collect()
    }

    fn width(&self, text: &str) -> f32 {
        self.pieces(text).iter().map(|(_, _, width)| width).sum()
    }

    fn fit_band(
        &self,
        band: &BandSpec,
        page_no: usize,
        pages: usize,
        page: PageGeom,
    ) -> Vec<ChromeRun> {
        let texts: [String; 3] = std::array::from_fn(|i| expand(&band.slots[i], page_no, pages));
        let natural: [f32; 3] = std::array::from_fn(|i| {
            if texts[i].is_empty() {
                0.0
            } else {
                self.width(&texts[i])
            }
        });
        let present: [bool; 3] = std::array::from_fn(|i| !texts[i].is_empty());
        let limits = slot_limits(natural, present, page.content_w.max(0.0));
        let mut runs = Vec::with_capacity(3);
        for i in 0..3 {
            if !present[i] {
                continue;
            }
            let (text, width) = if natural[i] <= limits[i] {
                (texts[i].clone(), natural[i])
            } else {
                match self.truncate(&texts[i], limits[i]) {
                    Some(fitted) => fitted,
                    None => continue,
                }
            };
            let x = match i {
                0 => page.left,
                1 => page.left + (page.content_w - width) / 2.0,
                _ => page.left + page.content_w - width,
            };
            let pieces = self.pieces(&text);
            runs.push(ChromeRun {
                x,
                width,
                text,
                pieces,
            });
        }
        runs
    }

    /// Longest prefix (at a cluster boundary, so a combining mark never
    /// loses its base; trailing spaces trimmed) that fits `limit` with an
    /// ellipsis appended; `None` when not even the ellipsis fits.
    fn truncate(&self, text: &str, limit: f32) -> Option<(String, f32)> {
        let boundaries: Vec<usize> = text_composition::source_clusters(text)
            .scan(0usize, |offset, cluster| {
                let start = *offset;
                *offset += cluster.len();
                Some(start)
            })
            .collect();
        let candidate = |chars: usize| -> String {
            let end = boundaries.get(chars).copied().unwrap_or(text.len());
            let mut out = text[..end].trim_end().to_string();
            out.push_str(self.ellipsis);
            out
        };
        // Width grows (almost) monotonically with the prefix length: binary
        // search the longest fitting prefix, then step down past any kerning
        // wobble so the result is guaranteed to fit. Gallop first so a huge
        // value (a long first heading as `{title}`) costs shaping in
        // proportion to what fits the band, not to the whole value, on every
        // page.
        let (mut lo, mut hi) = (0usize, boundaries.len().saturating_sub(1));
        let mut probe = 1usize;
        while probe < hi {
            if self.width(&candidate(probe)) <= limit {
                lo = probe;
                probe = probe.saturating_mul(2);
            } else {
                hi = probe - 1;
                break;
            }
        }
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.width(&candidate(mid)) <= limit {
                lo = mid
            } else {
                hi = mid - 1
            }
        }
        loop {
            let fitted = candidate(lo);
            let width = self.width(&fitted);
            if width <= limit {
                return Some((fitted, width));
            }
            lo = lo.checked_sub(1)?;
        }
    }
}

/// Maximum width per slot (left, center, right) for one band on one page.
///
/// With a center slot, each side may take up to 40% of the content width and
/// the center gets the remainder of a symmetric reservation, so it stays
/// centered on the page. Without one, the two sides share the width, and a
/// short side lends its slack to a long one.
fn slot_limits(natural: [f32; 3], present: [bool; 3], width: f32) -> [f32; 3] {
    let [left, _, right] = natural;
    if present[1] {
        let cap = width * SIDE_SHARE_WITH_CENTER;
        let side = if present[0] { left.min(cap) } else { 0.0 }.max(if present[2] {
            right.min(cap)
        } else {
            0.0
        });
        let gaps = if present[0] || present[2] {
            2.0 * SLOT_GAP_PT
        } else {
            0.0
        };
        [cap, (width - 2.0 * side - gaps).max(0.0), cap]
    } else if present[0] && present[2] {
        let room = (width - SLOT_GAP_PT).max(0.0);
        let half = room / 2.0;
        if left + right <= room {
            [left, 0.0, right]
        } else if left <= half {
            [left, 0.0, room - left]
        } else if right <= half {
            [room - right, 0.0, right]
        } else {
            [half, 0.0, half]
        }
    } else {
        [width, width, width]
    }
}

/// Paint one page's chrome. Each band is one pagination artifact.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_page(
    body: &mut String,
    annots: &mut Vec<LinkAnnotation>,
    current_fill: &mut Fill,
    owner_mcid: usize,
    chrome: &ChromePages,
    page_idx: usize,
    subsets: &[EmbeddedFace<'_>],
    subset_lookup: &EmbeddedFaceLookup,
    faces: &Faces,
    shaped_cache: &ShapedRunCache,
    palette: &Palette,
) {
    let Some(bands) = chrome.pages.get(page_idx) else {
        return;
    };
    for band in bands {
        body.push_str(band.kind.marker());
        if let Some(rule_y) = band.rule_y {
            append_rgb_stroke_line_operator(
                body,
                palette.rule,
                RULE_WIDTH_PT,
                chrome.left,
                rule_y,
                chrome.left + chrome.content_w,
            );
        }
        for run in &band.runs {
            let mut x = run.x;
            for (slot, text, width) in &run.pieces {
                let seg = Seg {
                    x,
                    slot: *slot,
                    text_hash: seg_text_hash(text),
                    text: text.clone(),
                    link: None,
                    fill: Fill::Muted,
                    strike: false,
                    task: None,
                    width: *width,
                    expansion_permille: 0,
                };
                draw_seg(
                    body,
                    annots,
                    current_fill,
                    owner_mcid,
                    &seg,
                    chrome.size,
                    band.baseline,
                    subsets,
                    subset_lookup,
                    faces,
                    shaped_cache,
                    palette,
                );
                x += width;
            }
        }
        body.push_str("EMC\n");
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn values() -> TokenValues {
        TokenValues {
            title: "Widget Spec".into(),
            author: "Acme".into(),
            date: iso_date(1_700_000_000),
        }
    }

    fn expanded(template: &str, page: usize, pages: usize) -> String {
        expand(&parse_template(template, &values()), page, pages)
    }

    #[test]
    fn closed_token_set_expands_and_unknown_tokens_stay_literal() {
        assert_eq!(expanded("{page} / {pages}", 3, 12), "3 / 12");
        assert_eq!(
            expanded("{title} by {author}, {date}", 1, 1),
            "Widget Spec by Acme, 2023-11-14"
        );
        assert_eq!(expanded("{foo} {Page} {page", 2, 2), "{foo} {Page} {page");
        assert_eq!(expanded("{{page}}", 7, 9), "{7}");
        assert_eq!(expanded("}{", 1, 1), "}{");
        assert_eq!(expanded("", 1, 1), "");
    }

    #[test]
    fn substituted_values_are_not_reexpanded() {
        let values = TokenValues {
            title: "{page}".into(),
            author: String::new(),
            date: String::new(),
        };
        let pieces = parse_template("{title}", &values);
        assert_eq!(expand(&pieces, 5, 9), "{page}");
    }

    #[test]
    fn controls_become_spaces_and_blank_slots_vanish() {
        assert_eq!(expanded("a\nb\tc", 1, 1), "a b c");
        let empty = TokenValues {
            title: String::new(),
            author: String::new(),
            date: String::new(),
        };
        assert!(parse_template("{title}", &empty).is_empty());
        assert!(parse_template("  ", &empty).is_empty());
        assert!(!parse_template("{page}", &empty).is_empty());
    }

    #[test]
    fn iso_date_is_utc_and_clamped() {
        assert_eq!(iso_date(0), "1970-01-01");
        assert_eq!(iso_date(1_700_000_000), "2023-11-14");
        assert_eq!(iso_date(u64::MAX), "9999-12-31");
    }

    fn chrome(md: &str, opts: &PdfOptions, pages: usize) -> ChromePages {
        let faces = Faces::load(opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let spec = resolve(&crate::parse_markdown(md), opts, &faces, page)
            .unwrap()
            .unwrap();
        spec.layout_pages(&faces, page, pages)
    }

    fn band_texts(chrome: &ChromePages, page_idx: usize, kind: BandKind) -> Vec<String> {
        chrome.pages[page_idx]
            .iter()
            .filter(|band| band.kind == kind)
            .flat_map(|band| band.runs.iter().map(|run| run.text.clone()))
            .collect()
    }

    #[test]
    fn default_options_resolve_to_nothing() {
        let opts = PdfOptions::default();
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let doc = crate::parse_markdown("# T");
        assert!(resolve(&doc, &opts, &faces, page).unwrap().is_none());
        // Slots that expand to nothing (no epoch for {date}) draw nothing either.
        let mut opts = PdfOptions::default();
        opts.running.footer.center = Some("{date}".into());
        opts.running.skip_first_page = true;
        assert!(resolve(&doc, &opts, &faces, page).unwrap().is_none());
    }

    #[test]
    fn every_page_gets_its_own_expansion_and_first_page_can_be_skipped() {
        let mut opts = PdfOptions {
            title: None,
            metadata_epoch_seconds: Some(1_700_000_000),
            ..Default::default()
        };
        opts.running.header.left = Some("{title}".into());
        opts.running.header.right = Some("{date}".into());
        opts.running.footer.center = Some("{page} / {pages}".into());
        let pages = chrome("# From The Heading\n\nBody.", &opts, 3);
        assert_eq!(
            band_texts(&pages, 0, BandKind::Header),
            ["From The Heading", "2023-11-14"]
        );
        for (idx, want) in ["1 / 3", "2 / 3", "3 / 3"].into_iter().enumerate() {
            assert_eq!(band_texts(&pages, idx, BandKind::Footer), [want]);
        }
        opts.running.skip_first_page = true;
        let pages = chrome("# H", &opts, 2);
        assert!(pages.pages[0].is_empty());
        assert_eq!(band_texts(&pages, 1, BandKind::Footer), ["2 / 2"]);
        let texts: Vec<&str> = pages.texts().map(|(_, text)| text).collect();
        assert!(texts.contains(&"2 / 2") && !texts.contains(&"1 / 2"));
    }

    #[test]
    fn page_numbers_sugar_fills_an_empty_footer_center_only() {
        let opts = PdfOptions {
            page_numbers: true,
            ..Default::default()
        };
        assert_eq!(
            band_texts(&chrome("x", &opts, 2), 1, BandKind::Footer),
            ["2"]
        );
        let mut explicit = opts.clone();
        explicit.running.footer.center = Some("p. {page}".into());
        assert_eq!(
            band_texts(&chrome("x", &explicit, 2), 1, BandKind::Footer),
            ["p. 2"]
        );
        let mut sides = opts;
        sides.running.footer.left = Some("L".into());
        assert_eq!(
            band_texts(&chrome("x", &sides, 1), 0, BandKind::Footer),
            ["L", "1"]
        );
    }

    #[test]
    fn overflowing_slots_are_ellipsized_within_their_limit_and_stay_apart() {
        let long = "An extraordinarily long running title that cannot possibly fit ".repeat(4);
        let mut opts = PdfOptions::default();
        opts.running.header.left = Some(long.clone());
        opts.running.header.center = Some(long.clone());
        opts.running.header.right = Some(long);
        let pages = chrome("x", &opts, 1);
        let page = PageGeom::from_theme(&opts.theme);
        let runs = &pages.pages[0][0].runs;
        assert_eq!(runs.len(), 3);
        for run in runs {
            assert!(run.text.ends_with('\u{2026}'), "{}", run.text);
            assert!(run.x >= page.left - 0.01);
            assert!(run.x + run.width <= page.left + page.content_w + 0.01);
        }
        assert!(runs[0].x + runs[0].width < runs[1].x);
        assert!(runs[1].x + runs[1].width < runs[2].x);
    }

    #[test]
    fn explicit_bands_must_fit_their_margins_but_the_legacy_folio_stays_lenient() {
        let mut opts = PdfOptions::default();
        opts.theme.page.margins.top_pt = 20.0;
        opts.theme.page.margins.bottom_pt = 20.0;
        let faces = Faces::load(&opts).unwrap();
        let page = PageGeom::from_theme(&opts.theme);
        let doc = crate::parse_markdown("x");
        let mut header = opts.clone();
        header.running.header.center = Some("H".into());
        let err = resolve(&doc, &header, &faces, page)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("pdf_running_header_does_not_fit") && err.contains("margin_top_pt"),
            "{err}"
        );
        let mut footer = opts.clone();
        footer.running.footer.rule = true;
        let err = resolve(&doc, &footer, &faces, page)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("pdf_running_footer_does_not_fit") && err.contains("margin_bottom_pt"),
            "{err}"
        );
        let legacy = PdfOptions {
            page_numbers: true,
            ..opts
        };
        assert!(resolve(&doc, &legacy, &faces, page).unwrap().is_some());
        // Default 72pt margins fit text, rules and both bands.
        let mut full = PdfOptions::default();
        full.running.header = PdfRunningBand {
            left: Some("a".into()),
            center: None,
            right: None,
            rule: true,
        };
        full.running.footer = PdfRunningBand {
            left: None,
            center: Some("b".into()),
            right: None,
            rule: true,
        };
        assert!(
            resolve(&doc, &full, &faces, PageGeom::from_theme(&full.theme))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn glyphs_the_body_face_lacks_use_the_fallback_faces() {
        // The GH #3 repertoire: the body faces lack some of these, the
        // bundled symbol face carries them.
        let symbols =
            "\u{2248} \u{2212} \u{2192} \u{21d2} \u{2260} \u{2264} \u{2211} \u{221a} \u{221e}";
        let mut opts = PdfOptions::default();
        opts.running.header.left = Some(format!("Sum {symbols} of (a\\b)"));
        let faces = Faces::load(&opts).unwrap();
        let fallback: Vec<char> = symbols
            .chars()
            .filter(|&c| faces.fallback_slot(F_BODY, c) == super::super::F_SYMBOL)
            .collect();
        assert!(!fallback.is_empty(), "fixture needs a fallback glyph");
        let pages = chrome("x", &opts, 1);
        let run = &pages.pages[0][0].runs[0];
        let joined: String = run
            .pieces
            .iter()
            .map(|(_, text, _)| text.as_str())
            .collect();
        assert_eq!(joined, run.text);
        for c in fallback {
            assert!(
                run.pieces
                    .iter()
                    .any(|(slot, text, _)| { *slot == super::super::F_SYMBOL && text.contains(c) }),
                "{c:?} not routed to the symbol face: {:?}",
                run.pieces
            );
        }
        let widths: f32 = run.pieces.iter().map(|(_, _, width)| width).sum();
        assert!((widths - run.width).abs() < 0.01);
        assert!(
            pages
                .texts()
                .any(|(slot, _)| slot == super::super::F_SYMBOL)
        );
    }

    #[test]
    fn a_huge_title_is_ellipsized_to_the_band() {
        let mut opts = PdfOptions {
            title: Some("Enormous title words ".repeat(20_000)),
            ..Default::default()
        };
        opts.running.header.left = Some("{title}".into());
        opts.running.header.right = Some("{page}".into());
        let pages = chrome("x", &opts, 3);
        let page = PageGeom::from_theme(&opts.theme);
        assert_eq!(pages.pages.len(), 3);
        for bands in &pages.pages {
            let runs = &bands[0].runs;
            assert!(runs[0].text.ends_with('\u{2026}'));
            assert!(runs[0].text.len() < 1_000, "{}", runs[0].text.len());
            assert!(runs[0].x + runs[0].width < runs[1].x);
            assert!(runs[1].x + runs[1].width <= page.left + page.content_w + 0.01);
        }
    }

    #[test]
    fn truncation_never_separates_a_combining_mark_from_its_base() {
        let mut opts = PdfOptions::default();
        opts.running.header.center = Some("e\u{301}".repeat(400));
        let pages = chrome("x", &opts, 1);
        let text = &pages.pages[0][0].runs[0].text;
        let body = text.strip_suffix('\u{2026}').expect("ellipsized");
        assert!(body.ends_with("e\u{301}"), "{text:?}");
    }

    #[test]
    fn slot_limits_keep_center_centered_and_lend_side_slack() {
        let w = 100.0;
        // Center only: full width.
        assert_eq!(slot_limits([0.0, 50.0, 0.0], [false, true, false], w)[1], w);
        // Center with a short left side reserves that width on both sides.
        let limits = slot_limits([10.0, 90.0, 0.0], [true, true, false], w);
        assert_eq!(limits[1], w - 2.0 * 10.0 - 2.0 * SLOT_GAP_PT);
        // Long sides cap at 40% each.
        let limits = slot_limits([90.0, 90.0, 90.0], [true, true, true], w);
        assert_eq!(limits[0], 40.0);
        assert!(limits[1] >= 0.0);
        // Two sides: the short one lends slack.
        let limits = slot_limits([10.0, 0.0, 200.0], [true, false, true], w);
        assert_eq!(limits, [10.0, 0.0, w - SLOT_GAP_PT - 10.0]);
    }
}
