//! Bounded, cursor-based admission for the native flowchart subset.
//!
//! Consume node labels before looking for edges: arrows, brackets, comments,
//! and semicolons in quoted text are content, not graph structure. A rejected
//! statement rejects the whole graph so callers can retain its source.

use super::{FlowDirection, FlowEdge, FlowNode, NodeShape};
use std::collections::BTreeMap;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_NODES: usize = 256;
const MAX_EDGES: usize = 1024;
const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 512;

pub(super) struct Graph {
    pub dir: FlowDirection,
    pub nodes: BTreeMap<String, FlowNode>,
    pub order: Vec<String>,
    pub edges: Vec<FlowEdge>,
}

pub(super) fn parse(source: &str) -> Option<Graph> {
    if source.len() > MAX_SOURCE_BYTES
        || source
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return None;
    }
    let mut parser = Parser { source, pos: 0 };
    let mut graph = Graph {
        dir: FlowDirection::TopToBottom,
        nodes: BTreeMap::new(),
        order: Vec::new(),
        edges: Vec::new(),
    };
    parser.separators()?;
    if super::names_flowchart(parser.rest()) {
        parser.word()?;
        parser.spaces();
        if !parser.at_end_of_statement() {
            graph.dir = match parser.word()? {
                "TD" | "TB" => FlowDirection::TopToBottom,
                "BT" => FlowDirection::BottomToTop,
                "LR" => FlowDirection::LeftToRight,
                "RL" => FlowDirection::RightToLeft,
                _ => return None,
            };
            parser.spaces();
        }
        if !parser.at_end_of_statement() {
            return None;
        }
        parser.separators()?;
    }
    while !parser.rest().is_empty() {
        let mut from = parser.group(&mut graph)?;
        parser.spaces();
        while !parser.at_end_of_statement() {
            let (dashed, thick, arrow, label) = parser.edge()?;
            let to = parser.group(&mut graph)?;
            let count = from.len().checked_mul(to.len())?;
            if graph.edges.len().checked_add(count)? > MAX_EDGES {
                return None;
            }
            for from in &from {
                for to in &to {
                    graph.edges.push(FlowEdge {
                        from: from.clone(),
                        to: to.clone(),
                        label: label.clone(),
                        dashed,
                        thick,
                        arrow,
                    });
                }
            }
            from = to;
            parser.spaces();
        }
        parser.separators()?;
    }
    (!graph.nodes.is_empty()).then_some(graph)
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.source[self.pos..]
    }

    fn take(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    fn spaces(&mut self) {
        while let Some(c @ (' ' | '\t' | '\r')) = self.rest().chars().next() {
            self.pos += c.len_utf8();
        }
    }

    fn at_end_of_statement(&self) -> bool {
        self.rest().is_empty()
            || self.rest().starts_with(['\n', ';'])
            || self.rest().starts_with("%%")
    }

    fn separators(&mut self) -> Option<()> {
        loop {
            self.spaces();
            if self.take(";") || self.take("\n") {
                continue;
            }
            if self.rest().starts_with("%%") {
                // Mermaid configuration is not an ordinary comment. Retain
                // it as source instead of silently drawing a different graph.
                if self.rest()[2..].trim_start().starts_with('{') {
                    return None;
                }
                self.pos += self.rest().find('\n').unwrap_or(self.rest().len());
                continue;
            }
            return Some(());
        }
    }

    fn word(&mut self) -> Option<&'a str> {
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() || c == ';' || self.rest().starts_with("%%") {
                break;
            }
            self.pos += c.len_utf8();
        }
        (self.pos != start).then_some(&self.source[start..self.pos])
    }

    fn group(&mut self, graph: &mut Graph) -> Option<Vec<String>> {
        let mut nodes = vec![self.node(graph)?];
        self.spaces();
        while self.take("&") {
            let id = self.node(graph)?;
            if !nodes.contains(&id) {
                nodes.push(id);
            }
            self.spaces();
        }
        Some(nodes)
    }

    fn node(&mut self, graph: &mut Graph) -> Option<String> {
        self.spaces();
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
                || self.rest().starts_with("--")
                || self.rest().starts_with("-.")
            {
                break;
            }
            self.pos += c.len_utf8();
        }
        let id = &self.source[start..self.pos];
        if id.is_empty()
            || id.len() > MAX_ID_BYTES
            || matches!(
                id,
                "graph"
                    | "flowchart"
                    | "subgraph"
                    | "end"
                    | "direction"
                    | "style"
                    | "classDef"
                    | "class"
                    | "click"
                    | "linkStyle"
                    | "accTitle"
                    | "accDescr"
            )
        {
            return None;
        }
        self.spaces();
        if ["[[", "[(", "[/", "[\\", "{{", "((("]
            .iter()
            .any(|open| self.rest().starts_with(open))
        {
            return None;
        }
        let shape = if self.take("([") {
            Some((NodeShape::Stadium, "])"))
        } else if self.take("((") {
            Some((NodeShape::Circle, "))"))
        } else if self.take("[") {
            Some((NodeShape::Rectangle, "]"))
        } else if self.take("(") {
            Some((NodeShape::Rounded, ")"))
        } else if self.take("{") {
            Some((NodeShape::Diamond, "}"))
        } else {
            None
        };
        let explicit = shape.is_some();
        let (shape, label) = if let Some((shape, close)) = shape {
            (shape, self.label(close, true)?)
        } else {
            (NodeShape::Rectangle, id.to_owned())
        };
        if !graph.nodes.contains_key(id) {
            if graph.nodes.len() >= MAX_NODES {
                return None;
            }
            graph.order.push(id.to_owned());
        } else if !explicit {
            return Some(id.to_owned());
        }
        let width = (label.len() as f32 * 8.5 + 32.0).max(80.0);
        let height = match shape {
            NodeShape::Diamond => 54.0,
            NodeShape::Circle => width,
            _ => 38.0,
        };
        graph.nodes.insert(
            id.to_owned(),
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
        Some(id.to_owned())
    }

    fn label(&mut self, close: &str, node: bool) -> Option<String> {
        self.spaces();
        let quoted = self.take("\"");
        let start = self.pos;
        while !self.rest().is_empty() {
            if quoted && self.take("\"") {
                let label = self.source[start..self.pos - 1].to_owned();
                self.spaces();
                return (self.take(close) && label.len() <= MAX_LABEL_BYTES).then_some(label);
            }
            if !quoted && self.rest().starts_with(close) {
                let label = self.source[start..self.pos].trim().to_owned();
                self.pos += close.len();
                return (label.len() <= MAX_LABEL_BYTES).then_some(label);
            }
            let c = self.rest().chars().next()?;
            if c == '\n'
                || self.pos - start >= MAX_LABEL_BYTES
                || (!quoted
                    && (c == '"' || (node && matches!(c, '[' | ']' | '(' | ')' | '{' | '}'))))
            {
                return None;
            }
            self.pos += c.len_utf8();
        }
        None
    }

    fn edge(&mut self) -> Option<(bool, bool, bool, Option<String>)> {
        self.spaces();
        for (token, dashed, thick, arrow) in [
            ("-.->", true, false, true),
            ("-.-", true, false, false),
            ("-->", false, false, true),
            ("---", false, false, false),
            ("==>", false, true, true),
            ("===", false, true, false),
        ] {
            if self.take(token) {
                self.spaces();
                let label = if self.take("|") {
                    Some(self.label("|", false)?)
                } else {
                    None
                };
                return Some((dashed, thick, arrow, label));
            }
        }
        let (dashed, thick, closers) = if self.take("--") {
            (false, false, ["-->", "---"])
        } else if self.take("-.") {
            (true, false, [".->", ".-"])
        } else if self.take("==") {
            (false, true, ["==>", "==="])
        } else {
            return None;
        };
        self.spaces();
        let quoted = self.take("\"");
        let start = self.pos;
        let (label, arrow) = loop {
            if quoted && self.take("\"") {
                let label = self.source[start..self.pos - 1].to_owned();
                self.spaces();
                if self.take(closers[0]) {
                    break (label, true);
                }
                if self.take(closers[1]) {
                    break (label, false);
                }
                return None;
            }
            if !quoted {
                let end = self.pos;
                if self.take(closers[0]) {
                    break (self.source[start..end].trim().to_owned(), true);
                }
                if self.take(closers[1]) {
                    break (self.source[start..end].trim().to_owned(), false);
                }
            }
            let c = self.rest().chars().next()?;
            if c == '\n'
                || self.pos - start >= MAX_LABEL_BYTES
                || (!quoted && matches!(c, ';' | '"' | '|'))
            {
                return None;
            }
            self.pos += c.len_utf8();
        };
        if label.is_empty() || label.len() > MAX_LABEL_BYTES {
            return None;
        }
        Some((dashed, thick, arrow, Some(label)))
    }
}
