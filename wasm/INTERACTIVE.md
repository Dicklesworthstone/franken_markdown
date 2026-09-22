# Native rendering in a portable HTML workspace

`@franken-suite/franken-markdown/interactive` exports
`renderOfflineWorkspace(markdown, runtime, options)`. It builds one editable HTML
file containing trusted generated bindings, their matching WASM binary, and all
explicit image/font resources. After initialization, edits call the same advanced
HTML renderer used by the package, rather than its smaller JavaScript Markdown
fallback. The complete generated document is displayed in a sandboxed frame,
retaining native styles, math/diagrams, syntax highlighting and embedded fonts
without exposing editor controls to document styles or IDs.

This is an **opt-in, larger artifact**, not a replacement for lightweight
`renderInteractiveHtml`. Rebuild the WASM package from matching source first:
the compiled binary carries the editor controller. Updating JavaScript alone
cannot upgrade an old binary; the exporter rejects it with
`UNSUPPORTED_WASM_PACKAGE`. The generated bindings must be self-contained
`wasm-bindgen --target web` output from that same build. A mismatched runtime
fails visibly rather than silently falling back.

## Example: Node host with a matching local build

```js
import { readFile, writeFile } from 'node:fs/promises';
import { renderOfflineWorkspace } from '@franken-suite/franken-markdown/interactive';

const [source, bindings, wasm, image] = await Promise.all([
  readFile('./document.md', 'utf8'),
  readFile('./wasm/pkg/franken_markdown.js', 'utf8'),
  readFile('./wasm/pkg/franken_markdown_bg.wasm'),
  readFile('./images/plot.png'),
]);
const result = await renderOfflineWorkspace(source, { bindings, wasm }, {
  title: 'Research notes',
  font: 'serif',
  fontScale: 1.125,
  lang: 'en',
  toc: true,
  pageNumbers: true,
  metadataEpochSeconds: 0,
  pdfImages: [{ destination: 'images/plot.png', bytes: image }],
});
await writeFile('./document.html', result.bytes);
```

A browser host can provide the same trusted runtime strings and byte views from
its own build/asset loader. This API does not infer or fetch a module URL. Source,
scalar settings, runtime bytes, exact image views and font-slot weight pins are
captured before asynchronous initialization; later caller mutations cannot
retarget the document. The installed package is initialized with the supplied
binary, not a default filesystem/network lookup.

## Editing, downloading and failure behavior

**Export PDF** and **Ctrl/Cmd+P** download the current source rendered by the
shared PDF engine. They do not print a stale preview frame, wait for its load
handler, or use browser print layout. Explicit images, fonts, author, epoch,
language, table of contents, page numbers and code-line-number settings reach the
existing PDF ABI. Toolbar zoom changes viewing size, not PDF typography. The
browser's own menu-driven Print command is not the native PDF exporter.

**Save HTML** and **Ctrl/Cmd+Shift+S** flush current rendering and retain source,
runtime, assets and view settings in another editable file. Saved generations
reuse a single runtime payload; unused assets are retained for later edits.
**Save Markdown** and **Ctrl/Cmd+S** download source only; separately supplied
assets are not rewritten into Markdown. Untouched CR/CRLF source stays byte-exact.

Initialization or rendering errors preserve source and the last successful
preview. HTML/PDF exports requiring the failed renderer do not claim success;
Markdown remains downloadable. Parser/renderer diagnostics are visible. Native
results are freed on success and validation failures, and returned bytes are
copied before releasing generated handles.

The HTML file includes an offline content-security policy: no external images,
fonts, fetches, forms, object plugins or scripts. WASM compilation and the trusted
embedded module are allowed without JavaScript `eval`. The preview has an
additional script-free policy and sandbox. **The supplied bindings are executable
trusted application code**; never accept them from Markdown or an untrusted user.
The source is safe-parsed; raw HTML and caller CSS options are rejected here.

## Scope and limits

The API accepts `font`, `darkMode` (`auto`/`disabled`), `title`, `lang`, numeric
`fontScale` (0.5..3), `toc`, `tocDepth` (1..6), `author`, `metadataEpochSeconds`,
`pageNumbers`, `codeLineNumbers`, `pdfImages` and `fontAssets`. Unsupported options
fail instead of disappearing. It retains the underlying core's rendering limits;
this adapter does not implement new Markdown syntax, image formats, or PDF
inline-image layout. Core diagnostics remain authoritative for unsupported
content or supplied resources.

Budgets are admitted before copying binary payloads or initializing WASM: 32 MiB
UTF-8 source, 4 MiB binding source, 64 MiB WASM, 4096 images, five font slots,
32 MiB per image/font and 128 MiB runtime/assets combined. Shared or detached
buffers, duplicate trimmed image destinations, duplicate font slots and unpaired
surrogates in captured strings are rejected. Base64 increases file size; output
is capped at 256 MiB. This whole-document synchronous render path is not a worker
scheduler or an incremental-layout performance claim.

## Verification

From the repository root:

```sh
node --test tests/interactive_controller.test.mjs wasm/interactive.test.mjs wasm/interactive_runtime.test.mjs
tsc --noEmit --strict --target ES2022 --module NodeNext wasm/interactive_types.test.ts
python wasm/interactive_browser_check.py
```

The browser probe requires Playwright and Chromium on the verification host;
`CHROMIUM` may name an installed executable. It builds its fixture in a temporary
directory, uses the production exporter/bootstrap/controller, and instantiates
an actual empty WASM module. Core HTML/PDF bindings and the initial native preview
are explicit doubles. It checks actual downloads, repeated reparsing into fresh
browser pages, source safety, asset retention, view settings and failure recovery.
It does not assert that `file://` navigation works on the host, nor does it disable
the artifact CSP to make tests pass.

The Node tests exercise
real adapter/controller code with isolated binding/DOM doubles, including exact
19-argument HTML and 27-argument PDF calls, ownership, startup races, error
cleanup, diagnostics, admission limits and compatibility rejection. They do not
prove Rust compilation, PDF quality, or native/WASM output parity.

A release/verification host must additionally rebuild the matching package,
generate a real document with fonts/images/math/diagrams, reopen it offline, and
compare native renderer outputs. Use the project's DSR workflow, not Actions.
