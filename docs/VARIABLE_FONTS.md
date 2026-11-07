# Variable fonts in franken_markdown

How `fmd-font` instances a `wght` face, how tests compare outlines, and how
those sizes feed the distribution-parity budget (`smif.2`).

## What is implemented

`Font::instance(weight)` reads `fvar`/`avar`/`gvar`/`HVAR`/`MVAR` clean-room,
applies packed tuple deltas (shared or private point numbers, IUP, four phantom
points), and freezes horizontal and font-wide metrics at the selected weight.
It rewrites `glyf`/`loca`/`hmtx`, recomputes `head` bounds and the horizontal
extrema in `hhea`, and returns a **static** TrueType font with the consumed
variation tables removed. The same weight twice is byte-identical.

Host bytes reach the renderer through `FontAssets` slot pins (`--pdf-font` /
`--pdf-font-weight` on the CLI, `WasmRenderOptions::with_font_asset_bytes` +
`with_font_slot_weight` in the browser API). When `body-bold` is empty and
`body-regular` is a `wght` face, bold instances from that same file at the
bold slot's effective weight (default 700).

Static host faces ignore a weight pin (`FontWeightIgnoredStatic`).

Each sparse `gvar` tuple infers its missing point deltas on the original
outline coordinates before its region weight is applied. Only those completed
tuple contributions are summed. This preserves distinct sparse regions when
several regions are active, including mixtures with dense tuples, without
letting one region's touched points mask another's interpolation. Scratch
buffers are reused within each glyph, and dense/component tuples skip IUP.
The final summed deltas use OpenType rounding, including negative half units.

Composite instancing applies `gvar` component pseudo-points before the four
phantom points, using the OpenType point-number order. An offset that outgrows
its original signed byte is re-encoded as a signed word; its variation is not
discarded. Component transforms and point attachments retain their original
meaning. After every child has been instanced, the engine recomputes composite
glyph bounds and left side bearings from the actual resulting outlines. This
also updates parents with no variation tuple of their own and works regardless
of glyph order. Horizontal advances remain independent of component movement.
Without `HVAR`, static `hmtx` values use the composite's own phantom-point
deltas. With `HVAR`, explicit horizontal variation metrics take precedence as
described below. The public outline reader separately honors the last child
marked `USE_MY_METRICS`, as it does for static fonts; these are distinct
unhinted-table and outline metrics. The instancer does not execute TrueType
hinting programs.

The bounds pass reuses the ordinary outline decoder without constructing a
temporary font. Its existing per-glyph depth, component and point limits remain
in force, with a shared 16-million-operation allowance for the complete font.
A malformed, cyclic, unrepresentable or over-budget composite prevents a static
instance from being returned. Original caller-owned font bytes are unchanged.

The independently generated `FmdCompositeVariationsVF` regression compares
complete outline vertices, stored bounds and horizontal metrics against
FontTools' saved static instances at three weights. It includes growing byte
offsets, a nested scaled parent, point-attached accents and subset round trips.
This extends the synthetic bbox-only evidence described below; it does not
claim complete validation of arbitrary retail variable fonts or hinted output.

## Horizontal and font-wide variation metrics

`HVAR` advance deltas are applied to the original `hmtx` values. The resulting
advances replace the metrics derived from `gvar` phantom points, so an advance
represented in both tables changes exactly once. Both implicit glyph-ID
indexing and explicit delta-set maps are supported, including shared map rows,
short maps that repeat their final entry, and no-variation indices. Optional
mapped left side bearings likewise use their original values plus the `HVAR`
delta. Without that map, the final outline and `gvar` left phantom determine
the bearing. Right-bearing maps are read and validated; static TrueType derives
the right bearing from advance, left bearing, and the final outline width.
These paths agree for consistent `gvar` and `HVAR` tables, as required by the
[OpenType HVAR format](https://learn.microsoft.com/en-us/typography/opentype/spec/hvar).

Contourless glyphs still process their four `gvar` phantom points. A space
therefore gets its selected advance even when the variable font has no `HVAR`
table. It remains contourless in the static font and does not acquire synthetic
ink bounds.

`MVAR` applies the standard metric tags for `OS/2`, `hhea`, `vhea`, `post`, and
the first ten adjustable `gasp` ranges. This includes typographic ascent,
descent, line gap, clipping metrics, x-height/cap-height, caret metrics,
subscript/superscript dimensions, strikeout and underline measurements. The
terminal `gasp` range remains `65535`. Unknown private metric tags are ignored
according to the [MVAR specification](https://learn.microsoft.com/en-us/typography/opentype/spec/mvar).
As in FontTools static instancing, originally equal `hhea` and `OS/2`
ascent/descent/line-gap triples stay synchronized; intentionally different
`hhea` values remain independent.

The metric reader uses the same normalized, `avar`-mapped axis location as the
outline instancer. Other axes remain at their defaults. It handles intermediate
and negative regions, both signed short/byte and long/short delta rows, and
OpenType integer rounding (half-unit ties toward positive infinity). Region
scalars are computed once, delta rows remain borrowed, and repeated map indices
reuse cached results. Each store admits at most 262,144 region columns, with
one eight-million-unit work allowance across `HVAR` and `MVAR`. A malformed, truncated,
over-budget or unrepresentable metric table rejects the entire instance;
partially updated metrics are never returned.

After outlines and metrics are final, a linear scan rebuilds the font-wide
bounding box, maximum advance, minimum left/right bearings, and maximum ink
extent. Contourless glyphs participate in maximum advance only, matching the
[`head`](https://learn.microsoft.com/en-us/typography/opentype/spec/head) and
[`hhea`](https://learn.microsoft.com/en-us/typography/opentype/spec/hhea) rules.
The serializer moves the completed outline tables into the static font without
cloning another full `glyf`/`loca` pair.

The authored OFL fixtures `FmdMetricsVF` and `FmdMappedMetricsVF` compare saved
FontTools static-instance horizontal metrics and global table fields at six
weights. They exercise two axes, nonlinear `avar`, intermediate regions,
contourless space, and both kinds of advance mapping. FontTools 4.61.1 does not
instantiate MVAR `gasp` range tags; those expectations are explicitly separate
spec-derived tests. The independent FontTools table oracle is not used to
assert behavior that FontTools does not implement. Additional tests cover
HVAR-absent phantom metrics, static subset round trips, checksum validity,
determinism, source immutability, truncation and mutation.

This is horizontal TrueType instancing. Per-glyph vertical `VVAR` metrics,
CFF2 outlines, and execution of TrueType hinting programs are outside the
supported path.

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

This older bbox score is deterministic, has no rasterizer dependency, and
matches the apply-then-encode pipeline. On its own it cannot catch a point
moving inside an unchanged box. The composite and sparse-tuple fixtures also
compare complete outline vertices against independently saved FontTools
instances. Sparse-tuple cases cover overlapping regions, dense/sparse mixtures,
reversed tuple order, separate contours, untouched phantom points, and negative
half-unit rounding.

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
| variable_triangle_fixture | 416 | gvar+glyf synthetic (not a design face) |
| instanced static @ 400 | 304 | what PDF/HTML actually embed |
| instanced static @ 700 | 308 | what PDF/HTML actually embed |

No-claim line: a retail variable font can be **larger** than the static cuts
it replaces. Do not budget the host VF into the npm/WASM package. `smif.2`
should count **instanced static subsets** (the bytes after `Font::instance` +
subset), which is what ships in HTML data URLs and PDF font streams.

The triangle fixture exists to prove instancing, not to shrink the
distribution. Replacing the five bundled IBM Plex / Computer Modern cuts with
one retail VF is a separate measurement, gated on an actual candidate file.

## Hostile sweeps

`fvar` and `gvar` table bytes are LCG-mutated independently. `Font::parse` +
`Font::instance` + `render_pdf` must never panic. Existing `gvar` recovery may
retain a default outline; invalid `HVAR`/`MVAR` admission returns `None` for the
whole instance. The metric tests also exercise every table-prefix truncation
and deterministic table-byte mutation, including malformed offsets and counts.

## Tests and scripts

- `fmd-font/tests/variation_test.rs` — committed FmdTestVF axes, bbox
  tolerance, fvar LCG, optional `FMD_DUMP_TRIANGLE_VF` dump.
- `fmd-font/tests/composite_variation_test.rs` — independently captured
  composite outlines, stored bounds, metrics, and subset round trips.
- `fmd-font/tests/sparse_tuple_variation_test.rs` — independently captured
  overlapping tuple interpolation, complete contours, and rounding.
- `fmd-font/tests/variation_metrics_test.rs` — independent static metric
  oracle, HVAR/MVAR integration, blank glyphs, and bounded hostile inputs.
- `tests/variable_font_e2e.rs` — library + WASM API + CLI slot.
- `scripts/variable-font-e2e.sh` — CLI e2e with a machine-readable
  `summary.json` / `checks.jsonl` under `tests/artifacts/variable-font/<run-id>/`.
