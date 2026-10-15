# Bounded retained-worker output reuse

The book worker now wraps its native retained book in `createCachedBookRenderer`
from `wasm/book_retained.mjs`. This is enabled in the actual worker entrypoint,
for both retained publication and retained-preview modes. One-shot rendering
and the public native `BookOutput` helpers are unchanged.

## Work avoided

For an exactly unchanged normalized capture, revisiting PDF, EPUB, site ZIP,
full-book preview, link-check bytes, or a selected chapter can reuse the previous
successful output instead of invoking that exporter again. Chapter previews are
keyed by their selected index and keep the native/fallback capability receipt.

The full-book preview still uses `renderBookPreview`, the existing production
codec. Its site input comes from the same cached native site export used by site
publication: preview then site, or site then preview, needs only one site export
while that entry remains resident. No second ZIP reader, Markdown parser, include
resolver, layout engine, or renderer was added.

Source changes select a different exact full-book key; a prior output is never
used merely because the selected chapter itself looks unchanged. Keys include
all chapter/include text, paths, ordering and role partition, preserving BOM,
line endings and Unicode without hashes or concatenated ambiguous keys.

A bounded history lets undo/redo revisit earlier successful source states without
repeating their exports or replaying intermediate native source transactions.
Settings, paper/margins, running matter, fonts, images and resource removals still
invalidate **every** current and historical output. Unknown future options disable
all reuse. Restoring earlier settings after revocation cannot resurrect discarded
outputs. Clearing on project replacement remains the host's responsibility, as
with the underlying retained worker; source history grants no new authority.

A history hit does not pretend to mutate the retained native handle. Every miss
supplies the complete current capture to the existing retained renderer, whose
baseline tracks the actual native state. It therefore computes the correct next
update even after the transport has returned an older cached source state. Native
source revisions and fresh transport request IDs remain separate. Misses preserve
existing source-update/replacement validation and old-binary reconstruction.

## Ownership and limits

Default ceilings are **32 MiB of retained output bytes**, **16 MiB per output**,
and **64 outputs**, using one global least-recently-used output eviction order.
At most **eight source-state keys**, including the current one, are retained.
Historical keys additionally share **8,388,608 UTF-16 code units** of source/path
text (16 MiB at two bytes per code unit), not per-snapshot allowances. Only small
key containers are copied; the exact immutable strings may be shared. Empty
historical entries release their source charge when their last output is evicted.

The currently admitted source is excluded from the *history* allowance, just as
current source/resources and native input are excluded from the output allowance.
A very large current book can still benefit from repeat exports, but is not kept
as history when switching away if its key exceeds the historical budget. No source
is truncated or rejected merely because it is not cacheable.

Internal embeddings may lower these ceilings with the adapter's third argument.
Zero output budgets or `maxSnapshots: 0` disable output retention. `maxSnapshots: 1`
or `maxHistoryUnits: 0` keeps ordinary current-capture caching without history.
Large outputs still render normally and do not evict useful small entries merely
because they cannot be admitted. A failed optional cache allocation does not
turn a successful native export into a failed publication.

Entries retain only owned byte arrays and primitive wire metadata, never native
result objects, helper closures, WASM-memory views, file handles or caller grants.
Every cache hit returns a separate owned byte array. Mutating or transferring a
returned buffer therefore cannot corrupt another request's output. Existing
worker output-size, chapter-envelope and source-revision validation still runs
on every response; no old request IDs or save receipts are cached.

These limits do not bound the WASM heap, current source/resources, transient
export allocations, preview decoding, or output copies retained by consumers.
`cacheStats` exposes output bytes, entry/snapshot counts, historical source-unit
charges and cumulative lookup counters, not source text or resource keys. Nothing is persisted or sent to a server.

Explicit clear, cancellation, source/render/protocol failure and worker retirement
release the cache. A pending hit has an asynchronous lifecycle check just like
a native export; clearing cannot let an old response reappear after cancellation.

## Verification

```sh
node --test wasm/tests/book_publication_output_cache.test.mjs
```

69 executed tests passed with no failures or skips. They import the complete
production retained renderer/cache normally, using explicit deterministic native
engine and preview-codec doubles. Tests cover every normalized option dependency,
source and structural changes, immutable/transferable result ownership, shared
site/preview work, LRU/byte limits, old binaries, failures and cancellation. Added
history cases cover lazy native synchronization after hits, structural undo,
Unicode/key-boundary distinctions, independent historical source budgets, large
current books, revocation, and 500 seeded source/format/history transitions checked
against fresh uncached fixture outputs. A real Node worker-thread case executes
the production renderer/cache and transfers output buffers across the thread
boundary; its native engine is an explicit double, not Rust/WASM. A VM
test executes the complete actual worker entrypoint with explicit engine, codec
and installation adapters and the real cache module. The test filename is covered
by the existing `book_publication*.test.mjs` package-gate wildcard.

Exporter-call counts demonstrate work elision, not measured Rust/WASM speedup.
Native rendering/byte parity, real-browser acceptance, the older publication
suites and a full assembled-package build were not executed in the authoring
environment. The broader repeated-render performance bead (`...-z57k`) remains open; this is
the worker-output slice, not native dependency-graph caching or a native benchmark.
