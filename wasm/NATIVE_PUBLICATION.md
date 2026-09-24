# Publishing from a portable native workspace

A workspace created with `createNativeWorkspaceExporter` can publish **HTML,
PDF, EPUB and SVG** after it is opened in the browser. Rebuild the WASM package
from matching source to include the updated editor controls and native bindings.
This does not change the lightweight `renderInteractiveHtml` renderer.

Use **Export EPUB** for an ebook and **Export SVG** for a continuous vector
poster. Both use the current lossless Markdown source, committed document font
family and type scale, embedded images, five supported font slots and their
weight pins. EPUB also receives the document title, language, table of contents
and navigation depth. SVG uses the native poster width; PDF paper, margins,
author/date metadata, page numbers and code-line-number switches are not SVG or
EPUB options. Viewing zoom or a forced viewing theme does not alter exports.
Renderer diagnostics remain attached to the publication, including stable codes
and document/source scope where supplied by the native engine.

All four publication controls share the existing dedicated export worker lane.
Only one publication runs at a time, independently of the live preview worker.
**Cancel export**, source edits, text composition and page suspension retire the
request. Current source and settings are checked again before downloading, so
even an edit that did not dispatch an input event cannot publish stale output.
A cancelled worker is terminated; a later explicit export creates a fresh one.
There is no silent main-thread rendering or automatic replay.

**Save HTML** still saves an editable workspace, not the published HTML document.
It retains source, committed settings, embedded engine and resources. On reopen,
the publication buttons are reconstructed once from the engine's capabilities;
transient progress, disabled buttons and pending jobs are not persisted.

EPUB requires `renderEpubConfiguredAdvanced`; SVG requires
`renderSvgConfiguredResources`. Older embedded packages remain usable for
HTML/PDF, but do not offer unsupported publication buttons. Direct requests for
an unavailable publication reject with `UNSUPPORTED_WASM_PACKAGE`, rather than
using a legacy renderer that would discard image/font resources.

## Verification

```sh
node --test wasm/native_workspace_publication.test.mjs
python wasm/native_workspace_publication_browser.py --chromium /path/to/chromium
```

The Node checks run the production serialized worker/runtime in real worker
threads with explicit native ABI doubles and an empty WASM initialization
fixture. They cover exact native argument propagation, resource ownership,
reopen state, output envelopes, capability mismatches, deadlines, cancellation
and stale-result guards. The browser check runs the production controller,
runtime and blob workers against a small fixture shell and the same class of
explicit ABI double; it checks downloads, settings, save/reopen, cancellation,
older bindings and absence of external network requests.

Where browser policy disallows local-file navigation, `--transport content`
checks the saved document through content injection instead. That mode is not
proof of `file://` acceptance. Neither fixture claims native Rust typesetting,
EPUB conformance, complete SVG rendering, or rebuilt-package acceptance; those
remain the responsibility of the generated-WASM and native/browser gates.
