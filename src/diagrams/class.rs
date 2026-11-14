//! Bounded Mermaid class diagrams using the shared native graph geometry.
//!
//! Admit the complete diagram before drawing it. Unsupported directives and
//! malformed members or relationships preserve the original source in callers.

use super::{
    FlowDirection, FlowEdge, FlowNode, NodeShape, SvgNum, assign_flowchart_layers,
    escape_svg_text, get_edge_endpoints, layout_flowchart_nodes, layout_flowchart_returns,
};
use std::collections::BTreeMap;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_CLASSES: usize = 256;
const MAX_RELATIONS: usize = 1024;
const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
const MAX_CARDINALITY_BYTES: usize = 64;
const MAX_MEMBERS_PER_CLASS: usize = 64;
const MAX_MEMBERS: usize = 2048;
const ROW: f32 = 18.0;
const WRAP_WIDTH: f32 = 360.0;

pub(super) fn is_header(line: &str) -> bool {
    line.split_whitespace().next() == Some("classDiagram")
}

pub(super) fn render(source: &str) -> Option<String> {
    let diagram = parse(source)?;
    draw(&diagram)
}

#[derive(Default)]
struct Class {
    label: String,
    explicit_label: bool,
    annotation: Option<String>,
    attributes: Vec<Member>,
    methods: Vec<Member>,
}

struct Member {
    text: String,
    abstract_method: bool,
    static_member: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    None,
    Arrow,
    Triangle,
    Aggregation,
    Composition,
}

struct Relation {
    from: String,
    to: String,
    start: End,
    end: End,
    dashed: bool,
    label: Option<String>,
    start_cardinality: Option<String>,
    end_cardinality: Option<String>,
}

struct Diagram {
    classes: BTreeMap<String, Class>,
    order: Vec<String>,
    relations: Vec<Relation>,
    direction: FlowDirection,
    direction_set: bool,
    members: usize,
}

fn parse(source: &str) -> Option<Diagram> {
    if source.len() > MAX_SOURCE_BYTES
        || source
            .chars()
            .any(|ch| {
                (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                    || matches!(ch, '\u{fffe}' | '\u{ffff}')
            })
    {
        return None;
    }
    let mut diagram = Diagram {
        classes: BTreeMap::new(),
        order: Vec::new(),
        relations: Vec::new(),
        direction: FlowDirection::TopToBottom,
        direction_set: false,
        members: 0,
    };
    let mut header = false;
    let mut block: Option<String> = None;
    for raw in source.split(['\n', '\r']) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix("%%") {
            if comment.trim_start().starts_with('{') {
                return None;
            }
            continue;
        }
        if !header {
            if line != "classDiagram" {
                return None;
            }
            header = true;
            continue;
        }
        if let Some(id) = &block {
            if line == "}" {
                block = None;
            } else {
                diagram.member(id, line)?;
            }
            continue;
        }
        if let Some(rest) = keyword(line, "direction") {
            if diagram.direction_set {
                return None;
            }
            diagram.direction = match rest {
                "TD" | "TB" => FlowDirection::TopToBottom,
                "BT" => FlowDirection::BottomToTop,
                "LR" => FlowDirection::LeftToRight,
                "RL" => FlowDirection::RightToLeft,
                _ => return None,
            };
            diagram.direction_set = true;
        } else if let Some(rest) = keyword(line, "class") {
            block = diagram.declaration(rest)?;
        } else if line.starts_with("<<") {
            let mut cursor = Cursor(line);
            let annotation = cursor.annotation()?;
            let id = cursor.id()?;
            cursor.finish()?;
            diagram.annotate(id, annotation)?;
        } else {
            diagram.statement(line)?;
        }
    }
    if !header || block.is_some() || diagram.classes.is_empty() {
        return None;
    }
    Some(diagram)
}

fn keyword<'a>(source: &'a str, word: &str) -> Option<&'a str> {
    source
        .strip_prefix(word)
        .filter(|tail| tail.starts_with(char::is_whitespace))
        .map(str::trim_start)
}

impl Diagram {
    fn class(&mut self, id: &str) -> Option<&mut Class> {
        if !self.classes.contains_key(id) {
            if self.classes.len() == MAX_CLASSES {
                return None;
            }
            self.order.push(id.to_owned());
            self.classes.insert(
                id.to_owned(),
                Class {
                    label: id.to_owned(),
                    ..Class::default()
                },
            );
        }
        self.classes.get_mut(id)
    }

    fn declaration(&mut self, source: &str) -> Option<Option<String>> {
        let mut cursor = Cursor(source);
        let id = cursor.id()?;
        self.class(id)?;
        if cursor.take("[") {
            let label = text(cursor.quoted()?, MAX_TEXT_BYTES)?;
            if !cursor.take("]") {
                return None;
            }
            let class = self.classes.get_mut(id)?;
            if class.explicit_label {
                return None;
            }
            class.label = label;
            class.explicit_label = true;
        }
        if cursor.0.starts_with("<<") {
            let annotation = cursor.annotation()?;
            self.annotate(id, annotation)?;
        }
        if cursor.take("{") {
            if cursor.0.is_empty() {
                return Some(Some(id.to_owned()));
            }
            // Empty and single-member inline blocks are unambiguous. Multiple
            // members use physical lines; semicolon statements are not admitted.
            let member = cursor.0.strip_suffix('}')?.trim();
            if !member.is_empty() {
                self.member(id, member)?;
            }
            return Some(None);
        }
        cursor.finish()?;
        Some(None)
    }

    fn annotate(&mut self, id: &str, annotation: &str) -> Option<()> {
        let annotation = text(annotation, MAX_ID_BYTES)?;
        if annotation.contains(['<', '>', '{', '}', ';', '~']) {
            return None;
        }
        let class = self.class(id)?;
        if class.annotation.is_some() {
            return None;
        }
        class.annotation = Some(annotation);
        Some(())
    }

    fn member(&mut self, id: &str, source: &str) -> Option<()> {
        if source.starts_with("<<") {
            let mut cursor = Cursor(source);
            let annotation = cursor.annotation()?;
            cursor.finish()?;
            return self.annotate(id, annotation);
        }
        if source.contains(['{', '}', ';'])
            || source.starts_with("::")
            || ["class", "namespace", "direction", "note", "style", "click", "classDef"]
                .iter()
                .any(|word| keyword(source, word).is_some())
        {
            return None;
        }
        let mut value = text(source, MAX_TEXT_BYTES)?;
        // Tilde generic notation needs a separate type grammar. A leading
        // tilde is the ordinary package/internal visibility marker.
        if value.strip_prefix('~').unwrap_or(&value).contains('~') {
            return None;
        }
        let mut depth = 0usize;
        let mut method = false;
        for ch in value.chars() {
            match ch {
                '(' => {
                    depth = depth.checked_add(1)?;
                    method = true;
                }
                ')' => depth = depth.checked_sub(1)?,
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        let static_member = value.ends_with('$');
        let abstract_method = method && value.ends_with('*');
        if static_member || abstract_method {
            let stripped = value[..value.len() - 1].trim_end();
            if stripped.is_empty() || stripped.ends_with(['$', '*']) {
                return None;
            }
            // Static fields have a native underline. Keep the abstract marker
            // literal: the shared Flow diagram profile deliberately admits
            // body/bold text only, so italic text would refuse the whole scene.
            if static_member {
                value = stripped.to_owned();
            }
        }
        if !method && value.ends_with('*') {
            return None;
        }
        if self.members == MAX_MEMBERS {
            return None;
        }
        let class = self.class(id)?;
        if class.attributes.len() + class.methods.len() == MAX_MEMBERS_PER_CLASS {
            return None;
        }
        let member = Member {
            text: value,
            abstract_method,
            static_member,
        };
        if method {
            class.methods.push(member);
        } else {
            class.attributes.push(member);
        }
        self.members += 1;
        Some(())
    }

    fn statement(&mut self, source: &str) -> Option<()> {
        let mut cursor = Cursor(source);
        let from = cursor.id()?;
        if cursor.take(":") {
            return self.member(from, cursor.0);
        }
        let start_cardinality = cursor.cardinality()?;
        let (start, end, dashed) = cursor.relation()?;
        let end_cardinality = cursor.cardinality()?;
        let to = cursor.id()?;
        let label = if cursor.take(":") {
            if cursor.0.contains(';') {
                return None;
            }
            Some(text(cursor.0, MAX_TEXT_BYTES)?)
        } else {
            cursor.finish()?;
            None
        };
        self.class(from)?;
        self.class(to)?;
        if self.relations.len() == MAX_RELATIONS {
            return None;
        }
        self.relations.push(Relation {
            from: from.to_owned(),
            to: to.to_owned(),
            start,
            end,
            dashed,
            label,
            start_cardinality,
            end_cardinality,
        });
        Some(())
    }
}

fn text(source: &str, limit: usize) -> Option<String> {
    let source = source.trim();
    if source.is_empty() || source.len() > limit {
        return None;
    }
    // Expand tabs explicitly; every renderer receives the same visible text.
    Some(source.replace('\t', "    "))
}

struct Cursor<'a>(&'a str);

impl<'a> Cursor<'a> {
    fn take(&mut self, token: &str) -> bool {
        if let Some(rest) = self.0.strip_prefix(token) {
            self.0 = rest.trim_start();
            true
        } else {
            false
        }
    }

    fn id(&mut self) -> Option<&'a str> {
        let id = if let Some(quoted) = self.0.strip_prefix('`') {
            let (id, tail) = quoted.split_once('`')?;
            self.0 = tail.trim_start();
            id
        } else {
            let mut end = 0;
            for (index, ch) in self.0.char_indices() {
                if ch.is_alphanumeric() || ch == '_' {
                    end = index + ch.len_utf8();
                } else if ch == '-' && !self.0[index..].starts_with("--") {
                    end = index + 1;
                } else {
                    break;
                }
            }
            let id = &self.0[..end];
            self.0 = self.0[end..].trim_start();
            if matches!(
                id,
                "classDiagram" | "class" | "direction" | "namespace" | "note"
                    | "style" | "classDef" | "cssClass" | "click" | "link"
                    | "callback" | "accTitle" | "accDescr"
            ) {
                return None;
            }
            id
        };
        if id.trim().is_empty() || id.len() > MAX_ID_BYTES {
            return None;
        }
        Some(id)
    }

    fn quoted(&mut self) -> Option<&'a str> {
        let source = self.0.strip_prefix('"')?;
        let (value, tail) = source.split_once('"')?;
        self.0 = tail.trim_start();
        Some(value)
    }

    fn annotation(&mut self) -> Option<&'a str> {
        let source = self.0.strip_prefix("<<")?;
        let (value, tail) = source.split_once(">>")?;
        self.0 = tail.trim_start();
        Some(value)
    }

    fn relation(&mut self) -> Option<(End, End, bool)> {
        // An operator is contiguous. Trimming between its pieces would turn
        // "A -- order" into an aggregation targeting an invented "rder" class.
        let take = |source: &mut &'a str, token: &str| {
            if let Some(rest) = source.strip_prefix(token) {
                *source = rest;
                true
            } else {
                false
            }
        };
        let mut rest = self.0;
        let start = if take(&mut rest, "<|") {
            End::Triangle
        } else if take(&mut rest, "<") {
            End::Arrow
        } else if take(&mut rest, "*") {
            End::Composition
        } else if take(&mut rest, "o") {
            End::Aggregation
        } else {
            End::None
        };
        let dashed = if take(&mut rest, "..") {
            true
        } else if take(&mut rest, "--") {
            false
        } else {
            return None;
        };
        let end = if take(&mut rest, "|>") {
            End::Triangle
        } else if take(&mut rest, ">") {
            End::Arrow
        } else if take(&mut rest, "*") {
            End::Composition
        } else if take(&mut rest, "o") {
            End::Aggregation
        } else {
            End::None
        };
        self.0 = rest.trim_start();
        Some((start, end, dashed))
    }

    fn cardinality(&mut self) -> Option<Option<String>> {
        if self.0.starts_with('"') {
            Some(Some(text(self.quoted()?, MAX_CARDINALITY_BYTES)?))
        } else {
            Some(None)
        }
    }

    fn finish(self) -> Option<()> {
        self.0.is_empty().then_some(())
    }
}

struct Text {
    lines: Vec<String>,
    width: f32,
}

impl Text {
    fn new(value: &str, size: f32) -> Self {
        let mut lines = Vec::new();
        let mut start = 0;
        let mut width: f32 = 0.0;
        let mut maximum: f32 = 0.0;
        for (index, ch) in value.char_indices() {
            let advance = char_width(ch, size);
            if width + advance > WRAP_WIDTH && index > start {
                lines.push(value[start..index].to_owned());
                maximum = maximum.max(width);
                start = index;
                width = 0.0;
            }
            width += advance;
        }
        lines.push(value[start..].to_owned());
        Self {
            lines,
            width: maximum.max(width),
        }
    }

    fn height(&self) -> f32 {
        self.lines.len() as f32 * ROW
    }

    fn draw(&self, svg: &mut String, x: f32, y: f32, class: &str, anchor: &str) {
        for (index, line) in self.lines.iter().enumerate() {
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" class=\"{class}\" text-anchor=\"{anchor}\">{}</text>\n",
                SvgNum(x),
                SvgNum(y + index as f32 * ROW),
                escape_svg_text(line)
            ));
        }
    }
}

fn char_width(ch: char, size: f32) -> f32 {
    // Conservative envelope shared by browser and bundled-font consumers.
    // It is a layout bound, not a claim to shape arbitrary scripts.
    if ch.is_ascii() {
        size
    } else {
        size * 1.5
    }
}

fn text_width(value: &str, size: f32) -> f32 {
    value.chars().map(|ch| char_width(ch, size)).sum()
}

struct MemberLayout<'a> {
    member: &'a Member,
    text: Text,
}

struct ClassLayout<'a> {
    name: Text,
    annotation: Option<Text>,
    attributes: Vec<MemberLayout<'a>>,
    methods: Vec<MemberLayout<'a>>,
    width: f32,
    header: f32,
    attributes_height: f32,
    height: f32,
}

impl<'a> ClassLayout<'a> {
    fn new(class: &'a Class) -> Self {
        let name = Text::new(&class.label, 14.0);
        let annotation = class
            .annotation
            .as_ref()
            .map(|value| Text::new(&format!("<<{value}>>"), 11.0));
        let layout = |members: &'a [Member]| {
            members
                .iter()
                .map(|member| MemberLayout {
                    member,
                    text: Text::new(&member.text, 12.0),
                })
                .collect::<Vec<_>>()
        };
        let attributes = layout(&class.attributes);
        let methods = layout(&class.methods);
        let mut width = name.width;
        if let Some(annotation) = &annotation {
            width = width.max(annotation.width);
        }
        for member in attributes.iter().chain(&methods) {
            width = width.max(member.text.width);
        }
        let compartment = |members: &[MemberLayout<'_>]| {
            members.iter().map(|member| member.text.height()).sum::<f32>() + 12.0
        };
        let header = name.height() + annotation.as_ref().map_or(0.0, Text::height) + 16.0;
        let attributes_height = compartment(&attributes);
        let height = header + attributes_height + compartment(&methods);
        Self {
            name,
            annotation,
            attributes,
            methods,
            width: (width + 28.0).max(150.0),
            header,
            attributes_height,
            height,
        }
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Bounds {
    fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.width + 4.0
            && self.x + self.width + 4.0 > other.x
            && self.y < other.y + other.height + 4.0
            && self.y + self.height + 4.0 > other.y
    }
}

struct Label<'a> {
    text: &'a str,
    x: f32,
    y: f32,
    cardinality: bool,
}

impl Label<'_> {
    fn bounds(&self) -> Bounds {
        let width = text_width(self.text, 11.0) + 8.0;
        Bounds {
            x: self.x - width / 2.0,
            y: self.y - 9.0,
            width,
            height: 18.0,
        }
    }

    fn draw(&self, svg: &mut String) {
        let bounds = self.bounds();
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"18\" rx=\"2\" class=\"fmd-class-label-bg\"/>\n",
            SvgNum(bounds.x), SvgNum(bounds.y), SvgNum(bounds.width)
        ));
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" class=\"{}\" text-anchor=\"middle\">{}</text>\n",
            SvgNum(self.x), SvgNum(self.y + 4.0),
            if self.cardinality { "fmd-class-cardinality" } else { "fmd-class-label" },
            escape_svg_text(self.text)
        ));
    }
}

struct EdgeScene<'a> {
    points: Vec<(f32, f32)>,
    labels: Vec<Label<'a>>,
    outside: bool,
}

fn draw(diagram: &Diagram) -> Option<String> {
    let layouts: BTreeMap<_, _> = diagram
        .classes
        .iter()
        .map(|(id, class)| (id.clone(), ClassLayout::new(class)))
        .collect();
    let mut nodes: BTreeMap<_, _> = layouts
        .iter()
        .map(|(id, layout)| {
            (
                id.clone(),
                FlowNode {
                    label: String::new(),
                    shape: NodeShape::Rectangle,
                    layer: 0,
                    x: 0.0,
                    y: 0.0,
                    width: layout.width,
                    height: layout.height,
                },
            )
        })
        .collect();
    let edges: Vec<_> = diagram
        .relations
        .iter()
        .map(|relation| FlowEdge {
            from: relation.from.clone(),
            to: relation.to.clone(),
            // The shared router reserves a single-line text envelope. Reserving
            // the widest caption also makes room for endpoint cardinalities.
            label: [
                relation.label.as_ref(),
                relation.start_cardinality.as_ref(),
                relation.end_cardinality.as_ref(),
            ]
            .into_iter()
            .flatten()
            .max_by_key(|label| label.len())
            .cloned(),
            dashed: relation.dashed,
            thick: false,
            arrow: false,
        })
        .collect();
    let horizontal = matches!(
        diagram.direction,
        FlowDirection::LeftToRight | FlowDirection::RightToLeft
    );
    // A self-relation travels along the outside of its class. Keep enough
    // space on that rail for its separate label and endpoint multiplicities.
    for relation in &diagram.relations {
        if relation.from != relation.to
            || (relation.start_cardinality.is_none() && relation.end_cardinality.is_none())
        {
            continue;
        }
        let largest = [
            relation.start_cardinality.as_deref(),
            relation.end_cardinality.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(|value| text_width(value, 11.0) + 8.0)
        .fold(0.0f32, f32::max);
        let node = nodes.get_mut(&relation.from)?;
        if horizontal {
            let label = relation.label.as_ref().map_or(0.0, |value| text_width(value, 11.0) + 8.0);
            node.width = node.width.max(2.0 * largest + label + 80.0);
        } else {
            node.height = node.height.max(134.0);
        }
    }
    assign_flowchart_layers(&mut nodes, &diagram.order, &edges);
    let (mut width, mut height) =
        layout_flowchart_nodes(&mut nodes, &diagram.order, &edges, diagram.direction);
    let largest_cardinality = diagram
        .relations
        .iter()
        .flat_map(|relation| {
            [
                relation.start_cardinality.as_deref(),
                relation.end_cardinality.as_deref(),
            ]
        })
        .flatten()
        .map(|label| text_width(label, 11.0) + 8.0)
        .fold(0.0f32, f32::max);
    if largest_cardinality > 0.0 {
        let extra = if horizontal {
            2.0 * largest_cardinality + 80.0
        } else {
            80.0
        };
        let last_layer = nodes.values().map(|node| node.layer).max().unwrap_or(0);
        for node in nodes.values_mut() {
            let forward = matches!(
                diagram.direction,
                FlowDirection::TopToBottom | FlowDirection::LeftToRight
            );
            let layer = if forward {
                node.layer
            } else {
                last_layer - node.layer
            };
            if horizontal {
                node.x += layer as f32 * extra;
            } else {
                node.y += layer as f32 * extra;
            }
        }
        if horizontal {
            width += last_layer as f32 * extra;
        } else {
            height += last_layer as f32 * extra;
        }
    }
    let base_nodes = nodes;
    let mut forced = vec![false; edges.len()];
    let mut final_scene = None;
    // A direct caption/cardinality conflict gets its own outside route. The
    // second pass either proves clear labels or refuses the complete diagram.
    for _ in 0..2 {
        let mut nodes = base_nodes.clone();
        let (mut canvas_width, mut canvas_height) = (width, height);
        let mut routing_edges = Vec::new();
        let mut indices = Vec::new();
        for (edge, force) in edges.iter().zip(&forced) {
            if *force {
                let mut reservation = edge.clone();
                reservation.label = None;
                routing_edges.push(reservation);
            }
            indices.push(routing_edges.len());
            routing_edges.push(edge.clone());
        }
        let routes = layout_flowchart_returns(
            &mut nodes,
            &routing_edges,
            diagram.direction,
            &mut canvas_width,
            &mut canvas_height,
        );
        let mut scenes = Vec::new();
        for (relation, index) in diagram.relations.iter().zip(indices) {
            let from = nodes.get(&relation.from)?;
            let to = nodes.get(&relation.to)?;
            let (x1, y1, x2, y2) = get_edge_endpoints(from, to, diagram.direction);
            let route = routes.get(index)?.as_ref();
            let points = if let Some(route) = route {
                route.points.to_vec()
            } else if horizontal {
                let mid = (x1 + x2) / 2.0;
                vec![(x1, y1), (mid, y1), (mid, y2), (x2, y2)]
            } else {
                let mid = (y1 + y2) / 2.0;
                vec![(x1, y1), (x1, mid), (x2, mid), (x2, y2)]
            };
            let mut labels = Vec::new();
            if let Some(label) = &relation.label {
                let (x, y) = route.map_or(((x1 + x2) / 2.0, (y1 + y2) / 2.0), |r| r.label);
                labels.push(Label {
                    text: label,
                    x,
                    y,
                    cardinality: false,
                });
            }
            let (card_start, card_end) =
                route.map_or(((x1, y1), (x2, y2)), |r| (r.points[2], r.points[3]));
            let vector = if horizontal {
                ((card_end.0 - card_start.0).signum(), 0.0)
            } else {
                (0.0, (card_end.1 - card_start.1).signum())
            };
            for (value, origin, sign) in [
                (relation.start_cardinality.as_deref(), card_start, 1.0),
                (relation.end_cardinality.as_deref(), card_end, -1.0),
            ] {
                if let Some(value) = value {
                    let distance = if horizontal {
                        text_width(value, 11.0) / 2.0 + 18.0
                    } else {
                        26.0
                    };
                    labels.push(Label {
                        text: value,
                        x: origin.0 + vector.0 * distance * sign,
                        y: origin.1 + vector.1 * distance * sign,
                        cardinality: true,
                    });
                }
            }
            scenes.push(EdgeScene {
                points,
                labels,
                outside: route.is_some(),
            });
        }
        let mut retry = false;
        let mut labels: Vec<(usize, Bounds)> = Vec::new();
        for (index, scene) in scenes.iter().enumerate() {
            for label in &scene.labels {
                let bounds = label.bounds();
                if nodes.values().any(|node| {
                    bounds.overlaps(Bounds {
                        x: node.x,
                        y: node.y,
                        width: node.width,
                        height: node.height,
                    })
                }) {
                    if scene.outside {
                        return None;
                    }
                    forced[index] = true;
                    retry = true;
                }
                for &(other, previous) in &labels {
                    if bounds.overlaps(previous) {
                        if scene.outside && scenes[other].outside {
                            return None;
                        }
                        if !scene.outside {
                            forced[index] = true;
                        }
                        if !scenes[other].outside {
                            forced[other] = true;
                        }
                        retry = true;
                    }
                }
                labels.push((index, bounds));
            }
        }
        if !retry {
            final_scene = Some((nodes, scenes, canvas_width, canvas_height));
            break;
        }
    }
    let (mut nodes, mut scenes, mut width, mut height) = final_scene?;
    let (mut min_x, mut min_y) = (0.0f32, 0.0f32);
    for scene in &scenes {
        for label in &scene.labels {
            let bounds = label.bounds();
            min_x = min_x.min(bounds.x - 12.0);
            min_y = min_y.min(bounds.y - 12.0);
            width = width.max(bounds.x + bounds.width + 12.0);
            height = height.max(bounds.y + bounds.height + 12.0);
        }
    }
    for node in nodes.values_mut() {
        node.x -= min_x;
        node.y -= min_y;
    }
    for scene in &mut scenes {
        for point in &mut scene.points {
            point.0 -= min_x;
            point.1 -= min_y;
        }
        for label in &mut scene.labels {
            label.x -= min_x;
            label.y -= min_y;
        }
    }
    width -= min_x;
    height -= min_y;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-class\" role=\"img\" font-family=\"Arial,Helvetica,sans-serif\" xml:space=\"preserve\">\n<title>Class diagram</title>\n",
        SvgNum(width), SvgNum(height), SvgNum(width), SvgNum(height)
    );
    svg.push_str(STYLE);
    for (relation, scene) in diagram.relations.iter().zip(&scenes) {
        svg.push_str("<path d=\"");
        for (index, &(x, y)) in scene.points.iter().enumerate() {
            svg.push_str(&format!(
                "{} {} {} ",
                if index == 0 { "M" } else { "L" },
                SvgNum(x),
                SvgNum(y)
            ));
        }
        svg.push_str("\" class=\"fmd-class-relation\"");
        if relation.dashed {
            svg.push_str(" stroke-dasharray=\"5,4\"");
        }
        svg.push_str("/>\n");
        endpoint(&mut svg, &scene.points, relation.start, false);
        endpoint(&mut svg, &scene.points, relation.end, true);
        for label in &scene.labels {
            label.draw(&mut svg);
        }
    }
    for id in &diagram.order {
        let node = nodes.get(id)?;
        let layout = layouts.get(id)?;
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-class-node\"/>\n",
            SvgNum(node.x), SvgNum(node.y), SvgNum(node.width), SvgNum(node.height)
        ));
        for offset in [layout.header, layout.header + layout.attributes_height] {
            svg.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-class-divider\"/>\n",
                SvgNum(node.x), SvgNum(node.y + offset),
                SvgNum(node.x + node.width), SvgNum(node.y + offset)
            ));
        }
        let mut y = node.y + 21.0;
        if let Some(annotation) = &layout.annotation {
            annotation.draw(&mut svg, node.x + node.width / 2.0, y, "fmd-class-annotation", "middle");
            y += annotation.height();
        }
        layout.name.draw(&mut svg, node.x + node.width / 2.0, y, "fmd-class-name", "middle");
        let mut y = node.y + layout.header + 17.0;
        for (members, start) in [
            (&layout.attributes, y),
            (&layout.methods, y + layout.attributes_height),
        ] {
            y = start;
            for member in members {
                member.text.draw(
                    &mut svg,
                    node.x + 14.0,
                    y,
                    if member.member.abstract_method { "fmd-class-abstract" } else { "fmd-class-member" },
                    "start",
                );
                if member.member.static_member {
                    for (line, value) in member.text.lines.iter().enumerate() {
                        let baseline = y + line as f32 * ROW + 2.0;
                        svg.push_str(&format!(
                            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-class-static\"/>\n",
                            SvgNum(node.x + 14.0), SvgNum(baseline),
                            SvgNum(node.x + 14.0 + text_width(value, 12.0)), SvgNum(baseline)
                        ));
                    }
                }
                y += member.text.height();
            }
        }
    }
    svg.push_str("</svg>\n");
    Some(svg)
}

fn endpoint(svg: &mut String, points: &[(f32, f32)], end: End, reverse: bool) {
    if end == End::None {
        return;
    }
    let mut ordered: Box<dyn Iterator<Item = &(f32, f32)> + '_> = if reverse {
        Box::new(points.iter().rev())
    } else {
        Box::new(points.iter())
    };
    let Some(&(x, y)) = ordered.next() else {
        return;
    };
    let Some(&(next_x, next_y)) = ordered.find(|&&(px, py)| px != x || py != y) else {
        return;
    };
    let length = (next_x - x).hypot(next_y - y);
    // Outside routes can turn twelve points from a node. Keep a hollow
    // triangle or diamond wholly on its own terminal segment, never through
    // the first elbow or across a neighbouring relationship.
    let extent = match end {
        End::Triangle => 14.0,
        End::Aggregation | End::Composition => 16.0,
        _ => 10.0,
    };
    let scale = ((length - 1.0) / extent).clamp(0.0, 1.0);
    let (dx, dy) = (
        (next_x - x) / length * scale,
        (next_y - y) / length * scale,
    );
    let point = |along: f32, across: f32| {
        format!(
            "{},{}",
            SvgNum(x + along * dx - across * dy),
            SvgNum(y + along * dy + across * dx)
        )
    };
    match end {
        End::None => {}
        End::Arrow => svg.push_str(&format!(
            "<path d=\"M {} {} L {} {} L {} {}\" class=\"fmd-class-association\"/>\n",
            SvgNum(x + 10.0 * dx - 5.0 * dy), SvgNum(y + 10.0 * dy + 5.0 * dx),
            SvgNum(x), SvgNum(y),
            SvgNum(x + 10.0 * dx + 5.0 * dy), SvgNum(y + 10.0 * dy - 5.0 * dx)
        )),
        End::Triangle => svg.push_str(&format!(
            "<polygon points=\"{} {} {}\" class=\"fmd-class-inheritance\"/>\n",
            point(0.0, 0.0), point(14.0, 7.0), point(14.0, -7.0)
        )),
        End::Aggregation | End::Composition => svg.push_str(&format!(
            "<polygon points=\"{} {} {} {}\" class=\"{}\"/>\n",
            point(0.0, 0.0), point(8.0, 5.0), point(16.0, 0.0), point(8.0, -5.0),
            if end == End::Composition { "fmd-class-composition" } else { "fmd-class-aggregation" }
        )),
    }
}

const STYLE: &str = r#"<style>
.fmd-class-node, .fmd-class-label-bg, .fmd-class-inheritance, .fmd-class-aggregation { fill: var(--bg-canvas, #ffffff); }
.fmd-class-node, .fmd-class-divider { stroke: var(--border-default, #d0d7de); stroke-width: 1.5; }
.fmd-class-relation, .fmd-class-association { fill: none; stroke: var(--fg-muted, #57606a); stroke-width: 1.5; stroke-linejoin: round; }
.fmd-class-inheritance, .fmd-class-aggregation, .fmd-class-composition { stroke: var(--fg-muted, #57606a); stroke-width: 1.5; stroke-linejoin: round; }
.fmd-class-composition { fill: var(--fg-muted, #57606a); }
.fmd-class-name { font-size: 14px; font-weight: 700; fill: var(--fg-default, #1f2328); }
.fmd-class-member, .fmd-class-abstract { font-size: 12px; fill: var(--fg-default, #1f2328); }
.fmd-class-label, .fmd-class-cardinality, .fmd-class-annotation { font-size: 11px; fill: var(--fg-muted, #57606a); }
.fmd-class-static { stroke: var(--fg-default, #1f2328); stroke-width: 0.8; }
@media (prefers-color-scheme: dark) {
.fmd-class-node, .fmd-class-label-bg, .fmd-class-inheritance, .fmd-class-aggregation { fill: #161b22; }
.fmd-class-node, .fmd-class-divider { stroke: #30363d; }
.fmd-class-relation, .fmd-class-association, .fmd-class-inheritance, .fmd-class-aggregation, .fmd-class-composition { stroke: #8b949e; }
.fmd-class-composition { fill: #8b949e; }
.fmd-class-name, .fmd-class-member, .fmd-class-abstract { fill: #e6edf3; }
.fmd-class-label, .fmd-class-cardinality, .fmd-class-annotation { fill: #8b949e; }
.fmd-class-static { stroke: #e6edf3; }
}
</style>
"#;
