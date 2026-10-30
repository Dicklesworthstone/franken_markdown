import { createWorkerRenderer } from "../document_worker.mjs";

const sampleMarkdown = `# franken_markdown demo

This preview is rendered by the same browser package API that applications use.

| Item | Status | Notes |
|---|:---:|---|
| HTML | ready | Self-contained preview with embedded subset fonts |
| PDF | ready | Deterministic bytes, tagged text, links, and outlines |
| WASM | first-class | No filesystem, process, threads, or network in the core |

> Blockquotes, tables, links, lists, and code blocks use the default theme.

\`\`\`rust
fn main() {
    println!("render markdown");
}
\`\`\`

- Toggle the serif font for long-form reading.
- Disable dark CSS if the host page needs a fixed light palette.
- Download a PDF generated from the same Markdown source.
`;

// The factory seam is for host integration and controller tests, not a renderer
// selected by page content. The production default only starts module workers.
export function createDemoController(doc, win, factory = createWorkerRenderer) {
  const els = {};
  for (const [key, selector] of Object.entries({
    markdown: "#markdown", render: "#render", downloadPdf: "#download-pdf",
    preview: "#preview", diagnostics: "#diagnostics", status: "#status",
    sourceSize: "#source-size", previewMeta: "#preview-meta", font: "#font",
    darkMode: "#dark-mode", title: "#title", author: "#author", customCss: "#custom-css",
    allowHtml: "#allow-html", lineNumbers: "#line-numbers",
  })) {
    els[key] = doc.querySelector(selector);
    if (!els[key]) throw new Error(`franken_markdown demo is missing required element ${selector}`);
  }
  let alive = true, suspended = false, composing = false, generation = 0;
  let timer = null, lastPdfUrl = null;
  const clients = { html: null, pdf: null }, jobs = { html: null, pdf: null }, listeners = [];
  const fields = ["markdown", "font", "darkMode", "title", "author", "customCss", "allowHtml", "lineNumbers"];
  const raw = () => Object.fromEntries(fields.map(key =>
    [key, key === "allowHtml" || key === "lineNumbers" ? els[key].checked : els[key].value]));
  const unchanged = expected => {
    const current = raw();
    return fields.every(key => current[key] === expected[key]);
  };
  const current = (kind, job) => alive && !suspended && !composing && jobs[kind] === job
    && generation === job.generation && unchanged(job.raw);
  const listen = (target, event, handler) => {
    target.addEventListener(event, handler);
    listeners.push([target, event, handler]);
  };
  function status(message) {
    els.render.textContent = jobs.html ? "Cancel render" : "Render";
    els.downloadPdf.textContent = jobs.pdf ? "Cancel PDF" : "Download PDF";
    els.render.disabled = els.downloadPdf.disabled = !alive || suspended || composing;
    els.status.textContent = jobs.html && jobs.pdf ? "rendering HTML and PDF in workers"
      : jobs.html ? "rendering HTML in worker" : jobs.pdf ? "rendering PDF in worker" : message;
  }
  function diagnostics(items) {
    els.diagnostics.replaceChildren();
    for (const diagnostic of items.length ? items : [{ severity: "empty", message: "No diagnostics." }]) {
      const item = doc.createElement("li");
      item.className = diagnostic.severity === "empty" ? "empty" : diagnostic.severity === "error" ? "error" : "";
      item.textContent = diagnostic.severity === "empty" ? diagnostic.message
        : `${diagnostic.severity} ${diagnostic.start}-${diagnostic.end}: ${diagnostic.message}`;
      els.diagnostics.appendChild(item);
    }
  }
  function errorMessage(error) {
    els.diagnostics.replaceChildren();
    const item = doc.createElement("li");
    item.className = "error";
    item.textContent = `${error?.code ? `${error.code}: ` : ""}${error instanceof Error ? error.message : String(error)}`;
    els.diagnostics.appendChild(item);
  }
  function clearTimer() {
    if (timer !== null) win.clearTimeout(timer);
    timer = null;
  }
  function revokePdf() {
    if (lastPdfUrl !== null) win.URL.revokeObjectURL(lastPdfUrl);
    lastPdfUrl = null;
  }
  function cancel(kind) {
    const job = jobs[kind];
    jobs[kind] = null; // Fence publication before abort listeners can settle work.
    if (job) {
      job.abort.abort();
      clients[kind]?.dispose();
      clients[kind] = null;
    }
  }
  function invalidate() {
    generation++;
    clearTimer();
    cancel("html");
    cancel("pdf");
    revokePdf();
    els.preview.srcdoc = "";
    els.previewMeta.textContent = "preview out of date";
  }
  function options(kind, snapshot) {
    const common = { font: snapshot.font, darkMode: snapshot.darkMode,
      title: snapshot.title.trim() || undefined, allowRawHtml: snapshot.allowHtml };
    return kind === "html" ? { ...common,
      customCss: snapshot.customCss.trim() ? snapshot.customCss : undefined }
      : { ...common, author: snapshot.author.trim() || undefined,
        codeLineNumbers: snapshot.lineNumbers, metadataEpochSeconds: 1700000000 };
  }
  async function run(kind) {
    if (!alive || suspended || composing) return false;
    if (kind === "html") clearTimer();
    cancel(kind);
    const captured = raw();
    if (!captured.markdown.trim()) {
      invalidate();
      els.previewMeta.textContent = "empty source";
      diagnostics([]);
      status("empty source");
      return false;
    }
    const job = { raw: captured, generation, abort: new AbortController() };
    jobs[kind] = job;
    if (kind === "html") {
      els.preview.srcdoc = "";
      els.previewMeta.textContent = "rendering";
    }
    status("");
    let completion = "render failed";
    try {
      if (!clients[kind] || clients[kind].disposed)
        clients[kind] = factory({ maxPendingOperations: 1 });
      const output = await clients[kind].render(kind, captured.markdown, options(kind, captured), { signal: job.abort.signal });
      if (!current(kind, job)) {
        if (jobs[kind] === job) {
          els.preview.srcdoc = "";
          els.previewMeta.textContent = "source or settings changed; render again";
        }
        return false;
      }
      if (kind === "html") {
        // The existing preview iframe remains sandboxed. Worker output is not
        // inserted into the host DOM, and exporting does not grant raw HTML trust.
        els.preview.srcdoc = output.text();
      } else {
        revokePdf();
        const url = win.URL.createObjectURL(output.blob());
        if (!current(kind, job)) { win.URL.revokeObjectURL(url); return false; }
        lastPdfUrl = url;
        const link = doc.createElement("a");
        link.href = url;
        const base = captured.title.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "franken-markdown";
        link.download = output.filename(base);
        try {
          doc.body.appendChild(link);
          if (!current(kind, job)) { revokePdf(); return false; }
          link.click();
        } finally { link.remove(); }
      }
      els.previewMeta.textContent = `${output.sourceLength} source bytes -> ${output.bytes.byteLength} ${kind} bytes`;
      diagnostics(output.diagnostics);
      completion = kind === "pdf" ? "PDF ready" : "ready";
      return true;
    } catch (error) {
      if (current(kind, job)) errorMessage(error);
      return false;
    } finally {
      if (jobs[kind] === job) {
        jobs[kind] = null;
        status(unchanged(captured) ? completion : "source or settings changed; render again");
      }
    }
  }
  function schedulePreview() {
    if (!alive || suspended) return;
    invalidate();
    els.sourceSize.textContent = `${new TextEncoder().encode(els.markdown.value).byteLength} bytes`;
    status(composing ? "composing text" : "waiting for edits");
    if (!composing) timer = win.setTimeout(() => { timer = null; void run("html"); }, 180);
  }
  for (const key of fields) {
    listen(els[key], "input", schedulePreview);
    listen(els[key], "change", schedulePreview);
  }
  listen(els.markdown, "compositionstart", () => {
    composing = true;
    invalidate();
    status("composing text");
  });
  listen(els.markdown, "compositionend", () => { composing = false; schedulePreview(); });
  listen(els.render, "click", () => {
    if (jobs.html) { cancel("html"); status("render cancelled"); }
    else void run("html");
  });
  listen(els.downloadPdf, "click", () => {
    if (jobs.pdf) { cancel("pdf"); status("PDF cancelled"); }
    else void run("pdf");
  });
  listen(win, "pagehide", () => {
    suspended = true;
    composing = false;
    invalidate();
    for (const kind of ["html", "pdf"]) { clients[kind]?.dispose(); clients[kind] = null; }
    els.allowHtml.checked = false;
    status("paused; source retained");
  });
  listen(win, "pageshow", event => {
    if (event.persisted) { suspended = false; schedulePreview(); }
  });
  if (!els.markdown.value) els.markdown.value = sampleMarkdown;
  schedulePreview();
  return Object.freeze({
    renderPreview: () => run("html"), downloadPdf: () => run("pdf"),
    dispose() {
      if (!alive) return;
      alive = false;
      invalidate();
      for (const kind of ["html", "pdf"]) { clients[kind]?.dispose(); clients[kind] = null; }
      for (const [target, event, handler] of listeners) target.removeEventListener(event, handler);
      status("demo disposed; source retained");
    },
  });
}

if (typeof document !== "undefined" && typeof window !== "undefined") {
  createDemoController(document, window);
}
