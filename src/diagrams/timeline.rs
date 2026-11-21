//! Bounded Mermaid timelines, compiled once for every publication backend.
//!
//! Periods and their events retain source order. Parse the complete diagram
//! before laying it out; unsupported syntax or a resource limit returns the
//! entire fence to the ordinary code renderer.

use super::{SvgNum, escape_svg_text};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_PERIODS: usize = 64;
const MAX_EVENTS: usize = 256;
const MAX_SECTIONS: usize = 32;
const MAX_LABEL_BYTES: usize = 1024;
const MAX_TEXT_LINES: usize = 2048;
const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;
const MAX_CANVAS_SIDE: f32 = 16_384.0;
const MAX_CANVAS_AREA: f64 = 100_000_000.0;
const MARGIN: f32 = 24.0;
const PADDING: f32 = 12.0;
const COLUMN_WIDTH: f32 = 224.0;
const COLUMN_GAP: f32 = 24.0;
const EVENT_GAP: f32 = 10.0;
const PERIOD_SIZE: f32 = 15.0;
const EVENT_SIZE: f32 = 13.0;
const COLORS: [(&str, &str); 12] = [
    ("#2563eb", "#dbeafe"),
    ("#0f766e", "#ccfbf1"),
    ("#b45309", "#fef3c7"),
    ("#7e22ce", "#f3e8ff"),
    ("#be123c", "#ffe4e6"),
    ("#0369a1", "#e0f2fe"),
    ("#4d7c0f", "#ecfccb"),
    ("#c2410c", "#ffedd5"),
    ("#4f46e5", "#e0e7ff"),
    ("#be185d", "#fce7f3"),
    ("#475569", "#e2e8f0"),
    ("#a16207", "#fef9c3"),
];

#[derive(Clone, Copy)]
enum Direction {
    LeftToRight,
    TopDown,
}

struct Period {
    label: String,
    events: Vec<String>,
    section: usize,
}

struct Section {
    label: Option<String>,
    start: usize,
    end: usize,
}

struct Timeline {
    direction: Direction,
    title: Option<String>,
    sections: Vec<Section>,
    periods: Vec<Period>,
    section_colors: bool,
}

impl Timeline {
    fn color(&self, index: usize) -> (&'static str, &'static str) {
        let color = if self.section_colors {
            self.periods[index].section
        } else {
            index
        };
        COLORS[color % COLORS.len()]
    }
}

fn word<'a>(source: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = source.strip_prefix(keyword)?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim_start())
}

pub(super) fn is_header(line: &str) -> bool {
    word(line, "timeline").is_some()
}

fn valid_scalar(ch: char) -> bool {
    (!ch.is_control() || matches!(ch, '\n' | '\r' | '\t')) && !matches!(ch, '\u{fffe}' | '\u{ffff}')
}

// Decode label text after separating periods/events. An encoded colon stays
// text, and an encoded '<' is never reparsed as markup or a line break.
fn label(source: &str) -> Option<String> {
    if source.is_empty() || source.len() > MAX_LABEL_BYTES {
        return None;
    }
    let mut text = String::with_capacity(source.len());
    let mut rest = source;
    while !rest.is_empty() {
        if rest.starts_with('<')
            && let Some(end) = rest.find('>').filter(|&end| end <= 5)
            && matches!(
                rest[..=end].to_ascii_lowercase().as_str(),
                "<br>" | "<br/>" | "<br />"
            )
        {
            text.push('\n');
            rest = &rest[end + 1..];
            continue;
        }
        if matches!(rest.as_bytes()[0], b'#' | b'&')
            && let Some(end) = rest.find(';').filter(|&end| end > 1 && end <= 33)
        {
            let body = &rest[1..end];
            let number = if rest.starts_with('#') {
                body.bytes()
                    .all(|byte| byte.is_ascii_digit())
                    .then_some(body)
            } else {
                body.strip_prefix('#')
            };
            if let Some(number) = number {
                let value = if let Some(hex) = number
                    .strip_prefix('x')
                    .or_else(|| number.strip_prefix('X'))
                {
                    u32::from_str_radix(hex, 16).ok()?
                } else {
                    number.parse().ok()?
                };
                let ch = char::from_u32(value).filter(|&ch| valid_scalar(ch))?;
                text.push(ch);
                rest = &rest[end + 1..];
                continue;
            }
            if let Some(value) = crate::parse::entities::lookup(body) {
                if !value.chars().all(valid_scalar) {
                    return None;
                }
                text.push_str(value);
                rest = &rest[end + 1..];
                continue;
            }
        }
        let ch = rest.chars().next()?;
        text.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    (!text.trim().is_empty()).then_some(text)
}

fn parse(source: &str) -> Option<Timeline> {
    if source.len() > MAX_SOURCE_BYTES
        || !source.chars().all(valid_scalar)
        || source.contains("%%{")
    {
        return None;
    }
    let mut lines = source
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("%%"));
    let direction = match word(lines.next()?, "timeline")? {
        "" | "LR" => Direction::LeftToRight,
        "TD" => Direction::TopDown,
        _ => return None,
    };
    let mut timeline = Timeline {
        direction,
        title: None,
        sections: vec![Section {
            label: None,
            start: 0,
            end: 0,
        }],
        periods: Vec::new(),
        section_colors: false,
    };
    let mut events = 0;
    for line in lines {
        if let Some(value) = word(line, "title") {
            if timeline.title.replace(label(value)?).is_some() {
                return None;
            }
            continue;
        }
        if let Some(value) = word(line, "section") {
            let value = label(value)?;
            timeline.section_colors = true;
            let previous = timeline.sections.last_mut()?;
            if previous.start == timeline.periods.len() {
                // The implicit initial section may be replaced before it owns
                // data. Explicit empty sections must not silently disappear.
                if previous.label.is_some() {
                    return None;
                }
                previous.label = Some(value);
            } else {
                if timeline.sections.len() == MAX_SECTIONS {
                    return None;
                }
                timeline.sections.push(Section {
                    label: Some(value),
                    start: timeline.periods.len(),
                    end: timeline.periods.len(),
                });
            }
            continue;
        }
        if [
            "timeline",
            "title",
            "section",
            "direction",
            "classDef",
            "class",
            "style",
            "click",
            "accTitle",
            "accDescr",
        ]
        .iter()
        .any(|keyword| {
            word(line, keyword).is_some()
                || line
                    .strip_prefix(keyword)
                    .is_some_and(|rest| rest.starts_with(':'))
        }) {
            return None;
        }
        let (period, event_text) = line.split_once(':')?;
        let section = timeline.sections.len() - 1;
        if period.trim().is_empty() {
            // A new section never inherits the preceding section's last period.
            if timeline.periods.last()?.section != section {
                return None;
            }
        } else {
            if timeline.periods.len() == MAX_PERIODS {
                return None;
            }
            timeline.periods.push(Period {
                label: label(period.trim())?,
                events: Vec::new(),
                section,
            });
            timeline.sections[section].end = timeline.periods.len();
        }
        for event in event_text.split(':') {
            if events == MAX_EVENTS {
                return None;
            }
            timeline
                .periods
                .last_mut()?
                .events
                .push(label(event.trim())?);
            events += 1;
        }
    }
    let last = timeline.sections.last()?;
    if timeline.periods.is_empty() || last.start == last.end {
        return None;
    }
    Some(timeline)
}

struct Text {
    lines: Vec<String>,
    size: f32,
    leading: f32,
}

// A conservative envelope, not a substitute for the output's actual font
// shaper. The shared cluster segmenter keeps marks and joined sequences intact
// when a long word must wrap. A cluster too wide for its cell refuses the figure.
fn width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|ch| if ch.is_ascii() { size } else { size * 2.0 })
        .sum()
}

impl Text {
    fn new(value: &str, size: f32, max_width: f32, budget: &mut usize) -> Option<Self> {
        let mut lines = Vec::new();
        let mut append = |line: String| {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
            lines.push(line);
            Some(())
        };
        for hard_line in value.split('\n') {
            let mut line = String::new();
            let mut used = 0.0;
            for word in hard_line
                .split([' ', '\t', '\r'])
                .filter(|word| !word.is_empty())
            {
                let advance = width(word, size);
                // Preserve nonbreaking spaces and word joiners verbatim. An
                // overwide bound phrase must not be split into separate labels.
                if advance > max_width
                    && word
                        .chars()
                        .any(|ch| matches!(ch, '\u{00a0}' | '\u{2007}' | '\u{202f}' | '\u{2060}'))
                {
                    return None;
                }
                if advance <= max_width {
                    if !line.is_empty() && used + size + advance > max_width {
                        append(std::mem::take(&mut line))?;
                        used = 0.0;
                    }
                    if !line.is_empty() {
                        line.push(' ');
                        used += size;
                    }
                    line.push_str(word);
                    used += advance;
                } else {
                    if !line.is_empty() {
                        append(std::mem::take(&mut line))?;
                        used = 0.0;
                    }
                    for cluster in crate::font_context::segment_clusters(word) {
                        let text = word.get(cluster.start..cluster.end)?;
                        let advance = width(text, size);
                        if advance > max_width {
                            return None;
                        }
                        if !line.is_empty() && used + advance > max_width {
                            append(std::mem::take(&mut line))?;
                            used = 0.0;
                        }
                        line.push_str(text);
                        used += advance;
                    }
                }
            }
            // Empty hard lines retain an authored <br><br> gap.
            append(line)?;
        }
        Some(Self {
            lines,
            size,
            leading: size + 5.0,
        })
    }

    fn optional(
        value: Option<&str>,
        size: f32,
        max_width: f32,
        budget: &mut usize,
    ) -> Option<Option<Self>> {
        match value {
            Some(value) => Self::new(value, size, max_width, budget).map(Some),
            None => Some(None),
        }
    }

    fn height(&self) -> f32 {
        self.lines.len() as f32 * self.leading
    }

    fn draw(&self, out: &mut String, x: f32, y: f32, class: &str, anchor: &str) {
        for (index, line) in self.lines.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\" font-size=\"{}\" class=\"{class}\">{}</text>\n",
                SvgNum(x),
                SvgNum(y + self.size + index as f32 * self.leading),
                SvgNum(self.size),
                escape_svg_text(line),
            ));
        }
    }
}

struct PeriodLayout {
    label: Text,
    events: Vec<Text>,
}

fn periods(
    timeline: &Timeline,
    period_width: f32,
    event_width: f32,
    budget: &mut usize,
) -> Option<Vec<PeriodLayout>> {
    timeline
        .periods
        .iter()
        .map(|period| {
            Some(PeriodLayout {
                label: Text::new(
                    &period.label,
                    PERIOD_SIZE,
                    period_width - 2.0 * PADDING,
                    budget,
                )?,
                events: period
                    .events
                    .iter()
                    .map(|event| Text::new(event, EVENT_SIZE, event_width - 2.0 * PADDING, budget))
                    .collect::<Option<Vec<_>>>()?,
            })
        })
        .collect()
}

fn description(timeline: &Timeline) -> String {
    let mut result = String::new();
    for section in &timeline.sections {
        if let Some(label) = &section.label {
            result.push_str(&escape_svg_text(&label.replace('\n', " ")));
            result.push_str(": ");
        }
        for period in &timeline.periods[section.start..section.end] {
            result.push_str(&escape_svg_text(&period.label.replace('\n', " ")));
            result.push_str(": ");
            for (index, event) in period.events.iter().enumerate() {
                if index > 0 {
                    result.push_str("; ");
                }
                result.push_str(&escape_svg_text(&event.replace('\n', " ")));
            }
            result.push_str(". ");
        }
    }
    result
}

fn start(timeline: &Timeline, width: f32, height: f32) -> Option<String> {
    // Shared SVG image admission applies to PDF/poster consumers too. Refuse
    // the complete diagram before emitting a canvas another backend would drop.
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width > MAX_CANVAS_SIDE
        || height > MAX_CANVAS_SIDE
        || f64::from(width.ceil()) * f64::from(height.ceil()) > MAX_CANVAS_AREA
    {
        return None;
    }
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" class=\"fmd-diagram fmd-timeline\" role=\"img\" xml:space=\"preserve\">\n<title>{}</title>\n<desc>{}</desc>\n<style>.fmd-timeline text{{font-family:Arial,Helvetica,sans-serif;fill:#1f2328}}.fmd-timeline .fmd-timeline-title{{fill:var(--fg-default,#1f2328);font-weight:600}}.fmd-timeline .fmd-timeline-period-label,.fmd-timeline .fmd-timeline-section-label{{font-weight:600}}</style>\n",
        SvgNum(width),
        SvgNum(height),
        SvgNum(width),
        SvgNum(height),
        escape_svg_text(timeline.title.as_deref().unwrap_or("Timeline diagram")),
        description(timeline),
    ))
}

fn line(out: &mut String, x1: f32, y1: f32, x2: f32, y2: f32, color: &str, class: &str) {
    if x1 == x2 && y1 == y2 {
        return;
    }
    out.push_str(&format!(
        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{color}\" stroke-width=\"1.5\" class=\"{class}\"/>\n",
        SvgNum(x1), SvgNum(y1), SvgNum(x2), SvgNum(y2),
    ));
}

fn dot(out: &mut String, x: f32, y: f32, color: &str) {
    out.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{color}\" class=\"fmd-timeline-point\"/>\n",
        SvgNum(x),
        SvgNum(y),
    ));
}

fn rect(
    out: &mut String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: (&str, &str),
    class: &str,
) {
    out.push_str(&format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"6\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1.5\" class=\"{class}\"/>\n",
        SvgNum(x), SvgNum(y), SvgNum(width), SvgNum(height), color.1, color.0,
    ));
}

fn horizontal(timeline: &Timeline) -> Option<String> {
    let mut budget = MAX_TEXT_LINES;
    let layouts = periods(timeline, COLUMN_WIDTH, COLUMN_WIDTH, &mut budget)?;
    let width = 2.0 * MARGIN
        + timeline.periods.len() as f32 * COLUMN_WIDTH
        + timeline.periods.len().saturating_sub(1) as f32 * COLUMN_GAP;
    let title = Text::optional(
        timeline.title.as_deref(),
        20.0,
        width - 2.0 * MARGIN,
        &mut budget,
    )?;
    let section_top = MARGIN + title.as_ref().map_or(0.0, |text| text.height() + 20.0);
    let mut sections = Vec::new();
    let mut section_height: f32 = 0.0;
    for section in &timeline.sections {
        let width = (section.end - section.start) as f32 * (COLUMN_WIDTH + COLUMN_GAP) - COLUMN_GAP;
        let text = Text::optional(
            section.label.as_deref(),
            15.0,
            width - 2.0 * PADDING,
            &mut budget,
        )?;
        if let Some(text) = &text {
            section_height = section_height.max(text.height() + 2.0 * PADDING);
        }
        sections.push((text, width));
    }
    let period_top = section_top
        + if section_height > 0.0 {
            section_height + 16.0
        } else {
            0.0
        };
    let period_height = layouts
        .iter()
        .map(|period| period.label.height() + 2.0 * PADDING)
        .fold(0.0, f32::max);
    let axis_y = period_top + period_height + 20.0;
    let events_top = axis_y + 24.0;
    let events_height = layouts
        .iter()
        .map(|period| {
            period
                .events
                .iter()
                .map(|event| event.height() + 2.0 * PADDING)
                .sum::<f32>()
                + period.events.len().saturating_sub(1) as f32 * EVENT_GAP
        })
        .fold(0.0, f32::max);
    let height = events_top + events_height + MARGIN;
    let mut out = start(timeline, width, height)?;
    if let Some(title) = &title {
        title.draw(
            &mut out,
            width / 2.0,
            MARGIN,
            "fmd-timeline-title",
            "middle",
        );
    }
    line(
        &mut out,
        MARGIN,
        axis_y,
        width - MARGIN,
        axis_y,
        "#64748b",
        "fmd-timeline-axis",
    );
    out.push_str(&format!(
        "<path d=\"M {} {} L {} {} L {} {} Z\" fill=\"#64748b\" class=\"fmd-timeline-arrow\"/>\n",
        SvgNum(width - MARGIN + 6.0),
        SvgNum(axis_y),
        SvgNum(width - MARGIN - 2.0),
        SvgNum(axis_y - 4.0),
        SvgNum(width - MARGIN - 2.0),
        SvgNum(axis_y + 4.0),
    ));
    for (section, (label, section_width)) in timeline.sections.iter().zip(sections) {
        if let Some(label) = label {
            let x = MARGIN + section.start as f32 * (COLUMN_WIDTH + COLUMN_GAP);
            rect(
                &mut out,
                x,
                section_top,
                section_width,
                section_height,
                timeline.color(section.start),
                "fmd-timeline-section",
            );
            label.draw(
                &mut out,
                x + section_width / 2.0,
                section_top + (section_height - label.height()) / 2.0,
                "fmd-timeline-section-label",
                "middle",
            );
        }
    }
    for (index, period) in layouts.iter().enumerate() {
        let color = timeline.color(index);
        let x = MARGIN + index as f32 * (COLUMN_WIDTH + COLUMN_GAP);
        let center = x + COLUMN_WIDTH / 2.0;
        line(
            &mut out,
            center,
            period_top + period_height,
            center,
            axis_y,
            color.0,
            "fmd-timeline-connector",
        );
        dot(&mut out, center, axis_y, color.0);
        rect(
            &mut out,
            x,
            period_top,
            COLUMN_WIDTH,
            period_height,
            color,
            "fmd-timeline-period",
        );
        period.label.draw(
            &mut out,
            center,
            period_top + (period_height - period.label.height()) / 2.0,
            "fmd-timeline-period-label",
            "middle",
        );
        let mut y = events_top;
        let mut previous = axis_y;
        for event in &period.events {
            line(
                &mut out,
                center,
                previous,
                center,
                y,
                color.0,
                "fmd-timeline-connector",
            );
            let height = event.height() + 2.0 * PADDING;
            rect(
                &mut out,
                x,
                y,
                COLUMN_WIDTH,
                height,
                color,
                "fmd-timeline-event",
            );
            event.draw(
                &mut out,
                x + PADDING,
                y + PADDING,
                "fmd-timeline-event-label",
                "start",
            );
            previous = y + height;
            y = previous + EVENT_GAP;
        }
    }
    out.push_str("</svg>\n");
    (out.len() <= MAX_SVG_BYTES).then_some(out)
}

fn vertical(timeline: &Timeline) -> Option<String> {
    const PERIOD_WIDTH: f32 = 200.0;
    const EVENT_WIDTH: f32 = 460.0;
    const GAP: f32 = 36.0;
    const WIDTH: f32 = 2.0 * MARGIN + PERIOD_WIDTH + GAP + EVENT_WIDTH;
    const AXIS: f32 = MARGIN + PERIOD_WIDTH + GAP / 2.0;
    const EVENT_X: f32 = MARGIN + PERIOD_WIDTH + GAP;
    let mut budget = MAX_TEXT_LINES;
    let layouts = periods(timeline, PERIOD_WIDTH, EVENT_WIDTH, &mut budget)?;
    let title = Text::optional(
        timeline.title.as_deref(),
        20.0,
        WIDTH - 2.0 * MARGIN,
        &mut budget,
    )?;
    let mut y = MARGIN + title.as_ref().map_or(0.0, |title| title.height() + 20.0);
    let mut sections = Vec::new();
    let mut rows = Vec::new();
    for section in &timeline.sections {
        let label = Text::optional(
            section.label.as_deref(),
            15.0,
            WIDTH - 2.0 * MARGIN - 2.0 * PADDING,
            &mut budget,
        )?;
        let section_y = y;
        if let Some(label) = &label {
            y += label.height() + 2.0 * PADDING + 16.0;
        }
        sections.push((label, section_y));
        for period in &layouts[section.start..section.end] {
            let period_height = period.label.height() + 2.0 * PADDING;
            let events_height = period
                .events
                .iter()
                .map(|event| event.height() + 2.0 * PADDING)
                .sum::<f32>()
                + period.events.len().saturating_sub(1) as f32 * EVENT_GAP;
            let height = period_height.max(events_height);
            rows.push((y, height, period_height, events_height));
            y += height + 28.0;
        }
    }
    let height = y + MARGIN;
    let mut out = start(timeline, WIDTH, height)?;
    if let Some(title) = &title {
        title.draw(
            &mut out,
            WIDTH / 2.0,
            MARGIN,
            "fmd-timeline-title",
            "middle",
        );
    }
    let start_y = rows.first()?.0;
    line(
        &mut out,
        AXIS,
        start_y,
        AXIS,
        y - 8.0,
        "#64748b",
        "fmd-timeline-axis",
    );
    out.push_str(&format!(
        "<path d=\"M {} {} L {} {} L {} {} Z\" fill=\"#64748b\" class=\"fmd-timeline-arrow\"/>\n",
        SvgNum(AXIS),
        SvgNum(y),
        SvgNum(AXIS - 4.0),
        SvgNum(y - 8.0),
        SvgNum(AXIS + 4.0),
        SvgNum(y - 8.0),
    ));
    for (section, (label, section_y)) in timeline.sections.iter().zip(sections) {
        if let Some(label) = label {
            rect(
                &mut out,
                MARGIN,
                section_y,
                WIDTH - 2.0 * MARGIN,
                label.height() + 2.0 * PADDING,
                timeline.color(section.start),
                "fmd-timeline-section",
            );
            label.draw(
                &mut out,
                WIDTH / 2.0,
                section_y + PADDING,
                "fmd-timeline-section-label",
                "middle",
            );
        }
    }
    for (index, (period, (row_y, row_height, period_height, events_height))) in
        layouts.iter().zip(rows).enumerate()
    {
        let color = timeline.color(index);
        let center = row_y + row_height / 2.0;
        let period_y = center - period_height / 2.0;
        line(
            &mut out,
            MARGIN + PERIOD_WIDTH,
            center,
            AXIS,
            center,
            color.0,
            "fmd-timeline-connector",
        );
        dot(&mut out, AXIS, center, color.0);
        rect(
            &mut out,
            MARGIN,
            period_y,
            PERIOD_WIDTH,
            period_height,
            color,
            "fmd-timeline-period",
        );
        period.label.draw(
            &mut out,
            MARGIN + PERIOD_WIDTH / 2.0,
            period_y + PADDING,
            "fmd-timeline-period-label",
            "middle",
        );
        let mut event_y = center - events_height / 2.0;
        let first_center = event_y + (period.events.first()?.height() + 2.0 * PADDING) / 2.0;
        let last_center =
            center + events_height / 2.0 - (period.events.last()?.height() + 2.0 * PADDING) / 2.0;
        line(
            &mut out,
            AXIS,
            first_center,
            AXIS,
            last_center,
            color.0,
            "fmd-timeline-connector",
        );
        for event in &period.events {
            let event_height = event.height() + 2.0 * PADDING;
            line(
                &mut out,
                AXIS,
                event_y + event_height / 2.0,
                EVENT_X,
                event_y + event_height / 2.0,
                color.0,
                "fmd-timeline-connector",
            );
            rect(
                &mut out,
                EVENT_X,
                event_y,
                EVENT_WIDTH,
                event_height,
                color,
                "fmd-timeline-event",
            );
            event.draw(
                &mut out,
                EVENT_X + PADDING,
                event_y + PADDING,
                "fmd-timeline-event-label",
                "start",
            );
            event_y += event_height + EVENT_GAP;
        }
    }
    out.push_str("</svg>\n");
    (out.len() <= MAX_SVG_BYTES).then_some(out)
}

pub(super) fn render(source: &str) -> Option<String> {
    let timeline = parse(source)?;
    match timeline.direction {
        Direction::LeftToRight => horizontal(&timeline),
        Direction::TopDown => vertical(&timeline),
    }
}
