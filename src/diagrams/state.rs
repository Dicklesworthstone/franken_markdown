//! Bounded Mermaid state machines, lowered into the shared native graph scene.
//!
//! Parse every statement before drawing. Composite states, notes and styling
//! are deliberately refused as a whole, so their meaning is never discarded.

use super::{FlowDirection, FlowEdge, FlowNode, NodeShape, flowchart::Graph};
use std::collections::{BTreeMap, BTreeSet};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_NODES: usize = 256;
const MAX_EDGES: usize = 1024;
const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 512;
// Source controls are rejected, so these identities cannot alias user states.
const START: &str = "\0start";
const END: &str = "\0end";

pub(super) fn is_header(line: &str) -> bool {
    matches!(
        line.split_whitespace().next(),
        Some("stateDiagram" | "stateDiagram-v2")
    )
}

pub(super) fn render(source: &str) -> Option<String> {
    let graph = parse(source)?;
    Some(super::render_flow_graph(graph, true))
}

fn parse(source: &str) -> Option<Graph> {
    if source.len() > MAX_SOURCE_BYTES
        || source
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let mut parser = Parser {
        graph: Graph {
            dir: FlowDirection::TopToBottom,
            nodes: BTreeMap::new(),
            order: Vec::new(),
            edges: Vec::new(),
        },
        descriptions: BTreeSet::new(),
        shapes: BTreeSet::new(),
        direction: false,
    };
    let mut header = false;
    for line in source.split(['\n', '\r']) {
        let line = uncomment(line)?.trim();
        if line.is_empty() {
            continue;
        }
        if !header {
            if !matches!(line, "stateDiagram" | "stateDiagram-v2") {
                return None;
            }
            header = true;
            continue;
        }
        parser.statement(line)?;
    }
    if !header || parser.graph.nodes.is_empty() {
        return None;
    }
    // Fork/join bars are perpendicular to the direction of travel, even when
    // a direction statement occurs after their declarations.
    if matches!(
        parser.graph.dir,
        FlowDirection::LeftToRight | FlowDirection::RightToLeft
    ) {
        for node in parser.graph.nodes.values_mut() {
            if node.shape == NodeShape::StateBar {
                std::mem::swap(&mut node.width, &mut node.height);
            }
        }
    }
    Some(parser.graph)
}

/// Only a quoted state declaration has quoted grammar; descriptions following
/// a colon remain plain text, including ordinary quotation marks.
fn uncomment(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let quote_start = trimmed
        .strip_prefix("state")
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .and_then(|rest| {
            let rest = rest.trim_start();
            rest.starts_with('"').then_some(line.len() - rest.len())
        });
    let quote_end = match quote_start {
        Some(start) => Some(start + 1 + line[start + 1..].find('"')?),
        None => None,
    };
    for (offset, _) in line.match_indices("%%") {
        if quote_start
            .zip(quote_end)
            .is_some_and(|(start, end)| offset > start && offset < end)
        {
            continue;
        }
        if line[offset + 2..].trim_start().starts_with('{') {
            return None;
        }
        return Some(&line[..offset]);
    }
    Some(line)
}

struct Parser {
    graph: Graph,
    descriptions: BTreeSet<String>,
    shapes: BTreeSet<String>,
    direction: bool,
}

impl Parser {
    fn statement(&mut self, line: &str) -> Option<()> {
        if let Some(rest) = keyword(line, "direction") {
            if self.direction {
                return None;
            }
            self.graph.dir = match rest {
                "TD" | "TB" => FlowDirection::TopToBottom,
                "BT" => FlowDirection::BottomToTop,
                "LR" => FlowDirection::LeftToRight,
                "RL" => FlowDirection::RightToLeft,
                _ => return None,
            };
            self.direction = true;
            return Some(());
        }
        if let Some(rest) = keyword(line, "state") {
            return self.declaration(rest);
        }
        // A colon owns the remainder of its line. An arrow in that label is
        // content; only the part preceding the colon has transition syntax.
        let (statement, label) = match line.split_once(':') {
            // Mermaid's inline class attachment is syntax, not a description.
            Some((_, label)) if label.starts_with("::") => return None,
            Some((statement, label)) => (statement.trim(), Some(valid_label(label)?)),
            None => (line, None),
        };
        if let Some((from, to)) = statement.split_once("-->") {
            let from = from.trim();
            let to = to.trim();
            let from = if from == "[*]" {
                self.special(START, NodeShape::StateStart)?
            } else {
                self.node(from)?
            };
            let to = if to == "[*]" {
                self.special(END, NodeShape::StateEnd)?
            } else {
                self.node(to)?
            };
            if self.graph.edges.len() == MAX_EDGES {
                return None;
            }
            self.graph.edges.push(FlowEdge {
                from,
                to,
                label: label.map(str::to_owned),
                dashed: false,
                thick: false,
                arrow: true,
            });
            return Some(());
        }
        let id = self.node(statement)?;
        if let Some(label) = label {
            self.describe(&id, label)?;
        }
        Some(())
    }

    fn declaration(&mut self, rest: &str) -> Option<()> {
        if let Some(quoted) = rest.strip_prefix('"') {
            let (label, tail) = quoted.split_once('"')?;
            let id = keyword(tail.trim_start(), "as")?;
            let label = valid_label(label)?;
            let id = self.node(id)?;
            return self.describe(&id, label);
        }
        if let Some((id, stereotype)) = rest.split_once(char::is_whitespace) {
            let shape = match stereotype.trim() {
                "<<choice>>" => NodeShape::StateChoice,
                "<<fork>>" | "<<join>>" => NodeShape::StateBar,
                _ => return None,
            };
            let id = self.node(id)?;
            if self.descriptions.contains(&id) || !self.shapes.insert(id.clone()) {
                return None;
            }
            let node = self.graph.nodes.get_mut(&id)?;
            node.shape = shape;
            node.label.clear();
            (node.width, node.height) = match shape {
                NodeShape::StateChoice => (28.0, 28.0),
                _ => (56.0, 8.0),
            };
            return Some(());
        }
        self.node(rest).map(|_| ())
    }

    fn describe(&mut self, id: &str, label: &str) -> Option<()> {
        // Multiple descriptions and named pseudostate labels have richer
        // Mermaid semantics; preserve source until those are implemented.
        if self.shapes.contains(id) || !self.descriptions.insert(id.to_owned()) {
            return None;
        }
        let node = self.graph.nodes.get_mut(id)?;
        node.label = label.to_owned();
        node.width = label_width(label);
        Some(())
    }

    fn node(&mut self, id: &str) -> Option<String> {
        if id.is_empty()
            || id.len() > MAX_ID_BYTES
            || !id
                .chars()
                .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            || id.contains("--")
            || matches!(
                id,
                "state"
                    | "stateDiagram"
                    | "stateDiagram-v2"
                    | "direction"
                    | "note"
                    | "end"
                    | "classDef"
                    | "class"
                    | "style"
                    | "click"
                    | "accTitle"
                    | "accDescr"
            )
        {
            return None;
        }
        if !self.graph.nodes.contains_key(id) {
            self.insert(
                id,
                FlowNode {
                    label: id.to_owned(),
                    shape: NodeShape::Rectangle,
                    layer: 0,
                    x: 0.0,
                    y: 0.0,
                    width: label_width(id),
                    height: 38.0,
                },
            )?;
        }
        Some(id.to_owned())
    }

    fn special(&mut self, id: &str, shape: NodeShape) -> Option<String> {
        if !self.graph.nodes.contains_key(id) {
            let size = if shape == NodeShape::StateStart {
                14.0
            } else {
                20.0
            };
            self.insert(
                id,
                FlowNode {
                    label: String::new(),
                    shape,
                    layer: 0,
                    x: 0.0,
                    y: 0.0,
                    width: size,
                    height: size,
                },
            )?;
        }
        Some(id.to_owned())
    }

    fn insert(&mut self, id: &str, node: FlowNode) -> Option<()> {
        if self.graph.nodes.len() == MAX_NODES {
            return None;
        }
        self.graph.order.push(id.to_owned());
        self.graph.nodes.insert(id.to_owned(), node);
        Some(())
    }
}

fn keyword<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    line.strip_prefix(word)
        .filter(|rest| rest.starts_with(char::is_whitespace))
        .map(str::trim)
}

fn valid_label(label: &str) -> Option<&str> {
    let label = label.trim();
    (!label.is_empty() && label.len() <= MAX_LABEL_BYTES).then_some(label)
}

fn label_width(label: &str) -> f32 {
    // One font em per UTF-8 byte is a conservative envelope, including wide
    // capitals and CJK. Native Flow remeasures actual glyph runs as well.
    (label.len() as f32 * 13.0 + 32.0).max(80.0)
}
