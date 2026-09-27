# Transactional fonts in portable native workspaces

The native workspace runtime can replace or clear embedded font slots after
initialization. It uses the existing HTML/PDF/EPUB/SVG font-capable native ABI;
there is no second font parser, shaping engine or network font service.
Rebuild the matching WASM package and standalone exporter to carry the updated
runtime into newly generated files. Existing exported HTML is not upgraded.

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
node --test wasm/native_workspace_fonts.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/native_workspace_fonts_browser.py
```

The Node tests execute production resource transactions and record exact native
ABI arguments for all four outputs. The Chromium tests execute the production
bootstrap and real Worker transport, using explicit recording/error-injection
ABI adapters. They cover cache retirement, save/reopen payloads, cancellation,
shared operation ownership, byte/weight propagation and atomic rollback. These
checks are not a compiled WASM build, font-table/shaping conformance, or visual
font rendering certification. No installed font files are used in these tests.
