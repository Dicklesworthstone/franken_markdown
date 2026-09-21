// Separate entrypoint from the preview: source recovery remains available even
// when its WASM artifact, worker startup or Canvas renderer cannot load.
import { createSourceControls, createSourceEditingControls } from "./flow_document.mjs";
import { createDraftControls } from "./flow_draft_controls.mjs";
import { createFileControls } from "./flow_file_controls.mjs";

const source = document.querySelector("#source"),
  filename = document.querySelector("#source-filename");
const status = document.querySelector("#source-status");
let controls = null,
  drafts = null,
  editing = null,
  files = null,
  retained = null;
function start() {
  files?.dispose();
  files = null;
  controls?.dispose();
  drafts?.dispose();
  editing?.dispose();
  const initial =
    retained && retained.view === source.value && retained.document.filename === filename.value
      ? retained.document
      : undefined;
  retained = null;
  controls = createSourceControls({
    sourceEditor: source,
    filename,
    open: document.querySelector("#open-markdown"),
    prepare: document.querySelector("#prepare-markdown"),
    download: document.querySelector("#source-download"),
    status,
    initialDocument: initial,
    confirmReplace: (next) =>
      window.confirm(
        `Replace the current editor with ${next.filename}? Download the current Markdown first to keep a separate copy.`,
      ),
    // The preview revokes its old document's image grant synchronously on this
    // event, BEFORE source controls emit the ordinary input notification.
    onReplace: () => source.dispatchEvent(new Event("fmd-document-replaced")),
  });
  editing = createSourceEditingControls({
    sourceEditor: source,
    undo: document.querySelector("#source-undo"),
    redo: document.querySelector("#source-redo"),
    query: document.querySelector("#source-find"),
    ignoreCase: document.querySelector("#source-find-insensitive"),
    previous: document.querySelector("#source-find-previous"),
    next: document.querySelector("#source-find-next"),
    replacement: document.querySelector("#source-replacement"),
    replace: document.querySelector("#source-replace"),
    replaceAll: document.querySelector("#source-replace-all"),
    status: document.querySelector("#source-edit-status"),
  });
  try {
    files = createFileControls({
      root: document,
      window,
      controls,
      sourceEditor: source,
      filename,
    });
  } catch {
    status.textContent =
      "Direct file saving is unavailable. Source import and Markdown downloads remain available.";
  }
  drafts = createDraftControls({
    sourceEditor: source,
    filename,
    remember: document.querySelector("#remember-draft"),
    refresh: document.querySelector("#refresh-draft"),
    restore: document.querySelector("#restore-draft"),
    forget: document.querySelector("#forget-draft"),
    save: document.querySelector("#save-draft"),
    status: document.querySelector("#draft-status"),
    readDocument: () => controls.snapshot(),
    restoreDocument: (next) => controls.replace(next),
    confirmRestore: (next) =>
      window.confirm(
        `Restore ${next.filename} from browser draft storage and replace the current editor? Current image access will be revoked.`,
      ),
    confirmForget: (name) =>
      window.confirm(
        `Forget the stored draft ${name ?? ""}? This removes only its stored source, not the current editor. Download Markdown first for a separate copy.`,
      ),
  });
}
window.addEventListener("pagehide", (event) => {
  // Preserve only the in-memory source for bfcache, never handles/image grants.
  // Do not claim an asynchronous unload save completed. Only a prior IndexedDB
  // transaction acknowledgment is durable enough to report as saved.
  if (event.persisted) {
    try {
      retained = { document: controls.snapshot(), view: source.value };
    } catch {
      retained = null;
    }
  }
  files?.dispose();
  files = null;
  controls?.dispose();
  drafts?.dispose();
  editing?.dispose();
  controls = null;
  drafts = null;
  editing = null;
});
window.addEventListener("pageshow", (event) => {
  if (event.persisted) start();
});
start();
