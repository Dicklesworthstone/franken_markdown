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
//! An inline definition may supply `[n][default]`: the first argument is
//! then optional at each call, and the remaining `n - 1` are mandatory.
//! Explicit `[]` is an empty argument, not a request to use the default.
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
//! - **Expansion is budgeted**: token-copy and substitution work, including
//!   intermediate replacements that disappear during rescanning, and nesting
//!   depth are capped. Visible and empty-output fan-out bombs error cleanly.
//!
//! # Provenance (§11.3)
//!
//! Body-produced tokens carry the **call site's span** (the expansion
//! site — exactly the rule command-produced glyphs already follow), while
//! argument tokens keep their own source spans (they are real source
//! text). Omitted optional arguments come from the definition and are
//! rebased to the call, just like body material. `isolate` and
//! `tex_to_color_map` therefore keep working through macros.
//!
//! # Cache identity
//!
//! [`MacroSet::canonical_bytes`] is a deterministic serialization of the
//! whole table (sorted, delimited, versioned). Consumers fold it into
//! their typeset cache keys, so **a pack change re-typesets, correctly** —
//! the §14.4 requirement. Inline defaults are part of the source string;
//! no optional-argument state persists between parse calls.

use crate::error::MathError;
use crate::node::Span;
use crate::token::{Tok, TokKind, lex};
use std::collections::BTreeMap;

/// Total token-copy and substitution work an expansion may perform.
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
    /// Some(empty) still means argument #1 is optional. Declaration spans
    /// are replaced by the invocation span when the default is actually used.
    default: Option<Vec<Tok<'a>>>,
}

/// Read an optional bracket argument without treating a protected `]` as a
/// terminator. Braces protect their entire contents; escaped bracket tokens
/// are literals. A closing brace may not be stolen from the surrounding group.
fn optional_group(
    toks: &[Tok<'_>],
    open: usize,
    src_len: usize,
    name: &str,
) -> Result<(usize, usize), MathError> {
    let start = open + 1;
    let mut groups = 0usize;
    for (index, token) in toks.iter().enumerate().skip(start) {
        match token.kind {
            TokKind::BeginGroup => groups += 1,
            TokKind::EndGroup if groups == 0 => {
                return Err(MathError::Malformed {
                    what: format!("\\{name}: optional argument closes a surrounding group"),
                    at: token.span.start,
                });
            }
            TokKind::EndGroup => groups -= 1,
            TokKind::Char(']') if groups == 0 => return Ok((start, index)),
            _ => {}
        }
    }
    Err(MathError::Malformed {
        what: format!("\\{name}: unclosed optional argument (expected ']')"),
        at: src_len,
    })
}

fn skip_spaces(toks: &[Tok<'_>], index: &mut usize) {
    while toks
        .get(*index)
        .is_some_and(|token| matches!(token.kind, TokKind::Space))
    {
        *index += 1;
    }
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
                default: None,
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
    /// Parse an inline definition, register it, and return its end index.
    fn definition(&mut self, toks: &[Tok<'a>], i: usize, renew: bool) -> Result<usize, MathError> {
        let cw_span = toks[i].span;
        let which = if renew {
            "\\renewcommand"
        } else {
            "\\newcommand"
        };
        let mut j = i + 1;
        skip_spaces(toks, &mut j);
        // The name: `{\name}` or bare `\name`.
        let braced = matches!(toks.get(j).map(|t| &t.kind), Some(TokKind::BeginGroup));
        if braced {
            j += 1;
            skip_spaces(toks, &mut j);
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
            skip_spaces(toks, &mut j);
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
        skip_spaces(toks, &mut j);
        // Optional parameter count `[n]`, followed by an optional default.
        let mut params = 0_u8;
        let mut default = None;
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
            skip_spaces(toks, &mut j);
            if matches!(toks.get(j).map(|t| &t.kind), Some(TokKind::Char('['))) {
                let (start, end) = optional_group(toks, j, self.src_len, name)?;
                // A default is literal replacement material, not a second
                // parameterized body. Escaped \# remains an ordinary token.
                validate_body_tokens(name, 0, &toks[start..end], cw_span.start)?;
                self.charge(end - start, cw_span)?;
                default = Some(toks[start..end].to_vec());
                j = end + 1;
                skip_spaces(toks, &mut j);
            }
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
        // Validate before replacing the live definition. Defaults and bodies
        // participate in the same bounded token-copy work accounting.
        validate_body_tokens(name, params, &toks[body_start..k], cw_span.start)?;
        self.charge(k - body_start, cw_span)?;
        let body = toks[body_start..k].to_vec();
        self.table.insert(
            name,
            Live {
                params,
                body,
                default,
            },
        );
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
        self.charge(1, call_start)?;
        let params = self.table.get(name).map(|live| live.params).unwrap_or(0);
        let optional = self
            .table
            .get(name)
            .is_some_and(|live| live.default.is_some());
        let mut j = i + 1;
        // Borrow explicit arguments. None marks an omitted optional argument;
        // only that case clones the definition's default token sequence.
        let mut arguments: Vec<Option<&[Tok<'a>]>> = Vec::new();
        let mut end_span = call_start;
        let mut use_default = false;
        if optional {
            skip_spaces(toks, &mut j);
            if matches!(
                toks.get(j).map(|token| &token.kind),
                Some(TokKind::Char('['))
            ) {
                let (start, end) = optional_group(toks, j, self.src_len, name)?;
                arguments.push(Some(&toks[start..end]));
                end_span = toks[end].span;
                j = end + 1;
            } else {
                arguments.push(None);
                use_default = true;
            }
        }
        for argn in arguments.len() + 1..=usize::from(params) {
            skip_spaces(toks, &mut j);
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
                arguments.push(Some(&toks[start..k]));
                end_span = toks[k].span;
                j = k + 1;
            } else if matches!(first.kind, TokKind::EndGroup) {
                return Err(MathError::Malformed {
                    what: format!("\\{name}: missing argument #{argn} before closing group"),
                    at: first.span.start,
                });
            } else {
                arguments.push(Some(core::slice::from_ref(first)));
                end_span = first.span;
                j += 1;
            }
        }
        let call_span = call_start.union(end_span);
        let mut default = Vec::new();
        if use_default {
            let count = self
                .table
                .get(name)
                .and_then(|live| live.default.as_ref())
                .map_or(0, Vec::len);
            self.charge(count, call_span)?;
            default = self
                .table
                .get(name)
                .and_then(|live| live.default.clone())
                .unwrap_or_default();
            for token in &mut default {
                token.span = call_span;
            }
        }
        let args: Vec<&[Tok<'a>]> = arguments
            .iter()
            .map(|argument| argument.unwrap_or(default.as_slice()))
            .collect();
        self.splice(name, &args, call_span, active, depth, out)?;
        Ok(j)
    }

    /// Substitute the entire body before rescanning it. A nested invocation
    /// such as `\inner{#2}{#1}` must see the outer call's actual arguments,
    /// not literal parameter tokens from the outer definition. Substitution
    /// boundaries must not become artificial boundaries for nested arguments.
    fn splice(
        &mut self,
        name: &'a str,
        args: &[&[Tok<'a>]],
        call_span: Span,
        active: &mut Vec<String>,
        depth: usize,
        out: &mut Vec<Tok<'a>>,
    ) -> Result<(), MathError> {
        let count = self.table.get(name).map_or(0, |live| live.body.len());
        // Charge traversal/copy work even when every parameter is empty and
        // the complete body subsequently disappears during substitution.
        self.charge(count, call_span)?;
        let body = self
            .table
            .get(name)
            .map(|live| live.body.clone())
            .unwrap_or_default();
        let mut replacement = Vec::new();
        let mut j = 0;
        while j < body.len() {
            let token = &body[j];
            if matches!(token.kind, TokKind::Char('#')) {
                let index = body.get(j + 1).and_then(|next| match next.kind {
                    TokKind::Char(c @ '1'..='9') => Some(c as usize - '1' as usize),
                    _ => None,
                });
                let Some(argument) = index.and_then(|index| args.get(index)) else {
                    return Err(MathError::Malformed {
                        what: format!("macro \\{name} body has an invalid parameter reference"),
                        at: call_span.start,
                    });
                };
                self.charge(argument.len(), call_span)?;
                replacement.extend_from_slice(argument);
                j += 2;
            } else {
                self.charge(1, call_span)?;
                let mut token = token.clone();
                token.span = call_span;
                replacement.push(token);
                j += 1;
            }
        }

        active.push(name.to_owned());
        let result = (|| {
            let mut cursor = 0;
            while cursor < replacement.len() {
                match replacement[cursor].kind {
                    TokKind::ControlWord(nested) if self.table.contains_key(nested) => {
                        cursor = self.call(&replacement, cursor, nested, active, depth + 1, out)?;
                    }
                    _ => {
                        out.push(replacement[cursor].clone());
                        cursor += 1;
                    }
                }
            }
            Ok(())
        })();
        active.pop();
        result
    }

    fn budget_error(span: Span) -> MathError {
        MathError::Malformed {
            what: format!(
                "macro expansion requires more than {EXPANSION_TOKEN_BUDGET} token work units"
            ),
            at: span.start,
        }
    }

    /// Charge before allocating replacements, including material later
    /// consumed by a nested macro. Counting final output alone leaves
    /// exponential empty-output expansions effectively unbounded.
    fn charge(&mut self, units: usize, span: Span) -> Result<(), MathError> {
        self.budget = self
            .budget
            .checked_sub(units)
            .ok_or_else(|| Self::budget_error(span))?;
        Ok(())
    }
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
        let x = toks
            .iter()
            .find(|t| matches!(t.kind, TokKind::Char('x')))
            .unwrap();
        assert_eq!(&src[x.span.start..x.span.end], "x");
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
        let mut set = MacroSet::new();
        set.define("ping", 0, r"\pong").unwrap();
        set.define("pong", 0, r"\ping").unwrap();
        let err = expand_str(r"\ping", &set).unwrap_err();
        assert!(err.to_string().contains("recursive macro"), "{err}");
    }

    #[test]
    fn expansion_bombs_hit_the_budget() {
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
        let err = expand_str(r"\newcommand{\x}{a}\newcommand{\x}{b}", &set).unwrap_err();
        assert!(err.to_string().contains("already defined"), "{err}");
        let err = expand_str(r"\renewcommand{\y}{a}", &set).unwrap_err();
        assert!(err.to_string().contains("not defined"), "{err}");
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
        b.define("half", 1, r"\frac{#1}{2}").unwrap();
        b.define("dd", 0, r"\mathrm{d}").unwrap();
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
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

    #[test]
    fn nested_parameterized_macros_receive_substituted_arguments() {
        let mut set = MacroSet::new();
        set.define("ratio", 2, r"\frac{#1}{#2}").unwrap();
        set.define("inverse", 2, r"\ratio{#2}{#1}").unwrap();
        set.define("twice", 1, r"\inverse{2}{#1}+\inverse{2}{#1}")
            .unwrap();
        assert_eq!(
            expand_str(r"\twice{x+y}", &set).unwrap(),
            r"\frac {x+y}{2}+\frac {x+y}{2}",
        );
    }

    #[test]
    fn parameter_slots_are_not_nested_argument_boundaries() {
        let mut set = MacroSet::new();
        set.define("pair", 2, "#1+#2").unwrap();
        set.define("apply", 2, "#1{#2}").unwrap();
        assert_eq!(expand_str(r"\apply{\pair{x}}{y}", &set).unwrap(), "x+y");
        set.define("identity", 1, "#1").unwrap();
        assert_eq!(expand_str(r"\apply{\identity}{x}", &set).unwrap(), "x");
    }

    #[test]
    fn nested_expansion_preserves_literal_spans_and_rebases_generated_tokens() {
        let mut set = MacroSet::new();
        set.define("ratio", 2, r"\frac{#1}{#2}").unwrap();
        set.define("half", 1, r"\ratio{#1}{2}").unwrap();
        let src = r"a+\half{中}";
        let call_start = src.find(r"\half").unwrap();
        let tokens = expand(lex(src), &set, src.len()).unwrap();
        for token in &tokens {
            assert!(token.span.start <= token.span.end && token.span.end <= src.len());
            match token.kind {
                TokKind::Char('中') => assert_eq!(&src[token.span.start..token.span.end], "中"),
                TokKind::Char('2') | TokKind::ControlWord("frac") => {
                    assert_eq!((token.span.start, token.span.end), (call_start, src.len()));
                }
                _ => {}
            }
        }
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokKind::Char('中')))
        );
    }

    #[test]
    fn empty_output_fanout_is_bounded_too() {
        let mut set = MacroSet::new();
        set.define("a", 0, "").unwrap();
        for (previous, name) in [
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
            set.define(name, 0, &format!("\\{previous}\\{previous}"))
                .unwrap();
        }
        let error = expand_str(r"\r", &set).unwrap_err();
        assert!(error.to_string().contains("token work units"), "{error}");
    }

    #[test]
    fn discarded_intermediate_replacements_still_consume_budget() {
        let mut set = MacroSet::new();
        set.define("discard", 1, "").unwrap();
        set.define("large", 1, r"\discard{#1#1#1#1#1#1#1#1#1}")
            .unwrap();
        let source = format!("\\large{{{}}}", "x".repeat(8192));
        let error = expand_str(&source, &set).unwrap_err();
        assert!(error.to_string().contains("token work units"), "{error}");
    }

    #[test]
    fn composed_recursive_arguments_remain_rejected() {
        let mut set = MacroSet::new();
        set.define("identity", 1, "#1").unwrap();
        let error = expand_str(r"\identity{\identity{x}}", &set).unwrap_err();
        assert!(error.to_string().contains("recursive macro"), "{error}");
    }

    #[test]
    fn missing_argument_cannot_consume_a_closing_group() {
        let mut set = MacroSet::new();
        set.define("identity", 1, "#1").unwrap();
        let error = expand_str(r"{\identity}", &set).unwrap_err();
        assert!(error.to_string().contains("missing argument #1"), "{error}");
    }

    #[test]
    fn optional_defaults_and_explicit_overrides_compose_with_required_arguments() {
        let set = MacroSet::new();
        let source = r"\newcommand{\power}[2][2]{#2^{#1}}\power{x}+\power[3]{y}";
        assert_eq!(expand_str(source, &set).unwrap(), "x^{2}+y^{3}");
    }

    #[test]
    fn explicit_empty_optional_argument_does_not_select_the_default() {
        let set = MacroSet::new();
        let source = r"\newcommand{\join}[2][d]{#1#2}\join[]{x}+\join{y}";
        assert_eq!(expand_str(source, &set).unwrap(), "x+dy");
        let source = r"\newcommand{\empty}[1][]{#1}\empty+\empty[z]";
        assert_eq!(expand_str(source, &set).unwrap(), "+z");
    }

    #[test]
    fn optional_brackets_respect_braced_and_escaped_closers() {
        let set = MacroSet::new();
        assert_eq!(
            expand_str(r"\newcommand{\pick}[1][{]}]{#1}\pick", &set).unwrap(),
            "{]}",
        );
        assert_eq!(
            expand_str(r"\newcommand{\pick}[1][x]{#1}\pick[{]}]", &set).unwrap(),
            "{]}",
        );
        assert_eq!(
            expand_str(r"\newcommand{\pick}[1][\]]{#1}\pick", &set).unwrap(),
            r"\]",
        );
    }

    #[test]
    fn defaults_may_invoke_macros_and_nested_calls_may_override_them() {
        let set = MacroSet::new();
        let source = concat!(
            r"\newcommand{\denom}{2}",
            r"\newcommand{\ratio}[2][\denom]{\frac{#2}{#1}}",
            r"\newcommand{\third}[1]{\ratio[3]{#1}}",
            r"\ratio{x}+\third{y}",
        );
        assert_eq!(
            expand_str(source, &set).unwrap(),
            r"\frac {x}{2}+\frac {y}{3}"
        );
    }

    #[test]
    fn renewcommand_replaces_and_can_remove_optional_defaults() {
        let set = MacroSet::new();
        let source = concat!(
            r"\newcommand{\pick}[1][a]{#1}\pick+",
            r"\renewcommand{\pick}[1][b]{#1}\pick+",
            r"\renewcommand{\pick}[1]{#1}\pick{c}",
        );
        assert_eq!(expand_str(source, &set).unwrap(), "a+b+c");
    }

    #[test]
    fn default_tokens_take_call_spans_but_explicit_optional_tokens_keep_their_source() {
        let set = MacroSet::new();
        let prefix = r"\newcommand{\pick}[1][z]{#1}";
        let source = format!("{prefix}\\pick");
        let tokens = expand(lex(&source), &set, source.len()).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            (tokens[0].span.start, tokens[0].span.end),
            (prefix.len(), source.len())
        );
        let source = format!("{prefix}\\pick[中]");
        let tokens = expand(lex(&source), &set, source.len()).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(&source[tokens[0].span.start..tokens[0].span.end], "中");
    }

    #[test]
    fn malformed_optional_arguments_fail_without_stealing_outer_delimiters() {
        let set = MacroSet::new();
        for (source, message) in [
            (r"\newcommand{\pick}[1][abc", "unclosed optional argument"),
            (r"\newcommand{\pick}[1][#1]{#1}", "uses #1 but declares 0"),
            (
                r"\newcommand{\pick}[1][x]{#1}\pick[a",
                "unclosed optional argument",
            ),
            (
                r"\newcommand{\pick}[1][x]{#1}{\pick[a}",
                "surrounding group",
            ),
            (
                r"\newcommand{\pick}[2][x]{#2}\pick[y]",
                "input ends before #2",
            ),
        ] {
            let error = expand_str(source, &set).unwrap_err();
            assert!(error.to_string().contains(message), "{source}: {error}");
        }
    }

    #[test]
    fn optional_defaults_do_not_bypass_recursion_or_work_limits() {
        let set = MacroSet::new();
        let error =
            expand_str(r"\newcommand{\selfref}[1][\selfref]{#1}\selfref", &set).unwrap_err();
        assert!(error.to_string().contains("recursive macro"), "{error}");
        let source = format!(
            "\\newcommand{{\\large}}[1][{}]{{#1#1#1#1#1#1#1#1#1}}\\large",
            "x".repeat(8192),
        );
        let error = expand_str(&source, &set).unwrap_err();
        assert!(error.to_string().contains("token work units"), "{error}");
    }

    #[test]
    fn empty_parameter_substitution_still_charges_body_traversal() {
        let mut set = MacroSet::new();
        set.define("erase", 1, &"#1".repeat(1024)).unwrap();
        let source = r"\erase{}".repeat(100);
        let error = expand_str(&source, &set).unwrap_err();
        assert!(error.to_string().contains("token work units"), "{error}");
    }

    #[test]
    fn optional_macros_reach_the_real_math_parser_and_mathml_renderer() {
        let source = concat!(
            r"\newcommand{\ratio}[2][2]{\frac{#2}{#1}}",
            r"\newcommand{\third}[1]{\ratio[3]{#1}}",
            r"\ratio{x}+\third{y}",
        );
        let actual = crate::parse(source).unwrap();
        let expected = crate::parse(r"\frac{x}{2}+\frac{y}{3}").unwrap();
        for display in [false, true] {
            let xml = crate::to_mathml(&actual, display);
            assert_eq!(xml, crate::to_mathml(&expected, display));
            crate::mathml_well_formed(&xml).unwrap();
        }
    }
}
