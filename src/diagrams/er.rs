//! Bounded Mermaid entity-relationship diagrams with native crow-foot geometry.
//!
//! Parse the whole source before producing a scene. Entity attributes and
//! aliases retain source ownership; unsupported directives never disappear
//! behind a partially rendered diagram.

use super::{FlowDirection, FlowEdge, FlowNode, NodeShape, SvgInt, SvgNum, escape_svg_text};
use std::collections::BTreeMap;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_ENTITIES: usize = 256;
const MAX_RELATIONSHIPS: usize = 1024;
const MAX_ATTRIBUTES: usize = 2048;
const MAX_ENTITY_ATTRIBUTES: usize = 128;
const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 512;
// Virtual node padding keeps endpoint symbols clear of routing corners,
// captions, adjacent entities and the actual table's painted bounds.
const PORT_SPACE: f32 = 26.0;
const CELL_PAD: f32 = 10.0;
const LINE_HEIGHT: f32 = 18.0;

#[derive(Clone, Copy)]
enum Cardinality {
    ZeroOrOne,
    One,
    ZeroOrMany,
    OneOrMany,
}

impl Cardinality {
    fn optional(self) -> bool {
        matches!(self, Self::ZeroOrOne | Self::ZeroOrMany)
    }

    fn many(self) -> bool {
        matches!(self, Self::ZeroOrMany | Self::OneOrMany)
    }

    fn name(self) -> &'static str {
        match self {
            Self::ZeroOrOne => "zero or one",
            Self::One => "exactly one",
            Self::ZeroOrMany => "zero or more",
            Self::OneOrMany => "one or more",
        }
    }
}

struct Attribute {
    kind: String,
    name: String,
    keys: Vec<&'static str>,
    comment: String,
}

struct Entity {
    label: String,
    alias: Option<String>,
    attributes: Vec<Attribute>,
}

struct Relationship {
    edge: FlowEdge,
    from_card: Cardinality,
    to_card: Cardinality,
}

struct Diagram {
    entities: BTreeMap<String, Entity>,
    order: Vec<String>,
    relationships: Vec<Relationship>,
    dir: FlowDirection,
    direction_seen: bool,
    attributes: usize,
}

impl Diagram {
    fn entity(&mut self, id: &str, alias: Option<String>) -> Option<()> {
        if !self.entities.contains_key(id) {
            if self.entities.len() >= MAX_ENTITIES {
                return None;
            }
            self.order.push(id.to_owned());
            self.entities.insert(
                id.to_owned(),
                Entity {
                    label: id.to_owned(),
                    alias: None,
                    attributes: Vec::new(),
                },
            );
        }
        if let Some(alias) = alias {
            let entity = self.entities.get_mut(id)?;
            if entity.alias.as_ref().is_some_and(|old| old != &alias) {
                return None;
            }
            entity.label.clone_from(&alias);
            entity.alias = Some(alias);
        }
        Some(())
    }
}

pub(super) fn is_header(line: &str) -> bool {
    matches!(line.split_whitespace().next(), Some("erDiagram"))
}

fn parse(source: &str) -> Option<Diagram> {
    if source.len() > MAX_SOURCE_BYTES
        || source.chars().any(|ch| {
            (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                || matches!(ch, '\u{fffe}' | '\u{ffff}')
        })
    {
        return None;
    }
    let mut cursor = Cursor { source, pos: 0 };
    cursor.blank()?;
    if !cursor.keyword("erDiagram") {
        return None;
    }
    cursor.line_end()?;
    let mut diagram = Diagram {
        entities: BTreeMap::new(),
        order: Vec::new(),
        relationships: Vec::new(),
        dir: FlowDirection::TopToBottom,
        direction_seen: false,
        attributes: 0,
    };
    loop {
        cursor.blank()?;
        if cursor.rest().is_empty() {
            break;
        }
        if cursor.keyword("direction") {
            if diagram.direction_seen {
                return None;
            }
            cursor.spaces();
            diagram.dir = match cursor.name(MAX_ID_BYTES, false)?.as_str() {
                "TB" => FlowDirection::TopToBottom,
                "BT" => FlowDirection::BottomToTop,
                "LR" => FlowDirection::LeftToRight,
                "RL" => FlowDirection::RightToLeft,
                _ => return None,
            };
            diagram.direction_seen = true;
            cursor.line_end()?;
            continue;
        }
        let from = cursor.name(MAX_ID_BYTES, true)?;
        cursor.spaces();
        let alias = if cursor.take("[") {
            cursor.spaces();
            let alias = cursor.name(MAX_LABEL_BYTES, false)?;
            cursor.spaces();
            if !cursor.take("]") {
                return None;
            }
            cursor.spaces();
            Some(alias)
        } else {
            None
        };
        let has_alias = alias.is_some();
        diagram.entity(&from, alias)?;
        if cursor.take("{") {
            attributes(&mut cursor, &mut diagram, &from)?;
            cursor.line_end()?;
            continue;
        }
        if cursor.at_line_end() {
            cursor.line_end()?;
            continue;
        }
        // Aliases belong to declarations, not relationship endpoints.
        if has_alias || diagram.relationships.len() >= MAX_RELATIONSHIPS {
            return None;
        }
        let from_card = cursor.cardinality(true)?;
        cursor.spaces();
        let dashed = if cursor.take("--") {
            false
        } else if cursor.take("..") {
            true
        } else {
            return None;
        };
        cursor.spaces();
        let to_card = cursor.cardinality(false)?;
        cursor.spaces();
        let to = cursor.name(MAX_ID_BYTES, true)?;
        cursor.spaces();
        if !cursor.take(":") {
            return None;
        }
        cursor.spaces();
        let label = if cursor.rest().starts_with('"') {
            cursor.quoted(MAX_LABEL_BYTES)?
        } else {
            cursor.name(MAX_LABEL_BYTES, false)?
        };
        cursor.line_end()?;
        diagram.entity(&to, None)?;
        diagram.relationships.push(Relationship {
            edge: FlowEdge {
                from,
                to,
                label: Some(label.replace('\t', "    ")),
                dashed,
                thick: false,
                arrow: false,
            },
            from_card,
            to_card,
        });
    }
    (!diagram.entities.is_empty()).then_some(diagram)
}

fn attributes(cursor: &mut Cursor<'_>, diagram: &mut Diagram, id: &str) -> Option<()> {
    loop {
        cursor.blank()?;
        if cursor.take("}") {
            return Some(());
        }
        if diagram.attributes >= MAX_ATTRIBUTES
            || diagram.entities.get(id)?.attributes.len() >= MAX_ENTITY_ATTRIBUTES
        {
            return None;
        }
        let kind = cursor.attribute_word(true)?;
        cursor.blank()?;
        let mut name = cursor.attribute_word(false)?;
        let primary = name.starts_with('*');
        if primary {
            name.remove(0);
        }
        cursor.blank()?;
        let mut keys = Vec::new();
        if let Some(key) = cursor.key() {
            keys.push(key);
            loop {
                cursor.blank()?;
                if !cursor.take(",") {
                    break;
                }
                cursor.blank()?;
                let key = cursor.key()?;
                if keys.contains(&key) {
                    return None;
                }
                keys.push(key);
            }
        }
        if primary && !keys.contains(&"PK") {
            keys.insert(0, "PK");
        }
        cursor.blank()?;
        let comment = if cursor.rest().starts_with('"') {
            cursor.quoted(MAX_LABEL_BYTES)?
        } else {
            String::new()
        };
        diagram.entities.get_mut(id)?.attributes.push(Attribute {
            kind,
            name,
            keys,
            comment,
        });
        diagram.attributes += 1;
    }
}

struct Cursor<'a> {
    source: &'a str,
    pos: usize,
}

impl Cursor<'_> {
    fn rest(&self) -> &str {
        &self.source[self.pos..]
    }

    fn take(&mut self, value: &str) -> bool {
        if self.rest().starts_with(value) {
            self.pos += value.len();
            true
        } else {
            false
        }
    }

    fn keyword(&mut self, value: &str) -> bool {
        if self.rest().strip_prefix(value).is_some_and(|tail| {
            tail.is_empty() || tail.starts_with(char::is_whitespace) || tail.starts_with("%%")
        }) {
            self.pos += value.len();
            true
        } else {
            false
        }
    }

    fn spaces(&mut self) {
        while let Some(ch @ (' ' | '\t')) = self.rest().chars().next() {
            self.pos += ch.len_utf8();
        }
    }

    fn comment(&mut self) -> Option<()> {
        if self.rest().starts_with("%%") {
            if self.rest()[2..].trim_start().starts_with('{') {
                return None;
            }
            self.pos += self.rest().find(['\r', '\n']).unwrap_or(self.rest().len());
        }
        Some(())
    }

    fn blank(&mut self) -> Option<()> {
        loop {
            while let Some(ch @ (' ' | '\t' | '\r' | '\n')) = self.rest().chars().next() {
                self.pos += ch.len_utf8();
            }
            if !self.rest().starts_with("%%") {
                return Some(());
            }
            self.comment()?;
        }
    }

    fn at_line_end(&self) -> bool {
        self.rest().is_empty()
            || self.rest().starts_with(['\r', '\n'])
            || self.rest().starts_with("%%")
    }

    fn line_end(&mut self) -> Option<()> {
        self.spaces();
        self.comment()?;
        self.at_line_end().then_some(())
    }

    fn quoted(&mut self, limit: usize) -> Option<String> {
        if !self.take("\"") {
            return None;
        }
        let end = self.rest().find('"')?;
        let value = self.rest().get(..end)?;
        if value.len() > limit || value.contains(['\r', '\n']) {
            return None;
        }
        let value = value.to_owned();
        self.pos += end + 1;
        Some(value)
    }

    fn name(&mut self, limit: usize, identity: bool) -> Option<String> {
        if self.rest().starts_with('"') {
            let value = self.quoted(limit)?;
            return (!value.trim().is_empty()).then_some(value);
        }
        let start = self.pos;
        while let Some(ch) = self.rest().chars().next() {
            if !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '*')) {
                break;
            }
            self.pos += ch.len_utf8();
        }
        let value = &self.source[start..self.pos];
        if value.is_empty()
            || value.len() > limit
            || (identity
                && matches!(
                    value,
                    "erDiagram" | "direction" | "style" | "classDef" | "class" | "click"
                        | "subgraph" | "end" | "accTitle" | "accDescr" | "title"
                ))
        {
            return None;
        }
        Some(value.to_owned())
    }

    fn cardinality(&mut self, left: bool) -> Option<Cardinality> {
        let spellings = if left {
            ["|o", "||", "}o", "}|"]
        } else {
            ["o|", "||", "o{", "|{"]
        };
        for (spelling, cardinality) in spellings.into_iter().zip([
            Cardinality::ZeroOrOne,
            Cardinality::One,
            Cardinality::ZeroOrMany,
            Cardinality::OneOrMany,
        ]) {
            if self.take(spelling) {
                return Some(cardinality);
            }
        }
        None
    }

    fn attribute_word(&mut self, kind: bool) -> Option<String> {
        let start = self.pos;
        while let Some(ch) = self.rest().chars().next() {
            if ch.is_whitespace() || matches!(ch, '"' | '}' | '%') {
                break;
            }
            self.pos += ch.len_utf8();
        }
        let value = &self.source[start..self.pos];
        let stem = if kind {
            value.strip_suffix('?').unwrap_or(value)
        } else {
            value.strip_prefix('*').unwrap_or(value)
        };
        if value.len() > MAX_ID_BYTES
            || stem.chars().next().is_none_or(|ch| !(ch.is_alphabetic() || ch == '_'))
            || !stem.chars().all(|ch| {
                ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | ',' | '[' | ']' | '(' | ')')
            })
        {
            return None;
        }
        Some(value.to_owned())
    }

    fn key(&mut self) -> Option<&'static str> {
        for key in ["PK", "FK", "UK"] {
            if self.rest().strip_prefix(key).is_some_and(|tail| {
                tail.is_empty()
                    || tail.starts_with(char::is_whitespace)
                    || tail.starts_with([',', '"', '}'])
                    || tail.starts_with("%%")
            }) {
                self.pos += key.len();
                return Some(key);
            }
        }
        None
    }
}

// Each ASCII character reserves a full em, and non-ASCII characters reserve
// two ems, including wide W/M glyphs in the fixed SVG font family. Wrapped lines
// are source slices: no text is omitted, ellipsized or reinterpreted as markup.
fn units(text: &str) -> usize {
    text.chars().map(|ch| if ch.is_ascii() { 1 } else { 2 }).sum()
}

fn wrap(text: &str, columns: usize) -> Vec<String> {
    // XML/browser tab stops vary; four explicit spaces keep native geometry
    // and browser text identical without changing the entity lookup identity.
    let text = text.replace('\t', "    ");
    let mut result = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (offset, ch) in text.char_indices() {
        let width = if ch.is_ascii() { 1 } else { 2 };
        if used + width > columns {
            result.push(text[start..offset].to_owned());
            start = offset;
            used = 0;
        }
        used += width;
    }
    result.push(text[start..].to_owned());
    result
}

struct EntityBox {
    title: Vec<String>,
    rows: Vec<[Vec<String>; 4]>,
    columns: [f32; 4],
    row_heights: Vec<f32>,
    header_height: f32,
    width: f32,
    height: f32,
}

impl EntityBox {
    fn new(entity: &Entity) -> Self {
        let title = wrap(&entity.label, 48);
        let mut columns = [0.0_f32; 4];
        let keys = entity.attributes.iter().any(|attribute| !attribute.keys.is_empty());
        let comments = entity.attributes.iter().any(|attribute| !attribute.comment.is_empty());
        let mut rows = Vec::new();
        let mut row_heights = Vec::new();
        for attribute in &entity.attributes {
            let row = [
                wrap(&attribute.kind, 24),
                wrap(&attribute.name, 32),
                vec![attribute.keys.join(", ")],
                wrap(&attribute.comment, 40),
            ];
            for (index, lines) in row.iter().enumerate() {
                if index < 2 || (index == 2 && keys) || (index == 3 && comments) {
                    let width = lines.iter().map(|line| units(line)).max().unwrap_or(0);
                    columns[index] = columns[index].max(width as f32 * 12.0 + CELL_PAD * 2.0);
                }
            }
            let lines = row.iter().map(Vec::len).max().unwrap_or(1);
            row_heights.push(lines as f32 * LINE_HEIGHT + 12.0);
            rows.push(row);
        }
        let title_width = title.iter().map(|line| units(line)).max().unwrap_or(0) as f32 * 13.0 + 24.0;
        let width = title_width.max(columns.iter().sum()).max(120.0);
        if !rows.is_empty() {
            columns[1] += width - columns.iter().sum::<f32>();
        }
        let header_height = title.len() as f32 * LINE_HEIGHT + 16.0;
        let height = header_height + row_heights.iter().sum::<f32>();
        Self { title, rows, columns, row_heights, header_height, width, height }
    }
}

pub(super) fn render(source: &str) -> Option<String> {
    let diagram = parse(source)?;
    let mut boxes = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    for id in &diagram.order {
        let entity = diagram.entities.get(id)?;
        let measured = EntityBox::new(entity);
        nodes.insert(id.clone(), FlowNode {
            label: entity.label.clone(),
            shape: NodeShape::Rectangle,
            layer: 0,
            x: 0.0,
            y: 0.0,
            width: measured.width + PORT_SPACE * 2.0,
            height: measured.height + PORT_SPACE * 2.0,
        });
        boxes.insert(id.clone(), measured);
    }
    let edges: Vec<_> = diagram.relationships.iter().map(|relation| relation.edge.clone()).collect();
    super::assign_flowchart_layers(&mut nodes, &diagram.order, &edges);
    let (mut width, mut height) =
        super::layout_flowchart_nodes(&mut nodes, &diagram.order, &edges, diagram.dir);
    let routes =
        super::layout_flowchart_returns(&mut nodes, &edges, diagram.dir, &mut width, &mut height);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-er\" role=\"img\" font-family=\"Arial,Helvetica,sans-serif\" xml:space=\"preserve\">\n<title>Entity relationship diagram</title>\n<desc>",
        SvgInt(width), SvgInt(height), SvgInt(width), SvgInt(height)
    );
    for relationship in &diagram.relationships {
        let from = &diagram.entities.get(&relationship.edge.from)?.label;
        let to = &diagram.entities.get(&relationship.edge.to)?.label;
        svg.push_str(&escape_svg_text(&format!(
            "{from} ({}) to {to} ({}), {}: {}. ",
            relationship.from_card.name(),
            relationship.to_card.name(),
            if relationship.edge.dashed { "non-identifying" } else { "identifying" },
            relationship.edge.label.as_deref().unwrap_or("")
        )));
    }
    svg.push_str("</desc>\n");
    svg.push_str(r#"<style>
.fmd-er-entity-bg { fill: var(--bg-canvas, #ffffff); }
.fmd-er-entity-header { fill: var(--bg-subtle, #f6f8fa); }
.fmd-er-entity-stripe { fill: var(--bg-subtle, #f6f8fa); }
.fmd-er-entity-border, .fmd-er-cell-rule { fill: none; stroke: var(--border-default, #d0d7de); stroke-width: 1; }
.fmd-er-title { fill: var(--fg-default, #1f2328); font-size: 13px; font-weight: 600; text-anchor: middle; }
.fmd-er-attribute { fill: var(--fg-default, #1f2328); font-size: 12px; }
.fmd-er-edge, .fmd-er-cardinality { fill: none; stroke: var(--fg-muted, #57606a); stroke-width: 1.5; }
.fmd-er-cardinality-circle { fill: var(--bg-canvas, #ffffff); stroke: var(--fg-muted, #57606a); stroke-width: 1.5; }
.fmd-er-label-bg { fill: var(--bg-canvas, #ffffff); }
.fmd-er-label { fill: var(--fg-muted, #57606a); font-size: 11px; text-anchor: middle; }
@media (prefers-color-scheme: dark) {
.fmd-er-entity-bg, .fmd-er-cardinality-circle, .fmd-er-label-bg { fill: #161b22; }
.fmd-er-entity-header, .fmd-er-entity-stripe { fill: #21262d; }
.fmd-er-entity-border, .fmd-er-cell-rule { stroke: #30363d; }
.fmd-er-title, .fmd-er-attribute { fill: #e6edf3; }
.fmd-er-edge, .fmd-er-cardinality, .fmd-er-cardinality-circle { stroke: #8b949e; }
.fmd-er-label { fill: #8b949e; }
}
</style>
"#);
    let axis = match diagram.dir {
        FlowDirection::TopToBottom => (0.0, 1.0),
        FlowDirection::BottomToTop => (0.0, -1.0),
        FlowDirection::LeftToRight => (1.0, 0.0),
        FlowDirection::RightToLeft => (-1.0, 0.0),
    };
    for (relationship, route) in diagram.relationships.iter().zip(routes) {
        let edge = &relationship.edge;
        let from = nodes.get(&edge.from)?;
        let to = nodes.get(&edge.to)?;
        let (x1, y1, x2, y2) = super::get_edge_endpoints(from, to, diagram.dir);
        let dash = if edge.dashed { " stroke-dasharray=\"4,4\"" } else { "" };
        let (label_x, label_y, label_width) = if let Some(route) = route {
            let [a, b, c, d, e, f] = route.points;
            svg.push_str(&format!(
                "<path d=\"M {} {} L {} {} L {} {} L {} {} L {} {} L {} {}\" class=\"fmd-er-edge\"{dash} />\n",
                SvgNum(a.0), SvgNum(a.1), SvgNum(b.0), SvgNum(b.1),
                SvgNum(c.0), SvgNum(c.1), SvgNum(d.0), SvgNum(d.1),
                SvgNum(e.0), SvgNum(e.1), SvgNum(f.0), SvgNum(f.1)
            ));
            (route.label.0, route.label.1, route.label_width)
        } else {
            let (cx1, cy1, cx2, cy2) = if axis.0 == 0.0 {
                (x1, (y1 + y2) / 2.0, x2, (y1 + y2) / 2.0)
            } else {
                ((x1 + x2) / 2.0, y1, (x1 + x2) / 2.0, y2)
            };
            svg.push_str(&format!(
                "<path d=\"M {} {} C {} {}, {} {}, {} {}\" class=\"fmd-er-edge\"{dash} />\n",
                SvgNum(x1), SvgNum(y1), SvgNum(cx1), SvgNum(cy1),
                SvgNum(cx2), SvgNum(cy2), SvgNum(x2), SvgNum(y2)
            ));
            ((x1 + x2) / 2.0, (y1 + y2) / 2.0,
                super::flowchart_label_width(edge.label.as_deref().unwrap_or("")))
        };
        endpoint(
            &mut svg,
            (x1 - axis.0 * PORT_SPACE, y1 - axis.1 * PORT_SPACE),
            axis,
            relationship.from_card,
            dash,
        );
        endpoint(
            &mut svg,
            (x2 + axis.0 * PORT_SPACE, y2 + axis.1 * PORT_SPACE),
            (-axis.0, -axis.1),
            relationship.to_card,
            dash,
        );
        if let Some(label) = edge.label.as_deref().filter(|label| !label.is_empty()) {
            svg.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"16\" class=\"fmd-er-label-bg\" />\n<text x=\"{}\" y=\"{}\" class=\"fmd-er-label\">{}</text>\n",
                SvgNum(label_x - label_width / 2.0), SvgNum(label_y - 8.0),
                SvgNum(label_width), SvgNum(label_x), SvgNum(label_y + 4.0),
                escape_svg_text(label)
            ));
        }
    }
    for id in &diagram.order {
        draw_entity(&mut svg, nodes.get(id)?, boxes.get(id)?);
    }
    svg.push_str("</svg>\n");
    Some(svg)
}

fn endpoint(svg: &mut String, origin: (f32, f32), axis: (f32, f32), card: Cardinality, dash: &str) {
    let point = |distance: f32, cross: f32| (
        origin.0 + axis.0 * distance - axis.1 * cross,
        origin.1 + axis.1 * distance + axis.0 * cross,
    );
    let line = |svg: &mut String, from: (f32, f32), to: (f32, f32), extra: &str| {
        svg.push_str(&format!(
            "<path d=\"M {} {} L {} {}\" class=\"fmd-er-cardinality\"{extra} />\n",
            SvgNum(from.0), SvgNum(from.1), SvgNum(to.0), SvgNum(to.1)
        ));
    };
    let metadata = format!("{dash} data-cardinality=\"{}\"", card.name());
    line(svg, point(0.0, 0.0), point(PORT_SPACE, 0.0), &metadata);
    if card.many() {
        let a = point(2.0, -6.0);
        let b = point(12.0, 0.0);
        let c = point(2.0, 6.0);
        svg.push_str(&format!(
            "<path d=\"M {} {} L {} {} L {} {}\" class=\"fmd-er-cardinality\" />\n",
            SvgNum(a.0), SvgNum(a.1), SvgNum(b.0), SvgNum(b.1), SvgNum(c.0), SvgNum(c.1)
        ));
    } else {
        line(svg, point(6.0, -6.0), point(6.0, 6.0), "");
    }
    if card.optional() {
        let center = point(20.0, 0.0);
        svg.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"4\" class=\"fmd-er-cardinality-circle\" />\n",
            SvgNum(center.0), SvgNum(center.1)
        ));
    } else {
        line(svg, point(19.0, -6.0), point(19.0, 6.0), "");
    }
}

fn draw_entity(svg: &mut String, node: &FlowNode, measured: &EntityBox) {
    let (x, y) = (node.x + PORT_SPACE, node.y + PORT_SPACE);
    rect(svg, x, y, measured.width, measured.height, "fmd-er-entity-bg");
    rect(svg, x, y, measured.width, measured.header_height, "fmd-er-entity-header");
    for (line, text) in measured.title.iter().enumerate() {
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" class=\"fmd-er-title\">{}</text>\n",
            SvgNum(x + measured.width / 2.0), SvgNum(y + 22.0 + line as f32 * LINE_HEIGHT),
            escape_svg_text(text)
        ));
    }
    let mut row_y = y + measured.header_height;
    for (index, (row, height)) in measured.rows.iter().zip(&measured.row_heights).enumerate() {
        if index % 2 == 1 {
            rect(svg, x, row_y, measured.width, *height, "fmd-er-entity-stripe");
        }
        rule(svg, (x, row_y), (x + measured.width, row_y));
        let mut cell_x = x;
        for (lines, width) in row.iter().zip(measured.columns) {
            if width == 0.0 {
                continue;
            }
            for (line, text) in lines.iter().enumerate() {
                if !text.is_empty() {
                    svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" class=\"fmd-er-attribute\">{}</text>\n",
                        SvgNum(cell_x + CELL_PAD), SvgNum(row_y + 20.0 + line as f32 * LINE_HEIGHT),
                        escape_svg_text(text)
                    ));
                }
            }
            cell_x += width;
        }
        row_y += height;
    }
    let mut cell_x = x;
    for width in measured.columns.into_iter().filter(|width| *width > 0.0) {
        if cell_x > x {
            rule(svg, (cell_x, y + measured.header_height), (cell_x, y + measured.height));
        }
        cell_x += width;
    }
    rect(svg, x, y, measured.width, measured.height, "fmd-er-entity-border");
}

fn rect(svg: &mut String, x: f32, y: f32, width: f32, height: f32, class: &str) {
    svg.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"{class}\" />\n",
        SvgNum(x), SvgNum(y), SvgNum(width), SvgNum(height)
    ));
}

fn rule(svg: &mut String, start: (f32, f32), end: (f32, f32)) {
    svg.push_str(&format!(
        "<path d=\"M {} {} L {} {}\" class=\"fmd-er-cell-rule\" />\n",
        SvgNum(start.0), SvgNum(start.1), SvgNum(end.0), SvgNum(end.1)
    ));
}
