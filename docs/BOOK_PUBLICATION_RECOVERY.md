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

## Workbench status and cancellation

The edition session forwards these phases to the workbench's existing live status
region. Authors see the current format and its position in the selected edition,
initial navigation checking, final verification, and an explicit restart message
with attempt counts. These are stage counts, not invented render percentages.
The same captured book is used after recovery; preparation never saves a file.

`state.publicationProgress` contains the current frozen display metadata while
the job is current, or `null` otherwise. The session stays `busy` through all
phases and backoff. Cancel remains available, and preparation/bundling remain
unavailable until appropriate. No download URL or clean-verification badge is
created by a progress notification, including `verifying-publication`.

Progress is bound to the exact source/editor checkpoint and edition job. Raw
edits, source/resource changes, changed format/verification policy, composition,
suspension and disposal retire work through the existing host paths. Even an
unnotified edit discovered by a progress callback cancels the publisher before
its next native operation. View callbacks are rechecked too. Late callbacks from
cancelled or older jobs cannot repaint a new edition or a finished delivery ZIP.

Metadata admission copies only bounded primitive fields without invoking
accessors. Malformed, duplicate and out-of-order events are ignored; this optional
channel cannot substitute for output/report validation. Older publishers that do
not send progress still work under the existing strict/non-strict result policy.
The unchanged controller uses its existing `role="status"`, polite live region,
buttons, source subscriptions and atomic download display; no new DOM surface,
worker, parser, persistence path or automatic retry of filesystem writes exists.

## Verification

```sh
node --test wasm/tests/book_publication_recovery.test.mjs \
  wasm/tests/book_publication_retained_verification.test.mjs \
  wasm/tests/book_publication_workbench_progress.test.mjs
```

103 Node tests passed together: 47 recovery/progress cases, 27 unchanged retained
verification regressions, and 29 new workbench progress cases. The complete
production coordinator and both checker adapters execute via normal imports. A public-factory test executes the actual
entry module in a VM with explicit transport/capture/decoder boundaries. Cases
cover every failing phase, whole-edition replacement, exact source/assets across
host edits, report refusal, output budgets, backoff cancellation, shared deadlines,
late settlement, observer reentry and retry opt-out. The new test is included by
the existing `book_publication*.test.mjs` gate wildcard. The workbench cases run
the complete production coordinator/session and unchanged controller through
normal imports with explicit DOM/source-host/URL and native engine boundaries.
They exercise live status/buttons/download gating, retry at an actual coordinator
backoff, cancellation/composition/policy/lifecycle changes, unnotified raw edits,
observer reentry, hostile/late metadata, legacy publishers and delivery bundling.
The unchanged controller and retained-verification test blobs were checked against
main; no golden, test gate or existing regression was changed.

The native renderer and report generation are explicit test doubles. This is not
Rust/WASM rendering parity, output-format conformance, a browser acceptance run,
or a complete repository/package build. Declaration syntax was checked; full
TypeScript integration was not run. No dependency or native ABI was changed.
