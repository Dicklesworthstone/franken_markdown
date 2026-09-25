# Recover a native workspace without rendering

**Save recovery copy** downloads the current source, committed settings, embedded
images/fonts and matching engine as an editable `.recovery.html` workspace. It is
available before native initialization and after a render or startup failure.
Unlike normal **Save HTML**, it does not call, await or cancel the renderer.

This closes a data-retention gap: Markdown alone does not contain separately
embedded image/font bytes, while ordinary Save HTML waits for a current preview.
A failed or stalled engine must not trap those resources inside the open editor.
Recovery remains an explicit local download; no browser-storage write or network
upload is introduced.

## What the recovery copy preserves

The controller snapshots the current lossless source anchor and committed runtime
JSON. Source BOM, CRLF and non-BMP Unicode remain intact. Unused embedded resources
are retained for later edits/redo, and unfinished settings drafts are not applied.
The detached saved copy omits the preview and displays an explicit recovery notice
instead of presenting a stale rendering as current. The live preview and document
are not changed. Report panels, unfinished jobs and dynamic toolbar controls are
not persisted; they are reconstructed once on reopen.

An earlier Save HTML waiting on a stalled render is superseded when the recovery
copy is downloaded. A late render completion cannot cause a surprise second
download. Recovery does not stop an independently running render, export or
settings preflight.

A working engine regenerates the preview on reopen and removes the recovery
marker. A broken engine remains broken: recovery preserves its bytes, it does not
repair, upgrade or replace them. Even damaged runtime JSON can be retained without
parsing it. Literal HTML script-tokenizer characters in that JSON are escaped;
valid JSON retains the same decoded values. Trusted embedded binding JavaScript
remains executable application code, not an untrusted-input sandbox.

When native payload data exists but the native runtime handoff is missing, the
controller does not silently switch to the reduced JavaScript Markdown parser or
browser-print PDF export. It reports the failure and exposes recovery plus source
download. Ordinary lightweight workspaces retain their existing behavior and do
not show native recovery controls.

## Admission and document identity

Recovery refuses source edits still under composition or a suspended page. Before
cloning the document it validates the native source's 32 MiB UTF-8 limit and the
embedded runtime text's 256 MiB limit. Final serialized HTML is limited to 256 MiB.
Lone UTF-16 surrogates are rejected rather than silently changed by Blob encoding.
The same native-source and output guards apply to ordinary workspace saving.

Source value, lossless anchor identity and committed runtime text are rechecked
before download. A serialization-time document change fails explicitly instead of
saving a mixture of revisions. Bounds cover retained input/output, not peak browser
memory: DOM cloning and HTML serialization still allocate temporary copies.
A download request is not proof that the browser or user saved the file.

## Verification

```sh
node --test wasm/native_workspace_recovery.test.mjs
python wasm/native_workspace_recovery_browser.py --chromium /path/to/chromium
```

The Node tests execute the private production UTF-8 counter, including full-size
32 MiB strings, malformed surrogate sequences and exact byte boundaries. The
Chromium probe executes the full production controller with explicit failing,
hanging, uninitialized and healthy native API fixtures. It checks downloads,
resource/source identity, repeated reopen, script-like data, stale-save suppression,
settings snapshots, admission and lightweight compatibility. The oversized-source
browser case supplies the string through a scoped textarea getter to isolate
controller admission from Chromium's unrelated huge-textarea layout cost.

Browser checks use offline content injection, not file navigation, and do not
prove Rust rendering, production native-runtime transactions, rebuilt-WASM package
acceptance, browser crash recovery or persistence without an explicit download.
Composition and page lifecycle are controller states, not OS IME/BFCache proof.
The probe installs nothing and retains its downloads and receipt in a new output
directory. `--controller` accepts a pinned controller for regression checks.
