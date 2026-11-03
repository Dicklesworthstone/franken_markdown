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

## Open existing Markdown source

**Open Markdown** selects one local `.md`, `.markdown` or `.txt` file in both
native and lightweight workspaces. It replaces source only, after confirmation:
**document settings, titles and explicitly embedded images/fonts are retained**.
The selected filename does not authorize relative image loading, access to other
files, a disk write, or evaluation of any script in the source. Existing resource
bindings may be referenced by the new source. Save HTML retains those resources;
Save Markdown contains only source and does not copy image files beside it.

Files are limited to 32 MiB and decoded as strict UTF-8. Invalid bytes are refused
rather than replaced. A UTF-8 BOM, mixed LF/CRLF/CR line endings, and other valid
source characters survive exact source downloads and HTML save/reopen while the
imported textarea view is unchanged. Ordinary edits use the textarea's LF view;
returning to that exact imported view recovers the imported string. Selecting an
empty file is a valid, confirmed replacement. Selecting identical source is a
no-op, not a new history entry or implicit save.

Only one picker/read is active. FileReader is aborted after ten seconds, on
intervening source input or composition, or when the page is suspended. Exact
source-view and revision checks before and after confirmation prevent late reads
from replacing newer typing, including edits that did not dispatch an input
event. Opening and source downloads do not require a working renderer. A failed
preview reports its error without discarding the imported source. The initial
workspace is still the unsaved-work baseline until a saved HTML file is reopened.

**Undo source replacement** restores the preceding exact source and selection.
This is one session-only replacement snapshot, not the textarea's native typing
history or a persistent backup. Each side is bounded to 32 MiB of UTF-8 source;
these logical bounds are not physical browser heap guarantees. A subsequent
source edit, composition, or page suspension retires it. A second successful
Open replaces it with the immediately preceding source. Cancelled/failed reads,
view changes and downloads leave it available. History and selected file objects
are not serialized; saving HTML recreates clean controls on reopening. Important
edits should still be downloaded before replacing source.

The compiled WASM must contain the updated controller; existing exported files
do not upgrade themselves. Browsers without FileReader keep the existing editor
and downloads without offering a nonfunctional Open control.

## Publish a plain native HTML document

**Publish HTML** downloads a complete rendered document as
`TITLE.published.html`, separate from the editable workspace produced by
**Save HTML**. It contains native-rendered content and the resources that the
native renderer embeds, not the editor, source JSON, binding JavaScript, WASM
binary, or a saved iframe. Unused resource bindings remain in the workspace;
they are not packaged separately with a publication.

Publication uses the latest source directly, even before preview debounce. It
uses committed document typography, title, language and TOC settings. View zoom,
a forced preview theme and unapplied settings fields do not affect the published
document. It does not replace the preview, rewrite source, consume source-open
undo, or treat a requested download as proof of saving. A publication is not an
editable-workspace backup; keep **Save HTML** for that purpose.

The native renderer's offline policy is retained in the published document:
embedded data images/fonts and inline styles are allowed, but scripts and remote
resource loads are denied. Source remains safely parsed by the native engine.
Unsupported native syntax/assets still produce that engine's diagnostics; this
operation adds no new parser or rendering fallback. Errors preserve the live
workspace and do not download its stale preview. Malformed editor Unicode is
rejected before HTML/PDF dispatch rather than silently replaced during encoding.

The button is available only with a publishing-capable native workspace runtime
and waits for initialization. Lightweight workspaces and older runtimes without
the capability do not offer a misleading substitute. The compiled WASM must
also contain the updated controller; existing standalone files do not upgrade
themselves. This is a local download, not uploading or deploying a website.

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

The source-file checks run the whole shipped controller:

```sh
node --test wasm/native_workspace_source.test.mjs
python wasm/native_workspace_source_browser.py --mode content
```

Node uses explicit DOM, clock, FileReader and renderer adapters with real File,
Blob and UTF-8 codecs. The browser probe uses actual Chromium file selection,
FileReader, textarea normalization, confirmation dialogs, downloads and repeated
HTML reparsing. Its initial shell and render functions are adapters, not rebuilt
Rust/WASM; adapter PDF bytes are not PDF-layout evidence. The default browser
probe mode is `file`; use `--mode content` explicitly on hosts that block file
navigation. Content mode and synthetic lifecycle events do not establish native
file-navigation or actual back/forward-cache acceptance. Both modes retain test
artifacts and require already-installed Playwright and Chromium.

Native publication checks:

```sh
node --test wasm/interactive_runtime.test.mjs wasm/native_workspace_source.test.mjs wasm/native_workspace_publishing.test.mjs
python wasm/native_workspace_publishing_browser.py --mode content
```

These tests cover committed settings/resource propagation, current source,
preview independence, failed output ownership, Unicode admission, readiness,
diagnostics and repeated publishing after workspace reopening. The browser
probe runs the production bootstrap, runtime and controller with an actual empty
WASM module and explicit shell/native-ABI adapters. It loads the published PNG
and checks the exported content policy against an inserted script. It does not
prove native parsing, font embedding or PDF layout. File/content navigation
modes have the same boundaries described for the source probe above; it reuses
that probe's retained fixture shell and requires both Python files in the repo.
The existing DSR Node selection includes both source and publishing suites.
