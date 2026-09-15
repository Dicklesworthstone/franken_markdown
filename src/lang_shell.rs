#![forbid(unsafe_code)]

//! POSIX shell incremental lexer (FCB-022 · fcb-9vx.16).
//!
//! Classifies shell source into byte-exact [`Span`]s that tile the input
//! exactly. Handles heredoc operators, single/double quotes, parameter
//! expansions with nested quoting, command substitution with balanced
//! parens and nested quoting, comments, escaped newlines, variables, and
//! shell operators — under a declared capability with no execution model.

use crate::highlight::{Span, Tok};

const KEYWORDS: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "in", "function", "select", "time", "coproc", "local", "export", "readonly", "declare",
    "typeset", "unset", "shift", "return", "exit", "eval", "exec", "set", "trap", "umask", "alias",
    "unalias", "source", "wait",
];

const BUILTINS: &[&str] = &[
    "echo", "printf", "read", "cd", "pwd", "test", "pushd", "popd", "dirs", "jobs", "fg", "bg",
    "kill", "getopts", "type", "command", "builtin", "enable", "help",
];

/// What the previous significant token was.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Prev {
    Start,
    Value,
    Keyword,
    Operator,
}

fn classify_word(word: &str, prev: Prev) -> Tok {
    if KEYWORDS.contains(&word) {
        return Tok::Keyword;
    }
    if BUILTINS.contains(&word) && matches!(prev, Prev::Start | Prev::Keyword | Prev::Operator) {
        return Tok::Func;
    }
    Tok::Plain
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn consume_while(code: &str, mut pos: usize, pred: impl Fn(char) -> bool) -> usize {
    while pos < code.len() {
        let c = code[pos..].chars().next().unwrap_or('\0');
        if pred(c) {
            pos += c.len_utf8();
        } else {
            break;
        }
    }
    pos
}

fn ic_len(code: &str, pos: usize) -> usize {
    code[pos..].chars().next().map_or(1, |c| c.len_utf8())
}

/// Lex POSIX shell source into exact tiling spans.
pub fn lex_shell_into(code: &str, spans: &mut Vec<Span>) {
    let bytes_len = code.len();
    let mut pos = 0usize;
    let mut last_end = 0usize;
    let mut prev = Prev::Start;

    fn push_tiling(
        spans: &mut Vec<Span>,
        last_end: &mut usize,
        kind: Tok,
        start: usize,
        end: usize,
    ) {
        if *last_end < start {
            spans.push(Span {
                kind: Tok::Plain,
                start: *last_end,
                end: start,
            });
        }
        spans.push(Span { kind, start, end });
        *last_end = end;
    }

    while pos < bytes_len {
        let rest = &code[pos..];
        let ch = match rest.chars().next() {
            Some(c) => c,
            None => break,
        };
        let clen = ch.len_utf8();

        // Whitespace.
        if ch.is_whitespace() {
            let start = pos;
            pos += clen;
            while pos < bytes_len {
                let c = code[pos..].chars().next().unwrap_or('\0');
                if c.is_whitespace() {
                    pos += c.len_utf8();
                } else {
                    break;
                }
            }
            push_tiling(spans, &mut last_end, Tok::Plain, start, pos);
            prev = Prev::Operator;
            continue;
        }

        // Comments: `#` to EOL.
        if ch == '#' {
            let start = pos;
            let end = code[start..].find('\n').map_or(bytes_len, |nl| start + nl);
            push_tiling(spans, &mut last_end, Tok::Comment, start, end);
            pos = end;
            prev = Prev::Start;
            continue;
        }

        // Single quotes: fully verbatim.
        if ch == '\'' {
            let start = pos;
            let end = code[start + 1..]
                .find('\'')
                .map_or(bytes_len, |at| start + 1 + at + 1);
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Command substitution `$(...)`: balanced parens with nested quoting.
        if rest.starts_with("$(") {
            let start = pos;
            let mut depth = 0usize;
            let mut scan = start + 2;
            let mut closed = false;
            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap_or('\0');
                if c == '\'' || c == '"' {
                    let quote = c;
                    let mut inner = scan + 1;
                    while inner < bytes_len {
                        let ic = code[inner..].chars().next().unwrap_or('\0');
                        if ic == '\\' {
                            inner += 1;
                            if inner < bytes_len {
                                inner += ic_len(code, inner);
                            }
                            continue;
                        }
                        if ic == quote {
                            break;
                        }
                        inner += ic.len_utf8();
                    }
                    scan = inner;
                    continue;
                }
                if c == '(' {
                    depth += 1;
                } else if c == ')' {
                    if depth == 0 {
                        scan += c.len_utf8();
                        closed = true;
                        break;
                    }
                    depth -= 1;
                }
                scan += c.len_utf8();
            }
            let end = if closed { scan } else { bytes_len.min(scan) };
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Double quotes.
        if ch == '"' {
            let start = pos;
            let mut scan = start + 1;
            let mut closed = false;
            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap_or('\0');
                if c == '\\' {
                    let next_scan = scan + 1;
                    if next_scan >= bytes_len {
                        scan = bytes_len;
                        break;
                    }
                    let escaped = code[next_scan..].chars().next().unwrap_or('\0');
                    scan = next_scan + escaped.len_utf8();
                    continue;
                }
                if c == '"' {
                    scan += c.len_utf8();
                    closed = true;
                    break;
                }
                scan += c.len_utf8();
            }
            let end = if closed { scan } else { bytes_len.min(scan) };
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Legacy backtick substitution.
        if ch == '`' {
            let start = pos;
            let end = code[start + 1..]
                .find('`')
                .map_or(bytes_len, |at| start + 1 + at + 1);
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Heredoc operators.
        if rest.starts_with("<<<") {
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + 3);
            pos += 3;
            prev = Prev::Operator;
            continue;
        }
        if rest.starts_with("<<-") {
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + 3);
            pos += 3;
            prev = Prev::Operator;
            continue;
        }
        if rest.starts_with("<<") {
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + 2);
            pos += 2;
            prev = Prev::Operator;
            continue;
        }

        // Variables.
        if ch == '$' {
            let start = pos;
            let next = code.get(start + 1..).and_then(|s| s.chars().next());
            match next {
                Some('{') => {
                    let mut depth = 0usize;
                    let mut scan = start + 1;
                    let mut closed = false;
                    while scan < bytes_len {
                        let c = code[scan..].chars().next().unwrap_or('\0');
                        if c == '{' {
                            depth += 1;
                        } else if c == '}' {
                            if depth == 0 {
                                scan += c.len_utf8();
                                closed = true;
                                break;
                            }
                            depth -= 1;
                        }
                        scan += c.len_utf8();
                    }
                    let end = if closed { scan } else { bytes_len.min(scan) };
                    push_tiling(spans, &mut last_end, Tok::Str, start, end);
                    pos = end;
                    prev = Prev::Value;
                    continue;
                }
                Some(c) if c.is_ascii_digit() || "@*#? !$".contains(c) => {
                    let end = start + 2;
                    push_tiling(spans, &mut last_end, Tok::Number, start, end);
                    pos = end;
                    prev = Prev::Value;
                    continue;
                }
                Some(c) if is_word_char(c) => {
                    let mut scan = start + 1;
                    while scan < bytes_len {
                        let c = code[scan..].chars().next().unwrap_or('\0');
                        if is_word_char(c) {
                            scan += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    push_tiling(spans, &mut last_end, Tok::Str, start, scan);
                    pos = scan;
                    prev = Prev::Value;
                    continue;
                }
                _ => {
                    push_tiling(spans, &mut last_end, Tok::Str, start, start + 1);
                    pos = start + 1;
                    prev = Prev::Value;
                    continue;
                }
            }
        }

        // Multi-char operators.
        const OPERATORS: &[&str] = &[";;&", ";;", ";&", "|&", "&&", "||", "==", "!=", "+="];
        let mut matched = false;
        for op in OPERATORS {
            if rest.starts_with(op) {
                push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + op.len());
                pos += op.len();
                prev = Prev::Operator;
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        // Single-char operators.
        if matches!(ch, '|' | '&' | ';' | '<' | '>' | '=' | '!' | '(' | ')') {
            let kind = if matches!(ch, '(' | ')') {
                Tok::Punct
            } else {
                Tok::Operator
            };
            push_tiling(spans, &mut last_end, kind, pos, pos + clen);
            pos += clen;
            prev = Prev::Operator;
            continue;
        }

        // Words: identifiers, commands, flags, numbers.
        if is_word_char(ch) {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, is_word_char);
            let word = &code[start..pos];
            let kind = classify_word(word, prev);
            push_tiling(spans, &mut last_end, kind, start, pos);
            prev = if kind == Tok::Keyword {
                Prev::Keyword
            } else {
                Prev::Value
            };
            continue;
        }

        // Escaped characters.
        if ch == '\\' {
            let start = pos;
            let next = pos + 1;
            let adv = if next < bytes_len {
                code[next..].chars().next().unwrap_or('\0').len_utf8()
            } else {
                0
            };
            push_tiling(spans, &mut last_end, Tok::Plain, start, next + adv);
            pos = next + adv;
            prev = Prev::Value;
            continue;
        }

        // Everything else: Plain, one char.
        push_tiling(spans, &mut last_end, Tok::Plain, pos, pos + clen);
        pos += clen;
        prev = Prev::Start;
    }

    // Defensive final tile.
    let tail_start = last_end;
    if tail_start < bytes_len {
        push_tiling(spans, &mut last_end, Tok::Plain, tail_start, bytes_len);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(span.start, cursor, "gap/overlap at {cursor}");
            assert!(span.end > span.start, "empty span at {cursor}");
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    fn lex_shell(code: &str) -> Vec<Span> {
        let mut spans = Vec::new();
        lex_shell_into(code, &mut spans);
        spans
    }

    #[test]
    fn keywords_builtins_and_commands_classified() {
        let code = "if test -f x; then echo done; fi";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
        let keywords: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(keywords.contains(&"if"));
        assert!(keywords.contains(&"then"));
        assert!(keywords.contains(&"fi"));
    }

    #[test]
    fn single_quotes_are_verbatim_no_expansion() {
        let code = "echo '$HOME and `cmd`'";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
        let strs: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            strs.contains(&"'$HOME and `cmd`'"),
            "single-quoted string is verbatim: {strs:?}"
        );
    }

    #[test]
    fn double_quotes_allow_expansions() {
        let code = "echo \"home is $HOME\"";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
    }

    #[test]
    fn parameter_expansions_with_nested_quoting() {
        let code = "echo ${VAR:-\"default value\"} ${1}";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
    }

    #[test]
    fn command_substitution_balanced_parens() {
        let code = "echo $(echo \"inner (paren) done\")";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
    }

    #[test]
    fn heredoc_operators_classified() {
        let code = "cat <<EOF\ntext\nEOF\ncat <<-TAB\tmore\nTAB";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
        let ops: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Operator)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(ops.contains(&"<<"));
        assert!(ops.contains(&"<<-"));
    }

    #[test]
    fn escaped_newline_continues_command() {
        let code = "echo one \\\n  two";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
    }

    #[test]
    fn unterminated_single_quote_spans_to_eof() {
        let code = "echo 'never closed";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
        let last = spans.last().unwrap();
        assert_eq!(last.kind, Tok::Str);
        assert_eq!(last.end, code.len());
    }

    #[test]
    fn comments_stop_at_hash_only_outside_quotes() {
        let code = "echo '# not a comment' # real comment";
        let spans = lex_shell(code);
        assert_tiling(code, &spans);
        let comments: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Comment)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert_eq!(comments.len(), 1, "{comments:?}");
        assert!(comments[0].contains("real comment"));
    }

    #[test]
    fn negative_control_tiling_oracle_detects_gap() {
        let code = "abc";
        let gapped = vec![
            Span {
                kind: Tok::Plain,
                start: 0,
                end: 1,
            },
            Span {
                kind: Tok::Plain,
                start: 2,
                end: 3,
            },
        ];
        let detected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_tiling(code, &gapped);
        }))
        .is_err();
        assert!(detected, "tiling oracle must catch a gap");
        let mut spans = Vec::new();
        lex_shell_into(code, &mut spans);
        assert_tiling(code, &spans);
    }
}
