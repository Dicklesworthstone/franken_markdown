# Container-aware standalone Markdown definitions

The standalone JavaScript preview parses a bounded block tree before rendering
inline text. Link-reference and footnote definitions are collected while parsing
those blocks, not by a separate line-scanning pass. Native Rust/WASM rendering
and publication exports are unchanged.

Definitions at the beginning of list items, ordered items, blockquotes, and
nested combinations resolve throughout the document, including forward links,
reference images, and notes. Actual fenced/indented code and table cells are
never scanned for definitions. A reference-shaped continuation of an ordinary
paragraph remains paragraph content. Removing a definition does not reconstruct
source or change the surrounding list, callout, heading, or table boundaries.

First definitions win in source order. A footnote reserves its identity before
its child blocks are parsed, so a nested duplicate cannot replace its containing
note. Notes are rendered once through the existing work queue; cycles append
references, not recursive render work. Headings and backlink IDs are allocated
at rendering time, not during definition collection. Unused notes do not render
visible content, although their valid block-level reference definitions remain
document-global.

Footnote IDs compare exactly, including case and Unicode spelling. `[^A]` and
`[^a]` address separate notes, while a wrong-case reference with no exact
definition remains literal text. Repeated uses of the same ID share one note
and retain all backlinks.

A note retains a complete run of blank lines when the next nonblank line has
at least four columns of indentation. Removing those four columns preserves
additional indentation, rich Markdown blocks, and literal blank lines inside
code. Mixed tabs and spaces follow four-column tab stops. A blank run followed
by outside content ends the note without consuming that content.

Block-tree nodes, list items, and table rows share the structure budget described
in `LIGHTWEIGHT_RENDER_LIMITS.md`; there is no unbounded secondary AST or parsing
pass. Depth exhaustion preserves the existing escaped `<pre>` fallback rather
than collecting hidden definitions. Resource exhaustion still throws
`FMD_RENDER_LIMIT` before returning HTML.

This remains a reduced renderer, not a claim of full CommonMark/GFM parity. The
reference scope and paragraph/code boundaries follow CommonMark 0.31.2 section
4.7 (https://spec.commonmark.org/0.31.2/#link-reference-definitions). Footnotes
and callouts are extensions. Reference labels use the complete pinned Unicode
full fold described in [REFERENCE_LABELS.md](REFERENCE_LABELS.md).

## Document-prefix boundaries

A leading Unicode BOM is ignored before classifying the first Markdown block,
so it cannot turn a fence, heading or reference definition into ordinary text.
BOM characters within document content remain literal. The edited source is
not rewritten by rendering.

Valid leading frontmatter is excluded before collecting blocks, references or
footnotes. Admission follows the native parser's existing subset: exact `---`
opening and closing lines, at least one nonempty assignment key using `=` or
`:`, optional comments, ordinary/quoted values, and indented `|` or `>` block
scalars with optional `+`/`-` chomping indicators. Unknown keys remain metadata.
Scalar indentation is validated, including partial dedents and over-indented
leading blank lines. This is the project's metadata grammar, not full YAML.

The prefix must begin at the document start. LF, CRLF and lone CR each end one
logical line, including when they are mixed within a document. Empty,
comment-only, malformed and unclosed candidates retain their entire ordinary
Markdown content. Later metadata-looking blocks keep their normal Markdown
meaning. Definitions and note-shaped lines inside valid metadata cannot leak
into the body or defeat a body's real definitions.

The fallback applies these content boundaries without adding metadata-based
render configuration. The prefix cursor uses constant-size state and shares
the existing source/work limits; it does not build a metadata AST or publish
speculative definitions.

## Paragraph continuation and container ownership

Quotes and list items retain unmarked or dedented lines only while their current
leaf is an open paragraph, including paragraphs inside nested quotes and lists.
A definition-shaped lazy line remains visible paragraph text; it cannot become
a global link definition. Completed headings, tables, footnotes, and literal
code blocks leave following outside text and real definitions in their original
scope. Explicitly indented or quoted paragraphs can resume lazy continuation
after a closed block or a blank line.

The collector tracks a bounded container stack and current leaf in one pass.
It never reparses accumulated prefixes. Multiline reference candidates use
constant-size recognition state, so a heading or fence marker inside a title
does not change container ownership. Fences share closing rules with block
parsing, and retain their literal blank lines. Tab indentation uses four-column
stops when removing list and quote containers.

Paragraph interruption follows the same rules inside and outside containers:
empty list markers and ordered markers starting above one do not interrupt an
open paragraph. Properly indented nested lists retain their own start numbers.
Setext underlines apply to their complete paragraph; removing a container cannot
promote an unmarked lazy line into a heading or nested list. Raw HTML and math
retain the standalone renderer's existing escaped-text behavior.

## Table cell and block ownership

Matched code spans retain their internal pipes as part of the same cell,
following the native renderer's established extension. Delimiters must have
the same maximal backtick-run length; an unmatched opener cannot hide later
columns. Escaped final pipes remain cell content, and actual final separators
remain optional. Code delimiter positions are indexed once per row and their
allocation shares the existing structure budget.

Cell source stays intact until inline parsing, preserving escaped reference
labels and normal destination/title decoding. The table-specific `\|` decoding
inside code spans propagates through emphasis, links and image labels; ordinary
prose code is unchanged. Existing alignments and ragged-row behavior remain.

Headers, body rows and container tracking share admission rules. Supported
block openers retain their priority, including pipe-bearing indented code and
footnote definitions after a table. A reference-shaped pipe row remains literal
cell content rather than defining a global link. Subsequent fences keep their
definitions literal, and real definitions after those fences remain usable.

## Multiline reference definitions

Reference definitions use their own bounded cursor, separate from the inline
link destination parser. Labels can continue across lines and are limited to
999 Unicode characters. Destinations may start on the following nonblank line;
angle-delimited empty destinations (`<>`) and balanced/escaped parentheses are
supported. Titles accept double quotes, single quotes, or parentheses and can
continue across nonblank lines. Escaped title delimiters and HTML entities
retain the existing single-escape rendering policy.

A malformed title on the destination line invalidates that definition. A
malformed title on a following line does not discard the already-valid URL:
the following line remains available for normal block parsing. Blank lines
cannot be consumed as part of a label, destination, or title. The cursor never
concatenates a remaining-document suffix, and all speculative scans share the
work budget. These rules apply equally inside supported containers and notes.

A focused development comparison against markdown-it-py checks reference
link/image destination and title resolution; it is not a general conformance
suite and adds no runtime dependency. The Chromium smoke harness exercises the
production renderer with a minimal DOM host, not the complete workspace
controller. Both optional harnesses and recorded results are in the change
bundle's validation directory, not required by the dependency-free Node tests.

## Tests

```sh
node --test tests/interactive_renderer.test.mjs
node --test tests/interactive_definitions.test.mjs
node --test wasm/tests/interactive_container_boundaries.test.mjs
node --test wasm/tests/interactive_footnote_identity.test.mjs
node --test wasm/tests/interactive_document_prefix.test.mjs
node --test wasm/tests/interactive_table_ownership.test.mjs
node --check src/interactive_renderer.js
```

The established renderer test entry point imports the new definition tests.
Fixtures cover mixed nested containers, first-line and continuation definitions,
code shielding, paragraph preservation, duplicate precedence, task/list
structure, nested/cyclic notes, unique IDs, URL safety, and depth/structure
budgets. No external runtime dependency or network access is introduced.
