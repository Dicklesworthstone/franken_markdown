# Unicode-aware, cancellable semantic reading search

The `./flow-reader` API searches the admitted engine reading snapshot, not a
second Markdown parse. Its live Canvas demo now uses asynchronous Find, with
**Ignore case (Unicode)** and **Whole words only** controls. Source editing and
source replacement are separate workflows and are not changed by reading Find.

```js
const document = await readFlowDocument(session, { token: session.token });
const controller = new AbortController();
const result = await document.findAsync("STRASSE", {
  caseInsensitive: true,
  wholeWord: true,
  maxMatches: 1000,
  signal: controller.signal
});
if (result.matches.length) {
  const match = result.matches[0];
  reader.selectMatch(match); // Native Copy selects the original "Straße".
  const location = document.locate(match.nodeIndex);
  host.reveal(location); // Fence unsubmitted source edits in the host first.
}
```

`find` and `findAsync` share the same literal, non-overlapping matching rules.
Default searches remain case-sensitive. The existing `asciiCaseInsensitive`
option still folds only A-Z; it cannot be combined with `caseInsensitive:true`.

## Meaning and offsets

Unicode-insensitive search uses frozen Unicode **15.1.0 default full C+F case
folding**, without Turkic tailoring. Examples include sharp-s/SS, ligature/letter
expansions, Greek final sigma and supplementary-plane cased letters. It does not
normalize accents, strip marks, transliterate or use ambient locale rules.
Precomposed `é` and decomposed `e` plus acute therefore remain different.

A match starts and ends at complete original scalar boundaries. `ss` may match
`ß`, and `ffi` may match `ﬃ`, but `s` cannot match half of `ß`. Returned offsets
remain UTF-16 positions in the ORIGINAL unsplit reading leaf, not in a folded
copy, Markdown source or Canvas glyph fragment. Existing authenticated match
objects and native DOM Range selection work across styled spans unchanged.

Whole-word matching checks adjacent Unicode letters, marks, numbers, connector
punctuation and joiners U+200C/U+200D. These characters keep a word together;
other neighboring characters delimit it. This is an explicit adjacency policy,
not UAX #29 segmentation or language-sensitive word breaking. JavaScript Unicode
property classifications follow the host engine; case-fold data is separately
frozen. Search never joins different paragraphs or table cells, and never counts
both a container transcript and its children.

## Work, cancellation and bounds

KMP prefix state and a ring mapping folded scalars to original offsets use
query-sized auxiliary memory, rather than retaining a folded document copy.
Queries are at most 1,024 UTF-16 units; at most 1,000 matches are retained. The
`truncated` flag is true only after finding an actual additional eligible match.
The existing reading-snapshot admission budgets still apply.

`findAsync` yields real event-loop turns around every 4,096 original UTF-16 units
or node-work units, including inside one large leaf. Small searches need no
timer. Resumed work checks cancellation, source revision, layout revision and
session disposal. Rejections publish no partial match array. No worker request,
network access, clipboard permission or source write is involved. This is not a
hard millisecond deadline or preemption of native rendering/DOM construction.

The live controls coalesce same-turn inputs and abort superseded searches. Late
successes and failures cannot replace the newest results or disposed views.
Input-method composition waits until composition ends. Enter/Shift+Enter during
search retains at most one requested direction, applied only to that exact
search; source changes, pause, replacement and disposal discard it. Completed
matches and native selection survive scroll-only paints. Pending semantic
collection leaves old text readable while navigation is paused. A missing
`findAsync` implementation is an explicit package mismatch, not a blocking
fallback. Host source/layout fences still apply at navigation time and after
focus callbacks.

## Verification and regeneration

```sh
node --test wasm/flow_reading.test.mjs wasm/flow_reading_search.test.mjs
python wasm/tests/run_flow_reading_search.py --chromium /usr/bin/chromium
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext --lib ES2022,DOM wasm/flow_reader_search_types_test.mts
python scripts/generate-reading-casefold.py
```

The generator requires Python's Unicode database 15.1.0 and changes only the
marked generated region in `wasm/flow_reading.mjs`. Its Unicode data license
travels inside that already-shipped runtime module; no new package dependency or
assembly path is required. All 1,530 nonidentity production mappings were checked
against Python's casefold database over every Unicode scalar.

Node tests use explicit native semantic-page fixtures and include 300 seeded
comparisons with independent whole-string matching/range logic. Chromium tests
execute the real reader, controls, DOM, Range selection and event loop; only
native semantic pages and selected delayed-result scenarios are fixtures. These
checks do not execute Rust, fonts, the WASM binary or the complete worker demo.
