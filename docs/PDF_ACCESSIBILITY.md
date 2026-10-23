# PDF Accessibility Ledger

Date: 2026-10-23
Scope: tagged-PDF structure semantics emitted by the clean-room PDF writer
(`src/pdf.rs`). Beads: `qw1.9`, `6lcj`, `j36e`.

`franken_markdown` writes a tagged PDF (`/MarkInfo << /Marked true >>`,
`/StructTreeRoot`, `/Lang (en-US)`, `/Tabs /S`) so the output is usable by
screen readers and other assistive technology, not merely visually plausible.
This ledger records exactly which structure semantics are supported, how they
are produced, the known limitations of the line-based writer, and the explicit
non-goals. It depends on **no third-party PDF parser** in production; the
guarantees below are pinned by byte-level structural tests in
`tests/pdf_test.rs`, `tests/pdf_accessibility_pages_test.rs`, and
`tests/pdf_table_accessibility_test.rs`.

## Supported semantics

The structure tree is a real hierarchy rooted at a single `/Document` element:

| Markdown | Structure | Notes |
|---|---|---|
| `#` … `######` heading | `/H1` … `/H6` | Exact level from the source heading, independent of the type scale |
| Paragraph | `/P` | Wrapped lines share a single `/P`, including across page boundaries; every `/MCR` names its actual page |
| Bullet / ordered / task list | `/L` → `/LI` → `/LBody` → `/P` | Nested lists produce nested `/L`, to arbitrary depth |
| Blockquote | `/BlockQuote` | Nested quotes nest; content keeps its own `/P`/`/L`/… inside |
| Pipe table | `/Table` → `/TR` → `/TH`\|`/TD` | Complete rectangular grid, including blank cells and entirely blank rows |
| Table header cell | `/TH` with a stable `/ID` and `/A << /O /Table /Scope /Column >>` | Column scope; blank headers retain their column positions |
| Table data cell | `/TD` with `/A << /O /Table /Headers [ (header-id) ] >>` | Explicit association to its column's header; `/IDTree` resolves every header ID |
| Repeated table header | `/Artifact` with pagination/header attributes | Continued-page paint stays outside the reading order; empty marks on the original `/TH` retain ownership of clickable annotations |
| Fenced code block | `/Code` | All lines of one block share a single `/Code` |
| Image | `/Figure` with `/Alt` and `/A << /O /Layout /BBox [...] >>` | Alt text from the Markdown `![alt]`; bbox locates the image |
| Inline / autolink link | `/Link` nested in its `/P` or heading (one per link run; prose stays in the parent) with `/OBJR` to its annotation, and the annotation's reverse `/StructParent` | Fully bidirectional: the element references the annotation and the annotation maps back through the parent tree (PDF/UA) |
| Backgrounds, panels, zebra stripes, inline-code chips, rules, thematic breaks, blockquote gutter bars | `/Artifact` (BMC…EMC) | Decoration is kept out of the reading order |
| Running header/footer and page numbers (`PdfOptions.running`, `page_numbers`) | `/Artifact <</Type /Pagination /Subtype /Header\|/Footer>> BDC … EMC` | One pagination artifact per band, text and rule together; never in the reading order and never given an `/MCID` |

Cross-cutting guarantees, all asserted by tests:

- **No unmarked content.** Every byte of page content is inside either a
  structure marked-content sequence (`/<Tag> <</MCID n>> BDC … EMC`) or an
  `/Artifact BMC … EMC` span (running header/footer bands and repeated table
  headers use the typed
  `/Artifact <</Type /Pagination …>> BDC` form). Marked content is always balanced
  (`#BDC + #BMC == #EMC`). This is the PDF/UA 7.1 "all content is tagged"
  requirement.
- **Bidirectional links.** Each OBJR-referenced link annotation carries a
  `/StructParent` whose key maps, through the parent tree, back to its owning
  structure element (a `/Link` in prose, or its cell/figure in a table);
  the `/StructTreeRoot` advertises `/ParentTreeNextKey`.
- **Parent tree integrity.** Each page declares `/StructParents`, and the
  `/ParentTree` `/Nums` maps every content `/MCID`, in order, to the structure
  element that owns it. Each content-stream `/MCID` is referenced by exactly one
  structure `/MCR`.
- **Complete table relationships.** Every rendered column has one header cell
  and one data cell per body row, including empty positions. Data cells name
  their header explicitly, with unique IDs across separate or cached tables,
  page continuations, and footnotes. Empty cells add no glyphs or invented labels.
- **Selectable text preserved.** `/ToUnicode` maps (including ligature glyphs)
  are unchanged, so copy/search still work over the tagged content.
- **Logical elements span pages.** A source paragraph, code block, quote, list,
  list item, table, row or cell retains one structure element as it continues
  onto later pages. Repeated header paint does not repeat its text or image alt
  in the logical reading order. Independently laid-out footnote containers and
  page-local inline formulas/images keep distinct identities.
- **Determinism.** Given fixed input/theme/fonts/options the tagged bytes are
  stable across runs and `SOURCE_DATE_EPOCH` values
  (`scripts/check-determinism.sh`).

## How it is produced

The writer flattens the document AST to a vector of laid-out `Line`s, paginates,
then the page builder emits marked-content sequences for visible lines and
individual table cells. Empty cell sequences preserve grid positions without
painting or changing placement. Each mark carries a *container path* — the chain
of structure elements from just below `/Document` down to the element that owns
the mark's content (enclosing blockquotes and lists in source nesting order,
then the leaf or table `/Table`/`/TR`/`/TH`/`/TD`). `build_struct_tree` diffs consecutive paths
to open and reuse shared ancestors. Its identity map persists across pages,
while each content reference and annotation retains its own page. Lists and
quotes allocate document-wide identities before their children, preserving
nesting even when a footnote is laid out in a separate temporary vector. Table
rows retain their original source indices through wrapping and pagination.
Inline formulas and figures use a page-qualified occurrence key because their
MCIDs restart on each page. Both PDF emission modes use this same hierarchy.

A table header ID is derived from the document-wide table identity and source
column. Each data cell's `/Headers` array names that ID, and `/StructTreeRoot`
contains a byte-sorted `/IDTree` name tree mapping every ID to its header element.
This follows the identifier and table-attribute requirements in
[ISO 32000-1, Tables 322 and 349](https://opensource.adobe.com/dc-acrobat-sdk-docs/standards/pdfstandards/pdf/PDF32000_2008.pdf).
The renderer's existing normalized grid width also supplies missing cells in
ragged host-created tables. Financial footing strokes remain artifacts rather
than invented data rows.

## Known limitations

These are deliberate consequences of a small, line-based, dependency-free writer.
They are safe (they never corrupt the tree) and are candidates for future beads.

- **Complex table headers and spanning cells** are not represented by the
  Markdown AST. Header associations cover its single column-header row; row
  headers, multiple header tiers, and row/column spans remain unsupported.
- **List markers** (bullet/number/checkbox) are part of the item's first line
  text, not a separate `/Lbl` element, and ordered vs. bullet lists both tag as
  `/L` without an explicit `/ListNumbering` attribute.

## Non-goals

- Formal PDF/UA-1 or WCAG certification, or any dependency on a third-party PDF
  validator in the production build. External validators may be used as
  developer-only spot checks.
- MathML / complex-table (multiple header tiers, spanning cells) structures, and full artifact
  classification beyond keeping decoration out of the reading order.

## Verification

`tests/pdf_accessibility_pages_test.rs` checks emitted multipage PDFs for one
logical element per source block/cell, nested list/quote ownership, header
artifacts with preserved annotations, unique dense MCID owners, separately
laid-out footnotes, inline formula identity, deterministic output, and equality
between monolithic and chunked emission. It also exercises adaptive table
scaling below the ordinary body-table floor inside a small-type footnote.

`tests/pdf_table_accessibility_test.rs` verifies complete sparse and blank
matrices, normalized ragged host tables, sorted ID lookup and column-specific
header associations, distinct cached tables, wrapping and repeated headers,
list/footnote ownership, linked images with preserved alt text, PDF/A output,
annotation back-references, deterministic bytes, and parity between emitters.

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

## PDF/UA-1 measured conformance (2026-08-28, veraPDF `ua1`)

The `pdf-a-validation` workflow runs a PDF/UA-1 leg on a fixed fixture and
ratchets `failedRules` against `tests/fixtures/pdfua-baseline.txt` (currently
**3**; worse fails CI, better tightens the baseline on review). Measured gaps:

| veraPDF rule | What it flags |
|---|---|
| clause 7.1, test 9 | heading-structure detail (H4–H6 collapsed to generic `/H` when measured; exact `/H4`–`/H6` tags have since landed, re-measure before tightening) |
| clause 7.1, test 10 | heading-structure detail (same roadmap item) |
| clause 5, test 1 | document-level requirement (viewer/title metadata class) |

These rows map to the roadmap items above (H4–H6 exact tags, now landed; document title
display). When they land, tighten the baseline in the same commit.
