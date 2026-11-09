# PDF Accessibility Ledger

Date: 2026-11-09
Scope: tagged-PDF structure semantics emitted by the clean-room PDF writer
(`src/pdf.rs`). Beads: `qw1.9`, `6lcj`, `j36e`.

`franken_markdown` writes a tagged PDF (`/MarkInfo << /Marked true >>`,
`/StructTreeRoot`, `/Lang`, `/Tabs /S`) so the output is usable by
screen readers and other assistive technology, not merely visually plausible.
This ledger records exactly which structure semantics are supported, how they
are produced, the known limitations of the line-based writer, and the explicit
non-goals. It depends on **no third-party PDF parser** in production; the
guarantees below are pinned by byte-level structural tests in
`tests/pdf_test.rs`, `tests/pdf_accessibility_pages_test.rs`,
`tests/pdf_table_accessibility_test.rs`, `tests/pdf_language_test.rs`, and
`tests/pdf_note_accessibility_test.rs`.

The catalog language follows `PdfOptions.lang`, including regional tags such
as `fr-CA`. An absent or blank language retains the default `en-US`; supplied
values use the writer's PDF string encoding. This applies to ordinary and
PDF/A output and lets assistive readers select the document's language even
when the hyphenator uses a fallback pattern set.

## Supported semantics

The structure tree is a real hierarchy rooted at a single `/Document` element:

| Markdown | Structure | Notes |
|---|---|---|
| `#` … `######` heading | `/H1` … `/H6` | Exact level from the source heading, independent of the type scale |
| Paragraph | `/P` | Wrapped lines share a single `/P`, including across page boundaries; every `/MCR` names its actual page |
| Bullet / ordered / task list | `/L` → `/LI`, with sibling `/Lbl` and `/LBody` children | Markers are selectable labels; body paragraphs, headings, tables, code, figures and nested lists keep their own structure |
| Blockquote | `/BlockQuote` | Nested quotes nest; content keeps its own `/P`/`/L`/… inside |
| Pipe table | `/Table` → `/TR` → `/TH`\|`/TD` | Complete rectangular grid, including blank cells and entirely blank rows |
| Table header cell | `/TH` with a stable `/ID` and `/A << /O /Table /Scope /Column >>` | Column scope; blank headers retain their column positions |
| Table data cell | `/TD` with `/A << /O /Table /Headers [ (header-id) ] >>` | Explicit association to its column's header; `/IDTree` resolves every header ID |
| Repeated table header | `/Artifact` with pagination/header attributes | Continued-page paint stays outside the reading order; empty marks reuse the original cell or its `/Link`, which owns each repeated clickable annotation |
| Fenced code block | `/Code` | All lines of one block share a single `/Code` |
| Image | `/Figure` with `/Alt` and `/A << /O /Layout /BBox [...] >>` | Alt text from the Markdown `![alt]`; bbox locates the image; linked images retain their `/Figure` inside a `/Link`, including inline and table images |
| Inline math | `/Formula` with `/Alt` and `/A << /O /Layout /BBox [...] >>` | Source math and bounds stay on the formula; linked formulas are children of their `/Link` |
| Inline / autolink link | `/Link` nested in its `/P`, heading, `/TH`, or `/TD`, with `/OBJR` to its annotations and each annotation's reverse `/StructParent` | Styled text, inline images, and formulas share the enclosing link across wrapping and page boundaries; links in separate cells retain distinct owners |
| Footnote or endnote body | `/Note` with a unique `/ID` registered in `/IDTree` | One complete logical note across page-foot placement, oversized/unreferenced endnote fallback, page breaks and rich child blocks |
| Footnote citation | `/Reference`, with its generated `/Link` child | Each actual source citation keeps its own occurrence; repeated citation text or an authored destination that resembles a note ID cannot manufacture a citation role |
| Footnote number | `/Lbl` directly inside `/Note` | Existing superscript paint is retained; optional return links remain `/Link` children with annotation ownership |
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
  `/Link` element, including links in table cells and linked inline objects.
  A linked figure or formula still owns its content `/MCID`, alt text, and
  bounds; its enclosing `/Link` owns the annotation `/OBJR`.
  The `/StructTreeRoot` advertises `/ParentTreeNextKey`.
- **Link reading order.** Separate links to the same destination remain
  separate `/Link` children when unlinked content intervenes, preserving that
  content's position in the parent `/K` array. Contiguous styled text, images,
  and formulas share their link across line and page boundaries.
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
- **Logical elements span pages.** A source paragraph, link, code block, quote, list,
  list item, note, table, row or cell retains one structure element as it continues
  onto later pages. Repeated header paint does not repeat its text, image alt,
  or formula source in the logical reading order. Independently laid-out footnote containers and
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
Linked content adds a `/Link` below its text or cell container, with any
`/Figure` or `/Formula` below the link. Annotation ownership follows that
enclosing link without changing the content's marked owner or geometry.
Contiguous link runs are tracked independently for each text flow and table
cell, so another column's paint cannot interrupt a wrapped link. Repeated
headers replay the original segment-to-link mapping without advancing those
logical runs.

A table header ID is derived from the document-wide table identity and source
column. Each data cell's `/Headers` array names that ID, and `/StructTreeRoot`
contains a byte-sorted `/IDTree` name tree mapping every ID to its header element.
This follows the identifier and table-attribute requirements in
[ISO 32000-1, Tables 322 and 349](https://opensource.adobe.com/dc-acrobat-sdk-docs/standards/pdfstandards/pdf/PDF32000_2008.pdf).
The renderer's existing normalized grid width also supplies missing cells in
ragged host-created tables. Financial footing strokes remain artifacts rather
than invented data rows.

## Footnotes, citations and return links

Source-facing PDF rendering prepares an immutable sidecar together with the
ordinary document AST. The sidecar records actual `FootnoteRef` occurrences,
generated note labels and complete note bodies. It stores structural ordinals,
so an owned copy or repeated preparation cannot retain addresses into a
different document. Rendering binds those ordinals to its immutable input;
text, identifier spelling and link destinations never establish a citation role.

Each rendered note has one `/Note` and an ID such as `fmd-note-1`. The existing
byte-sorted `/IDTree` contains both note and table-header IDs. Paragraphs,
headings, quotes, lists, tables, figures and code remain children with their own
roles. A note moved to the trailing Notes section retains the same identity
on every line, including baseline-grid migration and multipage continuations.
Chapter language sections continue to own their defining notes, independently
of which chapter cites or links to them.

Each linked source citation has `/Reference` → `/Link`; the `/Link` owns the
actual annotation and its reverse parent-tree entry. Adjacent or repeated
citations are distinct occurrences even when the writer shapes their text as
one run. A citation inside an existing author link receives `/Reference`
inside that link and preserves the author's enclosing action. Undefined notes
remain literal text. Duplicate definitions retain the first definition, and
cyclic note references do not duplicate bodies.

The existing `footnote_backlinks` option makes a note's `/Lbl` contain its
return `/Link`. Navigation continues to use actual emitted destinations after
pagination. These are PDF 1.7 roles and link annotations, following
[ISO 32000-1, section 14.8.4.4 and Tables 322–323](https://opensource.adobe.com/dc-acrobat-sdk-docs/standards/pdfstandards/pdf/PDF32000_2008.pdf).
The writer does not insert the PDF 2.0 `/Ref` association key into a PDF 1.7
structure dictionary.

Semantic byte ranges survive token fallback, wrapping, table layouts and
paragraph alternatives without changing shaping boundaries. Where a shaped
run contains several roles, its already selected glyphs, pair adjustments and
positioned offsets are emitted with separate marked-content spans. The
integration tests compare normalized paint operators against the same prepared
layout without the semantic sidecar, and check chunked/monolithic parity,
fitting probes, equal-looking cached content, PDF/A and book language sections.

Use `render_pdf`, `render_pdf_document`, their reported/profiled/PDF/A variants,
or the book APIs to retain this source provenance. The explicitly low-level
`pdf::render` family accepts an already prepared document representation and
does not prepare or reconstruct source notes; a plain caller-prepared AST
cannot prove which ordinary text or links originally came from footnote syntax.
That compatibility path retains its existing destination-name convention for
page-note placement. Source-facing rendering, fitting, verification and anchor
audits instead use the typed citation inventory: an authored `#fmd-fn-N` link
cannot steal a note's first-citation page or manufacture a return destination.
Leading alert paragraphs retain these roles across their temporary layout clone.

## List labels and numbering

Each source list item has one `/Lbl` before its `/LBody`. Bullets, ordinal
numbers and checked/unchecked task markers belong to that label; ordinary body
text that happens to resemble a marker is still body text. Label identity is
carried from layout rather than inferred from painted characters.

The enclosing `/L` records `/A << /O /List /ListNumbering /Disc >>` for ordinary
bullets and `/Decimal` for numbered lists. Lists containing custom task markers
use `/None`, including mixed task/ordinary lists. Nested lists keep their own
numbering metadata and source hierarchy.

A continued item retains the same label and body across pages. If an oversized
ordinal itself wraps, its physical fragments belong to one logical `/Lbl`.
Empty items retain an empty `/LBody` without invisible filler text. Headings,
code, tables, quotes and linked figures at the beginning of an item preserve
their own body roles instead of absorbing the label. This structure applies to
ordinary and PDF/A output and to both PDF emitters.

`tests/pdf_list_accessibility_test.rs` decodes emitted structures, marked-content
ownership and ToUnicode text to verify these cases; the list geometry suites
independently cover their placement and pagination.

## Known limitations

These are deliberate consequences of a small, line-based, dependency-free writer.
They are safe (they never corrupt the tree) and are candidates for future beads.

- **Complex table headers and spanning cells** are not represented by the
  Markdown AST. Header associations cover its single column-header row; row
  headers, multiple header tiers, and row/column spans remain unsupported.

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
between monolithic and chunked emission. Mixed text/image/formula links retain
one `/Link` across pages, unlinked objects keep ordinary paragraph ownership,
and missing internal destinations create no clickable annotations.
Separate same-target links retain intervening prose in the parent reading order.
It also exercises adaptive table
scaling below the ordinary body-table floor inside a small-type footnote.

`tests/pdf_table_accessibility_test.rs` verifies complete sparse and blank
matrices, normalized ragged host tables, sorted ID lookup and column-specific
header associations, distinct cached tables, wrapping and repeated headers,
list/footnote ownership, linked images with preserved alt text, PDF/A output,
annotation back-references to `/Link` owners, linked formulas and images under
repeated headers, distinct same-target links in separate cells, deterministic
bytes, and parity between emitters.
Same-target links within a cell also retain intervening content in `/K` order,
including when the first link wraps across pages and its header repeats.

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
