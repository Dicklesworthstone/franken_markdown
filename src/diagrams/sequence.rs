//! Bounded sequence diagrams shared by the native publication backends.
//!
//! Parse the entire diagram before drawing it. Unsupported or malformed syntax
//! returns the original fence to the caller instead of dropping an interaction
//! or making alternative branches look like an unconditional sequence.

use super::{SvgNum, escape_svg_text};
use std::collections::BTreeMap;

const MAX_SOURCE: usize = 64 * 1024;
const MAX_PARTICIPANTS: usize = 64;
const MAX_EVENTS: usize = 1024;
const MAX_DEPTH: usize = 16;
const MAX_ACTIVATIONS: usize = 32;
const MAX_LABEL: usize = 1024;
const MARGIN: f32 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArrowHead {
    None,
    Filled,
    Open,
    Cross,
    Both,
}

#[derive(Clone, Copy, Debug)]
struct Arrow {
    head: ArrowHead,
    dashed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fragment {
    Loop,
    Alt,
    Opt,
    Par,
    Critical,
    Break,
}

impl Fragment {
    fn name(self) -> &'static str {
        match self {
            Self::Loop => "loop",
            Self::Alt => "alt",
            Self::Opt => "opt",
            Self::Par => "par",
            Self::Critical => "critical",
            Self::Break => "break",
        }
    }

    fn branch(self) -> Option<&'static str> {
        match self {
            Self::Alt => Some("else"),
            Self::Par => Some("and"),
            Self::Critical => Some("option"),
            _ => None,
        }
    }
}

struct Participant {
    label: String,
    actor: bool,
    active: usize,
    max_active: usize,
}

#[derive(Clone, Copy)]
enum NoteSide {
    Left,
    Right,
    Over,
}

enum Event {
    Message {
        from: usize,
        to: usize,
        text: String,
        arrow: Arrow,
        activation: i8,
    },
    Note {
        from: usize,
        to: usize,
        side: NoteSide,
        text: String,
    },
    Activate(usize),
    Deactivate(usize),
    Start(Fragment, String),
    Branch(&'static str, String),
    End,
}

struct Diagram {
    participants: Vec<Participant>,
    indices: BTreeMap<String, usize>,
    events: Vec<Event>,
    title: Option<String>,
    accessible_title: Option<String>,
    description: Option<String>,
    max_depth: usize,
}

pub(super) fn is_header(source: &str) -> bool {
    source
        .split(|ch: char| ch.is_whitespace() || ch == ';')
        .next()
        == Some("sequenceDiagram")
}

fn word<'a>(source: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = source.strip_prefix(keyword)?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim_start())
}

// Mermaid permits semicolon-separated statements and uses #59; for a literal
// semicolon. Protect complete character references before splitting statements.
fn statements(source: &str) -> Option<Vec<&str>> {
    if source.len() > MAX_SOURCE
        || source.chars().any(|ch| {
            (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                || matches!(ch, '\u{fffe}' | '\u{ffff}')
        })
    {
        return None;
    }
    let mut result = Vec::new();
    for line in source.split(['\n', '\r']) {
        let mut rest = line.trim();
        while !rest.is_empty() {
            if rest.starts_with("%%{") {
                return None;
            }
            if rest.starts_with("%%") {
                break;
            }
            let bytes = rest.as_bytes();
            let mut pos = 0;
            while pos < bytes.len() {
                if matches!(bytes[pos], b'#' | b'&') {
                    let tail = &rest[pos + 1..];
                    if let Some(end) = tail
                        .bytes()
                        .take(33)
                        .position(|byte| byte == b';')
                        .filter(|&end| end > 0)
                        && tail[..end]
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'#')
                    {
                        pos += end + 2;
                        continue;
                    }
                }
                if bytes[pos] == b';' {
                    break;
                }
                pos += 1;
            }
            let statement = rest[..pos].trim();
            if !statement.is_empty() {
                result.push(statement);
                // Declarations/metadata are bounded too, even if they emit no row.
                if result.len() > MAX_EVENTS + MAX_PARTICIPANTS + 16 {
                    return None;
                }
            }
            rest = rest.get(pos + 1..).unwrap_or("").trim_start();
        }
    }
    Some(result)
}

fn label(source: &str) -> Option<String> {
    if source.len() > MAX_LABEL {
        return None;
    }
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while !rest.is_empty() {
        if rest.starts_with('<')
            && let Some(end) = rest.find('>').filter(|&end| end <= 5)
            && matches!(
                rest[..=end].to_ascii_lowercase().as_str(),
                "<br>" | "<br/>" | "<br />"
            )
        {
            out.push('\n');
            rest = &rest[end + 1..];
            continue;
        }
        if (rest.starts_with('#') || rest.starts_with('&'))
            && let Some(end) = rest.find(';').filter(|&end| end > 1 && end <= 33)
        {
            let body = &rest[1..end];
            let numeric = if rest.starts_with('#') {
                body.bytes().all(|b| b.is_ascii_digit()).then_some(body)
            } else {
                body.strip_prefix('#')
            };
            if let Some(number) = numeric {
                let ch = char::from_u32(number.parse().ok()?)?;
                if ch.is_control() || matches!(ch, '\u{fffe}' | '\u{ffff}') {
                    return None;
                }
                out.push(ch);
                rest = &rest[end + 1..];
                continue;
            }
            if let Some(value) = crate::parse::entities::lookup(body) {
                out.push_str(value);
                rest = &rest[end + 1..];
                continue;
            }
        }
        let ch = rest.chars().next()?;
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    Some(out)
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '.' | '-'))
}

impl Diagram {
    fn participant(&mut self, id: &str) -> Option<usize> {
        if !identifier(id) {
            return None;
        }
        if let Some(&index) = self.indices.get(id) {
            return Some(index);
        }
        if self.participants.len() == MAX_PARTICIPANTS {
            return None;
        }
        let index = self.participants.len();
        self.indices.insert(id.to_owned(), index);
        self.participants.push(Participant {
            label: id.to_owned(),
            actor: false,
            active: 0,
            max_active: 0,
        });
        Some(index)
    }

    fn activate(&mut self, index: usize) -> Option<()> {
        let participant = &mut self.participants[index];
        if participant.active == MAX_ACTIVATIONS {
            return None;
        }
        participant.active += 1;
        participant.max_active = participant.max_active.max(participant.active);
        Some(())
    }

    fn deactivate(&mut self, index: usize) -> Option<()> {
        self.participants[index].active = self.participants[index].active.checked_sub(1)?;
        Some(())
    }
}

fn message(source: &str) -> Option<(&str, &str, &str, Arrow, i8)> {
    let (head, text) = source.split_once(':')?;
    const ARROWS: [(&str, ArrowHead, bool); 10] = [
        ("<<-->>", ArrowHead::Both, true),
        ("<<->>", ArrowHead::Both, false),
        ("-->>", ArrowHead::Filled, true),
        ("->>", ArrowHead::Filled, false),
        ("-->", ArrowHead::None, true),
        ("->", ArrowHead::None, false),
        ("--x", ArrowHead::Cross, true),
        ("-x", ArrowHead::Cross, false),
        ("--)", ArrowHead::Open, true),
        ("-)", ArrowHead::Open, false),
    ];
    for (pos, _) in head.char_indices() {
        for (token, arrow_head, dashed) in ARROWS {
            if let Some(rest) = head[pos..].strip_prefix(token) {
                let from = head[..pos].trim();
                let rest = rest.trim();
                let (activation, to) = if let Some(to) = rest.strip_prefix('+') {
                    (1, to.trim())
                } else if let Some(to) = rest.strip_prefix('-') {
                    (-1, to.trim())
                } else {
                    (0, rest)
                };
                return Some((
                    from,
                    to,
                    text.trim(),
                    Arrow {
                        head: arrow_head,
                        dashed,
                    },
                    activation,
                ));
            }
        }
    }
    None
}

fn parse(source: &str) -> Option<Diagram> {
    let statements = statements(source)?;
    let mut diagram = Diagram {
        participants: Vec::new(),
        indices: BTreeMap::new(),
        events: Vec::new(),
        title: None,
        accessible_title: None,
        description: None,
        max_depth: 0,
    };
    let mut stack: Vec<Fragment> = Vec::new();
    let mut numbering: Option<(u32, u32)> = None;
    for (index, statement) in statements.into_iter().enumerate() {
        if statement == "sequenceDiagram" && index == 0 {
            continue;
        }
        if let Some(rest) = word(statement, "participant").or_else(|| word(statement, "actor")) {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let id = &rest[..end];
            let suffix = rest[end..].trim();
            let display = if suffix.is_empty() {
                id
            } else {
                word(suffix, "as")?
            };
            let participant = diagram.participant(id)?;
            if display.trim().is_empty() {
                return None;
            }
            diagram.participants[participant].label = label(display.trim())?;
            diagram.participants[participant].actor = word(statement, "actor").is_some();
            continue;
        }
        if let Some(rest) = word(statement, "autonumber") {
            let values = rest.split_whitespace().collect::<Vec<_>>();
            numbering = Some(match values.as_slice() {
                [] => (1, 1),
                [start] => (start.parse::<u32>().ok()?, 1),
                [start, step] => {
                    let step = step.parse::<u32>().ok()?;
                    if step == 0 {
                        return None;
                    }
                    (start.parse::<u32>().ok()?, step)
                }
                _ => return None,
            });
            continue;
        }
        let metadata = if let Some(rest) =
            word(statement, "title").or_else(|| statement.strip_prefix("title:"))
        {
            Some((&mut diagram.title, rest))
        } else if let Some(rest) = statement.strip_prefix("accTitle:") {
            Some((&mut diagram.accessible_title, rest))
        } else {
            statement
                .strip_prefix("accDescr:")
                .map(|rest| (&mut diagram.description, rest))
        };
        if let Some((slot, value)) = metadata {
            if value.trim().is_empty() || slot.replace(label(value.trim())?).is_some() {
                return None;
            }
            continue;
        }
        let event = if let Some(id) = word(statement, "activate") {
            let participant = diagram.participant(id)?;
            diagram.activate(participant)?;
            Event::Activate(participant)
        } else if let Some(id) = word(statement, "deactivate") {
            let participant = diagram.participant(id)?;
            diagram.deactivate(participant)?;
            Event::Deactivate(participant)
        } else if let Some((kind, text)) = [
            Fragment::Loop,
            Fragment::Alt,
            Fragment::Opt,
            Fragment::Par,
            Fragment::Critical,
            Fragment::Break,
        ]
        .into_iter()
        .find_map(|kind| word(statement, kind.name()).map(|text| (kind, text)))
        {
            if stack.len() == MAX_DEPTH {
                return None;
            }
            stack.push(kind);
            diagram.max_depth = diagram.max_depth.max(stack.len());
            Event::Start(kind, label(text)?)
        } else if statement == "end" {
            stack.pop()?;
            Event::End
        } else if let Some((kind, text)) = ["else", "and", "option"]
            .into_iter()
            .find_map(|kind| word(statement, kind).map(|text| (kind, text)))
        {
            if stack.last()?.branch() != Some(kind) {
                return None;
            }
            Event::Branch(kind, label(text)?)
        } else if let Some(note) = word(statement, "Note").or_else(|| word(statement, "note")) {
            let (targets, text) = note.split_once(':')?;
            let (side, ids) = if let Some(ids) = word(targets, "over") {
                (NoteSide::Over, ids)
            } else if let Some(ids) = word(targets, "left of") {
                (NoteSide::Left, ids)
            } else {
                (NoteSide::Right, word(targets, "right of")?)
            };
            let (first, second) = ids.split_once(',').unwrap_or((ids, ids));
            if !matches!(side, NoteSide::Over) && first != second {
                return None;
            }
            let from = diagram.participant(first.trim())?;
            let to = diagram.participant(second.trim())?;
            Event::Note {
                from,
                to,
                side,
                text: label(text.trim())?,
            }
        } else {
            let (from, to, text, arrow, activation) = message(statement)?;
            let from = diagram.participant(from)?;
            let to = diagram.participant(to)?;
            if activation > 0 {
                diagram.activate(to)?;
            }
            if activation < 0 {
                diagram.deactivate(from)?;
            }
            let mut text = label(text)?;
            if let Some((next, step)) = numbering.as_mut() {
                text = format!("{next}. {text}");
                *next = next.checked_add(*step)?;
            }
            Event::Message {
                from,
                to,
                text,
                arrow,
                activation,
            }
        };
        if diagram.events.len() == MAX_EVENTS {
            return None;
        }
        diagram.events.push(event);
    }
    (!diagram.participants.is_empty() && stack.is_empty()).then_some(diagram)
}

struct Text {
    lines: Vec<String>,
    width: f32,
    size: f32,
}

fn units(text: &str) -> usize {
    text.chars()
        .map(|ch| if ch.is_ascii() { 1 } else { 2 })
        .sum()
}

impl Text {
    fn new(text: &str, columns: usize, size: f32) -> Self {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let (mut start, mut used, mut space) = (0, 0, None);
            for (index, ch) in paragraph.char_indices() {
                let advance = if ch.is_ascii() { 1 } else { 2 };
                if used + advance > columns {
                    let end = space.filter(|&pos| pos > start).unwrap_or(index);
                    lines.push(paragraph[start..end].to_owned());
                    start = end;
                    used = units(&paragraph[start..index]);
                    space = None;
                }
                used += advance;
                if ch.is_whitespace() {
                    space = Some(index + ch.len_utf8());
                }
            }
            lines.push(paragraph[start..].to_owned());
        }
        let width = lines.iter().map(|line| units(line)).max().unwrap_or(0) as f32 * size;
        Self { lines, width, size }
    }

    fn height(&self) -> f32 {
        self.lines.len() as f32 * (self.size + 4.0)
    }

    fn draw(&self, svg: &mut String, x: f32, y: f32, anchor: &str, class: &str) {
        for (index, line) in self.lines.iter().enumerate() {
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" font-size=\"{}\" class=\"{class}\">{}</text>\n",
                SvgNum(x), SvgNum(y + self.size + index as f32 * (self.size + 4.0)),
                SvgNum(self.size), escape_svg_text(line)
            ));
        }
    }
}

struct Frame {
    y: f32,
    bottom: f32,
    depth: usize,
}

struct Bar {
    participant: usize,
    depth: usize,
    y: f32,
    bottom: f32,
}

enum Row {
    Message {
        x1: f32,
        x2: f32,
        y: f32,
        text_y: f32,
        text: Text,
        arrow: Arrow,
        self_message: bool,
    },
    Note {
        x: f32,
        y: f32,
        width: f32,
        text: Text,
    },
    FrameText {
        frame: usize,
        y: f32,
        text: Text,
        branch: bool,
    },
}

fn bar_x(participant: usize, xs: &[f32], active: &[Vec<usize>]) -> f32 {
    xs[participant] + active[participant].len().saturating_sub(1) as f32 * 6.0
}

fn begin_bar(participant: usize, y: f32, active: &mut [Vec<usize>], bars: &mut Vec<Bar>) {
    let index = bars.len();
    bars.push(Bar {
        participant,
        depth: active[participant].len(),
        y,
        bottom: y,
    });
    active[participant].push(index);
}

fn end_bar(participant: usize, y: f32, active: &mut [Vec<usize>], bars: &mut [Bar]) {
    if let Some(index) = active[participant].pop() {
        bars[index].bottom = y.max(bars[index].y + 10.0);
    }
}

fn arrowhead(svg: &mut String, x: f32, y: f32, direction: f32, head: ArrowHead) {
    if head == ArrowHead::None {
        return;
    }
    let back = x - direction * 8.0;
    match head {
        ArrowHead::Cross => {
            svg.push_str(&format!(
                "<path d=\"M {} {} L {} {} M {} {} L {} {}\" class=\"fmd-seq-arrow-open\"/>\n",
                SvgNum(x - 4.0),
                SvgNum(y - 4.0),
                SvgNum(x + 4.0),
                SvgNum(y + 4.0),
                SvgNum(x - 4.0),
                SvgNum(y + 4.0),
                SvgNum(x + 4.0),
                SvgNum(y - 4.0)
            ));
        }
        ArrowHead::Open => {
            svg.push_str(&format!(
                "<path d=\"M {} {} L {} {} L {} {}\" class=\"fmd-seq-arrow-open\"/>\n",
                SvgNum(back),
                SvgNum(y - 4.5),
                SvgNum(x),
                SvgNum(y),
                SvgNum(back),
                SvgNum(y + 4.5)
            ));
        }
        _ => {
            svg.push_str(&format!(
                "<path d=\"M {} {} L {} {} L {} {} Z\" class=\"fmd-seq-arrow\"/>\n",
                SvgNum(back),
                SvgNum(y - 4.5),
                SvgNum(x),
                SvgNum(y),
                SvgNum(back),
                SvgNum(y + 4.5)
            ));
        }
    }
}

pub(super) fn render(source: &str) -> Option<String> {
    let diagram = parse(source)?;
    let labels: Vec<_> = diagram
        .participants
        .iter()
        .map(|part| Text::new(&part.label, 18, 13.0))
        .collect();
    let widths: Vec<_> = labels
        .iter()
        .map(|text| (text.width + 24.0).max(110.0))
        .collect();
    let heights: Vec<_> = labels
        .iter()
        .zip(&diagram.participants)
        .map(|(text, part)| text.height() + if part.actor { 62.0 } else { 20.0 })
        .collect();
    let header_height = heights.iter().copied().fold(36.0, f32::max);
    let title = diagram
        .title
        .as_ref()
        .map(|title| Text::new(title, 56, 18.0));
    let header_top = MARGIN + title.as_ref().map_or(0.0, |title| title.height() + 16.0);
    let header_bottom = header_top + header_height;
    let mut xs = Vec::new();
    let mut next_x = 0.0;
    for (index, width) in widths.iter().enumerate() {
        xs.push(next_x + width / 2.0);
        next_x +=
            width + 60.0 + diagram.participants[index].max_active.saturating_sub(1) as f32 * 6.0;
    }
    // Each constraint moves only a suffix, so satisfying a later message cannot
    // reduce a previously admitted distance between participant columns.
    for event in &diagram.events {
        if let Event::Message { from, to, text, .. } = event
            && from != to
        {
            let required = Text::new(text, 42, 12.0).width + 48.0;
            let (left, right) = ((*from).min(*to), (*from).max(*to));
            let extra = (required - (xs[right] - xs[left])).max(0.0);
            for x in &mut xs[right..] {
                *x += extra;
            }
        }
    }
    let mut left = xs[0] - widths[0] / 2.0;
    let mut right = xs
        .iter()
        .zip(&widths)
        .zip(&diagram.participants)
        .map(|((x, width), part)| (x + width / 2.0).max(x + part.max_active as f32 * 6.0 + 12.0))
        .fold(0.0, f32::max);
    let mut rows = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut frame_stack: Vec<usize> = Vec::new();
    let mut bars = Vec::new();
    let mut active = vec![Vec::new(); xs.len()];
    let mut y = header_bottom + 24.0;
    let mut last_message = header_bottom + 8.0;
    let mut frame_label_width = 0.0f32;
    for event in &diagram.events {
        match event {
            Event::Message {
                from,
                to,
                text,
                arrow,
                activation,
            } => {
                let text = Text::new(text, 42, 12.0);
                let self_message = from == to;
                let message_y = y + text.height() + 5.0;
                let end_y = message_y + if self_message { 26.0 } else { 0.0 };
                let direction = if from <= to { 1.0 } else { -1.0 };
                let x1 = bar_x(*from, &xs, &active)
                    + if active[*from].is_empty() {
                        0.0
                    } else {
                        direction * 6.0
                    };
                if *activation > 0 {
                    begin_bar(*to, end_y, &mut active, &mut bars);
                }
                let incoming = if self_message { 1.0 } else { -direction };
                let x2 = bar_x(*to, &xs, &active)
                    + if active[*to].is_empty() {
                        0.0
                    } else {
                        incoming * 6.0
                    };
                if self_message {
                    right = right.max(x1.max(x2) + 60.0).max(x1 + 18.0 + text.width);
                } else {
                    let center = (x1 + x2) / 2.0;
                    left = left.min(center - text.width / 2.0);
                    right = right.max(center + text.width / 2.0);
                }
                rows.push(Row::Message {
                    x1,
                    x2,
                    y: message_y,
                    text_y: y,
                    text,
                    arrow: *arrow,
                    self_message,
                });
                if *activation < 0 {
                    end_bar(*from, end_y + 2.0, &mut active, &mut bars);
                }
                last_message = end_y;
                y = end_y + 24.0;
            }
            Event::Activate(participant) => {
                begin_bar(*participant, last_message, &mut active, &mut bars)
            }
            Event::Deactivate(participant) => {
                end_bar(*participant, last_message + 2.0, &mut active, &mut bars)
            }
            Event::Note {
                from,
                to,
                side,
                text,
            } => {
                let text = Text::new(text, 36, 11.0);
                let width = (text.width + 24.0).max(match side {
                    NoteSide::Over => (xs[*from] - xs[*to]).abs() + 60.0,
                    _ => 120.0,
                });
                let x = match side {
                    NoteSide::Left => xs[*from] - width - 24.0,
                    NoteSide::Right => xs[*from] + 24.0,
                    NoteSide::Over => (xs[*from] + xs[*to]) / 2.0 - width / 2.0,
                };
                left = left.min(x);
                right = right.max(x + width);
                let height = text.height() + 20.0;
                rows.push(Row::Note { x, y, width, text });
                last_message = y + height;
                y += height + 20.0;
            }
            Event::Start(kind, label) => {
                let text = Text::new(&format!("[{}] {label}", kind.name()), 56, 12.0);
                frame_label_width = frame_label_width.max(text.width + 24.0);
                let index = frames.len();
                frames.push(Frame {
                    y: y - 8.0,
                    bottom: y,
                    depth: frame_stack.len(),
                });
                frame_stack.push(index);
                let height = text.height() + 20.0;
                rows.push(Row::FrameText {
                    frame: index,
                    y,
                    text,
                    branch: false,
                });
                y += height;
                last_message = y;
            }
            Event::Branch(kind, label) => {
                let text = Text::new(&format!("[{kind}] {label}"), 56, 12.0);
                frame_label_width = frame_label_width.max(text.width + 24.0);
                let height = text.height() + 28.0;
                rows.push(Row::FrameText {
                    frame: *frame_stack.last()?,
                    y: y + 8.0,
                    text,
                    branch: true,
                });
                y += height;
                last_message = y;
            }
            Event::End => {
                y += 8.0;
                frames[frame_stack.pop()?].bottom = y;
                last_message = y;
                y += 18.0;
            }
        }
    }
    let bottom = y + 8.0;
    for participant in 0..active.len() {
        while !active[participant].is_empty() {
            end_bar(participant, bottom, &mut active, &mut bars);
        }
    }
    let minimum_width = frame_label_width.max(title.as_ref().map_or(0.0, |title| title.width));
    if right - left < minimum_width {
        let extra = (minimum_width - (right - left)) / 2.0;
        left -= extra;
        right += extra;
    }
    let frame_padding = if frames.is_empty() {
        0.0
    } else {
        20.0 + diagram.max_depth as f32 * 14.0
    };
    let offset = MARGIN + frame_padding - left;
    let width = right - left + 2.0 * (MARGIN + frame_padding);
    let height = bottom + header_height + MARGIN;
    let frame_edges = |frame: &Frame| {
        let pad = 20.0 + (diagram.max_depth - frame.depth) as f32 * 14.0;
        (left - pad + offset, right - left + 2.0 * pad)
    };
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-sequence\" role=\"img\" font-family=\"Arial,Helvetica,sans-serif\" xml:space=\"preserve\">\n<title>{}</title>\n",
        SvgNum(width),
        SvgNum(height),
        SvgNum(width),
        SvgNum(height),
        escape_svg_text(
            diagram
                .accessible_title
                .as_deref()
                .or(diagram.title.as_deref())
                .unwrap_or("Sequence diagram")
        )
    );
    if let Some(description) = &diagram.description {
        svg.push_str(&format!("<desc>{}</desc>\n", escape_svg_text(description)));
    }
    svg.push_str(STYLE);
    if let Some(title) = title {
        title.draw(&mut svg, width / 2.0, MARGIN, "middle", "fmd-seq-title");
    }
    for frame in &frames {
        let (x, frame_width) = frame_edges(frame);
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"fmd-seq-frame\"/>\n",
            SvgNum(x),
            SvgNum(frame.y),
            SvgNum(frame_width),
            SvgNum(frame.bottom - frame.y)
        ));
    }
    for x in &xs {
        svg.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-seq-lifeline\"/>\n",
            SvgNum(x + offset),
            SvgNum(header_bottom),
            SvgNum(x + offset),
            SvgNum(bottom)
        ));
    }
    for bar in bars {
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"12\" height=\"{}\" class=\"fmd-seq-activation\"/>\n",
            SvgNum(xs[bar.participant] + offset + bar.depth as f32 * 6.0 - 6.0),
            SvgNum(bar.y),
            SvgNum(bar.bottom - bar.y)
        ));
    }
    for row in rows {
        match row {
            Row::Message {
                x1,
                x2,
                y,
                text_y,
                text,
                arrow,
                self_message,
            } => {
                let (x1, x2) = (x1 + offset, x2 + offset);
                let dash = if arrow.dashed {
                    " stroke-dasharray=\"4,4\""
                } else {
                    ""
                };
                let (end_y, direction) = if self_message {
                    svg.push_str(&format!("<path d=\"M {} {} L {} {} L {} {} L {} {}\" class=\"fmd-seq-msg-line\"{dash}/>\n",
                        SvgNum(x1), SvgNum(y), SvgNum(x1.max(x2) + 60.0), SvgNum(y), SvgNum(x1.max(x2) + 60.0), SvgNum(y + 26.0), SvgNum(x2), SvgNum(y + 26.0)));
                    text.draw(&mut svg, x1 + 18.0, text_y, "start", "fmd-seq-msg-text");
                    (y + 26.0, -1.0)
                } else {
                    svg.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-seq-msg-line\"{dash}/>\n",
                        SvgNum(x1), SvgNum(y), SvgNum(x2), SvgNum(y)));
                    text.draw(
                        &mut svg,
                        (x1 + x2) / 2.0,
                        text_y,
                        "middle",
                        "fmd-seq-msg-text",
                    );
                    (y, if x2 > x1 { 1.0 } else { -1.0 })
                };
                arrowhead(&mut svg, x2, end_y, direction, arrow.head);
                if arrow.head == ArrowHead::Both {
                    arrowhead(
                        &mut svg,
                        x1,
                        y,
                        if self_message { -1.0 } else { -direction },
                        ArrowHead::Filled,
                    );
                }
            }
            Row::Note { x, y, width, text } => {
                svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"3\" class=\"fmd-seq-note\"/>\n",
                    SvgNum(x + offset), SvgNum(y), SvgNum(width), SvgNum(text.height() + 20.0)));
                text.draw(
                    &mut svg,
                    x + offset + width / 2.0,
                    y + 10.0,
                    "middle",
                    "fmd-seq-note-text",
                );
            }
            Row::FrameText {
                frame,
                y,
                text,
                branch,
            } => {
                let (x, frame_width) = frame_edges(&frames[frame]);
                if branch {
                    svg.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"fmd-seq-branch\"/>\n",
                        SvgNum(x), SvgNum(y - 10.0), SvgNum(x + frame_width), SvgNum(y - 10.0)));
                }
                text.draw(&mut svg, x + 12.0, y, "start", "fmd-seq-frame-text");
            }
        }
    }
    for (index, participant) in diagram.participants.iter().enumerate() {
        let x = xs[index] + offset;
        for top in [header_bottom - heights[index], bottom] {
            if participant.actor {
                svg.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"7\" class=\"fmd-seq-actor\"/>\n<path d=\"M {} {} V {} M {} {} H {} M {} {} L {} {} M {} {} L {} {}\" class=\"fmd-seq-actor\"/>\n",
                    SvgNum(x), SvgNum(top + 10.0), SvgNum(x), SvgNum(top + 17.0), SvgNum(top + 36.0),
                    SvgNum(x - 12.0), SvgNum(top + 24.0), SvgNum(x + 12.0),
                    SvgNum(x), SvgNum(top + 36.0), SvgNum(x - 10.0), SvgNum(top + 48.0),
                    SvgNum(x), SvgNum(top + 36.0), SvgNum(x + 10.0), SvgNum(top + 48.0)));
                labels[index].draw(&mut svg, x, top + 54.0, "middle", "fmd-seq-box-text");
            } else {
                svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" class=\"fmd-seq-box\"/>\n",
                    SvgNum(x - widths[index] / 2.0), SvgNum(top), SvgNum(widths[index]), SvgNum(heights[index])));
                labels[index].draw(&mut svg, x, top + 10.0, "middle", "fmd-seq-box-text");
            }
        }
    }
    svg.push_str("</svg>\n");
    Some(svg)
}

const STYLE: &str = r#"<style>
.fmd-seq-lifeline{stroke:var(--border-default,#d0d7de);stroke-width:1.5;stroke-dasharray:4,4}
.fmd-seq-box{fill:var(--bg-canvas,#ffffff);stroke:var(--border-default,#d0d7de);stroke-width:1.5}
.fmd-seq-box-text,.fmd-seq-title,.fmd-seq-frame-text{fill:var(--fg-default,#1f2328);font-weight:600}
.fmd-seq-msg-text{fill:var(--fg-default,#1f2328)}
.fmd-seq-msg-line,.fmd-seq-arrow-open,.fmd-seq-actor{fill:none;stroke:var(--fg-default,#1f2328);stroke-width:1.5}
.fmd-seq-arrow{fill:var(--fg-default,#1f2328)}
.fmd-seq-note{fill:#fff8c5;stroke:#d4a72c;stroke-width:1}
.fmd-seq-note-text{fill:#574600}
.fmd-seq-frame{fill:none;stroke:var(--fg-muted,#57606a);stroke-width:1.5}
.fmd-seq-branch{stroke:var(--fg-muted,#57606a);stroke-width:1;stroke-dasharray:5,4}
.fmd-seq-activation{fill:var(--bg-subtle,#f6f8fa);stroke:var(--fg-muted,#57606a);stroke-width:1}
@media(prefers-color-scheme:dark){
.fmd-seq-lifeline,.fmd-seq-box{stroke:#30363d}.fmd-seq-box{fill:#161b22}
.fmd-seq-box-text,.fmd-seq-title,.fmd-seq-frame-text,.fmd-seq-msg-text,.fmd-seq-arrow{fill:#e6edf3}
.fmd-seq-msg-line,.fmd-seq-arrow-open,.fmd-seq-actor{stroke:#e6edf3}
.fmd-seq-frame,.fmd-seq-branch,.fmd-seq-activation{stroke:#8b949e}.fmd-seq-activation{fill:#161b22}
.fmd-seq-note{fill:#2e2600;stroke:#9e7a00}.fmd-seq-note-text{fill:#e3b341}}
</style>
"#;
