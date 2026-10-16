# Streaming document batches

`@franken-suite/franken-markdown/document-batch` renders independent Markdown
documents through bounded, owned document workers. It supports HTML, PDF, SVG,
EPUB and interactive HTML, using the existing Rust/WASM renderer for every job.
Importing the module or constructing a batch does not start workers or load WASM.

```js
import { renderDocumentBatch } from "@franken-suite/franken-markdown/document-batch";

const jobs = [
  { name: "preview", format: "html", source: "# Hello" },
  { name: "print", format: "pdf", source: "# Hello",
    options: { page: { size: "a4" }, running: { footer: { right: "{page}/{pages}" } } } },
];

for await (const receipt of renderDocumentBatch(jobs, {
  concurrency: 2,
  order: "completion",
})) {
  if (receipt.status === "fulfilled") {
    console.log(receipt.name, receipt.value.filename(receipt.name), receipt.value.bytes);
  } else {
    console.error(receipt.index, receipt.name, receipt.reason);
  }
}
```

## Sources, ordering and backpressure

Sources may be arrays, synchronous iterables, asynchronous iterables, or
`ReadableStream` instances that expose async iteration. The async protocol takes
precedence when both are present. A synchronous iterator returning promises
instead of iterator-result objects is invalid; use `Symbol.asyncIterator`.

`concurrency` defaults to 2 and accepts integers from 1 through 8. It bounds the
window of admitted jobs and retained results. There is at most one outstanding
async source pull, which reserves a window slot. The source is never exhausted
in advance and there are no hidden per-worker queues. Pausing consumption stops
new admission; a pending pull may still finish, but it does not start another.

`order: "input"` is the default and preserves source order. `order: "completion"`
delivers results in settlement order, including results completed while the
consumer was paused. A long PDF therefore need not block completed HTML jobs or
prevent their workers from accepting subsequent work. Every receipt retains its
zero-based input `index` and optional `name`, regardless of delivery order.
Neither mode waits for a stalled next source item before delivering eligible
completed output.

Source options and asset views are snapshotted by the existing worker renderer
at admission, not at batch construction. Synchronous sources retain immediate
admission on `next()`; async inputs are admitted after their pull resolves.
Treat each yielded job as handed off and do not mutate it afterward.

## Cancellation and failures

An invalid job, failed render, per-document abort or timeout yields a rejected
receipt. Unrelated jobs continue; a fatally closed worker is replaced for a later
job. Failed documents are never retried or replayed. Invalid batch configuration
throws synchronously. Source iterator failures reject iteration and release all
owned workers.

Pass `signal` in batch options to cancel the entire batch. It interrupts pending
source pulls as well as rendering, with `DocumentBatchError.code === "ABORTED"`.
`return()`, `throw()` and breaking a `for await` loop also release owned workers.
The scheduler calls a non-exhausted source's `return()` once and observes cleanup
rejections, but does not await cleanup that could be stuck behind a pending pull.
Late input cannot be admitted after cancellation.

The batch cannot forcibly cancel arbitrary host I/O. Pass the same signal to
source operations that support cancellation, and give streaming producers their
own cleanup logic. For example:

```js
async function* loadMarkdown(urls, signal) {
  for (const url of urls) {
    const response = await fetch(url, { signal });
    if (!response.ok) throw new Error(`Markdown fetch failed: ${response.status}`);
    yield { name: url, format: "html", source: await response.text() };
  }
}

async function renderUrls(urls, consume) {
  const controller = new AbortController();
  try {
    for await (const receipt of renderDocumentBatch(loadMarkdown(urls, controller.signal), {
      signal: controller.signal,
      concurrency: 2,
    })) {
      await consume(receipt);
    }
  } finally {
    controller.abort();
  }
}
```

Fetching is explicit host behavior in this example; the Markdown renderer itself
does not acquire network or filesystem authority.

## Worker budgets and types

`worker` accepts `workerFactory`, `timeoutMs`, `maxPendingBytes` and
`maxOutputBytes` from the document-worker API. A factory must return a fresh,
owned endpoint each time. `maxPendingOperations` is fixed internally at 1.
Per-job `controls` can override its deadline or provide its own abort signal.
Worker deadlines start at render admission, not during source I/O; use the batch
signal to stop a stalled producer.

Budgets are **per worker**, so aggregate ingress/output allowances scale with
concurrency. They do not cap the WASM heap or memory retained by the application
after receiving outputs. Consume or persist results incrementally rather than
collecting an unlimited result array. See `DOCUMENT_WORKER.md` for format options,
resource budgets and worker ownership.

The package exports `DocumentBatchItem`, `DocumentBatchOptions`,
`DocumentBatchReceipt` and `DocumentBatchIterator` types. Input options remain
correlated with their format; successful outputs preserve format-specific types,
and rejected reasons are `unknown` rather than an assumed error shape.
