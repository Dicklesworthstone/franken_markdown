import { createBookWorker } from "../book-worker.js";
import { createBookControls } from "./book_controls.mjs";
import { createBookCollection } from "./book_collection.mjs";
import { createBookLibraryControls } from "./book_library_controls.mjs";
import { createBookPreviewControls } from "./book_preview_controls.mjs";

const collection = createBookCollection();
let library = null, preview = null;
const controls = createBookControls({ root: document, worker: createBookWorker(), collection,
  confirm: text => window.confirm(text), onProjectReplaced: () => library?.detach() });
try { library = createBookLibraryControls({ root: document, controls, collection, window }); }
catch {
  document.querySelector("#library-status").textContent = "Local library is unavailable. Editing, publishing and source downloads remain available; download a source project to keep your work.";
}
try { preview = createBookPreviewControls({ root: document, controls, collection, window,
  worker: createBookWorker({ maxOutputBytes: 64 * 1024 * 1024 }) }); }
catch {
  document.querySelector("#preview-status").textContent = "Preview is unavailable. Editing, local saves and publication exports remain available.";
}
window.addEventListener("pagehide", event => {
  if (event.persisted) { preview?.suspend(); library?.suspend(); controls.suspend(); }
  else { preview?.dispose(); library?.dispose(); controls.dispose(); }
});
window.addEventListener("pageshow", event => { if (event.persisted) { library?.resume(); preview?.resume(); } });
