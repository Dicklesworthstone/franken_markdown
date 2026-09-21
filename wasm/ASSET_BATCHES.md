# Atomic image completion

Both `createFlowSession` and `createWorkerFlowSession` expose
`supportsAssetBatches` and `provideAssets(results)`. A successful nonempty batch
resolves all supplied pending images with one candidate parse, one complete
layout pass, and one layout-revision increment. It does not change the source
revision or invalidate other in-flight image requests. Parsing is still
whole-document, not incremental.

```js
// `session` may be a synchronous flow session or a worker session.
// These are already authorized, loaded and decoded by the host.
if (!session.supportsAssetBatches) {
  throw new Error("Rebuild the matching native/WASM package for atomic batches.");
}
const token = await session.provideAssets([
  { requestId: first.id, generation: first.generation,
    width: first.width, height: first.height, bytes: first.encodedBytes },
  { requestId: second.id, generation: second.generation,
    width: second.width, height: second.height }
]);
// Repaint and refresh reading/selection geometry against this acknowledged token.
```

Collect the complete pending inventory before delivering a batch: completion
removes requests, so paging that inventory while delivering can skip entries.
The host chooses when to flush a group. The existing `FlowImageAssets` loader
continues using individual `provideAsset` calls; it does not automatically
coalesce completions. Single-image APIs remain unchanged.

## Transactions and ownership

All IDs must be distinct, still pending, and from the current source/resource
generation. Any bad identity, dimensions, bytes, retained-payload budget or
layout failure rejects the entire batch. No valid prefix is consumed. Hosts
retain their payloads and may explicitly retry; the engine never retries or
silently replaces batching with a sequence of partial single-image writes.
An empty supported batch is a no-op. Duplicate IDs use `UNKNOWN_ASSET_REQUEST`;
stale generations use `STALE_ASSET_GENERATION`.

Absent `bytes` means dimensions only; an empty Uint8Array remains a present empty
payload. Neither is sufficient encoded image data for document export. Exact
Uint8Array subviews and Node Buffer views are copied without unrelated backing
bytes. Caller buffers are never detached. Shared and detached buffers are
rejected. Request and generation IDs remain decimal strings or bigint, never
JavaScript Number. No URLs, loaders, permissions or image decoders are added.

## Limits and worker behavior

The browser facade admits at most 1,024 results, at most 8 MiB per payload, and
32 MiB across the batch. The native retained-image limit also includes already
accepted payloads. These are payload/work limits, not a total heap ceiling.
The private WASM seam uses at most 96 KiB of canonical metadata plus one packed
payload; malformed fields, lengths, duplicate IDs and extra bytes are rejected
before per-result payload copies.

Worker requests are normalized on both sides. Queue admission charges all
payloads before copying them. The default 16 MiB pending-byte limit still
applies, and can reject a batch smaller than the native 32 MiB limit. All views
are snapshotted at enqueue. Await the returned Promise before using the new
layout token; getters expose only acknowledged state. Queued cancellation drops
the entire request before dispatch. In-flight cancellation terminates the worker
and loses the session, with no rollback claim or automatic replay.

Older native binaries or worker packages advertise no batch support and fail
with `UNSUPPORTED_WASM_PACKAGE`; ordinary reading and single-image APIs remain
available. Rebuild the package through the existing DSR path to use the new
native binding.

## Verification

```sh
node --test wasm/flow_asset_batch.test.mjs wasm/flow_asset_batch_worker.test.mjs
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext --lib ES2022,DOM wasm/flow_asset_batch_types_test.mts
```

The tests execute the real JS facade, normalization and worker transport,
including actual Node worker threads and ownership transfer. They inject an
explicit native-session double; they do not execute Rust or a generated WASM
module. Native transaction and packed-ABI regressions live beside the Rust
implementations and must be run with the repository's Rust toolchain separately.
