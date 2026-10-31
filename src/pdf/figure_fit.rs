//! Final containing-page fit for block figures, diagrams and display formulas.
//!
//! A figure's initial dimensions precede quote decoration, captions, note
//! spacing and grid quantization. An oversized row fits against the completed
//! geometry and the pager's keep policy, so its page-height cap includes those
//! costs rather than only raw artwork. Figures that already fit on their own
//! page retain their preferred dimensions and ordinary pagination behavior.

use super::{
    Faces, Line, PageGeom, baseline_grid, joint_pagination, line_leading, pdf_height_block_end,
};

pub(super) fn apply(lines: &mut [Line], page: PageGeom, faces: &Faces) {
    if !lines.iter().any(|line| line.image.is_some()) {
        return;
    }
    let page = baseline_grid::page_for_lines(page, lines);
    let capacity = page.top_y() - page.bottom;
    let mut kept_height = 0.0;
    let mut start = 0;
    while start < lines.len() {
        let end = pdf_height_block_end(lines, start);
        if lines[start].page_break_before
            || (start > 0 && lines[start - 1].flow.note != lines[start].flow.note)
        {
            kept_height = 0.0;
        }
        if end == start + 1
            && lines[start].image.is_some()
            && line_leading(&lines[start]) + lines[start].gap_after > capacity
        {
            let available = capacity - kept_height;
            if !fit(&mut lines[start], available, faces) {
                // An impossible preceding keep chain remains the planner's
                // explicit infeasibility. Still keep the figure itself inside
                // a page on the ordinary guaranteed-progress path.
                fit(&mut lines[start], capacity, faces);
            }
        }
        if joint_pagination::block_policy(lines, start).keep_with_next {
            kept_height += lines[start..end]
                .iter()
                .map(|line| line_leading(line) + line.gap_after)
                .sum::<f32>();
        } else {
            kept_height = 0.0;
        }
        start = end;
    }
}

/// Find the largest uniform scale whose complete row fits. A fixed number of
/// iterations bounds work even where grid rounding makes height discontinuous.
/// Only geometry changes: asset bytes, paths, alt/source text and link identity
/// remain intact. Ordinary figures that already fit are untouched.
fn fit(line: &mut Line, available: f32, faces: &Faces) -> bool {
    if line_leading(line) + line.gap_after <= available {
        return true;
    }
    let Some(image) = &line.image else {
        return false;
    };
    let (width, height, size, gap, grid, x) = (
        image.width_pt,
        image.height_pt,
        line.size,
        line.gap_after,
        line.grid,
        line.rule_x,
    );
    let mut resize = |scale: f32| {
        if let Some(image) = &mut line.image {
            image.width_pt = width * scale;
            image.height_pt = height * scale;
            if image.formula {
                line.rule_x = x + (width - image.width_pt) * 0.5;
            }
            line.size = (image.height_pt / 1.32).max(image.marker_size.unwrap_or(1.0));
        }
        if let Some(grid) = grid {
            line.gap_after = grid.natural_gap();
            baseline_grid::remeasure_figure(line, faces, grid);
        }
        line_leading(line) + line.gap_after
    };
    // Do not erase artwork to manufacture a feasible result. If even its
    // smallest nonzero box cannot fit beside a label/keep chain, retain the
    // original and let the strict/grid validator report the actual conflict.
    let mut low = 1.0 / 1_048_576.0;
    if resize(low) > available {
        if let Some(image) = &mut line.image {
            image.width_pt = width;
            image.height_pt = height;
        }
        line.size = size;
        line.gap_after = gap;
        line.grid = grid;
        line.rule_x = x;
        return false;
    }
    let mut high = 1.0;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        // Leave one milli-point for non-grid fixed-point conversion. Grid
        // rows are already integral pitches; subtracting an epsilon there
        // would unnecessarily discard an entire otherwise fitting pitch.
        if resize(mid) <= available - if grid.is_some() { 0.0 } else { 0.001 } {
            low = mid;
        } else {
            high = mid;
        }
    }
    resize(low);
    true
}
