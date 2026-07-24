//! The TeX-surface lexer.
//!
//! Tokenization follows TeX's fixed catcode assignments as used by the math
//! surface: control words are `\` + ASCII letters (with following whitespace
//! consumed, as TeX does), control symbols are `\` + one non-letter
//! character, `%` opens a comment running to end of line, and the special
//! characters `{ } ^ _ & ~ $` lex to their own token kinds. Everything else
//! is a character token. Whitespace runs collapse to one [`TokKind::Space`]
//! token (math mode ignores it; text mode renders one interword space).
//!
//! The lexer is infallible: every byte sequence lexes to a token stream.
//! Errors (unknown commands, malformed structure) are the parser's to
//! report, precisely — never the lexer's to garble.

use crate::node::Span;

/// One lexed token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tok<'src> {
    pub(crate) kind: TokKind<'src>,
    pub(crate) span: Span,
}

/// What a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokKind<'src> {
    /// `\name` — name is ASCII letters, without the backslash.
    ControlWord(&'src str),
    /// `\c` for one non-letter `c` (`\\`, `\,`, `\{`, `\ `, …).
    ControlSymbol(char),
    /// `{`
    BeginGroup,
    /// `}`
    EndGroup,
    /// `^`
    Sup,
    /// `_`
    Sub,
    /// `&`
    AlignTab,
    /// `~`
    Tie,
    /// `$`
    MathShift,
    /// A run of whitespace.
    Space,
    /// Any other single character.
    Char(char),
}

/// Lex `source` completely. Infallible; comments and their line ends are
/// dropped, whitespace runs collapse to single [`TokKind::Space`] tokens.
pub(crate) fn lex(source: &str) -> Vec<Tok<'_>> {
    let bytes = source.as_bytes();
    let mut toks = Vec::new();
    let mut iter = source.char_indices().peekable();
    while let Some((start, ch)) = iter.next() {
        match ch {
            '%' => {
                // Comment: skip through the end of the line (inclusive).
                for (_, c) in iter.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '\\' => {
                match iter.peek().copied() {
                    Some((_, c)) if c.is_ascii_alphabetic() => {
                        // Control word: consume the letters.
                        let mut end = start + 1;
                        while let Some(&(i, c)) = iter.peek() {
                            if c.is_ascii_alphabetic() {
                                end = i + c.len_utf8();
                                iter.next();
                            } else {
                                break;
                            }
                        }
                        // TeX consumes whitespace after a control word.
                        while let Some(&(_, c)) = iter.peek() {
                            if c.is_whitespace() {
                                iter.next();
                            } else {
                                break;
                            }
                        }
                        let name = name_slice(source, start + 1, end);
                        toks.push(Tok {
                            kind: TokKind::ControlWord(name),
                            span: Span::new(start, end),
                        });
                    }
                    Some((i, c)) => {
                        iter.next();
                        toks.push(Tok {
                            kind: TokKind::ControlSymbol(c),
                            span: Span::new(start, i + c.len_utf8()),
                        });
                    }
                    None => {
                        // A trailing lone backslash: surface it as a
                        // character token; the parser reports it precisely.
                        toks.push(Tok {
                            kind: TokKind::Char('\\'),
                            span: Span::new(start, start + 1),
                        });
                    }
                }
            }
            '{' => toks.push(single(TokKind::BeginGroup, start, 1)),
            '}' => toks.push(single(TokKind::EndGroup, start, 1)),
            '^' => toks.push(single(TokKind::Sup, start, 1)),
            '_' => toks.push(single(TokKind::Sub, start, 1)),
            '&' => toks.push(single(TokKind::AlignTab, start, 1)),
            '~' => toks.push(single(TokKind::Tie, start, 1)),
            '$' => toks.push(single(TokKind::MathShift, start, 1)),
            c if c.is_whitespace() => {
                let mut end = start + c.len_utf8();
                while let Some(&(i, c2)) = iter.peek() {
                    if c2.is_whitespace() {
                        end = i + c2.len_utf8();
                        iter.next();
                    } else {
                        break;
                    }
                }
                toks.push(Tok {
                    kind: TokKind::Space,
                    span: Span::new(start, end),
                });
            }
            c => {
                let _ = bytes;
                toks.push(Tok {
                    kind: TokKind::Char(c),
                    span: Span::new(start, start + c.len_utf8()),
                });
            }
        }
    }
    toks
}

fn single(kind: TokKind<'static>, start: usize, len: usize) -> Tok<'static> {
    Tok {
        kind,
        span: Span::new(start, start + len),
    }
}

/// Slice a control-word name out of the source, tolerating the impossible
/// (out-of-bounds would mean a lexer bug; degrade to an empty name rather
/// than panic — the parser then reports an unknown command, which is at
/// least a precise failure).
fn name_slice(source: &str, start: usize, end: usize) -> &str {
    source.get(start..end).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds<'a>(src: &'a str) -> Vec<TokKind<'a>> {
        lex(src).into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn control_words_consume_trailing_space() {
        assert_eq!(
            kinds(r"\pi x"),
            vec![TokKind::ControlWord("pi"), TokKind::Char('x')]
        );
    }

    #[test]
    fn control_word_span_excludes_trailing_space() {
        let toks = lex(r"\pi x");
        assert_eq!(toks[0].span, Span::new(0, 3));
        assert_eq!(toks[1].span, Span::new(4, 5));
    }

    #[test]
    fn control_symbols() {
        assert_eq!(
            kinds(r"\\ \, \{"),
            vec![
                TokKind::ControlSymbol('\\'),
                TokKind::Space,
                TokKind::ControlSymbol(','),
                TokKind::Space,
                TokKind::ControlSymbol('{'),
            ]
        );
    }

    #[test]
    fn control_space() {
        assert_eq!(
            kinds(r"a\ b"),
            vec![
                TokKind::Char('a'),
                TokKind::ControlSymbol(' '),
                TokKind::Char('b'),
            ]
        );
    }

    #[test]
    fn specials() {
        assert_eq!(
            kinds("{x}^2_i&~$"),
            vec![
                TokKind::BeginGroup,
                TokKind::Char('x'),
                TokKind::EndGroup,
                TokKind::Sup,
                TokKind::Char('2'),
                TokKind::Sub,
                TokKind::Char('i'),
                TokKind::AlignTab,
                TokKind::Tie,
                TokKind::MathShift,
            ]
        );
    }

    #[test]
    fn comments_run_to_eol() {
        assert_eq!(
            kinds("a% comment ^ _ $\nb"),
            vec![TokKind::Char('a'), TokKind::Char('b'),]
        );
    }

    #[test]
    fn escaped_percent_is_a_control_symbol() {
        assert_eq!(
            kinds(r"\%b"),
            vec![TokKind::ControlSymbol('%'), TokKind::Char('b'),]
        );
    }

    #[test]
    fn whitespace_collapses() {
        assert_eq!(
            kinds("a  \t\n b"),
            vec![TokKind::Char('a'), TokKind::Space, TokKind::Char('b'),]
        );
    }

    #[test]
    fn trailing_backslash_degrades_to_char() {
        assert_eq!(kinds("a\\"), vec![TokKind::Char('a'), TokKind::Char('\\')]);
    }

    #[test]
    fn unicode_chars_lex_with_correct_spans() {
        let toks = lex("αβ");
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].span, Span::new(0, 2));
        assert_eq!(toks[1].span, Span::new(2, 4));
    }
}
