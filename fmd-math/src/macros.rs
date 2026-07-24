//! User macros and preamble packs: `\newcommand`-tier non-recursive
//! substitution, expanded at the token level before parsing (§11.4).
//!
//! # The model
//!
//! A [`MacroSet`] is a named table of substitution macros — either a
//! **preamble pack** (the `tex_templates.yml` concept reborn: a macro/symbol
//! bundle selected by config, looked up here by its stable content id) or
//! definitions a consumer assembles. Source strings may additionally define
//! macros inline with `\newcommand{\name}[n]{body}` / `\renewcommand`;
//! inline definitions layer over the pack (`\newcommand` refuses to shadow
//! an existing name, `\renewcommand` requires one — LaTeX's own rules).
//!
//! Expansion is **token-level**, before the grammar: a macro call's
//! arguments are collected as balanced token groups (or single tokens, the
//! TeX undelimited-argument rule), the body's `#k` parameters splice the
//! argument tokens in, and the result is rescanned so macros may reference
//! other macros. Two disciplines make this safe under the parser-budget
//! doctrine (§16.5):
//!
//! - **Recursion is refused, by name**: a macro that re-enters its own
//!   expansion — directly, through another macro, or through an argument —
//!   is a precise [`MathError::Malformed`] naming the macro, never a hang.
//! - **Expansion is budgeted**: total spliced tokens and nesting depth are
//!   capped, so a fan-out bomb errors cleanly.
//!
//! # Provenance (§11.3)
//!
//! Body-produced tokens carry the **call site's span** (the expansion
//! site — exactly the rule command-produced glyphs already follow), while
//! argument tokens keep their own source spans (they are real source
//! text). `isolate` and `tex_to_color_map` therefore keep working through
//! macros: the literal pieces match by their true spans, the produced
//! material by the macro call that made it.
//!
//! # Cache identity
//!
//! [`MacroSet::canonical_bytes`] is a deterministic serialization of the
//! whole table (sorted, delimited, versioned). Consumers fold it into
//! their typeset cache keys, so **a pack change re-typesets, correctly** —
//! the §14.4 requirement.

use crate::error::MathError;
use crate::node::Span;
use crate::token::{Tok, TokKind, lex};
use std::collections::BTreeMap;

/// Total tokens an expansion may produce before it is refused as a bomb.
const EXPANSION_TOKEN_BUDGET: usize = 65_536;
/// Nesting depth of macro-within-macro expansion.
const EXPANSION_DEPTH_BUDGET: usize = 32;

/// A named table of `\newcommand`-tier substitution macros. See the module
/// docs for the expansion, provenance, and budget rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MacroSet {
    defs: BTreeMap<String, MacroDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MacroDef {
    /// Parameter count, 0..=9.
    params: u8,
    /// The body, TeX source.
    body: String,
}

impl MacroSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The builtin preamble packs, by stable content id (the ids the
    /// fmn-config registry records): `fmd-math/pack/default` (the everyday
    /// bundle), `fmd-math/pack/basic` (minimal), `fmd-math/pack/empty`
    /// (bare primitives). Plain names (`default`, `basic`, `empty`) are
    /// accepted too.
    #[must_use]
    pub fn pack(id: &str) -> Option<Self> {
        match id {
            "fmd-math/pack/default" | "default" => {
                // The Reference's default template declares `\minus`, a
                // binary-minus shorthand (its one real macro; the rest of
                // its preamble is package loading with no native meaning).
                // Built directly — the definition is static and trivially
                // valid (the tests define the same macro through the
                // validating path).
                let mut defs = BTreeMap::new();
                defs.insert(
                    "minus".to_owned(),
                    MacroDef {
                        params: 0,
                        body: "-".to_owned(),
                    },
                );
                Some(Self { defs })
            }
            "fmd-math/pack/basic" | "basic" | "fmd-math/pack/empty" | "empty" => Some(Self::new()),
            _ => None,
        }
    }

    /// Define a macro: `params` parameters (`#1`…`#9`), a TeX-source body.
    /// Replaces any existing definition of the name (packs are assembled
    /// with this; *source-level* shadowing rules are `\newcommand`'s).
    ///
    /// # Errors
    ///
    /// [`MathError::Malformed`] (at byte 0 of the definition body) for an
    /// invalid name, too many parameters, an unbalanced body, or a `#k`
    /// outside `1..=params`.
    pub fn define(&mut self, name: &str, params: u8, body: &str) -> Result<(), MathError> {
        let malformed = |what: String| MathError::Malformed { what, at: 0 };
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err(malformed(format!(
                "macro name {name:?} must be one or more ASCII letters"
            )));
        }
        if params > 9 {
            return Err(malformed(format!(
                "macro \\{name} declares {params} parameters; TeX allows at most 9"
            )));
        }
        validate_body(name, params, body)?;
        self.defs.insert(
            name.to_owned(),
            MacroDef {
                params,
                body: body.to_owned(),
            },
        );
        Ok(())
    }

    /// The defined names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
    }

    /// How many macros are defined.
    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// A deterministic serialization of the whole table — the cache-key
    /// ingredient (hash these bytes; equal bytes ⇔ equal macro semantics).
    /// Format: a version tag, then `name US params US body RS` per macro in
    /// sorted order (US/RS are the ASCII unit/record separators, which
    /// cannot appear in names and are vanishingly unlikely in bodies; the
    /// version tag changes if this framing ever does).
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = b"fmd-math-macroset-v1\x1e".to_vec();
        for (name, def) in &self.defs {
            out.extend_from_slice(name.as_bytes());
            out.push(0x1f);
            out.push(b'0' + def.params);
            out.push(0x1f);
            out.extend_from_slice(def.body.as_bytes());
            out.push(0x1e);
        }
        out
    }
}

/// Validate a macro body at definition time: balanced groups and in-range
/// `#k` references, so use-site errors can only be about *use*.
fn validate_body(name: &str, params: u8, body: &str) -> Result<(), MathError> {
    let malformed = |what: String| MathError::Malformed { what, at: 0 };
    let mut depth = 0_i32;
    let toks = lex(body);
    let mut i = 0;
    while i < toks.len() {
        match toks[i].kind {
            TokKind::BeginGroup => depth += 1,
            TokKind::EndGroup => {
                depth -= 1;
                if depth < 0 {
                    return Err(malformed(format!(
                        "macro \\{name} body has an unmatched '}}'"
                    )));
                }
            }
            TokKind::Char('#') => {
                let param = toks.get(i + 1).and_then(|t| match t.kind {
                    TokKind::Char(c) => c.to_digit(10),
                    _ => None,
                });
                match param {
                    Some(d) if (1..=u32::from(params)).contains(&d) => i += 1,
                    Some(d) => {
                        return Err(malformed(format!(
                            "macro \\{name} body uses #{d} but declares {params} parameter(s)"
                        )));
                    }
                    None => {
                        return Err(malformed(format!(
                            "macro \\{name} body has a '#' not followed by a parameter digit"
                        )));
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    if depth != 0 {
        return Err(malformed(format!(
            "macro \\{name} body has {depth} unclosed '{{'"
        )));
    }
    Ok(())
}

/// One live macro during expansion: the definition's body, pre-lexed.
struct Live<'a> {
    params: u8,
    body: Vec<Tok<'a>>,
}

/// Expand a lexed token stream against a macro set, processing inline
/// `\newcommand`/`\renewcommand` definitions. Returns the expanded stream;
/// tokens spliced from macro bodies carry their call site's span.
pub(crate) fn expand<'a>(
    toks: Vec<Tok<'a>>,
    set: &'a MacroSet,
    src_len: usize,
) -> Result<Vec<Tok<'a>>, MathError> {
    // Fast path: nothing to expand and nothing to define.
    let involved = !set.is_empty()
        || toks
            .iter()
            .any(|t| matches!(t.kind, TokKind::ControlWord("newcommand" | "renewcommand")));
    if !involved {
        return Ok(toks);
    }

    let mut table: BTreeMap<&'a str, Live<'a>> = BTreeMap::new();
    for (name, def) in &set.defs {
        table.insert(
            name.as_str(),
            Live {
                params: def.params,
                body: lex(&def.body),
            },
        );
    }

    let mut cx = Expansion {
        table,
        budget: EXPANSION_TOKEN_BUDGET,
        src_len,
    };
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let tok = &toks[i];
        match tok.kind {
            TokKind::ControlWord(cw @ ("newcommand" | "renewcommand")) => {
                i = cx.definition(&toks, i, cw == "renewcommand")?;
            }
            TokKind::ControlWord(name) if cx.table.contains_key(name) => {
                let mut active = Vec::new();
                i = cx.call(&toks, i, name, &mut active, 0, &mut out)?;
            }
            _ => {
                out.push(tok.clone());
                i += 1;
            }
        }
    }
    Ok(out)
}

struct Expansion<'a> {
    table: BTreeMap<&'a str, Live<'a>>,
    budget: usize,
    src_len: usize,
}

impl<'a> Expansion<'a> {
    /// Parse an inline definition starting at `toks[i]` (the
    /// `\newcommand`/`\renewcommand` token); registers it and returns the
    /// index after the definition.
    fn definition(&mut self, toks: &[Tok<'a>], i: usize, renew: bool) -> Result<usize, MathError> {
        let cw_span = toks[i].span;
        let which = if renew {
            "\\renewcommand"
        } else {
            "\\newcommand"
        };
        let mut j = i + 1;
        let skip_space = |j: &mut usize| {
            while toks
                .get(*j)
                .is_some_and(|t| matches!(t.kind, TokKind::Space))
            {
                *j += 1;
            }
        };
        skip_space(&mut j);
        // The name: `{\name}` or bare `\name`.
        let braced = matches!(toks.get(j).map(|t| &t.kind), Some(TokKind::BeginGroup));
        if braced {
            j += 1;
            skip_space(&mut j);
        }
        let Some(name_tok) = toks.get(j) else {
            return Err(MathError::Malformed {
                what: format!("{which} ends before its macro name"),
                at: self.src_len,
            });
        };
        let TokKind::ControlWord(name) = name_tok.kind else {
            return Err(MathError::Malformed {
                what: format!("{which} expects a \\name to define"),
                at: name_tok.span.start,
            });
        };
        j += 1;
        if braced {
            skip_space(&mut j);
            let Some(Tok {
                kind: TokKind::EndGroup,
                ..
            }) = toks.get(j)
            else {
                return Err(MathError::Malformed {
                    what: format!("{which}{{\\{name}}} has an unclosed name group"),
                    at: toks.get(j).map_or(self.src_len, |t| t.span.start),
                });
            };
            j += 1;
        }
        skip_space(&mut j);
        // Optional parameter count `[n]`.
        let mut params = 0_u8;
        if matches!(toks.get(j).map(|t| &t.kind), Some(TokKind::Char('['))) {
            let digit = toks.get(j + 1).and_then(|t| match t.kind {
                TokKind::Char(c) => c.to_digit(10),
                _ => None,
            });
            let close = matches!(toks.get(j + 2).map(|t| &t.kind), Some(TokKind::Char(']')));
            match (digit, close) {
                (Some(d @ 1..=9), true) => {
                    params = u8::try_from(d).unwrap_or(9);
                    j += 3;
                }
                _ => {
                    return Err(MathError::Malformed {
                        what: format!("{which}{{\\{name}}}: expected [1]..[9] parameter count"),
                        at: toks.get(j).map_or(self.src_len, |t| t.span.start),
                    });
                }
            }
            skip_space(&mut j);
        }
        // The body: one balanced group.
        let Some(Tok {
            kind: TokKind::BeginGroup,
            ..
        }) = toks.get(j)
        else {
            return Err(MathError::Malformed {
                what: format!("{which}{{\\{name}}}: expected a {{body}} group"),
                at: toks.get(j).map_or(self.src_len, |t| t.span.start),
            });
        };
        let body_start = j + 1;
        let mut depth = 1_i32;
        let mut k = body_start;
        while k < toks.len() {
            match toks[k].kind {
                TokKind::BeginGroup => depth += 1,
                TokKind::EndGroup => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        if depth != 0 {
            return Err(MathError::Malformed {
                what: format!("{which}{{\\{name}}}: unclosed body group"),
                at: self.src_len,
            });
        }
        // LaTeX's shadowing rules, kept: \newcommand refuses to redefine,
        // \renewcommand requires a prior definition.
        let exists = self.table.contains_key(name);
        if !renew && exists {
            return Err(MathError::Malformed {
                what: format!(
                    "\\newcommand: \\{name} is already defined (use \\renewcommand to replace it)"
                ),
                at: cw_span.start,
            });
        }
        if renew && !exists {
            return Err(MathError::Malformed {
                what: format!("\\renewcommand: \\{name} is not defined (use \\newcommand)"),
                at: cw_span.start,
            });
        }
        // Validate the body's parameter references now, so use sites can't
        // fail on definition faults.
        let body = toks[body_start..k].to_vec();
        validate_body_tokens(name, params, &body, cw_span.start)?;
        self.table
            .insert(name_interned(toks, j, name), Live { params, body });
        Ok(k + 1)
    }

    /// Expand one macro call at `toks[i]`; pushes onto `out` and returns
    /// the index after the call's arguments.
    fn call(
        &mut self,
        toks: &[Tok<'a>],
        i: usize,
        name: &'a str,
        active: &mut Vec<String>,
        depth: usize,
        out: &mut Vec<Tok<'a>>,
    ) -> Result<usize, MathError> {
        let call_start = toks[i].span;
        if depth >= EXPANSION_DEPTH_BUDGET {
            return Err(MathError::Malformed {
                what: format!(
                    "macro expansion nests deeper than {EXPANSION_DEPTH_BUDGET} (at \\{name})"
                ),
                at: call_start.start,
            });
        }
        if active.iter().any(|a| a == name) {
            return Err(MathError::Malformed {
                what: format!(
                    "recursive macro: \\{name} expands itself (macros are non-recursive substitutions)"
                ),
                at: call_start.start,
            });
        }
        let params = self.table.get(name).map(|l| l.params).unwrap_or(0);
        // Collect the arguments: balanced groups, or one token (TeX's
        // undelimited-argument rule), skipping intervening spaces.
        let mut j = i + 1;
        let mut args: Vec<&[Tok<'a>]> = Vec::new();
        let mut end_span = call_start;
        for argn in 1..=params {
            while toks
                .get(j)
                .is_some_and(|t| matches!(t.kind, TokKind::Space))
            {
                j += 1;
            }
            let Some(first) = toks.get(j) else {
                return Err(MathError::Malformed {
                    what: format!("\\{name} needs {params} argument(s); input ends before #{argn}"),
                    at: self.src_len,
                });
            };
            if matches!(first.kind, TokKind::BeginGroup) {
                let start = j + 1;
                let mut depth_b = 1_i32;
                let mut k = start;
                while k < toks.len() {
                    match toks[k].kind {
                        TokKind::BeginGroup => depth_b += 1,
                        TokKind::EndGroup => {
                            depth_b -= 1;
                            if depth_b == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                if depth_b != 0 {
                    return Err(MathError::Malformed {
                        what: format!("\\{name}: unclosed argument group for #{argn}"),
                        at: self.src_len,
                    });
                }
                args.push(&toks[start..k]);
                end_span = toks[k].span;
                j = k + 1;
            } else {
                args.push(core::slice::from_ref(first));
                end_span = first.span;
                j += 1;
            }
        }
        let call_span = call_start.union(end_span);
        self.splice(name, &args, call_span, active, depth, out)?;
        Ok(j)
    }

    /// Splice a macro's body: `#k` becomes the argument's tokens (their own
    /// spans — they are source text), everything else the body token with
    /// the call site's span; nested calls expand recursively.
    fn splice(
        &mut self,
        name: &'a str,
        args: &[&[Tok<'a>]],
        call_span: Span,
        active: &mut Vec<String>,
        depth: usize,
        out: &mut Vec<Tok<'a>>,
    ) -> Result<(), MathError> {
        active.push(name.to_owned());
        // The body is cloned out of the table so nested expansion may
        // consult the table freely (bodies are small; the budget bounds
        // the total work).
        let body = self
            .table
            .get(name)
            .map(|l| l.body.clone())
            .unwrap_or_default();
        let mut j = 0;
        while j < body.len() {
            let t = &body[j];
            match t.kind {
                TokKind::Char('#') => {
                    let d = body.get(j + 1).and_then(|n| match n.kind {
                        TokKind::Char(c) => c.to_digit(10),
                        _ => None,
                    });
                    let Some(d) = d else {
                        return Err(MathError::Malformed {
                            what: format!("macro \\{name} body has a stray '#'"),
                            at: call_span.start,
                        });
                    };
                    let arg = args.get(d as usize - 1).copied().unwrap_or(&[]);
                    // Argument tokens keep their own spans; nested calls
                    // within them expand under the active stack.
                    let mut k = 0;
                    while k < arg.len() {
                        match arg[k].kind {
                            TokKind::ControlWord(n) if self.table.contains_key(n) => {
                                k = self.call(arg, k, n, active, depth + 1, out)?
                            }
                            _ => {
                                self.push(out, arg[k].clone(), call_span, false)?;
                                k += 1;
                            }
                        }
                    }
                    j += 2;
                }
                TokKind::ControlWord(n) if self.table.contains_key(n) => {
                    j = self.call(&body, j, n, active, depth + 1, out)?;
                }
                _ => {
                    self.push(out, t.clone(), call_span, true)?;
                    j += 1;
                }
            }
        }
        active.pop();
        Ok(())
    }

    /// Push one token, rewriting body-material spans to the call site and
    /// enforcing the token budget.
    fn push(
        &mut self,
        out: &mut Vec<Tok<'a>>,
        mut tok: Tok<'a>,
        call_span: Span,
        rewrite_span: bool,
    ) -> Result<(), MathError> {
        if self.budget == 0 {
            return Err(MathError::Malformed {
                what: format!("macro expansion produced more than {EXPANSION_TOKEN_BUDGET} tokens"),
                at: call_span.start,
            });
        }
        self.budget -= 1;
        if rewrite_span {
            tok.span = call_span;
        }
        out.push(tok);
        Ok(())
    }
}

/// The interned name for a definition: the `&'a str` slice out of the
/// source tokens (the definition's own name token), so the table key lives
/// as long as the stream.
fn name_interned<'a>(toks: &[Tok<'a>], upto: usize, name: &'a str) -> &'a str {
    // The name token was within toks[..upto]; its ControlWord slice already
    // borrows 'a. `name` IS that slice — just return it.
    let _ = (toks, upto);
    name
}

/// Token-level body validation for inline definitions (the string-level
/// twin lives in [`validate_body`]).
fn validate_body_tokens(
    name: &str,
    params: u8,
    body: &[Tok<'_>],
    at: usize,
) -> Result<(), MathError> {
    let mut j = 0;
    while j < body.len() {
        if let TokKind::Char('#') = body[j].kind {
            let d = body.get(j + 1).and_then(|n| match n.kind {
                TokKind::Char(c) => c.to_digit(10),
                _ => None,
            });
            match d {
                Some(d) if (1..=u32::from(params)).contains(&d) => j += 1,
                Some(d) => {
                    return Err(MathError::Malformed {
                        what: format!(
                            "macro \\{name} body uses #{d} but declares {params} parameter(s)"
                        ),
                        at,
                    });
                }
                None => {
                    return Err(MathError::Malformed {
                        what: format!("macro \\{name} body has a '#' not followed by a digit"),
                        at,
                    });
                }
            }
        }
        j += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn expand_str<'a>(src: &'a str, set: &'a MacroSet) -> Result<String, MathError> {
        let toks = expand(lex(src), set, src.len())?;
        Ok(toks
            .iter()
            .map(|t| match &t.kind {
                TokKind::ControlWord(w) => format!("\\{w} "),
                TokKind::ControlSymbol(c) => format!("\\{c}"),
                TokKind::BeginGroup => "{".into(),
                TokKind::EndGroup => "}".into(),
                TokKind::Sup => "^".into(),
                TokKind::Sub => "_".into(),
                TokKind::AlignTab => "&".into(),
                TokKind::Tie => "~".into(),
                TokKind::MathShift => "$".into(),
                TokKind::Space => " ".into(),
                TokKind::Char(c) => (*c).to_string(),
            })
            .collect())
    }

    #[test]
    fn pack_macros_expand_with_call_site_spans() {
        let set = MacroSet::pack("fmd-math/pack/default").unwrap();
        let src = r"a\minus b";
        let toks = expand(lex(src), &set, src.len()).unwrap();
        // \minus → '-', carrying the \minus call's span.
        let minus = toks
            .iter()
            .find(|t| matches!(t.kind, TokKind::Char('-')))
            .expect("expanded minus");
        assert_eq!((minus.span.start, minus.span.end), (1, 7));
    }

    #[test]
    fn inline_definition_with_arguments() {
        let set = MacroSet::new();
        let out = expand_str(r"\newcommand{\half}[1]{\frac{#1}{2}}\half{x}", &set).unwrap();
        assert_eq!(out, r"\frac {x}{2}");
    }

    #[test]
    fn arguments_keep_their_own_spans_and_bodies_take_the_call() {
        let set = MacroSet::new();
        let src = r"\newcommand{\half}[1]{\frac{#1}{2}}\half{x}";
        let toks = expand(lex(src), &set, src.len()).unwrap();
        let call_start = src.find(r"\half{x}").unwrap();
        // The literal argument x keeps its true source span.
        let x = toks
            .iter()
            .find(|t| matches!(t.kind, TokKind::Char('x')))
            .unwrap();
        assert_eq!(&src[x.span.start..x.span.end], "x");
        // Body material (\frac, the 2, the braces) carries the call span.
        let frac = toks
            .iter()
            .find(|t| matches!(t.kind, TokKind::ControlWord("frac")))
            .unwrap();
        assert_eq!(frac.span.start, call_start);
        assert_eq!(frac.span.end, src.len());
    }

    #[test]
    fn macros_reference_other_macros() {
        let mut set = MacroSet::new();
        set.define("dd", 0, r"\mathrm{d}").unwrap();
        set.define("dx", 0, r"\dd x").unwrap();
        let out = expand_str(r"\dx", &set).unwrap();
        assert_eq!(out, r"\mathrm {d}x");
    }

    #[test]
    fn recursion_is_refused_with_the_macro_named() {
        let mut set = MacroSet::new();
        set.define("loop", 0, r"a\loop").unwrap();
        let err = expand_str(r"\loop", &set).unwrap_err();
        assert!(err.to_string().contains("recursive macro: \\loop"), "{err}");

        // Mutual recursion too.
        let mut set = MacroSet::new();
        set.define("ping", 0, r"\pong").unwrap();
        set.define("pong", 0, r"\ping").unwrap();
        let err = expand_str(r"\ping", &set).unwrap_err();
        assert!(err.to_string().contains("recursive macro"), "{err}");
    }

    #[test]
    fn expansion_bombs_hit_the_budget() {
        // Exponential fan-out without self-reference: geometric doubling
        // through a chain long enough to exceed the token budget.
        let mut set = MacroSet::new();
        set.define("a", 0, "xx").unwrap();
        for (prev, name) in [
            ("a", "b"),
            ("b", "c"),
            ("c", "d"),
            ("d", "e"),
            ("e", "f"),
            ("f", "g"),
            ("g", "h"),
            ("h", "i"),
            ("i", "j"),
            ("j", "k"),
            ("k", "l"),
            ("l", "m"),
            ("m", "n"),
            ("n", "o"),
            ("o", "p"),
            ("p", "q"),
            ("q", "r"),
        ] {
            let body = format!("\\{prev}\\{prev}");
            set.define(name, 0, &body).unwrap();
        }
        let err = expand_str(r"\r", &set).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("more than") || msg.contains("nests deeper"),
            "{msg}"
        );
    }

    #[test]
    fn shadowing_rules_are_latexs() {
        let set = MacroSet::new();
        // \newcommand refuses to redefine.
        let err = expand_str(r"\newcommand{\x}{a}\newcommand{\x}{b}", &set).unwrap_err();
        assert!(err.to_string().contains("already defined"), "{err}");
        // \renewcommand requires a definition.
        let err = expand_str(r"\renewcommand{\y}{a}", &set).unwrap_err();
        assert!(err.to_string().contains("not defined"), "{err}");
        // The legal pair works.
        let out = expand_str(r"\newcommand{\x}{a}\renewcommand{\x}{b}\x", &set).unwrap();
        assert_eq!(out, "b");
    }

    #[test]
    fn definition_faults_are_precise() {
        let set = MacroSet::new();
        for (src, needle) in [
            (r"\newcommand", "ends before its macro name"),
            (r"\newcommand{x}{a}", "expects a \\name"),
            (r"\newcommand{\x}[0]{a}", "expected [1]..[9]"),
            (r"\newcommand{\x}[2]{#3}", "uses #3 but declares 2"),
            (r"\newcommand{\x}", "expected a {body} group"),
            (r"\newcommand{\x}{a", "unclosed body group"),
        ] {
            let err = expand_str(src, &set).unwrap_err();
            assert!(err.to_string().contains(needle), "{src}: {err}");
        }
    }

    #[test]
    fn undelimited_single_token_arguments() {
        let set = MacroSet::new();
        let out = expand_str(r"\newcommand{\sq}[1]{#1^2}\sq x", &set).unwrap();
        assert_eq!(out, "x^2");
    }

    #[test]
    fn canonical_bytes_are_deterministic_and_content_sensitive() {
        let mut a = MacroSet::new();
        a.define("dd", 0, r"\mathrm{d}").unwrap();
        a.define("half", 1, r"\frac{#1}{2}").unwrap();
        let mut b = MacroSet::new();
        // Insertion order must not matter (sorted table).
        b.define("half", 1, r"\frac{#1}{2}").unwrap();
        b.define("dd", 0, r"\mathrm{d}").unwrap();
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
        // Any content edit changes the bytes (a pack change re-typesets).
        let mut c = MacroSet::new();
        c.define("dd", 0, r"\mathrm{D}").unwrap();
        c.define("half", 1, r"\frac{#1}{2}").unwrap();
        assert_ne!(a.canonical_bytes(), c.canonical_bytes());
    }

    #[test]
    fn define_validation_is_precise() {
        let mut set = MacroSet::new();
        assert!(set.define("", 0, "x").is_err());
        assert!(set.define("bad name", 0, "x").is_err());
        assert!(set.define("x", 10, "y").is_err());
        assert!(set.define("x", 1, "#2").is_err());
        assert!(set.define("x", 0, "{unclosed").is_err());
        assert!(set.define("x", 0, "}stray").is_err());
        assert!(set.define("ok", 2, r"\frac{#1}{#2}").is_ok());
    }

    #[test]
    fn packs_exist_by_content_id_and_name() {
        for id in [
            "fmd-math/pack/default",
            "default",
            "fmd-math/pack/basic",
            "basic",
            "fmd-math/pack/empty",
            "empty",
        ] {
            assert!(MacroSet::pack(id).is_some(), "{id}");
        }
        assert!(MacroSet::pack("nonexistent").is_none());
        assert_eq!(MacroSet::pack("default").unwrap().len(), 1);
        assert!(MacroSet::pack("empty").unwrap().is_empty());
    }
}
