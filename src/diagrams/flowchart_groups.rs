//! Compound flowchart layout, isolated from the established flat graph path.
//!
//! Each subgraph is packed as one indivisible child of its parent. Edges use
//! reserved layer gaps and side gutters while climbing to their endpoints'
//! common ancestor, so an unrelated node/group never becomes part of a box or
//! an obstacle hidden under a connector. All work is bounded by admission in
//! `flowchart`; geometry additionally has a finite canvas limit.

use super::flowchart::{ParsedGraph, Subgraph};
use super::{
    FlowDirection, FlowEdge, FlowNode, NodeShape, SvgNum, assign_flowchart_layers, escape_svg_text,
    flowchart_label_width,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

const PADDING: f32 = 20.0;
const TITLE_HEIGHT: f32 = 32.0;
// Bold Computer Modern's W is wider than one em. Reserve 1.25 em per
// UTF-8 byte at the 13px title size; this also bounds the browser sans title.
const TITLE_BYTE_WIDTH: f32 = 16.25;
const NODE_GAP: f32 = 28.0;
const SLOT_STEP: f32 = 6.0;
const MAX_DIMENSION: f32 = 1_000_000.0;
const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Child {
    Node(usize),
    Group(usize),
}

impl Child {
    fn key(self) -> String {
        match self {
            Self::Node(index) => format!("node{index}"),
            Self::Group(index) => format!("group{index}"),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Rect {
    fn primary_size(self, horizontal: bool) -> f32 {
        if horizontal { self.width } else { self.height }
    }

    fn cross_size(self, horizontal: bool) -> f32 {
        if horizontal { self.height } else { self.width }
    }

    fn primary_start(self, horizontal: bool) -> f32 {
        if horizontal { self.x } else { self.y }
    }

    fn port(self, horizontal: bool, end: bool) -> (f32, f32) {
        let primary = self.primary_start(horizontal)
            + if end {
                self.primary_size(horizontal)
            } else {
                0.0
            };
        let cross = if horizontal {
            self.y + self.height / 2.0
        } else {
            self.x + self.width / 2.0
        };
        point(horizontal, primary, cross)
    }
}

#[derive(Clone, Copy)]
struct Placement {
    child: Child,
    rect: Rect,
}

struct Slot {
    order: usize,
    // An unlabeled, nonparallel forward edge between adjacent children can
    // stay in their common gap. Every other edge has its own side gutter.
    rail: Option<f32>,
}

struct Container {
    bounds: Rect,
    placements: Vec<Placement>,
    child_layers: BTreeMap<Child, usize>,
    layers: BTreeMap<usize, (f32, f32)>,
    slots: BTreeMap<usize, Slot>,
    title: (f32, f32),
}

struct Scene {
    dir: FlowDirection,
    ids: Vec<String>,
    nodes: Vec<FlowNode>,
    edges: Vec<FlowEdge>,
    groups: Vec<Subgraph>,
    owners: Vec<usize>,
    parents: Vec<usize>,
    // For each container, the direct child containing a given node.
    children_of: Vec<Vec<Option<Child>>>,
    endpoints: Vec<(usize, usize)>,
    containers: Vec<Container>,
    node_bounds: Vec<Rect>,
}

struct Route {
    points: Vec<(f32, f32)>,
    label: Option<(f32, f32)>,
}

fn horizontal(dir: FlowDirection) -> bool {
    matches!(dir, FlowDirection::LeftToRight | FlowDirection::RightToLeft)
}

fn forward(dir: FlowDirection) -> bool {
    matches!(dir, FlowDirection::TopToBottom | FlowDirection::LeftToRight)
}

fn point(horizontal: bool, primary: f32, cross: f32) -> (f32, f32) {
    if horizontal {
        (primary, cross)
    } else {
        (cross, primary)
    }
}

fn cross(horizontal: bool, point: (f32, f32)) -> f32 {
    if horizontal { point.1 } else { point.0 }
}

fn append(points: &mut Vec<(f32, f32)>, next: (f32, f32)) {
    if points.last().copied() != Some(next) {
        points.push(next);
    }
}

impl Scene {
    fn layout(parsed: ParsedGraph) -> Option<Self> {
        let ParsedGraph {
            graph,
            subgraphs,
            owners,
        } = parsed;
        let ids = graph.order;
        let nodes = ids
            .iter()
            .map(|id| graph.nodes.get(id).cloned())
            .collect::<Option<Vec<_>>>()?;
        let indices: BTreeMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        let endpoints = graph
            .edges
            .iter()
            .map(|edge| {
                Some((
                    *indices.get(edge.from.as_str())?,
                    *indices.get(edge.to.as_str())?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let root = subgraphs.len();
        let owners: Vec<_> = ids
            .iter()
            .map(|id| owners.get(id).copied().unwrap_or(root))
            .collect();
        let parents: Vec<_> = subgraphs
            .iter()
            .map(|group| group.parent.unwrap_or(root))
            .collect();
        let mut children_of = vec![vec![None; nodes.len()]; root + 1];
        let mut children = vec![Vec::new(); root + 1];
        let mut first_node = vec![usize::MAX; root];
        for (node, &owner) in owners.iter().enumerate() {
            children[owner].push(Child::Node(node));
            let mut container = owner;
            let mut child = Child::Node(node);
            loop {
                children_of[container][node] = Some(child);
                if container == root {
                    break;
                }
                first_node[container] = first_node[container].min(node);
                child = Child::Group(container);
                container = parents[container];
            }
        }
        for (group, &parent) in parents.iter().enumerate() {
            children[parent].push(Child::Group(group));
        }
        for list in &mut children {
            list.sort_by_key(|child| match child {
                Child::Node(index) => *index,
                Child::Group(index) => first_node[*index],
            });
        }
        let mut layouts: Vec<Option<Container>> = (0..=root).map(|_| None).collect();
        // Parents precede children in the parser, so reverse order sizes each
        // nested box before using it as an indivisible layout child.
        for container in (0..root).rev().chain(std::iter::once(root)) {
            layouts[container] = Some(layout_container(
                graph.dir,
                &children[container],
                &children_of[container],
                &nodes,
                &graph.edges,
                &endpoints,
                &layouts,
                subgraphs.get(container),
            )?);
        }
        let mut containers = layouts.into_iter().collect::<Option<Vec<_>>>()?;
        let mut node_bounds = vec![Rect::default(); nodes.len()];
        let mut queue = VecDeque::from([(root, 0.0, 0.0)]);
        let is_horizontal = horizontal(graph.dir);
        while let Some((container, x, y)) = queue.pop_front() {
            let layout = &mut containers[container];
            layout.bounds.x = x;
            layout.bounds.y = y;
            layout.title.0 += x;
            layout.title.1 += y;
            let primary_offset = if is_horizontal { x } else { y };
            let cross_offset = if is_horizontal { y } else { x };
            for range in layout.layers.values_mut() {
                range.0 += primary_offset;
                range.1 += primary_offset;
            }
            for slot in layout.slots.values_mut() {
                if let Some(rail) = &mut slot.rail {
                    *rail += cross_offset;
                }
            }
            for placement in &layout.placements {
                let bounds = Rect {
                    x: placement.rect.x + x,
                    y: placement.rect.y + y,
                    ..placement.rect
                };
                match placement.child {
                    Child::Node(node) => node_bounds[node] = bounds,
                    Child::Group(group) => queue.push_back((group, bounds.x, bounds.y)),
                }
            }
        }
        Some(Self {
            dir: graph.dir,
            ids,
            nodes,
            edges: graph.edges,
            groups: subgraphs,
            owners,
            parents,
            children_of,
            endpoints,
            containers,
            node_bounds,
        })
    }

    fn gap(&self, container: usize, child: Child, edge: usize, outgoing: bool) -> Option<f32> {
        let layout = &self.containers[container];
        let layer = layout.layers.get(layout.child_layers.get(&child)?)?;
        let offset = 12.0 + layout.slots.get(&edge)?.order as f32 * SLOT_STEP;
        Some(if forward(self.dir) == outgoing {
            layer.1 + offset
        } else {
            layer.0 - offset
        })
    }

    fn branch(
        &self,
        node: usize,
        edge: usize,
        ancestor: usize,
        outgoing: bool,
    ) -> Option<(Vec<(f32, f32)>, Child)> {
        let is_horizontal = horizontal(self.dir);
        let end = forward(self.dir) == outgoing;
        let mut points = vec![self.node_bounds[node].port(is_horizontal, end)];
        let mut container = self.owners[node];
        let mut child = Child::Node(node);
        while container != ancestor {
            let layout = &self.containers[container];
            let rail = layout.slots.get(&edge)?.rail?;
            let gap = self.gap(container, child, edge, outgoing)?;
            let last = *points.last()?;
            append(
                &mut points,
                point(is_horizontal, gap, cross(is_horizontal, last)),
            );
            append(&mut points, point(is_horizontal, gap, rail));
            let boundary = layout.bounds.primary_start(is_horizontal)
                + if end {
                    layout.bounds.primary_size(is_horizontal)
                } else {
                    0.0
                };
            append(&mut points, point(is_horizontal, boundary, rail));
            child = Child::Group(container);
            container = self.parents[container];
        }
        Some((points, child))
    }

    fn routes(&self) -> Option<Vec<Route>> {
        let mut routes = Vec::with_capacity(self.edges.len());
        let is_horizontal = horizontal(self.dir);
        for (edge, &(from, to)) in self.endpoints.iter().enumerate() {
            let mut ancestor = self.owners[from];
            while self.children_of[ancestor][to].is_none() {
                ancestor = *self.parents.get(ancestor)?;
            }
            let (mut points, from_child) = self.branch(from, edge, ancestor, true)?;
            let (target, to_child) = self.branch(to, edge, ancestor, false)?;
            let from_cross = cross(is_horizontal, *points.last()?);
            let to_cross = cross(is_horizontal, *target.last()?);
            let depart = self.gap(ancestor, from_child, edge, true)?;
            let arrive = self.gap(ancestor, to_child, edge, false)?;
            append(&mut points, point(is_horizontal, depart, from_cross));
            let slot = self.containers[ancestor].slots.get(&edge)?;
            let label = if let Some(rail) = slot.rail {
                append(&mut points, point(is_horizontal, depart, rail));
                append(&mut points, point(is_horizontal, arrive, rail));
                append(&mut points, point(is_horizontal, arrive, to_cross));
                self.edges[edge]
                    .label
                    .as_ref()
                    .map(|_| point(is_horizontal, (depart + arrive) / 2.0, rail))
            } else {
                append(&mut points, point(is_horizontal, depart, to_cross));
                None
            };
            for target in target.into_iter().rev() {
                append(&mut points, target);
            }
            routes.push(Route { points, label });
        }
        Some(routes)
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_container(
    dir: FlowDirection,
    children: &[Child],
    children_of: &[Option<Child>],
    nodes: &[FlowNode],
    edges: &[FlowEdge],
    endpoints: &[(usize, usize)],
    layouts: &[Option<Container>],
    group: Option<&Subgraph>,
) -> Option<Container> {
    if children.is_empty() {
        return None;
    }
    let is_horizontal = horizontal(dir);
    let header = if group.is_some() { TITLE_HEIGHT } else { 0.0 };
    let title_width = group.map_or(0.0, |group| group.label.len() as f32 * TITLE_BYTE_WIDTH);
    let mut proxies = BTreeMap::new();
    let mut order = Vec::with_capacity(children.len());
    for &child in children {
        let (width, height) = match child {
            Child::Node(index) => (nodes[index].width, nodes[index].height),
            Child::Group(index) => {
                let bounds = layouts[index].as_ref()?.bounds;
                (bounds.width, bounds.height)
            }
        };
        let key = child.key();
        order.push(key.clone());
        proxies.insert(
            key,
            FlowNode {
                label: String::new(),
                shape: NodeShape::Rectangle,
                layer: 0,
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
        );
    }
    let mut relevant = Vec::new();
    let mut dependencies = Vec::new();
    for (edge, &(from, to)) in endpoints.iter().enumerate() {
        match (children_of[from], children_of[to]) {
            (Some(a), Some(b)) if a != b => {
                relevant.push(edge);
                dependencies.push(FlowEdge {
                    from: a.key(),
                    to: b.key(),
                    label: None,
                    dashed: false,
                    thick: false,
                    arrow: true,
                });
            }
            (Some(Child::Node(_)), Some(Child::Node(_))) | (Some(_), None) | (None, Some(_)) => {
                relevant.push(edge)
            }
            _ => {}
        }
    }
    assign_flowchart_layers(&mut proxies, &order, &dependencies);
    let child_layers: BTreeMap<_, _> = children
        .iter()
        .map(|&child| Some((child, proxies.get(&child.key())?.layer)))
        .collect::<Option<_>>()?;
    let mut layers: BTreeMap<usize, Vec<(Child, Rect)>> = BTreeMap::new();
    for &child in children {
        let node = proxies.get(&child.key())?;
        layers.entry(node.layer).or_default().push((
            child,
            Rect {
                width: node.width,
                height: node.height,
                ..Rect::default()
            },
        ));
    }
    let gap = 40.0 + relevant.len() as f32 * SLOT_STEP * 2.0;
    let max_label = relevant
        .iter()
        .filter(|&&edge| {
            let (from, to) = endpoints[edge];
            children_of[from].is_some() && children_of[to].is_some()
        })
        .filter_map(|&edge| edges[edge].label.as_deref())
        .map(flowchart_label_width)
        .fold(0.0, f32::max);
    let primary_padding = gap + if is_horizontal { max_label / 2.0 } else { 0.0 };
    let primary_content = layers
        .values()
        .map(|layer| {
            layer
                .iter()
                .map(|(_, rect)| rect.primary_size(is_horizontal))
                .fold(0.0, f32::max)
        })
        .sum::<f32>()
        + (layers.len().saturating_sub(1)) as f32 * gap;
    let mut primary_size = primary_content + 2.0 * primary_padding;
    if is_horizontal {
        primary_size = primary_size.max(title_width + 2.0 * PADDING);
    }
    let body_cross = layers
        .values()
        .map(|layer| {
            layer
                .iter()
                .map(|(_, rect)| rect.cross_size(is_horizontal))
                .sum::<f32>()
                + layer.len().saturating_sub(1) as f32 * NODE_GAP
        })
        .fold(0.0, f32::max)
        + 2.0 * PADDING;
    let body_cross = body_cross.max(if is_horizontal {
        80.0
    } else {
        (title_width + 2.0 * PADDING).max(160.0)
    });
    let mut placements = Vec::with_capacity(children.len());
    let mut layer_ranges = BTreeMap::new();
    let mut primary = (primary_size - primary_content) / 2.0;
    for (&layer, list) in &layers {
        let size = list
            .iter()
            .map(|(_, rect)| rect.primary_size(is_horizontal))
            .fold(0.0, f32::max);
        let cross_size = list
            .iter()
            .map(|(_, rect)| rect.cross_size(is_horizontal))
            .sum::<f32>()
            + list.len().saturating_sub(1) as f32 * NODE_GAP;
        let mut cross = (body_cross - cross_size) / 2.0;
        for &(child, mut rect) in list {
            let start = primary + (size - rect.primary_size(is_horizontal)) / 2.0;
            let start = if forward(dir) {
                start
            } else {
                primary_size - start - rect.primary_size(is_horizontal)
            };
            let (x, y) = point(is_horizontal, start, cross);
            rect.x = x;
            rect.y = y + header;
            let start = rect.primary_start(is_horizontal);
            let end = start + rect.primary_size(is_horizontal);
            let range = layer_ranges.entry(layer).or_insert((start, end));
            range.0 = range.0.min(start);
            range.1 = range.1.max(end);
            placements.push(Placement { child, rect });
            cross += rect.cross_size(is_horizontal) + NODE_GAP;
        }
        primary += size + gap;
    }
    let mut slots = BTreeMap::new();
    let mut pairs = BTreeSet::new();
    let mut next_rail = body_cross;
    for (order, edge) in relevant.into_iter().enumerate() {
        let (from, to) = endpoints[edge];
        let direct = match (children_of[from], children_of[to]) {
            (Some(a), Some(b)) if a != b => {
                child_layers[&b] == child_layers[&a] + 1
                    && edges[edge].label.is_none()
                    && pairs.insert((from, to))
            }
            _ => false,
        };
        let rail = if direct {
            None
        } else {
            let internal = children_of[from].is_some() && children_of[to].is_some();
            let half = if !is_horizontal && internal {
                edges[edge]
                    .label
                    .as_deref()
                    .map_or(8.0, |label| (flowchart_label_width(label) / 2.0).max(8.0))
            } else {
                8.0
            };
            let rail = next_rail + half;
            next_rail = rail + half + 16.0;
            Some(rail + if is_horizontal { header } else { 0.0 })
        };
        slots.insert(edge, Slot { order, rail });
    }
    let cross_size = next_rail + PADDING;
    let bounds = if is_horizontal {
        Rect {
            width: primary_size,
            height: header + cross_size,
            ..Rect::default()
        }
    } else {
        Rect {
            width: cross_size,
            height: header + primary_size,
            ..Rect::default()
        }
    };
    if !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width > MAX_DIMENSION
        || bounds.height > MAX_DIMENSION
    {
        return None;
    }
    Some(Container {
        bounds,
        placements,
        child_layers,
        layers: layer_ranges,
        slots,
        title: (
            if is_horizontal {
                primary_size / 2.0
            } else {
                body_cross / 2.0
            },
            header / 2.0,
        ),
    })
}

pub(super) fn render(parsed: ParsedGraph) -> Option<String> {
    let scene = Scene::layout(parsed)?;
    let routes = scene.routes()?;
    let bounds = scene.containers.last()?.bounds;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-flowchart\" role=\"img\">\n<title>Flowchart</title>\n",
        SvgNum(bounds.width),
        SvgNum(bounds.height),
        SvgNum(bounds.width),
        SvgNum(bounds.height)
    );
    svg.push_str(STYLE);
    for (index, group) in scene.groups.iter().enumerate() {
        let layout = &scene.containers[index];
        let rect = layout.bounds;
        // Keep shapes and text at the SVG root: the shared Flow inventory
        // deliberately accepts a closed primitive vocabulary, without groups.
        writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\" class=\"fmd-subgraph-box\" data-subgraph-id=\"{}\"/>",
            SvgNum(rect.x), SvgNum(rect.y), SvgNum(rect.width), SvgNum(rect.height), escape_svg_text(&group.id)).ok()?;
        writeln!(
            svg,
            "<text x=\"{}\" y=\"{}\" class=\"fmd-subgraph-title\">{}</text>",
            SvgNum(layout.title.0),
            SvgNum(layout.title.1),
            escape_svg_text(&group.label)
        )
        .ok()?;
    }
    for (edge, route) in scene.edges.iter().zip(&routes) {
        svg.push_str("<path d=\"");
        for (index, &(x, y)) in route.points.iter().enumerate() {
            write!(
                svg,
                "{} {} {} ",
                if index == 0 { 'M' } else { 'L' },
                SvgNum(x),
                SvgNum(y)
            )
            .ok()?;
        }
        svg.push_str("\" class=\"fmd-edge-path\"");
        if edge.dashed {
            svg.push_str(" stroke-dasharray=\"4,4\"");
        }
        match (edge.arrow, edge.thick) {
            (false, false) => svg.push_str(" style=\"marker-end:none\""),
            (true, true) => svg.push_str(" style=\"stroke-width:3\""),
            (false, true) => svg.push_str(" style=\"marker-end:none;stroke-width:3\""),
            _ => {}
        }
        svg.push_str("/>\n");
    }
    // Captions are painted after all strokes. Their dedicated gutters keep
    // captions apart, and the backing preserves text at connector crossings.
    for (edge, route) in scene.edges.iter().zip(&routes) {
        if let (Some(label), Some((x, y))) = (&edge.label, route.label) {
            let width = flowchart_label_width(label);
            writeln!(
                svg,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"16\" class=\"fmd-edge-label-bg\"/>",
                SvgNum(x - width / 2.0),
                SvgNum(y - 8.0),
                SvgNum(width)
            )
            .ok()?;
            writeln!(
                svg,
                "<text x=\"{}\" y=\"{}\" class=\"fmd-edge-label-text\">{}</text>",
                SvgNum(x),
                SvgNum(y),
                escape_svg_text(label)
            )
            .ok()?;
        }
    }
    for (index, (node, bounds)) in scene.nodes.iter().zip(&scene.node_bounds).enumerate() {
        draw_node(&mut svg, &scene.ids[index], node, *bounds)?;
    }
    svg.push_str("</svg>\n");
    (svg.len() <= MAX_SVG_BYTES).then_some(svg)
}

fn draw_node(svg: &mut String, id: &str, node: &FlowNode, rect: Rect) -> Option<()> {
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let id = escape_svg_text(id);
    match node.shape {
        NodeShape::Rectangle | NodeShape::Rounded | NodeShape::Stadium => {
            let radius = if node.shape == NodeShape::Rectangle {
                6.0
            } else {
                16.0
            };
            writeln!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" class=\"fmd-node-rect\" data-node-id=\"{id}\"/>",
                SvgNum(rect.x), SvgNum(rect.y), SvgNum(rect.width), SvgNum(rect.height), SvgNum(radius)).ok()?;
        }
        NodeShape::Circle => {
            writeln!(
                svg,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" class=\"fmd-node-rect\" data-node-id=\"{id}\"/>",
                SvgNum(cx),
                SvgNum(cy),
                SvgNum(rect.width / 2.0)
            )
            .ok()?;
        }
        NodeShape::Diamond => {
            writeln!(
                svg,
                "<polygon points=\"{},{} {},{} {},{} {},{}\" class=\"fmd-node-rect\" data-node-id=\"{id}\"/>",
                SvgNum(cx),
                SvgNum(rect.y),
                SvgNum(rect.x + rect.width),
                SvgNum(cy),
                SvgNum(cx),
                SvgNum(rect.y + rect.height),
                SvgNum(rect.x),
                SvgNum(cy)
            )
            .ok()?;
        }
        _ => return None,
    }
    writeln!(
        svg,
        "<text x=\"{}\" y=\"{}\" class=\"fmd-node-text\">{}</text>",
        SvgNum(cx),
        SvgNum(cy),
        escape_svg_text(&node.label)
    )
    .ok()?;
    Some(())
}

const STYLE: &str = r#"<defs>
<marker id="fmd-group-arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 1.5 L 8 5 L 0 8.5 z" fill="var(--fg-muted,#57606a)"/></marker>
<style>
.fmd-diagram{font-family:Arial,Helvetica,sans-serif;font-size:13px}
.fmd-subgraph-box{fill:var(--bg-subtle,#f6f8fa);stroke:var(--border-default,#d0d7de);stroke-width:1.5}
.fmd-subgraph-title{fill:var(--fg-default,#1f2328);font-weight:600;text-anchor:middle;dominant-baseline:central}
.fmd-node-rect{fill:var(--bg-canvas,#ffffff);stroke:var(--border-default,#d0d7de);stroke-width:1.5}
.fmd-node-text{fill:var(--fg-default,#1f2328);text-anchor:middle;dominant-baseline:central;font-weight:500}
.fmd-edge-path{fill:none;stroke:var(--fg-muted,#57606a);stroke-width:1.5;marker-end:url(#fmd-group-arrow)}
.fmd-edge-label-bg{fill:var(--bg-canvas,#ffffff)}
.fmd-edge-label-text{fill:var(--fg-muted,#57606a);font-size:11px;text-anchor:middle;dominant-baseline:central}
@media(prefers-color-scheme:dark){
.fmd-subgraph-box{fill:#0d1117;stroke:#30363d}.fmd-node-rect{fill:#161b22;stroke:#30363d}
.fmd-subgraph-title,.fmd-node-text{fill:#e6edf3}.fmd-edge-path{stroke:#8b949e}
.fmd-edge-label-bg{fill:#0d1117}.fmd-edge-label-text,#fmd-group-arrow path{fill:#8b949e}
}
</style>
</defs>
"#;
