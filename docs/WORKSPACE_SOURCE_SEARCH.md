# Find and replace in portable workspaces

Newly generated single-file workspaces include **Find / replace** beside Save
Markdown. It operates on the source textarea, not rendered HTML or inferred AST
spans. The same asset is bundled into the shell used by native workspaces; rebuild
the matching WASM package to carry it into newly exported native files. Existing
HTML files and already-built WASM packages are not upgraded in place.

## Workflow

Enter literal text and choose **Find**. Previous / Next select occurrences in the
source, wrap at the ends, and scroll to the match using the editor's actual font
and wrapping metrics. **Replace match** changes only the selected occurrence.
**Replace all** applies the entire admitted match set in one source edit.
**Undo replacement** restores the previous source and selection until another
source edit, source composition, or page suspension invalidates that transaction.

Search includes Markdown syntax, destinations and code, with no syntax filter.
It does not interpret regular expressions or replacement expressions: `$&`,
backslashes and HTML remain literal text. Empty replacement deletes matches;
identical replacement emits no source edit. Matches do not overlap and inserted
replacement text is not searched again within the same batch.

Queries and replacements can span lines. In the query, Enter finds/navigates and
Shift+Enter inserts a newline. Ctrl/Cmd+F opens Find and Ctrl/Cmd+H focuses
replacement **when focus is in the source editor or search panel**. F3 and
Shift+F3 navigate; Escape closes the panel. Browser Find/History shortcuts in the
preview and unrelated settings controls are not intercepted.

Match case is on initially. Turning it off uses JavaScript Unicode
case-insensitive literal matching, retaining original UTF-16 source offsets.
This is not locale-specific matching, Unicode normalization or multi-character
case folding: composed/decomposed accents and `ß` versus `ss` remain distinct.
The displayed textarea's LF line endings are the editing coordinate system.
Undo can restore the controller's untouched lossless source anchor, including
CRLF, BOM and controls, when the exact anchored view is restored.

## Safety and bounds

Each explicit search admits at most 32 MiB of valid UTF-8 source, a 4 KiB query,
and 10,000 complete, non-overlapping matches. Replacement input is limited to
64 KiB UTF-8, and the resulting source to 32 MiB. Invalid Unicode is rejected
before an encoder could replace it. Match overflow rejects the whole result set;
a truncated set is never presented as a complete replace-all operation. Output
size is computed before constructing the batch's replacement string.

Search is explicit, not timer-driven after every keystroke. These are logical
input/output budgets, not guarantees about browser heap size or layout latency.
Off-screen navigation measures an inert, temporary text mirror through the
matched source line and removes it synchronously. No untrusted HTML is inserted
into that mirror, no parser is run to find matches, and no resource is fetched.

Source/query/case changes invalidate results. Every mutation also compares the
captured source and revision with the live textarea, catching changes that did
not emit input events. Single replacement requires the selected match. Undo is
retired after intervening source activity, including synchronous edit-and-revert
activity. Composition in the source, query or replacement pauses mutation;
pagehide drops match and undo state. No input event is published after a failed
textarea assignment. Accepted batches publish one ordinary input event, allowing
the existing controller to invalidate previews, exports, settings preflights and
image operations through their established paths.

No renderer-owned settings, image grants, assets or source anchors are modified
by this module. A later preview failure does not undo the source operation or
prevent source saving and replacement undo. Queries, match sets and undo buffers
are not persisted; a saved UI shell is rebuilt with fresh controls on reopen.
The search panel is excluded from browser printing.

## Tests

```sh
node --test tests/interactive_renderer.test.mjs
node --test tests/interactive_search.test.mjs
node --check src/interactive_search.js
# Optional development dependency: Python Playwright and a local Chromium.
CHROMIUM=/usr/bin/chromium python tests/interactive_search_browser.py
```

The Node suite executes the production matcher/planner, including a deterministic
2,000-case comparison with literal split/join, UTF-8 limits, Unicode offsets,
non-overlap and output expansion. The Chromium harness executes the production
search UI and lightweight renderer with explicit input/save adapters, plus the
production print CSS. It covers navigation, one-event batches, undo, stale and
reentrant edits, composition, page lifecycle, serialized-shell reset, lossless
source-anchor restoration, and parser failure. It is not the complete native
controller, WASM build, or Rust test suite.
