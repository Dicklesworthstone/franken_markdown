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

**Save Markdown** saves exact source, not a package of separately supplied image
or font resources. Native rendering has the selected engine's image/syntax
support; this exporter does not add a second Markdown parser or extend native
image-decoder support. The lightweight editor's direct data-URI image imports
are not translated into native image bindings by this packaging API.

## Settings and limits

Supported options are `font`, `darkMode`, numeric `fontScale` (0.5..3), `title`,
`author`, `lang`, `metadataEpochSeconds`, `pageNumbers`, `codeLineNumbers`, `toc`,
`tocDepth`, `pdfImages` and `fontAssets`. `allowRawHtml` may only be false. Other
options, including custom CSS and custom PDF paper, are rejected rather than
silently lost on saving. Viewing zoom is distinct from PDF typography.

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
