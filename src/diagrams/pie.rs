//! Bounded Mermaid pie charts shared by HTML, PDF, SVG and EPUB.
//!
//! Values are parsed as integer hundredths, so admission, totals and displayed
//! percentages do not depend on floating-point parsing. Only vector geometry
//! uses floats. Unknown syntax returns the complete source to the code renderer.

use super::escape_svg_text;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_SLICES: usize = 64;
const MAX_LABEL_BYTES: usize = 512;
const MAX_TITLE_BYTES: usize = 1024;
const COLORS: [&str; 12] = [
    "#2563eb", "#0f766e", "#b45309", "#7e22ce", "#be123c", "#0369a1", "#4d7c0f", "#c2410c",
    "#4f46e5", "#be185d", "#475569", "#a16207",
];

struct Slice<'a> {
    label: &'a str,
    value: &'a str,
    hundredths: u64,
}

struct Chart<'a> {
    title: Option<&'a str>,
    show_data: bool,
    slices: Vec<Slice<'a>>,
    total: u128,
}

fn keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim_start())
}

pub(super) fn is_header(line: &str) -> bool {
    keyword(line, "pie").is_some()
}

// Comments are syntax only outside quoted labels. A label may contain %%.
fn without_comment(line: &str) -> &str {
    let mut quoted = false;
    let bytes = line.as_bytes();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'"' {
            quoted = !quoted;
        } else if !quoted && byte == b'%' && bytes.get(index + 1) == Some(&b'%') {
            return &line[..index];
        }
    }
    line
}

fn hundredths(number: &str) -> Option<u64> {
    let (whole, fractional) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 2
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
        || (number.contains('.') && fractional.is_empty())
    {
        return None;
    }
    let decimal = match fractional.len() {
        0 => 0,
        1 => fractional.parse::<u64>().ok()? * 10,
        _ => fractional.parse::<u64>().ok()?,
    };
    let value = whole
        .parse::<u64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(decimal)?;
    (value > 0).then_some(value)
}

fn parse(source: &str) -> Option<Chart<'_>> {
    if source.len() > MAX_SOURCE_BYTES
        // Initialization directives can request unsupported layouts such as
        // donuts. They are not ordinary comments to silently discard.
        || source.lines().any(|line| line[without_comment(line).len()..].starts_with("%%{"))
        || source.chars().any(|ch| {
            (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                || matches!(ch, '\u{fffe}' | '\u{ffff}')
        })
    {
        return None;
    }
    let mut lines = source
        .lines()
        .map(without_comment)
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut header = keyword(lines.next()?, "pie")?;
    let show_data = if let Some(rest) = keyword(header, "showData") {
        header = rest;
        true
    } else {
        false
    };
    let mut chart = Chart {
        title: None,
        show_data,
        slices: Vec::new(),
        total: 0,
    };
    if !header.is_empty() {
        chart.title = Some(keyword(header, "title")?);
    }
    for line in lines {
        if let Some(title) = keyword(line, "title") {
            if chart.title.replace(title).is_some() || !chart.slices.is_empty() {
                return None;
            }
            continue;
        }
        let quoted = line.strip_prefix('"')?;
        let end = quoted.find('"')?;
        let label = &quoted[..end];
        let value = quoted[end + 1..].trim_start().strip_prefix(':')?.trim();
        if label.is_empty()
            || label.len() > MAX_LABEL_BYTES
            || chart.slices.len() == MAX_SLICES
            || chart.slices.iter().any(|slice| slice.label == label)
        {
            return None;
        }
        let hundredths = hundredths(value)?;
        chart.total += u128::from(hundredths);
        chart.slices.push(Slice {
            label,
            value,
            hundredths,
        });
    }
    if chart.slices.is_empty()
        || chart
            .title
            .is_some_and(|title| title.is_empty() || title.len() > MAX_TITLE_BYTES)
    {
        return None;
    }
    Some(chart)
}

// Conservative text boxes, including full-width scripts. Keep every character
// visible; long legends and titles wrap rather than becoming clipped SVG text.
fn wrap(text: &str, width: usize) -> Vec<&str> {
    let mut lines = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (index, ch) in text.char_indices() {
        let advance = if ch.is_ascii() { 1 } else { 2 };
        if used + advance > width {
            lines.push(&text[start..index]);
            start = index;
            used = 0;
        }
        used += advance;
    }
    lines.push(&text[start..]);
    lines
}

fn percent(hundredths: u64, total: u128) -> String {
    let tenths = (u128::from(hundredths) * 1000 + total / 2) / total;
    if tenths % 10 == 0 {
        format!("{}%", tenths / 10)
    } else {
        format!("{}.{:01}%", tenths / 10, tenths % 10)
    }
}

pub(super) fn render(source: &str) -> Option<String> {
    let chart = parse(source)?;
    let title_lines = chart.title.map_or_else(Vec::new, |title| wrap(title, 24));
    let top = 24.0 + title_lines.len() as f64 * 26.0;
    let legends: Vec<_> = chart
        .slices
        .iter()
        .map(|slice| {
            let text = if chart.show_data {
                format!("{} [{}]", slice.label, slice.value)
            } else {
                slice.label.to_owned()
            };
            wrap(&text, 24)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    let legend_height: usize = legends.iter().map(|lines| lines.len() * 18 + 12).sum();
    let body_height = (legend_height as f64).max(280.0);
    let height = top + body_height + 24.0;
    let (cx, cy, radius) = (144.0, top + body_height / 2.0, 120.0);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 640 {height:.1}\" width=\"640\" height=\"{height:.1}\" class=\"fmd-diagram fmd-pie\" role=\"img\">\n<title>{}</title>\n<desc>",
        escape_svg_text(chart.title.unwrap_or("Pie chart")),
    );
    for (index, slice) in chart.slices.iter().enumerate() {
        if index > 0 {
            svg.push_str("; ");
        }
        svg.push_str(&format!(
            "{}: {} ({})",
            escape_svg_text(slice.label),
            slice.value,
            percent(slice.hundredths, chart.total)
        ));
    }
    svg.push_str("</desc>\n<style>.fmd-pie text{font-family:Arial,Helvetica,sans-serif;fill:var(--fg-default,#1f2328)}.fmd-pie .fmd-pie-percent{fill:#fff;font-weight:700}</style>\n");
    for (index, line) in title_lines.iter().enumerate() {
        svg.push_str(&format!("<text x=\"320\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"20\" font-weight=\"600\">{}</text>\n", 28.0 + index as f64 * 26.0, escape_svg_text(line)));
    }
    let mut cumulative = 0u128;
    let mut legend_y = top + (body_height - legend_height as f64) / 2.0 + 16.0;
    for (index, (slice, lines)) in chart.slices.iter().zip(&legends).enumerate() {
        let color = COLORS[index % COLORS.len()];
        let fraction = slice.hundredths as f64 / chart.total as f64;
        let start = cumulative as f64 / chart.total as f64 * std::f64::consts::TAU
            - std::f64::consts::FRAC_PI_2;
        cumulative += u128::from(slice.hundredths);
        let end = cumulative as f64 / chart.total as f64 * std::f64::consts::TAU
            - std::f64::consts::FRAC_PI_2;
        let point = |angle: f64| {
            let round = |value: f64| (value * 1000.0).round() / 1000.0;
            (
                round(cx + radius * angle.cos()),
                round(cy + radius * angle.sin()),
            )
        };
        let (x1, y1) = point(start);
        let (x2, y2) = point(end);
        let large = u8::from(u128::from(slice.hundredths) * 2 > chart.total);
        if chart.slices.len() == 1 || (large == 1 && (x1, y1) == (x2, y2)) {
            // An SVG arc with identical endpoints paints nothing. If the
            // remaining sliver is below coordinate precision, retain the
            // dominant circle and every exact value in its legend/description.
            svg.push_str(&format!(
                "<circle cx=\"{cx:.3}\" cy=\"{cy:.3}\" r=\"{radius:.3}\" fill=\"{color}\"/>\n"
            ));
        } else {
            svg.push_str(&format!("<path d=\"M {cx:.3} {cy:.3} L {x1:.3} {y1:.3} A {radius:.3} {radius:.3} 0 {large} 1 {x2:.3} {y2:.3} Z\" fill=\"{color}\" stroke=\"#fff\" stroke-width=\"1\"/>\n"));
        }
        if fraction >= 0.05 {
            let mid = (start + end) / 2.0;
            let (x, y) = if chart.slices.len() == 1 {
                (cx, cy)
            } else {
                (
                    cx + radius * 0.64 * mid.cos(),
                    cy + radius * 0.64 * mid.sin(),
                )
            };
            svg.push_str(&format!("<text x=\"{x:.3}\" y=\"{:.3}\" text-anchor=\"middle\" font-size=\"13\" class=\"fmd-pie-percent\">{}</text>\n", y + 4.0, percent(slice.hundredths, chart.total)));
        }
        svg.push_str(&format!(
            "<rect x=\"292\" y=\"{:.1}\" width=\"14\" height=\"14\" rx=\"2\" fill=\"{color}\"/>\n",
            legend_y - 12.0
        ));
        for line in lines {
            svg.push_str(&format!(
                "<text x=\"318\" y=\"{legend_y:.1}\" font-size=\"13\">{}</text>\n",
                escape_svg_text(line)
            ));
            legend_y += 18.0;
        }
        legend_y += 12.0;
    }
    svg.push_str("</svg>\n");
    Some(svg)
}
