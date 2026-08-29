# Microtypography cost hooks (bead qw1.7.5)

`franken_markdown` ships **fixed-point microtypography cost hooks** in
`src/layout.rs`. They are computational primitives — the deltas a renderer can
opt into — and are **off by default** (`MicrotypeOptions::DISABLED`), so default
HTML and PDF output is unchanged.

## Hooks

| Hook | What it computes |
|---|---|
| `MicrotypeOptions { protrusion, max_expansion_per_mille }` | Policy; `DISABLED` (default) and `CONSERVATIVE` presets |
| `protrusion_for_text(text, size, opts) -> Protrusion` | Left/right optical-margin protrusion of a run's boundary characters |
| `protruded_fit_width(natural, text, size, opts) -> LayoutUnit` | The width to fit against once boundary punctuation is allowed to hang into the margin |
| `expansion_budget(line_width, opts) -> LayoutUnit` | Per-line font expansion/contraction budget |

The optical-margin table (right-edge per-mille): `. ,` = 550, `: ;` = 420,
`! ?` = 250, quotes = 350, brackets = 120, hyphens = 80.

## Determinism

Every hook uses **integer / fixed-point math only** — per-mille tables times
`milli_points`, accumulated in `i128`/`u128` and clamped to `i32`. No floating
point enters a layout comparison, so decisions are byte-stable across runs and
platforms (`tests/layout_test.rs::microtype_protrusion_and_expansion_are_integer_deterministic`,
`microtype_protrusion_table_is_stable`).

## Intended effect (demonstrated)

`microtype_protrusion_changes_a_line_fit_decision_deterministically` pins the
intended decision change: a line 2 pt over the column does not fit with
protrusion disabled, but its trailing period protrudes 550‰ × 10 pt = 5_500
milli-points and the line then fits — an exact, deterministic delta.

## Status: wired behind `--microtype protrusion` (opt-in since 0.4.2)

The hooks are complete, tested, and now wired end-to-end for optical-margin
protrusion in justified PDF body paragraphs: `TextBox.protrusion` is
precomputed at box construction (font size known there), the Knuth-Plass
breaker fits against protrusion-adjusted line widths (O(1) edge lookups in
`MetricPrefixes`), the whole-paragraph fast path honors the same credit, and
the emitter shifts line starts left by the credited protrusion. Default renders
are byte-identical (tests/microtype_test.rs pins all four behaviors).
Expansion (font stretch) shipped as the `Tz` emitter (Unreleased): the justifier
already credits word boxes ±15‰ glyph elasticity (`glue_adjustments_into`), and
`build_segs_adjusted` now converts that credit into a uniform per-line `Tz`
operator (exact because the credit distribution is proportional to box width)
instead of flat letter-spacing. `--microtype expansion` enables glyph scaling
alone; `--microtype protrusion` enables both effects. Related quality lever:
`--typography-homogeneous` (Verna DocEng '25 gradual adjacent demerits) refines
the KP fitness-class penalty for smoother inter-word spacing.

The hooks are complete, tested, and conservative-by-default. Enabling them inside
the optimal line breaker is intentionally **not** done by default, per the
`tests/artifacts/perf/qw1.7-reprofile/DECISION.md` gate (microtypography is a
*quality* feature that *adds* cost; line breaking is rank 4 / 7.5 ms and not
first-order). The breaker is deliberately size-agnostic (it carries box widths,
not font sizes), so wiring protrusion through it requires precomputing per-box
protrusion at box construction (where the font size is known) and storing it on
`TextBox` — a broad, opt-in change to be made when a quality pass justifies
enabling microtypography by default. The design and deltas above are the
contract that wiring will honor.

## Adjacent techniques (Unreleased)

- **River-seed demerits** (`--typography-antiriver`, opt-in): the KP inner
  loop penalizes a candidate whose previous line's last drawn inter-word
  space aligns (within 1% of the measure, natural-width prefix sums) with a
  space in the candidate line — the two-line seed of a whitespace river.
  Flat cost `1_000` per seeded edge; applies to ragged flows too.
  `tests/river_penalty_test.rs` pins: off = classic byte-identical, on never
  increases detected seeds across a fixed corpus, on is live.
- **Plass optimal pagination** (`--pdf-optimal-pagination`, opt-in): a
  document-wide DP over page partitions minimizing the total of the same
  void-badness + keep-penalty cost the greedy breaker applies per page
  (Plass & Li, 1981). Forced boundaries stay hard constraints; the final
  page stays unpenalized; break penalties are precomputed per index so each
  edge is O(1). `plass_pagination_tests` (in src/pdf.rs) pin legality,
  total-cost dominance over greedy, and a crafted myopia fixture where the
  DP strictly wins by trading void on one page for a kept-together block on
  the next.
- **SMAWK-accelerated line breaking — evaluated, deliberately NOT
  implemented.** SMAWK's O(n) total-fit DP requires the edge-cost matrix to
  be totally monotone (equivalently, the cost satisfies the concave
  quadrangle inequality). Three of our cost terms violate that premise by
  construction: (1) the flagged-hyphenation interaction
  `10_000·flag(i)·flag(j)` has non-monotone 0/1 flags (hyphen points
  alternate), so cross-differences take both signs; (2) overfull lines pay a
  deliberately *graded finite* `1e9 + overflow-scaled` cost so overflowing
  tokens self-isolate — a non-monotone ∞-substitute that breaks the
  feasibility-window nesting SMAWK relies on; (3) fitness-class adjacency
  makes the true objective chain-dependent (cost of edge (i,j) depends on
  the fitness of the line ending at i), and the class-stratified parallel-DP
  reformulation still inherits (1) and (2). Implementing SMAWK would either
  change break decisions (violating byte-determinism) or rest on an invalid
  monotonicity proof. The line DP is also not on the critical path (rank 4 /
  7.5 ms per the perf gate), so the speedup would buy nothing user-visible.

## Multi-objective breaking + constrained tables (Unreleased)

- **Pareto line breaking** (`--typography-pareto`, opt-in): the KP search
  keeps bounded per-candidate fronts of non-dominated states over two demerit
  dimensions — structure (badness², fitness-class + gradual spacing, rivers,
  overflow) and hyphenation (break penalties, flagged-flag adjacency) —
  instead of one scalar winner. The final pick stays min-scalar, so the
  scalar total can never exceed the classic path's; the sweep in
  `tests/pareto_test.rs` (280+ hyphenation-bearing fixtures) confirms
  dominance everywhere and measures where the fronts actually change the
  chosen breaks. Measured result on this cost model: the classic scalar DP is
  already optimal on most real fixtures — pareto is a strict safety net for
  class-mismatch cases, not a change agent.
- **Constrained table layout (Marx & Stuckey, practical core)**: the column
  allocator (`allocate_table_column_widths`) is a min-plus DP over the
  discretized extra-width grid minimizing total height-sensitive wrap badness
  subject to the hard constraints (sum == budget, per-column minimum floor).
  `table_alloc_optimality_tests` (in src/pdf.rs) prove the DP's output
  matches EXHAUSTIVE enumeration of every legal grid allocation (with the
  identical sub-unit finish pass) on synthetic multi-column fixtures — the
  optimality certificate for the shipped algorithm. Full M&S profile
  automata for max-based row heights remain future work; the shipped
  surrogate (sum of per-column extra-line² penalties) is the standard
  practical stand-in.
