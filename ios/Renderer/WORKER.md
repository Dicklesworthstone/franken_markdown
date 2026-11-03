# Worker-backed flow editing

The `./flow-worker` entry point moves the persistent flow editor's WASM loading,
parsing, shaping, cache and snapshot serialization into a dedicated module
worker. It returns an asynchronous version of the existing `./flow` API. The
caller still owns document input, resource authorization and drawing; no new
Markdown parser, renderer, font discovery or image fetching is introduced.

```js
import { createWorkerFlowSession } from "@franken-suite/franken-markdown/flow-worker";

const source = "# Guide\n\n**Bold** and [a link](#guide).";
const editor = await createWorkerFlowSession(source, { viewportWidth: 360 });
try {
  await editor.edit(0, 0, "Introduction\n\n", {
    expectedRevision: editor.revision
  });
  await editor.reflow({ viewportWidth: 720 }, editor.token);
  const page = await editor.snapshot({ limit: 256, glyphs: true });
  const hit = await editor.hitTest(20, 30, page);
  // The host draws page.items and authorizes any hit.linkTarget itself.
  console.log(page.items, hit.linkTarget);
} finally {
  editor.dispose();
}
```

## Ownership and ordering

Each session owns one worker and its WASM instance; sessions do not share an
implicit pool. Importing the entry point neither starts a worker nor initializes
WASM on the caller thread. Creation rejects rather than silently running the
renderer on the main thread when workers are unavailable or cannot load.

Only one operation is dispatched at a time. Other operations stay in the local
bounded queue. Defaults are 32 active-plus-queued requests and 16 MiB of charged
argument payloads; configure `maxPendingOperations` (1..128) and
`maxPendingBytes` (1..64 MiB) in the third argument. Queue overflow rejects with
`WORKER_QUEUE_FULL` before copying binary input. The charge conservatively counts
UTF-16 string storage and binary views, not the entire browser/WASM heap.
Responses retain the Rust facade's existing page/count/serialization limits.

Options, tokens and image views are snapshotted when invoked. Only that owned
image copy is transferred: callers' buffers are never detached, and mutation
after enqueue cannot alter the eventual request. Font and asset responses are
owned copies. Shared or detached input buffers are rejected.

`revision`, `layoutRevision`, `token` and `layoutOptions` are the last acknowledged
state. **Await a successful mutation before using its new token.** Queuing two
edits with the same expected revision does not silently rebase the second edit;
it reports the resulting revision conflict. Source retrieval is asynchronous
`await editor.getSource()`, not a speculative `source` property.

UTF-16/UTF-8 domains, source envelopes, asset generations and host authorization
rules are identical to `FLOW.md`. All remotely supplied identities remain decimal
strings or bigint inputs, never lossy JavaScript Numbers.

## Cancellation and deadlines

Every async method accepts a final `{ signal, timeoutMs }` control argument.
`timeoutMs` starts when the operation is enqueued, including queue wait. Its
default is 30,000 ms, configurable per worker or request; 0 disables it.
Creation has a separate `startupTimeoutMs` (default 60,000) and optional `signal`
in the third argument. That signal applies only until creation settles.

Cancellation has two deliberately different outcomes:

- **Not dispatched:** abort or timeout removes that queued operation. It never
  executes, and the session remains usable.
- **Already dispatched:** abort or timeout terminates the dedicated worker.
  Synchronous WASM cannot handle a cancellation message while it is executing.
  The triggering operation rejects with `ABORTED` or `TIMEOUT`; other queued
  operations reject with `SESSION_LOST`. The session becomes disposed.

The second case does **not** claim rollback, acknowledge a successful edit, or
replay any mutation. Retain authoritative Markdown in the host and explicitly
create a new session after losing a worker. A cancellation arriving after an
acknowledgment does not undo that completed operation. Timer delivery depends
on the host event loop; these are not real-time CPU deadlines.

`dispose()` is immediate and idempotent, terminates the worker, and rejects all
outstanding operations with `SESSION_DISPOSED`. Worker crashes, undecodable or
out-of-order replies, and unrecoverable WASM errors also close the channel.
Ordinary typed Rust transaction errors leave the worker usable and its previous
acknowledged state unchanged. Worker termination does not promise to execute
Rust destructors; this core owns no ambient I/O resources.

## Paged consumption and stale input

```js
const token = editor.token;
for await (const page of editor.pages({ limit: 128, glyphs: true, token })) {
  // Render/consume a bounded page. No automatic prefetch accumulates pages.
  consumeDrawingPage(page);
}
```

The iterator captures its token **when `pages()` is called**, not at the first
`next()`. An intervening edit, resize or asset completion makes it reject rather
than combine layouts. Hit testing and selection likewise require the token of
the display actually shown. Async page controls apply to each requested page.

## Deployment

Rebuild the generated WASM package containing `FmdFlowSession`; an older release
cannot supply the flow backend. Both package assembly routes include the worker
entry and every relative import. The worker entry is marked as a side effect so
bundlers do not remove its message handler. No dependency or WASM size-budget
increase is added by this JavaScript wrapper.

The default `new Worker(new URL("./flow_worker.js", import.meta.url),
{ type: "module" })` resolves beside the package. Serve worker/WASM assets with
your application's normal module-worker/CSP policy. A bundler can supply
`workerFactory` in the third argument. Its worker entry can import
`@franken-suite/franken-markdown/flow-worker/worker`; the factory must create a
fresh dedicated worker because the session takes ownership and terminates it.
There is no fallback to a shared endpoint or the UI thread.

This API does not add a Canvas/GPU presentation layer, incremental parsing or a
new multilingual shaping profile. It isolates the existing bounded synchronous
core and exposes its actual measured display data. Main-thread string admission
and snapshot consumption still do some work; the expensive core runs off-thread.

## Verification routes

`node --test wasm/worker_transport.test.mjs wasm/flow_worker.test.mjs` executes
real Node worker threads, including a synchronously blocked worker. Session
fixtures explicitly use a raw binding double: these prove transport and API
behavior, not Rust rendering.

`node wasm/flow_worker_smoke.mjs <assembled-package> <generated.wasm>` instead
loads the production worker entry and real generated WASM, compares drawing
items/glyphs/reading semantics with the synchronous facade, edits and resizes,
completes assets out of order and checks fresh-render equivalence. It is wired
into both the package check and the DSR package quality route. It must pass
before claiming generated-worker rendering proof.

Types: `tsc --noEmit --strict --target ES2022 --module NodeNext
--moduleResolution NodeNext wasm/flow_worker_types_test.mts`.
