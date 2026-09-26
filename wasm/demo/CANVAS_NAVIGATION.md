# Canvas-to-source and internal-link navigation

The live `flow-canvas.html` editor can inspect a native Canvas hit, select its
original enclosing Markdown block, and follow an internal heading link. Click
text or an image, then choose **Show Canvas source** or **Follow internal link**.
The buttons support ordinary browser keyboard activation. The existing semantic
reader remains the keyboard-first alternative for choosing text and headings;
Canvas itself is not a newly invented accessible text editor.

## Authoritative locations, not inferred source

The selected `FlowHit.enclosingSourceSpan` must exactly match a node in the
current reading snapshot. The preview controller's existing `locateReading`
then supplies the original UTF-16 source range using its validated source-map
conversion. Display-item indices and fragment-local byte/UTF-16 caret offsets
are never treated as Markdown offsets. Multiple table cells can share an
ancestor block span: selecting one selects that enclosing Markdown block, not a
guessed cell or phrase. Images can be inspected without text caret metadata.

The link target is resolved by `FlowReadingDocument.locateFragment`, against
engine-supplied heading IDs. The controller rechecks the destination before
scrolling. There is no new slugger, second percent decoder, Markdown parser,
location.hash write, external URL opening, clipboard write or resource fetch.
Root-only, external-file, unsafe and missing targets remain inert. Source
inspection still works for those links when the source envelope is available.

Both actions leave the Markdown value unchanged. Source selection rechecks the
snapshot after focusing the textarea, since focus handlers can replace its
content synchronously. Navigation is disabled without a successfully admitted
semantic snapshot; a reading-limit failure does not disable Canvas or editing.

## Lifecycle and concurrency

Every action is tied to the actual controller, painter, reading document and
presented frame objects, plus their source/layout revisions and current editor
text. Equal numeric revisions from different sessions are not interchangeable.
Unsubmitted source changes are caught even before an input event or animation
frame. Edits, document replacement, scrolling, repaint, worker restart and page
suspension revoke the selection. Select a fresh location after these changes.

Exactly one physical hit request is retained by the page's navigation owner.
While it is pending, repeated clicks are not queued. Invalidation revokes the
request's authority but does not cancel a worker RPC or issue a replacement.
Late successes and failures are observed without publishing over the current UI.
The same owner survives worker restarts and persisted page suspension. A hung
hit therefore pauses inspection until that request settles; ordinary source
editing and the independent preview controller remain available.

## Checks and limitations

```sh
node --test wasm/tests/flow_canvas_navigation.test.mjs
python3 wasm/tests/run_flow_canvas_navigation.py --chromium /usr/bin/chromium
```

The 21 Node regressions execute the production navigation controller with
explicit DOM/native-hit/preview doubles. Ten Chromium checks execute the actual
live-page HTML, entrypoint and navigation module, using native buttons, mouse
coordinates, keyboard activation, focus, textarea selection and scrolling.
All browser network requests are blocked. The runner uses an in-memory document
and relinks only module import specifiers to local Blob modules; native hits,
reading snapshots, the preview controller and unrelated page controllers are
explicit test doubles. The existing folder-grant entrypoint test keeps all its
assertions and adds the new unrelated controller binding to its fixture.

These tests do not execute the Rust parser/shaper, generated WASM, actual glyph
hit testing, or the complete semantic-admission pipeline. The matching native
and WASM package, full repository tests and assistive-technology acceptance
remain separate requirements. The unfinished PDF ragged-policy handoff is not
changed by this browser implementation.
