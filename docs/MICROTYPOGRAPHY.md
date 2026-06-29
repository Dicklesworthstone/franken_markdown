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

## Status: hooks done; default-render wiring is gated

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
