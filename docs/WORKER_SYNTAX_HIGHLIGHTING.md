# Persistent syntax highlighting in worker flow sessions

The worker flow API now exposes the same opt-in code-fence highlighting as the
synchronous flow session. It invokes the existing native shared-lexer renderer
inside the dedicated worker; it does not tokenize Markdown, paint HTML, or
reshape text on the UI thread.

```js
const session = await createWorkerFlowSession(markdown);
if (!session.supportsCodeHighlighting) {
  session.dispose();
  throw new Error("Install matching worker and native packages with code highlighting");
}
await session.setCodeHighlighting(true, session.token);
// Use fresh snapshots or the existing Canvas renderer after the mutation.
const page = await session.snapshot({ glyphs: true, token: session.token });
```

`codeHighlighting` is the last acknowledged mode. Merely enqueueing a toggle does
not publish a new mode or revision. A real mode change advances only the layout
revision, not the source/resource generation. Same-mode requests are no-ops but
still require an exact source/layout token. Edits, resize and asset completion
continue through the native session's persistent mode. Native lexer, output and
shaping errors reject rather than silently publishing an uncolored fallback.

A toggle can change item boundaries and selection offsets. Requery snapshots,
hit tests and selections using the returned token; do not reuse old item indices.
Worker queue budgets, deadlines and cancellation apply unchanged. Cancelling a
queued operation removes only that operation. Cancelling an in-flight operation
terminates the worker and loses the session; the host retains its authoritative
source and explicitly recreates it. No mutation is retried automatically.

## Ready-to-render startup

The public worker factory can configure syntax before publishing the session:

```js
const session = await createWorkerFlowSession(markdown, { viewportWidth: 800 }, {
  initialCodeHighlighting: true,
  startupTimeoutMs: 60_000,
  signal: startupAbortController.signal,
});
// The Promise resolves only after native mode setup is acknowledged.
```

This does not expose an intermediate uncolored session. Creation still performs
its ordinary initial layout, followed by the native mode transaction when needed;
it is not a claim of a single shaping pass. Omit `initialCodeHighlighting` to keep
the native default. Explicit `false` also works with legacy plain-only packages.
Explicit `true` requires support: missing support, budget failure, cancellation
or an inconsistent acknowledgment rejects creation and terminates the new worker.

The original monotonic startup deadline covers the worker factory, native
creation and optional mode setup. Setup receives only the remaining budget,
never a renewed deadline. `startupTimeoutMs: 0` disables that deadline; an expired
finite deadline is never converted into zero. The startup signal covers both
operations but does not become a lifetime cancellation signal after creation.

## Compatibility and protocol

Capabilities and initial mode travel in the creation value. The legacy state
schema remains exactly `{ token, layout }`, preserving older clients. Legacy
workers/native packages negotiate `supportsCodeHighlighting === false`; reads
remain usable, but toggling returns `UNSUPPORTED_WASM_PACKAGE` before dispatch.

The private toggle reply carries its previous token and acknowledged mode.
The client checks source stability, exact layout-revision progression, unchanged
layout options, and agreement with the original request. Corrupted replies close
the session before queued work is dispatched. The public result remains the
ordinary `FlowToken`, without protocol-only fields.

## Verification scope

`wasm/flow_worker_highlighting.test.mjs` exercises the JS facade, request
normalizers, worker queue/dispatcher and snapshot delta codec with a native
renderer double and structured-clone loopback endpoint. It covers negotiation,
stale tokens and budget errors, no-ops, exact u64 identities, persistence,
queued/in-flight cancellation, corrupted acknowledgments, and startup publication,
cleanup and deadline accounting. The TypeScript
fixture covers the public mode/cancellation contract.

These tests are not generated-WASM, real browser-worker, glyph-shaping or raster
proof. The generated native package must include the existing
`FmdFlowSession.setCodeHighlighting` export. Rebuild and run the repository's
native/WASM gates through the configured DSR environment before release.
