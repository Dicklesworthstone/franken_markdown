//! JSX structural scanning layered on the existing JavaScript lexer.
//!
//! This scanner owns markup/text/expression boundaries, not JavaScript token
//! classification. JavaScript slices are disjoint and delegated once each.
//! Explicit stacks avoid recursive descent on untrusted code-fence contents.

use crate::highlight::{Span, Tok};
use crate::lang_javascript::lex_javascript_into;

const MAX_CONTEXTS: usize = 256;

#[derive(Clone, Copy)]
enum Frame {
    JavaScript {
        braced: bool,
        opening: usize,
        braces: usize,
        after_value: bool,
    },
    Markup {
        opening: usize,
        depth: usize,
        in_tag: bool,
        closing: bool,
    },
}

struct Emitter<'a> {
    output: Option<&'a mut Vec<Span>>,
    end: usize,
}

impl Emitter<'_> {
    fn emit(&mut self, kind: Tok, start: usize, end: usize) {
        if start == end {
            return;
        }
        debug_assert_eq!(start, self.end);
        debug_assert!(start < end);
        if let Some(output) = self.output.as_mut() {
            output.push(Span { kind, start, end });
        }
        self.end = end;
    }

    fn javascript(&mut self, code: &str, start: usize, end: usize, after_value: bool) {
        if start == end {
            return;
        }
        if self.output.is_none() {
            self.end = end;
            return;
        }
        // A completed JSX expression is a JavaScript value. Seed that lexical
        // context when resuming, so `/` remains division rather than opening
        // a regex. The synthetic bytes never enter the returned source spans.
        let seeded;
        let (source, prefix) = if after_value {
            seeded = format!("0 {}", &code[start..end]);
            (seeded.as_str(), 2)
        } else {
            (&code[start..end], 0)
        };
        let mut spans = Vec::new();
        lex_javascript_into(source, &mut spans);
        for span in spans {
            if span.end <= prefix {
                continue;
            }
            let from = start + span.start.max(prefix) - prefix;
            let to = start + span.end - prefix;
            self.emit(span.kind, from, to);
        }
    }
}

/// Append source-relative tiling spans, or scan without allocating spans when
/// only the conservative incremental hold boundary is needed. A still-open
/// JSX element is retained from its opener so a later chunk keeps its context.
pub(super) fn scan(code: &str, output: Option<&mut Vec<Span>>) -> usize {
    let mut emitter = Emitter { output, end: 0 };
    let mut frames = vec![Frame::JavaScript {
        braced: false,
        opening: 0,
        braces: 0,
        after_value: false,
    }];
    let mut pos = 0;
    let mut pending = code.len();
    'frames: while let Some(frame) = frames.pop() {
        match frame {
            Frame::JavaScript {
                braced,
                opening,
                mut braces,
                after_value,
            } => {
                let start = pos;
                let mut expects_value = !after_value;
                while pos < code.len() {
                    let byte = code.as_bytes()[pos];
                    if byte == b'<' && expects_value && starts_tag(code, pos, false) {
                        emitter.javascript(code, start, pos, after_value);
                        if frames.len() + 2 > MAX_CONTEXTS {
                            pending = pending.min(pos);
                            emitter.emit(Tok::Plain, pos, code.len());
                            pos = code.len();
                            continue 'frames;
                        }
                        frames.push(Frame::JavaScript {
                            braced,
                            opening,
                            braces,
                            after_value: true,
                        });
                        frames.push(Frame::Markup {
                            opening: pos,
                            depth: 0,
                            in_tag: true,
                            closing: false,
                        });
                        continue 'frames;
                    }
                    if byte == b'{' {
                        braces += 1;
                    } else if byte == b'}' {
                        if braced && braces == 0 {
                            emitter.javascript(code, start, pos, after_value);
                            emitter.emit(Tok::Punct, pos, pos + 1);
                            pos += 1;
                            continue 'frames;
                        }
                        braces = braces.saturating_sub(1);
                    }
                    pos = javascript_atom(code, pos, &mut expects_value);
                }
                emitter.javascript(code, start, pos, after_value);
                if braced {
                    pending = pending.min(opening);
                }
            }
            Frame::Markup {
                opening,
                mut depth,
                mut in_tag,
                mut closing,
            } => {
                let mut start = pos;
                while pos < code.len() {
                    let byte = code.as_bytes()[pos];
                    if byte == b'{' {
                        emitter.emit(if in_tag { Tok::Type } else { Tok::Plain }, start, pos);
                        emitter.emit(Tok::Punct, pos, pos + 1);
                        let expression_start = pos;
                        pos += 1;
                        if frames.len() + 2 > MAX_CONTEXTS {
                            pending = pending.min(opening);
                            emitter.emit(Tok::Plain, pos, code.len());
                            pos = code.len();
                            continue 'frames;
                        }
                        frames.push(Frame::Markup {
                            opening,
                            depth,
                            in_tag,
                            closing,
                        });
                        frames.push(Frame::JavaScript {
                            braced: true,
                            opening: expression_start,
                            braces: 0,
                            after_value: false,
                        });
                        continue 'frames;
                    }
                    if in_tag {
                        if matches!(byte, b'\'' | b'"') {
                            pos = quoted_end(code, pos, byte);
                            continue;
                        }
                        if byte == b'>' {
                            let self_closing = pos > 0 && code.as_bytes()[pos - 1] == b'/';
                            pos += 1;
                            emitter.emit(Tok::Type, start, pos);
                            if closing {
                                depth = depth.saturating_sub(1);
                            } else if !self_closing {
                                depth += 1;
                            }
                            if depth == 0 {
                                continue 'frames;
                            }
                            in_tag = false;
                            start = pos;
                            continue;
                        }
                    } else if byte == b'<' && starts_tag(code, pos, true) {
                        emitter.emit(Tok::Plain, start, pos);
                        in_tag = true;
                        closing = code.as_bytes().get(pos + 1) == Some(&b'/');
                        start = pos;
                    }
                    pos = next_char(code, pos);
                }
                emitter.emit(if in_tag { Tok::Type } else { Tok::Plain }, start, pos);
                pending = pending.min(opening);
            }
        }
    }
    debug_assert_eq!(emitter.end, code.len());
    pending
}

fn next_char(code: &str, pos: usize) -> usize {
    code.get(pos..)
        .and_then(|rest| rest.chars().next())
        .map_or(code.len(), |ch| pos + ch.len_utf8())
}

fn name_start(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '_' | '$' | '\u{200c}' | '\u{200d}')
}

fn starts_tag(code: &str, pos: usize, closing_allowed: bool) -> bool {
    let Some(rest) = code.get(pos + 1..) else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let rest = if closing_allowed {
        rest.strip_prefix('/').unwrap_or(rest)
    } else {
        rest
    };
    rest.is_empty() || rest.chars().next().is_some_and(|ch| ch == '>' || name_start(ch))
}

fn quoted_end(code: &str, start: usize, quote: u8) -> usize {
    let mut pos = start + 1;
    while pos < code.len() {
        let byte = code.as_bytes()[pos];
        pos = next_char(code, pos);
        if byte == b'\\' {
            // Advance over a scalar, not two bytes: escaped Unicode and a
            // trailing backslash must never create a non-UTF-8 slice boundary.
            pos = next_char(code, pos);
        } else if byte == quote {
            break;
        }
    }
    pos
}

fn regex_end(code: &str, start: usize) -> Option<usize> {
    let mut pos = start + 1;
    let mut in_class = false;
    while pos < code.len() {
        let byte = code.as_bytes()[pos];
        if matches!(byte, b'\r' | b'\n') {
            return None;
        }
        pos = next_char(code, pos);
        match byte {
            b'\\' => pos = next_char(code, pos),
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => {
                while let Some(ch) = code.get(pos..).and_then(|rest| rest.chars().next()) {
                    if !ch.is_alphabetic() {
                        break;
                    }
                    pos += ch.len_utf8();
                }
                return Some(pos);
            }
            _ => {}
        }
    }
    Some(pos)
}

/// Skip one JavaScript lexical atom and update expression-position context.
/// The shared JavaScript lexer remains responsible for its token kind.
fn javascript_atom(code: &str, start: usize, expects_value: &mut bool) -> usize {
    let rest = &code[start..];
    if rest.starts_with("//") || rest.starts_with("<!--") {
        return rest.find('\n').map_or(code.len(), |end| start + end);
    }
    if rest.starts_with("/*") {
        return code[start + 2..]
            .find("*/")
            .map_or(code.len(), |end| start + 2 + end + 2);
    }
    let Some(ch) = rest.chars().next() else {
        return code.len();
    };
    if ch.is_whitespace() {
        return next_char(code, start);
    }
    if matches!(ch, '\'' | '"') {
        *expects_value = false;
        return quoted_end(code, start, ch as u8);
    }
    if ch == '`' {
        *expects_value = false;
        return template_end(code, start);
    }
    if ch == '/' && *expects_value {
        if let Some(end) = regex_end(code, start) {
            *expects_value = false;
            return end;
        }
    }
    if name_start(ch) {
        let mut end = next_char(code, start);
        while let Some(ch) = code.get(end..).and_then(|rest| rest.chars().next()) {
            if !name_start(ch) && !ch.is_numeric() {
                break;
            }
            end += ch.len_utf8();
        }
        *expects_value = matches!(
            &code[start..end],
            "return" | "throw" | "yield" | "case" | "delete" | "void" | "typeof"
                | "new" | "in" | "of" | "instanceof" | "await" | "else" | "do"
        );
        return end;
    }
    if ch.is_ascii_digit() {
        let mut end = start + 1;
        while code
            .as_bytes()
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        {
            end += 1;
        }
        *expects_value = false;
        return end;
    }
    if rest.starts_with("++") || rest.starts_with("--") {
        // A postfix increment remains a value; a prefix increment still
        // expects its operand. Do not mistake `value++ < Other` for markup.
        return start + 2;
    }
    *expects_value = !matches!(ch, ')' | ']' | '.');
    next_char(code, start)
}

#[derive(Clone, Copy)]
enum TemplateFrame {
    Text,
    Expression { braces: usize, expects_value: bool },
}

/// Keep strings, comments, regexes and nested templates inside ${...} atomic
/// for JSX boundary detection. Template highlighting is delegated unchanged
/// to the JavaScript lexer; JSX inside template substitutions is not parsed
/// as markup by this layer.
fn template_end(code: &str, start: usize) -> usize {
    let mut frames = vec![TemplateFrame::Text];
    let mut pos = start + 1;
    while pos < code.len() {
        let Some(frame) = frames.pop() else {
            return pos;
        };
        let byte = code.as_bytes()[pos];
        match frame {
            TemplateFrame::Text => {
                if byte == b'`' {
                    pos += 1;
                    if frames.is_empty() {
                        return pos;
                    }
                } else if byte == b'$' && code.as_bytes().get(pos + 1) == Some(&b'{') {
                    if frames.len() + 2 > MAX_CONTEXTS {
                        return code.len();
                    }
                    frames.push(TemplateFrame::Text);
                    frames.push(TemplateFrame::Expression { braces: 0, expects_value: true });
                    pos += 2;
                } else {
                    frames.push(TemplateFrame::Text);
                    pos = next_char(code, pos);
                    if byte == b'\\' {
                        pos = next_char(code, pos);
                    }
                }
            }
            TemplateFrame::Expression { mut braces, mut expects_value } => {
                if byte == b'}' && braces == 0 {
                    pos += 1;
                    continue;
                }
                if byte == b'`' {
                    if frames.len() + 2 > MAX_CONTEXTS {
                        return code.len();
                    }
                    frames.push(TemplateFrame::Expression { braces, expects_value: false });
                    frames.push(TemplateFrame::Text);
                    pos += 1;
                    continue;
                }
                if byte == b'{' {
                    braces += 1;
                } else if byte == b'}' {
                    braces = braces.saturating_sub(1);
                }
                pos = javascript_atom(code, pos, &mut expects_value);
                frames.push(TemplateFrame::Expression { braces, expects_value });
            }
        }
    }
    code.len()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn tiles(code: &str) -> Vec<Span> {
        let mut spans = Vec::new();
        scan(code, Some(&mut spans));
        let mut cursor = 0;
        for span in &spans {
            assert_eq!(span.start, cursor);
            assert!(span.start < span.end && span.end <= code.len());
            assert!(code.is_char_boundary(span.start) && code.is_char_boundary(span.end));
            cursor = span.end;
        }
        assert_eq!(cursor, code.len());
        spans
    }

    #[test]
    fn markup_text_and_javascript_expressions_have_separate_modes() {
        let code = "const view = <div>return // plain text{items.map(x => <b>{x}</b>)}</div>;";
        let spans = tiles(code);
        assert!(spans.iter().any(|s| s.kind == Tok::Plain && &code[s.start..s.end] == "return // plain text"));
        assert!(spans.iter().any(|s| s.kind == Tok::Type && &code[s.start..s.end] == "</div>"));
        assert!(spans.iter().any(|s| s.kind == Tok::Func && &code[s.start..s.end] == "map"));
    }

    #[test]
    fn expression_attributes_are_delegated_instead_of_being_swallowed_by_tags() {
        let code = "<Button onClick={() => alert('hi')} size={42} child={<Icon />} />";
        let spans = tiles(code);
        assert!(spans.iter().any(|s| s.kind == Tok::Number && &code[s.start..s.end] == "42"));
        assert!(spans.iter().any(|s| s.kind == Tok::Func && &code[s.start..s.end] == "alert"));
        assert!(spans.iter().any(|s| s.kind == Tok::Type && &code[s.start..s.end] == "<Icon />"));
    }

    #[test]
    fn comparisons_strings_comments_regexes_and_templates_do_not_open_markup() {
        let code = "const a = left<right; const b = /<div>/g; const c = `<x>${`inner`}</x>`; /* <b> */ const d = '<i>'; const e = <Real/>;";
        let spans = tiles(code);
        let tags: Vec<_> = spans.iter().filter(|s| s.kind == Tok::Type && code[s.start..s.end].starts_with('<'))
            .map(|s| &code[s.start..s.end]).collect();
        assert_eq!(tags, vec!["<Real/>"]);
    }

    #[test]
    fn javascript_resumes_with_value_context_after_jsx() {
        let code = "const value = <Ratio/> / 2; const node = <Next/>;";
        let spans = tiles(code);
        assert!(spans.iter().any(|s| s.kind == Tok::Operator && &code[s.start..s.end] == "/"));
        assert!(spans.iter().any(|s| s.kind == Tok::Type && &code[s.start..s.end] == "<Next/>"));
    }

    #[test]
    fn every_utf8_prefix_of_tricky_input_tiles_without_panics() {
        for code in [
            "<Résumé title=\"escaped \\😀 >\">héllo{value}</Résumé>",
            "const x = <div title=\"trailing \\",
            "<>text</>",
            "<A value={{ braces: '}', regex: /[}<>]/, template: `x${`y`}` }} />",
            "<A><B>{ok ? <C/> : <D/>}</B></A>",
        ] {
            for end in (0..=code.len()).filter(|end| code.is_char_boundary(*end)) {
                tiles(&code[..end]);
            }
        }
    }

    #[test]
    fn excessive_expression_nesting_falls_back_without_recursion() {
        let code = format!("{}x{}", "<A>{".repeat(300), "}</A>".repeat(300));
        tiles(&code);
        assert_eq!(scan(&code, None), 0);
    }

    #[test]
    fn hold_boundary_tracks_open_context_not_the_last_angle_bracket() {
        assert_eq!(scan("<A><B/>text", None), 0);
        assert_eq!(scan("<A>{value", None), 0);
        let complete = "const x = <A><B/></A>;";
        assert_eq!(scan(complete, None), complete.len());
        let literal = "const x = '<A>'; // <B>";
        assert_eq!(scan(literal, None), literal.len());
        let partial = "const x = <";
        assert_eq!(scan(partial, None), partial.len() - 1);
    }
}
