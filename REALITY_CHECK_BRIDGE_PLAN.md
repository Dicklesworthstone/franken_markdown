# Reality-Check Bridge Plan

Date: 2026-06-28
Status: active — second full reality check (first was 2026-06-27, which produced beads `mwm.1`–`mwm.5`)
Scope: close every gap between the implemented code and the README/AGENTS/plan vision.

> This document is the iterate-in-place plan for the 2026-06-28 reality check. It
> is revised in-place across ambition rounds; it is NOT re-created per round.
> Beads remain the source of truth — every gap below maps to an existing bead or
> a new bead created from this plan.

## 0. Verified ground truth (2026-06-28)

The core engine genuinely delivers. Verified by running the software, not by
trusting tests:

- `cargo test`: 266 pass / 0 fail across 24 suites.
- Gate scripts green: `check-policy.sh`, `check-determinism.sh`,
  `check-wasm-core.sh`, `parser-diff.sh`.
- HTML render: 146 KB self-contained, inlined CSS, real subsetted `@font-face`.
- PDF render: 56 KB valid `%PDF-1.7`, byte-identical across runs; structural
  inspection confirms **5 embedded `FontFile2` TrueType subsets**,
  `Type0`/`CIDFontType2`/`Identity-H`, `ToUnicode`, `/W` arrays, link
  annotations.
- Clean-room boundary holds: zero third-party deps in the `--no-default-features`
  core; `unsafe_code = "forbid"`.

The bead graph is already comprehensive: 95/127 closed, 0 in-progress, 32 open,
and the open beads already cover WASM packaging, conformance, perf/SIMD, batch,
release, and the visual gauntlet. **This plan is therefore surgical**: it fills
the few genuine NO_BEAD holes and hardens weak acceptance criteria, rather than
duplicating tracked work.

## 1. Gap register (gap → status → owning bead)

| # | Gap | Reality-check finding | Owning bead | Action |
|---|-----|----------------------|-------------|--------|
| G1 | WASM browser package is a non-functional skeleton | No built `.wasm`; `wasm/franken_markdown.js` imports a missing `./pkg/`; demo can't load; `wasm_package_test.rs` Group A string-matches source | `3i5` (+ closed `3i5.2`/`3i5.4` on skeleton), open `3i5.5` | NEW build-artifact CI gate bead; HARDEN `3i5.5` acceptance |
| G2 | Full CommonMark/GFM conformance is unmeasured (~75–80% est.) | Only hand-written fixtures; no official spec suite; 12/250+ named entities; HTML blocks partial | `mwm.3` (strong) | REFINE: numeric target + surface measured % in capabilities/README |
| G3 | Deep pagination partial | Soft widow/orphan + heading/table-header keep only; no keep-with-next/footnotes/columns | closed `49d.1`; README honest | NEW follow-up bead for keep-with-next + footnote design (was punted) |
| G4 | SIMD acceleration not implemented | Design docs only; zero SIMD code (correctly gated on profiling) | `qw1.5.*` | No change — gate is correct |
| G5 | Asupersync batch mode absent | Not in `Cargo.toml`; no batch command | `zmd`, `zmd.1.*` | No change — full sub-tree exists |
| G6 | "One theme model" violated: PDF hardcodes colors | `zod` closed but explicitly "Avoided pdf lane"; `src/pdf.rs:48-68` hardcodes link/code/quote/table RGB; no dark mode in PDF | **NONE (NO_BEAD)** | NEW bead + companion test bead |
| G7 | No release/installer | No published artifacts | `08f` | No change |
| G8 | Visual/raster goldens early | Structural + determinism only; no raster comparison | `qw1`, `qw1.1` | REFINE: require deterministic raster/render-tree goldens |
| G9 | `--to pdf` w/o `--out` writes `document.pdf` | Contradicts README troubleshooting ("requires --out"); created stray repo-root files | partial `mwm.2` (dash handling, closed) | NEW tiny bead: reconcile default-output behavior vs docs |
| G10 | "first-class WASM" claim outruns artifacts | README/marketing assert shipped WASM; package is skeleton | `mwm.5` release gate | REFINE `mwm.5` + NEW `N5` claim-discipline CI gate; block on G1/N2 |

## 2. New beads to create (genuine NO_BEAD gaps)

### N1 — PDF: consume shared theme colors (unify the one-theme-model doctrine)
- **Why:** AGENTS.md doctrine: "HTML and PDF must share one parsed AST and one
  theme model so visual output stays coherent." `zod` built the theme tokens and
  wired HTML, but explicitly punted the PDF lane; PDF still hardcodes
  `LINK_COLOR`, `PANEL_GRAY`, quote/table colors in `src/pdf.rs:48-68`.
- **Scope:** Route PDF link/code-chip/blockquote/table-rule/heading-rule/zebra
  colors through `Theme`/`ThemeColors`. Decide and document PDF dark-mode policy
  (either honor `dark_mode` for PDF, or record it as an intentional light-only
  non-goal with rationale).
- **Acceptance:** PDF colors derive from theme tokens; changing a theme token
  changes both HTML and PDF; determinism + size budgets unchanged.
- **Tests (companion bead N1-T):** structural test asserts PDF color operators
  match theme tokens for a known theme; a second theme yields different,
  still-deterministic bytes.

### N2 — WASM: real built-artifact CI gate + headless execution proof
- **Why:** "first-class WASM" requires a *runnable* artifact. Today the JS
  package and demo were closed on a hand-written skeleton; `check-wasm-package.sh`
  exists but is NOT in CI; no test loads a real module in a browser/wasm runtime.
- **Scope:** Wire `scripts/check-wasm-package.sh` (build `wasm-bindgen` → emit
  `pkg/` → assemble) into CI; add a headless smoke test (node/`wasm-pack`-style
  runner or a wasm32 test harness) that imports the *generated* module and calls
  `renderHtml`/`renderPdf`/`capabilities`, asserting real output bytes.
- **Acceptance:** CI fails if the package can't build or the headless smoke test
  can't render; the emitted `.wasm` size is measured against a budget.
- **Explicit anti-pattern:** string-match-only tests over source files
  (`wasm_package_test.rs` Group A) MUST NOT count as proof of "first-class WASM";
  they are demoted to "source-shape lint."
- **Tests (companion bead N2-T):** headless render of a fixture Markdown to HTML
  and PDF, byte-compared (or structure-compared) to the native render of the same
  input + options.

### N3 — PDF: keep-with-next + footnote pagination design (deep-pagination tail)
- **Why:** `49d.1` landed the vertical page builder with soft penalties; the
  vision lists keep-with-next and footnote handling, which were punted.
- **Scope:** Design (and, if cheap, implement) explicit keep-with-next block
  association and footnote placement, with deterministic fallback. Mark column
  layout as an explicit non-goal unless demanded.
- **Acceptance:** fixtures where a heading never strands from its following block;
  footnote markers resolve deterministically; no regression in existing goldens.

### N4 — CLI/docs: reconcile `--to pdf` default-output behavior
- **Why:** Running `fmd --to pdf --text '# x'` (no `--out`) silently writes
  `document.pdf`; README troubleshooting says PDF "requires `--out <path>`".
- **Scope:** Either (a) make the default-output behavior explicit and documented
  (preferred: keep convenience, document it, emit the path on stderr — already
  does), or (b) require `--out` for PDF and error with exit 64. Pick one; align
  README, capabilities JSON, and `cli_contract.rs`.
- **Acceptance:** docs, capabilities, and behavior agree; a contract test pins it.

## 3. Refinements to existing beads (harden acceptance)

- **`3i5.5`** — add: proof MUST execute a generated wasm-bindgen module (not the
  native adapter, not string matching); size budget measures the real `.wasm`;
  reference G1/N2 as the build dependency.
- **`mwm.3`** — add: harness must emit a single measured conformance percentage,
  surfaced in `capabilities --json` and README, replacing the prose "useful
  subset"; every failing official example gets a gap-ledger row + owning bead.
- **`mwm.5`** — add: 2026-06-28 reality-check note; add N1/N2/N3/N4 as additional
  blockers; add a claim-discipline check (no README capability claim ships unless
  a runnable proof command exists), and explicitly block "first-class WASM"
  marketing on N2 being green.
- **`qw1` / `qw1.1`** — add: visual gauntlet must include a deterministic
  raster-or-render-tree golden (not only structural + byte determinism), so PDF
  *appearance* regressions are caught.

## 3b. Ambition layer — certified, ratcheted, drift-proof proofs

The incremental fixes above remove today's gaps; this layer makes the gaps
*unable to reopen*. Each upgrade turns a one-time fix into a standing invariant
enforced in CI, using techniques the "pull in comrak + a PDF crate" crowd would
never bother with — which is exactly the moat.

### N1 → theme as a single source of truth with derived projections
Treat `Theme` as a normalized store and HTML/PDF/CLI-JSON/WASM as *projections*.
- Add a **theme-token coverage ledger**: enumerate every visual token (link,
  code-chip bg/fg, quote bar/tint, table rule/zebra, heading rule, dark variants)
  and assert each is consumed by *both* the HTML CSS generator and the PDF color
  emitter. CI fails if a token exists with only one consumer (catches future
  divergence at the source, not in review).
- **Cross-surface invariant test:** for a fixed theme, the link color in the HTML
  `<style>`, the PDF link-annotation/border color operator, and the
  `capabilities --json` theme token all decode to the same RGB. One projection
  can never silently drift from another again.

### N2 → certified native↔WASM differential parity ("same core" with evidence)
The README claims HTML/PDF come from "the same dependency-free render core." Make
that a *proof*, not a promise, via a certified-rewrite framing:
- **Native render = specification; WASM render = implementation under test.**
- Build a small corpus (the showcase + edge fixtures). For each input+options,
  render natively and via the *generated* wasm module in a headless runtime;
  assert byte-identical HTML and byte-identical PDF (determinism already holds, so
  parity is the real test). Retain any counterexample as a fixture.
- **Size budget with teeth:** measure raw + gzip + brotli of the emitted `.wasm`;
  CI ratchets (a size increase beyond a committed delta fails until the budget is
  consciously bumped).

### mwm.3 → a ratcheted conformance ladder, not a one-time number
- Commit `conformance_target = N%` and a measured `conformance_actual`. CI **fails
  if actual drops below the committed floor** (a ratchet), so conformance can only
  go up. Raise the floor as gaps close.
- Layer three test modalities so coverage is adversarial, not anecdotal:
  official-spec **differential** (reference outputs vendored as *dev-only test
  data*, never a production dep), **metamorphic** (already present — extend:
  e.g. `wrap-in-blockquote(parse(x)) ≡ parse(wrap-in-blockquote(x))` for safe
  constructs), and **property/fuzz** (no panic; output is well-formed; round-trip
  invariants).
- Distinguish four states per example: `pass`, `known_gap` (owning bead),
  `intentional_non_goal` (rationale), `needs_design`. The README percentage is
  computed from this ledger, never hand-edited.

## 3c. New standing gate — claim discipline (mechanical, not vibes)

### N5 — README ↔ capabilities claim-discipline CI gate
- **Why:** the only place marketing outran reality this cycle was "first-class
  WASM." Make overclaiming *mechanically impossible* to ship.
- **Scope:** a script that extracts capability claims from README/CHANGELOG
  (a curated, labeled claim list) and cross-checks each against
  `capabilities --json` feature flags + a proof-command registry. A claim flagged
  `available` with no green proof command fails CI.
- **Acceptance:** flipping a feature to "available" in README without a passing
  proof command turns CI red. "first-class WASM" stays gated on N2 going green.
- **This is the structural antidote** to the bead-completion / doc-overclaim
  illusion this very reality check exists to catch — encoded so the *next* reality
  check finds nothing to flag here.

### Domain-depth note (for the optimizer/perf beads, not new scope)
The perf sub-tree (`qw1.5.*`, `qw1.7.*`) already cites the right tools: the
Knuth-Plass DP, prefix-sum segment metrics, and a committed double-array/compact
hyphen trie. The standing requirement when those land: any "shortcut"
optimization (Monge/SMAWK line-break, active-list dominance pruning) must **emit a
runtime certificate** proving its precondition for that input and fall back to
exact DP when the certificate fails — never an unproven asymptotic shortcut. This
is already encoded in `PERFORMANCE_OPTIMIZATION_PLAN.md`; the beads must keep that
proof obligation in their acceptance criteria.

## 4. Sequencing

1. N1 (theme colors) + N1-T — small, high-coherence, no external tooling.
2. N4 (doc reconciliation) — trivial, removes a live doc/behavior contradiction.
3. mwm.3 refinement → run the conformance harness → record the real %.
4. N2 (+N2-T) WASM artifact CI gate → unblocks honest "first-class WASM."
5. N5 claim-discipline gate — cheap, and it permanently prevents the overclaim
   this reality check just caught.
6. N3 pagination design; qw1/qw1.1 raster goldens.
7. mwm.5 release gate re-run once N1–N5 + 3i5.5 are green.

## 5. Proof obligations (every bead closeout)

`cargo fmt --check` · `cargo check --all-targets` ·
`cargo clippy --all-targets -- -D warnings` · `cargo test` ·
`scripts/check-policy.sh` · `scripts/check-wasm-core.sh` ·
`scripts/check-determinism.sh` · `scripts/parser-diff.sh`, plus the bead-specific
gate (raster golden, headless WASM smoke, conformance ledger, etc.). No claim in
README/capabilities without a runnable proof command.
