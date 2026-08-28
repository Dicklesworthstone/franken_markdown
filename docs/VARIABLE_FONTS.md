# Variable fonts in franken_markdown

How `fmd-font` instances a `wght` face, how tests compare outlines, and how
those sizes feed the distribution-parity budget (`smif.2`).

## What is implemented

`Font::instance(weight)` reads `fvar`/`avar`/`gvar` clean-room, applies packed
tuple deltas (shared or private point numbers, IUP, four phantom points),
rewrites `glyf`/`loca`/`hmtx`, and returns a **static** TrueType font with
`fvar`/`avar`/`gvar`/`HVAR` dropped. The same weight twice is byte-identical.

Host bytes reach the renderer through `FontAssets` slot pins (`--pdf-font` /
`--pdf-font-weight` on the CLI, `WasmRenderOptions::with_font_asset_bytes` +
`with_font_slot_weight` in the browser API). When `body-bold` is empty and
`body-regular` is a `wght` face, bold instances from that same file at the
bold slot's effective weight (default 700).

Static host faces ignore a weight pin (`FontWeightIgnoredStatic`).

## Outline tolerance comparison method

This engine does **not** rasterize glyphs and does **not** compute a Hausdorff
distance on contours. Those would pull a rasterizer or a geometry crate into
the core, which the clean-room doctrine forbids.

The published comparison method is:

1. Parse both faces with `Font::parse`.
2. Take the **cmap-shared** glyph set: for each probe codepoint, keep the
   glyph only when both faces map it to the same glyph id.
3. Read each glyph's `glyf` header bbox (`xMin, yMin, xMax, yMax`) in font
   units via `Font::glyph_bbox`.
4. The score is the **L∞** (max absolute) delta across those four values,
   over the shared set.

Assertions:

| Pair | Required score |
| --- | --- |
| Same face, same weight, twice | bbox L∞ = 0, and `as_sfnt()` bytes equal |
| Triangle fixture at 400 vs 900 | bbox L∞ = 50 (the private-point `gvar` hop on p0) |

The probe set for the synthetic fixture is U+0020 (space): that is the only
codepoint the fixture cmap maps, and it shares gid 0 with `.notdef`. A retail
face should probe a documented letter set the same way; the method does not
change.

This is deterministic, has no rasterizer dependency, and matches the
apply-then-encode pipeline. It will **not** catch a point that moves inside an
unchanged bbox; that is an accepted gap until a contour-walk comparison lands
as its own bead.

## Size report (feeds smif.2)

Measured 2026-08-28 from the files in `fmd-font/fonts/` and from
`variable_triangle_fixture()` / its instanced static faces (the e2e script
rewrites `tests/artifacts/variable-font/<run-id>/size_report.json` on every
run).

| Face | Bytes | Role |
| --- | ---: | --- |
| IBMPlexSans-Regular.ttf | 200500 | bundled static cut |
| IBMPlexSans-Bold.ttf | 200872 | bundled static cut |
| IBMPlexSans Regular + Bold | 401372 | two static files the VF is meant to replace |
| FmdTestVF.ttf | 338 | fvar/avar-only parse fixture (no glyf/gvar) |
| variable_triangle_fixture | measured by e2e | gvar+glyf synthetic (not a design face) |
| instanced static @ 400 / 700 | measured by e2e | what PDF/HTML actually embed |

No-claim line: a retail variable font can be **larger** than the static cuts
it replaces. Do not budget the host VF into the npm/WASM package. `smif.2`
should count **instanced static subsets** (the bytes after `Font::instance` +
subset), which is what ships in HTML data URLs and PDF font streams.

The triangle fixture exists to prove instancing, not to shrink the
distribution. Replacing the five bundled IBM Plex / Computer Modern cuts with
one retail VF is a separate measurement, gated on an actual candidate file.

## Hostile sweeps

`fvar` and `gvar` table bytes are LCG-mutated independently. `Font::parse` +
`Font::instance` + `render_pdf` must never panic. A mutated table may fail
closed to the default master or to a render error; aborting is the defect.

## Tests and scripts

- `fmd-font/tests/variation_test.rs` — committed FmdTestVF axes, bbox
  tolerance, fvar LCG, optional `FMD_DUMP_TRIANGLE_VF` dump.
- `tests/variable_font_e2e.rs` — library + WASM API + CLI slot.
- `scripts/variable-font-e2e.sh` — CLI e2e with a machine-readable
  `summary.json` / `checks.jsonl` under `tests/artifacts/variable-font/<run-id>/`.
