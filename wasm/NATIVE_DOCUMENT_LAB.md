# Native Document Lab in portable workspaces

**Document Lab** connects the portable native editor to the embedded engine's
`documentStats` and `accessibilityAudit` APIs. Rebuild a matching WASM package
containing the updated controller, then create a workspace with
`createNativeWorkspaceExporter`. Existing saved workspaces do not self-upgrade.
No extra JavaScript Markdown parser, native process, file access or network
service is introduced.

## Analyze, navigate, download

Open **Document Lab** and choose **Analyze document**. The panel displays native
word/character counts, exact source size, reading and speaking time estimates,
Flesch scores, structural inventory, outline and document findings. This is
separate from the lightweight editor's existing quick statistics. Readability
scores remain the engine's estimates, not language-independent measurements.

Outline buttons render the current revision when necessary and navigate to the
native heading ID **inside the sandboxed preview**. IDs are opaque values, never
CSS selectors or URLs; a heading cannot activate an identically named editor
control. Reports do not supply source spans, so no source line numbers are
invented. Missing preview headings produce a visible message.

**Audit accessibility** uses the existing native audit ABI, which runs with
**engine-default PDF options**. It does not inspect this workspace's configured
paper, supplied fonts or image resources. Its findings are authoring assistance,
not certification of the exported PDF, PDF/UA conformance, or a complete
accessibility evaluation. The panel labels that distinction explicitly.

**Download report JSON** downloads the original native report bytes, preserving
additive fields, full strings and every admitted row. Display is limited to the
first 200 headings and findings, with an explicit count when more are present.
Report text is inserted as text, not HTML. Long displayed strings are abbreviated
without changing the downloadable report. Source BOM, CRLF and non-BMP Unicode
are passed through the existing lossless source anchor.

## Cancellation and document identity

Analysis is explicit rather than triggered on every keystroke. It runs in the
same disposable worker slot as publication and settings preflight, independently
of the live-preview worker. Only one document operation occupies that slot; an
analysis never creates a fourth concurrent WASM instance or silently falls back
to the main-thread renderer.

**Cancel analysis**, closing the panel, Escape within the panel, source changes,
text composition and page suspension cancel or invalidate an unfinished report.
Applied settings invalidate completed reports. Source and settings are checked
again before displaying findings, navigating or downloading, including edits
made without an input event. A failed check leaves source, saved settings,
resources and render diagnostics unchanged. Retry is an explicit action.

**Save HTML** retains the editable workspace but strips the derived report panel
and transient controls. Reopening creates one empty Document Lab, not a saved
analysis job or misleading old report. Nothing is written to browser storage.
Older engines without report bindings still render/export; unsupported analysis
controls are absent. Engines exposing only one report show only that action.

Report admission is bounded to 8 MiB of UTF-8 JSON, 16,384 findings and 16,384
outline entries. Source admission retains the workspace's 32 MiB limit. Native
schemas, nonnegative safe integer counts, finite scores, UTF-8, MIME, response
identity and owned byte buffers are checked. Stats reports must describe the
admitted source byte length. Invalid reports fail explicitly rather than being
silently truncated. Worker startup and execution use the existing deadline.

## Verification

```sh
node --test wasm/native_workspace_analysis.test.mjs
python wasm/native_workspace_analysis_browser.py --chromium /path/to/chromium
```

The Node suite executes the production serialized worker/runtime in real Node
worker threads with explicit native ABI fixtures and an empty WASM module. The
Chromium probe uses the production controller, runtime and actual blob workers,
with an explicit fixture shell and native ABI/report doubles. It checks JSON
bytes, navigation isolation, cancellation, stale-source guards, resource/state
preservation, source import, save/reopen and the four existing publication paths.
Synthetic composition and page-transition events test controller behavior, not
operating-system IME gestures or actual back-forward-cache restoration.

Both suites are integration/ownership evidence, **not native Rust analysis or
rebuilt-WASM acceptance**. Where managed browser policy blocks file navigation,
`--transport content` explicitly checks saved document bytes through content
injection. This is not proof of `file://` compatibility and does not bypass that
policy. Browser artifacts and a JSON receipt are retained in a new directory;
`--output` must name a nonexisting directory. The probe never downloads packages,
installs a browser, deletes artifacts, or enables GitHub Actions.
