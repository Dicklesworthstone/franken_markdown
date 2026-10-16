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

## Resolve a disputed word without discarding surrounding edits

The additive `merger.planParagraphs(base, local, incoming)` review API can expose
several explicit conflicts inside one source line. Choosing a word version keeps
non-conflicting word changes from both sides. For example, local changes `red`
to `green` and `blue` to `gold`, while incoming changes `blue` to `small` and
`tall` to `short`. Selecting the local disputed word produces `green ... gold ...
short`, not the complete local paragraph that would lose the incoming `short`.

The original callable merger and `.plan()` retain their existing resolution
scope. The connected-folder controller opts into `planParagraphs` for its
**Resolve conflicting regions; keep other edits** review. The source-tree planner
uses the same API for the local library's existing region resolution. Whole-file
version choices deliberately remain whole-file choices. No automatic approval,
source installation, disk write or storage save is introduced.

A partial paragraph plan requires identical forward/reverse edit coordinates,
not merely identical clean output. Changed repeated words, ambiguous whitespace
alignment, conflicting line terminators and insufficient work retain complete
line choices. Both automatic and partial refinement spend the same allowance.
The aggregate 256-region ceiling includes word and line regions; a next nested
plan that will not fit remains one whole-line conflict rather than being truncated.

Word regions retain the original zero-based `startLine` / exclusive `endLine`
containing range, and add zero-based `startColumn` / exclusive `endColumn` UTF-16
code-unit offsets within that original line. These are source offsets, not visual
columns or grapheme counts. The folder review labels them explicitly. Conflict
text is complete, immutable and exact; display excerpts still grant no authority.
The same `resolve([{index, choice: "local" | "disk" | "edited", source?}])`
contract handles mixed line/word regions. Each region requires a decision;
manual replacements preserve existing Unicode and aggregate-output validation.

```sh
node --test wasm/tests/book_directory_merge*.test.mjs
```

For these changes, 77 tests passed together: the earlier 50 plus 17 partial-plan
and 10 controller cases. The 27 new cases were also executed on `1a60a56` sources:
19 failed and 8 passed before the region implementation. New coverage includes
multiple disputes, side-order reversal, 200 generated paragraph scenarios,
UTF-16 offsets, mixed word/line indices, source-tree roles and limits, and the
nested region ceiling. All previously reconstructed existing tests remain
byte-identical; no old assertion was weakened.

Controller tests copy/import the complete production UI and merge modules without
rewriting their source. The DOM, source host and directory-session boundary are
explicit adapters: they exercise displayed regions, mandatory choices, exact
approval output, cancellation, source input, project replacement, import/busy,
suspension, disposal and composition. They do not execute the real disk session,
OS file handles, browser APIs or IndexedDB. Library source-tree resolution is
executed; its full UI/storage session is not. Those boundaries, full repository
and package gates, and compiled Rust/WASM qualification remain unrun.
