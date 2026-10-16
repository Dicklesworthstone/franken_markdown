# Preserve independent edits inside a paragraph

Source-folder **Reconcile external edits** and local-library source reconciliation
share `book_source_merge.mjs`. They can now combine independent word edits to a
single physical source line, instead of requiring one whole paragraph to win.
The ordinary whole-file external refresh remains unchanged. Applying a review
still requires the existing explicit approval and source/disk revision checks;
this algorithm never writes files, installs projects, or authorizes resources.

For example, from `The red fox follows the blue bird.`, an editor change to
`quick fox` and an incoming change to `green bird` produce
`The quick fox follows the green bird.`. Separate insertions and deletions can
also combine. A successful paragraph merge is retained alongside independent
line edits, even when another region still needs an explicit resolution.

## Deliberately conservative text policy

Only a conflict group covering one original physical line is refined. Its two
versions must each remain a single line (possibly with a final line terminator).
Changes that split/join lines retain the existing line-level conflict behavior.
Tokens are complete non-horizontal-whitespace runs, horizontal whitespace runs,
and exact newline tokens. CRLF is indivisible. No word, combining sequence, emoji,
identifier or unspaced Markdown destination is spliced into a third spelling.
There is no Unicode normalization, locale-dependent segmenter or Markdown parser.
BOMs, tabs, mixed line endings and an absent final newline remain exact.

Refinement uses the existing bounded LCS/sparse diff in both directions without
common-end trimming, checking all four forward/reverse edit combinations. All
must yield the same clean text. Changed repeated non-whitespace tokens have no
reliable occurrence identity and keep the line explicit, even when the extreme
alignments agree. Unchanged repeated words remain usable context. Competing
replacements or insertions do not acquire a default winner.

This is a deterministic source merge, not proof that combined prose is sensible
or Markdown links are correct. Inspect the reviewed result before publishing.

## Bounds and compatibility

Token scans and retained tokens/reversals use the same two-million-unit allowance
as line matrices and sparse frontiers, shared across the entire review. There is
no fresh budget for each paragraph and no raised limit. An expensive, ambiguous
or over-budget refinement preserves the complete original line conflict and its
existing manual/version choices. Source-tree admission still enforces UTF-8 byte,
path, role and total-size limits after reconciliation.

The callable three-field result, `.plan()` interface, conflict line coordinates,
immutable regions and `resolve(choices)` contract are unchanged. Neither source
membership/order reconciliation nor settings/resource ownership is reimplemented.
No new runtime dependency, package entry point or build policy is introduced.

## Executed verification

```sh
node --test wasm/tests/book_directory_merge.test.mjs \
  wasm/tests/book_directory_merge_hunks.test.mjs \
  wasm/tests/book_directory_merge_paragraphs.test.mjs \
  wasm/tests/book_directory_merge_paragraph_oracle.test.mjs
```

50 tests pass: 25 byte-identical existing merge/region tests and 25 new tests.
The first 17 new cases were run against the original engine: 13 failed before
implementation. New coverage includes Unicode/newline preservation, insertions,
deletions, repeated-word ambiguity, a 5,000-word sparse paragraph, shared work
exhaustion, size refusal, complete source-tree role/membership reconciliation,
and 500 seeded paragraph replacements in both API forms and side orders.

An independent tiny-input oracle checks 24,360 two-sided repeated-word triples.
Every merge admitted by the implementation agrees with every shortest matching
coordinate interpretation enumerated by that oracle. This finite test is not a
proof about every possible manuscript or an assertion of semantic correctness.

Tests import the complete production merge module normally; no diff or tree
merger is mocked. Controller/browser/IndexedDB/OS-filesystem acceptance, compiled
Rust/WASM rendering, package assembly and the full repository suite were not run
for this algorithm change. Existing callers were inspected, not simulated as
browser end-to-end proof. Beads was read but not mutated: `br` is unavailable in
this execution environment.
