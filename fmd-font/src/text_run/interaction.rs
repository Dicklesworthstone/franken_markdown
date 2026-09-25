//! Queries share the same atomic source-cluster edges as hit testing.

use super::{
    CaretAffinity, CaretPosition, Direction, HitTestResult, OwnedTextRun, Range,
    SelectionRect, TextCluster,
};

fn edge(run: &OwnedTextRun, cluster: &TextCluster, affinity: CaretAffinity) -> Option<CaretPosition> {
    if cluster.byte_range.start >= cluster.byte_range.end
        || run.logical_text.get(cluster.byte_range.clone()).is_none()
        || cluster.utf16_range.start >= cluster.utf16_range.end
        || !cluster.x_start.is_finite() || !cluster.x_end.is_finite()
    {
        return None;
    }
    let left = cluster.x_start.min(cluster.x_end);
    let right = cluster.x_start.max(cluster.x_end);
    let rtl = run.context.direction == Direction::RightToLeft;
    let leading = affinity == CaretAffinity::Leading;
    Some(CaretPosition {
        byte_offset: if leading { cluster.byte_range.start } else { cluster.byte_range.end },
        utf16_offset: if leading { cluster.utf16_range.start } else { cluster.utf16_range.end },
        visual_x: if leading == rtl { right } else { left },
        affinity,
    })
}

fn empty_caret(affinity: CaretAffinity) -> CaretPosition {
    CaretPosition { byte_offset: 0, utf16_offset: 0, visual_x: 0.0, affinity }
}

pub(super) fn caret_at_byte(
    run: &OwnedTextRun, byte: usize, affinity: CaretAffinity,
) -> Option<CaretPosition> {
    if !run.logical_text.is_char_boundary(byte) {
        return None;
    }
    if run.clusters.is_empty() {
        return (byte == 0 && run.logical_text.is_empty()).then(|| empty_caret(affinity));
    }
    // The inventory is logical, including for RTL. At a shared boundary,
    // affinity chooses the preceding trailing or following leading edge.
    let next = run.clusters.partition_point(|cluster| cluster.byte_range.start < byte);
    if affinity == CaretAffinity::Trailing && next > 0 {
        let previous = &run.clusters[next - 1];
        if previous.byte_range.end == byte {
            return edge(run, previous, CaretAffinity::Trailing);
        }
    }
    if let Some(following) = run.clusters.get(next) {
        if following.byte_range.start == byte {
            return edge(run, following, CaretAffinity::Leading);
        }
    }
    let previous = run.clusters.get(next.checked_sub(1)?)?;
    if previous.byte_range.end == byte {
        return edge(run, previous, CaretAffinity::Trailing);
    }
    if previous.byte_range.contains(&byte) {
        // No interpolation or invented interior caret: return the real edge's
        // byte AND UTF-16 offsets, not the requested interior source position.
        return edge(run, previous, affinity);
    }
    None
}

pub(super) fn hit_test(run: &OwnedTextRun, x: f32) -> HitTestResult {
    let fallback = || HitTestResult {
        cluster_index: 0,
        caret: run.clusters.first()
            .and_then(|cluster| edge(run, cluster, CaretAffinity::Leading))
            .unwrap_or_else(|| empty_caret(CaretAffinity::Leading)),
        is_exact: false,
    };
    if x.is_nan() || run.clusters.is_empty() {
        return fallback();
    }
    let rtl = run.context.direction == Direction::RightToLeft;
    let mut nearest: Option<HitTestResult> = None;
    let mut distance = f64::INFINITY;
    for (index, cluster) in run.clusters.iter().enumerate() {
        let (Some(leading), Some(trailing)) = (
            edge(run, cluster, CaretAffinity::Leading),
            edge(run, cluster, CaretAffinity::Trailing),
        ) else {
            return fallback();
        };
        let left = leading.visual_x.min(trailing.visual_x);
        let right = leading.visual_x.max(trailing.visual_x);
        if x >= left && x <= right {
            // f64 differences avoid midpoint overflow for finite f32 geometry.
            let nearer_right = f64::from(right) - f64::from(x)
                < f64::from(x) - f64::from(left);
            let tie = f64::from(right) - f64::from(x)
                == f64::from(x) - f64::from(left);
            let caret = if tie || nearer_right != rtl { trailing } else { leading };
            return HitTestResult {
                cluster_index: index,
                caret,
                is_exact: x > 0.0 && x < run.total_advance && right > left,
            };
        }
        // Gaps, exterior clicks and infinities resolve to an actual nearest
        // edge, never a fabricated end-of-run coordinate paired with another
        // cluster's source range. Ties keep the first logical cluster.
        for caret in [leading, trailing] {
            let candidate_distance = (f64::from(x) - f64::from(caret.visual_x)).abs();
            let closer = nearest.as_ref().is_none_or(|best| {
                if x == f32::INFINITY {
                    caret.visual_x > best.caret.visual_x
                } else if x == f32::NEG_INFINITY {
                    caret.visual_x < best.caret.visual_x
                } else {
                    candidate_distance < distance
                }
            });
            if closer {
                distance = candidate_distance;
                nearest = Some(HitTestResult { cluster_index: index, caret, is_exact: false });
            }
        }
    }
    nearest.unwrap_or_else(fallback)
}

pub(super) fn selection_rects(
    run: &OwnedTextRun, range: Range<usize>, y: f32, height: f32,
) -> Vec<SelectionRect> {
    if range.start >= range.end || run.logical_text.get(range.clone()).is_none()
        || !y.is_finite() || !height.is_finite() || height <= 0.0 || !(y + height).is_finite()
    {
        return Vec::new();
    }
    let mut spans = Vec::new();
    for cluster in &run.clusters {
        if cluster.byte_range.start < range.end && cluster.byte_range.end > range.start {
            let (Some(a), Some(b)) = (
                edge(run, cluster, CaretAffinity::Leading),
                edge(run, cluster, CaretAffinity::Trailing),
            ) else {
                return Vec::new();
            };
            let left = a.visual_x.min(b.visual_x);
            let right = a.visual_x.max(b.visual_x);
            if !(right - left).is_finite() {
                return Vec::new();
            }
            if right > left {
                spans.push((left, right));
            }
        }
    }
    // Source order is not visual order for RTL or host-provided discontiguous
    // selections. Union actual visual intervals; summing widths double-counts
    // overlaps, and merging only with a right neighbor misses RTL adjacency.
    spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (left, right) in spans {
        if let Some(previous) = merged.last_mut() {
            if left <= previous.1 {
                previous.1 = previous.1.max(right);
                continue;
            }
        }
        merged.push((left, right));
    }
    if merged.iter().any(|(left, right)| !(right - left).is_finite()) {
        return Vec::new();
    }
    merged.into_iter().map(|(left, right)| SelectionRect {
        x: left, y, width: right - left, height,
    }).collect()
}
