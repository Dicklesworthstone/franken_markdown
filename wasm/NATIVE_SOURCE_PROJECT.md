# Portable, data-only native source projects

A native workspace can now save a **source project** (`.fmd.json`): exact Markdown,
committed document settings, every embedded image, and every embedded font slot.
Unlike Save Markdown, it retains separately embedded resources. Unlike Save HTML,
it carries no executable editor, binding JavaScript or WASM engine. A host can
reopen this data with an explicitly selected, trusted, matching runtime instead
of executing application code supplied by a document author.

## Save from a standalone workspace

Use **Save source project** beside Save HTML. The existing editor's lossless source
host supplies Markdown; the control never reconstructs it from the textarea's
newline-normalized value. BOMs, original line endings, Unicode spelling and
metadata newlines are preserved. All committed image bindings are retained,
including unused images kept for later edits. Font slots and variable-weight
pins are retained. An unapplied settings/font/image form is not saved.

Preparation runs in one disposable Blob worker with a 30-second deadline.
**Cancel project save**, source edits, composition, committed resource/settings
changes and page suspension terminate pending encoding. The exact source and
saved-resource snapshot are checked again before download activation, including
programmatic edits that emit no input event. Old worker responses cannot replace
newer work. Object URLs are released on a later save, edit, suspension or disposal.
A failed save leaves source/resources untouched; retry is explicit.

The worker parses saved runtime JSON as data and copies only settings/images/fonts
into the project. It never imports or evaluates the saved binding text. Saving
therefore works even when native engine startup failed, provided the editor's
lossless source host and committed resource block remain available. It does not
start a renderer or borrow/cancel the native preview, proof or export worker. A
browser without Worker support receives an explicit error, not a UI-thread copy
fallback. The existing content policy is unchanged.

A download request is not proof of a completed filesystem save and does not clear
the editor's unsaved-work warning. No source rewrite, browser-storage write,
network upload, ambient file access or automatic save is introduced. Serialized
transient controls are rebuilt on reopen; pending jobs are never replayed.

The controls are embedded by the updated exporter. Its selected WASM shell must
include the existing lossless-source host hook used for PDF proofing; earlier
standalone files do not self-upgrade. Plain Markdown and self-hosting HTML remain
available.

## Reopen without replacing the current document

Use **Open source project**, choose a local `.fmd.json` file, then select
**Build workspace**. When it is ready, select **Download workspace** and open the
downloaded HTML file. The new workspace contains the imported source, settings,
images and fonts, together with the current workspace's explicitly trusted engine.
It includes the same save and reopen controls, so this workflow can be repeated.

This is a separate-workspace workflow, **not an in-place replacement**. The current
editor, unsaved edits, undo history, resources and unsaved-work warning remain
unchanged. Building does not download automatically, navigate away, open a popup,
overwrite a file, save to browser storage or upload anything. It does not include
source, settings or assets from the current document in the rebuilt document.
The embedded engine is the only document-independent input reused from it.

Choosing a file alone does not read or render it. Build starts one disposable
worker that reads the selected file, strictly validates the complete project,
and only then imports and initializes the selected trusted binding JavaScript
and WASM. Project fields can never replace that executable code. The same
import-free workspace publisher serves ordinary exports and reopening; sandboxed
preview framing and the existing content policy are unchanged. Resources are
validated by the chosen renderer as usual, not silently dropped or fetched.

The file, saved-runtime JSON and output each have a 256 MiB admission bound;
source and asset limits remain those below. The trusted runtime is limited to
64 MiB WASM and 4 MiB binding text. A 60-second deadline spans reading, validation,
startup, rendering and assembly. Cancel, closing the dialog, changing the selected
file or committed runtime data, composition and suspension retire pending work.
File identity and exact runtime text are checked again on completion and before
download activation, including programmatic changes without DOM events. Cancelling
the file picker, or choosing the same file again, does not dismiss the dialog.

A completed workspace stays in memory until cancellation, replacement or close.
Downloading retains it so another explicit download uses the same bytes without
rendering again. Download URLs are released on a later download or when
the prepared output is retired. Suspension clears the selected file and requires
explicit reopening. Native export/settings operations must finish or be cancelled
before Build; opening does not cancel them or alter their result. Reopening does
not automatically replay jobs when a saved HTML document is opened again.

Unlike data-only saving, building an editable workspace **requires a successful
native render**. Its disposable worker can initialize independently of a failed
current preview, but corrupt or incompatible engine artifacts, rejected assets
or render failures produce an explicit error and no partial workspace. A browser
without Worker support has no synchronous fallback. The dialog reports native
diagnostics; it does not certify document fidelity or a completed filesystem save.
The public APIs below remain available for hosts that select another trusted engine.

## Create, read, edit and export

```js
import {readFile, writeFile} from 'node:fs/promises';
import {
  createNativeSourceProject,
  readNativeSourceProject,
  createNativeWorkspaceExporter,
} from '@franken-suite/franken-markdown/interactive/exporter';

const source = await readFile('./article.md', 'utf8');
const project = createNativeSourceProject(source, {
  title: 'Article', author: 'Author', font: 'serif', fontScale: 1.125,
  metadataEpochSeconds: 0, toc: true, tocDepth: 3, pageNumbers: true,
  page: {size: 'a4', margins: 36},
  pdfImages: [{destination: 'chart.svg', bytes: await readFile('./chart.svg')}],
});
await writeFile('./article.fmd.json', project.bytes);

// Reading/creating project data does not initialize an engine.
const restored = readNativeSourceProject(await readFile('./article.fmd.json'));
restored.options.title = 'Revised article';
const revised = createNativeSourceProject(restored.source, restored.options);
await writeFile('./revised.fmd.json', revised.bytes);

// The HOST chooses trusted artifacts from ONE matching wasm-bindgen build.
const exporter = await createNativeWorkspaceExporter({
  wasm: await readFile('./package/pkg/franken_markdown_bg.wasm'),
  bindings: await readFile('./package/pkg/franken_markdown.js', 'utf8'),
});
const workspace = exporter.renderProject(revised.bytes);
await writeFile('./revised.workspace.html', workspace.bytes);
```

`createNativeSourceProject(source, options)` accepts the same supported public
options as `exporter.render`, using its normalization and resource admission.
It is synchronous and returns `bytes`, `sourceLength`, `text()`, `blob()` and
`filename()`, with MIME `application/json` and format `native-source-project`.
Unsupported switches, such as custom CSS and raw-HTML passthrough, fail rather
than silently disappearing from the project. Source-project output is data,
not a new native rendering format or an extension of `FmdOutputFormat`.

`readNativeSourceProject(input)` accepts JSON text, an ArrayBuffer or an exact
byte view. It returns `{source, options}`. Options, image/font arrays and binary
views are fresh, editable host-owned copies; modifying them cannot change the
original project bytes or another decoded copy. The outer result is frozen.
Creating a project captures resources before returning. Its output bytes are
host-owned and mutable; `text()` and `blob()` reflect their current contents.

`exporter.renderProject(input)` validates and decodes the entire project, then
uses that exporter's already selected runtime and the ordinary rendering path.
There is no runtime-selection override in a project. The decoded options also
work with the direct document renderers; each renderer retains its own supported
resource/option contract. A new engine can change layout despite identical source
and settings. A project is not a reproducible engine artifact, an engine version
pin, a rendered publication or a cryptographically authenticated document.

## Format, trust and limits

The schema is `fmd-native-source-project-v1`, with exactly `schema`, `source`,
`options`, `images` and `fonts` at the top level. Images have `destination` and
canonical padded-base64 `bytes`. Fonts have `slot`, canonical-base64 `bytes` and
an optional `weight`. Unknown fields, runtime/bindings fields, unknown schemas,
duplicate resource identities and malformed base64 fail explicitly.

The saved options retain the native six-number `pageGeometry` representation
(width, height, top, right, bottom, left, in points). Reading converts it back to
the shared public `page` representation without rounding dimensions or changing
margin sides. Both f64 and staged native f32 content-rectangle checks apply.
An omitted page remains omitted. Optional metadata and TOC depth remain omitted;
an explicit metadata epoch of zero is never replaced by the clock.

The codec admits at most 32 MiB UTF-8 source, 32 MiB per resource, 128 MiB combined
images/fonts, 4096 images, five unique font slots, 8 KiB per destination and
64 KiB total destination text. Final JSON is bounded at 256 MiB. Unpaired Unicode
surrogates, shared/detached byte buffers and invalid UTF-8 fail. A JSON-file BOM
is allowed; a source-field BOM is preserved. Resource totals are checked before
binary decoding, and escaped UTF-8 JSON size before aggregate serialization.
These logical limits do not bound all JavaScript/native heap allocations.

The format keeps resource bytes; it does not prove that those bytes are a valid
image or font. The chosen native engine remains responsible for decoding and
rendering them. Reading a project grants no file/URL authority: image destinations
are keys, not fetch instructions. Project downloads contain document text and all
embedded resources, including unused resources; treat them as private document
data and distribute fonts/images only with appropriate rights.

`INVALID_SOURCE_PROJECT`, `SOURCE_PROJECT_LIMIT` and
`UNSUPPORTED_SOURCE_PROJECT` identify codec failures. Existing public workspace
option normalization and native rendering retain their own error contracts.

## Verification

```sh
node --test wasm/native_source_project*.test.mjs
tsc --noEmit --strict --lib ES2022,DOM --target ES2022 --module NodeNext \
  --moduleResolution NodeNext wasm/native_source_project_types.test.mts
python wasm/native_source_project_browser.py --mode file --output NEW_SAVE_DIRECTORY
python wasm/native_source_project_open_browser.py --mode file --output NEW_OPEN_DIRECTORY
node wasm/native_source_project_smoke.mjs PACKAGE MATCHING_WASM
```

The codec suites test exact round trips, hostile data, ownership, canonical
base64, UTF-8/geometry/aggregate limits and seeded cases. Exporter tests execute
the actual exporter/codec/page normalizer with explicitly replaced native engine,
preview and PDF-proof-session dependencies. UI tests use controlled DOM/URL/timer
adapters and execute the emitted worker body in isolated VM and real Node-worker
realms. Reopening tests also import an explicit binding adapter in a real Node
worker, check complete output parity with the ordinary exporter, and verify that
invalid projects fail before even trusted engine initialization. These tests do
not establish Rust rendering or native image/font validity.

The browser probes use the actual exporter, project controls, codec and Blob
workers in a locally installed Chromium. Native rendering and the lossless-source
host are explicit fixtures, not a rebuilt engine or the full production editor
controller. They retain downloads, HTML and narrow-viewport screenshots. The
reopening probe loads the downloaded workspace in a fresh browser context, saves
its complete project, then reopens it again and compares the two generated HTML
files byte-for-byte. It also checks current-document preservation, cancellation,
engine startup failure, hostile project fields and explicit-only downloads. The
synthetic composition checks are not operating-system IME qualification. The
reparse test reconstructs transient controls; it is not the full native Save HTML
lifecycle. Default file mode attempts actual file navigation. `--mode content`
uses fresh in-memory document realms where host policy blocks files and does not
prove file-navigation compatibility. No dependencies or browsers are downloaded.

The generated-package smoke uses an explicitly supplied real Rust/WASM package.
It compares restored-project workspace HTML and direct document HTML/PDF bytes
against ordinary rendering of the same source/settings/assets. Both assembly
scripts include the new modules and tests without changing native build, parity
or size-budget gates. Adding this probe is not evidence that a native/DSR build
has run in an environment without the compiled package.
