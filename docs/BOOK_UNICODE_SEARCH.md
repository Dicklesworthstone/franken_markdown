# Canonically equivalent Unicode in offline book search

The exported HTML book search compares canonically decomposed text after its
existing case and whitespace normalization. Composed and decomposed spellings
such as café and cafe plus a combining acute accent therefore match each other.
The same path supports canonical mark reordering and Hangul decomposition.
Original text, snippets, chapter filenames and emitted anchor IDs are unchanged.

This is not accent stripping, transliteration, locale-specific collation or
compatibility folding. A query requesting an accent still requires that mark;
a compatibility ligature is not automatically expanded to separate letters.
The existing literal-substring and quoted-phrase semantics remain in place.
Canonical matching also applies to terms distributed across section entries,
as described in BOOK_SECTION_SEARCH.md.

## Bounded normalization with real source coordinates

Normalization can both expand characters and reorder marks. Source maps must
therefore follow the normalized output rather than assuming each input character
contributes a fixed, monotone sequence of output offsets. For hit chunks, the
implementation queues original source positions for each normalized scalar,
then consumes them in stable occurrence order to map the bulk-normalized output.
Misses do not allocate these per-scalar source maps.

Source normalization operands remain bounded to 4096 UTF-16 code units. A chunk
reserves 256 units to finish a combining sequence rather than splitting across
an unsafe normalization boundary. A continuation that cannot fit produces an
explicit work-limit error, not silently inconsistent matching or an unbounded
normalization call. Cancellation and error recovery retain their existing paths.
No network access, storage, dependency, Rust API or feature flag is added.

## Executed verification

```sh
node --check src/book/site_search.js
node --test wasm/tests/book_site_search*.test.mjs
```

All 50 Node tests passed: 30 unchanged ordinary, streaming and lifecycle tests,
11 section-search regressions, and nine Unicode regressions. Six of the new
Unicode tests fail against the preceding production script. Unicode coverage
includes both-direction canonical matches, reordered-mark source offsets,
chunk boundaries, explicit combining-work refusal, cancellation, lazy mapping,
and comparison against independent whole-string normalization on 90 generated
large entries. The section suite separately checks 120 generated books against
an exhaustive section-membership oracle.

Headless Chromium additionally passed 15 checks using the exact production
script in an in-memory HTML harness with the export page's DOM IDs and content
security policy. Checks covered canonical cross-paragraph search, original-text
snippets, all result pages, Escape cancellation, error recovery and inert markup.
No network request was attempted and no browser script error occurred.

This browser harness is not a fresh Rust-generated book export. File-URL
navigation was blocked by the environment's browser policy; the browser checks
used in-memory content instead. Rust compilation, generated WASM, and end-to-end
Rust-generated ZIP exports were not run because no Rust toolchain is installed.
