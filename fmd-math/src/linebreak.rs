//! TeX's paragraph line breaker (tex.web §813–§890), for TexText
//! paragraphs wider than their line (franken_manim
//! fm-texttext-line-breaking-r11-bfdx).
//!
//! The breaker is the Knuth–Plass algorithm as TeX runs it: integer scaled
//! points, the §108 badness approximation, four fitness classes, the
//! active list kept in TeX's order with its tie rule (a later active node
//! wins a tie at each feasible break, the first node wins at the paragraph's
//! end), and TeX's three passes (`\pretolerance`, `\tolerance`, the
//! emergency pass whose last active node is forced through an overfull
//! line). Its input is the list of legal breakpoints with the paragraph's
//! running totals, which is what TeX's delta nodes encode; prefix totals in
//! integers give the same widths exactly.
//!
//! Not modelled: `\parshape`, `\hangindent` and `\looseness` (the
//! Reference's paragraphs set none of them, so every line is `\hsize`
//! wide), and hyphenation: TeX hyphenates only in its second pass, which a
//! paragraph reaches only when the first finds no feasible breaks.

/// One scaled point is 2^-16 pt; an em is 10 pt at fmd-math's design size.
const SP_PER_EM: f64 = 655_360.0;

/// TeX's infinitely bad (§108).
const INF_BAD: i64 = 10_000;
/// TeX's `awful_bad`, larger than any real total of demerits (§833).
const AWFUL_BAD: i64 = 0o7_777_777_777;
/// A penalty at or below this forces a break (`eject_penalty`, §157).
pub(crate) const EJECT_PENALTY: i64 = -10_000;
/// A penalty at or above this forbids a break (`inf_penalty`, §157).
pub(crate) const INF_PENALTY: i64 = 10_000;

/// Ems to scaled points, as TeX would read the dimension.
pub(crate) fn sp(em: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let scaled = (em * SP_PER_EM).round() as i64;
    scaled
}

/// The paragraph parameters in force when a paragraph ends.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Params {
    /// `\hsize`, every line's width, sp.
    pub(crate) line_width: i64,
    /// Whether `\leftskip` or `\rightskip` stretch by `fil`, as under
    /// `\centering`, `\raggedright` and `\raggedleft`: every line then
    /// stretches with badness 0.
    pub(crate) fil_background: bool,
    pub(crate) line_penalty: i64,
    pub(crate) adj_demerits: i64,
    pub(crate) double_hyphen_demerits: i64,
    pub(crate) final_hyphen_demerits: i64,
    pub(crate) pretolerance: i64,
    pub(crate) tolerance: i64,
    /// `\emergencystretch`, sp.
    pub(crate) emergency_stretch: i64,
}

impl Params {
    /// The Reference's TexText paragraph: `\documentclass[preview]{standalone}`
    /// under its default template (TeX Live 2025, read with `\the`):
    /// `\linepenalty` 10, `\adjdemerits` and `\doublehyphendemerits`
    /// 10000, `\pretolerance` 100, and the `\sloppy` settings the class
    /// leaves, `\tolerance` 9999 and `\emergencystretch` 3 em. `\centering`
    /// sets `\finalhyphendemerits` to 0; the other ragged declarations keep
    /// LaTeX's 5000.
    pub(crate) fn tex_text(line_width_em: f64, centred: bool) -> Self {
        Self {
            line_width: sp(line_width_em),
            fil_background: true,
            line_penalty: 10,
            adj_demerits: 10_000,
            double_hyphen_demerits: 10_000,
            final_hyphen_demerits: if centred { 0 } else { 5_000 },
            pretolerance: 100,
            tolerance: 9_999,
            emergency_stretch: sp(3.0),
        }
    }
}

/// Running natural width, stretch and shrink, sp. Stretch is finite only:
/// a paragraph's own glue is interword and `\hskip` glue, and TeX turns
/// infinite shrink into an error (§826).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    pub(crate) width: i64,
    pub(crate) stretch: i64,
    pub(crate) shrink: i64,
}

impl Totals {
    fn minus(self, other: Self) -> Self {
        Self {
            width: self.width - other.width,
            stretch: self.stretch - other.stretch,
            shrink: self.shrink - other.shrink,
        }
    }
}

/// A legal breakpoint.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Breakpoint {
    /// The totals of everything before the break.
    pub(crate) before: Totals,
    /// The totals where the next line begins: the discardable glue after
    /// the break is gone (§837).
    pub(crate) after: Totals,
    /// The break's penalty: 0 at glue.
    pub(crate) penalty: i64,
    /// A hyphenated (discretionary) break.
    pub(crate) flagged: bool,
}

/// TeX's badness of stretching or shrinking by `t` against a total
/// allowance `s` (§108): about 100 (t/s)^3, capped at 10000.
fn badness(t: i64, s: i64) -> i64 {
    if t == 0 {
        return 0;
    }
    if s <= 0 {
        return INF_BAD;
    }
    let r = if t <= 7_230_584 {
        (t * 297) / s
    } else if s >= 1_663_497 {
        t / (s / 297)
    } else {
        t
    };
    if r > 1290 {
        INF_BAD
    } else {
        (r * r * r + 0o400_000) / 0o1_000_000
    }
}

const VERY_LOOSE: usize = 0;
const LOOSE: usize = 1;
const DECENT: usize = 2;
const TIGHT: usize = 3;

struct Active {
    fitness: usize,
    /// The passive node of this break; `None` is the paragraph's start.
    passive: Option<usize>,
    total_demerits: i64,
    flagged: bool,
    /// The totals where a line from this break begins.
    start: Totals,
}

struct Passive {
    /// Index into the breakpoints.
    at: usize,
    prev: Option<usize>,
}

/// The breaks TeX chooses, as indices into `breaks`, in order. The
/// paragraph ends at `end`, the totals of its whole content (its last glue
/// already removed, as `\par` does, §816). Empty when the paragraph is one
/// line.
pub(crate) fn break_lines(breaks: &[Breakpoint], end: Totals, params: &Params) -> Vec<usize> {
    let mut passes = Vec::with_capacity(3);
    if params.pretolerance >= 0 {
        passes.push((params.pretolerance, false, 0));
    }
    passes.push((params.tolerance, params.emergency_stretch <= 0, 0));
    if params.emergency_stretch > 0 {
        passes.push((params.tolerance, true, params.emergency_stretch));
    }
    for (threshold, final_pass, extra_stretch) in passes {
        let pass = Pass {
            params,
            threshold,
            final_pass,
            background_stretch: extra_stretch,
        };
        if let Some(chosen) = pass.run(breaks, end) {
            return chosen;
        }
    }
    // The emergency pass always finishes: its artificial demerits keep one
    // active node alive (§854). Reaching here means no pass ran at all.
    Vec::new()
}

struct Pass<'a> {
    params: &'a Params,
    threshold: i64,
    final_pass: bool,
    /// `\emergencystretch`, added to the background in TeX's last pass.
    background_stretch: i64,
}

impl Pass<'_> {
    fn run(&self, breaks: &[Breakpoint], end: Totals) -> Option<Vec<usize>> {
        let mut active = vec![Active {
            fitness: DECENT,
            passive: None,
            total_demerits: 0,
            flagged: false,
            start: Totals::default(),
        }];
        let mut passive: Vec<Passive> = Vec::new();
        for (index, point) in breaks.iter().enumerate() {
            if active.is_empty() {
                return None;
            }
            self.try_break(&mut active, &mut passive, *point, Some(index));
        }
        if active.is_empty() {
            return None;
        }
        // §873: the final break, an eject at the paragraph's end, typed
        // hyphenated so that a hyphenated last-but-one line pays
        // \finalhyphendemerits.
        let last = Breakpoint {
            before: end,
            after: end,
            penalty: EJECT_PENALTY,
            flagged: true,
        };
        self.try_break(&mut active, &mut passive, last, None);
        // §874: the first active node with the fewest demerits.
        let mut best: Option<&Active> = None;
        for node in &active {
            if best.is_none_or(|b| node.total_demerits < b.total_demerits) {
                best = Some(node);
            }
        }
        let best = best?;
        // The final node's passive record is the paragraph's end; its chain
        // of predecessors is the chosen breaks, last first.
        let mut chosen = Vec::new();
        let mut cursor = best.passive.and_then(|p| passive[p].prev);
        while let Some(p) = cursor {
            chosen.push(passive[p].at);
            cursor = passive[p].prev;
        }
        chosen.reverse();
        Some(chosen)
    }

    /// TeX's `try_break` (§829) at one breakpoint; `at` is `None` for the
    /// paragraph's end.
    fn try_break(
        &self,
        active: &mut Vec<Active>,
        passive: &mut Vec<Passive>,
        point: Breakpoint,
        at: Option<usize>,
    ) {
        let params = self.params;
        // §831: an infinite penalty forbids the break; a very negative one
        // forces it.
        let mut pi = point.penalty;
        if pi >= INF_PENALTY {
            return;
        }
        if pi <= EJECT_PENALTY {
            pi = EJECT_PENALTY;
        }
        // §833: per fitness class, the fewest demerits of a feasible break
        // here and the break it follows (`best_place`, the predecessor's
        // passive record; `None` is the paragraph's start).
        let mut minimal = [AWFUL_BAD; 4];
        let mut best_place: [Option<usize>; 4] = [None; 4];
        let mut minimum_demerits = AWFUL_BAD;
        let mut index = 0;
        while index < active.len() {
            let r = &active[index];
            let line = point.before.minus(r.start);
            let shortfall = params.line_width - line.width;
            let (b, fit_class) = if shortfall > 0 {
                // §852: infinite stretch (the ragged skips) leaves every
                // line with badness 0.
                if params.fil_background {
                    (0, DECENT)
                } else {
                    let stretch = line.stretch + self.background_stretch;
                    if shortfall > 7_230_584 && stretch < 1_663_497 {
                        (INF_BAD, VERY_LOOSE)
                    } else {
                        let b = badness(shortfall, stretch);
                        let class = if b > 99 {
                            VERY_LOOSE
                        } else if b > 12 {
                            LOOSE
                        } else {
                            DECENT
                        };
                        (b, class)
                    }
                }
            } else {
                // §853: shrinking past the total shrink is overfull.
                let b = if -shortfall > line.shrink {
                    INF_BAD + 1
                } else {
                    badness(-shortfall, line.shrink)
                };
                (b, if b > 12 { TIGHT } else { DECENT })
            };
            let mut artificial = false;
            let stays_active = if b > INF_BAD || pi == EJECT_PENALTY {
                // §854: the emergency pass forces the paragraph's only
                // remaining node through an overfull line.
                if self.final_pass && minimum_demerits == AWFUL_BAD && active.len() == 1 {
                    artificial = true;
                } else if b > self.threshold {
                    active.remove(index);
                    continue;
                }
                false
            } else {
                if b > self.threshold {
                    index += 1;
                    continue;
                }
                true
            };
            // §855: record a feasible break. `<=`: on equal demerits the
            // later active node wins.
            let d = if artificial {
                0
            } else {
                self.demerits(b, pi, fit_class, r, point.flagged, at.is_none())
            } + r.total_demerits;
            if d <= minimal[fit_class] {
                minimal[fit_class] = d;
                best_place[fit_class] = r.passive;
                minimum_demerits = minimum_demerits.min(d);
            }
            if stays_active {
                index += 1;
            } else {
                active.remove(index);
            }
        }
        if minimum_demerits >= AWFUL_BAD {
            return;
        }
        // §836: new active nodes, after every older one, for the best
        // feasible breaks of each fitness class within \adjdemerits of the
        // best.
        let limit = if params.adj_demerits.abs() >= AWFUL_BAD - minimum_demerits {
            AWFUL_BAD - 1
        } else {
            minimum_demerits + params.adj_demerits.abs()
        };
        for (fit_class, demerits) in minimal.into_iter().enumerate() {
            if demerits > limit {
                continue;
            }
            passive.push(Passive {
                at: at.unwrap_or(usize::MAX),
                prev: best_place[fit_class],
            });
            active.push(Active {
                fitness: fit_class,
                passive: Some(passive.len() - 1),
                total_demerits: demerits,
                flagged: point.flagged,
                start: point.after,
            });
        }
    }

    /// §859: the demerits of a line from `r` ending at a break with
    /// badness `b`, penalty `pi` and fitness `fit_class`.
    fn demerits(
        &self,
        b: i64,
        pi: i64,
        fit_class: usize,
        r: &Active,
        flagged: bool,
        paragraph_end: bool,
    ) -> i64 {
        let params = self.params;
        let base = params.line_penalty + b;
        let mut d = if base.abs() >= 10_000 {
            100_000_000
        } else {
            base * base
        };
        if pi != 0 {
            if pi > 0 {
                d += pi * pi;
            } else if pi > EJECT_PENALTY {
                d -= pi * pi;
            }
        }
        if flagged && r.flagged {
            d += if paragraph_end {
                params.final_hyphen_demerits
            } else {
                params.double_hyphen_demerits
            };
        }
        if fit_class.abs_diff(r.fitness) > 1 {
            d += params.adj_demerits;
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Words of the given widths separated by rigid or shrinkable glue,
    /// as breakpoints at each glue.
    fn paragraph(words: &[i64], glue: Totals) -> (Vec<Breakpoint>, Totals) {
        let mut at = Totals::default();
        let mut breaks = Vec::new();
        for (i, w) in words.iter().enumerate() {
            at.width += w;
            if i + 1 < words.len() {
                let before = at;
                at.width += glue.width;
                at.stretch += glue.stretch;
                at.shrink += glue.shrink;
                breaks.push(Breakpoint {
                    before,
                    after: at,
                    penalty: 0,
                    flagged: false,
                });
            }
        }
        (breaks, at)
    }

    fn ragged(line_width: i64) -> Params {
        Params {
            line_width,
            ..Params::tex_text(34.5, true)
        }
    }

    #[test]
    fn badness_is_texs_cube_approximation() {
        assert_eq!(badness(0, 0), 0);
        assert_eq!(badness(5, 0), INF_BAD);
        // t = s is badness 100; t = s/2 rounds 12.87 down to 12.
        assert_eq!(badness(65_536, 65_536), 100);
        assert_eq!(badness(32_768, 65_536), 12);
        assert_eq!(badness(10 * 65_536, 65_536), INF_BAD);
    }

    #[test]
    fn ragged_ties_go_to_the_latest_break() {
        // Two lines either way: 2 + 3 or 3 + 2 words. Every ragged line has
        // badness 0, so the demerits tie, and TeX's `d <= minimal` keeps
        // the later predecessor: the first line is filled.
        let rigid = Totals {
            width: 10,
            ..Totals::default()
        };
        let (breaks, end) = paragraph(&[100; 5], rigid);
        assert_eq!(break_lines(&breaks, end, &ragged(330)), vec![2]);
    }

    #[test]
    fn a_slightly_shrunk_line_beats_an_extra_line() {
        // Four words fit one line only by shrinking 10 of 30: badness 4,
        // demerits 196, against 2 x 100 for two lines.
        let space = Totals {
            width: 10,
            stretch: 5,
            shrink: 10,
        };
        let (breaks, end) = paragraph(&[100; 4], space);
        assert!(break_lines(&breaks, end, &ragged(420)).is_empty());
        // Shrinking 20 of 30 (badness 30) costs more than a second line.
        assert_eq!(break_lines(&breaks, end, &ragged(410)), vec![2]);
    }

    #[test]
    fn a_paragraph_that_fits_is_one_line() {
        let (breaks, end) = paragraph(&[100; 3], Totals::default());
        assert!(break_lines(&breaks, end, &ragged(1_000)).is_empty());
    }

    #[test]
    fn an_overlong_word_still_breaks_in_the_emergency_pass() {
        // The middle word is wider than the line: no pass finds a feasible
        // line through it, and the emergency pass sets it overfull alone.
        let rigid = Totals {
            width: 10,
            ..Totals::default()
        };
        let (breaks, end) = paragraph(&[100, 500, 100], rigid);
        assert_eq!(break_lines(&breaks, end, &ragged(300)), vec![0, 1]);
    }

    #[test]
    fn infinite_penalties_forbid_and_ejects_force() {
        let rigid = Totals {
            width: 10,
            ..Totals::default()
        };
        let (mut breaks, end) = paragraph(&[100; 5], rigid);
        breaks[2].penalty = INF_PENALTY;
        // The filled first line would end at the forbidden break.
        assert_eq!(break_lines(&breaks, end, &ragged(330)), vec![1]);
        breaks[2].penalty = 0;
        breaks[0].penalty = EJECT_PENALTY;
        assert_eq!(break_lines(&breaks, end, &ragged(330)), vec![0, 3]);
    }
}
