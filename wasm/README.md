# franken_markdown WASM Package

This directory contains the source wrapper for the browser package. Generated
`wasm-bindgen` glue is intentionally not committed here; `scripts/check-wasm-package.sh`
builds it into `target/fmd-checks/wasm-package/`.

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
scripts/check-wasm-package.sh
```

The assembled package contains:

- `franken_markdown.js` - ergonomic hand-written ESM wrapper,
- `franken_markdown.d.ts` - exact TypeScript API contract,
- `demo/` - static browser demo that uses the public package wrapper,
- `package.json` - package metadata and exports,
- `pkg/` - generated wasm-bindgen glue and `.wasm` binary.

## Demo

After `scripts/check-wasm-package.sh`, serve the assembled package directory
with any static file server and open `demo/index.html`:

```bash
python3 -m http.server 8787 --directory target/fmd-checks/wasm-package
```

Then open `http://127.0.0.1:8787/demo/`. The demo is plain HTML/CSS/ESM and
does not require a bundler or network access for normal rendering. It imports
the worker renderer, renders the HTML preview in a module worker, and downloads
PDF bytes through a separate worker. After a preview completes, **Download HTML**
saves that same standalone document without rendering again. It retains the
selected theme and embedded fonts, or the replacement custom stylesheet when
one is supplied. Source or setting changes hide the link
until a fresh preview completes; page suspension and disposal release its Blob
URL. The preview stays sandboxed; source HTML uses the safe-markup sanitizer
unless raw HTML is explicitly enabled for trusted input.

### Open and save a Markdown file directly

Expand **Open and save a Markdown file directly** to connect a file selected with
the browser's native picker. **Save current file** checks its exact previous
contents, writes the current source, then verifies the saved bytes. **Save
Markdown as…** chooses another destination and confirms overwriting nonempty
contents. Ctrl/Cmd+S saves, and Shift+Ctrl/Cmd+S chooses a destination. No automatic
disk saving occurs. Untouched imported source keeps its UTF-8 BOM and original
line endings; source files are bounded to 4 MiB and must be valid UTF-8 Markdown.

Saving preserves the selected font, rendering settings, authorized images and
prepared HTML/PDF or editable-workspace downloads. Save as changes the source
filename only after verification. Open and **Reload file version** confirm
replacing the source, turn off raw HTML, revoke prior image access, and retire
older exports. Ordinary source imports and browser-draft recovery disconnect
the previous file association; a matching filename never grants write access.

If the disk file changed, saving pauses. Keep editing, use **Download recovery
Markdown**, save to a different file, or explicitly reload the disk version.
An uncertain write is never reported as saved or automatically retried. Source
edits, composition, access changes and suspension fence pending operations; an
older callback cannot reconnect a newer document. Handles are private to this
page session and never enter drafts, render workers or exported workspaces.

Direct pickers require browser support and a secure page. Unsupported browsers
retain **Open Markdown** and **Download Markdown**, and the save shortcut
downloads a source copy. Suspending or leaving the page disconnects direct file
access, without an unload save. Another application can race the final disk
write: verification checks that completed operation, not future external edits.
See [file saving and conflict recovery](./FILES.md) for the shared engine's
guarantees, limits and focused checks.

### Download an editable offline workspace

Expand **Editable workspace to keep offline**, then choose **Prepare editable
workspace**. Once preparation completes, **Download editable workspace** saves
one `.workspace.html` file with the exact source, the native rendering engine,
font choice, document color mode, title, author, PDF code line numbers and all
currently authorized local images. Untouched imported Markdown keeps its UTF-8
BOM and original line endings. Open the downloaded file to continue editing,
change document settings, save source/projects and export HTML or PDF offline.
Preparing the file does not automatically download it or replace the normal
HTML/PDF preview and downloads.

Custom CSS and **Allow raw HTML** are not supported by the native workspace.
Preparation reports how to change these settings instead of silently dropping
them. The controls retain those choices until you change them. Image access
comes only from the explicit selection below the editor; preparing a workspace
does not look for other files or fetch image URLs from Markdown. Supplied unused
image bindings remain available for later edits. There is no custom font picker
in this demo; the workspace starts with its selected bundled sans/serif fonts.

A separate disposable worker loads the fixed matching JS/WASM pair from the
package's `pkg/` directory and calls the existing [native workspace exporter](./NATIVE_WORKSPACE.md).
Runtime redirects are refused. Response bodies are streamed and admitted before
copying, with limits of 64 MiB for WASM and 4 MiB for bindings. This demo retains
its 4 MiB source limit; workspace image destination text has a combined 64 KiB
limit and generated workspace HTML is capped at 256 MiB. The whole request has
a 30-second deadline. **Cancel preparation**, source/settings/image changes,
file opening, composition and page suspension retire pending work and any old
workspace link. A late result cannot publish over newer edits. Workers are
released after preparation, and download links are revoked when invalidated.

The demo must be served with the complete generated package. Missing or older
bindings report a runtime/capability error; there is no reduced-parser fallback.
The lightweight `renderInteractiveHtml` API and document-worker
`interactive-html` format keep their existing behavior.

To check real downloads using an existing generated package and installed
Playwright/Chromium, run:

```bash
FMD_WASM_PKG=/absolute/path/to/pkg node wasm/tests/demo_html_download.browser.mjs
```

`FMD_PLAYWRIGHT_MODULE` can select an installed Playwright ESM entrypoint and
`FMD_CHROMIUM` an installed browser executable. The check runs the repository's
demo and workers, compares downloaded HTML bytes with the sandboxed preview,
and records the supplied glue/WASM hashes. Using a release package proves
compatibility with that artifact; it does not qualify the current Rust source.
Composition and page transitions in this check are synthetic events.

### Local source draft recovery

The primary HTML/PDF demo includes **Local draft recovery** below the settings
and image controls. Enable **Autosave Markdown on this device** to keep one
source draft in this browser. Autosave is off by default; merely finding a
stored draft never replaces the editor or authorizes new writes. After a reload,
use **Restore stored draft**, confirm replacing the current editor, then enable
autosave again if wanted. **Save draft now** commits pending changes immediately
when autosave is enabled. **Refresh recovery** reconnects storage and checks for
the latest stored revision, leaving current text intact and autosave off.

Only the Markdown source and its original filename are persisted. Untouched
imports and recovered copies retain UTF-8 BOM and CR/CRLF bytes for subsequent
source downloads even though the browser normalizes textarea line endings.
Once edited, the current textarea text is saved. Recovery turns off raw HTML,
revokes local image access, and retires old previews and downloads before
installing the recovered source. Settings, image bytes, font grants, rendered
output and filesystem handles are not stored. The main demo uses its own
`franken-markdown-demo-source-draft-v1` IndexedDB database, separate from the
Flow demo's source draft.

Turning autosave off stops future saves and retains the previously committed
copy. **Forget stored draft** asks for confirmation, removes only that stored
copy, keeps the current editor and turns autosave off. A second tab cannot
silently overwrite a newer stored revision: a conflict pauses autosave and
offers an explicit refresh. Pending and failed transactions have visible status;
only a completed transaction is reported as acknowledged. Storage failures leave
source editing and Markdown downloads available. A late recovery read cannot
replace more recent typing, a new file selection or a newly started composition.

Leaving or suspending the page closes recovery storage and stops pending work;
it does not perform or claim an unload save. Returning through the browser's
back/forward cache keeps the in-memory editor, checks storage again and requires
fresh consent to restore or autosave. Browser storage can be cleared or evicted,
so use **Download Markdown** to keep an independent copy. That download requests
a source file; it is not an acknowledgment that the browser saved it to disk.

Focused controller checks run with:

```bash
node wasm/tests/demo_draft_recovery.test.mjs
node wasm/demo_worker.test.mjs
```

For native browser storage, reloads, competing tabs and exact source downloads:

```bash
node wasm/tests/demo_draft_recovery.browser.mjs
```

The browser check supports `FMD_PLAYWRIGHT_MODULE`, `FMD_CHROMIUM` and
`FMD_BROWSER_OUTPUT`. It runs the production demo and recovery modules with an
explicit renderer double; it does not qualify worker/WASM rendering. IndexedDB,
file inputs, textarea normalization, reloads, conflicts and downloads are real;
composition and back/forward-cache lifecycle events are synthetic. Set
`FMD_DRAFT_BASELINE_REF` to a pre-recovery Git revision to run the same first
feature assertion against its original demo files; that negative check is
expected to fail after reproducing the baseline's loss of edits on reload.

### Local images in the HTML/PDF demo

Open **Local images for preview and exports** beneath the settings. Select PNG,
JPEG or SVG files, or choose a document folder and enter the Markdown file's path
within it, such as `docs/guide.md`. Folder selection strips the selected root;
only image files are read. With that example, `../images/chart.png` resolves to
the selected folder's `images/chart.png`. Selecting a folder does not open its
Markdown or read other file contents. Browsers without folder selection retain
the individual-image picker.

The preview, its exact-byte HTML download and PDF export receive the same
captured image bytes through the existing document workers. No source rewriting,
network loader or second image renderer is introduced. The native engine still
validates image content and sanitizes embedded SVG; file extensions alone do not
prove an image will render. The panel lists Markdown references you can copy.
It supplies literal plain and URI-encoded relative paths, with optional `./`;
arbitrary equivalent URL spellings are not inferred. Absolute paths, URLs,
queries, fragments and paths outside the selected folder acquire no file access.

A complete new selection replaces the old grant only after admission and reading
succeed. Rendering and rendered downloads pause during the read; cancelling or a
read failure retains the previous selection. Typing retains committed images,
but changes during a pending read retire that read. **Revoke image access**,
opening another Markdown file, page suspension and disposal release the grant
and retire affected previews/downloads. Returning to the page requires selecting
images again. Markdown downloads remain available during image loading and
contain source only; retain original image files separately.

Limits are 128 images, 8 MiB per image, 32 MiB combined file bytes, and 4096 folder
entries. Literal destination aliases have a separate 48 MiB image-payload and
512 Ki UTF-16-unit key budget, checked before reading. This demo explicitly uses
the existing 64 MiB document-worker ingress ceiling so accepted aliases, source
and settings fit together. Each import has a 30-second deadline. Cancellation
retires ownership immediately; an underlying browser `File.arrayBuffer()` read
may finish later and cannot install its result.

`node --test wasm/demo_worker.test.mjs wasm/tests/document_image_sources.test.mjs`
exercises the production controller, existing path grants, real `File`/`Blob`
bytes and physical document-worker transport. The worker integration uses an
explicit native-renderer receipt double; it does not qualify generated WASM,
browser appearance, SVG sanitization or PDF output. Existing real-browser
download checks remain separate.

### Web component

The `./web-component` entry registers `<fmd-view>` for fetched or inline
Markdown. Its shadow view applies the renderer's palette, body typography and
embedded body/mono fonts; `font="sans"` and `font="serif"` can coexist on the
same page. Document selectors are scoped to the host, including dark and print
styles. Each render owns its font registrations, which are released on
replacement or disconnection. `fmd-rendered` follows font decoding and retains
the original renderer byte counts and diagnostics. A font/render failure emits
`fmd-error` and keeps the previous view. This uses browser CSSOM and FontFace
APIs without adding a framework or changing the native renderer.

Fetched HTTP(S) Markdown resolves relative links and images from the final
response URL, including redirects. A view of `docs/guide.md` can therefore use
`images/chart.png` or `../intro.md` independently of the embedding page's path.
The browser loads those image URLs under the host's normal content policy;
embedded data images retain their exact URLs. Inline Markdown and opaque
`data:`/`blob:` sources retain ordinary host-relative resource URLs.

Heading, table-of-contents and footnote links navigate the component's own
shadow-tree targets. Clicking or pressing Enter scrolls and moves keyboard focus
to the target; the next Tab continues from there. Matching source-document links
with fragments behave the same way, including relative and absolute spellings
with the same path and query. Host URL/history and sibling component IDs remain
independent. Modified clicks, downloads, other browsing targets, external links
and links without a matching target retain normal browser behavior. Failed or
superseded renders keep the previous publication's navigation intact.

Run `node wasm/tests/run_fmd_view.mjs` for the browser lifecycle suite (explicit
renderer boundary fixtures and delayed native font decoding), or
`FMD_WASM_PKG=/absolute/path/to/pkg node wasm/tests/fmd_view_typography.browser.mjs`
for real generated-renderer typography, font isolation and standalone metric
comparisons. Both accept the installed Playwright/Chromium environment options
above. A released package remains historical artifact evidence.
`node wasm/tests/fmd_view_navigation.browser.mjs` uses the same installed browser
options to exercise real HTTP redirects, image loads, shadow DOM, focus and
native keyboard activation against a loopback fixture server. Its renderer is an
explicit boundary double, so the results establish component navigation behavior
and preserve renderer byte-count reporting; they do not qualify generated WASM.

## Browser Usage

```js
import { createRenderer } from "./franken_markdown.js";

const fmd = await createRenderer();
const html = await fmd.renderHtml("# Hello", { font: "sans", darkMode: "auto" });
document.body.innerHTML = html.text();

const pdf = await fmd.renderPdf("# Report", {
  title: "Report",
  metadataEpochSeconds: 1700000000
});
const url = URL.createObjectURL(pdf.blob());
```

`renderHtml` captures source, theme, title, CSS, trust flags and other settings,
and owns copies of the supplied image/font byte ranges before asynchronous
initialization yields. Later edits to the options, shared request objects or
caller buffers do not retarget that render. This applies after initialization
has already completed too. Submit another call to render later changes.
PDF, SVG and EPUB already use the same request-ownership model.

### Markdown authoring profiles

Single-document HTML, PDF (including PDF/A), SVG and EPUB exports accept
`profile: "commonmark-gfm"` or `profile: "gfm-plus"`. The CommonMark/GFM
profile keeps definition-list syntax as literal paragraphs; GFM-plus renders
definition lists. Omitting the option preserves the existing extended dialect.
The same choice is retained by document-worker renders, publications and batches.

```js
const source = "Term\n: A definition.\n";
const html = await fmd.renderHtml(source, { profile: "commonmark-gfm" });
const pdf = await fmd.renderPdf(source, { profile: "gfm-plus" });
```

The profile uses the shared Rust AST transformation, including nested lists,
quotes and footnotes. It does not change raw-HTML safety, image/font ownership,
or archival-PDF settings. It is separate from `pdfA: "2b"`.

Explicit selection requires the additive `render*ConfiguredProfiled` bindings
generated from matching source. Older packages reject it with
`UNSUPPORTED_WASM_PACKAGE` instead of silently rendering the wrong dialect.
Requests that omit the profile retain their original binding signatures.
Unknown spellings and blank values fail with `INVALID_OPTIONS` before rendering.
Book PDF, EPUB, offline-site and chapter/full-book previews also accept the same
`profile` option. Retained `book`/`book-worker` sessions preserve it through
source edits, source-set replacements and multi-format publications. A changed
dialect invalidates retained output; successful rendered book outputs include
`profile` only when explicitly selected. Retained books require
`FmdBook.setAuthoringProfile`; older native or worker packages fail with
`UNSUPPORTED_BOOK_PROFILE` when they cannot honor or acknowledge it. The root
one-shot helpers require `renderBookPdfConfiguredProfiled` and
`renderBookSitePublicationProfiled`, reporting `UNSUPPORTED_WASM_PACKAGE` for
older packages. See [book authoring dialects and saved projects](BOOK.md#markdown-authoring-dialects).
Editable interactive HTML does not accept this option.

With generated bindings built from matching source, `renderHtml`, `renderPdf`
(including PDF/A) and `renderEpub` inherit omitted `title`, `lang`, `toc` and
`tocDepth` settings from leading document frontmatter; PDF and EPUB also inherit
`author`. Explicit options take precedence, including `toc: false`. The source
key is `toc_depth`. The same Rust parser used by the CLI handles `key=value`,
YAML-style scalars, block scalars, BOMs and CR/CRLF source. Source bytes and
diagnostic offsets stay intact. Ordinary rules and malformed frontmatter keep
their previous rendering behavior. Document workers and Flow exports use this
facade and retain the same defaults when their options omit these fields.

The additive generated `sourceMetadata` export supplies those defaults; older
generated artifacts without it retain their previous behavior until rebuilt.
Existing browser string conventions remain: empty title/author options mean
unset and language strings are trimmed. The dependency-free Rust
`wasm::source_metadata` helper exposes typed optional defaults for hosts to
merge; existing Rust render APIs continue to take already-resolved options.

EPUB author metadata uses the additive `renderEpubConfiguredMetadata` binding.
It preserves the submitted author text in the escaped `dc:creator` package
element, and author changes produce a different deterministic publication
identifier. Authors are limited to 64 KiB of UTF-8 at the browser boundary.
Explicit or inherited authors fail with `UNSUPPORTED_WASM_PACKAGE` on older
packages that cannot carry them. Existing author-free calls retain their old
bindings and argument shape. Retained book sessions and standalone interactive
workspaces also carry their saved author into EPUB exports.

`documentStats`, `searchIndex`, `accessibilityAudit`, `semanticDiff` and
`renderSemanticDiff` also capture source strings before initialization yields;
diff helpers retain the submitted old/new labels. The wrapper tests exercise
these boundaries with explicit generated-binding doubles, including cold/warm
initialization, concurrent requests and caller-buffer detachment. They do not
substitute for rendering/parity checks against a generated WASM artifact.

Every render result has:

- `format`: `html` or `pdf`,
- `mimeType`: browser Blob MIME type,
- `extension`: default download extension,
- `sourceLength`: Markdown source byte length,
- `bytes`: `Uint8Array`,
- `diagnostics`: recoverable parser diagnostics,
- `text()`, `blob()`, and `filename()` helpers.

## Single-document PDF/A emission

`renderPdf(markdown, { pdfA: '2b', pdfAStrict: true })` selects the native
single-document archival writer. The same options work in document workers,
atomic document publications and Flow PDF exports. Paper, running bands, fonts,
images and typesetting settings are preserved; a positive baseline grid requires
the matching extended archival binding. Missing bindings and native refusals
never fall back to ordinary PDF.

Successful archival outputs carry `pdfA: '2b'` and the captured `pdfAStrict`
boolean. Workers verify that receipt against the request before publishing;
ordinary/off exports keep their original envelope. These are emission settings,
not independent PDF/A validation or certification. See
[the single-document PDF/A guide](../docs/BROWSER_PDFA.md) for direct, worker and
Flow examples and the compatibility contract. Chapter-aware archival exports
continue to use the `./book` entry.

## PDF baseline grids

```js
const pdf = await fmd.renderPdf(markdown, {
  baselineGridPt: 13.125,
  page: { size: "a4", margins: 48 },
});
```

`baselineGridPt` accepts `0` (disabled) or a finite number from `1` through `144`
points. The native core rounds once to `0.001` pt. It aligns body text, headings,
code, tables, lists, quotes and footnotes from each page's top content edge.
Rows may occupy multiple steps to preserve leading and painted ink. After
rounding the body bottom to the grid and reserving ink clearance, at least
40 points of usable body height must remain; insufficient capacity is an error.
Figures retain their internal coordinates; body alignment resumes after their
measured box. Internal formula glyphs and running margin bands retain their own
placement. Oversized grid content fails explicitly. `fitToPages` and an enabled
grid are incompatible because fitting changes type size and margins.

The option is shared by `renderPdf`, root `renderBookPdf`, Document Worker PDF
requests, Flow PDF exports, and retained `book`/`book-worker` options. Direct
requests use the additive `renderPdfConfiguredGrid` or
`renderBookPdfConfiguredGrid` binding. Old packages reject enabled requests with
`UNSUPPORTED_WASM_PACKAGE` before asset copying or initialization. Retained books
use `FmdBook.setBaselineGridPt`; they snapshot inputs before asynchronous loading
and reject missing support with `UNSUPPORTED_BOOK_OPTIONS` before native book
construction. Their page overrides and retained publication caches preserve the
grid. Omitted or zero values keep existing calls, signatures and default bytes.
These are programmatic options; the editor and book workbench add no new controls.

## PDF chapter and section headers

Text slots in single-document and book PDF exports accept `{chapter}` and
`{section}` alongside `{page}`, `{pages}`, `{title}`, `{author}`, and `{date}`:

```js
const pdf = await fmd.renderPdf(markdown, {
  running: {
    header: { left: "{chapter}", right: "{section}", rule: true },
    footer: { center: "{page} / {pages}" },
  },
});
```

The first body heading beginning on a physical page supplies its section title
and owning H1 chapter. Later headings update the context carried to the next
page. A continuation page retains the complete heading title, even when the
heading itself wraps. Before a body heading or H1 appears, the corresponding
token is empty. Generated contents/Notes headings and footnote headings do not
change the context. Titles are literal text, fitted to the margin slots and
included in the actual font subsets; text such as `{page}` in a heading is not
expanded again. The ordinary pagination and tagged reading order are unchanged.
Use a package rebuilt from this source; older renderers treat unknown tokens
literally.

## PDF running logos

Single-document and book PDFs can repeat a host-supplied logo in either margin band:

```js
const pdf = await fmd.renderPdf("# Report\n\nCurrent results.", {
  pdfImages: [{ destination: "brand.svg", bytes: logoBytes }],
  running: {
    header: { image: { dest: "brand.svg", heightPt: 24 }, right: "{title}", rule: true },
    footer: { center: "{page} / {pages}" },
    skipFirstPage: true,
  },
});
```

Each band accepts one `image` with `dest`, optional `position` (`left` or `right`,
default `left`), and optional `heightPt` (integer points, 1..65535). Omitted
height uses the renderer's automatic margin fit. The destination must match an
explicit image asset; neither the wrapper nor core loads a URL or file. Image
bytes and settings are captured before asynchronous initialization. The native
renderer preserves aspect ratio, reserves space for adjacent text, and refuses
missing/invalid assets or impossible margin geometry. After fitting, both image
dimensions must remain at least 0.01 pt, the PDF coordinate precision; smaller
results fail with `pdf_running_image_invalid` instead of producing an invisible
logo. Increase the requested height, available page width or margin, or use
artwork with a less extreme aspect ratio.

Logos require the additive `renderPdfConfiguredRunningImages` binding from a
matching rebuilt package. Older packages continue to handle existing text-only
requests but reject a requested logo. The separate retained-book API supports
the same image shape using `options.images` and its additive
`setPdfOptionsWithRunningImages` binding. The root `renderBookPdf(files, options)`
helper accepts the same `running` and `pdfImages` shape as `renderPdf`, including
host-only logos absent from chapter Markdown. Its additive
`renderBookPdfConfiguredAdvanced` binding also carries the `typography` switches
(`homogeneous`, `antiriver`, `pareto`), `optimalPagination`, and
`microtype: "expansion"`. Older packages reject these requested features with
`UNSUPPORTED_WASM_PACKAGE`; omitted or disabled settings preserve existing
book defaults and signatures. See [book publishing](BOOK.md) for an example.
The stateless `document-worker` supports the same running bands and `pdfImages`
through `renderPdf` and `render("pdf", ...)`. It captures nested settings before
queueing and charges their strings and records against the worker ingress budget.
See [document workers](DOCUMENT_WORKER.md) for limits and cancellation behavior.

## Long PDF footnote continuation

Pass `splitFootnotes: true` to PDF exports to continue long notes containing
paragraphs, code, lists or quotations across page feet. This is available on document, book, worker, Flow, and saved
offline-workspace PDF paths. Retained books also accept `split-footnotes` in the
`typography` token string. Omission or `false` preserves existing behavior.

This mode requires greedy pagination without a baseline grid. The core rejects
combinations with `optimalPagination`, `strictPagination`, or a positive
`baselineGridPt`. Notes containing tables, figures, math or headings, and
unreferenced notes, retain the full-fidelity endnote fallback. Use a matching
rebuilt JavaScript/WASM package: older native token parsers reject the request,
and the adapters do not retry after dropping it.
See [PDF footnotes](../docs/PDF_FOOTNOTES.md) for the scope and navigation rules.
