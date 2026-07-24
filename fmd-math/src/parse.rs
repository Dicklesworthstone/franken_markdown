//! The parser: token stream → [`Node`] tree, spans on every node.
//!
//! Two entries, one grammar. [`parse`](crate::parse) reads a whole string as
//! mathematics (the `Tex` surface: `&` and `\\` are legal at the top level
//! because the Reference wraps whole strings in an `align*`-class
//! environment). [`parse_text`](crate::parse_text) reads the TexText
//! contract: a text mainland with `$…$` math islands, `\textbf`/`\emph`/
//! `\underline` styling, and the escape set.
//!
//! Error doctrine: every failure is a precise, named [`MathError`] — an
//! unknown or tier-2 construct reports its construct-table name, a
//! structural fault reports what and where — and arbitrary input can never
//! hang, recurse unboundedly, or produce garbage (the depth guard bounds
//! nesting; the chaos suite locks the never-panic property).

use crate::atom::{char_class, intrinsic_class};
use crate::commands::{self, Cmd};
use crate::error::MathError;
use crate::node::{
    AccentKind, Delim, FracSpec, FragmentKind, Limits, Node, NodeKind, SpaceKind, Span, TextStyle,
};
use crate::token::{Tok, TokKind, lex};

/// Nesting depth bound: groups, arguments, `\left…\right`, environments,
/// and islands all descend one level. Real formulas in the G0-4 corpus stay
/// in single digits; the bound exists so hostile input errors cleanly
/// instead of exhausting the stack (64 keeps an ample margin against the
/// 2 MiB test-thread stack even in unoptimized builds, where the parser's
/// frames are largest).
pub(crate) const MAX_DEPTH: usize = 64;

/// The shared empty macro set (the plain entry points still run the
/// expansion pass, so inline `\newcommand` definitions work everywhere).
fn empty_macros() -> &'static crate::macros::MacroSet {
    static EMPTY: std::sync::OnceLock<crate::macros::MacroSet> = std::sync::OnceLock::new();
    EMPTY.get_or_init(crate::macros::MacroSet::new)
}

/// Parse a whole source string as mathematics.
pub(crate) fn parse_math(source: &str) -> Result<Node, MathError> {
    parse_math_with(source, empty_macros())
}

/// Parse mathematics against a macro set (a preamble pack and/or caller
/// definitions), expanding at the token level before the grammar runs.
pub(crate) fn parse_math_with<'s>(
    source: &'s str,
    macros: &'s crate::macros::MacroSet,
) -> Result<Node, MathError> {
    let toks = crate::macros::expand(lex(source), macros, source.len())?;
    let mut parser = Parser::with_tokens(source, toks);
    let (items, reason) = parser.math_list(Stops {
        top: true,
        ..Stops::default()
    })?;
    match reason {
        Reason::EndOfInput => Ok(Node::new(NodeKind::List(items), Span::new(0, source.len()))),
        Reason::EndGroup(span) => Err(MathError::Malformed {
            what: "unmatched '}'".to_owned(),
            at: span.start,
        }),
        Reason::MathShift(span) => Err(MathError::Malformed {
            what: "'$' may not appear inside mathematics (write \\$ for a dollar sign)".to_owned(),
            at: span.start,
        }),
        Reason::Right(span) => Err(MathError::Malformed {
            what: "\\right without a matching \\left".to_owned(),
            at: span.start,
        }),
        Reason::CellTab(span) | Reason::CellBreak(span) | Reason::BracketClose(span) => {
            Err(MathError::Malformed {
                what: "internal: unexpected list stop".to_owned(),
                at: span.start,
            })
        }
        Reason::EnvEnd { span, .. } => Err(MathError::Malformed {
            what: "\\end without a matching \\begin".to_owned(),
            at: span.start,
        }),
    }
}

/// Parse a whole source string under the TexText contract.
pub(crate) fn parse_text_mode(source: &str) -> Result<Node, MathError> {
    parse_text_mode_with(source, empty_macros())
}

/// Parse a TexText-contract string against a macro set.
pub(crate) fn parse_text_mode_with<'s>(
    source: &'s str,
    macros: &'s crate::macros::MacroSet,
) -> Result<Node, MathError> {
    let toks = crate::macros::expand(lex(source), macros, source.len())?;
    let mut parser = Parser::with_tokens(source, toks);
    let (items, reason) = parser.text_list()?;
    match reason {
        Reason::EndOfInput => Ok(Node::new(NodeKind::List(items), Span::new(0, source.len()))),
        Reason::EndGroup(span) => Err(MathError::Malformed {
            what: "unmatched '}'".to_owned(),
            at: span.start,
        }),
        other => Err(MathError::Malformed {
            what: "internal: unexpected text-list stop".to_owned(),
            at: other.span().start,
        }),
    }
}

/// Which tokens end the current list.
#[derive(Clone, Copy, Default)]
struct Stops {
    /// This is the outermost list of a source string: tolerate structural
    /// fragments (per-argument `SingleStringTex` semantics — an unmatched
    /// `}` or a stray `\right` marks a piece of a balanced whole).
    top: bool,
    /// `\right` closes the list (`\left` bodies).
    right: bool,
    /// `&`, `\\`, and `\end` close the list (environment cells).
    env: bool,
    /// `]` closes the list (`\sqrt[…]` indices).
    bracket: bool,
}

/// Why a list ended.
#[derive(Clone, Debug)]
enum Reason {
    EndOfInput,
    EndGroup(Span),
    Right(Span),
    MathShift(Span),
    CellTab(Span),
    CellBreak(Span),
    BracketClose(Span),
    EnvEnd { name: String, span: Span },
}

impl Reason {
    fn span(&self) -> Span {
        match self {
            Self::EndOfInput => Span::new(usize::MAX, usize::MAX),
            Self::EndGroup(s)
            | Self::Right(s)
            | Self::MathShift(s)
            | Self::CellTab(s)
            | Self::CellBreak(s)
            | Self::BracketClose(s) => *s,
            Self::EnvEnd { span, .. } => *span,
        }
    }
}

/// The span covering a list, with a zero-width fallback position for empty
/// lists.
fn list_span(items: &[Node], fallback: usize) -> Span {
    match (items.first(), items.last()) {
        (Some(first), Some(last)) => first.span.union(last.span),
        _ => Span::new(fallback, fallback),
    }
}

/// Map a direct math-mode character to its math codepoint (the G0-3
/// ratification's char→math-codepoint table: hyphen is a minus sign,
/// asterisk an operator).
const fn map_math_char(ch: char) -> char {
    match ch {
        '-' => '−',
        '*' => '∗',
        _ => ch,
    }
}

struct Parser<'s> {
    src: &'s str,
    toks: Vec<Tok<'s>>,
    pos: usize,
    depth: usize,
}

impl<'s> Parser<'s> {
    /// A parser over an already-lexed (and macro-expanded) token stream.
    fn with_tokens(src: &'s str, toks: Vec<Tok<'s>>) -> Self {
        Self {
            src,
            toks,
            pos: 0,
            depth: 0,
        }
    }

    fn peek(&self) -> Option<&Tok<'s>> {
        self.toks.get(self.pos)
    }

    fn next_tok(&mut self) -> Option<Tok<'s>> {
        let tok = self.toks.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek().map(|t| t.kind), Some(TokKind::Space)) {
            self.pos += 1;
        }
    }

    /// Byte offset used for "at end of input" errors.
    fn here(&self) -> usize {
        self.peek().map_or(self.src.len(), |t| t.span.start)
    }

    fn descend<T>(
        &mut self,
        at: usize,
        f: impl FnOnce(&mut Self) -> Result<T, MathError>,
    ) -> Result<T, MathError> {
        if self.depth >= MAX_DEPTH {
            return Err(MathError::Malformed {
                what: format!("nesting exceeds the depth limit ({MAX_DEPTH})"),
                at,
            });
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    // ── Math mode ───────────────────────────────────────────────────────

    /// Parse a math-mode horizontal list until a stop token. Handles script
    /// clusters, `\limits` designators, and the infix `\over`-class split.
    #[allow(clippy::too_many_lines)]
    fn math_list(&mut self, stops: Stops) -> Result<(Vec<Node>, Reason), MathError> {
        let mut items: Vec<Node> = Vec::new();
        let mut over: Option<(usize, FracSpec, Span)> = None;
        let reason = loop {
            let Some(tok) = self.peek().cloned() else {
                break Reason::EndOfInput;
            };
            match tok.kind {
                TokKind::Space => {
                    self.pos += 1;
                }
                TokKind::EndGroup => {
                    self.pos += 1;
                    if stops.top {
                        items.push(Node::new(
                            NodeKind::Fragment(FragmentKind::UnmatchedClose),
                            tok.span,
                        ));
                        continue;
                    }
                    break Reason::EndGroup(tok.span);
                }
                TokKind::MathShift => {
                    self.pos += 1;
                    if stops.top {
                        items.push(Node::new(
                            NodeKind::Fragment(FragmentKind::RedundantMathShift),
                            tok.span,
                        ));
                        continue;
                    }
                    break Reason::MathShift(tok.span);
                }
                TokKind::AlignTab => {
                    self.pos += 1;
                    if stops.env {
                        break Reason::CellTab(tok.span);
                    }
                    items.push(Node::new(NodeKind::AlignTab, tok.span));
                }
                TokKind::Tie => {
                    self.pos += 1;
                    items.push(Node::new(NodeKind::Tie, tok.span));
                }
                TokKind::Sup | TokKind::Sub => {
                    self.script_cluster(&mut items)?;
                }
                TokKind::Char('\'') => {
                    self.script_cluster(&mut items)?;
                }
                TokKind::Char(']') if stops.bracket => {
                    self.pos += 1;
                    break Reason::BracketClose(tok.span);
                }
                TokKind::Char(c) => {
                    self.pos += 1;
                    let mapped = map_math_char(c);
                    items.push(Node::new(
                        NodeKind::Symbol {
                            ch: mapped,
                            class: char_class(mapped),
                        },
                        tok.span,
                    ));
                }
                TokKind::BeginGroup => {
                    self.pos += 1;
                    let node = self.math_group_body(tok.span)?;
                    items.push(node);
                }
                TokKind::ControlSymbol('\\') => {
                    self.pos += 1;
                    if stops.env {
                        break Reason::CellBreak(tok.span);
                    }
                    items.push(Node::new(NodeKind::Linebreak, tok.span));
                }
                TokKind::ControlSymbol(c) => {
                    self.pos += 1;
                    match self.math_control_symbol(c, tok.span)? {
                        Some(node) => items.push(node),
                        None => break Reason::EndOfInput,
                    }
                }
                TokKind::ControlWord(name) => {
                    match self.math_control_word(name, tok.span, stops, &mut items, &mut over)? {
                        ControlFlow::Continue => {}
                        ControlFlow::Stop(reason) => break reason,
                    }
                }
            }
        };
        let items = resolve_over(items, over);
        Ok((items, reason))
    }

    /// A `{…}` group body in math mode. End of input closes the group
    /// (fragment semantics: the `}` lives in a later piece); the wrong
    /// closer stays a precise error.
    fn math_group_body(&mut self, open: Span) -> Result<Node, MathError> {
        let (items, reason) = self.descend(open.start, |p| p.math_list(Stops::default()))?;
        match reason {
            Reason::EndGroup(close) => Ok(Node::new(NodeKind::List(items), open.union(close))),
            Reason::EndOfInput => Ok(Node::new(
                NodeKind::List(items),
                Span::new(open.start, self.src.len()),
            )),
            other => Err(unexpected_close(&other, '{', open)),
        }
    }

    /// A control symbol in math mode. `Ok(None)` never occurs today; the
    /// option leaves room for stop-like symbols without reshaping callers.
    fn math_control_symbol(&mut self, c: char, span: Span) -> Result<Option<Node>, MathError> {
        let node = match c {
            ',' => Node::new(NodeKind::Space(SpaceKind::Thin), span),
            ':' => Node::new(NodeKind::Space(SpaceKind::Med), span),
            ';' => Node::new(NodeKind::Space(SpaceKind::Thick), span),
            '!' => Node::new(NodeKind::Space(SpaceKind::NegThin), span),
            ' ' => Node::new(NodeKind::Space(SpaceKind::ControlSpace), span),
            '{' => Node::new(
                NodeKind::Symbol {
                    ch: '{',
                    class: crate::atom::AtomClass::Open,
                },
                span,
            ),
            '}' => Node::new(
                NodeKind::Symbol {
                    ch: '}',
                    class: crate::atom::AtomClass::Close,
                },
                span,
            ),
            '|' => Node::new(
                NodeKind::Symbol {
                    ch: '‖',
                    class: crate::atom::AtomClass::Ord,
                },
                span,
            ),
            '%' | '$' | '&' | '#' | '_' => Node::new(
                NodeKind::Symbol {
                    ch: c,
                    class: crate::atom::AtomClass::Ord,
                },
                span,
            ),
            other => {
                return Err(MathError::UnsupportedCommand {
                    name: format!("\\{other}"),
                    span,
                });
            }
        };
        Ok(Some(node))
    }

    /// A control word in math mode.
    #[allow(clippy::too_many_lines)]
    fn math_control_word(
        &mut self,
        name: &'s str,
        span: Span,
        stops: Stops,
        items: &mut Vec<Node>,
        over: &mut Option<(usize, FracSpec, Span)>,
    ) -> Result<ControlFlow, MathError> {
        let Some(cmd) = commands::lookup(name) else {
            return Err(MathError::UnsupportedCommand {
                name: format!("\\{name}"),
                span,
            });
        };
        self.pos += 1;
        match cmd {
            Cmd::UnsupportedT2 => {
                return Err(MathError::UnsupportedCommand {
                    name: format!("\\{name}"),
                    span,
                });
            }
            Cmd::OverInfix(spec) => {
                if over.is_some() {
                    return Err(MathError::Malformed {
                        what: format!(
                            "ambiguous mathematics: two \\over-class commands in one group \
                             (second is \\{name})"
                        ),
                        at: span.start,
                    });
                }
                *over = Some((items.len(), spec, span));
            }
            Cmd::Right => {
                if stops.right {
                    return Ok(ControlFlow::Stop(Reason::Right(span)));
                }
                if stops.top {
                    let delim = self.delimiter("\\right")?;
                    items.push(Node::new(
                        NodeKind::Fragment(FragmentKind::StrayRight(delim)),
                        span.union(delim.span),
                    ));
                    return Ok(ControlFlow::Continue);
                }
                return Err(MathError::Malformed {
                    what: "\\right without a matching \\left".to_owned(),
                    at: span.start,
                });
            }
            Cmd::End => {
                let (env_name, _) = self.raw_group("environment name after \\end")?;
                if stops.env {
                    return Ok(ControlFlow::Stop(Reason::EnvEnd {
                        name: env_name,
                        span,
                    }));
                }
                return Err(MathError::Malformed {
                    what: format!("\\end{{{env_name}}} without a matching \\begin"),
                    at: span.start,
                });
            }
            Cmd::Limits | Cmd::NoLimits => {
                let mode = if matches!(cmd, Cmd::Limits) {
                    Limits::Limits
                } else {
                    Limits::NoLimits
                };
                if !set_limits(items.last_mut(), mode) {
                    return Err(MathError::Malformed {
                        what: format!("\\{name} is only meaningful after a big operator"),
                        at: span.start,
                    });
                }
            }
            Cmd::StyleSwitch(style) => {
                items.push(Node::new(NodeKind::StyleChange(style), span));
            }
            Cmd::Spacing(kind) => items.push(Node::new(NodeKind::Space(kind), span)),
            other => {
                let node = self.command_node(name, span, other)?;
                items.push(node);
            }
        }
        Ok(ControlFlow::Continue)
    }

    /// Build the node for a self-contained (non-structural) command. Shared
    /// between list position and argument position.
    #[allow(clippy::too_many_lines)]
    fn command_node(&mut self, name: &str, span: Span, cmd: Cmd) -> Result<Node, MathError> {
        match cmd {
            Cmd::Sym { ch, class, .. } => Ok(Node::new(NodeKind::Symbol { ch, class }, span)),
            Cmd::BigOp { ch, integral } => Ok(Node::new(
                NodeKind::BigOp {
                    ch,
                    limits: Limits::Default,
                    integral,
                },
                span,
            )),
            Cmd::OpName { rendered, limits } => Ok(Node::new(
                NodeKind::OpName {
                    name: rendered.to_owned(),
                    limits,
                },
                span,
            )),
            Cmd::OperatorName => {
                let (raw, raw_span) = self.raw_group("argument of \\operatorname")?;
                Ok(Node::new(
                    NodeKind::OpName {
                        name: raw.trim().to_owned(),
                        limits: false,
                    },
                    span.union(raw_span),
                ))
            }
            Cmd::Accent(kind) => {
                let base = self.argument(&format!("argument of \\{name}"))?;
                let span = span.union(base.span);
                Ok(Node::new(
                    NodeKind::Accent {
                        accent: kind,
                        base: Box::new(base),
                    },
                    span,
                ))
            }
            Cmd::Frac(spec) => {
                let num = self.argument(&format!("numerator of \\{name}"))?;
                let den = self.argument(&format!("denominator of \\{name}"))?;
                let span = span.union(den.span);
                Ok(Node::new(
                    NodeKind::Frac {
                        num: Box::new(num),
                        den: Box::new(den),
                        spec,
                    },
                    span,
                ))
            }
            Cmd::Radical => {
                self.skip_spaces();
                let index = if matches!(self.peek().map(|t| t.kind), Some(TokKind::Char('['))) {
                    let open = self.here();
                    self.pos += 1;
                    let (ix_items, reason) = self.descend(open, |p| {
                        p.math_list(Stops {
                            bracket: true,
                            ..Stops::default()
                        })
                    })?;
                    match reason {
                        Reason::BracketClose(close) => Some(Box::new(Node::new(
                            NodeKind::List(ix_items),
                            Span::new(open, close.end),
                        ))),
                        Reason::EndOfInput => {
                            return Err(MathError::Malformed {
                                what: format!("unclosed '[' in \\sqrt index opened at byte {open}"),
                                at: self.src.len(),
                            });
                        }
                        other => {
                            return Err(MathError::Malformed {
                                what: format!(
                                    "the \\sqrt index opened at byte {open} is not closed by ']'"
                                ),
                                at: other.span().start,
                            });
                        }
                    }
                } else {
                    None
                };
                let radicand = self.argument("argument of \\sqrt")?;
                let span = span.union(radicand.span);
                Ok(Node::new(
                    NodeKind::Radical {
                        index,
                        radicand: Box::new(radicand),
                    },
                    span,
                ))
            }
            Cmd::Text => {
                let (body, body_span) = self.text_island_argument(name)?;
                Ok(Node::new(NodeKind::Text { body }, span.union(body_span)))
            }
            Cmd::TextStyled(style) => {
                let (body, body_span) = self.text_island_argument(name)?;
                Ok(Node::new(
                    NodeKind::TextStyled { style, body },
                    span.union(body_span),
                ))
            }
            Cmd::Alphabet(font) => {
                let body = self.argument(&format!("argument of \\{name}"))?;
                let span = span.union(body.span);
                Ok(Node::new(
                    NodeKind::MathFont {
                        font,
                        body: Box::new(body),
                    },
                    span,
                ))
            }
            Cmd::Phantom(kind) => {
                let body = self.argument(&format!("argument of \\{name}"))?;
                let span = span.union(body.span);
                Ok(Node::new(
                    NodeKind::Phantom {
                        kind,
                        body: Box::new(body),
                    },
                    span,
                ))
            }
            Cmd::Stack(kind) => {
                let annotation = self.argument(&format!("first argument of \\{name}"))?;
                let base = self.argument(&format!("second argument of \\{name}"))?;
                let span = span.union(base.span);
                Ok(Node::new(
                    NodeKind::Stack {
                        kind,
                        annotation: Box::new(annotation),
                        base: Box::new(base),
                    },
                    span,
                ))
            }
            Cmd::Color => {
                let (raw, raw_span) = self.raw_group("argument of \\color")?;
                Ok(Node::new(
                    NodeKind::ColorChange(raw.trim().to_owned()),
                    span.union(raw_span),
                ))
            }
            Cmd::SizedDelim { size, class } => {
                let delim = self.delimiter(&format!("\\{name}"))?;
                let span = span.union(delim.span);
                Ok(Node::new(
                    NodeKind::SizedDelim {
                        size,
                        class: class.unwrap_or(crate::atom::AtomClass::Ord),
                        delim,
                    },
                    span,
                ))
            }
            Cmd::Left => {
                let left = self.delimiter("\\left")?;
                let (body, reason) = self.descend(span.start, |p| {
                    p.math_list(Stops {
                        right: true,
                        ..Stops::default()
                    })
                })?;
                let right = match reason {
                    Reason::Right(_) => self.delimiter("\\right")?,
                    // A `\left` whose `\right` lives in a later piece:
                    // fragment-close with the null delimiter.
                    Reason::EndOfInput => Delim {
                        ch: None,
                        span: Span::new(self.src.len(), self.src.len()),
                    },
                    other => {
                        return Err(MathError::Malformed {
                            what: format!(
                                "\\left at byte {} is closed by something other than \\right",
                                span.start
                            ),
                            at: other.span().start,
                        });
                    }
                };
                let span = span.union(right.span);
                Ok(Node::new(NodeKind::LeftRight { left, right, body }, span))
            }
            Cmd::Begin => self.environment(span),
            Cmd::UnsupportedT2 => Err(MathError::UnsupportedCommand {
                name: format!("\\{name}"),
                span,
            }),
            Cmd::OverInfix(_)
            | Cmd::Right
            | Cmd::End
            | Cmd::Limits
            | Cmd::NoLimits
            | Cmd::StyleSwitch(_)
            | Cmd::Spacing(_) => Err(MathError::Malformed {
                what: format!("\\{name} cannot be used in argument position"),
                at: span.start,
            }),
        }
    }

    /// One undelimited argument: a group, or a single token (with a
    /// command's own arguments, more lenient than TeX's single-token rule
    /// and harmless for corpus input, which is brace-disciplined).
    fn argument(&mut self, what: &str) -> Result<Node, MathError> {
        self.skip_spaces();
        let Some(tok) = self.peek().cloned() else {
            // End of input: the argument lives in a later piece of the
            // balanced whole (fragment semantics) — an empty list stands in.
            let end = self.src.len();
            let _ = what;
            return Ok(Node::new(NodeKind::List(Vec::new()), Span::new(end, end)));
        };
        match tok.kind {
            TokKind::BeginGroup => {
                self.pos += 1;
                self.math_group_body(tok.span)
            }
            TokKind::Char(c) => {
                self.pos += 1;
                let mapped = if c == '\'' { '′' } else { map_math_char(c) };
                Ok(Node::new(
                    NodeKind::Symbol {
                        ch: mapped,
                        class: char_class(mapped),
                    },
                    tok.span,
                ))
            }
            TokKind::ControlWord(name) => {
                let Some(cmd) = commands::lookup(name) else {
                    return Err(MathError::UnsupportedCommand {
                        name: format!("\\{name}"),
                        span: tok.span,
                    });
                };
                self.pos += 1;
                self.descend(tok.span.start, |p| p.command_node(name, tok.span, cmd))
            }
            TokKind::ControlSymbol(c) => {
                self.pos += 1;
                match self.math_control_symbol(c, tok.span)? {
                    Some(node) => Ok(node),
                    None => Err(MathError::Malformed {
                        what: format!("missing {what}"),
                        at: tok.span.start,
                    }),
                }
            }
            _ => Err(MathError::Malformed {
                what: format!("missing {what} (found an unusable token)"),
                at: tok.span.start,
            }),
        }
    }

    /// The braced argument of `\text`-class commands: text mode. A single
    /// character is tolerated (`\text x`).
    fn text_island_argument(&mut self, name: &str) -> Result<(Vec<Node>, Span), MathError> {
        self.skip_spaces();
        let Some(tok) = self.peek().cloned() else {
            return Err(MathError::Malformed {
                what: format!("missing argument of \\{name} at end of input"),
                at: self.src.len(),
            });
        };
        match tok.kind {
            TokKind::BeginGroup => {
                self.pos += 1;
                let (items, reason) = self.descend(tok.span.start, |p| p.text_list())?;
                match reason {
                    Reason::EndGroup(close) => Ok((items, tok.span.union(close))),
                    Reason::EndOfInput => Err(MathError::Malformed {
                        what: format!("unclosed '{{' opened at byte {}", tok.span.start),
                        at: self.src.len(),
                    }),
                    other => Err(unexpected_close(&other, '{', tok.span)),
                }
            }
            TokKind::Char(c) => {
                self.pos += 1;
                Ok((
                    vec![Node::new(
                        NodeKind::TextRun {
                            text: c.to_string(),
                            char_spans: vec![tok.span],
                        },
                        tok.span,
                    )],
                    tok.span,
                ))
            }
            _ => Err(MathError::Malformed {
                what: format!("missing argument of \\{name}"),
                at: tok.span.start,
            }),
        }
    }

    /// A delimiter token after `\left`/`\right`/`\big`-class commands.
    fn delimiter(&mut self, after: &str) -> Result<Delim, MathError> {
        self.skip_spaces();
        let Some(tok) = self.peek().cloned() else {
            return Err(MathError::Malformed {
                what: format!("missing delimiter after {after} at end of input"),
                at: self.src.len(),
            });
        };
        match tok.kind {
            TokKind::Char('.') => {
                self.pos += 1;
                Ok(Delim {
                    ch: None,
                    span: tok.span,
                })
            }
            TokKind::Char(c) if commands::char_is_delim(c) => {
                self.pos += 1;
                let mapped = match c {
                    '<' => '⟨',
                    '>' => '⟩',
                    other => other,
                };
                Ok(Delim {
                    ch: Some(mapped),
                    span: tok.span,
                })
            }
            TokKind::ControlSymbol('|') => {
                self.pos += 1;
                Ok(Delim {
                    ch: Some('‖'),
                    span: tok.span,
                })
            }
            TokKind::ControlSymbol(c @ ('{' | '}')) => {
                self.pos += 1;
                Ok(Delim {
                    ch: Some(c),
                    span: tok.span,
                })
            }
            TokKind::ControlWord(name) => match commands::lookup(name) {
                Some(Cmd::Sym {
                    ch, delim: true, ..
                }) => {
                    self.pos += 1;
                    Ok(Delim {
                        ch: Some(ch),
                        span: tok.span,
                    })
                }
                Some(_) => Err(MathError::Malformed {
                    what: format!("\\{name} is not a delimiter (after {after})"),
                    at: tok.span.start,
                }),
                None => Err(MathError::UnsupportedCommand {
                    name: format!("\\{name}"),
                    span: tok.span,
                }),
            },
            _ => Err(MathError::Malformed {
                what: format!("expected a delimiter after {after}"),
                at: tok.span.start,
            }),
        }
    }

    /// A raw `{…}` group returned as source text (environment names, array
    /// column specs, `\color` arguments).
    fn raw_group(&mut self, what: &str) -> Result<(String, Span), MathError> {
        self.skip_spaces();
        let Some(open) = self.peek().cloned() else {
            return Err(MathError::Malformed {
                what: format!("missing {what} at end of input"),
                at: self.src.len(),
            });
        };
        if !matches!(open.kind, TokKind::BeginGroup) {
            return Err(MathError::Malformed {
                what: format!("missing {what} (expected '{{')"),
                at: open.span.start,
            });
        }
        self.pos += 1;
        let mut depth = 1_usize;
        let content_start = open.span.end;
        loop {
            let Some(tok) = self.next_tok() else {
                return Err(MathError::Malformed {
                    what: format!(
                        "unclosed '{{' in {what}, opened at byte {}",
                        open.span.start
                    ),
                    at: self.src.len(),
                });
            };
            match tok.kind {
                TokKind::BeginGroup => depth += 1,
                TokKind::EndGroup => {
                    depth -= 1;
                    if depth == 0 {
                        let raw = self
                            .src
                            .get(content_start..tok.span.start)
                            .unwrap_or("")
                            .trim()
                            .to_owned();
                        return Ok((raw, open.span.union(tok.span)));
                    }
                }
                _ => {}
            }
        }
    }

    /// `\begin{name}…\end{name}`.
    fn environment(&mut self, begin_span: Span) -> Result<Node, MathError> {
        let (name, name_span) = self.raw_group("environment name after \\begin")?;
        if commands::env_is_t2(&name) {
            return Err(MathError::UnsupportedCommand {
                name: format!("env:{name}"),
                span: begin_span.union(name_span),
            });
        }
        let Some(def) = commands::lookup_env(&name) else {
            return Err(MathError::UnsupportedCommand {
                name: format!("env:{name}"),
                span: begin_span.union(name_span),
            });
        };
        let spec = if def.has_spec {
            let (raw, _) = self.raw_group(&format!("column spec of \\begin{{{name}}}"))?;
            Some(raw)
        } else {
            None
        };
        let mut rows: Vec<Vec<Node>> = Vec::new();
        let mut row: Vec<Node> = Vec::new();
        let end_span = loop {
            let cell_fallback = self.here();
            let (cell_items, reason) = self.descend(begin_span.start, |p| {
                p.math_list(Stops {
                    env: true,
                    ..Stops::default()
                })
            })?;
            let cell_span = list_span(&cell_items, cell_fallback);
            row.push(Node::new(NodeKind::List(cell_items), cell_span));
            match reason {
                Reason::CellTab(_) => {}
                Reason::CellBreak(_) => {
                    rows.push(std::mem::take(&mut row));
                }
                Reason::EnvEnd {
                    name: end_name,
                    span,
                } => {
                    if end_name != name {
                        return Err(MathError::Malformed {
                            what: format!("\\begin{{{name}}} closed by \\end{{{end_name}}}"),
                            at: span.start,
                        });
                    }
                    rows.push(std::mem::take(&mut row));
                    break span;
                }
                Reason::EndOfInput => {
                    return Err(MathError::Malformed {
                        what: format!("unclosed \\begin{{{name}}}"),
                        at: self.src.len(),
                    });
                }
                other => {
                    return Err(MathError::Malformed {
                        what: format!("\\begin{{{name}}} interrupted before its \\end"),
                        at: other.span().start,
                    });
                }
            }
        };
        // LaTeX ignores the empty row a trailing \\ would create.
        if let Some(last) = rows.last() {
            if last.len() == 1 && matches!(&last[0].kind, NodeKind::List(items) if items.is_empty())
            {
                rows.pop();
            }
        }
        // The end token's span is the \end control word; extend to cover the
        // trailing name group if the source has one.
        let full_end = self
            .toks
            .get(self.pos.saturating_sub(1))
            .map_or(end_span, |t| t.span);
        Ok(Node::new(
            NodeKind::Environment { name, spec, rows },
            begin_span.union(end_span).union(full_end),
        ))
    }

    /// A `^`/`_`/`'` script cluster attaching to the preceding atom (or to
    /// an empty base, as TeX does when a script opens a list).
    fn script_cluster(&mut self, items: &mut Vec<Node>) -> Result<(), MathError> {
        let base = match items.last() {
            Some(last) if intrinsic_class(last).is_some() => items.pop().map(Box::new),
            _ => None,
        };
        let mut sup: Option<Box<Node>> = None;
        let mut sub: Option<Box<Node>> = None;
        let mut primes: Vec<Span> = Vec::new();
        let start_span = base.as_deref().map_or_else(
            || self.peek().map_or(Span::new(0, 0), |t| t.span),
            |b| b.span,
        );
        let mut end_span = start_span;
        loop {
            self.skip_spaces();
            let Some(tok) = self.peek().cloned() else {
                break;
            };
            match tok.kind {
                TokKind::Sup => {
                    if sup.is_some() || !primes.is_empty() {
                        return Err(MathError::Malformed {
                            what: "double superscript".to_owned(),
                            at: tok.span.start,
                        });
                    }
                    self.pos += 1;
                    let arg = self.descend(tok.span.start, |p| p.argument("superscript"))?;
                    end_span = end_span.union(arg.span);
                    sup = Some(Box::new(arg));
                }
                TokKind::Sub => {
                    if sub.is_some() {
                        return Err(MathError::Malformed {
                            what: "double subscript".to_owned(),
                            at: tok.span.start,
                        });
                    }
                    self.pos += 1;
                    let arg = self.descend(tok.span.start, |p| p.argument("subscript"))?;
                    end_span = end_span.union(arg.span);
                    sub = Some(Box::new(arg));
                }
                TokKind::Char('\'') => {
                    if sup.is_some() {
                        return Err(MathError::Malformed {
                            what: "double superscript (prime follows an explicit superscript)"
                                .to_owned(),
                            at: tok.span.start,
                        });
                    }
                    self.pos += 1;
                    primes.push(tok.span);
                    end_span = end_span.union(tok.span);
                }
                _ => break,
            }
        }
        items.push(Node::new(
            NodeKind::Scripts {
                base,
                sub,
                sup,
                primes,
            },
            start_span.union(end_span),
        ));
        Ok(())
    }

    // ── Text mode (the TexText contract) ────────────────────────────────

    /// Parse a text-mode list until a stop token.
    #[allow(clippy::too_many_lines)]
    fn text_list(&mut self) -> Result<(Vec<Node>, Reason), MathError> {
        let mut items: Vec<Node> = Vec::new();
        let mut run = String::new();
        let mut run_spans: Vec<Span> = Vec::new();
        let reason = loop {
            let Some(tok) = self.peek().cloned() else {
                break Reason::EndOfInput;
            };
            match tok.kind {
                TokKind::Char(c) => {
                    self.pos += 1;
                    run.push(c);
                    run_spans.push(tok.span);
                }
                TokKind::Space => {
                    self.pos += 1;
                    if !run.is_empty() {
                        run.push(' ');
                        run_spans.push(tok.span);
                    }
                }
                TokKind::Tie => {
                    self.pos += 1;
                    flush_run(&mut items, &mut run, &mut run_spans);
                    items.push(Node::new(NodeKind::Tie, tok.span));
                }
                TokKind::MathShift => {
                    self.pos += 1;
                    flush_run(&mut items, &mut run, &mut run_spans);
                    // An immediately adjacent second '$' opens display
                    // mathematics ($$…$$).
                    let display = matches!(self.peek().map(|t| t.kind), Some(TokKind::MathShift));
                    if display {
                        self.pos += 1;
                    }
                    let (body, reason) =
                        self.descend(tok.span.start, |p| p.math_list(Stops::default()))?;
                    let close_end = match reason {
                        Reason::MathShift(close) => {
                            if display {
                                match self.peek().cloned() {
                                    Some(t) if matches!(t.kind, TokKind::MathShift) => {
                                        self.pos += 1;
                                        t.span.end
                                    }
                                    // Fragment: end of input stands in for
                                    // the second closing '$'.
                                    None => close.end,
                                    Some(t) => {
                                        return Err(MathError::Malformed {
                                            what: format!(
                                                "display mathematics opened with $$ at byte {} \
                                                 must close with $$",
                                                tok.span.start
                                            ),
                                            at: t.span.start,
                                        });
                                    }
                                }
                            } else {
                                close.end
                            }
                        }
                        // Fragment: the island's closer lives in a later
                        // piece; end of input closes it.
                        Reason::EndOfInput => self.src.len(),
                        other => {
                            return Err(MathError::Malformed {
                                what: format!(
                                    "the '$' math island opened at byte {} is interrupted",
                                    tok.span.start
                                ),
                                at: other.span().start,
                            });
                        }
                    };
                    items.push(Node::new(
                        NodeKind::MathIsland { body, display },
                        Span::new(tok.span.start, close_end),
                    ));
                }
                TokKind::BeginGroup => {
                    self.pos += 1;
                    flush_run(&mut items, &mut run, &mut run_spans);
                    let (body, reason) = self.descend(tok.span.start, |p| p.text_list())?;
                    match reason {
                        Reason::EndGroup(close) => {
                            items.push(Node::new(NodeKind::List(body), tok.span.union(close)));
                        }
                        Reason::EndOfInput => {
                            return Err(MathError::Malformed {
                                what: format!("unclosed '{{' opened at byte {}", tok.span.start),
                                at: self.src.len(),
                            });
                        }
                        other => return Err(unexpected_close(&other, '{', tok.span)),
                    }
                }
                TokKind::EndGroup => {
                    self.pos += 1;
                    flush_run(&mut items, &mut run, &mut run_spans);
                    break Reason::EndGroup(tok.span);
                }
                TokKind::AlignTab => {
                    return Err(MathError::Malformed {
                        what: "'&' outside an alignment".to_owned(),
                        at: tok.span.start,
                    });
                }
                TokKind::Sup | TokKind::Sub => {
                    // A bare script in prose ("e^0 = 1"): the Reference-era
                    // LaTeX recovers by inserting the missing '$'; keep the
                    // behavior as an explicit implicit island with an empty
                    // script base.
                    flush_run(&mut items, &mut run, &mut run_spans);
                    let mut cluster: Vec<Node> = Vec::new();
                    self.script_cluster(&mut cluster)?;
                    if let Some(node) = cluster.pop() {
                        let span = node.span;
                        items.push(Node::new(
                            NodeKind::MathIsland {
                                body: vec![node],
                                display: false,
                            },
                            span,
                        ));
                    }
                }
                TokKind::ControlSymbol(c) => {
                    self.pos += 1;
                    match c {
                        // Escapes join the surrounding run (per-char spans
                        // keep the two-byte provenance exact).
                        '$' | '%' | '&' | '#' | '_' | '{' | '}' => {
                            run.push(c);
                            run_spans.push(tok.span);
                            continue;
                        }
                        _ => {}
                    }
                    flush_run(&mut items, &mut run, &mut run_spans);
                    match c {
                        '\\' => items.push(Node::new(NodeKind::Linebreak, tok.span)),
                        ',' => items.push(Node::new(NodeKind::Space(SpaceKind::Thin), tok.span)),
                        ':' => items.push(Node::new(NodeKind::Space(SpaceKind::Med), tok.span)),
                        ';' => {
                            items.push(Node::new(NodeKind::Space(SpaceKind::Thick), tok.span));
                        }
                        '!' => {
                            items.push(Node::new(NodeKind::Space(SpaceKind::NegThin), tok.span));
                        }
                        ' ' => items.push(Node::new(
                            NodeKind::Space(SpaceKind::ControlSpace),
                            tok.span,
                        )),
                        other => {
                            return Err(MathError::UnsupportedCommand {
                                name: format!("\\{other}"),
                                span: tok.span,
                            });
                        }
                    }
                }
                TokKind::ControlWord(name) => {
                    flush_run(&mut items, &mut run, &mut run_spans);
                    let node = self.text_control_word(name, tok.span)?;
                    items.push(node);
                }
            }
        };
        flush_run(&mut items, &mut run, &mut run_spans);
        Ok((items, reason))
    }

    /// A control word in text mode.
    fn text_control_word(&mut self, name: &'s str, span: Span) -> Result<Node, MathError> {
        let Some(cmd) = commands::lookup(name) else {
            return Err(MathError::UnsupportedCommand {
                name: format!("\\{name}"),
                span,
            });
        };
        self.pos += 1;
        match cmd {
            Cmd::UnsupportedT2 => Err(MathError::UnsupportedCommand {
                name: format!("\\{name}"),
                span,
            }),
            Cmd::TextStyled(style) => {
                let (body, body_span) = self.text_island_argument(name)?;
                Ok(Node::new(
                    NodeKind::TextStyled { style, body },
                    span.union(body_span),
                ))
            }
            // `\underline` in text mode is a styling island, not the math
            // accent.
            Cmd::Accent(AccentKind::UnderLine) => {
                let (body, body_span) = self.text_island_argument(name)?;
                Ok(Node::new(
                    NodeKind::TextStyled {
                        style: TextStyle::Underline,
                        body,
                    },
                    span.union(body_span),
                ))
            }
            Cmd::Text => {
                let (body, body_span) = self.text_island_argument(name)?;
                Ok(Node::new(NodeKind::List(body), span.union(body_span)))
            }
            Cmd::Spacing(kind) => Ok(Node::new(NodeKind::Space(kind), span)),
            Cmd::Begin => self.environment(span),
            Cmd::End => {
                let (env_name, _) = self.raw_group("environment name after \\end")?;
                Err(MathError::Malformed {
                    what: format!("\\end{{{env_name}}} without a matching \\begin"),
                    at: span.start,
                })
            }
            // A structural math command has no meaning in the text
            // mainland: precise error.
            Cmd::OverInfix(_) | Cmd::Right | Cmd::Limits | Cmd::NoLimits | Cmd::StyleSwitch(_) => {
                Err(MathError::Malformed {
                    what: format!("\\{name} is a math-mode command; wrap it in $…$"),
                    at: span.start,
                })
            }
            // A self-contained math command in the text mainland becomes an
            // implicit one-command math island. The Reference era's LaTeX
            // accepted `\dots`/`\Rightarrow`/`\pi` in TexText mainlands by
            // inserting the missing `$` and recovering; the corpus leans on
            // that, so the TexText contract keeps the behavior — explicitly
            // and losslessly, as a MathIsland node.
            other => {
                let body = self.descend(span.start, |p| p.command_node(name, span, other))?;
                let island_span = body.span.union(span);
                Ok(Node::new(
                    NodeKind::MathIsland {
                        body: vec![body],
                        display: false,
                    },
                    island_span,
                ))
            }
        }
    }
}

/// Fold a recorded `\over`-class split into a single fraction node.
fn resolve_over(items: Vec<Node>, over: Option<(usize, FracSpec, Span)>) -> Vec<Node> {
    let Some((split, spec, over_span)) = over else {
        return items;
    };
    let mut num_items = items;
    let den_items = num_items.split_off(split.min(num_items.len()));
    let num_span = list_span(&num_items, over_span.start);
    let den_span = list_span(&den_items, over_span.end);
    let whole = num_span.union(den_span).union(over_span);
    vec![Node::new(
        NodeKind::Frac {
            num: Box::new(Node::new(NodeKind::List(num_items), num_span)),
            den: Box::new(Node::new(NodeKind::List(den_items), den_span)),
            spec,
        },
        whole,
    )]
}

/// Set the limits mode on the trailing big operator, reaching through an
/// already-attached script cluster. Returns false when there is no operator
/// to modify.
fn set_limits(last: Option<&mut Node>, mode: Limits) -> bool {
    match last.map(|n| &mut n.kind) {
        Some(NodeKind::BigOp { limits, .. }) => {
            *limits = mode;
            true
        }
        Some(NodeKind::OpName { limits, .. }) => {
            *limits = matches!(mode, Limits::Limits);
            true
        }
        Some(NodeKind::Scripts {
            base: Some(base), ..
        }) => set_limits(Some(base.as_mut()), mode),
        _ => false,
    }
}

/// Flush the pending text run into the item list.
fn flush_run(items: &mut Vec<Node>, run: &mut String, run_spans: &mut Vec<Span>) {
    if run.is_empty() {
        run_spans.clear();
        return;
    }
    let char_spans = std::mem::take(run_spans);
    let span = match (char_spans.first(), char_spans.last()) {
        (Some(first), Some(last)) => first.union(*last),
        _ => Span::new(0, 0),
    };
    items.push(Node::new(
        NodeKind::TextRun {
            text: std::mem::take(run),
            char_spans,
        },
        span,
    ));
}

/// The current list was closed by the wrong closer.
fn unexpected_close(reason: &Reason, opener: char, open: Span) -> MathError {
    MathError::Malformed {
        what: format!(
            "the '{opener}' opened at byte {} is closed by the wrong construct",
            open.start
        ),
        at: reason.span().start,
    }
}

/// Control-flow outcome of a list-level control word.
enum ControlFlow {
    Continue,
    Stop(Reason),
}
