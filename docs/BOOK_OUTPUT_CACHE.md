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

This cache owns one exact source capture. Any chapter/include text, path, order,
role partition, publishing setting, paper/margin, running matter, font, image, or
resource removal change invalidates all cached outputs. Unknown future options
disable output reuse. Misses use the existing native source-update/replacement
and old-binary fallback paths. Resource revocation cannot resurrect old output.

## Ownership and limits

Default ceilings are **32 MiB of retained output bytes**, **16 MiB per output**,
and **64 outputs**, using least-recently-used eviction. Internal embeddings may
lower these limits with the adapter's third argument; zero disables retention.
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
`cacheStats` exposes payload bytes, entry count and cumulative lookup counters,
not source text or resource keys. Nothing is persisted or sent to a server.

Explicit clear, cancellation, source/render/protocol failure and worker retirement
release the cache. A pending hit has an asynchronous lifecycle check just like
a native export; clearing cannot let an old response reappear after cancellation.

## Verification

```sh
node --test wasm/tests/book_publication_output_cache.test.mjs
```

54 executed tests passed with no failures or skips. They import the complete
production retained renderer/cache normally, using explicit deterministic native
engine and preview-codec doubles. Tests cover every normalized option dependency,
source and structural changes, immutable/transferable result ownership, shared
site/preview work, LRU/byte limits, old binaries, failures and cancellation. A VM
test executes the complete actual worker entrypoint with explicit engine, codec
and installation adapters and the real cache module. The test filename is covered
by the existing `book_publication*.test.mjs` package-gate wildcard.

Exporter-call counts demonstrate work elision, not measured Rust/WASM speedup.
Native rendering/byte parity, real-browser acceptance, the older publication
suites and a full assembled-package build were not executed in the authoring
environment. The broader repeated-render performance bead remains open.
