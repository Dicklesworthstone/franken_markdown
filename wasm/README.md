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
`../franken_markdown.js`, renders the HTML preview through `renderHtml`, and
downloads PDF bytes through `renderPdf`.

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

Every render result has:

- `format`: `html` or `pdf`,
- `mimeType`: browser Blob MIME type,
- `extension`: default download extension,
- `sourceLength`: Markdown source byte length,
- `bytes`: `Uint8Array`,
- `diagnostics`: recoverable parser diagnostics,
- `text()`, `blob()`, and `filename()` helpers.

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
