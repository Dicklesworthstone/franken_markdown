# Cancellable document settings

Native workspaces whose runtime advertises `settingsMode: "worker"` now route
**Apply settings** through `applySettingsAsync`. Heavy preflight rendering is no
longer invoked synchronously by the editor controller. Typography, navigation,
metadata and paper controls retain their existing meaning.

The dialog remains open during preflight, with a busy status and **Cancel**.
The committed settings, saved resource payload, document title/language and
previous preview remain unchanged until the runtime successfully publishes its
transaction. A repeated submission does not queue a second job. Publication
and Document Lab controls cannot start an overlapping document operation.

Cancel or Escape terminates the staged operation. Editing the form cancels its
old preflight while retaining the new draft. Source/view changes, source
replacement, composition and page suspension retire unfinished work too.
Pre-publication checks compare exact source, the source anchor, display options,
form values and panel identity. An edit/undo round trip with input events cancels
the job; silent changes to source, metadata or geometry are rejected before the
runtime is allowed to publish. Late callbacks cannot close a reopened dialog,
clear a newer draft or restore another operation's controls.

A successful preflight updates title/language, retires old preview continuations,
invalidates the prior Document Lab report and closes the dialog. The textarea
and its undo history are never rewritten. Unchanged Apply makes no native call;
omitted defaults and unedited newline-containing metadata remain exact.

**Save HTML** during preflight still saves committed settings, not an unfinished
form. Reopening reconstructs an idle settings panel and enabled publication
controls. Existing explicitly synchronous/legacy workspaces retain their prior
settings path. A runtime advertising workers but missing its async/cancellation
methods fails explicitly; worker failures never fall back to UI-thread rendering.

## Verification

```sh
python wasm/native_workspace_async_settings_browser.py --chromium /path/to/chromium
```

The probe runs the production controller in installed Chromium with an explicit
native transaction/API fixture and a real CPU-blocking Blob worker. It covers
argument propagation, responsive cancellation, unchanged-state preservation,
publication controls, silent mutations, old/new-operation races, metadata,
save/reopen, failure paths and legacy compatibility. Passing it establishes
controller integration, not native Rust rendering or the production runtime's
transaction implementation. Existing runtime/worker tests cover those separate
JavaScript boundaries; generated-WASM acceptance still requires a matching build.

The probe uses content injection and offline mode, not file navigation. Lifecycle
and composition events are synthetic rather than OS IME or real BFCache evidence.
It installs nothing, makes no external requests and retains a receipt/downloads
under a newly created output directory. Use `--controller` to test a pinned prior
controller and `--output` only for a nonexisting artifact directory.
