# Native-engine standalone workspaces

The `./interactive/exporter` entry exports a self-hosting HTML editor carrying a complete,
explicitly supplied Rust/WASM runtime. Editing and **Export PDF** use that engine,
not the reduced JavaScript Markdown parser or browser printing. The existing
`renderInteractiveHtml` API remains the smaller lightweight workspace; its
behavior and output format are unchanged. The one-shot `renderOfflineWorkspace`
entry in `./interactive` is also retained. This reusable factory instead loads
the supplied bindings and WASM into an isolated instance, so it does not depend
on or reuse the main wrapper's already-initialized singleton. Its subpath can
load without a neighboring `./pkg` directory.

## Create once, export many documents

```js
import {readFile, writeFile} from 'node:fs/promises';
import {createNativeWorkspaceExporter} from '@franken-suite/franken-markdown/interactive/exporter';

// The JavaScript and binary must come from ONE matching `wasm-bindgen --target
// web` build. Supply bytes/source, not URLs. DSR produces this package layout.
const exporter = await createNativeWorkspaceExporter({
  wasm: await readFile('./package/pkg/franken_markdown_bg.wasm'),
  bindings: await readFile('./package/pkg/franken_markdown.js', 'utf8'),
});
const output = exporter.render(await readFile('./article.md', 'utf8'), {
  font: 'serif', fontScale: 1.125, title: 'Article', lang: 'en',
  toc: true, tocDepth: 3, pageNumbers: true, metadataEpochSeconds: 0,
  page: {size: 'a4', orientation: 'landscape', margins: 36},
  pdfImages: [{destination: 'chart.svg', bytes: await readFile('./chart.svg')}],
});
await writeFile('./article.workspace.html', output.bytes);
```

In browsers, obtain the artifacts through an explicit file selection or your
application's chosen fetch policy and pass the same two fields. The exporter
itself has no ambient file access or URL fetching. Initialization uses a Blob
module in browsers and a data-URL module in Node. Reuse a factory for multiple
documents: separate factories deliberately own independent module instances,
even when identical glue is paired with different WASM bytes. Node retains ESM
module records for the process lifetime.

The result has the standard render helpers (`bytes`, `text()`, `blob()`,
`filename()`), diagnostics, UTF-8 source length, HTML extension and
`interactive-html` format. Export is synchronous after initialization. Runtime
bytes are copied before initialization yields; image/font byte views are
captured during the render call. Subarray/DataView offsets are respected.

## What travels in the file

The file contains source JSON, the editor application, the WASM binary, matching
binding JavaScript, native settings, and all explicitly supplied image/font
resources. Unused image bindings stay available for later edits. Saving HTML
from the workspace preserves these components through subsequent reopenings.
The first preview is already complete native HTML in a sandboxed iframe with a
resource-denying content policy; it does not briefly load an unsandboxed image
fragment before the engine starts. A startup failure leaves source and the last
preview available, reports an error, and does not silently choose the reduced
parser. The source remains downloadable while the native engine is unavailable.

Source and resource strings are JSON data, escaped against HTML script-end and
comment-tokenizer states. User source is never binding code. **The supplied
binding JavaScript is trusted executable application code**: do not accept it
from an untrusted document author. A generated binding requiring additional
relative JS imports/snippets is not a self-contained runtime and fails loading;
this API does not bundle or fetch those dependencies.

The generated file requires browser WebAssembly, ES modules, Blob-module imports,
and permission to execute its embedded application scripts. The preview itself
permits embedded images/fonts and inline styles, but no scripts, remote assets,
forms, or nested frames. This preview policy is not a sandbox for malicious
caller-supplied binding JavaScript.

**Save Markdown** saves exact source, not an image/font resource package.
Native image imports use short resource references: **Save HTML** retains their
bytes along with the editor and engine. Markdown alone retains those references
but does not copy the resources beside it. Native rendering still has the
selected engine's image/syntax support; this exporter does not add a second
Markdown parser or extend native image-decoder support.

## Author images without leaving the document

Use **Insert image**, paste clipboard-delivered image files into the source
editor, or drop local image files onto its current selection. All three entry
points share the same PNG/JPEG admission, browser decoding, source-revision
checks and resource transaction. Plain text and HTML-only pastes retain their
normal browser behavior. URL drags are not image imports; the editor does not
fetch those URLs or read ambient clipboard contents. File drops request a copy,
never a move of the original file.

In native workspaces, imported bytes enter the same resource arrays used by
both full HTML preview and PDF export, including export before the preview
refreshes. Short `fmd-import/<random-id>.png` or `.jpg` references keep base64 out
of the editing buffer. File contents determine the image format; escaped file
names are only alt text. Importing another image with the same name cannot
replace the earlier resource. The lightweight editor continues to embed data
URIs directly into Markdown for its portable-source workflow.

The complete batch is read and verified before any insertion. Packed native
buffers and escaped saved-runtime JSON are staged together and published before
synchronous editor input handlers run. A decoding error or intervening source
edit cancels the batch. If editing fails without changing source, publication
is rolled back; unexpected partial host edits retain resources and report the
problem rather than leave inserted references broken. Source undo keeps unused
resources available for redo, including across Save HTML/reopen cycles. Only
one picker, paste or drop import is in flight at a time.

Imports admit up to eight static PNG/JPEG files, 8 MiB each and 16 MiB per batch,
with at most 16,384 pixels per side and 24 million pixels per image. The native
workspace's lifetime image/font byte budget, resource count and destination-text
limits still apply, including retained unused resources. Decoding has a ten-second
deadline. Leaving/restoring the page invalidates an unfinished import. Source
admission checks native UTF-8 bytes as well as textarea length.

The supplied WASM must be rebuilt from matching sources to include the updated
embedded editor scripts. Existing exported documents do not upgrade themselves.
A new editor paired with an old native runtime rejects unsupported imports
explicitly instead of silently selecting the lightweight parser.

## Change typography, navigation and PDF paper in the editor

Native workspaces expose **Document settings** after engine initialization.
Choose sans/serif, a document type scale, automatic/light document color mode,
title, author and language. Enable a table of contents with depth 1..6, PDF page
numbers or PDF code line numbers. Existing embedded font assets and weight pins
remain in use; changing settings does not replace fonts or image resources.
The creation-time `metadataEpochSeconds`, including zero, is preserved.

Select Letter, A4 or custom dimensions, rotate portrait/landscape, and edit each
margin in PDF points (72 points per inch). Saved fractional dimensions remain
exact, without display rounding. Orientation changes width/height, not margin
sides. **Renderer default (no override)** removes the explicit geometry and uses
the original PDF ABI. Typography changes still work with older binaries lacking
the page export; an explicit paper request reports a capability error instead
of silently using default paper. The HTML preview remains continuous rather
than paginated; paper geometry applies to PDF. View zoom and theme are separate
from document export settings.

**Apply settings** validates an owned snapshot and renders current source with
the native engine before publishing saved JSON and replacing the preview.
Invalid geometry, rendering failures and publication failures preserve committed
settings; a failed preview replacement restores the previous preview. Source is
not rewritten, normalized or inserted into the textarea's undo history. Settings
and image transactions share a generation so an older operation cannot overwrite
new resources or settings. Stale open forms are rejected rather than replacing
newer applied settings. No network or browser-storage writes are introduced.

**Cancel** and Escape discard the draft. An unchanged Apply does not materialize
omitted metadata, TOC depth or default paper. Unedited metadata stays exact even
when a text control displays a newline-normalized value. Settings-only changes
activate the unsaved-work warning; restoring source alone does not clear it.
Restoring all original settings and source clears that warning. A download
request is not treated as proof of saving. Save HTML retains committed settings
through repeated reopenings, but never applies an unfinished form draft. The
controls are recreated rather than duplicated during reopening. Save Markdown
contains source only, not document settings or separately supplied resources.

The controls require a WASM build containing the updated embedded controller and
the matching workspace runtime. Existing standalone files do not self-upgrade.
A failed native startup leaves settings unavailable and source downloadable.
Lightweight workspaces do not show controls for unsupported native settings.

## Settings and limits

Supported options are `font`, `darkMode`, numeric `fontScale` (0.5..3), `title`,
`author`, `lang`, `metadataEpochSeconds`, `pageNumbers`, `codeLineNumbers`, `toc`,
`tocDepth`, `pdfImages`, `fontAssets` and `page`. `allowRawHtml` may only be false. Other
options, including custom CSS, are rejected rather than
silently lost on saving. Viewing zoom is distinct from PDF typography.

`page` uses the same contract as direct, worker and flow PDF exports: Letter,
A4 or explicit width/height in points, portrait/landscape orientation, and
uniform or per-side margins. The shared normalizer validates plain data (no
accessors), bounds and the 72-point minimum content rectangle, including native
f32 rounding. The saved payload carries the exact six-number ABI representation.
The offline renderer revalidates it, snapshots it, and dispatches to
`renderPdfConfiguredPage`. A binary missing that function is rejected for an
explicit page request; omitted `page` retains the existing PDF ABI and defaults.
Paper and margins survive save/reopen and do not change with display zoom.

Admission limits are 32 MiB source, 64 MiB WASM, 4 MiB binding JavaScript,
32 MiB per resource, 128 MiB image/font resources combined, 4096 images, five
unique font slots, 8 KiB per image destination, 64 KiB destination text in total,
and 256 MiB final HTML. Text limits count UTF-8 bytes and reject unpaired
surrogates. Shared/detached buffers and duplicate resource identities are
rejected. Resource counts and aggregate sizes are checked before encoding.

## Verification

`node --test wasm/interactive_export.test.mjs` checks packaging and ownership
using explicit ABI adapters; the loader tests execute a real tiny WASM module,
**not the Rust renderer**. `node wasm/native_workspace_smoke.mjs PACKAGE OUTPUT`
executes a matching built Rust/WASM package and retains an HTML workspace for
browser verification. The DSR package gate includes the new entry, its runtime,
declarations, documentation, unit checks and generated-package smoke test.
These proof classes are distinct: passing adapter tests does not prove rebuilt
Rust/WASM rendering or a browser save/reopen lifecycle.

`node --test wasm/native_workspace_page.test.mjs` covers geometry admission,
ABI selection, exact argument order, old-package behavior and damaged saved
payloads. `node wasm/native_workspace_fixture.mjs NEW_OUTPUT.html` builds an
explicit ABI-adapter fixture using the shipped exporter, controller and runtime
with a real tiny WASM module. `python wasm/native_workspace_browser_check.py
OUTPUT.html` then checks downloads, startup edits, iframe isolation, resource
retention, paper/margins, repeated reopening and source recovery after failure.
Its default mode attempts actual file navigation. Use `--mode content` explicitly
on hosts whose browser policy blocks `file://`; that mode reparses downloaded
bytes in fresh pages and does not prove file-navigation compatibility. It tests
browser plumbing, not Rust parsing/layout, font embedding or PDF conformance.

`node --test wasm/interactive_runtime.test.mjs wasm/native_workspace_images.test.mjs`
executes the actual runtime and importer with explicit ABI/DOM adapters. It
covers image ownership, budget admission, transactional publication/rollback,
PDF dispatch, clipboard/drop entry points and lightweight compatibility.

`python wasm/native_workspace_images_browser.py` creates a retained fixture and
checks it in locally installed Chromium using Playwright. It exercises real
image decoding, the file picker, browser undo/redo, downloads and repeated
save/reopen cycles. File drops use Chromium's trusted input protocol; clipboard
image events use synthetic File/DataTransfer payloads, not operating-system
clipboard gestures. Non-file events remain browser-owned. The fixture uses the
shipped runtime/controller/importer, explicit native renderer and initial-shell
doubles, and an empty real WASM module. Adapter PDF bytes are not PDF conformance
or native layout evidence. As with the existing browser probe, `--mode content`
is an explicit alternative on hosts blocking `file://`, not file-navigation
proof. No dependencies or browser binaries are downloaded by the probe.

`node --test wasm/interactive_runtime.test.mjs wasm/native_workspace_settings.test.mjs`
checks owned settings, full HTML/PDF argument propagation, unchanged resource
buffers, saved-data revalidation, staged rendering, failure rollback, stale
settings/image transactions and publication reentrancy. It executes production
runtime/boot code with explicit ABI/DOM adapters and real empty WASM startup.

`python wasm/native_workspace_settings_browser.py` exercises the actual
controller and runtime in locally installed Chromium: modal controls, Cancel
and Escape, exact paper geometry, settings-only modification warnings, immediate
PDF dispatch, source recovery, and repeated HTML downloads/reopening. It retains
screenshots and all downloaded bytes. The initial shell and renderer are explicit
adapters, not rebuilt Rust/WASM output; its PDF bytes are not valid PDF layout
proof. The default mode attempts file navigation; use `--mode content` explicitly
where browser policy blocks it. That alternative reparses saved bytes in fresh
pages and does not establish file-navigation compatibility. Use `--browser PATH`
for an existing Chromium executable or `--output NEW_DIRECTORY` for retained
artifacts. The probe never downloads dependencies or removes artifacts.
