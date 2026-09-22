# Atomic source-edit batches

Both `./flow` and `./flow-worker` expose `editMany`. It applies a multi-cursor,
replace-all, or editor transaction to one captured source revision and performs
one native transaction rather than rendering after each individual replacement.
The worker form keeps parsing, shaping and publication off the caller thread.

```js
import { createWorkerFlowSession } from "@franken-suite/franken-markdown/flow-worker";

const session = await createWorkerFlowSession("first\n\nlast");
try {
  if (!session.supportsEditBatches) {
    throw new Error("Rebuild the worker and WASM package from matching source");
  }
  const before = session.token;
  const after = await session.editMany(
    [
      { start: 7, end: 11, replacement: "ending" },
      { start: 0, end: 5, replacement: "beginning" },
    ],
    { expectedRevision: before.revision },
  );
  console.log(await session.getSource()); // beginning\n\nending
  const snapshot = await session.snapshot({ token: after });
  // Consume snapshot.items using the host's presentation layer.
} finally {
  session.dispose();
}
```

The synchronous session takes the same edits/options and returns its token
without a Promise. Its source is available as `session.source`.

## Transaction and coordinate rules

All offsets are UTF-16 code units in the ORIGINAL Markdown at `expectedRevision`,
with an exclusive `end`. They are not byte offsets, rendered-text positions or
offsets after an earlier edit. Input order need not be sorted. Native conversion
rejects a position inside a surrogate pair instead of rounding it.

Adjacent replacements are accepted. Overlapping replacements and insertions
inside a replaced range are rejected together. Insertions at a replacement's
start precede it; insertions at its end follow it. Same-position insertions keep
their caller order. An empty replacement deletes; an empty range inserts.

The native core validates every edit, computes the final source size, and renders
one candidate. Only a successful candidate replaces source, display, asset state
and revisions. Empty and collectively identical batches do not advance revisions.
A recoverable validation/layout failure publishes nothing. No intermediate
source or speculative token is published by the worker client.

## Admission, ownership and compatibility

A batch contains at most 4,096 edits. Combined replacement UTF-8 bytes and final
source are each limited to 4 MiB in the browser API. Malformed UTF-16 strings are
rejected individually before joining, so two invalid replacements cannot repair
one another into a valid surrogate pair. The same admission function is used by
the synchronous facade and both sides of the worker protocol.

Worker input is normalized into owned data records before enqueue returns.
Later mutation of the caller's array, objects or options cannot change a queued
transaction. The queue charges every replacement string and operation record;
its configured byte/count limits apply in addition to source admission limits.
These limits do not claim to bound the complete browser or WASM heap.

`reuseAssets` defaults to false. Set it to true only when the host verifies that
external bytes, base URI and authorization remain unchanged. Native block reuse
still requires exact source and AST verification; an unchanged URL alone is not
authorization. This API does not fetch assets or authorize navigation.

`supportsEditBatches` is negotiated at worker creation and is false for legacy
workers/native packages. Unsupported batches fail with `UNSUPPORTED_WASM_PACKAGE`;
there is no loop over `edit` and no alternate full-source write fallback.

## Cancellation and errors

Worker calls accept a final `{ signal, timeoutMs }`. Before dispatch, cancellation
removes the whole batch and releases queue capacity. After dispatch, cancellation
terminates the dedicated worker because synchronous native work cannot process a
cancellation message. The active call rejects with `ABORTED`/`TIMEOUT`; queued
calls reject with `SESSION_LOST`. This is NOT rollback. Hosts retain authoritative
source and explicitly recreate a lost session. Nothing is replayed automatically.

Ordinary `STALE_REVISION`, `INVALID_SELECTION`, `OVERLAPPING_EDITS`,
`BUDGET_EXCEEDED` and layout refusals keep the previous acknowledged session usable.
Malformed capability or mutation acknowledgments terminate the worker instead of
publishing untrusted state. Await a successful mutation before using its token;
two queued batches with the same expected revision are not silently rebased.

## Verification

`node --test wasm/flow_edit_batch.test.mjs wasm/flow_edit_worker.test.mjs` covers
both boundaries. Worker tests execute real Node worker threads, the production
RPC, request normalization and synchronous facade, including a blocked worker.
Only native rendering is doubled; these are NOT compiled-WASM rendering proofs.
Rust transaction tests cover the core's actual shaping and rollback separately.
