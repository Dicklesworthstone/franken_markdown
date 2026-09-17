import { createWorkerFlowSession } from "../flow-worker.js";
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { FlowImageAssets } from "../flow-assets.js";
import { createPreviewController } from "./flow_preview_controller.mjs";
import { createLocalImageSources } from "./local_image_sources.mjs";
import { readFlowDocument } from "../flow-reader.js";
import { createReadingControls } from "./flow_reading_controls.mjs";
import { createExportControls } from "./flow_export_controls.mjs";

const source = document.querySelector("#source"), viewport = document.querySelector("#viewport");
const canvas = document.querySelector("#preview"), extent = document.querySelector("#extent");
const status = document.querySelector("#status"), reading = document.querySelector("#reading");
const link = document.querySelector("#hit"), restart = document.querySelector("#restart");
const files = document.querySelector("#images"), imageStatus = document.querySelector("#image-status");
const insertImages = document.querySelector("#insert-images"), clearImages = document.querySelector("#clear-images");
let controller = null, painter = null, scheduled = 0, localSources = createLocalImageSources([]), readingControls = null, exportControls = null;

function start() {
  readingControls?.dispose(); exportControls?.dispose();
  exportControls = createExportControls({ html: document.querySelector("#export-html"), pdf: document.querySelector("#export-pdf"),
    download: document.querySelector("#export-download"), status: document.querySelector("#export-status"), sourceEditor: source,
    exportDocument: (format, options, currentSource) => controller.exportDocument(format, options, currentSource)
  });
  readingControls = createReadingControls({ root: reading, panel: document.querySelector("#reader-panel"),
    query: document.querySelector("#find-text"), insensitive: document.querySelector("#find-insensitive"),
    previous: document.querySelector("#find-previous"), next: document.querySelector("#find-next"),
    outline: document.querySelector("#outline"), sourceButton: document.querySelector("#reading-source"),
    status: document.querySelector("#reading-status"), sourceEditor: source,
    getLocation: (index, snapshot) => controller.locateReading(index, snapshot, source.value),
    onNavigate: location => { viewport.scrollTop = Math.max(0, location.bounds.y - 16); update(); }
  });
  painter = new FlowCanvasRenderer(canvas);
  controller = createPreviewController({ createSession: createWorkerFlowSession, painter, readDocument: readFlowDocument,
    createAssets: session => localSources.count ? new FlowImageAssets(session, { load: localSources.load, retainSourceBytes: true }) : null,
    onState(state) {
    readingControls.update(state); exportControls.update(state);
    if (state.status === "ready") {
      const frame = state.frame, images = state.images;
      extent.style.height = `${Math.max(viewport.clientHeight, frame.totalBounds.y + frame.totalBounds.height)}px`;
      const detail = images?.status === "loading" ? "Loading explicitly selected images; text remains usable."
        : images?.status === "ready" ? `Images: ${images.loaded} loaded, ${images.failed} failed, ${images.skipped} unchanged or unauthorized.${images.errors.length ? ` First failure: ${images.errors[0].code}.` : ""}`
        : images?.status === "error" ? `Image loading: ${images.code}. Text remains available.`
        : "Only explicitly selected local images can be loaded. No image URLs are fetched.";
      status.textContent = `Source ${frame.revision} · layout ${frame.layoutRevision} · ${frame.glyphs} viewport glyphs. ${detail}`;
    } else if (state.status === "error") {
      status.textContent = `${state.error.code}: ${state.error.message} Last successful pixels are unchanged; your source is still in the editor. Restart explicitly after worker loss.`;
    } else if (state.status === "busy") status.textContent = "Updating preview in the worker; source edits remain available.";
    else if (state.status === "disposed") status.textContent = "Preview stopped. Source remains in the editor.";
  } });
  update();
}
function update() {
  if (!controller || controller.disposed || scheduled) return;
  scheduled = requestAnimationFrame(() => {
    scheduled = 0;
    const width = Math.max(40, viewport.clientWidth), height = Math.max(40, viewport.clientHeight);
    canvas.style.width = `${width}px`; canvas.style.height = `${height}px`;
    try { controller.update({ source: source.value, width, height, scrollY: viewport.scrollTop,
      pixelRatio: Math.min(8, Math.max(1, window.devicePixelRatio || 1)) }); }
    catch (error) { status.textContent = `${error.code ?? "PREVIEW_ERROR"}: ${error.message} Source retained.`; }
  });
}
function changeImages(next) {
  // Change the grant only after admission succeeds. Restart clears published
  // pixels before disposing bitmaps, and invalidates old worker generations.
  localSources = next;
  insertImages.disabled = next.count === 0; clearImages.disabled = next.count === 0;
  imageStatus.textContent = next.count ? `${next.count} local image files authorized. Insert references or use their exact filenames. Nothing is uploaded.`
    : "No local images authorized. Network image loading is disabled.";
  if (!controller || controller.disposed) start(); else controller.restart();
  update();
}
files.addEventListener("change", () => {
  try { changeImages(createLocalImageSources(files.files)); }
  catch (error) { imageStatus.textContent = `${error.code ?? "IMAGE_ERROR"}: ${error.message}. The previous grant is unchanged.`; }
});
clearImages.addEventListener("click", () => { files.value = ""; changeImages(createLocalImageSources([])); });
insertImages.addEventListener("click", () => {
  source.setRangeText(`\n\n${localSources.references.join("\n\n")}\n`, source.selectionStart, source.selectionEnd, "end");
  // Programmatic textarea edits must reach source downloads and autosave too.
  source.dispatchEvent(new Event("input", { bubbles: true })); source.focus();
});
source.addEventListener("fmd-document-replaced", () => {
  // A new file or recovered draft does not inherit the previous document's
  // image authorization or retained native payloads, even for matching names.
  files.value = ""; exportControls?.invalidate();
  try { changeImages(createLocalImageSources([])); }
  catch (error) {
    controller?.dispose();
    status.textContent = `Preview restart failed after source replacement: ${error.message}. Original Markdown remains available in the source controls.`;
  }
});
source.addEventListener("input", update);
viewport.addEventListener("scroll", update, { passive: true });
const observer = new ResizeObserver(update); observer.observe(viewport);
restart.addEventListener("click", () => {
  if (controller?.disposed) start(); else controller.restart();
  update();
});
canvas.addEventListener("click", async event => {
  const active = painter, frame = active?.frame;
  if (!frame) return;
  const area = canvas.getBoundingClientRect();
  if (!area.width || !area.height) return;
  try {
    const hit = await active.hitTest((event.clientX - area.left) * frame.width / area.width,
      (event.clientY - area.top) * frame.height / area.height);
    if (active !== painter) return;
    link.textContent = hit.linkTarget ? `Link target (not opened): ${hit.linkTarget}`
      : hit.hit ? `Item ${hit.hit.itemIndex}; selection offsets are fragment-local, not original Markdown.` : "No text or link at this point.";
  } catch (error) { link.textContent = `${error.code ?? "HIT_ERROR"}: ${error.message}`; }
});
window.addEventListener("pagehide", () => { controller?.dispose(); readingControls?.dispose(); exportControls?.dispose(); if (scheduled) cancelAnimationFrame(scheduled); scheduled = 0; observer.disconnect(); });
window.addEventListener("pageshow", event => { if (event.persisted) { observer.observe(viewport); start(); } });
start();
