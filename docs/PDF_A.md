# PDF/A-2b in franken_markdown (q6xc.1)

Delta of ISO 19005-2 (PDF/A-2b) against current `fmd` PDF emission, plus what
`--pdf-a 2b` (library: [`PdfASettings::a2b`](../src/pdfa.rs)) actually writes.

The CLI flag `--pdf-a 2b` is the intended agent-facing spelling. This bead lands
the engine hook first (`render_pdf_pdfa` / `render_pdf_document_pdfa`) because
`src/cli.rs` was exclusively reserved by another agent at implementation time.
Wire the clap flag to `PdfASettings` when that lock is free; the objects below
do not depend on clap.

## Requirement vs current emission

| PDF/A-2b requirement | Current default PDF | `--pdf-a 2b` / `PdfASettings::a2b()` |
| --- | --- | --- |
| PDF 1.4–1.7 | PDF 1.7 | unchanged |
| XMP identification (`pdfaid:part=2`, `pdfaid:conformance=B`) | missing | Metadata stream on Catalog |
| OutputIntent + embedded ICC | missing | `/GTS_PDFA1` + compact sRGB ICC (CC0, see below) |
| DeviceRGB/Gray with an RGB OutputIntent | DeviceRGB in content | allowed once OutputIntent is present |
| All fonts embedded | subset Type0 + FontFile2 | unchanged (subsets are allowed) |
| ToUnicode for CID fonts | present | unchanged |
| CIDSet on subset CIDFonts | **missing** | **still missing** (q6xc.2 veraPDF gate) |
| Annotation Print flag (`/F` bit 3) | missing | `/F 4` on link annots |
| No encryption / JS / Launch / embedded files | none emitted | `javascript:` and `file:` URI actions rejected (strict) or dropped |
| Transparency / SMask | used for PNG alpha | allowed in PDF/A-2 (not PDF/A-1) |
| Classic xref (no object streams) | classic xref | unchanged |
| Tagged structure | optional when marks exist | unchanged; tagging is PDF/UA / A-2a, not 2b |

Default (`PdfASettings::OFF`) is byte-identical to historical output: the three
PDF/A objects are numbered after Info/SMask so they never renumber existing
objects.

## Compact sRGB ICC provenance

`src/pdfa.rs` `compact_srgb_icc()` is a **project-authored, CC0** ICC v2
monitor profile:

- Class `mntr`, space `RGB `, PCS `XYZ `, illuminant D50
- Primaries: IEC 61966-2-1 sRGB Bradford-adapted to D50
- TRC: single-entry `curv` gamma 2.2 (not the full sRGB piecewise function)

It is an OutputIntent identifier, **not** the color.org `sRGB2014.icc` binary
(that file is not CC0). q6xc.2 may swap in a larger lab-measured profile if
veraPDF or a print RIP requires it; the Catalog/OutputIntent wiring stays.

## Strict-mode rejection matrix

Named `pdf_a_*` codes on [`RenderError::InvalidInput`](../src/error.rs):

| Code | When | Fix |
| --- | --- | --- |
| `pdf_a_javascript_uri` | `--pdf-a-strict` and a `javascript:` URI action would be emitted | remove the link |
| `pdf_a_file_uri` | `--pdf-a-strict` and a `file:` URI action would be emitted | remove the link |

The PDF URL allow-list already drops both schemes before annotation emission,
so a Markdown `[x](javascript:…)` document still renders. The helper
`pdfa::check_uri_action` is the named gate for any future annot source.

## Known gaps (not this bead)

- CIDSet streams on subset CIDFonts (veraPDF common fail) — q6xc.2
- CLI `--pdf-a 2b` / `--pdf-a-strict` clap flags — blocked on `src/cli.rs`
- Full sRGB TRC / larger ICC — only if q6xc.2 measurement says so
- PDF/A-2a structure completeness / PDF/UA — different epics
