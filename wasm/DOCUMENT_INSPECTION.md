# Cancellable document intelligence

`@franken-suite/franken-markdown/document-inspector` runs the existing Rust
`documentStats` and `accessibilityAudit` APIs off the host thread, over one
captured Markdown source. It returns their complete native JSON reports together.
There is no JavaScript Markdown parser, invented lint rule, or partial-success
report. Importing the entry or constructing an inspector starts no worker and
loads no WASM.

```js
import { createDocumentInspector } from "@franken-suite/franken-markdown/document-inspector";

const inspector = createDocumentInspector({ maxPendingOperations: 1 });
const controller = new AbortController();
try {
  const report = await inspector.inspect("# Guide\n\n[Read more](#missing)", {
    signal: controller.signal, timeoutMs: 30000,
  });
  console.log(report.stats.words, report.stats.outline, report.stats.findings);
  console.log(report.accessibility.findings);
  // The host decides whether and where to save the complete JSON.
  console.log(report.filename("guide"), report.bytes);
} finally {
  inspector.dispose();
}
```

## Report meaning and ownership

The result schema is `fmd-document-inspection-v1`. `sourceLength` counts UTF-8
bytes. `stats` is the native `fmd-document-stats-v1` report: reading/speaking-time
estimates, readability heuristics, structural inventory, outline and source
quality findings. `accessibility` is the native schema-version-one PDF-target
source audit. It uses that API's existing default options; no raw-HTML trust,
asset loading or export settings are accepted by inspection.

**This is source analysis, not generated-PDF validation or PDF/UA certification.**
No findings does not prove conformance or that a document is accessible. Native
accessibility findings have codes and details; they do not necessarily carry a
severity. The API preserves this distinction instead of inventing a fatal verdict.
Readability and timing are estimates from the existing engine, not measurements
of a particular reader or promises of language-independent accuracy.

All additive native JSON fields are preserved, including fields this wrapper
does not interpret. The accessibility report may contain a text layer or other
source-derived content. Treat report downloads as document data, not anonymous
telemetry. The inspector introduces no network requests beyond loading its
application modules/WASM, no file reads, and no browser-storage writes.

The two native calls are sequential passes, not shared-AST or incremental
analysis. The first report is snapshotted before the second call. Only after
both succeed does the worker transfer a complete owned JSON buffer. `stats` and
`accessibility` are deeply frozen. The returned byte array is host-owned; editing
it does not mutate the already-decoded reports, but does change `text()`/`blob()`.

## Cancellation, stale results and limits

Methods accept `{signal, timeoutMs}`. A deadline includes queue wait, first-use
initialization and both analyses; zero disables it. Queued cancellation releases
only that request. In-flight cancellation terminates the worker immediately and
permanently closes the inspector because synchronous WASM cannot service an
abort message. Create a new inspector for later work. Failed work is never
retried, replayed or downgraded to a different parser. `dispose()` is immediate
and idempotent. Unsupported native exports report `UNSUPPORTED_WASM_PACKAGE`.

An editor must compare the captured source/revision with its current draft before
showing or saving a report, and abort pending work on edits. `sourceLength` alone
is not a revision fingerprint: different documents can have the same byte count.
The worker request identity connects each result to its captured input; it does
not know when the host has edited its document.

Sources accept at most 4 MiB of valid UTF-8, rejecting unpaired UTF-16 surrogates.
The default pending-ingress budget is 16 MiB (maximum 64 MiB), charged as 512 bytes
plus two bytes per source code unit; active and queued requests share it. There
are at most 32 pending operations by default (maximum 128). The combined JSON
output is at most 8 MiB, configurable downward through `maxOutputBytes`, with at
most 100,000 JSON values and depth 32. Escaped UTF-8 size is checked before building
each report string. Cycles, accessors, lossy JSON values, sparse arrays and invalid
schemas fail closed rather than quietly dropping findings. These are boundary
budgets, not a limit on the native engine's temporary WASM heap.

A `workerFactory` may supply a fresh Worker-compatible endpoint owned solely by
this inspector. The default uses the packaged module worker. Custom worker entries
can explicitly import `installDocumentInspector` from `document_inspector.mjs`;
the message protocol never selects import URLs or executable expressions.

## Verification

`node --test wasm/document_inspector.test.mjs` executes the production inspector
and transport in real Node worker threads with explicitly synthetic native-report
fixtures. It checks ownership, aggregate bounds, schema failures, cancellation,
queueing and no partial delivery. It is not proof of Rust parsing or audit quality.

`node wasm/document_inspector_smoke.mjs PACKAGE WASM` loads a supplied matching
**generated Rust/WASM package**, compares both reports against the existing direct
native APIs and checks repeat determinism. Both package assembly gates run this
probe and retain their existing rendering/parity/size checks. Adding this gate is
not evidence it has run in an environment without a built package.
