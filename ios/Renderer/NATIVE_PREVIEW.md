# Responsive previews in portable native workspaces

With a matching rebuilt WASM package, both `renderOfflineWorkspace` and
`createNativeWorkspaceExporter` produce an editor whose live HTML preview runs
in a dedicated worker. Typing, source-file opening and undo, view zoom/theme,
initial rendering, and resume all use the asynchronous preview path. The
lightweight JavaScript-only workspace keeps its existing behavior.

The worker is lazy and owns its own WASM instance. It receives the explicitly
embedded bindings, binary, fonts and images; it never infers or fetches a URL
from Markdown. Unchanged resource revisions are cached across edits. A maximum
of one active render and one latest queued revision bounds pending work. This
is whole-document rendering off the UI thread, not incremental Rust layout.

Only the current source, committed settings and view revision may replace the
preview. Typing invalidates old consumers immediately, before debounce. Exact
source comparisons also catch edits made without an input event. Source-file
replacement, image commit/rollback, successful settings changes, IME composition
and page suspension invalidate pending publication. Failed rendering retains
the last successful preview and leaves source recoverable.

## Failure and recovery

A render or startup failure displays a preview error and a **Restart preview**
control. New input retries nonterminal renderer errors; a terminated worker
requires explicit restart. Missing worker support, policy restrictions and
timeouts do not trigger an automatic synchronous live-render fallback. Explicit
Markdown downloads remain available. A worker that exceeds its existing
30-second startup/render deadline is terminated, including when its active
consumer has been superseded. Restart creates a replacement worker from the
current committed state.
There is no automatic retry loop.

Suspension terminates and drops the worker. Resume renders the then-current
source with a new worker. In-flight callbacks from the old worker cannot publish
into the resumed document. File handles and source-replacement undo retain their
separate, existing lifecycle rules.

The import-free worker bootstrap is a classic Blob script. Its trusted
binding module is loaded with dynamic `import()` inside that worker. This avoids
the module-worker startup failure observed in an opaque-origin Chromium probe,
without JavaScript `eval`, `importScripts`, external scripts, or weakening the
preview sandbox. The outer CSP explicitly allows `worker-src blob:`; native
preview documents still have their own script-free CSP and sandbox.

## Explicit exports and settings

**Save HTML** awaits a preview committed for the matching source revision before
cloning the workspace. Repeated requests for that pending revision are deduplicated;
a newer source edit cancels the stale download. Saving does not fall back to a
synchronous HTML render. The saved file contains a reusable worker bootstrap,
not a live worker, pending request, restart control, or extra runtime payload.

**Publish HTML** and **Export PDF** now use a separate worker when the rebuilt
controller is paired with the matching background-capable runtime. A complete
snapshot of current source, committed settings, image resources and fonts reaches
the shared native renderer without waiting for or scraping the preview. Published
HTML uses document typography/color settings, not viewing zoom or a forced view
theme. PDF paper, margins, metadata, font-weight pins and navigation settings are
preserved. The export worker transfers an exact owned UTF-8/PDF byte buffer;
borrowed native or pooled memory is never transferred or exposed.

There is one explicit export at a time, with no unbounded queue. Repeated keyboard
requests for that job produce one download. Preview rendering runs independently,
so heavy PDF computation cannot starve ongoing editing/preview work. A separate
status and **Cancel export** control remain visible while computation runs.
Cancellation terminates that worker without changing source, settings, resources
or the preview. Source changes, composition, successful settings/resource changes,
and page suspension cancel a pending export. A final revision check catches edits
made without an input event. A source edit followed by undo cannot resurrect a
cancelled download. View zoom/theme changes do not invalidate a document export.

Exports have the existing 30-second initialization/render deadlines. Invalid or
oversized results, diagnostics, PDF signatures and Unicode are rejected. Failure
or timeout restores the controls and offers an explicit retry; it never invokes
the synchronous renderer as a hidden fallback. Export diagnostics remain attached
to their output instead of replacing newer preview findings. Each export worker
is released after completion or failure rather than retaining another idle WASM
instance. Suspension never automatically replays an export on resume.

Source downloads and Save HTML remain available during exports. Save HTML waits
only for its own matching preview, removes transient export controls/state from
the detached copy, and does not persist the temporarily disabled PDF button.
Repeated reopening creates one idle control set. A download request is not proof
of saving and does not clear the existing unsaved-work warning.

**Apply settings** still uses its synchronous native preflight and transactional
publication path. Main-thread initialization, resource preparation and explicit
legacy `html()`/`pdf()` calls also remain synchronous. The new toolbar path is not
an all-rendering-off-thread or reduced-peak-memory claim. At peak, the editor,
preview and export can each hold a WASM/resource instance.

Rebuild the embedded controller and use the matching package adapters together.
Updating package JavaScript alone cannot update a previously compiled controller.
Both exporters reject a pre-worker controller with `UNSUPPORTED_WASM_PACKAGE`,
even if the Markdown contains the protocol marker: old Save HTML code does not
await background rendering and cannot be paired safely with this bootstrap.
Existing exported workspaces do not self-upgrade. Exporter checks for the earlier
asynchronous-preview protocol remain unchanged; an older compiled controller may
still expose synchronous exports. Rebuild the controller to obtain the new export
UI. Legacy bootstrap callers that do not supply a preview worker factory keep
their synchronous API contract and do not advertise unsupported cancellation.

## Verification

From the repository root:

```sh
node --test wasm/interactive_preview.test.mjs \
  wasm/native_workspace_preview.test.mjs \
  wasm/native_workspace_preview_package.test.mjs
python wasm/native_workspace_preview_browser.py --mode content
```

The Node tests exercise production scheduling, bootstrap, controller and exporter
code with explicit DOM/native-ABI adapters, plus actual worker threads and empty
WASM instantiation. Package checks cover the dependency inventory and isolated
public-exporter imports; they do not substitute for a generated package build.

The browser probe runs actual Blob workers, dynamic module loading, the
production exporter/controller, downloads and repeated fresh-page reopening.
The shell/native rendering exports are explicit fixtures, not rebuilt Rust.
It verifies continued typing during deliberately busy rendering, stale-result
rejection, failure/retry, suspension and a genuinely blocked worker terminated
by an accelerated host deadline. The CSP is not removed or bypassed. Artifacts
are retained. `--mode file` separately attempts actual file navigation; a passing
content-mode run is not file-navigation acceptance or real back/forward-cache
proof. Native Rust rendering, full-suite/build gates, package size and real
native/WASM parity still require the project's DSR verification host.


For cancellable document exports:

```sh
node --test wasm/interactive_preview.test.mjs \
  wasm/native_workspace_export_worker.test.mjs \
  wasm/native_workspace_exports.test.mjs
python wasm/native_workspace_exports_browser.py --mode content
```

The export Node suites execute the actual transport, boot and renderer adapter
with real worker threads and empty WASM initialization. They cover byte transfer
ownership, independent previews, cancellation, hung-renderer termination,
settings/resource revisions, retries, diagnostics and legacy compatibility.
Both export suites are selected by the existing DSR package gate; its build,
size and parity checks are unchanged.

The retained browser probe runs the shipped controller, runtime and workers in
locally installed Chromium. Its shell and HTML/PDF ABI are explicit fixtures,
not Rust-generated output; the PDF envelope is labeled `%PDF-ADAPTER` and is not
valid PDF layout proof. Real Blob workers, image decoding, keyboard/button
interaction, cancellation, downloads, and repeated saved-byte reparsing execute
under the existing offline policy. The fixture explicitly shortens the worker
deadline to 1.4 seconds for the stuck-computation test; production keeps its
30-second default. Synthetic suspension events are not back/forward-cache proof.
Use `--browser PATH` for an installed Chromium executable and `--output NEW_DIR`
for create-only retained artifacts. The probe installs nothing and deletes no
artifacts. File-navigation and genuine Rust/WASM rendering still require their
separate verification gates.
