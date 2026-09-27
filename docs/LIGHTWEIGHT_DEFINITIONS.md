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

Block-tree nodes, list items, and table rows share the structure budget described
in `LIGHTWEIGHT_RENDER_LIMITS.md`; there is no unbounded secondary AST or parsing
pass. Depth exhaustion preserves the existing escaped `<pre>` fallback rather
than collecting hidden definitions. Resource exhaustion still throws
`FMD_RENDER_LIMIT` before returning HTML.

This remains a reduced renderer, not a claim of full CommonMark/GFM parity. The
reference scope and paragraph/code boundaries follow CommonMark 0.31.2 section
4.7 (https://spec.commonmark.org/0.31.2/#link-reference-definitions). Footnotes
and callouts are extensions. Lazy blockquote continuation and full Unicode
case-folding of labels remain outside this change.

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
node --check src/interactive_renderer.js
```

The established renderer test entry point imports the new definition tests.
Fixtures cover mixed nested containers, first-line and continuation definitions,
code shielding, paragraph preservation, duplicate precedence, task/list
structure, nested/cyclic notes, unique IDs, URL safety, and depth/structure
budgets. No external runtime dependency or network access is introduced.
