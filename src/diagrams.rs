//! Zero-dependency pure-Rust diagram compiler.
//!
//! Compiles Mermaid flowcharts, sequence diagrams, and ASCII art diagrams
//! into clean, standalone SVG vector graphics for HTML and PDF rendering.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

/// Supported diagram types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Flowchart,
    Sequence,
    AsciiBox,
}

/// Detect if code block contains a recognized diagram DSL.
#[must_use]
pub fn is_diagram_code(code: &str, lang: &str) -> bool {
    let trimmed_lang = lang.trim().to_ascii_lowercase();
    if matches!(
        trimmed_lang.as_str(),
        "mermaid" | "flowchart" | "sequence" | "ditaa"
    ) {
        return true;
    }
    let trimmed = code.trim();
    let first_directive = trimmed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    first_directive.starts_with("graph ")
        || first_directive.starts_with("graph\n")
        || first_directive == "graph"
        || first_directive.starts_with("flowchart ")
        || first_directive.starts_with("flowchart\n")
        || first_directive == "flowchart"
        || first_directive.starts_with("sequenceDiagram")
        || (first_directive.starts_with("+---") && trimmed.contains('|'))
}

/// Parse and render a diagram to an SVG string.
/// Returns `Some(svg)` if successfully parsed, or `None` to fallback to code block.
#[must_use]
pub fn render_diagram_svg(code: &str, lang: &str) -> Option<String> {
    let trimmed = code.trim();
    let lower_lang = lang.trim().to_ascii_lowercase();
    let first_directive = trimmed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");

    if lower_lang == "sequence" || first_directive.starts_with("sequenceDiagram") {
        render_sequence_diagram(trimmed)
    } else if lower_lang == "ditaa"
        || (first_directive.starts_with("+---") && trimmed.contains('|'))
    {
        render_ascii_diagram(trimmed)
    } else if lower_lang == "flowchart"
        || lower_lang == "mermaid"
        || first_directive.starts_with("graph ")
        || first_directive == "graph"
        || first_directive.starts_with("flowchart ")
        || first_directive == "flowchart"
    {
        render_flowchart(trimmed)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Fast exact fixed-point writer for SVG coordinate floats
// ---------------------------------------------------------------------------

/// Formats an SVG coordinate/dimension with exactly the bytes
/// `format!("{:.1}", v)` produces (one fractional digit, trailing `.0`
/// kept), bypassing the exact-decimal ("dragon") float formatter that
/// `{:.N}` otherwise invokes per emitted number.
struct SvgNum(f32);

impl fmt::Display for SvgNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_svg_fixed(f, self.0, 1)
    }
}

/// Formats an SVG size with exactly the bytes `format!("{:.0}", v)`
/// produces (no fractional digits, no decimal point).
struct SvgInt(f32);

impl fmt::Display for SvgInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_svg_fixed(f, self.0, 0)
    }
}

/// Byte-identical replacement for `format!("{:.frac_digits}", v)` on f32,
/// derived from the value's exact binary expansion instead of the flt2dec
/// exact-decimal ("dragon") machinery.
///
/// Rust's `{:.N}` expands the f32's exact value `m * 2^e` into its
/// terminating decimal digits and rounds that expansion at the Nth
/// fractional digit with round-half-to-even ties (probe: `{:.2}` of
/// 0.125f32 -> "0.12", `{:.2}` of 2.675f32 -> "2.67" because 2.675f32 is
/// exactly 2.6749999523…). For `e < 0` the value is `m * 5^k / 10^k` with
/// `k = -e`, so `value * 10^N = m * 5^N / 2^(k-N)` exactly; the quotient and
/// the tie test are therefore an integer shift and a bitmask compare. For
/// `e >= 0` the value is the integer `m << e`. Negatives keep their sign
/// even when the rounded magnitude is zero ("-0.0" for -0.04), matching std.
///
/// Domain: every finite f32 with `0.05 <= |v| <= 4e9` (0.05 is the f32
/// nearest 0.05; every f32 below it rounds to zero at <= 1 fractional digit
/// and no exact 0.05 exists in binary, so the boundary is never a tie).
/// Every f32 below 0.05 prints its zero form directly; NaN, infinities and
/// larger magnitudes fall back to the std formatter, so the emitted bytes
/// match std for every f32. Locked by differential tests against `format!`
/// (dyadic sweeps including every tie shape, exhaustive small integers, a
/// per-exponent mantissa sweep through subnormals, and random bit patterns).
fn write_svg_fixed(out: &mut impl fmt::Write, v: f32, frac_digits: u32) -> fmt::Result {
    if !v.is_finite() || v.abs() > 4.0e9 {
        // Unreachable from diagram geometry (layout sums stay far below),
        // but keep exact std bytes if it ever happens.
        return if frac_digits == 0 {
            write!(out, "{v:.0}")
        } else {
            write!(out, "{v:.1}")
        };
    }
    if v.abs() < 0.05 {
        let zero = match (v.is_sign_negative(), frac_digits) {
            (true, 0) => "-0",
            (true, _) => "-0.0",
            (false, 0) => "0",
            (false, _) => "0.0",
        };
        return out.write_str(zero);
    }
    let bits = v.to_bits();
    // |v| >= 0.05 excludes every subnormal, so the implicit bit is always set.
    let m = ((bits & 0x007f_ffff) | 0x0080_0000) as u64; // [2^23, 2^24)
    let e2 = ((bits >> 23) & 0xff) as i32 - 127 - 23; // value = m * 2^e2
    let pow5: u64 = if frac_digits == 0 { 1 } else { 5 };
    let scale: u64 = if frac_digits == 0 { 1 } else { 10 };
    let q: u64 = if e2 >= 0 {
        (m << e2) * scale // integral value; no digits to round away
    } else {
        let t = (-e2) as u32 - frac_digits; // >= 1 - frac_digits
        if t == 0 {
            m * pow5 // exactly frac_digits fractional bits; nothing to round
        } else {
            let n = m * pow5; // = value * 2^t * 10^frac_digits, exactly
            let q0 = n >> t;
            let rem = n & ((1u64 << t) - 1);
            let half = 1u64 << (t - 1);
            q0 + u64::from(rem > half || (rem == half && (q0 & 1) == 1))
        }
    };
    // q = round_half_even(|v| * 10^frac_digits); emit sign + digits + point.
    let fd = frac_digits as usize;
    let mut rev = [0u8; 12]; // |v| <= 4e9 -> q <= 4e10 -> at most 11 digits
    let mut i = 0usize;
    let mut n = q;
    while n > 0 {
        rev[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    if i == 0 {
        rev[0] = b'0'; // |v| in [0.05, 0.5) at {:.0} rounds to "0"/"-0"
        i = 1;
    }
    let mut b = [0u8; 15]; // '-' + 11 digits + '.' + 1 fractional digit
    let mut len = 0usize;
    if v.is_sign_negative() {
        b[0] = b'-';
        len = 1;
    }
    if i > fd {
        for p in (fd..i).rev() {
            b[len] = rev[p];
            len += 1;
        }
    } else {
        b[len] = b'0';
        len += 1;
    }
    if fd > 0 {
        b[len] = b'.';
        len += 1;
        for p in (0..fd).rev() {
            b[len] = if p < i { rev[p] } else { b'0' };
            len += 1;
        }
    }
    if let Ok(s) = std::str::from_utf8(&b[..len]) {
        out.write_str(s)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mermaid Flowchart Compiler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowDirection {
    TopToBottom,
    LeftToRight,
    BottomToTop,
    RightToLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Rectangle,
    Rounded,
    Stadium,
    Diamond,
    Circle,
}

#[derive(Debug, Clone)]
struct FlowNode {
    label: String,
    shape: NodeShape,
    layer: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone)]
struct FlowEdge {
    from: String,
    to: String,
    label: Option<String>,
    dashed: bool,
}

fn render_flowchart(src: &str) -> Option<String> {
    let mut dir = FlowDirection::TopToBottom;
    let mut nodes: BTreeMap<String, FlowNode> = BTreeMap::new();
    let mut node_order: Vec<String> = Vec::new();
    let mut edges: Vec<FlowEdge> = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if line == "graph"
            || line == "flowchart"
            || line.starts_with("graph ")
            || line.starts_with("flowchart ")
        {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                dir = match parts[1].to_ascii_uppercase().as_str() {
                    "LR" => FlowDirection::LeftToRight,
                    "RL" => FlowDirection::RightToLeft,
                    "BT" => FlowDirection::BottomToTop,
                    _ => FlowDirection::TopToBottom,
                };
            }
            continue;
        }

        // Parse edge line, e.g. "A[Start] -->|label| B{Choice}" or "A --> B"
        parse_flowchart_line(line, &mut nodes, &mut node_order, &mut edges);
    }

    if nodes.is_empty() {
        return None;
    }

    // Assign layers via topological sort / BFS
    assign_flowchart_layers(&mut nodes, &node_order, &edges);

    // Compute geometry
    let (total_width, total_height) = layout_flowchart_nodes(&mut nodes, &node_order, dir);

    // Generate SVG
    let mut svg = String::with_capacity(4096);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-flowchart\">\n",
        SvgInt(total_width),
        SvgInt(total_height),
        SvgInt(total_width),
        SvgInt(total_height)
    ));

    // Defs: Arrowhead markers and styles
    svg.push_str("<defs>\n");
    svg.push_str(r#"<marker id="fmd-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
<path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--fg-muted, #57606a)" />
</marker>
<style>
.fmd-diagram { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif; font-size: 13px; line-height: 1.4; }
.fmd-node-rect { fill: var(--bg-canvas, #ffffff); stroke: var(--border-default, #d0d7de); stroke-width: 1.5; rx: 6px; }
.fmd-node-rounded { fill: var(--bg-canvas, #ffffff); stroke: var(--border-default, #d0d7de); stroke-width: 1.5; rx: 16px; }
.fmd-node-diamond { fill: var(--bg-canvas, #ffffff); stroke: var(--border-default, #d0d7de); stroke-width: 1.5; }
.fmd-node-text { fill: var(--fg-default, #1f2328); text-anchor: middle; dominant-baseline: central; font-weight: 500; }
.fmd-edge-path { fill: none; stroke: var(--fg-muted, #57606a); stroke-width: 1.5; marker-end: url(#fmd-arrow); }
.fmd-edge-label-bg { fill: var(--bg-canvas, #ffffff); opacity: 0.9; rx: 3px; }
.fmd-edge-label-text { fill: var(--fg-muted, #57606a); font-size: 11px; text-anchor: middle; dominant-baseline: central; }
@media (prefers-color-scheme: dark) {
  .fmd-node-rect, .fmd-node-rounded, .fmd-node-diamond { fill: #161b22; stroke: #30363d; }
  .fmd-node-text { fill: #e6edf3; }
  .fmd-edge-path { stroke: #8b949e; }
  .fmd-edge-label-bg { fill: #0d1117; }
  .fmd-edge-label-text { fill: #8b949e; }
  #fmd-arrow path { fill: #8b949e; }
}
</style>
"#);
    svg.push_str("</defs>\n");

    // Draw Edges
    for edge in &edges {
        if let (Some(from_node), Some(to_node)) = (nodes.get(&edge.from), nodes.get(&edge.to)) {
            let (x1, y1, x2, y2) = get_edge_endpoints(from_node, to_node, dir);
            let dash_attr = if edge.dashed {
                " stroke-dasharray=\"4,4\""
            } else {
                ""
            };

            if (x1 - x2).abs() < 2.0 || (y1 - y2).abs() < 2.0 {
                svg.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-edge-path\"{dash_attr} />\n",
                    SvgNum(x1),
                    SvgNum(y1),
                    SvgNum(x2),
                    SvgNum(y2)
                ));
            } else {
                let mid_x = (x1 + x2) / 2.0;
                let mid_y = (y1 + y2) / 2.0;
                let (cx1, cy1, cx2, cy2) = match dir {
                    FlowDirection::TopToBottom | FlowDirection::BottomToTop => {
                        (x1, mid_y, x2, mid_y)
                    }
                    FlowDirection::LeftToRight | FlowDirection::RightToLeft => {
                        (mid_x, y1, mid_x, y2)
                    }
                };
                svg.push_str(&format!(
                    "<path d=\"M {} {} C {} {}, {} {}, {} {}\" class=\"fmd-edge-path\"{dash_attr} />\n",
                    SvgNum(x1),
                    SvgNum(y1),
                    SvgNum(cx1),
                    SvgNum(cy1),
                    SvgNum(cx2),
                    SvgNum(cy2),
                    SvgNum(x2),
                    SvgNum(y2)
                ));
            }

            // Edge label
            if let Some(lbl) = &edge.label {
                let mid_x = (x1 + x2) / 2.0;
                let mid_y = (y1 + y2) / 2.0;
                let lbl_w = (lbl.len() as f32 * 7.0 + 8.0).max(24.0);
                svg.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"16\" class=\"fmd-edge-label-bg\" />\n",
                    SvgNum(mid_x - lbl_w / 2.0),
                    SvgNum(mid_y - 8.0),
                    SvgNum(lbl_w)
                ));
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" class=\"fmd-edge-label-text\">{}</text>\n",
                    SvgNum(mid_x),
                    SvgNum(mid_y),
                    escape_svg_text(lbl)
                ));
            }
        }
    }

    // Draw Nodes
    for id in &node_order {
        if let Some(node) = nodes.get(id) {
            let cx = node.x + node.width / 2.0;
            let cy = node.y + node.height / 2.0;

            match node.shape {
                NodeShape::Rectangle => {
                    svg.push_str(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-node-rect\" />\n",
                        SvgNum(node.x),
                        SvgNum(node.y),
                        SvgNum(node.width),
                        SvgNum(node.height)
                    ));
                }
                NodeShape::Rounded | NodeShape::Stadium => {
                    svg.push_str(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-node-rounded\" />\n",
                        SvgNum(node.x),
                        SvgNum(node.y),
                        SvgNum(node.width),
                        SvgNum(node.height)
                    ));
                }
                NodeShape::Diamond => {
                    let pts = format!(
                        "{},{} {},{} {},{} {},{}",
                        SvgNum(cx),
                        SvgNum(node.y),
                        SvgNum(node.x + node.width),
                        SvgNum(cy),
                        SvgNum(cx),
                        SvgNum(node.y + node.height),
                        SvgNum(node.x),
                        SvgNum(cy)
                    );
                    svg.push_str(&format!(
                        "<polygon points=\"{}\" class=\"fmd-node-diamond\" />\n",
                        pts
                    ));
                }
                NodeShape::Circle => {
                    let r = (node.width.min(node.height) / 2.0).max(18.0);
                    svg.push_str(&format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" class=\"fmd-node-rounded\" />\n",
                        SvgNum(cx),
                        SvgNum(cy),
                        SvgNum(r)
                    ));
                }
            }

            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" class=\"fmd-node-text\">{}</text>\n",
                SvgNum(cx),
                SvgNum(cy),
                escape_svg_text(&node.label)
            ));
        }
    }

    svg.push_str("</svg>\n");
    Some(svg)
}

fn parse_flowchart_line(
    line: &str,
    nodes: &mut BTreeMap<String, FlowNode>,
    node_order: &mut Vec<String>,
    edges: &mut Vec<FlowEdge>,
) {
    // Check if line contains an arrow delimiter: -->, ---, -.->, ==>, etc.
    let arrow_patterns = ["-->|", "-->", "-.->|", "-.->", "==>|", "==>", "---|", "---"];

    for pat in arrow_patterns {
        if let Some(pos) = line.find(pat) {
            let left_part = &line[..pos];
            let right_part = &line[pos + pat.len()..];

            let mut left_trimmed = left_part.trim();
            let mut edge_label = None;
            if let Some((node_part, lbl_part)) = left_trimmed.split_once("--") {
                let lbl = lbl_part.trim();
                if !lbl.is_empty() {
                    edge_label = Some(lbl.to_string());
                    left_trimmed = node_part.trim();
                }
            }

            let (from_id, _) = parse_or_insert_node(left_trimmed, nodes, node_order);

            let (right_edge_label, mut dest_str) = if pat.ends_with('|') {
                if let Some(pipe_pos) = right_part.find('|') {
                    (
                        Some(right_part[..pipe_pos].trim().to_string()),
                        right_part[pipe_pos + 1..].trim(),
                    )
                } else {
                    (None, right_part.trim())
                }
            } else {
                (None, right_part.trim())
            };

            if right_edge_label.is_some() {
                edge_label = right_edge_label;
            } else if edge_label.is_none() && dest_str.starts_with('|') {
                if let Some(second_pipe) = dest_str[1..].find('|') {
                    let pipe_pos = 1 + second_pipe;
                    edge_label = Some(dest_str[1..pipe_pos].trim().to_string());
                    dest_str = dest_str[pipe_pos + 1..].trim();
                }
            }

            let (to_id, _) = parse_or_insert_node(dest_str, nodes, node_order);
            let dashed = pat.contains("-.-");

            edges.push(FlowEdge {
                from: from_id,
                to: to_id,
                label: edge_label,
                dashed,
            });
            return;
        }
    }

    // Standalone node declaration: A[Label]
    parse_or_insert_node(line, nodes, node_order);
}

fn parse_or_insert_node(
    s: &str,
    nodes: &mut BTreeMap<String, FlowNode>,
    node_order: &mut Vec<String>,
) -> (String, bool) {
    let s = s.trim();
    if s.is_empty() {
        return (String::new(), false);
    }

    let mut id = s.to_string();
    let mut label = s.to_string();
    let mut shape = NodeShape::Rectangle;

    if let Some(open_idx) = s.find(['[', '(', '{']) {
        id = s[..open_idx].trim().to_string();
        let rest = &s[open_idx..];
        if rest.starts_with("([") && rest.ends_with("])") {
            shape = NodeShape::Stadium;
            label = rest[2..rest.len() - 2].to_string();
        } else if rest.starts_with("((") && rest.ends_with("))") {
            shape = NodeShape::Circle;
            label = rest[2..rest.len() - 2].to_string();
        } else if rest.starts_with('[') && rest.ends_with(']') {
            shape = NodeShape::Rectangle;
            label = rest[1..rest.len() - 1].to_string();
        } else if rest.starts_with('(') && rest.ends_with(')') {
            shape = NodeShape::Rounded;
            label = rest[1..rest.len() - 1].to_string();
        } else if rest.starts_with('{') && rest.ends_with('}') {
            shape = NodeShape::Diamond;
            label = rest[1..rest.len() - 1].to_string();
        }
    }

    if label.starts_with('"') && label.ends_with('"') && label.len() >= 2 {
        label = label[1..label.len() - 1].to_string();
    }

    let is_new = !nodes.contains_key(&id);
    if is_new {
        let width = (label.len() as f32 * 8.5 + 32.0).max(80.0);
        let height = if shape == NodeShape::Diamond {
            54.0
        } else {
            38.0
        };
        nodes.insert(
            id.clone(),
            FlowNode {
                label,
                shape,
                layer: 0,
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
        );
        node_order.push(id.clone());
    } else if let Some(node) = nodes.get_mut(&id) {
        if node.label == id && label != id {
            node.width = (label.len() as f32 * 8.5 + 32.0).max(80.0);
            node.height = if shape == NodeShape::Diamond {
                54.0
            } else {
                38.0
            };
            node.label = label;
            node.shape = shape;
        }
    }
    (id, is_new)
}

fn assign_flowchart_layers(
    nodes: &mut BTreeMap<String, FlowNode>,
    node_order: &[String],
    edges: &[FlowEdge],
) {
    let mut in_degrees: BTreeMap<String, usize> = BTreeMap::new();
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for id in node_order {
        in_degrees.insert(id.clone(), 0);
        adj.insert(id.clone(), Vec::new());
    }

    for edge in edges {
        if nodes.contains_key(&edge.from) && nodes.contains_key(&edge.to) {
            *in_degrees.entry(edge.to.clone()).or_insert(0) += 1;
            adj.entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }
    }

    let mut queue = VecDeque::new();
    for id in node_order {
        if in_degrees.get(id).copied().unwrap_or(0) == 0 {
            queue.push_back((id.clone(), 0usize));
        }
    }

    let mut visited = BTreeSet::new();
    while let Some((id, layer)) = queue.pop_front() {
        if visited.contains(&id) {
            continue;
        }
        visited.insert(id.clone());

        if let Some(node) = nodes.get_mut(&id) {
            node.layer = layer;
        }

        if let Some(neighbors) = adj.get(&id) {
            for neighbor in neighbors {
                queue.push_back((neighbor.clone(), layer + 1));
            }
        }
    }

    // For any unvisited nodes (cycles/disconnected)
    let mut fallback_layer = 0;
    for id in node_order {
        if !visited.contains(id) {
            if let Some(node) = nodes.get_mut(id) {
                node.layer = fallback_layer;
                fallback_layer += 1;
            }
        }
    }
}

fn layout_flowchart_nodes(
    nodes: &mut BTreeMap<String, FlowNode>,
    node_order: &[String],
    dir: FlowDirection,
) -> (f32, f32) {
    // Group nodes by layer respecting node_order
    let mut layers: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for id in node_order {
        if let Some(node) = nodes.get(id) {
            layers.entry(node.layer).or_default().push(id.clone());
        }
    }

    let margin = 24.0;
    let node_gap = 28.0;
    let layer_gap = 56.0;

    let mut max_w: f32 = 0.0;
    let mut max_h: f32 = 0.0;

    match dir {
        FlowDirection::TopToBottom | FlowDirection::BottomToTop => {
            let mut curr_y = margin;
            for node_ids in layers.values() {
                let layer_max_h = node_ids
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|n| n.height)
                    .fold(0.0f32, f32::max);

                let total_layer_w: f32 = node_ids
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|n| n.width)
                    .sum::<f32>()
                    + (node_ids.len().saturating_sub(1) as f32) * node_gap;

                max_w = max_w.max(total_layer_w + margin * 2.0);

                let mut curr_x = margin;
                for id in node_ids {
                    if let Some(node) = nodes.get_mut(id) {
                        node.x = curr_x;
                        node.y = curr_y + (layer_max_h - node.height) / 2.0;
                        curr_x += node.width + node_gap;
                    }
                }
                curr_y += layer_max_h + layer_gap;
            }
            max_h = curr_y - layer_gap + margin;

            // Center layers horizontally
            for node_ids in layers.values() {
                let layer_w: f32 = node_ids
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|n| n.width)
                    .sum::<f32>()
                    + (node_ids.len().saturating_sub(1) as f32) * node_gap;
                let offset = (max_w - layer_w) / 2.0 - margin;
                if offset > 0.0 {
                    for id in node_ids {
                        if let Some(node) = nodes.get_mut(id) {
                            node.x += offset;
                        }
                    }
                }
            }
        }
        FlowDirection::LeftToRight | FlowDirection::RightToLeft => {
            let mut curr_x = margin;
            for node_ids in layers.values() {
                let layer_max_w = node_ids
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|n| n.width)
                    .fold(0.0f32, f32::max);

                let total_layer_h: f32 = node_ids
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|n| n.height)
                    .sum::<f32>()
                    + (node_ids.len().saturating_sub(1) as f32) * node_gap;

                max_h = max_h.max(total_layer_h + margin * 2.0);

                let mut curr_y = margin;
                for id in node_ids {
                    if let Some(node) = nodes.get_mut(id) {
                        node.x = curr_x + (layer_max_w - node.width) / 2.0;
                        node.y = curr_y;
                        curr_y += node.height + node_gap;
                    }
                }
                curr_x += layer_max_w + layer_gap;
            }
            max_w = curr_x - layer_gap + margin;

            // Center layers vertically
            for node_ids in layers.values() {
                let layer_h: f32 = node_ids
                    .iter()
                    .filter_map(|id| nodes.get(id))
                    .map(|n| n.height)
                    .sum::<f32>()
                    + (node_ids.len().saturating_sub(1) as f32) * node_gap;
                let offset = (max_h - layer_h) / 2.0 - margin;
                if offset > 0.0 {
                    for id in node_ids {
                        if let Some(node) = nodes.get_mut(id) {
                            node.y += offset;
                        }
                    }
                }
            }
        }
    }

    (max_w.max(160.0), max_h.max(80.0))
}

fn get_edge_endpoints(from: &FlowNode, to: &FlowNode, dir: FlowDirection) -> (f32, f32, f32, f32) {
    match dir {
        FlowDirection::TopToBottom => (
            from.x + from.width / 2.0,
            from.y + from.height,
            to.x + to.width / 2.0,
            to.y,
        ),
        FlowDirection::BottomToTop => (
            from.x + from.width / 2.0,
            from.y,
            to.x + to.width / 2.0,
            to.y + to.height,
        ),
        FlowDirection::LeftToRight => (
            from.x + from.width,
            from.y + from.height / 2.0,
            to.x,
            to.y + to.height / 2.0,
        ),
        FlowDirection::RightToLeft => (
            from.x,
            from.y + from.height / 2.0,
            to.x + to.width,
            to.y + to.height / 2.0,
        ),
    }
}

// ---------------------------------------------------------------------------
// Mermaid Sequence Diagram Compiler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SeqParticipant {
    label: String,
    x: f32,
}

#[derive(Debug, Clone)]
enum SeqEvent {
    Message {
        from: String,
        to: String,
        text: String,
        dashed: bool,
    },
    Note {
        participants: Vec<String>,
        text: String,
    },
}

fn ensure_participant(
    id: &str,
    label: Option<&str>,
    part_indices: &mut BTreeMap<String, usize>,
    participants: &mut Vec<SeqParticipant>,
) {
    if let std::collections::btree_map::Entry::Vacant(e) = part_indices.entry(id.to_string()) {
        e.insert(participants.len());
        participants.push(SeqParticipant {
            label: label.unwrap_or(id).to_string(),
            x: 0.0,
        });
    }
}

fn render_sequence_diagram(src: &str) -> Option<String> {
    let mut participants: Vec<SeqParticipant> = Vec::new();
    let mut part_indices: BTreeMap<String, usize> = BTreeMap::new();
    let mut events: Vec<SeqEvent> = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with("%%")
            || line.starts_with("sequenceDiagram")
            || line.starts_with("autonumber")
            || line.starts_with("activate ")
            || line.starts_with("deactivate ")
            || line.starts_with("title ")
            || line.starts_with("accTitle:")
            || line.starts_with("accDescr:")
            || line.starts_with("loop ")
            || line.starts_with("alt ")
            || line.starts_with("else ")
            || line.starts_with("opt ")
            || line == "end"
            || line.starts_with("rect ")
        {
            continue;
        }

        if line.starts_with("participant ") || line.starts_with("actor ") {
            let rest = line
                .strip_prefix("participant ")
                .or_else(|| line.strip_prefix("actor "))
                .unwrap_or("")
                .trim();
            let (id, label) = if let Some((id_part, label_part)) = rest.split_once(" as ") {
                (id_part.trim(), label_part.trim())
            } else {
                (rest, rest)
            };
            ensure_participant(id, Some(label), &mut part_indices, &mut participants);
            continue;
        }

        if let Some(note_body) = line.strip_prefix("Note ") {
            // Note over A, B: Text
            if let Some((target_part, text_part)) = note_body.split_once(':') {
                let text = text_part.trim().to_string();
                let targets = if let Some(over_part) = target_part.strip_prefix("over ") {
                    over_part.split(',').map(|s| s.trim().to_string()).collect()
                } else if let Some(left_part) = target_part.strip_prefix("left of ") {
                    vec![left_part.trim().to_string()]
                } else if let Some(right_part) = target_part.strip_prefix("right of ") {
                    vec![right_part.trim().to_string()]
                } else {
                    vec![]
                };
                for t in &targets {
                    ensure_participant(t, None, &mut part_indices, &mut participants);
                }
                events.push(SeqEvent::Note {
                    participants: targets,
                    text,
                });
            }
            continue;
        }

        // Messages: A-->>B: text, A-->B: text, A->>B: text, A->B: text
        let msg_delims = ["-->>", "-->", "->>", "->"];
        for delim in msg_delims {
            if let Some(delim_idx) = line.find(delim) {
                let from = line[..delim_idx].trim().to_string();
                let rest = &line[delim_idx + delim.len()..];
                if let Some((to_part, text_part)) = rest.split_once(':') {
                    let to = to_part.trim().to_string();
                    let text = text_part.trim().to_string();

                    for id in [&from, &to] {
                        ensure_participant(id, None, &mut part_indices, &mut participants);
                    }

                    let dashed = delim.contains("--");
                    events.push(SeqEvent::Message {
                        from,
                        to,
                        text,
                        dashed,
                    });
                    break;
                }
            }
        }
    }

    if participants.is_empty() {
        return None;
    }

    let margin = 28.0;
    let part_w = 110.0;
    let part_h = 36.0;
    let part_gap = 60.0;
    let event_gap = 42.0;

    let mut curr_x = margin;
    for part in &mut participants {
        part.x = curr_x + part_w / 2.0;
        curr_x += part_w + part_gap;
    }

    let total_width = curr_x - part_gap + margin;
    let header_y = margin + part_h;
    let total_events_h = events.len() as f32 * event_gap + 20.0;
    let bottom_header_y = header_y + total_events_h;
    let total_height = bottom_header_y + part_h + margin;

    let mut svg = String::with_capacity(4096);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-sequence\">\n",
        SvgInt(total_width),
        SvgInt(total_height),
        SvgInt(total_width),
        SvgInt(total_height)
    ));

    svg.push_str("<defs>\n");
    svg.push_str(r#"<marker id="fmd-seq-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
<path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--fg-default, #1f2328)" />
</marker>
<style>
.fmd-seq-lifeline { stroke: var(--border-default, #d0d7de); stroke-width: 1.5; stroke-dasharray: 4,4; }
.fmd-seq-box { fill: var(--bg-canvas, #ffffff); stroke: var(--border-default, #d0d7de); stroke-width: 1.5; rx: 4px; }
.fmd-seq-box-text { fill: var(--fg-default, #1f2328); font-size: 13px; font-weight: 600; text-anchor: middle; dominant-baseline: central; }
.fmd-seq-msg-line { stroke: var(--fg-default, #1f2328); stroke-width: 1.5; marker-end: url(#fmd-seq-arrow); }
.fmd-seq-msg-text { fill: var(--fg-default, #1f2328); font-size: 12px; text-anchor: middle; dominant-baseline: auto; }
.fmd-seq-note { fill: #fff8c5; stroke: #d4a72c; stroke-width: 1; rx: 3px; }
.fmd-seq-note-text { fill: #574600; font-size: 11px; text-anchor: middle; dominant-baseline: central; }
@media (prefers-color-scheme: dark) {
  .fmd-seq-lifeline { stroke: #30363d; }
  .fmd-seq-box { fill: #161b22; stroke: #30363d; }
  .fmd-seq-box-text { fill: #e6edf3; }
  .fmd-seq-msg-line { stroke: #e6edf3; }
  .fmd-seq-msg-text { fill: #e6edf3; }
  #fmd-seq-arrow path { fill: #e6edf3; }
  .fmd-seq-note { fill: #2e2600; stroke: #9e7a00; }
  .fmd-seq-note-text { fill: #e3b341; }
}
</style>
"#);
    svg.push_str("</defs>\n");

    // Lifelines
    for part in &participants {
        svg.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-seq-lifeline\" />\n",
            SvgNum(part.x),
            SvgNum(header_y),
            SvgNum(part.x),
            SvgNum(bottom_header_y)
        ));
    }

    // Events (Messages and Notes)
    let mut curr_y = header_y + 28.0;
    for event in &events {
        match event {
            SeqEvent::Message {
                from,
                to,
                text,
                dashed,
            } => {
                if let (Some(&from_idx), Some(&to_idx)) =
                    (part_indices.get(from), part_indices.get(to))
                {
                    let x1 = participants[from_idx].x;
                    let x2 = participants[to_idx].x;
                    let dash_attr = if *dashed {
                        " stroke-dasharray=\"4,4\""
                    } else {
                        ""
                    };

                    svg.push_str(&format!(
                        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-seq-msg-line\"{dash_attr} />\n",
                        SvgNum(x1),
                        SvgNum(curr_y),
                        SvgNum(x2),
                        SvgNum(curr_y)
                    ));

                    let mid_x = (x1 + x2) / 2.0;
                    svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" class=\"fmd-seq-msg-text\">{}</text>\n",
                        SvgNum(mid_x),
                        SvgNum(curr_y - 6.0),
                        escape_svg_text(text)
                    ));
                }
                curr_y += event_gap;
            }
            SeqEvent::Note {
                participants: targets,
                text,
            } => {
                let (min_x, max_x) = if targets.is_empty() {
                    (margin, margin + 120.0)
                } else {
                    let xs: Vec<f32> = targets
                        .iter()
                        .filter_map(|t| part_indices.get(t))
                        .map(|&idx| participants[idx].x)
                        .collect();
                    let min = xs.iter().copied().fold(f32::INFINITY, f32::min);
                    let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    if (max - min).abs() < 1.0 {
                        (min - 50.0, min + 50.0)
                    } else {
                        (min - 30.0, max + 30.0)
                    }
                };

                let note_w = (max_x - min_x).max(80.0);
                let note_h = 24.0;
                svg.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-seq-note\" />\n",
                    SvgNum(min_x),
                    SvgNum(curr_y - note_h / 2.0),
                    SvgNum(note_w),
                    SvgNum(note_h)
                ));
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" class=\"fmd-seq-note-text\">{}</text>\n",
                    SvgNum(min_x + note_w / 2.0),
                    SvgNum(curr_y),
                    escape_svg_text(text)
                ));
                curr_y += event_gap;
            }
        }
    }

    // Top & Bottom Participant Boxes
    for part in &participants {
        let bx = part.x - part_w / 2.0;
        // Top
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-seq-box\" />\n",
            SvgNum(bx),
            SvgNum(margin),
            SvgNum(part_w),
            SvgNum(part_h)
        ));
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" class=\"fmd-seq-box-text\">{}</text>\n",
            SvgNum(part.x),
            SvgNum(margin + part_h / 2.0),
            escape_svg_text(&part.label)
        ));

        // Bottom
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-seq-box\" />\n",
            SvgNum(bx),
            SvgNum(bottom_header_y),
            SvgNum(part_w),
            SvgNum(part_h)
        ));
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" class=\"fmd-seq-box-text\">{}</text>\n",
            SvgNum(part.x),
            SvgNum(bottom_header_y + part_h / 2.0),
            escape_svg_text(&part.label)
        ));
    }

    svg.push_str("</svg>\n");
    Some(svg)
}

// ---------------------------------------------------------------------------
// ASCII Art / Box-Drawing Compiler
// ---------------------------------------------------------------------------

fn render_ascii_diagram(src: &str) -> Option<String> {
    let lines: Vec<&str> = src.lines().collect();
    if lines.is_empty() {
        return None;
    }

    let char_w = 8.5f32;
    let char_h = 16.0f32;
    let max_cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1);
    let total_width = (max_cols as f32 * char_w + 32.0).max(120.0);
    let total_height = (lines.len() as f32 * char_h + 32.0).max(60.0);

    let mut svg = String::with_capacity(2048);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-ascii\">\n",
        SvgInt(total_width),
        SvgInt(total_height),
        SvgInt(total_width),
        SvgInt(total_height)
    ));
    svg.push_str("<style>\n.fmd-ascii-text { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 13px; fill: var(--fg-default, #1f2328); dominant-baseline: hanging; white-space: pre; }\n@media (prefers-color-scheme: dark) { .fmd-ascii-text { fill: #e6edf3; } }\n</style>\n");

    let mut y = 16.0;
    for line in lines {
        svg.push_str(&format!(
            "<text x=\"16\" y=\"{}\" class=\"fmd-ascii-text\">{}</text>\n",
            SvgNum(y),
            escape_svg_text(line)
        ));
        y += char_h;
    }
    svg.push_str("</svg>\n");
    Some(svg)
}

fn push_escaped_svg_text(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut clean_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&apos;",
            _ => continue,
        };
        if clean_start < i {
            out.push_str(&s[clean_start..i]);
        }
        out.push_str(esc);
        clean_start = i + 1;
    }
    if clean_start < s.len() {
        out.push_str(&s[clean_start..]);
    }
}

fn escape_svg_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    push_escaped_svg_text(s, &mut out);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn render_flowchart_produces_valid_svg() {
        let code = r#"
graph TD
    A[Start Node] --> B{Is Active?}
    B -->|Yes| C[Process]
    B -->|No| D[Finish]
    C --> D
"#;
        let svg = render_diagram_svg(code, "mermaid").expect("should render flowchart");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("fmd-flowchart"));
        assert!(svg.contains("Start Node"));
        assert!(svg.contains("Is Active?"));
        assert!(svg.contains("Process"));
        assert!(svg.contains("fmd-node-diamond"));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn render_sequence_produces_valid_svg() {
        let code = r#"
sequenceDiagram
    Client->>Server: GET /status
    Server-->>Client: 200 OK
    Note over Client,Server: Completed roundtrip
"#;
        let svg = render_diagram_svg(code, "mermaid").expect("should render sequence");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("fmd-sequence"));
        assert!(svg.contains("Client"));
        assert!(svg.contains("Server"));
        assert!(!svg.contains("Server-"));
        assert!(svg.contains("stroke-dasharray=\"4,4\""));
        assert!(svg.contains("GET /status"));
        assert!(svg.contains("200 OK"));
        assert!(svg.contains("Completed roundtrip"));
    }

    #[test]
    fn render_ascii_box_produces_valid_svg() {
        let code = r#"
+----------+     +----------+
| Producer | --> | Consumer |
+----------+     +----------+
"#;
        let svg = render_diagram_svg(code, "ditaa").expect("should render ascii box");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("fmd-ascii"));
        assert!(svg.contains("Producer"));
    }

    fn svg_fixed(v: f32, frac_digits: u32) -> String {
        let mut s = String::new();
        write_svg_fixed(&mut s, v, frac_digits).expect("String fmt writes are infallible");
        s
    }

    fn assert_matches_std(v: f32) {
        assert_eq!(
            svg_fixed(v, 1),
            format!("{v:.1}"),
            "mismatch at {v:e} ({{:.1}})"
        );
        assert_eq!(
            svg_fixed(v, 0),
            format!("{v:.0}"),
            "mismatch at {v:e} ({{:.0}})"
        );
    }

    #[test]
    fn svg_fixed_matches_std_on_dyadic_grid_with_ties() {
        // Every j/2^k for k <= 8, j < 2^12, both signs: covers the reachable
        // diagram-geometry domain (multiples of 0.125 from layout sums, .5
        // label widths, .25 centering offsets) densely, and every exact-tie
        // shape (j odd at k >= 3 is a tie at both precisions).
        for k in 0..=8u32 {
            let denom = (1u64 << k) as f32;
            for j in 0..(1u64 << 12) {
                let v = j as f32 / denom;
                assert_matches_std(v);
                assert_matches_std(-v);
            }
        }
    }

    #[test]
    fn svg_fixed_matches_std_on_integers_and_display_wrapper() {
        // Exhaustive small integers plus a sparse sweep to the domain cap,
        // and the Display wrappers end-to-end through format! machinery.
        for j in 0..=30_000i64 {
            let v = j as f32;
            assert_matches_std(v);
            assert_matches_std(-v);
        }
        let mut j = 30_001i64;
        while j <= 4_000_000_000 {
            let v = j as f32;
            assert_matches_std(v);
            assert_matches_std(-v);
            j += 99_933;
        }
        for v in [
            0.049_999_996,
            0.05,
            0.06,
            0.25,
            0.5,
            0.75,
            1.5,
            2.65,
            69.75,
            91.5,
        ] {
            assert_eq!(format!("{}", SvgNum(v)), format!("{v:.1}"), "SvgNum({v:e})");
            assert_eq!(format!("{}", SvgInt(v)), format!("{v:.0}"), "SvgInt({v:e})");
        }
    }

    #[test]
    fn svg_fixed_matches_std_across_all_exponent_classes() {
        // Every biased exponent (subnormals through inf-class fallbacks) with
        // pseudo-random mantissas: hits the exact-tail band [0.05, 4e9], the
        // subnormal zero forms, huge magnitudes, and the fallback branch.
        let mut x = 0x2545_F491u32;
        for exp in 0..=255u32 {
            for _ in 0..256 {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                let v = f32::from_bits((exp << 23) | (x & 0x007f_ffff));
                assert_matches_std(v);
                let v = f32::from_bits((exp << 23) | (x & 0x007f_ffff) | 0x8000_0000);
                assert_matches_std(v);
            }
        }
    }

    #[test]
    fn svg_fixed_matches_std_on_random_bit_patterns() {
        let mut x = 0x9E37_79B9u32;
        for _ in 0..60_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            assert_matches_std(f32::from_bits(x));
        }
    }

    #[test]
    fn svg_fixed_locks_std_tie_and_zero_semantics() {
        // Values pinned from std probes (see write_svg_fixed docs).
        assert_eq!(svg_fixed(0.25, 1), "0.2"); // exact tie -> even
        assert_eq!(svg_fixed(0.75, 1), "0.8"); // exact tie -> even
        assert_eq!(svg_fixed(12.5, 0), "12"); // tie -> even
        assert_eq!(svg_fixed(13.5, 0), "14"); // tie -> even
        assert_eq!(svg_fixed(0.5, 0), "0");
        assert_eq!(svg_fixed(1.5, 0), "2");
        assert_eq!(svg_fixed(-0.0, 1), "-0.0");
        assert_eq!(svg_fixed(-0.04, 0), "-0"); // sign kept when rounding to zero
        assert_eq!(svg_fixed(-0.04, 1), "-0.0");
        assert_eq!(svg_fixed(0.05, 1), "0.1"); // f32 0.05 is above 0.05
        assert_eq!(svg_fixed(0.049_999_996, 1), "0.0");
        assert_eq!(svg_fixed(f32::INFINITY, 1), "inf"); // fallback branch
        assert_eq!(svg_fixed(-f32::NAN, 0), "NaN");
    }

    #[test]
    fn flowchart_view_box_uses_half_even_ties() {
        // Same-layer nodes with widths 91.5 (7-char label: 8.5*7+32) and 83
        // (6-char label) give total_width = 202.5 + 48 = 250.5 exactly;
        // {:.0} half-even keeps "250" (a naive half-up writer would emit 251).
        let code = "graph TD\n    A[abcd123]\n    B[abcde1]";
        let svg = render_diagram_svg(code, "mermaid").expect("should render flowchart");
        assert!(
            svg.starts_with(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 250 86\" width=\"250\" height=\"86\""
            ),
            "unexpected header: {}",
            svg.lines().next().unwrap_or_default()
        );
    }

    #[test]
    fn render_sequence_with_leading_comments_and_keywords() {
        let code = r#"
%% Header comment
sequenceDiagram
    autonumber
    title Authentication Flow
    Alice->>Bob: Login request
    activate Bob
    Bob-->>Alice: Login response
    deactivate Bob
"#;
        let svg =
            render_diagram_svg(code, "mermaid").expect("should render sequence with comments");
        assert!(svg.contains("fmd-sequence"));
        assert!(svg.contains("Alice"));
        assert!(svg.contains("Bob"));
        assert!(svg.contains("Login request"));
        assert!(svg.contains("Login response"));
    }

    #[test]
    fn render_flowchart_with_quoted_labels_and_arrow_text() {
        let code = r#"
%% Flowchart with special labels
flowchart LR
    A["Initial (State)"] -- Next --> B["Done [100%]"]
"#;
        let svg = render_diagram_svg(code, "mermaid").expect("should render flowchart");
        assert!(svg.contains("fmd-flowchart"));
        assert!(svg.contains("Initial (State)"));
        assert!(svg.contains("Done [100%]"));
        assert!(svg.contains("Next"));
        // Quotes should be stripped
        assert!(!svg.contains("&quot;Initial (State)&quot;"));
    }

    #[test]
    fn is_diagram_code_handles_comments_and_bare_keywords() {
        assert!(is_diagram_code("%% comment\ngraph\n  A --> B", ""));
        assert!(is_diagram_code(
            "%% comment\nsequenceDiagram\n  A->>B: Hi",
            ""
        ));
        assert!(is_diagram_code("flowchart\n  A --> B", ""));
    }
}
