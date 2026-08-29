//! Zero-dependency pure-Rust diagram compiler.
//!
//! Compiles Mermaid flowcharts, sequence diagrams, and ASCII art diagrams
//! into clean, standalone SVG vector graphics for HTML and PDF rendering.

use std::collections::{HashMap, HashSet, VecDeque};

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
    trimmed.starts_with("graph ")
        || trimmed.starts_with("flowchart ")
        || trimmed.starts_with("sequenceDiagram")
        || (trimmed.starts_with("+---") && trimmed.contains('|'))
}

/// Parse and render a diagram to an SVG string.
/// Returns `Some(svg)` if successfully parsed, or `None` to fallback to code block.
#[must_use]
pub fn render_diagram_svg(code: &str, lang: &str) -> Option<String> {
    let trimmed = code.trim();
    let lower_lang = lang.trim().to_ascii_lowercase();

    if lower_lang == "sequence" || trimmed.starts_with("sequenceDiagram") {
        render_sequence_diagram(trimmed)
    } else if lower_lang == "ditaa" || (trimmed.starts_with("+---") && trimmed.contains('|')) {
        render_ascii_diagram(trimmed)
    } else if lower_lang == "flowchart"
        || lower_lang == "mermaid"
        || trimmed.starts_with("graph ")
        || trimmed.starts_with("flowchart ")
    {
        render_flowchart(trimmed)
    } else {
        None
    }
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
    let mut nodes: HashMap<String, FlowNode> = HashMap::new();
    let mut node_order: Vec<String> = Vec::new();
    let mut edges: Vec<FlowEdge> = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        if line.starts_with("graph ") || line.starts_with("flowchart ") {
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
    let (total_width, total_height) = layout_flowchart_nodes(&mut nodes, dir);

    // Generate SVG
    let mut svg = String::with_capacity(4096);
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {:.0} {:.0}\" width=\"{:.0}\" height=\"{:.0}\" class=\"fmd-diagram fmd-flowchart\">\n",
        total_width, total_height, total_width, total_height
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
                    "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"fmd-edge-path\"{dash_attr} />\n",
                    x1, y1, x2, y2
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
                    "<path d=\"M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}\" class=\"fmd-edge-path\"{dash_attr} />\n",
                    x1, y1, cx1, cy1, cx2, cy2, x2, y2
                ));
            }

            // Edge label
            if let Some(lbl) = &edge.label {
                let mid_x = (x1 + x2) / 2.0;
                let mid_y = (y1 + y2) / 2.0;
                let lbl_w = (lbl.len() as f32 * 7.0 + 8.0).max(24.0);
                svg.push_str(&format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"16\" class=\"fmd-edge-label-bg\" />\n",
                    mid_x - lbl_w / 2.0, mid_y - 8.0, lbl_w
                ));
                svg.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" class=\"fmd-edge-label-text\">{}</text>\n",
                    mid_x,
                    mid_y,
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
                        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" class=\"fmd-node-rect\" />\n",
                        node.x, node.y, node.width, node.height
                    ));
                }
                NodeShape::Rounded | NodeShape::Stadium => {
                    svg.push_str(&format!(
                        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" class=\"fmd-node-rounded\" />\n",
                        node.x, node.y, node.width, node.height
                    ));
                }
                NodeShape::Diamond => {
                    let pts = format!(
                        "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
                        cx,
                        node.y,
                        node.x + node.width,
                        cy,
                        cx,
                        node.y + node.height,
                        node.x,
                        cy
                    );
                    svg.push_str(&format!(
                        "<polygon points=\"{}\" class=\"fmd-node-diamond\" />\n",
                        pts
                    ));
                }
                NodeShape::Circle => {
                    let r = (node.width.min(node.height) / 2.0).max(18.0);
                    svg.push_str(&format!(
                        "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"{:.1}\" class=\"fmd-node-rounded\" />\n",
                        cx, cy, r
                    ));
                }
            }

            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" class=\"fmd-node-text\">{}</text>\n",
                cx,
                cy,
                escape_svg_text(&node.label)
            ));
        }
    }

    svg.push_str("</svg>\n");
    Some(svg)
}

fn parse_flowchart_line(
    line: &str,
    nodes: &mut HashMap<String, FlowNode>,
    node_order: &mut Vec<String>,
    edges: &mut Vec<FlowEdge>,
) {
    // Check if line contains an arrow delimiter: -->, ---, -.->, ==>, etc.
    let arrow_patterns = ["-->|", "-->", "-.->|", "-.->", "==>|", "==>", "---|", "---"];

    for pat in arrow_patterns {
        if let Some(pos) = line.find(pat) {
            let left_part = &line[..pos];
            let right_part = &line[pos + pat.len()..];

            let (from_id, _) = parse_or_insert_node(left_part.trim(), nodes, node_order);

            let (edge_label, dest_str) = if pat.ends_with('|') {
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
    nodes: &mut HashMap<String, FlowNode>,
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
    }
    (id, is_new)
}

fn assign_flowchart_layers(
    nodes: &mut HashMap<String, FlowNode>,
    node_order: &[String],
    edges: &[FlowEdge],
) {
    let mut in_degrees: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();

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
    for (id, deg) in &in_degrees {
        if *deg == 0 {
            queue.push_back((id.clone(), 0usize));
        }
    }

    let mut visited = HashSet::new();
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

fn layout_flowchart_nodes(nodes: &mut HashMap<String, FlowNode>, dir: FlowDirection) -> (f32, f32) {
    // Group nodes by layer
    let mut layers: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (id, node) in nodes.iter() {
        layers.entry(node.layer).or_default().push(id.clone());
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

use std::collections::BTreeMap;

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
    part_indices: &mut HashMap<String, usize>,
    participants: &mut Vec<SeqParticipant>,
) {
    if let std::collections::hash_map::Entry::Vacant(e) = part_indices.entry(id.to_string()) {
        e.insert(participants.len());
        participants.push(SeqParticipant {
            label: label.unwrap_or(id).to_string(),
            x: 0.0,
        });
    }
}

fn render_sequence_diagram(src: &str) -> Option<String> {
    let mut participants: Vec<SeqParticipant> = Vec::new();
    let mut part_indices: HashMap<String, usize> = HashMap::new();
    let mut events: Vec<SeqEvent> = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with("sequenceDiagram") {
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

        // Messages: A->>B: text, A-->>B: text, A->B: text
        let msg_delims = ["->>", "-->>", "->", "-->"];
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
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {:.0} {:.0}\" width=\"{:.0}\" height=\"{:.0}\" class=\"fmd-diagram fmd-sequence\">\n",
        total_width, total_height, total_width, total_height
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
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"fmd-seq-lifeline\" />\n",
            part.x, header_y, part.x, bottom_header_y
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
                        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"fmd-seq-msg-line\"{dash_attr} />\n",
                        x1, curr_y, x2, curr_y
                    ));

                    let mid_x = (x1 + x2) / 2.0;
                    svg.push_str(&format!(
                        "<text x=\"{:.1}\" y=\"{:.1}\" class=\"fmd-seq-msg-text\">{}</text>\n",
                        mid_x,
                        curr_y - 6.0,
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
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" class=\"fmd-seq-note\" />\n",
                    min_x, curr_y - note_h / 2.0, note_w, note_h
                ));
                svg.push_str(&format!(
                    "<text x=\"{:.1}\" y=\"{:.1}\" class=\"fmd-seq-note-text\">{}</text>\n",
                    min_x + note_w / 2.0,
                    curr_y,
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
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" class=\"fmd-seq-box\" />\n",
            bx, margin, part_w, part_h
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" class=\"fmd-seq-box-text\">{}</text>\n",
            part.x,
            margin + part_h / 2.0,
            escape_svg_text(&part.label)
        ));

        // Bottom
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" class=\"fmd-seq-box\" />\n",
            bx, bottom_header_y, part_w, part_h
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" class=\"fmd-seq-box-text\">{}</text>\n",
            part.x,
            bottom_header_y + part_h / 2.0,
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
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {:.0} {:.0}\" width=\"{:.0}\" height=\"{:.0}\" class=\"fmd-diagram fmd-ascii\">\n",
        total_width, total_height, total_width, total_height
    ));
    svg.push_str("<style>\n.fmd-ascii-text { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 13px; fill: var(--fg-default, #1f2328); dominant-baseline: hanging; white-space: pre; }\n@media (prefers-color-scheme: dark) { .fmd-ascii-text { fill: #e6edf3; } }\n</style>\n");

    let mut y = 16.0;
    for line in lines {
        svg.push_str(&format!(
            "<text x=\"16\" y=\"{:.1}\" class=\"fmd-ascii-text\">{}</text>\n",
            y,
            escape_svg_text(line)
        ));
        y += char_h;
    }
    svg.push_str("</svg>\n");
    Some(svg)
}

fn escape_svg_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
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
}
