# Review a complete book before publishing

Open the assembled package's `demo/book.html`, add chapters and authorize local
images, then choose **Build book preview**. The reader shows chapter HTML emitted
by the existing Rust book-site exporter, with its chapter-relative asset
resolution, transclusions, table of contents and book navigation. No JavaScript
Markdown parser or alternate typesetter is introduced.

Use the chapter selector, Previous/Next, or book links in the reader. Navigation
uses the output filenames and source mapping from the engine's generated search
index, not a JavaScript guess at Markdown filenames or heading slugs. Fragment
navigation targets emitted element IDs. External, unresolved, query-bearing and
unsafe destinations do not navigate. The editor and its selection are unchanged.

This is a **restricted HTML-site reading preview**, not a PDF-pagination or EPUB
conformance proof. Active content, unsupported elements and external resources
are removed or blocked for isolation; inspect the exported artifact for its final
format. Images must be explicitly authorized before building the preview.

## Editing and cancellation

Preview is manual by default. **Update preview after edits** opts into a 600 ms
coalescing delay for this page session. Composition pauses the scheduled build.
Each build captures the whole current book; it does not incrementally typeset
individual chapters. Large books may be better served by explicit builds.

Edits, settings, chapter changes and image revocation immediately clear the old
reader and cancel any in-flight preview. Late results cannot republish that snapshot. Raw
editor/settings checkpoints are checked at publication and before navigation,
including edits that did not dispatch input events. Invalid current inputs are
not replaced by a preview of older valid model values.

A preview owns a separate worker from publication exports and PDF proofing. Cancelling
it does not cancel an export. Cancelling, failures and page suspension turn off
automatic preview; source editing, local saves and export controls remain
independent. Back/forward-cache return does not restore stale pages or image
access. Source and image grants are not persisted by the preview.

## Retained native books during editing

The publisher now opts into one retained preview worker and native `BookSession`.
After a successful preview, ordinary chapter or include-only source edits keep
that idle session available for the next build. Only changed source strings are
passed to the native revision-checked `updateSources` API, which reuses unchanged
chapter ASTs. A true no-op does not issue a native source update. The visible
reader is still cleared as soon as the source changes: a warm native cache is
not permission to show an old document.

Reusing a book requires identical ordered chapter/include paths, presentation
settings and exact image/font bytes and keys, including font weight pins. Source
renames, role/order changes, metadata, page geometry, image/font changes, imports
and source/portable restores release the idle worker immediately. The worker
independently compares the complete admitted input before reuse; a collection's
configuration revision is a lifecycle hint, not a substitute for that check.
Old native builds without source-update support reconstruct on an edit rather
than display a stale capture. Mismatched worker-protocol versions fail explicitly.

An edit while rendering terminates synchronous WASM work and loses the cache;
it does not queue an incremental update behind obsolete work. Cancellation,
failed input/update/export/ZIP validation, unexpected idle worker messages,
composition, page suspension and disposal all retire the affected worker. An
unused idle worker expires after 30 seconds. **Cancel and clear preview** also
releases idle state when the visible reader has already been cleared by editing.
A later explicit or opted-in automatic build recreates the renderer as needed.
Automatic preview remains off by default, and capture-time edits do not schedule
a second automatic build that would cancel the first.

This is native session/AST reuse, not incremental typesetting, partial ZIP
rewriting, an HTML cache, delta transfer, or a measured speedup claim. Every build
still admits, copies and transfers the complete current source/resource capture,
compares resource bytes, exports the full site, and applies the unchanged archive
validator and isolated iframe policy. Retention adds the bounded native session
and its input capture to worker memory; existing logical input/output limits are
not a whole-process memory ceiling. Nothing is persisted to storage.

Embedding hosts can opt in with `createBookWorker({ retainPreview: true,
idleTimeoutMs: 30000 })`. The generic API defaults to one-shot workers.
`cancelPending()` cancels only an in-flight request, `hasRetainedPreview` reports
idle retention, and `cancel()`/`dispose()` release both running and idle state.
Hosts must call the latter for resource revocation, suspension or explicit clear.
Only preview calls can retain a worker; PDF/EPUB/site/inspection/link operations
keep their one-job lifetime. The publisher releases its owned preview client if
preview-controller initialization fails. No new runtime module or package entry
is required; existing assemblers already ship every changed production file.

## Isolation

The host never inserts rendered HTML into its own DOM. The reader uses an iframe
with `sandbox="allow-scripts"` but **without** same-origin, top-navigation,
popup, form, download or storage-access permissions. A new random 128-bit channel
identifies each displayed page. Host messages must originate from that frame's
WindowProxy, its opaque origin, and the current channel.

A nonce-authorized bootstrap receives generated HTML as escaped JavaScript data,
parses it into an inert template, and restricts it before insertion. Static HTML,
MathML, basic SVG and disabled task-list checkboxes are retained. Active elements,
refresh directives, nested frames, form actions, event handlers, external image
sources and native link destinations are removed. Original link destinations are
held privately for navigation messages, not left as navigable URLs in the DOM.

The frame's content-security policy precedes the bootstrap and defaults to no
resource loading. It permits inline presentation styles and embedded data images
and fonts, not network scripts, remote fonts, connections, frames or objects.
This restriction applies to previews only; no export bytes are rewritten.
Readiness requires an authenticated bootstrap message, not merely an iframe load
event. No acknowledgment within ten seconds clears the frame and reports failure.
Hosting policies that block the bootstrap or require additional Trusted Types
integration may disable preview; there is no insecure fallback.

## Resource limits and compatibility

The preview adapter admits at most 128 chapters, a 64 MiB compressed archive,
8 MiB per expanded archive member and 32 MiB total expanded site content. The
worker message is capped at 64 MiB after JSON escaping. A displayed chapter is
limited to 50,000 elements. These are admission limits, not guarantees about a
browser's physical heap or the Rust renderer's temporary memory.

Classic ZIP layout, UTF-8 names, central/local header agreement, contiguous
payloads, declared expansion sizes, CRC-32, unique mappings and chapter identity
are validated. ZIP64, encrypted, split, descriptor-bearing and other unsupported
archives fail explicitly. This is a decoder for the project's generated site
format, not a user ZIP-import feature. The native platform's
`DecompressionStream("deflate-raw")` is required for compressed previews; absence
reports `PREVIEW_UNAVAILABLE`. Site/PDF/EPUB export remains available independently.

The existing worker API also accepts `render(files, "preview", options)`. Its
`book-preview` output contains UTF-8 `fmd-book-preview-v1` JSON with ordered `pages`
(`path`, `source`, `title`, `html`). Treat the HTML as untrusted host input. Existing
PDF/EPUB/site calls keep their original formats and TypeScript return types.

## Verification

```sh
node --test wasm/tests/book_site_preview.test.mjs \
  wasm/tests/book_preview_*.test.mjs
```

The codec tests use independent Node/zlib ZIP fixtures and native decompression.
Controller, bootstrap and worker tests use production code with explicitly named
DOM, editor, transport and renderer doubles. Structured-clone transfers are real.
Package tests inspect source wiring and both assemblers. These checks do not prove
native-browser template parsing, CSP enforcement, font/image rendering, visual
fidelity, accessibility behavior or generated Rust/WASM interoperability. Run the
project's generated-WASM and native-browser gates before making those claims.

Retained-preview regressions and the optional compiled-engine smoke runner:

```sh
node --test wasm/tests/book_retained*.test.mjs
# After building a matching package through DSR:
node wasm/book_retained_preview_smoke.mjs /path/to/assembled/package
```

The retained tests use complete production collection, native-session facade,
worker client/handler, ZIP codec and preview controller files. Node worker_threads
and structured-clone transfers are real; native ABI, editor and DOM adapters are
explicit. A same-invocation comparison produces identical preview bytes for five
edited captures using one retained worker/native-book fixture versus five
one-shot fixtures. This demonstrates avoided fixture reconstruction, not native
parser throughput, latency, rendering quality or a measured performance win.
The controller/startup regressions fail against the original publisher wiring.
The compiled smoke runner instead requires generated WASM and performs real
native source/include updates and fresh-export parity; it fails rather than
substituting an engine when artifacts are missing. It was unavailable on the
authoring host. No browser or full generated-package acceptance is implied by
the Node results; the existing native/WASM and browser gates remain separate.
