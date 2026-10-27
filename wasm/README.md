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
