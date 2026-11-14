//! Zero-dependency pure-Rust diagram compiler.
//!
//! Compiles Mermaid flowcharts, state diagrams, class diagrams, sequence diagrams,
//! pie charts, and ASCII art into standalone SVG for the shared rendering backends.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

mod class;
mod flowchart;
mod pie;
mod sequence;
mod state;

/// Supported diagram types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Flowchart,
    Sequence,
    AsciiBox,
}

/// Detect if code block contains a recognized diagram DSL.
#[inline(always)]
#[must_use]
pub fn is_diagram_code(code: &str, lang: &str) -> bool {
    let trimmed_lang = lang.trim().to_ascii_lowercase();
    if matches!(
        trimmed_lang.as_str(),
        "mermaid" | "flowchart" | "state" | "class" | "sequence" | "pie" | "ditaa"
    ) {
        return true;
    }
    let trimmed = code.trim();
    let first_directive = trimmed
        .split(['\n', '\r'])
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    names_flowchart(first_directive)
        || state::is_header(first_directive)
        || class::is_header(first_directive)
        || sequence::is_header(first_directive)
        || pie::is_header(first_directive)
        || (first_directive.starts_with("+---") && trimmed.contains('|'))
}

/// Parse and render a diagram to an SVG string.
/// Returns `Some(svg)` if successfully parsed, or `None` to fallback to code block.
#[must_use]
pub fn render_diagram_svg(code: &str, lang: &str) -> Option<String> {
    let trimmed = code.trim();
    let lower_lang = lang.trim().to_ascii_lowercase();
    let first_directive = trimmed
        .split(['\n', '\r'])
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");

    if lower_lang == "state" {
        // An explicit state fence owns its grammar, including the required
        // header. Other diagram headers must not bypass that contract.
        state::render(trimmed)
    } else if lower_lang == "class" {
        class::render(trimmed)
    } else if lower_lang == "sequence" || sequence::is_header(first_directive) {
        sequence::render(trimmed)
    } else if lower_lang == "ditaa"
        || (first_directive.starts_with("+---") && trimmed.contains('|'))
    {
        render_ascii_diagram(trimmed)
    } else if lower_lang == "flowchart" {
        render_flowchart(trimmed)
    } else if lower_lang == "pie" || pie::is_header(first_directive) {
        pie::render(trimmed)
    } else if state::is_header(first_directive) {
        state::render(trimmed)
    } else if class::is_header(first_directive) {
        class::render(trimmed)
    } else if lower_lang == "mermaid" && !names_flowchart(first_directive) {
        // gantt, erDiagram and other Mermaid
        // types this compiler does not draw stay readable source code instead
        // of being misread as flowchart nodes.
        None
    } else if lower_lang == "mermaid" || names_flowchart(first_directive) {
        render_flowchart(trimmed)
    } else {
        None
    }
}

/// A supported Mermaid diagram as an SVG figure, with alt text,
/// for paged and poster outputs. ASCII box art returns `None`: those outputs
/// keep it as a fitted monospace panel that preserves its row geometry.
#[must_use]
pub fn figure_svg(code: &str, lang: &str) -> Option<(String, &'static str)> {
    if !is_diagram_code(code, lang) {
        return None;
    }
    let svg = render_diagram_svg(code, lang)?;
    let alt = if svg.contains("class=\"fmd-diagram fmd-pie\"") {
        "Pie chart"
    } else if svg.contains("class=\"fmd-diagram fmd-sequence\"") {
        "Sequence diagram"
    } else if svg.contains("class=\"fmd-diagram fmd-state\"") {
        "State diagram"
    } else if svg.contains("class=\"fmd-diagram fmd-class\"") {
        "Class diagram"
    } else if svg.contains("class=\"fmd-diagram fmd-flowchart\"") {
        "Diagram"
    } else {
        return None;
    };
    Some((svg, alt))
}

/// A Mermaid flowchart header: `graph`/`flowchart`, optionally with a direction.
fn names_flowchart(directive: &str) -> bool {
    matches!(
        directive
            .split(|c: char| c.is_whitespace() || c == ';')
            .next(),
        Some("graph" | "flowchart")
    )
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
    StateStart,
    StateEnd,
    StateChoice,
    StateBar,
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
    thick: bool,
    arrow: bool,
}

fn render_flowchart(src: &str) -> Option<String> {
    Some(render_flow_graph(flowchart::parse(src)?, false))
}

fn render_flow_graph(graph: flowchart::Graph, state: bool) -> String {
    let flowchart::Graph {
        dir,
        mut nodes,
        order: node_order,
        edges,
    } = graph;

    // Keep every dependency in an acyclic graph ahead of its destination.
    assign_flowchart_layers(&mut nodes, &node_order, &edges);

    // Compute geometry
    let (mut total_width, mut total_height) =
        layout_flowchart_nodes(&mut nodes, &node_order, &edges, dir);
    let return_routes =
        layout_flowchart_returns(&mut nodes, &edges, dir, &mut total_width, &mut total_height);

    // Generate SVG
    let mut svg = String::with_capacity(4096);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram {}\">\n",
        SvgInt(total_width),
        SvgInt(total_height),
        SvgInt(total_width),
        SvgInt(total_height),
        if state { "fmd-state" } else { "fmd-flowchart" }
    ));
    if state {
        svg.push_str("<title>State diagram</title>\n");
    }

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
    if state {
        svg.push_str(
            "<style>\n.fmd-state-solid { fill: var(--fg-default, #1f2328); }\n\
             .fmd-state-end { fill: var(--bg-canvas, #ffffff); stroke: var(--fg-default, #1f2328); stroke-width: 1.5; }\n\
             @media (prefers-color-scheme: dark) {\n\
             .fmd-state-solid { fill: #e6edf3; }\n\
             .fmd-state-end { fill: #161b22; stroke: #e6edf3; }\n}\n</style>\n",
        );
    }
    svg.push_str("</defs>\n");

    // Draw Edges
    for (edge, return_route) in edges.iter().zip(&return_routes) {
        if let (Some(from_node), Some(to_node)) = (nodes.get(&edge.from), nodes.get(&edge.to)) {
            let (x1, y1, x2, y2) = get_edge_endpoints(from_node, to_node, dir);
            let edge_style = match (edge.arrow, edge.thick) {
                (true, false) => "",
                (false, false) => " style=\"marker-end:none\"",
                (true, true) => " style=\"stroke-width:3\"",
                (false, true) => " style=\"marker-end:none;stroke-width:3\"",
            };
            let dash_attr = if edge.dashed {
                " stroke-dasharray=\"4,4\""
            } else {
                ""
            };

            if let Some(route) = return_route {
                let [a, b, c, d, e, f] = route.points;
                svg.push_str(&format!(
                    "<path d=\"M {} {} L {} {} L {} {} L {} {} L {} {} L {} {}\" class=\"fmd-edge-path\"{dash_attr}{edge_style} />\n",
                    SvgNum(a.0), SvgNum(a.1),
                    SvgNum(b.0), SvgNum(b.1),
                    SvgNum(c.0), SvgNum(c.1),
                    SvgNum(d.0), SvgNum(d.1),
                    SvgNum(e.0), SvgNum(e.1),
                    SvgNum(f.0), SvgNum(f.1),
                ));
            } else if (x1 - x2).abs() < 2.0 || (y1 - y2).abs() < 2.0 {
                svg.push_str(&format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-edge-path\"{dash_attr}{edge_style} />\n",
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
                    "<path d=\"M {} {} C {} {}, {} {}, {} {}\" class=\"fmd-edge-path\"{dash_attr}{edge_style} />\n",
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
                let (mid_x, mid_y) = return_route
                    .as_ref()
                    .map_or(((x1 + x2) / 2.0, (y1 + y2) / 2.0), |route| route.label);
                let lbl_w = return_route
                    .as_ref()
                    .map_or_else(|| flowchart_label_width(lbl), |route| route.label_width);
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
                NodeShape::Diamond | NodeShape::StateChoice => {
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
                NodeShape::StateStart | NodeShape::StateEnd => {
                    if node.shape == NodeShape::StateEnd {
                        svg.push_str(&format!(
                            "<circle cx=\"{}\" cy=\"{}\" r=\"10\" class=\"fmd-state-end\" />\n",
                            SvgNum(cx),
                            SvgNum(cy)
                        ));
                    }
                    svg.push_str(&format!(
                        "<circle cx=\"{}\" cy=\"{}\" r=\"7\" class=\"fmd-state-solid\" />\n",
                        SvgNum(cx),
                        SvgNum(cy)
                    ));
                }
                NodeShape::StateBar => {
                    svg.push_str(&format!(
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-state-solid\" />\n",
                        SvgNum(node.x), SvgNum(node.y), SvgNum(node.width), SvgNum(node.height)
                    ));
                }
            }

            if !matches!(
                node.shape,
                NodeShape::StateStart
                    | NodeShape::StateEnd
                    | NodeShape::StateChoice
                    | NodeShape::StateBar
            ) {
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" class=\"fmd-node-text\">{}</text>\n",
                    SvgNum(cx),
                    SvgNum(cy),
                    escape_svg_text(&node.label)
                ));
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
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
            queue.push_back(id.clone());
        }
    }

    let mut visited = BTreeSet::new();
    while let Some(id) = queue.pop_front() {
        visited.insert(id.clone());
        let next_layer = nodes.get(&id).map_or(1, |node| node.layer + 1);
        if let Some(neighbors) = adj.get(&id) {
            for neighbor in neighbors {
                if let Some(node) = nodes.get_mut(neighbor) {
                    node.layer = node.layer.max(next_layer);
                }
                if let Some(remaining) = in_degrees.get_mut(neighbor) {
                    *remaining = remaining.saturating_sub(1);
                    if *remaining == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    // Cyclic components retain deterministic source-order placement after the
    // resolved part of the graph; they must not reuse a completed DAG layer.
    let mut fallback_layer = visited
        .iter()
        .filter_map(|id| nodes.get(id))
        .map(|node| node.layer + 1)
        .max()
        .unwrap_or(0);
    for id in node_order {
        if !visited.contains(id)
            && let Some(node) = nodes.get_mut(id)
        {
            node.layer = fallback_layer;
            fallback_layer += 1;
        }
    }
}

fn layout_flowchart_nodes(
    nodes: &mut BTreeMap<String, FlowNode>,
    node_order: &[String],
    edges: &[FlowEdge],
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
    // Adjacent horizontal transitions need enough room for their captions.
    // Long transitions use outside lanes and do not inflate every intervening
    // layer. Vertical captions fit in the normal band; their width contributes
    // to the final canvas envelope.
    let mut label_gaps = BTreeMap::<usize, f32>::new();
    for edge in edges {
        if let (Some(from), Some(to), Some(label)) =
            (nodes.get(&edge.from), nodes.get(&edge.to), &edge.label)
            && to.layer == from.layer + 1
        {
            let gap = label_gaps.entry(from.layer).or_insert(layer_gap);
            *gap = gap.max(flowchart_label_width(label) + 24.0);
        }
    }

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
            for (layer, node_ids) in &layers {
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
                curr_x += layer_max_w + label_gaps.get(layer).copied().unwrap_or(layer_gap);
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

    let width = max_w.max(160.0);
    let height = max_h.max(80.0);
    for node in nodes.values_mut() {
        match dir {
            FlowDirection::RightToLeft => node.x = width - node.x - node.width,
            FlowDirection::BottomToTop => node.y = height - node.y - node.height,
            _ => {}
        }
    }
    (width, height)
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

struct FlowReturnRoute {
    points: [(f32, f32); 6],
    label: (f32, f32),
    label_width: f32,
}

fn flowchart_label_width(label: &str) -> f32 {
    (label.len() as f32 * 11.0 + 8.0).max(24.0)
}

/// Leave the normal outgoing port, travel outside the entire graph, and
/// approach the normal incoming port. Reversing an ordinary edge's endpoints
/// would instead draw a self-loop or feedback connection through filled nodes.
/// Connections that skip layers also need outside lanes: their direct path
/// would otherwise cross unrelated nodes in the intervening layers.
fn layout_flowchart_returns(
    nodes: &mut BTreeMap<String, FlowNode>,
    edges: &[FlowEdge],
    dir: FlowDirection,
    width: &mut f32,
    height: &mut f32,
) -> Vec<Option<FlowReturnRoute>> {
    let horizontal = matches!(dir, FlowDirection::LeftToRight | FlowDirection::RightToLeft);
    let forward = matches!(dir, FlowDirection::TopToBottom | FlowDirection::LeftToRight);
    let mut layers: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    let mut outside: f32 = 0.0;
    for node in nodes.values() {
        let (start, end, cross_end) = if horizontal {
            (node.x, node.x + node.width, node.y + node.height)
        } else {
            (node.y, node.y + node.height, node.x + node.width)
        };
        let layer = layers.entry(node.layer).or_insert((start, end));
        layer.0 = layer.0.min(start);
        layer.1 = layer.1.max(end);
        outside = outside.max(cross_end);
    }

    // Distinct transitions must remain visible even when they share endpoints.
    // Keep a direct adjacent connection only if its caption clears every node
    // and every earlier direct caption. Conflicts receive separate outside
    // lanes. Reserve all direct captions before placing any outside lane, so a
    // wide label cannot reach into a lane allocated earlier in source order.
    let mut pairs = BTreeSet::new();
    let mut captions: Vec<[f32; 4]> = Vec::new();
    let mut direct = Vec::with_capacity(edges.len());
    for edge in edges {
        let (Some(from), Some(to)) = (nodes.get(&edge.from), nodes.get(&edge.to)) else {
            direct.push(false);
            continue;
        };
        let mut admitted =
            to.layer == from.layer + 1 && pairs.insert((edge.from.as_str(), edge.to.as_str()));
        if admitted && let Some(label) = &edge.label {
            let (x1, y1, x2, y2) = get_edge_endpoints(from, to, dir);
            let (x, y) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let half = flowchart_label_width(label) / 2.0;
            let bounds = [x - half, y - 8.0, x + half, y + 8.0];
            let overlaps = |other: [f32; 4]| {
                bounds[0] < other[2] + 4.0
                    && bounds[2] + 4.0 > other[0]
                    && bounds[1] < other[3] + 4.0
                    && bounds[3] + 4.0 > other[1]
            };
            admitted = !captions.iter().copied().any(overlaps)
                && !nodes.values().any(|node| {
                    overlaps([node.x, node.y, node.x + node.width, node.y + node.height])
                });
            if admitted {
                outside = outside.max(if horizontal { bounds[3] } else { bounds[2] });
                captions.push(bounds);
            }
        }
        direct.push(admitted);
    }

    let mut next_lane = outside + 16.0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (0.0f32, 0.0f32, *width, *height);
    let mut routes = Vec::with_capacity(edges.len());
    for (edge, direct) in edges.iter().zip(direct) {
        let (Some(from), Some(to)) = (nodes.get(&edge.from), nodes.get(&edge.to)) else {
            routes.push(None);
            continue;
        };
        let (x1, y1, x2, y2) = get_edge_endpoints(from, to, dir);
        let label_width = edge.label.as_deref().map_or(0.0, flowchart_label_width);
        if direct {
            if edge.label.is_some() {
                let (x, y) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                min_x = min_x.min(x - label_width / 2.0 - 12.0);
                min_y = min_y.min(y - 20.0);
                max_x = max_x.max(x + label_width / 2.0 + 12.0);
                max_y = max_y.max(y + 20.0);
            }
            routes.push(None);
            continue;
        }

        let from_layer = layers[&from.layer];
        let to_layer = layers[&to.layer];
        // The whole layer must be cleared before crossing sideways, including
        // taller circles or wider labels beside a smaller source/target node.
        let (depart, arrive) = if forward {
            (from_layer.1 + 12.0, to_layer.0 - 12.0)
        } else {
            (from_layer.0 - 12.0, to_layer.1 + 12.0)
        };
        let half_lane = if horizontal {
            8.0
        } else {
            (label_width / 2.0).max(8.0)
        };
        let rail = next_lane + half_lane;
        next_lane = rail + half_lane + 16.0;
        let (points, label) = if horizontal {
            (
                [
                    (x1, y1),
                    (depart, y1),
                    (depart, rail),
                    (arrive, rail),
                    (arrive, y2),
                    (x2, y2),
                ],
                ((depart + arrive) / 2.0, rail),
            )
        } else {
            (
                [
                    (x1, y1),
                    (x1, depart),
                    (rail, depart),
                    (rail, arrive),
                    (x2, arrive),
                    (x2, y2),
                ],
                (rail, (depart + arrive) / 2.0),
            )
        };
        for &(x, y) in &points {
            min_x = min_x.min(x - 12.0);
            min_y = min_y.min(y - 12.0);
            max_x = max_x.max(x + 12.0);
            max_y = max_y.max(y + 12.0);
        }
        if edge.label.is_some() {
            min_x = min_x.min(label.0 - label_width / 2.0 - 12.0);
            min_y = min_y.min(label.1 - 8.0 - 12.0);
            max_x = max_x.max(label.0 + label_width / 2.0 + 12.0);
            max_y = max_y.max(label.1 + 8.0 + 12.0);
        }
        routes.push(Some(FlowReturnRoute {
            points,
            label,
            label_width,
        }));
    }

    // Long labels on horizontal return lanes can reach past either end of
    // the original canvas. Translate the complete graph, keeping a zero-origin
    // viewBox and every port aligned, rather than clipping or truncating text.
    if min_x < 0.0 || min_y < 0.0 {
        for node in nodes.values_mut() {
            node.x -= min_x;
            node.y -= min_y;
        }
        for route in routes.iter_mut().flatten() {
            for point in &mut route.points {
                point.0 -= min_x;
                point.1 -= min_y;
            }
            route.label.0 -= min_x;
            route.label.1 -= min_y;
        }
    }
    *width = max_x - min_x;
    *height = max_y - min_y;
    routes
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
