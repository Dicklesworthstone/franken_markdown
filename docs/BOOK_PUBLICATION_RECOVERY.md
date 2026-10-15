# Recover a lost publication worker without losing the edition

`createBookPublicationWorker()` now restarts a failed publication once by default
when the worker reports `WORKER_FAILED` or its retained capture disappears with
`BOOK_CAPTURE_EXPIRED`. This applies to the source-only checker, any selected
output, and the final retained navigation check. It does not change the ordinary
`createBookWorker()` API or add replay to its source-mutation operations.

## One captured edition, one deadline

Capture still happens once, before asynchronous rendering. Recovery uses that
owned source/settings/image/font snapshot, not a new read of the editor or fresh
resource grants. The failed client's private outputs and report are discarded,
its client is disposed, and a new client restarts the **entire** selected edition.
No PDF from an earlier attempt is combined with an EPUB from a replacement worker.
Native and transport revisions start from the replacement client's own receipts.

Strict jobs repeat both the source-only preflight and final retained-native check.
Findings, inconsistent reports, malformed output, unsupported capabilities,
native transaction errors, stale revision errors, output limits and timeouts are
terminal. A previously clean report never substitutes for failed verification.
All results remain private until every selected format and required check passes.

`maxWorkerRetries` accepts integers from zero through two. The default is one
restart; zero restores immediate failure. Delays are 100 ms before the first
restart and 200 ms before the second. They consume the original shared
`timeoutMs` deadline, as do all repeated checks and exports. Retry never renews
that deadline or increases the combined output ceiling. A persistent load/crash
failure eventually returns its last error; recovery does not claim to repair a
missing or incompatible WASM binary.

Cancellation, AbortSignal, source invalidation through the existing host, and
publisher disposal stop both active work and backoff. No queued background task,
filesystem write, source installation, download, or network request is introduced
by the coordinator. Existing worker startup may load the package as before.

## Progress is not a publication receipt

`BookPublicationRequest.onProgress` receives frozen metadata:

- `phase`: `checking-links`, `rendering`, `verifying-publication`, or `retrying`;
- `attempt` / `maxAttempts`: one-based attempt and configured maximum;
- `completedFormats` / `totalFormats`: counts within this attempt;
- `format`: present only while rendering; `retryCode`: present only while retrying.

A retry notification names the upcoming attempt and resets completed formats to
zero because the previous private outputs were discarded. No source text, assets,
file handles, partial output bytes, report or download authority reaches an
observer. Exceptions and rejected observer promises cannot fail the publication;
an observer may explicitly cancel it. The operation remains busy through backoff.

```js
import { createBookPublicationWorker } from './book-worker.js';
const publisher = createBookPublicationWorker({ maxWorkerRetries: 1 });
try {
  const edition = await publisher.render(chapters, ['pdf', 'epub', 'site'], options, {
    strictLinks: true,
    signal: controller.signal,
    onProgress: progress => console.log(progress.phase, progress.attempt, progress.format),
  });
  // Only here are all edition.outputs available for an explicit save/download.
} finally {
  publisher.dispose();
}
```

## Verification

```sh
node --test wasm/tests/book_publication_recovery.test.mjs \
  wasm/tests/book_publication_retained_verification.test.mjs
```

74 Node tests passed: 47 new recovery/progress cases and 27 unchanged retained
verification regressions. The complete production coordinator and both checker
adapters execute via normal imports. A public-factory test executes the actual
entry module in a VM with explicit transport/capture/decoder boundaries. Cases
cover every failing phase, whole-edition replacement, exact source/assets across
host edits, report refusal, output budgets, backoff cancellation, shared deadlines,
late settlement, observer reentry and retry opt-out. The new test is included by
the existing `book_publication*.test.mjs` gate wildcard.

The native renderer and report generation are explicit test doubles. This is not
Rust/WASM rendering parity, output-format conformance, a browser acceptance run,
or a complete repository/package build. Declaration syntax was checked; full
TypeScript integration was not run. No dependency or native ABI was changed.
