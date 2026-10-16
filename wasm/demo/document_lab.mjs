import { createDocumentInspector } from "../document-inspector.js";

const MAX_VISIBLE = 100, MAX_TEXT = 2048;
function display(value) {
  const text = typeof value === "string" || typeof value === "number" ? String(value) : "";
  if (text.length <= MAX_TEXT) return text;
  let end = MAX_TEXT;
  const last = text.charCodeAt(end - 1);
  if (last >= 0xd800 && last <= 0xdbff) end--;
  return `${text.slice(0, end)}… [full text in JSON]`;
}

// Controller only: all document interpretation belongs to the native inspector.
// The injected factory is a host/test seam, never selected by document contents.
export function createDocumentLabController(doc, win, factory = createDocumentInspector) {
  const els = {};
  for (const [key, id] of Object.entries({
    source: "markdown", panel: "document-lab", open: "open-document-lab",
    inspect: "inspect-document", download: "download-inspection", status: "inspection-status",
    results: "inspection-results", metrics: "inspection-metrics", structure: "inspection-structure",
    outline: "inspection-outline", quality: "inspection-quality", accessibility: "inspection-accessibility",
  })) {
    els[key] = doc.getElementById(id);
    if (!els[key]) throw new Error(`Document Lab is missing #${id}`);
  }
  let alive = true, suspended = false, composing = false, generation = 0;
  let client = null, pending = null, retained = null, downloadUrl = null;
  const listeners = [];
  const available = () => alive && !suspended && !composing;
  const fresh = entry => available() && entry && entry.generation === generation && entry.source === els.source.value;
  const active = entry => pending === entry && fresh(entry);
  function listen(target, name, fn) {
    target.addEventListener(name, fn);
    listeners.push([target, name, fn]);
  }
  function revoke() {
    if (downloadUrl !== null) win.URL.revokeObjectURL(downloadUrl);
    downloadUrl = null;
  }
  function controls(message) {
    els.inspect.textContent = pending ? "Cancel inspection" : "Inspect current draft";
    els.inspect.disabled = !available();
    els.download.disabled = !fresh(retained) || pending !== null;
    els.panel.setAttribute("aria-busy", String(pending !== null));
    if (message !== undefined) els.status.textContent = message;
  }
  function clearReport() {
    retained = null;
    revoke();
    els.results.hidden = true;
    for (const key of ["metrics", "structure", "outline", "quality", "accessibility"])
      els[key].replaceChildren();
  }
  function closeClient() {
    const previous = client;
    client = null;
    previous?.dispose();
  }
  function invalidate(message) {
    generation++;
    const previous = pending;
    pending = null; // Fence old completions before abort listeners can run.
    if (previous) {
      previous.abort.abort();
      closeClient();
    }
    clearReport();
    controls(message);
  }
  function append(parent, tag, text) {
    const element = doc.createElement(tag);
    element.textContent = display(text);
    parent.appendChild(element);
    return element;
  }
  function metric(parent, label, value) {
    append(parent, "dt", label);
    append(parent, "dd", value);
  }
  function list(parent, values, describe, empty) {
    if (!values.length) append(parent, "li", empty);
    for (const item of values.slice(0, MAX_VISIBLE)) append(parent, "li", describe(item));
    if (values.length > MAX_VISIBLE)
      append(parent, "li", `Showing ${MAX_VISIBLE} of ${values.length}; the JSON report retains every entry.`);
  }
  function render(output) {
    const stats = output.stats;
    for (const [label, value] of [
      ["Source bytes", output.sourceLength], ["Words", stats.words], ["Lines", stats.lines],
      ["Characters", stats.characters], ["Sentences", stats.sentences],
      ["Estimated reading time (seconds)", stats.reading_time_secs],
      ["Estimated speaking time (seconds)", stats.speaking_time_secs],
      ["Flesch reading ease", stats.flesch_reading_ease],
      ["Flesch–Kincaid grade estimate", stats.flesch_kincaid_grade],
      ["Native readability label", stats.reading_ease_label],
    ]) metric(els.metrics, label, value);
    for (const [key, label] of Object.entries({ headings_total: "Headings", paragraphs: "Paragraphs",
      code_blocks: "Code blocks", tables: "Tables", images: "Images", lists: "Lists",
      links_total: "Links", task_items_total: "Tasks", task_items_completed: "Completed tasks" })) {
      if (Number.isSafeInteger(stats.structure[key]) && stats.structure[key] >= 0)
        metric(els.structure, label, stats.structure[key]);
    }
    list(els.outline, stats.outline, item =>
      `H${item.level}: ${display(item.text)} — #${display(item.slug)}`, "No headings reported.");
    const describe = item => [item.severity, item.code, item.message ?? item.detail]
      .filter(value => value !== undefined && value !== "").map(display).join(" — ");
    list(els.quality, stats.findings, describe, "No source-quality findings reported.");
    list(els.accessibility, output.accessibility.findings, describe, "No accessibility findings reported by this source audit.");
  }
  async function inspect() {
    if (!available()) return false;
    invalidate("Inspecting the captured draft in a worker…");
    const job = { source: els.source.value, generation, abort: new AbortController() };
    pending = job;
    controls();
    try {
      if (!client || client.disposed) {
        const next = factory({ maxPendingOperations: 1 });
        if (!active(job)) {
          next.dispose();
          if (pending === job) invalidate("Source changed. Inspect the current draft again.");
          return false;
        }
        client = next;
      }
      const output = await client.inspect(job.source, { signal: job.abort.signal });
      if (!active(job)) {
        if (pending === job) invalidate("Source changed. Inspect the current draft again.");
        return false;
      }
      render(output);
      if (!active(job)) {
        if (pending === job) invalidate("Source changed. Inspect the current draft again.");
        return false;
      }
      retained = { source: job.source, generation: job.generation, output };
      els.results.hidden = false;
      const total = output.stats.findings.length + output.accessibility.findings.length;
      controls(`Current draft inspected: ${total} findings reported. Source analysis only; not PDF certification.`);
      return true;
    } catch (error) {
      if (active(job)) {
        clearReport();
        controls(`Inspection failed: ${display(error?.code)} ${display(error?.message ?? error)}. No complete report is available.`);
      } else if (pending === job) invalidate("Source changed. Inspect the current draft again.");
      return false;
    } finally {
      if (pending === job) { pending = null; controls(); }
    }
  }
  function cancel() {
    if (!alive) return;
    invalidate("Inspection cancelled. No report was published.");
  }
  function downloadReport() {
    if (!available() || pending) return false;
    const captured = retained;
    if (!fresh(captured)) {
      invalidate("No current report. Inspect the current draft before downloading.");
      return false;
    }
    let link;
    try {
      revoke();
      const url = win.URL.createObjectURL(captured.output.blob());
      downloadUrl = url;
      if (retained !== captured || !fresh(captured)) {
        invalidate("Source changed. The report was not downloaded.");
        return false;
      }
      link = doc.createElement("a");
      link.href = url;
      link.download = "document.inspection.json";
      doc.body.appendChild(link);
      // Browser/host adapters can reenter during URL creation or DOM work.
      if (retained !== captured || !fresh(captured)) {
        invalidate("Source changed. The report was not downloaded.");
        return false;
      }
      link.click();
      if (retained === captured && fresh(captured)) controls("Report download requested for the inspected draft. The JSON may contain document text.");
      return true;
    } catch (error) {
      revoke();
      controls(`Report download failed: ${display(error?.message ?? error)}`);
      return false;
    } finally { link?.remove(); }
  }
  function checkCurrent() {
    if (!alive) return;
    if (retained && !fresh(retained)) invalidate("Source changed. Inspect the current draft again.");
  }
  for (const event of ["input", "change"]) listen(els.source, event, () => {
    if (alive) invalidate(composing ? "Finish composing before inspection." : "Source changed. Inspect the current draft again.");
  });
  listen(els.source, "compositionstart", () => {
    composing = true; invalidate("Finish composing before inspection.");
  });
  listen(els.source, "compositionend", () => {
    composing = false; invalidate("Draft ready for inspection.");
  });
  listen(els.inspect, "click", () => { if (pending) cancel(); else void inspect(); });
  listen(els.download, "click", downloadReport);
  listen(els.open, "click", () => { els.panel.open = true; checkCurrent(); });
  listen(els.panel, "toggle", checkCurrent);
  listen(win, "focus", checkCurrent);
  listen(win, "pagehide", () => {
    suspended = true; composing = false;
    invalidate("Document Lab paused; source retained.");
    closeClient();
  });
  listen(win, "pageshow", event => {
    if (event.persisted) { suspended = false; controls("Inspect the current draft to create a new report."); }
  });
  clearReport();
  controls("Inspect the current draft for structure, readability and accessibility findings.");
  return Object.freeze({ inspect, cancel, downloadReport,
    dispose() {
      if (!alive) return;
      alive = false;
      invalidate("Document Lab closed; source retained.");
      closeClient();
      for (const [target, event, fn] of listeners) target.removeEventListener(event, fn);
    },
  });
}

if (typeof document !== "undefined" && typeof window !== "undefined") {
  createDocumentLabController(document, window);
}
