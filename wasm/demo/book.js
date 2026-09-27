import { createBookWorker } from "../book-worker.js";
import { createBookCollection } from "./book_collection.mjs";
import { createBookControls } from "./book_controls.mjs";
import {
  createBookInspectionControls,
  createBookInspectionPanel,
  createBookLinkControls,
  createBookLinkPanel,
} from "./book_inspection_controls.mjs";
import { createBookLibraryControls } from "./book_library_controls.mjs";
import { createBookPdfControls, createBookPdfPanel } from "./book_pdf_controls.mjs";
import { createBookPreviewControls } from "./book_preview_controls.mjs";
import { createBookSearchControls } from "./book_search_controls.mjs";

const collection = createBookCollection();
let library = null,
  preview = null,
  pdfProof = null,
  search = null,
  inspection = null,
  links = null;
const controls = createBookControls({
  root: document,
  worker: createBookWorker(),
  collection,
  confirm: (text) => window.confirm(text),
  onProjectReplaced: () => library?.detach(),
});
try {
  library = createBookLibraryControls({ root: document, controls, collection, window });
} catch {
  document.querySelector("#library-status").textContent =
    "Local library is unavailable. Editing, publishing and source downloads remain available; download a source project to keep your work.";
}
try {
  preview = createBookPreviewControls({
    root: document,
    controls,
    collection,
    window,
    worker: createBookWorker({ maxOutputBytes: 64 * 1024 * 1024 }),
  });
} catch {
  document.querySelector("#preview-status").textContent =
    "Preview is unavailable. Editing, local saves and publication exports remain available.";
}
// Native PDF proofing has a dedicated client: cancelling it cannot retire
// publication, HTML-site preview, inspection or link checking.
let pdfWorker = null;
try {
  createBookPdfPanel(document);
  pdfWorker = createBookWorker();
  pdfProof = createBookPdfControls({ root: document, controls, collection, worker: pdfWorker });
} catch {
  pdfWorker?.dispose();
  const status = document.querySelector("#book-pdf-status");
  if (status) status.textContent =
    "Book PDF proofing is unavailable. Ordinary PDF/EPUB/site exports and source editing remain available.";
}
try {
  search = createBookSearchControls({
    root: document,
    controls,
    collection,
    confirm: (text) => window.confirm(text),
  });
} catch {
  document.querySelector("#search-status").textContent =
    "Source search is unavailable. Chapter editing, source downloads and publication remain available.";
}
try {
  createBookInspectionPanel(document);
  inspection = createBookInspectionControls({
    root: document,
    controls,
    collection,
    worker: createBookWorker({ maxOutputBytes: 2 * 1024 * 1024 }),
  });
} catch {
  const status = document.querySelector("#inspection-status");
  if (status)
    status.textContent =
      "Inspection is unavailable. Editing, local saves, preview and publication exports remain available.";
}
try {
  createBookLinkPanel(document);
  links = createBookLinkControls({
    root: document,
    controls,
    collection,
    worker: createBookWorker({ maxOutputBytes: 4 * 1024 * 1024 }),
  });
} catch {
  const status = document.querySelector("#links-status");
  if (status)
    status.textContent =
      "Expanded link checking is unavailable. Source inspection, editing, saves and publication exports remain available.";
}
window.addEventListener("pagehide", (event) => {
  if (event.persisted) {
    pdfProof?.suspend();
    links?.suspend();
    inspection?.suspend();
    search?.suspend();
    preview?.suspend();
    library?.suspend();
    controls.suspend();
  } else {
    pdfProof?.dispose();
    links?.dispose();
    inspection?.dispose();
    search?.dispose();
    preview?.dispose();
    library?.dispose();
    controls.dispose();
  }
});
window.addEventListener("pageshow", (event) => {
  if (event.persisted) {
    pdfProof?.resume();
    library?.resume();
    preview?.resume();
    search?.resume();
    inspection?.resume();
    links?.resume();
  }
});
