# PDF Accessibility Ledger

Date: 2026-06-29
Scope: tagged-PDF structure semantics emitted by the clean-room PDF writer
(`src/pdf.rs`). Bead: `qw1.9`.

`franken_markdown` writes a tagged PDF (`/MarkInfo << /Marked true >>`,
`/StructTreeRoot`, `/Lang (en-US)`, `/Tabs /S`) so the output is usable by
screen readers and other assistive technology, not merely visually plausible.
This ledger records exactly which structure semantics are supported, how they
are produced, the known limitations of the line-based writer, and the explicit
non-goals. It depends on **no third-party PDF parser** in production; the
guarantees below are pinned by byte-level structural tests in
`tests/pdf_test.rs`.

## Supported semantics

The structure tree is a real hierarchy rooted at a single `/Document` element:

| Markdown | Structure | Notes |
|---|---|---|
| `#`/`##`/`###` heading | `/H1` / `/H2` / `/H3` | Lower levels collapse to generic `/H` (see limitations) |
| Paragraph | `/P` | Wrapped lines of one paragraph share a single `/P` (one `/MCR` per line) |
| Bullet / ordered / task list | `/L` → `/LI` → `/LBody` → `/P` | Nested lists produce nested `/L`, to arbitrary depth |
| Blockquote | `/BlockQuote` | Nested quotes nest; content keeps its own `/P`/`/L`/… inside |
| Pipe table | `/Table` → `/TR` → `/TH`\|`/TD` | Per **cell** marks; header cells are `/TH` |
| Table header cell | `/TH` with `/A << /O /Table /Scope /Column >>` | Column scope (Markdown headers are column headers) |
| Fenced code block | `/Code` | All lines of one block share a single `/Code` |
| Image | `/Figure` with `/Alt` and `/A << /O /Layout /BBox [...] >>` | Alt text from the Markdown `![alt]`; bbox locates the image |
| Inline / autolink link | `/Link` with `/OBJR` to its annotation, and the annotation's reverse `/StructParent` | Fully bidirectional: the element references the annotation and the annotation maps back through the parent tree (PDF/UA) |
| Backgrounds, panels, zebra stripes, inline-code chips, rules, thematic breaks, blockquote gutter bars | `/Artifact` (BMC…EMC) | Decoration is kept out of the reading order |

Cross-cutting guarantees, all asserted by tests:

- **No unmarked content.** Every byte of page content is inside either a
  structure marked-content sequence (`/<Tag> <</MCID n>> BDC … EMC`) or an
  `/Artifact BMC … EMC` span. Marked content is always balanced
  (`#BDC + #BMC == #EMC`). This is the PDF/UA 7.1 "all content is tagged"
  requirement.
- **Bidirectional links.** Each OBJR-referenced link annotation carries a
  `/StructParent` whose key maps, through the parent tree, back to its owning
  `/Link` element; the `/StructTreeRoot` advertises `/ParentTreeNextKey`.
- **Parent tree integrity.** Each page declares `/StructParents`, and the
  `/ParentTree` `/Nums` maps every content `/MCID`, in order, to the structure
  element that owns it. Each content-stream `/MCID` is referenced by exactly one
  structure `/MCR`.
- **Selectable text preserved.** `/ToUnicode` maps (including ligature glyphs)
  are unchanged, so copy/search still work over the tagged content.
- **Determinism.** Given fixed input/theme/fonts/options the tagged bytes are
  stable across runs and `SOURCE_DATE_EPOCH` values
  (`scripts/check-determinism.sh`).

## How it is produced

The writer flattens the document AST to a vector of laid-out `Line`s, paginates,
then in the page builder emits one marked-content sequence per visible line
(per **cell** for table rows). Each mark carries a *container path* — the chain
of structure elements from just below `/Document` down to the element that owns
the mark's content (blockquotes, then list `/L`/`/LI`/`/LBody`, then the leaf or
table `/Table`/`/TR`/`/TH`/`/TD`). `build_struct_tree` diffs consecutive paths
to open and reuse shared ancestors, so tables, lists, and blockquotes nest
correctly without the page builder needing global structure state. List
membership is threaded from `layout_list` (innermost list wins); table cell
columns are threaded from `layout_table`; blockquote and code grouping are
derived from existing per-line data.

## Known limitations

These are deliberate consequences of a small, line-based, dependency-free writer.
They are safe (they never corrupt the tree) and are candidates for future beads.

- **Heading levels H4–H6** collapse to the generic `/H` tag. They share the body
  text measure, so the writer cannot recover the exact source level from glyph
  size alone. H1–H3 are exact.
- **Inline links inside a paragraph** are tagged at *line* granularity: a line
  that contains any link run becomes a single `/Link` element covering the whole
  line, rather than splitting the surrounding prose into `/P` text plus a nested
  `/Link` span. The link annotation is still correctly referenced with `/OBJR`.
- **Empty table cells** emit no `/TD`; a row with blank source cells is not
  back-filled to a rectangular grid. Non-empty cells are always tagged.
- **No `/Headers` ID associations** between body cells and header cells. Header
  cells carry a column `/Scope`, which covers simple Markdown grids; there is no
  row/column id linkage and no row/col span (GFM has neither).
- **Page-split blocks fragment.** A paragraph, list, table, or blockquote split
  across a page break becomes sibling elements under `/Document` (one fragment
  per page), not one logical element spanning pages. Reading order is preserved.
- **List markers** (bullet/number/checkbox) are part of the item's first line
  text, not a separate `/Lbl` element, and ordered vs. bullet lists both tag as
  `/L` without an explicit `/ListNumbering` attribute.

## Non-goals

- Formal PDF/UA-1 or WCAG certification, or any dependency on a third-party PDF
  validator in the production build. External validators may be used as
  developer-only spot checks.
- MathML / complex-table (`/Headers`, span) structures, and full artifact
  classification beyond keeping decoration out of the reading order.

## Verification

Closing `qw1.9` cited:

- `cargo test` — `tests/pdf_test.rs::pdf_structure_tree_is_hierarchical_and_accessible`
  (Document root + parent chain, `/L`/`/LI`/`/LBody` nesting, `/Table`/`/TR`/`/TH`/`/TD`
  with column scope, `/BlockQuote`, `/Code`, `/Link` `/OBJR`, `/Artifact`
  balance, MCID↔MCR parity) and
  `pdf_emits_tagged_structure_tree_for_core_blocks`,
  `pdf_renders_supplied_png_image_as_xobject` (`/Figure` `/Alt` + `/BBox`).
- `scripts/check-determinism.sh` — byte-identical tagged PDF across runs.
- `scripts/check-policy.sh` — clean-room, no new dependencies, no `unsafe`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`.
