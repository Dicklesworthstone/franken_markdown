//! Opt-in, fixed-point body baselines shared by measurement and placement.
//!
//! A row's baseline advance and total advance are integral grid pitches. Its
//! non-discardable mathematical/footing depth remains below the baseline; the
//! following gap absorbs the remainder of that pitch. The body bottom is also
//! aligned, so a page-note region has the same phase as ordinary body text.
//! Running margin bands and the internals of figures have independent geometry.

use super::*;
use crate::BaselineGrid;

#[derive(Clone, Copy)]
pub(super) struct PageGrid {
    pitch: i64,
    origin: i64,
    bottom: i64,
}

impl PageGrid {
    pub fn new(grid: BaselineGrid, origin: f32, bottom: f32) -> Self {
        Self {
            pitch: i64::from(grid.spacing_milli_points()),
            origin: milli(origin),
            bottom: outward(bottom),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct LineGrid {
    pitch: i64,
    /// From the preceding grid edge to this row's text baseline.
    advance: i64,
    leading: i64,
    gap: i64,
    natural_gap: f32,
    /// All painted content, including panel padding, rules and figure boxes.
    ink_above: i64,
    ink_below: i64,
    /// Text/figure ink used for separating rows; adjacent panel fills may join.
    text_above: i64,
    text_below: i64,
    /// A list label on a figure has its own baseline near the row's top.
    marker_ink: Option<(i64, i64)>,
    clearance: i64,
    bottom_guard: i64,
}

impl LineGrid {
    pub fn leading(self) -> f32 {
        points(self.leading)
    }

    pub fn natural_gap(self) -> f32 {
        self.natural_gap
    }

    fn marker_advance(self, size: f32, quote: bool) -> i64 {
        let above = self.marker_ink.map_or(0, |ink| ink.0);
        let above = if quote {
            above.max(outward(size * 0.85)) + outward(QUOTE_BG_PAD_V)
        } else {
            above
        };
        round_up(outward(size * 1.32).max(above + self.clearance), self.pitch)
    }

    pub fn marker_offset(self, size: f32, quote: bool) -> f32 {
        points(self.advance - self.marker_advance(size, quote))
    }

    pub fn height(self) -> LayoutUnit {
        LayoutUnit::from_milli_points((self.leading + self.gap).min(i64::from(i32::MAX)) as i32)
    }
}

fn milli(value: f32) -> i64 {
    (f64::from(value) * 1000.0).round() as i64
}

fn outward(value: f32) -> i64 {
    (f64::from(value.max(0.0)) * 1000.0).ceil() as i64
}

fn points(value: i64) -> f32 {
    (value as f64 / 1000.0) as f32
}

fn round_up(value: i64, pitch: i64) -> i64 {
    ((value.max(0) + pitch - 1) / pitch) * pitch
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("pdf_baseline_grid: {message}"))
}

/// Grid admission precedes page fitting and the ordinary API's geometry
/// sanitization: opting in cannot silently change invalid margins or fonts.
pub(super) fn validate_options(opts: &PdfOptions) -> Result<()> {
    if opts.theme.page.baseline_grid.is_none() {
        return Ok(());
    }
    if opts.fit_to_pages.is_some_and(|pages| pages > 0) {
        return Err(RenderError::InvalidInput(
            "pdf_baseline_grid_incompatible: baseline grids cannot be combined with fit_to_pages; remove the page-fit target or disable the grid".to_string(),
        ));
    }
    let page = opts.theme.page;
    let [width, height] = [page.size.width_pt, page.size.height_pt];
    let [top, right, bottom, left] = [
        page.margins.top_pt,
        page.margins.right_pt,
        page.margins.bottom_pt,
        page.margins.left_pt,
    ];
    if [width, height]
        .iter()
        .any(|value| !value.is_finite() || !(MIN_PAGE_DIM..=14_400.0).contains(value))
        || [top, right, bottom, left]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        || width - left - right < MIN_CONTENT_DIM
        || height - top - bottom < MIN_CONTENT_DIM
    {
        return Err(invalid(
            "page geometry must leave a finite printable body without clamping",
        ));
    }
    Ok(())
}

/// Measure the exact source faces used by emission. The font descriptor's
/// descent is retained as a conservative floor even for all-uppercase lines.
/// It also keeps validation meaningful before and after glyph subsetting.
fn measure(line: &Line, faces: &Faces, scratch: &mut ShapeScratch) -> LineGrid {
    let size = line
        .image
        .as_ref()
        .and_then(|image| image.marker_size)
        .unwrap_or(line.size);
    let mut above = line.text_ink.0;
    let mut below = line.text_ink.1;
    for seg in &line.segs {
        if is_inline_math(seg.slot) {
            if let Some(formula) =
                inline_math_fit::placement(&seg.text, size, seg.slot == F_DISPLAY_MATH)
            {
                above = above.max(formula.ascent);
                below = below.max(formula.descent);
            }
            continue;
        }
        if seg.slot == F_IMAGE {
            if let Some(image) = inline_image::image_box(&seg.text, size) {
                above = above.max(image.height - image.descent);
                below = below.max(image.descent);
            }
            continue;
        }
        let face = faces.face(seg.slot);
        let scale = size / f32::from(face.font.units_per_em.max(1));
        // Descriptors are emitted in 1/1000 em; round outward here too.
        below = below.max((-f32::from(face.font.descent) * scale).max(0.0));
        let run = shape_run_with_scratch(face, &seg.text, scratch);
        if let Some(positioned) = &run.positioned {
            let ink = positioned.ink(font_size_of(size));
            above = above.max(ink.0);
            below = below.max(ink.1);
        } else {
            for id in run.glyphs {
                if let Some(bbox) = face.font.glyph_bbox(id) {
                    above = above.max(f32::from(bbox[3]).max(0.0) * scale);
                    below = below.max((-f32::from(bbox[1])).max(0.0) * scale);
                }
            }
        }
    }
    let marker_ink = line
        .image
        .as_ref()
        .and_then(|image| image.marker_size)
        .map(|_| (outward(above), outward(below)));
    let mut text_above = above;
    let mut text_below = below;
    if let Some(image) = &line.image {
        // A figure is a box at the row baseline. Its internal glyph positions
        // are not individually snapped and its dimensions are never scaled.
        above = image.height_pt;
        below = 0.0;
        text_above = above;
        text_below = below;
    }
    if line.rule {
        above = above.max(line.size * 0.5 + 0.35);
    }
    for stroke in &line.table_footing {
        below = below.max(stroke.below + 0.35);
    }
    if line.bg != 0 {
        above = above.max((line.size * PANEL_ASCENT_FRAC).max(line.text_ink.0) + PANEL_PAD_V);
        below = below.max((line.size * PANEL_DESCENT_FRAC).max(line.text_ink.1) + PANEL_PAD_V);
    }
    if line.shade {
        above = above.max((line.size * 0.92).max(line.text_ink.0));
        below = below.max((line.size * 0.40).max(line.text_ink.1));
    }
    if !line.quote_bars.is_empty() {
        if line.image.is_none() {
            above = above.max(line.size * 0.85);
            below = below.max(line.size * 0.20);
        }
        above += QUOTE_BG_PAD_V;
        below += QUOTE_BG_PAD_V;
    }
    if line.bg == 0
        && !line.rule
        && line
            .segs
            .iter()
            .any(|seg| seg.slot == F_MONO && !seg.text.trim().is_empty())
    {
        above = above.max(line.size * 0.74);
        below = below.max(line.size * 0.26);
    }
    LineGrid {
        pitch: 0,
        advance: 0,
        leading: 0,
        gap: 0,
        natural_gap: line.gap_after,
        ink_above: outward(above),
        ink_below: outward(below),
        text_above: outward(text_above),
        text_below: outward(text_below),
        marker_ink,
        clearance: 0,
        bottom_guard: 0,
    }
}

fn lift(line: &Line) -> i64 {
    outward(inline_math_extents(line).1 + financial_tables::footing_depth(line))
}

fn quantize(lines: &mut [Line], pitch: i64, clearance: i64, bottom_guard: i64) {
    for line in lines {
        let Some(mut grid) = line.grid else { continue };
        // Notes and measured alternatives may already carry metrics. A block
        // can still add a final gap after measuring its text; detect that edit.
        if grid.pitch == 0 || line.gap_after != points(grid.gap) {
            grid.natural_gap = line.gap_after;
        }
        let depth = lift(line);
        let natural_advance = outward(natural_line_leading(line)).saturating_sub(depth);
        grid.pitch = pitch;
        grid.clearance = clearance;
        grid.advance = round_up(natural_advance.max(grid.ink_above + clearance), pitch);
        if let Some(size) = line.image.as_ref().and_then(|image| image.marker_size) {
            grid.advance = grid
                .advance
                .max(grid.marker_advance(size, !line.quote_bars.is_empty()));
        }
        grid.leading = grid.advance + depth;
        grid.gap = round_up(grid.leading + outward(grid.natural_gap), pitch) - grid.leading;
        grid.clearance = clearance;
        grid.bottom_guard = bottom_guard;
        line.gap_after = points(grid.gap);
        line.grid = Some(grid);
    }
}

/// Called after block spacing is final, before either page planner. Reapplying
/// it is idempotent, including notes measured early for foot-region admission.
pub(super) fn apply(lines: &mut [Line], grid: Option<BaselineGrid>, faces: &Faces) {
    let Some(grid) = grid else { return };
    let mut scratch = ShapeScratch::default();
    for line in &mut *lines {
        if line.grid.is_none() {
            line.grid = Some(measure(line, faces, &mut scratch));
        }
    }
    let mut clearance = 0;
    let mut bottom_guard = 0;
    for line in &*lines {
        if let Some(metrics) = line.grid {
            let depth = lift(line);
            let marker_below = metrics.marker_ink.map_or(0, |ink| ink.1);
            let marker_bound = marker_below
                + if metrics.marker_ink.is_some() && !line.quote_bars.is_empty() {
                    outward(QUOTE_BG_PAD_V)
                } else {
                    0
                };
            clearance = clearance.max((metrics.text_below.max(marker_below) - depth).max(0));
            bottom_guard = bottom_guard.max((metrics.ink_below.max(marker_bound) - depth).max(0));
        }
    }
    quantize(
        lines,
        i64::from(grid.spacing_milli_points()),
        clearance,
        bottom_guard,
    );
}

/// A paragraph alternative has already been measured in its real fonts. Its
/// final gap, guard and clearance must match the finalized surrounding body.
pub(super) fn finalize_alternative(lines: &mut [Line], baseline: &Line) {
    if let Some(grid) = baseline.grid {
        // Quotes attach their bars/background after paragraph shaping. The
        // candidate's glyphs are already measured, but its newly transferred
        // decoration needs the same bounds as the finalized baseline shape.
        for line in &mut *lines {
            if !line.quote_bars.is_empty()
                && let Some(metrics) = &mut line.grid
            {
                metrics.ink_above =
                    metrics.text_above.max(outward(line.size * 0.85)) + outward(QUOTE_BG_PAD_V);
                metrics.ink_below =
                    metrics.text_below.max(outward(line.size * 0.20)) + outward(QUOTE_BG_PAD_V);
            }
        }
        quantize(lines, grid.pitch, grid.clearance, grid.bottom_guard);
    }
}

/// The original bottom and origin live in PageGrid, so every caller may safely
/// request this adjustment without compounding the reserved descender space.
pub(super) fn page_for_lines(mut page: PageGeom, lines: &[Line]) -> PageGeom {
    if let Some(grid) = page.grid {
        let guard = lines
            .first()
            .and_then(|line| line.grid)
            .map_or(0, |line| line.bottom_guard);
        let capacity = (grid.origin - grid.bottom - guard).max(0);
        let bottom = grid.origin - capacity / grid.pitch * grid.pitch;
        page.bottom = points(bottom);
    }
    page
}

fn height_milli<'a>(lines: impl Iterator<Item = &'a Line>) -> i64 {
    lines.fold(0_i64, |height, line| {
        let row = line.grid.map_or_else(
            || outward(natural_line_leading(line) + line.gap_after),
            |grid| grid.leading + grid.gap,
        );
        height.saturating_add(row)
    })
}

pub(super) fn height<'a>(lines: impl Iterator<Item = &'a Line>) -> f32 {
    points(height_milli(lines))
}

pub(super) fn place<'a>(
    lines: impl Iterator<Item = &'a Line>,
    top: f32,
    placed: &mut Vec<Placed<'a>>,
) {
    place_at(lines, milli(top), placed);
}

/// Sum note rows before conversion to points. Repeated f32 additions on a
/// long fractional-pitch note can otherwise shift its entire region off-grid.
pub(super) fn place_notes<'a>(
    lines: impl Iterator<Item = &'a Line> + Clone,
    bottom: f32,
    placed: &mut Vec<Placed<'a>>,
) {
    let top = milli(bottom) + height_milli(lines.clone());
    place_at(lines, top, placed);
}

fn place_at<'a>(
    lines: impl Iterator<Item = &'a Line>,
    mut cursor: i64,
    placed: &mut Vec<Placed<'a>>,
) {
    for line in lines {
        if let Some(grid) = line.grid {
            placed.push(Placed {
                line,
                y: points(cursor - grid.advance),
            });
            cursor -= grid.leading + grid.gap;
        } else {
            // Keep the row observable to validation; never discard content if
            // a future layout path forgets to install its grid metrics.
            cursor -= outward(natural_line_leading(line));
            placed.push(Placed {
                line,
                y: points(cursor + lift(line)),
            });
            cursor -= outward(line.gap_after);
        }
    }
}

/// Reject an impossible grid before emitting any PDF objects. Validate the
/// actual pager output, including repeated headers and separately placed notes,
/// rather than assuming the optimal planner accepted every structural request.
pub(super) fn validate_layout(
    lines: &[Line],
    page: PageGeom,
    plan: Option<&PageBreakPlan>,
) -> Result<()> {
    let Some(grid) = page.grid else { return Ok(()) };
    let effective = page_for_lines(page, lines);
    if effective.top_y() - effective.bottom < MIN_CONTENT_DIM {
        return Err(invalid(
            "grid pitch and ink clearance leave too little printable body height",
        ));
    }
    let notes = PageNotes::scan(lines);
    let mut start = 0;
    let mut emitted = false;
    while let Some(placed) =
        next_placed_page(lines, &mut start, page, &mut emitted, plan, notes.as_ref())
    {
        let mut previous_bottom = f32::INFINITY;
        for placed in placed {
            let Some(metrics) = placed.line.grid else {
                return Err(invalid("a body row has no admitted grid metrics"));
            };
            let mut top = placed.y + points(metrics.ink_above);
            let mut bottom = placed.y - points(metrics.ink_below);
            let mut text_top = placed.y + points(metrics.text_above);
            let mut text_bottom = placed.y - points(metrics.text_below);
            if let Some((above, below)) = metrics.marker_ink {
                let (size, marker_y) = line_text_position(placed.line, placed.y);
                text_top = text_top.max(marker_y + points(above));
                text_bottom = text_bottom.min(marker_y - points(below));
                let (above, below) = if placed.line.quote_bars.is_empty() {
                    (points(above), points(below))
                } else {
                    (
                        points(above).max(size * 0.85) + QUOTE_BG_PAD_V,
                        points(below).max(size * 0.20) + QUOTE_BG_PAD_V,
                    )
                };
                top = top.max(marker_y + above);
                bottom = bottom.min(marker_y - below);
            }
            if !top.is_finite()
                || !bottom.is_finite()
                || top > points(grid.origin) + 0.01
                || bottom < points(grid.bottom) - 0.01
            {
                return Err(invalid(
                    "content cannot fit on the configured grid without leaving the body margins",
                ));
            }
            if metrics.text_above > 0 || metrics.text_below > 0 || metrics.marker_ink.is_some() {
                if text_top > previous_bottom + 0.01 {
                    return Err(invalid(
                        "content cannot fit on the configured grid without overlapping rows",
                    ));
                }
                previous_bottom = text_bottom;
            }
        }
    }
    Ok(())
}
