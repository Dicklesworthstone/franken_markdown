import { createWorkerFlowSession } from "../flow-worker.js";
import { FlowCanvasRenderer } from "../flow-canvas.js";
import { createPreviewController } from "./flow_preview_controller.mjs";

const source = document.querySelector("#source"), viewport = document.querySelector("#viewport");
const canvas = document.querySelector("#preview"), extent = document.querySelector("#extent");
const status = document.querySelector("#status"), reading = document.querySelector("#reading");
const link = document.querySelector("#hit"), restart = document.querySelector("#restart");
let controller = null, painter = null, scheduled = 0;

function start() {
  painter = new FlowCanvasRenderer(canvas);
  controller = createPreviewController({ createSession: createWorkerFlowSession, painter, onState(state) {
    reading.textContent = state.reading;
    if (state.status === "ready") {
      const frame = state.frame;
      extent.style.height = `${Math.max(viewport.clientHeight, frame.totalBounds.y + frame.totalBounds.height)}px`;
      status.textContent = `Source ${frame.revision} · layout ${frame.layoutRevision} · ${frame.glyphs} viewport glyphs. Images remain placeholders until a host authorizes loading.`;
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
window.addEventListener("pagehide", () => { controller?.dispose(); if (scheduled) cancelAnimationFrame(scheduled); scheduled = 0; observer.disconnect(); });
window.addEventListener("pageshow", event => { if (event.persisted) { observer.observe(viewport); start(); } });
start();
