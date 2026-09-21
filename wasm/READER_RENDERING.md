# Interruptible semantic DOM rendering

`FlowReaderView.renderAsync(snapshot, { signal })` prepares the same selectable,
semantic DOM as synchronous `render(snapshot)`, through one shared builder. It
uses the already admitted reading snapshot: no parsing, shaping, native requests,
URL loading, or authorization changes occur here.

```js
const reader = new FlowReaderView(container);
const controller = new AbortController();
await reader.renderAsync(snapshot, { signal: controller.signal });
// Only now is reader.document the newly published snapshot.
```

Large builds yield real event-loop tasks around every 256 logical work units.
The checkpoints cover descendants, table grouping, list continuations, individual
style runs, and the scan that combines adjacent style runs into one logical link.
A single large styled paragraph is therefore interruptible, not just a collection
of many root nodes. Short renders do not schedule timers.

The existing reading admission limits still bound source text, node count, depth,
inline runs, links, and list metadata. Both the last displayed tree and one staged
replacement may coexist. Work units are not milliseconds or an exact heap limit:
individual DOM operations, large Text-node creation, cleanup, final replacement,
and the browser's ensuing layout remain synchronous. This is not viewport DOM
virtualization or a hard frame-time guarantee.

Nothing from the staged tree is attached until preparation and its final source/
layout checks succeed. During the wait, the last successful document remains
readable and its native selection stays intact. An abort, stale source/layout,
or preparation error discards the stage and removes its link listeners. The
signal is never forwarded to a worker. A new render (synchronous or asynchronous),
`clear()`, or `dispose()` supersedes the previous attempt; rejected invalid inputs
do not cancel a valid ongoing render. Re-rendering the currently displayed
snapshot remains a no-op, but revokes any pending replacement.

The public promise settles only after staged listener cleanup. `RENDER_SUPERSEDED`
identifies a replaced attempt; `ABORTED` identifies host cancellation, and existing
stale/disposal error codes remain explicit. Cancellation does not roll back a tree
that already finished publishing. The host must fence unsubmitted editor changes
and cancel rendering when its source intent changes. Clear/dispose immediately
remove displayed content when its authorization is revoked.

## Verification

```sh
python wasm/tests/run_flow_reader_render_async.py --chromium /usr/bin/chromium
```

The tests use real Chromium DOM, timers, event dispatch, and native text selection,
with production semantic admission and search. Only the native reading-page
provider is a fixture. They cover role/structure parity, nested and legacy lists,
large exact ordinals, tasks, tables, Unicode cross-style selection, cancellation
inside one large styled leaf, cleanup, stale generations, replacement, and
reentrant publication. They do not execute generated WASM or certify assistive-
technology conformance.
