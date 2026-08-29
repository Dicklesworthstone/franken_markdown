//! Tests for `src/svg.rs` (standalone vector-SVG poster backend). Included
//! via `#[path]` so they run standalone before the module is registered in
//! `lib.rs`. Tests may unwrap/panic for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/svg.rs"]
mod svg;

use franken_markdown::parse_markdown;
use franken_markdown::theme::{Theme, ThemeColors};
use svg::{SvgOptions, render_svg, render_svg_with_report};

fn render_str(src: &str) -> (String, svg::SvgReport) {
    let doc = parse_markdown(src);
    let (bytes, report) = render_svg_with_report(&doc, &SvgOptions::default());
    (String::from_utf8(bytes).unwrap(), report)
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

/// Parse `<path id="gN" d="..."/>` def lines out of the `<defs>` block.
fn defs(svg: &str) -> Vec<(String, String)> {
    svg.lines()
        .filter(|line| line.starts_with("<path id=\""))
        .map(|line| {
            let id_start = "<path id=\"".len();
            let id_end = line[id_start..].find('"').unwrap() + id_start;
            let d_start = line.find(" d=\"").unwrap() + 4;
            let d_end = line[d_start..].find('"').unwrap() + d_start;
            (
                line[id_start..id_end].to_string(),
                line[d_start..d_end].to_string(),
            )
        })
        .collect()
}

/// Well-formedness scan: balanced element stack, double-quoted attributes,
/// no unescaped markup characters in text or attribute values.
fn assert_well_formed(svg: &str) {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = svg;
    while let Some(open) = rest.find('<') {
        let text = &rest[..open];
        assert!(
            text.trim().is_empty(),
            "unexpected text node {text:?} (poster must be elements only)"
        );
        let tag_src = &rest[open..];
        // Find the tag end, honouring quoted attribute values.
        let mut in_quote = false;
        let mut close = None;
        for (i, ch) in tag_src.char_indices().skip(1) {
            match ch {
                '"' => in_quote = !in_quote,
                '>' if !in_quote => {
                    close = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let close = close.unwrap_or_else(|| panic!("unterminated tag: {tag_src:?}"));
        let tag = &tag_src[1..close];
        if let Some(decl) = tag.strip_prefix('?') {
            assert!(decl.ends_with('?'), "malformed PI: {tag:?}");
        } else if let Some(name) = tag.strip_prefix('/') {
            let top = stack
                .pop()
                .unwrap_or_else(|| panic!("stack underflow at </{name}>"));
            assert_eq!(top, name.trim(), "mismatched close tag </{name}>");
        } else {
            let self_closing = tag.ends_with('/');
            let body = tag.strip_suffix('/').unwrap_or(tag);
            let mut parts = body.split_whitespace();
            let name = parts.next().unwrap_or_else(|| panic!("empty tag: {tag:?}"));
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric()),
                "bad element name {name:?}"
            );
            // Every attribute must be name="value" with escaped content.
            let attrs = &body[name.len()..];
            let mut chars = attrs.chars().peekable();
            loop {
                while chars.peek().is_some_and(|&c| c.is_whitespace()) {
                    chars.next();
                }
                if chars.peek().is_none() {
                    break;
                }
                let mut attr_name = String::new();
                for c in chars.by_ref() {
                    if c == '=' {
                        break;
                    }
                    attr_name.push(c);
                }
                let attr_name = attr_name.trim();
                assert!(
                    !attr_name.is_empty()
                        && attr_name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':'),
                    "bad attribute name {attr_name:?} in <{name}>"
                );
                assert_eq!(chars.next(), Some('"'), "attribute {attr_name} not quoted");
                let mut value = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    value.push(c);
                }
                // Raw markup characters must be entity-escaped in values.
                assert!(!value.contains('<'), "raw < in attribute of <{name}>");
                for (i, _) in value.match_indices('&') {
                    let tail = &value[i..];
                    assert!(
                        tail.starts_with("&amp;")
                            || tail.starts_with("&lt;")
                            || tail.starts_with("&gt;")
                            || tail.starts_with("&quot;")
                            || tail.starts_with("&apos;"),
                        "unescaped & in attribute of <{name}>: {value:?}"
                    );
                }
            }
            if !self_closing {
                stack.push(name.to_string());
            }
        }
        rest = &tag_src[close + 1..];
    }
    assert!(rest.trim().is_empty(), "trailing text {rest:?}");
    assert!(stack.is_empty(), "unclosed elements: {stack:?}");
}

const ALL_BLOCKS: &str = "# Title\n\n\
A paragraph with **bold**, *italic*, ***both***, `code`, ~~strike~~, and a\n\
[link](https://example.com) plus a ![image](x.png) and a footnote[^a].\n\n\
Soft break\nline continues.  \nHard break above.\n\n\
## Section\n\n\
- bullet one\n- bullet two with a longer run of text meant to wrap around the\n  measure at least once in the poster\n  - nested\n\n\
1. first\n2. second\n\n\
- [x] done\n- [ ] todo\n\n\
> A quoted paragraph with **emphasis**.\n>\n> - quoted item\n\n\
```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n\
| Name | Value | Note |\n|:-----|------:|------|\n| a | 1 | left |\n| b | 22 | right |\n\n\
---\n\n\
$$\nx^2 + y^2 = z^2\n$$\n\n\
Term\n: A definition of the term.\n\n\
[^a]: footnote text\n";

#[test]
fn well_formed_showcase() {
    let src = std::fs::read_to_string("examples/showcase.md").unwrap();
    let (out, _) = render_str(&src);
    assert_well_formed(&out);
}

#[test]
fn well_formed_all_blocks() {
    let (out, report) = render_str(ALL_BLOCKS);
    assert_well_formed(&out);
    assert!(
        report.glyphs_drawn > 200,
        "poster should be dense: {report:?}"
    );
    assert!(report.paths_emitted > 30, "many unique glyphs: {report:?}");
}

#[test]
fn defs_dedup_and_report_consistency() {
    // Bold 'A' at heading size (24pt) and body size (11pt) is the same
    // (face, glyph) outline and must share one def across sizes.
    let (out, report) = render_str("# A\n\n**A** **A**\n");
    let defs = defs(&out);
    assert_eq!(defs.len(), report.paths_emitted);
    assert_eq!(count_occurrences(&out, "<path id=\""), report.paths_emitted);
    assert_eq!(count_occurrences(&out, "<use href="), report.glyphs_drawn);
    // Exactly one def for bold 'A', referenced three times (h1 + 2 body).
    assert_eq!(defs.len(), 1, "one unique glyph: {defs:?}");
    let (id, _) = &defs[0];
    assert_eq!(count_occurrences(&out, &format!("href=\"#{id}\"")), 3);
}

#[test]
fn determinism_byte_identical() {
    let src = std::fs::read_to_string("examples/showcase.md").unwrap();
    let doc = parse_markdown(&src);
    let a = render_svg(&doc, &SvgOptions::default());
    let b = render_svg(&doc, &SvgOptions::default());
    assert_eq!(a, b, "fixed input must render byte-identically");
    let doc2 = parse_markdown(ALL_BLOCKS);
    let c = render_svg(&doc2, &SvgOptions::default());
    let d = render_svg(&doc2, &SvgOptions::default());
    assert_eq!(c, d, "synthetic all-blocks doc must be deterministic");
}

#[test]
fn known_glyph_path_fingerprint() {
    let (out, report) = render_str("A");
    assert_eq!(report.paths_emitted, 1);
    let defs = defs(&out);
    assert_eq!(defs.len(), 1);
    // IBM Plex Sans Regular, 'A' (gid from cmap), 100 pt em, y-flipped,
    // 0.01-quantized. Pinned so the glyf→path transform cannot drift.
    assert_eq!(
        defs[0].1,
        "M53.00 0.00L46.00 -20.60L17.80 -20.60L10.80 0.00L2.30 0.00L26.70 -69.80L37.40 -69.80L61.80 0.00L53.00 0.00ZM32.10 -62.00L31.60 -62.00L19.80 -28.00L43.90 -28.00L32.10 -62.00Z"
    );
}

#[test]
fn showcase_ascii_zero_missing() {
    let src = std::fs::read_to_string("examples/showcase.md").unwrap();
    let ascii: String = src.chars().filter(char::is_ascii).collect();
    let (_, report) = render_str(&ascii);
    assert_eq!(
        report.glyphs_missing, 0,
        "ASCII content must be fully covered"
    );
}

#[test]
fn showcase_full_coverage() {
    // The showcase's only non-ASCII characters are en/em dashes, which the
    // bundled body face maps.
    let src = std::fs::read_to_string("examples/showcase.md").unwrap();
    let (_, report) = render_str(&src);
    assert_eq!(
        report.glyphs_missing, 0,
        "showcase should render with no gaps"
    );
}

#[test]
fn headings_are_paths_not_text() {
    let (out, _) = render_str("# Hello\n\nbody\n");
    assert!(
        !out.contains("<text"),
        "poster must not use <text> elements"
    );
    // H1 is 24 pt on the 100 pt def em.
    assert!(out.contains("scale(0.24)"), "heading size scale present");
    // Body is 11 pt.
    assert!(out.contains("scale(0.11)"), "body size scale present");
}

#[test]
fn missing_glyphs_counted_and_skipped() {
    // U+0378 is permanently unassigned; no bundled face can map it.
    let (out, report) = render_str("A\u{0378}B");
    assert_eq!(report.glyphs_missing, 1);
    assert_eq!(report.glyphs_drawn, 2);
    assert_eq!(report.paths_emitted, 2, "only A and B get defs: {out}");
}

#[test]
fn theme_colors_are_attribute_escaped() {
    let mut colors = ThemeColors::light();
    colors.accent = "\"><script>alert(1)</script>".to_string();
    let theme = Theme {
        colors,
        ..Theme::default()
    };
    let doc = parse_markdown("[click](https://example.com)");
    let (bytes, _) = render_svg_with_report(
        &doc,
        &SvgOptions {
            theme,
            ..SvgOptions::default()
        },
    );
    let out = String::from_utf8(bytes).unwrap();
    assert!(
        !out.contains("<script>"),
        "injection must be escaped: {out}"
    );
    assert!(
        out.contains("&quot;&gt;&lt;script&gt;"),
        "escaped form present"
    );
    assert_well_formed(&out);
}
