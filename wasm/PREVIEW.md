# Review a book before publishing

Open the assembled package's `demo/book.html`, add chapters and authorize local
images, then choose **Build book preview**. With matching rebuilt WASM, the reader
renders only the selected chapter through the Rust book engine. It retains the
site's chapter-relative assets, transclusions, stylesheet and book navigation,
without exporting a ZIP, rendering other chapters, or generating a whole-book
search index. No JavaScript Markdown parser or alternate typesetter is introduced.

Use the chapter selector, Previous/Next, or book links in the reader. The complete
ordered chapter map comes from Rust, not a JavaScript guess at filenames or
heading slugs. Cross-chapter navigation requests that chapter asynchronously;
fragment navigation within the current chapter reuses its generated HTML. An
explicit rebuild preserves the selected source across edits and reordering. If
that source was removed, it chooses the nearest remaining chapter. Navigation
to a source removed during capture fails instead of opening its old numeric slot.
External, unresolved, query-bearing and unsafe destinations never navigate.
The editor and its selection are unchanged.

This is a **restricted HTML-site reading preview**, not a PDF-pagination or EPUB
conformance proof. Active content, unsupported elements and external resources
are removed or blocked for isolation; inspect the exported artifact for its final
format. Images and fonts must be explicitly authorized before rendering.

## Editing, capture and cancellation

Preview is manual by default. **Update preview after edits** opts into a 600 ms
coalescing delay for this page session. Composition pauses the scheduled build.
The current chapter, rather than the entire site, is rendered after each admitted
edit. Parsing/source expansion still belongs to the native book engine.

Edits, settings, chapter changes and resource revocation immediately clear the
old reader and cancel any in-flight preview. A chapter request also retires the
previous iframe channel while it runs. Late successes, failures and messages
cannot republish that snapshot or replace a newer reader's status. Raw editor
and settings checkpoints are checked before navigation and after rendering,
including changes that did not dispatch input events. Invalid current inputs
are not replaced by a preview of older valid model values.

The preview owns a separate worker from publication exports and PDF proofing.
Cancelling it does not cancel an export. Cancelling, failures and page suspension
turn off automatic preview. Source editing, local saves and export controls
remain independent. Returning from the back/forward cache does not restore stale
pages or resource access. The preview persists neither source nor resource grants.

## Retained books and source-only transfers

The publisher opts into one retained preview worker and native `BookSession`.
After success, chapter/include text edits keep that idle book available. The
shared collection-capture helper checks both the collection's render-configuration
identity and this controller's last acknowledged worker capture revision. When
both match, it captures validated source-only project data instead of cloning
image/font stores. The worker client compares that text with its private idle
baseline and sends only changed strings. Chapter navigation without edits sends
an empty source delta: no source text, settings, images or fonts are retransmitted.

Native `updateSources` transactions reuse unchanged chapter ASTs. A no-op does
not issue a native source update. The visible reader is nevertheless cleared as
soon as source changes: retained parsed state is not permission to show an old
document. Only the currently displayed chapter's HTML is retained by the reader;
there is no multi-page HTML cache or queue of obsolete render requests.

Settings, resource changes/revocation, source renames/order/roles, imports and
project restores release the idle worker. A changed configuration, expired worker
or foreign capture revision requires a complete fresh snapshot. The helper does
not retry failed deltas against stale source. Worker-side admission and exact
profile/resource comparison remain authoritative; a host revision is not a
substitute for native validation. Older native source-edit APIs reconstruct from
the complete current worker-owned capture instead of displaying stale output.

An edit while rendering terminates the worker, including synchronous WASM work,
and loses its retained book. Input/update/output failures, unexpected idle
messages, composition, suspension and disposal also retire it. Idle state expires
after 30 seconds. **Cancel and clear preview** releases retained state even when
an edit has already cleared the visible reader. A later explicit or opted-in
build captures current inputs again. Capture-time edits do not schedule a second
automatic build that would cancel the first.

These are avoided renderer calls and transport copies, not a measured native
throughput or latency claim. The initial capture still copies admitted assets,
and native rendering may copy them internally. Input/output budgets are logical
limits, not whole-process memory ceilings. Nothing is persisted by this cache.

## Compatibility and worker API

`createBookWorker({ retainPreview: true })` retains both `preview` and
`chapter-preview` requests. `retainBook: true` additionally shares a native book
across PDF/EPUB/site exports; inspection and link jobs remain one-shot. Generic
workers default to one-shot operation. `cancelPending()` cancels only in-flight
work; `cancel()` and `dispose()` release both active and idle state. Hosts must
use the latter for revocation, suspension or explicit clearing.

```js
const output = await worker.render(files, "chapter-preview", options, {
  selectedChapter: 0,
  signal: abortController.signal,
});
// output.bytes: fmd-book-chapter-preview-v1 JSON with pages metadata,
// selected index and only that chapter's html. HTML still requires isolation.
```

The workbench uses the selected-chapter route when the worker advertises protocol
support. A capable native package returns `previewMode: "native-chapter"`. An
older native package lacking the new ABI uses the existing bounded ZIP decoder
and reports `site-fallback`; the reader labels this mode. A native render, budget
or protocol failure **never** retries the larger whole-site operation. Old worker
scripts must be rebuilt: mismatched protocol replies fail explicitly. Older
embedding adapters without the capability marker keep the unchanged full-preview
route and local navigation over its pages.

The original `render(files, "preview", options)` contract is unchanged:
`fmd-book-preview-v1` JSON contains `pages` with `path`, `source`, `title`, `html`.
The new format uses `fmd-book-chapter-preview-v1` with metadata-only `pages`,
`selected`, and one `html` string. The host validates format, mode, requested
index, count, and exact source identities against its captured collection before
framing content. Direct `BookSession.renderChapterPreview` requires the native
ABI and does not fall back. See `book.d.ts` and `book-worker.d.ts` for contracts.
Existing PDF/EPUB/site export formats are unchanged.

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

## Resource limits

The workbench still admits **128 chapters and 128 include-only files**. The
selected-chapter library/worker contract supports up to 4096 metadata entries,
8 MiB selected HTML and 64 MiB serialized JSON. Native mode has no whole-site
expanded-HTML limit because it never builds the site. Displayed chapters remain
limited to 50,000 elements. Neither the core nor workbench input budget is raised.

Old-native fallback and the legacy full-preview format retain their original
128-chapter, 64 MiB compressed-archive, 8 MiB per-member and 32 MiB total expanded
site-content limits. Classic ZIP layout, UTF-8 names, central/local agreement,
contiguous payloads, declared expansion sizes, CRC-32, unique mappings and
chapter identity are validated. ZIP64, encrypted, split and descriptor-bearing
archives fail explicitly. This decodes the engine's generated site, not arbitrary
user ZIP imports. `DecompressionStream("deflate-raw")` is required only for
compressed fallback/legacy previews, not native selected-chapter rendering.

## Verification

```sh
node --test wasm/tests/book_chapter_controls.test.mjs \
  wasm/tests/book_preview_controls.test.mjs \
  wasm/tests/book_retained_controls.test.mjs
node --test wasm/tests/book_chapter_preview.test.mjs \
  wasm/tests/book_chapter_worker.test.mjs
# After building a matching package through the repository's prescribed path:
node wasm/book_retained_preview_smoke.mjs /path/to/assembled/package
```

The controller tests execute production controller, collection-capture helper,
worker client/handler, retained-session facade, codecs and frame serializer with
explicit DOM/editor/native ABI doubles. Structured-clone transfers are real.
The retained-control suite also uses the production collection and resource
stores. The 12 MiB asset fixture records zero retransmitted asset bytes on a
subsequent chapter navigation or source edit; this measures transport volume,
not native rendering speed. Legacy behavior and old-native ZIP fallback remain
covered. The selected-route regression fails against the previous controller.

These tests do not establish Rust/WASM execution, native-browser CSP enforcement,
font/image rendering, visual fidelity or the full package build. Run the native,
generated-WASM and browser gates before making those claims. Rust/Cargo and the
required remote build service were unavailable in the authoring environment;
no native binary was rebuilt for this integration.
