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

**Publish HTML**, **Export PDF**, and **Apply settings** continue to use the
existing synchronous native renderer and transactional settings path. They do
not wait for or scrape a stale background preview. They may still occupy the
main thread. A second main-thread WASM instance remains initialized for these
operations; this change is not an all-rendering-off-thread or reduced-memory
claim.

Rebuild the embedded controller and use the matching package adapters together.
Updating package JavaScript alone cannot update a previously compiled controller.
Both exporters reject a pre-worker controller with `UNSUPPORTED_WASM_PACKAGE`,
even if the Markdown contains the protocol marker: old Save HTML code does not
await background rendering and cannot be paired safely with this bootstrap.
Existing exported workspaces do not self-upgrade. Legacy bootstrap callers that
do not supply a preview worker factory keep their synchronous API contract.

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
