# Complete-section search in exported books

The offline search page embedded in native and ZIP HTML book exports now matches
all query terms across the entries in a section, not just a single paragraph.
A heading, prose paragraph, code block, table row and referenced-note entry may
supply different terms. A section is the contiguous run of entries with one
emitted page/anchor identity; coverage is reset at each section or chapter.
The existing inherited chapter-title matching remains available.

Quoted phrases must occur within one real entry or chapter title. The engine
never concatenates paragraphs and thereby invents an exact phrase. Matching
continues to normalize whitespace and case, including the existing sigma policy.

Existing single-passage and exact-heading rankings are preserved. Sections that
need several passages to satisfy a query rank after complete single-passage
matches, with stable source-order ties. All matches count toward the reported
total; only the best 200 results are retained and paginated 25 at a time.

Each result carries at most eight source-coordinate witnesses, bounded by the
query-term limit rather than paragraph count. Nearby hits in one field share an
excerpt. The production controller shows these bounded excerpts as inert text,
so readers see the evidence from multiple paragraphs without rescanning entries
or introducing HTML interpretation. Returned links still use actual emitted
anchors, never a new JavaScript slug algorithm.

The implementation retains chunked normalization, lazy source-coordinate maps,
cooperative cancellation, disposal fencing, and the existing admission limits.
It adds no network access, storage, package dependency, or Rust API.

Verification: the eleven new section tests reproduced nine failures against the
previous shipped JavaScript and pass after the change. The existing ordinary
search and streaming suites also pass, for 34 Node tests total. The section suite
includes an independent whole-section membership oracle over 120 generated books.

```sh
node --check src/book/site_search.js
node --test wasm/tests/book_site_search.test.mjs \
  wasm/tests/book_site_search_streaming.test.mjs \
  wasm/tests/book_site_search_sections.test.mjs
```

These execute the JavaScript used by the exported page. DOM behavior is exercised
with test doubles, not a browser engine. Rust builds and generated WASM/ZIP exports
were not executed in the authoring environment, which has no Rust toolchain.
