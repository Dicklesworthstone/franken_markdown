# Emoji and Symbol Fallback Policy (bead y5i9.2)

Date: 2026-08-28
Scope: how `franken_markdown` handles emoji and unsupported symbol codepoints
that have no glyph in the bundled face set. Bead: `y5i9.2`.

## Decision (v1)

**Strategy: warning-only.** Emoji and other unsupported symbol codepoints are
rendered as `.notdef` boxes (PDF) or as the raw UTF-8 bytes escaped to
HTML, and the render surfaces a single `missing_glyphs` warning that names the
count and a small sample. No curated emoji subset is bundled; no
emoji-as-drawn-vector fallback is performed. The behavior is identical to the
v0.3.4 release.

This is the deliberately conservative position. The clean-room, dependency-lean
doctrine forbids dragging in `noto-emoji` or any other emoji font crate, and a
hand-rolled Noto Emoji subset would itself be a multi-week design + curation
job (Unicode 15.1 has 3,790 emoji; the right subset depends on the corpus).
Falling through to a drawn fallback would require us to teach the layout
engine to rasterize per-glyph outlines, which is the same complexity class as
the PDF text shaping we already own.

## The strategies the policy enumerates

The config key `emoji_strategy` exists today for forward compatibility; only
one value is honored in v1.

| Value         | v1 status | Behavior |
|---------------|-----------|----------|
| `warning`     | **honored** (default) | Render `.notdef`; emit `missing_glyphs` warning. Current behavior. |
| `noto_subset` | accepted, not honored | Reserved for a future curated Noto Sans Symbols / Noto Emoji subset. The size cost must be measured first; a small subset is feasible but every added face is a multi-KB cost against the WASM size budget. |
| `drawn`       | accepted, not honored | Reserved for a future per-glyph vector-drawing path (fmd-math's `drawn.rs` precedent). Out of scope until a benchmark proves the cost is acceptable in the hot path. |

Unknown values are a parse error that names the three legal selectors. The
intent is to make the policy machine-readable (agents and CI can branch on
the value) without silently substituting a default a caller did not ask for.

## Why "warning-only" is the right v1

1. **Determinism.** `.notdef` is byte-stable for a given font subset. A
   curated emoji face would change the subsetter's seed for every document
   that contains an emoji codepoint, and the SHA-1 of the rendered output
   would no longer match a `SOURCE_DATE_EPOCH`-pinned build. That's a strict
   regression for the determinism gate.
2. **Audit surface.** Adding a bundled face is a 30–80 KB hit to the
   distribution archives and the WASM package. The size budget gate
   (`scripts/check-wasm-package.sh`) is already at the committed ratchet;
   bumping it for an emoji face that only some users need would
   disproportionately penalize everyone.
3. **Honesty.** Rendering `.notdef` makes the gap visible. A user with a
   critical emoji in their document sees the warning, knows to add a font
   (via `--pdf-font` for PDF, or a custom `<style>` for HTML) or remove the
   emoji. A "silent" emoji face that just-renders would hide a real
   problem (e.g. wrong glyph for a region, or an outdated symbol).

## Migration path

The policy above is designed to evolve without re-litigating the bead.

1. **Phase 1 (v1, today).** `emoji_strategy = warning` (default). No bundled
   emoji face. `missing_glyphs` is the single warning code that covers
   emoji + math + non-Latin alike.
2. **Phase 2 (when a corpus demands it).** Curate a Noto Sans Symbols
   subset for the top-N most-frequent symbol codepoints, bundled as a
   new face. The size delta must be measured against the current ratchet
   and either absorbed (subset) or the ratchet bumped with documented
   justification. A new warning code `emoji_glyphs_missing` is introduced
   only if the strategy decides to split emoji from non-emoji glyph
   warnings.
3. **Phase 3 (long tail).** A drawn fallback for the symbol subset, if
   the rendered text needs vector emoji that no face provides. The
   fmd-math `drawn.rs` precedent (KaTeX-style rendering) is the
   reference implementation, but it pulls layout cost into the hot path
   and needs a benchmark before it ships.

## How an agent branches on this

For now, the only stable contract is the `missing_glyphs` warning code and
its `count` and `sample` fields. Agents and CI should:

- surface the warning to the user (or refuse the PR in CI),
- propose a fix (drop the emoji, add a face, or override with a custom
  CSS for HTML that pulls an emoji web font).

There is no per-emoji warning code in v1. A future `emoji_glyphs_missing`
code would let agents distinguish "the user's text contains emoji that our
bundled faces cannot render" from "the user's text contains a regional
indicator / combining diacritic that our bundled faces cannot render",
which is useful for diagnostics but not yet necessary for v1.

## Cross-references

- `src/pdf.rs` — `RenderWarning::MissingGlyphs` definition, the `render_warnings`
  walker, and the existing test in `src/pdf.rs:35019` that covers the
  emoji-occurrence counting path.
- `src/config.rs` — `CONFIG_KEYS` array where `emoji_strategy` is registered.
- `src/cli.rs` — `print_robot_docs()` entry that names the policy.
- `fmd-font/fonts/` — the curated Noto Sans Math symbol fallback face, which
  is the model for any future Noto Sans Symbols / Noto Emoji subset.
