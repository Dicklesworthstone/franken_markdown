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
reader and cancel its worker. Late results cannot republish that snapshot. Raw
editor/settings checkpoints are checked at publication and before navigation,
including edits that did not dispatch input events. Invalid current inputs are
not replaced by a preview of older valid model values.

A preview owns a separate disposable worker from publication exports. Cancelling
it does not cancel an export. Cancelling, failures and page suspension turn off
automatic preview; source editing, local saves and export controls remain
independent. Back/forward-cache return does not restore stale pages or image
access. Source and image grants are not persisted by the preview.

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
