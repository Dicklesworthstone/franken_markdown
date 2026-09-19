# Browser book publishing

Serve the assembled WASM package over HTTP and open `demo/book.html`, or follow
**Open the book publisher** from the Flow editor's export section. The link opens
a new tab without moving or discarding the current Flow document. Download its
Markdown first to import it into a collection; there is no implicit cross-tab
transfer of source or image authority.

The workbench combines the existing Rust `FmdBook` renderer with an ordered local
collection and dedicated module workers. It adds no Markdown renderer, ZIP writer,
browser-print PDF, third-party dependency, network loader, or server-side storage.

## Complete workflow

Add `.md`/`.markdown` files, PNG/JPEG/SVG image files, or a folder. Folder import
strips the one selected root and preserves nested paths. Other extensions are
ignored and counted. Imports append only after the complete batch is valid;
duplicate keys, malformed UTF-8, over-budget data, and edits during an asynchronous
read refuse the import without replacing existing source. File-picker order is
not assumed to be reading order: move chapters up/down explicitly.

Create, edit, rename, reorder, or remove chapters. Relative links and images use
the book-relative path: `guide/start.md` referencing `figure.svg` needs an
explicitly supplied `guide/figure.svg`. Renaming does not rewrite links. The core
owns cross-chapter resolution, transclusion, and format-specific validation;
this UI does not guess those semantics or silently fetch missing resources.

Choose title, author, language, font family/scale, theme, contents, and PDF page
numbers. Prepare a merged PDF, EPUB, or HTML-site ZIP, then click the separate
Download link. An edit, reorder, import, image revocation, cancellation, or page
suspension retires the prepared result. Response identity and the current UI/model
snapshot are checked again before publication and download activation.

## Keep the source

The collection starts in memory with **autosave off**. The optional local book
library adds named saves, recent revision recovery, and opt-in autosave; see
[LIBRARY.md](LIBRARY.md) for its conflict and retention rules. Keep a downloaded
source project as a separate backup before leaving. `book.fmdbook.json` records
ordered source paths/text, source roles, and settings. Chapter-only projects keep
schema version 1; projects with include-only files use version 2 so older clients
refuse them rather than silently publish snippets as chapters. The current
workbench reads both versions. It is plain unencrypted JSON. Images, fonts,
worker state, handles, and access grants are deliberately excluded. Keep original
image files separately. Reopening a project requires confirmation, clears all
previous image grants, and refuses replacement if edits occur while reading or
confirming. Reauthorize images before publishing the restored book.

Prepare selected source remains available when WASM cannot load or publishing
settings are invalid. A half-edited invalid path gets a safe fallback filename.
Untouched imported BOMs and CR/LF bytes survive textarea newline normalization;
edited text follows the textarea's LF convention. Returning to the original view
restores the original bytes. Invalid typed Unicode is retained visibly but must
be repaired before UTF-8 publication. No operation overwrites an existing disk
file; preparing an object URL is not proof that a download completed.

The workbench admits 128 chapters and 128 include-only sources, 4 MiB per file
and 16 MiB combined source plus paths; 128 images, 8 MiB each and 32 MiB total. Source projects have a 64 MiB JSON
ceiling (escaping can make JSON larger than source). An unusually escape-heavy
collection that exceeds it can still be downloaded file by file. Projects
are not encrypted vaults or substitutes for backups. Page hiding revokes URLs and
images; back/forward cache retains only the in-memory source collection.

## Worker API

```js
import { createBookWorker } from "@franken-suite/franken-markdown/book-worker";
const publisher = createBookWorker();
try {
  const output = await publisher.render([
    { path: "start.md", source: "# Start\n\n[Next](end.md)" },
    { path: "end.md", source: "# End" }
  ], "epub", { title: "Manual", toc: true });
  // output.bytes, output.blob(), output.mimeType and output.extension
  // Let the host explicitly offer the Blob for download.
} finally {
  publisher.dispose();
}
```

`render(files, format, options, { signal })` accepts `pdf`, `epub`, or `site` and
the existing `BookOptions`. One export owns one worker. Concurrent calls reject
with `BOOK_BUSY`; there is no unbounded queue. `cancel()`, an AbortSignal, or
`dispose()` terminates the worker, including synchronous Rust work. Cancellation
and errors leave the client reusable until disposal. Each export starts a fresh
WASM instance; this trades repeated startup cost for simple ownership and reliable
cancellation. No engine initialization occurs on the UI thread.

Chapter text is immutable and asset views are copied before any asynchronous
startup. Only private copies are transferred, never the caller's buffers. The
shared book facade now rejects malformed Unicode and snapshots images/fonts
across its own asynchronous initialization too. Returned bytes are transferred
as an owned view, never the larger backing WASM memory.

The worker deadline defaults to 120000 ms (configurable 1..600000). Returned output
is capped at 128 MiB; `maxOutputBytes` may lower that ceiling. This is an output
admission limit, **not a bound on the Rust renderer's temporary heap**. Native
book ingress limits remain separate from the smaller workbench limits.

## Shared snippets and include-only sources

The book facade expands `{{#include relative/path.md}}` directives through the
existing Rust transclusion engine before parsing. This applies to `createBook`,
the one-shot PDF/EPUB/site helpers, and worker exports and previews. Chapters can
include other selected chapters. Missing files are errors, never network or
filesystem requests. Rebuild the matching Rust/WASM package to use this feature;
an older `FmdBook` without `fromSources` refuses expansion rather than silently
publishing literal directives. Books without directives or extra sources retain
the original constructor path.

For reusable fragments that must **not** become chapters, supply `includeSources`
in `BookOptions`. The source strings may contain arbitrary UTF-8 text, including
code snippets in `.txt` or `.rs` files. They are not image/font authorizations.

```js
import { createBookWorker } from "@franken-suite/franken-markdown/book-worker";
const publisher = createBookWorker();
try {
  const output = await publisher.render([
    { path: "guide/start.md", source: "# Manual\n\n{{#include ../parts/shared.md:example}}\n" }
  ], "epub", {
    title: "Manual",
    includeSources: [{
      path: "parts/shared.md",
      source: "<!-- ANCHOR: example -->\nA shared explanation.\n<!-- ANCHOR_END: example -->\n"
    }]
  });
  // One chapter, not two. Offer output.blob() for explicit download.
} finally {
  publisher.dispose();
}
```

The same options work with the retained `createBook` session. Set
`expandIncludes: false` to preserve literal directives or supply already expanded
source. Nonempty `includeSources` with expansion disabled is an error. The raw
`FmdBook` constructor and Rust `BookRenderer::new` remain parse-only; their
explicit expansion APIs are `FmdBook.fromSources` and
`BookRenderer::from_sources`.

Nested include paths resolve relative to the file containing each directive.
Paths are case-sensitive, literal book-relative names: `%` and `#` are filename
characters, not URL escapes or fragments. Absolute paths, schemes, controls and
root escapes are rejected. Named snippets and line selectors such as `:3:8` or
`:3:` reuse the existing Rust rules. Fenced and indented examples remain literal.
No JavaScript Markdown/include parser is used.

Transclusion is textual, as in the native book command: links and image URLs
inside the inserted Markdown resolve from the **containing chapter**, not the
snippet's directory. Use explicit book-root URLs where necessary. Source inspection
continues to inspect only the original published chapters, not include-only files
or an expanded resource bundle. Its source-navigation targets use chapter paths,
not positions in the mixed source list.

Chapter and resource strings are validated and snapshotted before asynchronous
initialization or worker dispatch. They share the API's 4096-source and 64 MiB
UTF-8 text/path limits. Rust separately bounds expanded chapter text/paths to
64 MiB, the sum of original text and resolver copies to 64 MiB, and the whole
book to 4096 include resolutions; the expansion engine retains its depth-16
limit. Selecting one line still charges a full resolver copy. These are logical
content/work budgets, not a guarantee about physical heap usage. `sourceLength`
counts each originally supplied chapter/resource string once, excluding paths.

## Author shared sources in the workbench

Use **New include-only source** for an editable shared fragment, or the explicit
**Add include-only UTF-8 files** / **Add an include-source folder** pickers for
existing snippets. The ordinary Markdown/image import remains unchanged: it does
not read arbitrary code or text files. The include pickers interpret every
selected file as UTF-8 text, never an image or font grant; invalid encoding refuses
the complete batch. Folder imports preserve paths beneath one selected root.
Keep binary assets in the separate image picker.

The source list labels fragments with `[include]`; only chapters receive reading
order numbers. **Source role** changes a Markdown file between chapter and
include-only without modifying its source. A `.rs` or `.txt` source cannot become
a chapter until renamed to `.md` or `.markdown`. Resource paths share the same
namespace as chapters, so a duplicate cannot silently replace a chapter or snippet.
The workbench deliberately uses its stricter portable path policy (no `%`, `#`,
dot segments, or reserved characters), even though the lower-level API accepts
some of those literal filename characters.

For example, keep `guide/start.md` as a chapter containing
`{{#include ../parts/shared.md}}`, and mark `parts/shared.md` as include-only.
All PDF/EPUB/site exports and book previews receive the two source groups through
the same `BookOptions.includeSources` route. Only `guide/start.md` becomes a
chapter. Nested include interpretation remains in Rust. Renaming/removing a
source does not rewrite references; missing includes remain explicit errors.

Find/replace searches both roles, including code fragments. Reviewed replacements
and their undo/redo preserve roles, paths, order, and unmatched original bytes.
Project downloads and local-library saves/recovery use the same versioned source
codec, retaining include-only content and roles but no image authorization. A
collection containing only snippets can still be edited and saved; publication
requires at least one chapter. Selected-source downloads remain available without
WASM and preserve the original filename extension and UTF-8 bytes.

Changing a source role or editing a snippet invalidates prepared exports,
previews, inspection reports, and pending source operations just like a chapter
edit. Programmatically changed role controls are included in the download and
revision checkpoints even without a DOM input event. Reopening still requires
confirmation and revokes all images; page suspension retains only in-memory source.

## Verification

```sh
node --test wasm/tests/book_session.test.mjs wasm/tests/book_worker.test.mjs \
  wasm/tests/book_collection.test.mjs wasm/tests/book_controls.test.mjs \
  wasm/tests/book_package.test.mjs wasm/tests/book_includes.test.mjs \
  wasm/tests/book_site_preview.test.mjs \
  wasm/tests/book_resource_collection.test.mjs wasm/tests/book_resource_controls.test.mjs
```

The tests use real adapters/controller, native Node File/Blob/object URLs and
explicit engine, DOM, and transport doubles. Package inventory checks are static.
They do not prove generated-WASM rendering, native-browser module workers, IME,
visual output, or publication-format conformance. Run the project's DSR-managed
WASM build/parity gates and browser acceptance before claiming those properties.
Both existing package assemblers include the worker, its dependencies and demo.
