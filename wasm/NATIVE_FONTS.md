# Transactional fonts in portable native workspaces

The native workspace runtime can replace or clear embedded font slots after
initialization. It uses the existing HTML/PDF/EPUB/SVG font-capable native ABI;
there is no second font parser, shaping engine or network font service.
Rebuild the matching WASM package and standalone exporter to carry the updated
runtime into newly generated files. Existing exported HTML is not upgraded.

## Local font controls

Newly generated workspace shells include **Document fonts** next to Save HTML
when the embedded runtime supports background font updates. The dialog lists
body regular, bold, italic, bold italic, and code/monospace slots with their
current byte counts and optional weight pins. Each slot can keep its font,
replace it from an explicitly selected local `.ttf` file, change an embedded
font's weight pin, or return to the native default. Select the actions and choose
**Apply fonts**: all changes are one transaction, never a partially applied batch.

Local selection allows 32 MiB per file and 64 MiB across a batch; the runtime's
128 MiB resulting image+font budget still applies. All selected metadata and
weights are checked before the first read. Reads are abortable and have a
10-second deadline. The native worker, not the filename or a JavaScript parser,
decides whether the supplied TrueType font is actually supported. Collections,
WOFF, font discovery, and remote-font fetching are not offered by this dialog.

**Cancel operation** aborts file reading or worker validation without altering
committed fonts. Source edits, font-draft edits, text composition, view changes,
and page suspension invalidate pending work. A final source/settings/font/view
snapshot check also rejects silent source changes. Closing the dialog, Escape,
and page suspension discard selected file handles. Source stays untouched: font
updates emit no textarea input event and do not normalize the controller's
lossless CRLF/BOM source anchor.

Successful font-only changes trigger an unsaved-work warning on navigation.
Failed/no-op imports do not. Save HTML retains the committed fonts, whereas Save
Markdown contains only source; initiating a download is not treated as proof of
saving. On reopening, controls are rebuilt after HTML parsing completes, so a
serialized dialog cannot create duplicate controls or retain file selections.
The dialog is excluded from browser printing.

## Runtime contract

`window.__fmdNativeRuntime` provides `fontSlots` (immutable slot/byte-count/optional
weight descriptions), `fontMode`, `fontsPending`, `cancelFonts()` and
`applyFontsAsync(patches, markdown, preview, display, isCurrent)`.

A batch contains one through five distinct slot patches:

```js
await runtime.applyFontsAsync([
  {slot: 'body-regular', bytes: selectedFontBytes, weight: 450},
  {slot: 'body-bold', clear: true},
], currentMarkdown, previewElement, {scale: 1}, isCurrentRevision);
```

Slots are `body-regular`, `body-bold`, `body-italic`, `body-bold-italic`, and
`mono-regular`. Omitted slots retain their resources. Clearing removes both the
font bytes and its weight pin, returning that slot to native fallback behavior.
A weight-only patch changes an already embedded slot; `weight: undefined` removes
its explicit pin. Importing new bytes without a weight uses the native default,
not the previous font's pin. Explicit weights must be integers from 1 to 1000.

Each font is limited to 32 MiB, and the resulting font+image set to 128 MiB.
Replacement/clearing releases the replaced bytes' budget. Input is admitted
before copying/encoding, respects exact typed-array/DataView byte ranges, and
rejects shared/detached buffers, getters, duplicate/unknown slots and ambiguous
patches. Font records and summaries use canonical slot order. Summaries never
expose the mutable ABI byte buffers.

## Preflight and publication

`applyFontsAsync` stages privately owned bytes, then preflights the current
source through the existing native HTML renderer in a disposable worker. The
native ABI validates supplied font assets; this JavaScript layer only admits
resource shape and size. Font validation, shaping and subsetting do not run on
the UI thread. Exact no-op changes skip rendering and publication.

The operation shares the settings/export/analysis slot: there is no extra
parallel WASM engine or unbounded queue. Cancellation, source/view invalidation,
page suspension or competing resource/settings publication retires it. The
caller must provide `isCurrent` to check its source/selection revision before
publication, including changes not accompanied by input events.

On success, inert saved runtime JSON, preview DOM and live font state switch
atomically. A write/DOM failure restores the prior data and preview before live
fonts change. The old preview worker is disposed because its protocol caches
fonts at initialization; the next edit starts a worker with committed fonts.
Subsequent HTML, PDF, EPUB and SVG exports receive the same updated bytes and
pins. No source, image grants, document settings or viewing zoom are changed.
The runtime dispatches `fmd-native-fonts-changed` after a successful publication.

The lower-level `createNativeWorkspaceRenderer().stageFonts()` transaction is
also exposed to trusted runtime hosts. Like settings/images, it uses one shared
generation for commit/rollback; it does not itself perform a rendering preflight.
Portable workspaces should use the background `applyFontsAsync` path.

## Verification

```sh
node --test wasm/native_workspace_fonts.test.mjs wasm/native_workspace_font_files.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/native_workspace_fonts_browser.py
CHROMIUM=/usr/bin/chromium python wasm/native_workspace_font_controls_browser.py
```

The Node tests execute production resource transactions and record exact native
ABI arguments for all four outputs. The Chromium tests execute the production
bootstrap and real Worker transport, using explicit recording/error-injection
ABI adapters. They cover cache retirement, save/reopen payloads, cancellation,
shared operation ownership, byte/weight propagation and atomic rollback. These
checks are not a compiled WASM build, font-table/shaping conformance, or visual
font rendering certification. No installed font files are used in these tests.

The font-controls browser harness executes the production authoring asset,
FileReader, native bootstrap and Worker transport with the same recording ABI.
It covers real local-file selection, multi-slot apply, rejection/cancellation,
read deadlines, save/reopen script order, fresh controls, source preservation and
font-only unload warnings. Source/save host adapters are explicit; this is not
the complete existing workspace controller or a compiled Rust/WASM render.
