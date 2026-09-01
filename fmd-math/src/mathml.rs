//! Node-tree → MathML Core serializer.
//!
//! Deterministic: attribute order is fixed per element, every element has an
//! explicit close tag (no self-closing form), and text/attr values are XML
//! escaped. The walk never panics; a hostile or fragment tree still yields a
//! well-formed fragment.

use crate::atom::AtomClass;
use crate::node::{
    AccentKind, Delim, FragmentKind, Limits, MathFont, Node, NodeKind, PhantomKind, SpaceKind,
    Span, StackKind, TextStyle,
};
use crate::style::Style;

const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";

/// Capacity-hint multiplier: MathML output bytes per TeX source byte.
/// Measured over the repo math fixtures plus the round-2 pass-20 math-heavy
/// corpus (120 distinct (tex, display) shapes, 6,046 occurrence-weighted
/// renders): ratio min 4.74, distinct p50 8.88, occurrence-weighted median
/// 7.94; together with the floor below, 8x covers every measured shape —
/// see tests/artifacts/perf/round2-reserve-r1-mathml-presize/ratio-measurement.txt.
const MATHML_BYTES_PER_TEX_BYTE: usize = 8;

/// Capacity-hint floor covering the fixed `<math …>` wrapper: the largest
/// output for a one-byte TeX source is 83 B (`+`), so 160 B doubles that
/// margin and absorbs every measured short formula on its own.
const MATHML_CAPACITY_FLOOR: usize = 160;

/// Serialize `node` as a complete `<math>…</math>` fragment.
///
/// `display = true` sets `display="block"` (TeX display / `$$`); `false` sets
/// `display="inline"` (`$…$`).
#[must_use]
pub fn to_mathml(node: &Node, display: bool) -> String {
    to_mathml_with_capacity(node, display, 0)
}

/// [`to_mathml`] with a TeX-source-length hint used only to presize the
/// output buffer (`MATHML_CAPACITY_FLOOR + tex_len *
/// MATHML_BYTES_PER_TEX_BYTE` bytes, so the serializer does not walk the
/// realloc-doubling chain for typical formulas). The emitted bytes are
/// identical to [`to_mathml`] for every `(node, display, tex_len)` — buffer
/// capacity is never observable in the output.
#[must_use]
pub fn to_mathml_with_capacity(node: &Node, display: bool, tex_len: usize) -> String {
    let cap = MATHML_CAPACITY_FLOOR + tex_len.saturating_mul(MATHML_BYTES_PER_TEX_BYTE);
    let mut w = Writer::with_capacity(cap);
    let display_val = if display { "block" } else { "inline" };
    w.open("math", &[("xmlns", MATHML_NS), ("display", display_val)]);
    let style = if display { Style::Display } else { Style::Text };
    match &node.kind {
        NodeKind::List(items) => emit_run(&mut w, items, style, None, None),
        _ => emit_node(&mut w, node, style),
    }
    w.close("math");
    w.buf
}

/// Serialize `node` as a MathML element (no outer `<math>` wrapper).
///
/// A top-level [`NodeKind::List`] becomes a single `<mrow>`.
pub fn to_mathml_element(node: &Node) -> String {
    let mut w = Writer::with_capacity(0);
    emit_node(&mut w, node, Style::Display);
    w.buf
}

/// Std-only well-formedness check: balanced tags, quoted attributes, escaped
/// text. Accepts the serializer's output contract (no self-closing tags).
pub fn mathml_well_formed(xml: &str) -> Result<(), String> {
    check_well_formed(xml)
}

struct Writer {
    buf: String,
}

impl Writer {
    fn with_capacity(cap: usize) -> Self {
        Self {
            buf: String::with_capacity(cap),
        }
    }

    #[inline(always)]
    fn open(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        self.buf.push('<');
        self.buf.push_str(tag);
        for &(name, value) in attrs {
            self.buf.push(' ');
            self.buf.push_str(name);
            self.buf.push_str("=\"");
            push_escaped(&mut self.buf, value, true);
            self.buf.push('"');
        }
        self.buf.push('>');
    }

    #[inline(always)]
    fn close(&mut self, tag: &str) {
        self.buf.push('<');
        self.buf.push('/');
        self.buf.push_str(tag);
        self.buf.push('>');
    }

    #[inline(always)]
    fn text(&mut self, s: &str) {
        push_escaped(&mut self.buf, s, false);
    }

    #[inline(always)]
    fn char_text(&mut self, ch: char) {
        match ch {
            '&' => self.buf.push_str("&amp;"),
            '<' => self.buf.push_str("&lt;"),
            '>' => self.buf.push_str("&gt;"),
            _ => self.buf.push(ch),
        }
    }
}

#[inline(always)]
fn push_escaped(buf: &mut String, s: &str, attr: bool) {
    if !s
        .as_bytes()
        .iter()
        .any(|&b| b == b'&' || b == b'<' || b == b'>' || (attr && b == b'"'))
    {
        buf.push_str(s);
        return;
    }
    let bytes = s.as_bytes();
    let mut clean_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' if attr => "&quot;",
            _ => continue,
        };
        if clean_start < i {
            buf.push_str(&s[clean_start..i]);
        }
        buf.push_str(esc);
        clean_start = i + 1;
    }
    if clean_start < s.len() {
        buf.push_str(&s[clean_start..]);
    }
}

fn emit_node(w: &mut Writer, node: &Node, style: Style) {
    match &node.kind {
        NodeKind::List(items) => {
            w.open("mrow", &[]);
            emit_run(w, items, style, None, None);
            w.close("mrow");
        }
        NodeKind::Symbol { ch, class } => emit_symbol(w, *ch, *class),
        NodeKind::BigOp { ch, .. } => {
            w.open("mo", &[("movablelimits", "true")]);
            w.char_text(*ch);
            w.close("mo");
        }
        NodeKind::OpName { name, .. } => {
            w.open("mi", &[("mathvariant", "normal")]);
            w.text(name);
            w.close("mi");
        }
        NodeKind::Scripts {
            base,
            sub,
            sup,
            primes,
        } => emit_scripts(
            w,
            base.as_deref(),
            sub.as_deref(),
            sup.as_deref(),
            primes,
            style,
        ),
        NodeKind::Frac { num, den, spec } => {
            emit_frac(w, num, den, spec.bar, spec.delims, spec.forced_style, style)
        }
        NodeKind::Radical { index, radicand } => emit_radical(w, index.as_deref(), radicand, style),
        NodeKind::Accent { accent, base } => emit_accent(w, *accent, base, style),
        NodeKind::LeftRight { left, right, body } => emit_left_right(w, left, right, body, style),
        NodeKind::SizedDelim { delim, .. } => {
            if let Some(ch) = delim.ch {
                w.open("mo", &[]);
                w.char_text(ch);
                w.close("mo");
            }
        }
        NodeKind::Text { body } => emit_mtext_nodes(w, body),
        NodeKind::TextRun { text, .. } => {
            w.open("mtext", &[]);
            w.text(text);
            w.close("mtext");
        }
        NodeKind::TextStyled { style: ts, body } => emit_text_styled(w, *ts, body),
        NodeKind::MathIsland { body, display } => {
            let inner_style = if *display {
                Style::Display
            } else {
                Style::Text
            };
            w.open("mrow", &[]);
            emit_run(w, body, inner_style, None, None);
            w.close("mrow");
        }
        NodeKind::StyleChange(_)
        | NodeKind::AlignChange(_)
        | NodeKind::SizeChange(_)
        | NodeKind::ColorChange(_)
        | NodeKind::LineSpacing(_) => {
            // Remainder markers only have meaning inside a list walk.
        }
        NodeKind::MathFont { font, body } => {
            w.open("mstyle", &[("mathvariant", math_font_variant(*font))]);
            emit_node(w, body, style);
            w.close("mstyle");
        }
        NodeKind::Phantom { kind, body } => emit_phantom(w, *kind, body, style),
        NodeKind::Stack {
            kind,
            annotation,
            base,
        } => emit_stack(w, *kind, annotation, base, style),
        NodeKind::XArrow {
            mapsto,
            above,
            below,
        } => emit_xarrow(w, *mapsto, above, below.as_deref(), style),
        NodeKind::Space(kind) => emit_space(w, *kind),
        NodeKind::Tie => {
            w.open("mtext", &[]);
            w.buf.push('\u{00A0}');
            w.close("mtext");
        }
        NodeKind::Linebreak => {
            w.open("mspace", &[("linebreak", "newline")]);
            w.close("mspace");
        }
        NodeKind::AlignTab => {}
        NodeKind::AlignBlock { lines, .. } => emit_align_block(w, lines, style),
        NodeKind::Environment { name, spec, rows } => {
            emit_environment(w, name, spec.as_deref(), rows, style)
        }
        NodeKind::Fragment(kind) => emit_fragment(w, kind),
    }
}

fn emit_run(w: &mut Writer, items: &[Node], style: Style, color: Option<&str>, size: Option<f64>) {
    // Remainder markers (`\color`, `\displaystyle`, …) are siblings, not
    // nested groups. Walking them recursively is O(markers) stack frames, so
    // a long run of `\color{red}` would overflow. Fold style in a loop.
    let mut items = items;
    let mut style = style;
    let mut color = color;
    let mut size = size;
    while !items.is_empty() {
        let marker_at = items.iter().position(|n| is_remainder_marker(&n.kind));
        match marker_at {
            None => {
                emit_styled_siblings(w, items, style, color, size);
                return;
            }
            Some(0) => {
                let Some((first, rest)) = items.split_first() else {
                    return;
                };
                let (next_style, next_color, next_size) =
                    apply_marker(&first.kind, style, color, size);
                style = next_style;
                color = next_color;
                size = next_size;
                items = rest;
            }
            Some(k) => {
                emit_styled_siblings(w, &items[..k], style, color, size);
                items = &items[k..];
            }
        }
    }
}

fn is_remainder_marker(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::StyleChange(_)
            | NodeKind::AlignChange(_)
            | NodeKind::SizeChange(_)
            | NodeKind::ColorChange(_)
            | NodeKind::LineSpacing(_)
    )
}

fn apply_marker<'a>(
    kind: &'a NodeKind,
    style: Style,
    color: Option<&'a str>,
    size: Option<f64>,
) -> (Style, Option<&'a str>, Option<f64>) {
    match kind {
        NodeKind::StyleChange(s) => (*s, color, size),
        NodeKind::ColorChange(c) => (style, Some(c.as_str()), size),
        NodeKind::SizeChange(f) => (style, color, Some(*f)),
        NodeKind::AlignChange(_) | NodeKind::LineSpacing(_) => (style, color, size),
        _ => (style, color, size),
    }
}

fn emit_styled_siblings(
    w: &mut Writer,
    items: &[Node],
    style: Style,
    color: Option<&str>,
    size: Option<f64>,
) {
    if items.is_empty() {
        return;
    }
    let wrap = color.is_some() || size.is_some() || style_needs_mstyle(style);
    if wrap {
        let ds = if matches!(style, Style::Display) {
            "true"
        } else {
            "false"
        };
        let sl = match style {
            Style::Display | Style::Text => "0",
            Style::Script => "1",
            Style::ScriptScript => "2",
        };
        // An <mstyle> here carries at most MSTYLE_MAX_ATTRS attributes
        // (displaystyle, scriptlevel, mathcolor, mathsize), so a
        // fixed-capacity stack array + len replaces the old per-group heap
        // Vec, and the mathsize percentage is written digit-by-digit into a
        // stack buffer instead of a format! String.
        let mut pct_buf = [0u8; PERCENT_BUF_LEN];
        let size_val = size.map(|f| write_percent(&mut pct_buf, f));
        let mut attrs: [(&str, &str); MSTYLE_MAX_ATTRS] = [("", ""); MSTYLE_MAX_ATTRS];
        let mut n_attrs = 0usize;
        if style_needs_mstyle(style) {
            attrs[n_attrs] = ("displaystyle", ds);
            n_attrs += 1;
            attrs[n_attrs] = ("scriptlevel", sl);
            n_attrs += 1;
        }
        if let Some(c) = color {
            attrs[n_attrs] = ("mathcolor", c);
            n_attrs += 1;
        }
        if let Some(s) = size_val {
            attrs[n_attrs] = ("mathsize", s);
            n_attrs += 1;
        }
        w.open("mstyle", &attrs[..n_attrs]);
        for n in items {
            emit_node(w, n, style);
        }
        w.close("mstyle");
    } else {
        for n in items {
            emit_node(w, n, style);
        }
    }
}

fn style_needs_mstyle(style: Style) -> bool {
    !matches!(style, Style::Display | Style::Text)
}

/// An `<mstyle>` emitted by `emit_styled_siblings` carries at most four
/// attributes: displaystyle, scriptlevel, mathcolor, mathsize.
const MSTYLE_MAX_ATTRS: usize = 4;

/// Longest possible `mathsize` value: sign + 10 digits + `%` (the clamp
/// bounds the value to `-10000%` = 7 bytes, but the buffer is sized for any
/// i32 so the digit writer cannot overflow).
const PERCENT_BUF_LEN: usize = 12;

/// Write the `mathsize` percentage for a size factor into `buf` and return
/// it as a `&str`, avoiding a per-styled-group `format!` allocation.
/// Byte-identical to the previous `format!("{n}%")` shape where `n` is the
/// rounded, clamped percentage — locked by the
/// `percent_digits_match_format_reference` test.
fn write_percent(buf: &mut [u8; PERCENT_BUF_LEN], factor: f64) -> &str {
    let pct = (factor * 100.0).round();
    let n = if pct.is_finite() {
        pct.clamp(-10_000.0, 10_000.0) as i32
    } else {
        100
    };
    let mut len = 0usize;
    if n < 0 {
        buf[len] = b'-';
        len += 1;
    }
    let mut mag = n.unsigned_abs();
    let mut digits = [0u8; 10];
    let mut nd = 0usize;
    loop {
        digits[nd] = b'0' + (mag % 10) as u8;
        nd += 1;
        mag /= 10;
        if mag == 0 {
            break;
        }
    }
    while nd > 0 {
        nd -= 1;
        buf[len] = digits[nd];
        len += 1;
    }
    buf[len] = b'%';
    len += 1;
    std::str::from_utf8(&buf[..len]).unwrap_or_default()
}

fn emit_symbol(w: &mut Writer, ch: char, class: AtomClass) {
    let tag = symbol_tag(ch, class);
    w.open(tag, &[]);
    w.char_text(ch);
    w.close(tag);
}

fn symbol_tag(ch: char, class: AtomClass) -> &'static str {
    match class {
        AtomClass::Ord if ch.is_ascii_digit() => "mn",
        AtomClass::Ord => "mi",
        AtomClass::Op
        | AtomClass::Bin
        | AtomClass::Rel
        | AtomClass::Open
        | AtomClass::Close
        | AtomClass::Punct
        | AtomClass::Inner => "mo",
    }
}

fn emit_scripts(
    w: &mut Writer,
    base: Option<&Node>,
    sub: Option<&Node>,
    sup: Option<&Node>,
    primes: &[Span],
    style: Style,
) {
    let limits = scripts_as_limits(base, style);
    let has_primes = !primes.is_empty();
    let has_sub = sub.is_some();
    let has_sup = sup.is_some() || has_primes;
    if !has_sub && !has_sup {
        match base {
            Some(b) => emit_node(w, b, style),
            None => {
                w.open("mrow", &[]);
                w.close("mrow");
            }
        }
        return;
    }
    let tag = if limits {
        match (has_sub, has_sup) {
            (true, true) => "munderover",
            (true, false) => "munder",
            (false, true) => "mover",
            (false, false) => "mrow",
        }
    } else {
        match (has_sub, has_sup) {
            (true, true) => "msubsup",
            (true, false) => "msub",
            (false, true) => "msup",
            (false, false) => "mrow",
        }
    };
    w.open(tag, &[]);
    match base {
        Some(b) => emit_node(w, b, style),
        None => {
            w.open("mrow", &[]);
            w.close("mrow");
        }
    }
    if has_sub {
        if let Some(s) = sub {
            emit_node(w, s, style);
        }
    }
    if has_sup {
        emit_superscript(w, sup, primes, style);
    }
    w.close(tag);
}

fn emit_superscript(w: &mut Writer, sup: Option<&Node>, primes: &[Span], style: Style) {
    if primes.is_empty() {
        if let Some(s) = sup {
            emit_node(w, s, style);
        }
        return;
    }
    if sup.is_none() && primes.len() == 1 {
        w.open("mo", &[]);
        w.buf.push('′');
        w.close("mo");
        return;
    }
    w.open("mrow", &[]);
    for _ in primes {
        w.open("mo", &[]);
        w.buf.push('′');
        w.close("mo");
    }
    if let Some(s) = sup {
        emit_node(w, s, style);
    }
    w.close("mrow");
}

#[inline(always)]
fn scripts_as_limits(base: Option<&Node>, style: Style) -> bool {
    let Some(node) = base else {
        return false;
    };
    match &node.kind {
        NodeKind::BigOp {
            limits, integral, ..
        } => match limits {
            Limits::Limits => true,
            Limits::NoLimits => false,
            Limits::Default => !*integral && matches!(style, Style::Display),
        },
        NodeKind::OpName { limits, .. } => *limits && matches!(style, Style::Display),
        _ => false,
    }
}

fn emit_frac(
    w: &mut Writer,
    num: &Node,
    den: &Node,
    bar: bool,
    delims: Option<(char, char)>,
    forced_style: Option<Style>,
    style: Style,
) {
    let wrap_style = forced_style;
    if let Some(st) = wrap_style {
        let ds = if matches!(st, Style::Display) {
            "true"
        } else {
            "false"
        };
        w.open("mstyle", &[("displaystyle", ds)]);
        emit_frac_body(w, num, den, bar, delims, style);
        w.close("mstyle");
    } else {
        emit_frac_body(w, num, den, bar, delims, style);
    }
}

fn emit_frac_body(
    w: &mut Writer,
    num: &Node,
    den: &Node,
    bar: bool,
    delims: Option<(char, char)>,
    style: Style,
) {
    if let Some((left, right)) = delims {
        w.open("mrow", &[]);
        emit_fence(w, left);
        emit_mfrac(w, num, den, bar, style);
        emit_fence(w, right);
        w.close("mrow");
    } else {
        emit_mfrac(w, num, den, bar, style);
    }
}

fn emit_mfrac(w: &mut Writer, num: &Node, den: &Node, bar: bool, style: Style) {
    if bar {
        w.open("mfrac", &[]);
    } else {
        w.open("mfrac", &[("linethickness", "0")]);
    }
    emit_node(w, num, style);
    emit_node(w, den, style);
    w.close("mfrac");
}

fn emit_fence(w: &mut Writer, ch: char) {
    w.open("mo", &[("fence", "true"), ("stretchy", "true")]);
    w.char_text(ch);
    w.close("mo");
}

fn emit_radical(w: &mut Writer, index: Option<&Node>, radicand: &Node, style: Style) {
    if let Some(ix) = index {
        w.open("mroot", &[]);
        emit_node(w, radicand, style);
        emit_node(w, ix, style);
        w.close("mroot");
    } else {
        w.open("msqrt", &[]);
        emit_node(w, radicand, style);
        w.close("msqrt");
    }
}

fn emit_accent(w: &mut Writer, accent: AccentKind, base: &Node, style: Style) {
    let tag = if accent.is_over() { "mover" } else { "munder" };
    let stretchy = matches!(
        accent,
        AccentKind::WideHat
            | AccentKind::WideTilde
            | AccentKind::OverLine
            | AccentKind::UnderLine
            | AccentKind::OverBrace
            | AccentKind::UnderBrace
            | AccentKind::OverRightArrow
            | AccentKind::OverLeftArrow
    );
    w.open(tag, &[]);
    emit_node(w, base, style);
    if stretchy {
        w.open("mo", &[("stretchy", "true")]);
    } else {
        w.open("mo", &[]);
    }
    w.text(accent_char(accent));
    w.close("mo");
    w.close(tag);
}

#[inline(always)]
fn accent_char(kind: AccentKind) -> &'static str {
    match kind {
        AccentKind::Hat | AccentKind::WideHat => "\u{02C6}",
        AccentKind::Check => "\u{02C7}",
        AccentKind::Tilde | AccentKind::WideTilde => "\u{02DC}",
        AccentKind::Acute => "\u{00B4}",
        AccentKind::Grave => "`",
        AccentKind::Dot => "\u{02D9}",
        AccentKind::Ddot => "\u{00A8}",
        AccentKind::Breve => "\u{02D8}",
        AccentKind::Bar => "\u{00AF}",
        AccentKind::Vec | AccentKind::OverRightArrow => "\u{2192}",
        AccentKind::Dddot => "\u{20DB}",
        AccentKind::Ddddot => "\u{20DC}",
        AccentKind::Ring => "\u{02DA}",
        AccentKind::OverLine => "\u{203E}",
        AccentKind::UnderLine => "_",
        AccentKind::OverBrace => "\u{23DE}",
        AccentKind::UnderBrace => "\u{23DF}",
        AccentKind::OverLeftArrow => "\u{2190}",
    }
}

fn emit_left_right(w: &mut Writer, left: &Delim, right: &Delim, body: &[Node], style: Style) {
    w.open("mrow", &[]);
    if let Some(ch) = left.ch {
        emit_fence(w, ch);
    }
    emit_run(w, body, style, None, None);
    if let Some(ch) = right.ch {
        emit_fence(w, ch);
    }
    w.close("mrow");
}

fn emit_mtext_nodes(w: &mut Writer, body: &[Node]) {
    w.open("mtext", &[]);
    collect_text(w, body);
    w.close("mtext");
}

fn collect_text(w: &mut Writer, body: &[Node]) {
    for n in body {
        match &n.kind {
            NodeKind::TextRun { text, .. } => w.text(text),
            NodeKind::Symbol { ch, .. } => w.char_text(*ch),
            NodeKind::List(items) | NodeKind::Text { body: items } => collect_text(w, items),
            NodeKind::TextStyled { body, .. } => collect_text(w, body),
            NodeKind::Space(_) => w.buf.push(' '),
            NodeKind::Tie => w.buf.push('\u{00A0}'),
            _ => {}
        }
    }
}

fn emit_text_styled(w: &mut Writer, ts: TextStyle, body: &[Node]) {
    let variant = match ts {
        TextStyle::Bold => "bold",
        TextStyle::Emph => "italic",
        TextStyle::Underline => "normal",
    };
    w.open("mtext", &[("mathvariant", variant)]);
    collect_text(w, body);
    w.close("mtext");
}

fn math_font_variant(font: MathFont) -> &'static str {
    match font {
        MathFont::Blackboard => "double-struck",
        MathFont::Calligraphic => "script",
        MathFont::Roman => "normal",
        MathFont::Bold => "bold",
        MathFont::BoldItalic => "bold-italic",
        MathFont::SansSerif => "sans-serif",
        MathFont::Typewriter => "monospace",
        MathFont::Italic => "italic",
    }
}

fn emit_phantom(w: &mut Writer, kind: PhantomKind, body: &Node, style: Style) {
    match kind {
        PhantomKind::Full => {
            w.open("mphantom", &[]);
            emit_node(w, body, style);
            w.close("mphantom");
        }
        PhantomKind::Horizontal => {
            w.open("mpadded", &[("height", "0"), ("depth", "0")]);
            w.open("mphantom", &[]);
            emit_node(w, body, style);
            w.close("mphantom");
            w.close("mpadded");
        }
        PhantomKind::Vertical => {
            w.open("mpadded", &[("width", "0")]);
            w.open("mphantom", &[]);
            emit_node(w, body, style);
            w.close("mphantom");
            w.close("mpadded");
        }
    }
}

fn emit_stack(w: &mut Writer, kind: StackKind, annotation: &Node, base: &Node, style: Style) {
    let tag = match kind {
        StackKind::Stackrel | StackKind::Overset => "mover",
        StackKind::Underset => "munder",
    };
    w.open(tag, &[]);
    emit_node(w, base, style);
    emit_node(w, annotation, style);
    w.close(tag);
}

fn emit_xarrow(w: &mut Writer, mapsto: bool, above: &Node, below: Option<&Node>, style: Style) {
    let arrow = if mapsto { "\u{21A6}" } else { "\u{2192}" };
    let tag = if below.is_some() {
        "munderover"
    } else {
        "mover"
    };
    w.open(tag, &[]);
    w.open("mo", &[("stretchy", "true")]);
    w.text(arrow);
    w.close("mo");
    if let Some(b) = below {
        emit_node(w, b, style);
    }
    emit_node(w, above, style);
    w.close(tag);
}

fn emit_space(w: &mut Writer, kind: SpaceKind) {
    let width = em_from_mu(kind.mu());
    w.open("mspace", &[("width", &width)]);
    w.close("mspace");
}

fn em_from_mu(mu: i32) -> String {
    // width = mu/18 em, rounded to thousandths.
    let sign = if mu < 0 { -1 } else { 1 };
    let milli = if mu == 0 {
        0
    } else {
        (mu * 1000 + 9 * sign) / 18
    };
    let mut s = String::new();
    if milli < 0 {
        s.push('-');
    }
    let abs = milli.unsigned_abs();
    let whole = abs / 1000;
    let frac = abs % 1000;
    s.push_str(&whole.to_string());
    if frac != 0 {
        s.push('.');
        if frac < 100 {
            s.push('0');
        }
        if frac < 10 {
            s.push('0');
        }
        s.push_str(&frac.to_string());
        while s.ends_with('0') && s.contains('.') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s.push_str("em");
    s
}

fn emit_environment(
    w: &mut Writer,
    name: &str,
    spec: Option<&str>,
    rows: &[Vec<Node>],
    style: Style,
) {
    let fences = env_fences(name);
    let columnalign = env_columnalign(name, spec, column_count(rows));
    if let Some((left, right)) = fences {
        w.open("mrow", &[]);
        if let Some(ch) = left {
            emit_fence(w, ch);
        }
        emit_table(w, rows, columnalign.as_deref(), style);
        if let Some(ch) = right {
            emit_fence(w, ch);
        }
        w.close("mrow");
    } else {
        emit_table(w, rows, columnalign.as_deref(), style);
    }
}

#[inline(always)]
fn env_fences(name: &str) -> Option<(Option<char>, Option<char>)> {
    match name {
        "pmatrix" => Some((Some('('), Some(')'))),
        "bmatrix" => Some((Some('['), Some(']'))),
        "Bmatrix" => Some((Some('{'), Some('}'))),
        "vmatrix" => Some((Some('|'), Some('|'))),
        "Vmatrix" => Some((Some('\u{2016}'), Some('\u{2016}'))),
        "cases" => Some((Some('{'), None)),
        _ => None,
    }
}

fn column_count(rows: &[Vec<Node>]) -> usize {
    rows.iter().map(Vec::len).max().unwrap_or(0)
}

fn env_columnalign(name: &str, spec: Option<&str>, cols: usize) -> Option<String> {
    if let Some(spec) = spec {
        let mut parts = Vec::new();
        for ch in spec.chars() {
            match ch {
                'l' => parts.push("left"),
                'r' => parts.push("right"),
                'c' => parts.push("center"),
                _ => {}
            }
        }
        if !parts.is_empty() {
            return Some(parts.join(" "));
        }
    }
    match name {
        "align" | "align*" | "aligned" => {
            if cols == 0 {
                return None;
            }
            let mut parts = Vec::with_capacity(cols);
            for i in 0..cols {
                parts.push(if i % 2 == 0 { "right" } else { "left" });
            }
            Some(parts.join(" "))
        }
        "cases" => Some("left left".to_owned()),
        _ => None,
    }
}

fn emit_align_block(w: &mut Writer, lines: &[Node], style: Style) {
    w.open("mtable", &[]);
    for line in lines {
        w.open("mtr", &[]);
        w.open("mtd", &[]);
        emit_node(w, line, style);
        w.close("mtd");
        w.close("mtr");
    }
    w.close("mtable");
}

fn emit_table(w: &mut Writer, rows: &[Vec<Node>], columnalign: Option<&str>, style: Style) {
    if let Some(align) = columnalign {
        w.open("mtable", &[("columnalign", align)]);
    } else {
        w.open("mtable", &[]);
    }
    let width = column_count(rows);
    for row in rows {
        w.open("mtr", &[]);
        for i in 0..width {
            w.open("mtd", &[]);
            if let Some(cell) = row.get(i) {
                emit_node(w, cell, style);
            }
            w.close("mtd");
        }
        w.close("mtr");
    }
    w.close("mtable");
}

fn emit_fragment(w: &mut Writer, kind: &FragmentKind) {
    match kind {
        FragmentKind::UnmatchedClose | FragmentKind::RedundantMathShift => {}
        FragmentKind::StrayRight(delim) => {
            if let Some(ch) = delim.ch {
                emit_fence(w, ch);
            }
        }
    }
}

fn check_well_formed(xml: &str) -> Result<(), String> {
    let bytes = xml.as_bytes();
    let mut i = 0;
    let mut stack: Vec<(String, usize)> = Vec::new();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let start = i;
            i += 1;
            if i >= bytes.len() {
                return Err("truncated tag".to_owned());
            }
            if bytes[i] == b'/' {
                i += 1;
                let name = read_name(bytes, &mut i)?;
                skip_ws(bytes, &mut i);
                if bytes.get(i).copied() != Some(b'>') {
                    return Err(format!("malformed close tag at {start}"));
                }
                i += 1;
                match stack.pop() {
                    Some((open, _)) if open == name => {}
                    Some((open, at)) => {
                        return Err(format!(
                            "close </{name}> at {start} does not match <{open}> opened at {at}"
                        ));
                    }
                    None => return Err(format!("unmatched close </{name}> at {start}")),
                }
            } else {
                let name = read_name(bytes, &mut i)?;
                read_attrs(bytes, &mut i)?;
                if bytes.get(i).copied() == Some(b'/') {
                    return Err(format!(
                        "self-closing tag <{name}/> at {start} is forbidden"
                    ));
                }
                if bytes.get(i).copied() != Some(b'>') {
                    return Err(format!("unterminated open tag <{name}> at {start}"));
                }
                i += 1;
                stack.push((name, start));
            }
        } else {
            // Text: reject raw '<' (handled) and bare '&'.
            if bytes[i] == b'&' {
                i += 1;
                consume_entity(bytes, &mut i)?;
            } else {
                i += 1;
            }
        }
    }
    if let Some((open, at)) = stack.last() {
        return Err(format!("unclosed <{open}> opened at {at}"));
    }
    Ok(())
}

fn read_name(bytes: &[u8], i: &mut usize) -> Result<String, String> {
    let start = *i;
    if *i >= bytes.len() || !bytes[*i].is_ascii_alphabetic() {
        return Err(format!("expected tag name at {start}"));
    }
    *i += 1;
    while *i < bytes.len() && (bytes[*i].is_ascii_alphanumeric() || bytes[*i] == b'-') {
        *i += 1;
    }
    let name = core::str::from_utf8(&bytes[start..*i]).map_err(|_| "non-utf8 tag name")?;
    Ok(name.to_owned())
}

fn skip_ws(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

fn read_attrs(bytes: &[u8], i: &mut usize) -> Result<(), String> {
    loop {
        skip_ws(bytes, i);
        if *i >= bytes.len() {
            return Err("truncated attributes".to_owned());
        }
        match bytes[*i] {
            b'>' | b'/' => return Ok(()),
            b'a'..=b'z' | b'A'..=b'Z' => {
                let _ = read_name(bytes, i)?;
                skip_ws(bytes, i);
                if bytes.get(*i).copied() != Some(b'=') {
                    return Err("attribute missing '='".to_owned());
                }
                *i += 1;
                skip_ws(bytes, i);
                if bytes.get(*i).copied() != Some(b'"') {
                    return Err("attribute value must be double-quoted".to_owned());
                }
                *i += 1;
                while *i < bytes.len() && bytes[*i] != b'"' {
                    if bytes[*i] == b'&' {
                        *i += 1;
                        consume_entity(bytes, i)?;
                    } else if bytes[*i] == b'<' {
                        return Err("raw '<' in attribute".to_owned());
                    } else {
                        *i += 1;
                    }
                }
                if bytes.get(*i).copied() != Some(b'"') {
                    return Err("unterminated attribute value".to_owned());
                }
                *i += 1;
            }
            _ => return Err(format!("unexpected byte 0x{:02x} in tag", bytes[*i])),
        }
    }
}

fn consume_entity(bytes: &[u8], i: &mut usize) -> Result<(), String> {
    let start = *i;
    if bytes.get(*i).copied() == Some(b'#') {
        *i += 1;
        let hex = bytes.get(*i).copied() == Some(b'x') || bytes.get(*i).copied() == Some(b'X');
        if hex {
            *i += 1;
        }
        let digit_start = *i;
        while *i < bytes.len() {
            let b = bytes[*i];
            let ok = if hex {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            };
            if !ok {
                break;
            }
            *i += 1;
        }
        if *i == digit_start {
            return Err("empty numeric entity".to_owned());
        }
    } else {
        while *i < bytes.len() && bytes[*i].is_ascii_alphabetic() {
            *i += 1;
        }
        if *i == start {
            return Err("bare '&'".to_owned());
        }
    }
    if bytes.get(*i).copied() != Some(b';') {
        return Err("entity missing ';'".to_owned());
    }
    *i += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PERCENT_BUF_LEN, em_from_mu, write_percent};

    #[test]
    fn em_from_mu_thousandths() {
        assert_eq!(em_from_mu(3), "0.167em");
        assert_eq!(em_from_mu(18), "1em");
        assert_eq!(em_from_mu(-3), "-0.167em");
        assert_eq!(em_from_mu(0), "0em");
    }

    /// The pre-R1 reference implementation: the old `percent_size` body
    /// (`format!`-based). `write_percent` must reproduce it byte-for-byte.
    fn percent_size_ref(factor: f64) -> String {
        let pct = (factor * 100.0).round();
        let n = if pct.is_finite() {
            pct.clamp(-10_000.0, 10_000.0) as i32
        } else {
            100
        };
        format!("{n}%")
    }

    #[test]
    fn percent_digits_match_format_reference() {
        let mut buf = [0u8; PERCENT_BUF_LEN];
        // Dense sweep over the clamping range (factor 100 -> pct 10_000) and
        // beyond it on both sides; 0.0173 steps avoid landing only on
        // representable round numbers.
        let mut factors: Vec<f64> = Vec::new();
        let mut f = -300.0;
        while f <= 300.0 {
            factors.push(f);
            f += 0.0173;
        }
        factors.extend([
            -0.0,
            0.0,
            0.004_999,
            0.005,
            0.494_999,
            0.495,
            0.994_999,
            0.995,
            1.0,
            9.995,
            10.0,
            99.995,
            100.0,
            1e3,
            1e5,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MIN,
            f64::MAX,
        ]);
        for f in factors {
            assert_eq!(
                write_percent(&mut buf, f),
                percent_size_ref(f),
                "factor {f}"
            );
        }
        // Exact clamp corners and the zero/ten-thousand boundaries.
        for f in [-100.0, 100.0, -100.0 - 1e-9, 100.0 + 1e-9, 0.0, 10.0, -10.0] {
            assert_eq!(
                write_percent(&mut buf, f),
                percent_size_ref(f),
                "clamp corner {f}"
            );
        }
        // Spot-check the literal byte shapes.
        assert_eq!(write_percent(&mut buf, 0.0), "0%");
        assert_eq!(write_percent(&mut buf, 0.5), "50%");
        assert_eq!(write_percent(&mut buf, 1.0), "100%");
        assert_eq!(write_percent(&mut buf, 2.074), "207%");
        assert_eq!(write_percent(&mut buf, 100.0), "10000%");
        assert_eq!(write_percent(&mut buf, -0.8), "-80%");
        assert_eq!(write_percent(&mut buf, f64::NAN), "100%");
    }
}
