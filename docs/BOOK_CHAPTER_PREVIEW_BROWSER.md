# Selected-chapter browser previews

The `book.js` facade exposes `BookSession.renderChapterPreview(index)` and
`renderBookChapterPreview(files, index, options)`. These call the native
selected-chapter ABI directly, require matching rebuilt `wasm-book` bindings,
and fail explicitly on older binaries. They do not fall back to full-site work.
`session.supportsChapterPreview` checks the live native capability. Use
`parseBookChapterPreview(bytes, expectedIndex, expectedCount)` to validate and
freeze the navigation metadata; its `html` is still **untrusted**.

## Cancellable worker path

```js
import { createBookWorker } from "./book-worker.js";
import { parseBookChapterPreview } from "./book.js";

const worker = createBookWorker({ retainPreview: true });
try {
  let output = await worker.render(files, "chapter-preview", options, {
    selectedChapter: 0,
    signal: abortController.signal,
  });
  const first = parseBookChapterPreview(output.bytes, 0, files.length);
  // Pass first.html through the existing isolated preview frame, not innerHTML.

  // Navigate without transferring unchanged sources, settings, images or fonts.
  output = await worker.renderSourceUpdate([], "chapter-preview", {
    expectedRevision: output.retainedInputRevision,
  }, { selectedChapter: 1 });
} finally {
  worker.dispose();
}
```

Selection is an operation argument, never part of `BookOptions`, source revision,
or resource authority. `renderSources` can also render a selected chapter after
capturing current chapter/include text; it sends only changed strings against
the idle baseline. `retainBook` shares the same native session with PDF, EPUB and
site exports. No HTML cache or request queue is introduced.

`previewMode` is `native-chapter` when only one AST is rendered and no site ZIP is
created/decompressed. It is `site-fallback` only when the native package lacks the
new capability: the existing bounded ZIP decoder then selects one chapter from
the validated site. Native errors, budget failures and malformed replies never
retry the more expensive path. The original `preview` format remains unchanged.
Old **worker scripts**, unlike old native packages, must be rebuilt; a protocol
mismatch fails explicitly rather than guessing which chapter was returned.

Native mode admits a complete 4096-chapter map, 8 MiB selected HTML and 64 MiB JSON.
Fallback retains the legacy 128-chapter/32 MiB total HTML/64 MiB archive limits.
Both client and worker verify the requested index, chapter count and mode before
retaining state. The host must also match source identities to its captured book.
Both routes require the same sandbox, CSP, inactive-resource policy and
message-channel checks; JSON validation does not sanitize HTML.

Cancellation and timeout terminate the worker, including synchronous work. Idle
expiry releases source/assets/native state. A source delta after expiry fails
with `BOOK_CAPTURE_EXPIRED`; capture current source again rather than replaying
an old snapshot. Bad output or failed source transactions retire the capture.

## Verification scope

The chapter API and worker tests exercise the production JavaScript codecs,
session lifecycle and worker protocol with an explicit native-ABI double. Worker
tests use real Node worker threads, structured cloning/transfers and cancellation
of synchronous fixture work. The legacy fallback test uses a real compressed ZIP
fixture. None of these tests is native rendering, WASM execution or a performance
benchmark; run the repository Rust/WASM gates through its prescribed build path
before claiming renderer parity or shipping a rebuilt binary.
