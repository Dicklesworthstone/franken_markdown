import { createWorkerRenderer } from "../document_worker.mjs";
import { markdownDownload, readMarkdownFile } from "./flow_document.mjs";
import { captureDocumentImageFiles } from "./local_image_sources.mjs";

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
    downloadMarkdown: "#download-markdown", openMarkdown: "#open-markdown", markdownFile: "#markdown-file", fileStatus: "#file-status",
    markdown: "#markdown", render: "#render", downloadPdf: "#download-pdf", downloadHtml: "#download-html",
    preview: "#preview", diagnostics: "#diagnostics", status: "#status",
    sourceSize: "#source-size", previewMeta: "#preview-meta", font: "#font",
    darkMode: "#dark-mode", title: "#title", author: "#author", customCss: "#custom-css",
    allowHtml: "#allow-html", lineNumbers: "#line-numbers",
    imageFiles: "#image-files", imageFolder: "#image-folder", imageDocumentPath: "#image-document-path",
    applyImageFolder: "#apply-image-folder", clearImages: "#clear-images", cancelImages: "#cancel-images",
    imageStatus: "#image-status", imageReferences: "#image-references", imageFolderHelp: "#image-folder-help",
  })) {
    els[key] = doc.querySelector(selector);
    if (!els[key]) throw new Error(`franken_markdown demo is missing required element ${selector}`);
  }
  let alive = true, suspended = false, composing = false, generation = 0;
  let timer = null, lastPdfUrl = null, lastHtmlUrl = null, preparedHtml = null, observed = null;
  let fileJob = null, cleanSource, originalSource, sourceFilename = "franken-markdown.md";
  let sourceDownload = null, sourceDownloadId = 0;
  let imageAssets = [], imageJob = null;
  const directorySupported = "webkitdirectory" in els.imageFolder;
  const clients = { html: null, pdf: null }, jobs = { html: null, pdf: null }, listeners = [];
  const fields = ["markdown", "font", "darkMode", "title", "author", "customCss", "allowHtml", "lineNumbers"];
  const raw = () => Object.fromEntries(fields.map(key =>
    [key, key === "allowHtml" || key === "lineNumbers" ? els[key].checked : els[key].value]));
  const unchanged = expected => {
    const current = raw();
    return fields.every(key => current[key] === expected[key]);
  };
  const current = (kind, job) => alive && !suspended && !composing && !imageJob && jobs[kind] === job
    && generation === job.generation && unchanged(job.raw);
  const listen = (target, event, handler) => {
    target.addEventListener(event, handler);
    listeners.push([target, event, handler]);
  };
  function status(message) {
    els.render.textContent = jobs.html ? "Cancel render" : "Render";
    els.downloadPdf.textContent = jobs.pdf ? "Cancel PDF" : "Download PDF";
    els.render.disabled = els.downloadPdf.disabled = !alive || suspended || composing || imageJob !== null;
    els.openMarkdown.disabled = els.markdownFile.disabled = els.downloadMarkdown.disabled = !alive || suspended || composing;
    els.status.textContent = jobs.html && jobs.pdf ? "rendering HTML and PDF in workers"
      : jobs.html ? "rendering HTML in worker" : jobs.pdf ? "rendering PDF in worker" : message;
    refreshImageControls();
  }
  function refreshImageControls() {
    const blocked = !alive || suspended || composing;
    els.imageFiles.disabled = blocked;
    els.imageFolder.disabled = els.imageDocumentPath.disabled = blocked || !directorySupported;
    els.applyImageFolder.disabled = els.imageFolder.disabled || !els.imageFolder.files?.length;
    els.clearImages.disabled = blocked || (!imageAssets.length && !imageJob);
    els.cancelImages.disabled = blocked || imageJob === null;
  }
  function retireImageImport(message) {
    const previous = imageJob;
    imageJob = null;
    if (!previous) return false;
    win.clearTimeout(previous.timer);
    previous.abort.abort();
    if (message && alive) els.imageStatus.textContent = message;
    refreshImageControls();
    return true;
  }
  function releaseImages() {
    retireImageImport();
    imageAssets = [];
    els.imageFiles.value = els.imageFolder.value = els.imageDocumentPath.value = "";
    els.imageReferences.textContent = "";
    els.imageStatus.textContent = "Image access revoked. Select images again to include them in previews and exports.";
    refreshImageControls();
  }
  function clearImages() {
    if (!alive) return false;
    releaseImages();
    schedulePreview(true);
    return true;
  }
  function cancelImages() {
    if (!alive || !retireImageImport("Image loading cancelled. The previous image selection is retained.")) return false;
    schedulePreview(true);
    return true;
  }
  async function importImages(files, { folder = false, documentPath = "" } = {}) {
    if (!alive || suspended || composing) return false;
    // A new selection retires old publications immediately. The previous grant
    // survives until the complete new selection is admitted and read.
    invalidate();
    const job = { raw: raw(), generation, abort: new AbortController() };
    imageJob = job;
    const owns = () => alive && !suspended && imageJob === job;
    const fresh = () => owns() && !composing && generation === job.generation && unchanged(job.raw);
    els.imageStatus.textContent = "Reading selected images; rendering resumes when the complete selection is ready…";
    status("reading local images");
    try {
      const selected = captureDocumentImageFiles(files, { folder, documentPath });
      job.timer = win.setTimeout(() => {
        if (!owns()) return;
        retireImageImport("Image loading timed out. The previous selection is retained; choose the files again to retry.");
        schedulePreview(true);
      }, 30000);
      const assets = await selected.read(job.abort.signal);
      if (!fresh()) {
        if (owns()) els.imageStatus.textContent = "The draft changed during image loading; nothing was installed. Choose the images again.";
        return false;
      }
      imageAssets = assets;
      els.imageReferences.textContent = selected.references.join("\n");
      els.imageFiles.value = els.imageFolder.value = "";
      els.imageStatus.textContent = `${selected.count} local image${selected.count === 1 ? "" : "s"} authorized (${selected.totalBytes.toLocaleString()} bytes).`
        + (folder ? ` Paths are relative to ${documentPath || "the selected folder root"}.` : " Paths use the selected filenames.")
        + " Preview and HTML/PDF exports use these same bytes. Check the render diagnostics for unsupported or invalid images.";
      return true;
    } catch (error) {
      if (owns()) els.imageStatus.textContent = `${error?.code ?? "IMAGE_READ_ERROR"}: ${error instanceof Error ? error.message : "Images could not be read."} The previous image selection is retained.`;
      return false;
    } finally {
      if (imageJob === job) {
        win.clearTimeout(job.timer);
        imageJob = null;
        schedulePreview(true);
      }
    }
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
  function retireSourceDownload(output) {
    if (output && !output.revoked) {
      output.revoked = true;
      win.URL.revokeObjectURL(output.url);
    }
  }
  function revokeMarkdown() {
    const previous = sourceDownload;
    sourceDownload = null;
    retireSourceDownload(previous);
  }
  function revokeHtml() {
    preparedHtml = null;
    els.downloadHtml.hidden = true;
    els.downloadHtml.removeAttribute("href");
    els.downloadHtml.removeAttribute("download");
    if (lastHtmlUrl !== null) win.URL.revokeObjectURL(lastHtmlUrl);
    lastHtmlUrl = null;
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
    if (fileJob) {
      fileJob = null;
      els.fileStatus.textContent = "File opening cancelled because the draft or page changed. Choose the file again.";
    }
    retireImageImport("Image loading cancelled because the draft or page changed. Choose the images again.");
    generation++;
    clearTimer();
    cancel("html");
    cancel("pdf");
    revokeHtml();
    revokePdf();
    revokeMarkdown();
    els.preview.srcdoc = "";
    els.previewMeta.textContent = "preview out of date";
  }
  function options(kind, snapshot, images) {
    const common = { font: snapshot.font, darkMode: snapshot.darkMode,
      title: snapshot.title.trim() || undefined, allowRawHtml: snapshot.allowHtml,
      ...(images.length ? { pdfImages: images } : {}) };
    return kind === "html" ? { ...common,
      customCss: snapshot.customCss.trim() ? snapshot.customCss : undefined }
      : { ...common, author: snapshot.author.trim() || undefined,
        codeLineNumbers: snapshot.lineNumbers, metadataEpochSeconds: 1700000000 };
  }
  async function run(kind) {
    if (!alive || suspended || composing || imageJob) return false;
    if (kind === "html") clearTimer();
    cancel(kind);
    const captured = raw();
    if (kind === "html") observed = captured;
    if (!captured.markdown.trim()) {
      invalidate();
      els.previewMeta.textContent = "empty source";
      diagnostics([]);
      status("empty source");
      return false;
    }
    const job = { raw: captured, generation, images: imageAssets, abort: new AbortController() };
    jobs[kind] = job;
    if (kind === "html") {
      revokeHtml();
      els.preview.srcdoc = "";
      els.previewMeta.textContent = "rendering";
    }
    status("");
    let completion = "render failed";
    try {
      if (!clients[kind] || clients[kind].disposed)
        clients[kind] = factory({ maxPendingOperations: 1, maxPendingBytes: 64 * 1024 * 1024 });
      const output = await clients[kind].render(kind, captured.markdown, options(kind, captured, job.images), { signal: job.abort.signal });
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
        const text = output.text();
        const base = captured.title.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "franken-markdown";
        const filename = output.filename(base);
        const url = win.URL.createObjectURL(output.blob());
        if (!current(kind, job)) { win.URL.revokeObjectURL(url); return false; }
        lastHtmlUrl = url;
        preparedHtml = { raw: captured, generation: job.generation };
        els.preview.srcdoc = text;
        els.downloadHtml.href = url;
        els.downloadHtml.download = filename;
        els.downloadHtml.hidden = false;
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
  function schedulePreview(force = false) {
    if (!alive || suspended) return;
    // A textarea's blur can emit change after its input preview has completed.
    // Retiring that identical result would consume the first download click.
    if (!force && observed && unchanged(observed)) return;
    observed = raw();
    invalidate();
    els.sourceSize.textContent = `${new TextEncoder().encode(els.markdown.value).byteLength} bytes`;
    status(composing ? "composing text" : "waiting for edits");
    if (!composing) timer = win.setTimeout(() => { timer = null; void run("html"); }, 180);
  }
  function downloadMarkdown() {
    if (!alive || suspended || composing) return false;
    const captured = raw(), revision = generation, id = ++sourceDownloadId;
    const fresh = () => alive && !suspended && !composing && id === sourceDownloadId
      && revision === generation && unchanged(captured);
    let output = null, link = null, requested = false;
    try {
      const document = markdownDownload({ filename: sourceFilename,
        source: captured.markdown === cleanSource ? originalSource : captured.markdown });
      revokeMarkdown();
      if (!fresh()) return false;
      output = { url: win.URL.createObjectURL(document.blob), revoked: false };
      if (!fresh()) return false;
      sourceDownload = output;
      link = doc.createElement("a");
      link.href = output.url;
      link.download = document.filename;
      doc.body.appendChild(link);
      if (!fresh()) return false;
      link.click();
      requested = true;
      if (fresh()) els.fileStatus.textContent = `Markdown download requested: ${document.filename}.`;
      return true;
    } catch (error) {
      if (fresh()) els.fileStatus.textContent = `${error?.code ?? "SOURCE_DOWNLOAD_ERROR"}: ${error instanceof Error ? error.message : "Markdown download failed."}`;
      return false;
    } finally {
      link?.remove();
      if (output && (!requested || !fresh())) {
        if (sourceDownload === output) sourceDownload = null;
        retireSourceDownload(output);
      }
    }
  }
  listen(els.downloadMarkdown, "click", () => { downloadMarkdown(); });
  async function openMarkdown(file) {
    if (!alive || suspended || composing || !file) return false;
    // The later source selection owns replacement. Retire an earlier image
    // read before capturing fileJob, so its cleanup cannot cancel this open.
    cancelImages();
    const job = { raw: raw(), generation };
    fileJob = job; // A newer selection owns publication, even if an old read settles last.
    const owns = () => alive && !suspended && fileJob === job;
    const fresh = () => owns() && !composing && generation === job.generation && unchanged(job.raw);
    els.fileStatus.textContent = "Reading local Markdown…";
    try {
      const next = await readMarkdownFile(file);
      if (!fresh()) return false;
      if (job.raw.markdown !== cleanSource && !win.confirm(
        `Replace the current draft with ${next.filename}? Choose Download Markdown first if you need to keep the current source.`,
      )) {
        els.fileStatus.textContent = "Open cancelled; current draft retained.";
        return false;
      }
      if (!fresh()) return false;
      fileJob = null;
      els.markdown.value = next.source;
      cleanSource = els.markdown.value; // Browsers normalize CR/CRLF in textareas.
      originalSource = next.source;
      sourceFilename = next.filename;
      els.title.value = next.filename.replace(/\.(md|markdown|txt)$/i, "") || next.filename;
      els.allowHtml.checked = false;
      releaseImages();
      // Even an identical import is a document replacement. Retire exports
      // before notifying Document Lab through its existing input listener.
      schedulePreview(true);
      els.markdown.dispatchEvent(new Event("input", { bubbles: true }));
      els.fileStatus.textContent = `Opened ${next.filename}. Raw HTML is off. Select its local images below to include them in the preview and exports.`;
      els.markdown.focus();
      return true;
    } catch (error) {
      if (owns()) els.fileStatus.textContent = `${error?.code ? `${error.code}: ` : ""}${error instanceof Error ? error.message : "The file could not be opened."}`;
      return false;
    } finally {
      if (fileJob === job) {
        if (owns() && !fresh()) els.fileStatus.textContent = "The draft changed during file loading; choose the file again.";
        fileJob = null;
      }
    }
  }
  listen(els.openMarkdown, "click", () => {
    if (alive && !suspended && !composing) els.markdownFile.click();
  });
  listen(els.markdownFile, "change", () => {
    const file = els.markdownFile.files?.[0];
    els.markdownFile.value = ""; // A failed/cancelled file can be selected again.
    if (file) void openMarkdown(file);
  });
  listen(els.imageFiles, "change", () => {
    const files = els.imageFiles.files;
    // Admission captures at most 128 entries synchronously before its first
    // await. Do not copy an arbitrarily large FileList just to reject it.
    if (files?.length) void importImages(files);
    els.imageFiles.value = "";
  });
  listen(els.imageFolder, "change", () => {
    cancelImages();
    if (alive && !suspended && directorySupported && els.imageFolder.files?.length)
      els.imageStatus.textContent = "Folder selected. Set the Markdown path within it, then use this image folder. The current grant is unchanged until loading succeeds.";
    refreshImageControls();
  });
  listen(els.imageDocumentPath, "input", () => { cancelImages(); });
  listen(els.imageDocumentPath, "change", () => { cancelImages(); });
  listen(els.applyImageFolder, "click", () => {
    if (directorySupported && els.imageFolder.files?.length)
      void importImages(els.imageFolder.files, { folder: true, documentPath: els.imageDocumentPath.value });
  });
  listen(els.clearImages, "click", clearImages);
  listen(els.cancelImages, "click", cancelImages);
  for (const key of fields) {
    listen(els[key], "input", () => schedulePreview());
    listen(els[key], "change", () => schedulePreview());
  }
  listen(els.markdown, "compositionstart", () => {
    composing = true;
    invalidate();
    status("composing text");
  });
  listen(els.markdown, "compositionend", () => { composing = false; schedulePreview(true); });
  listen(els.render, "click", () => {
    if (jobs.html) { cancel("html"); status("render cancelled"); }
    else void run("html");
  });
  listen(els.downloadPdf, "click", () => {
    if (jobs.pdf) { cancel("pdf"); status("PDF cancelled"); }
    else void run("pdf");
  });
  listen(els.downloadHtml, "click", event => {
    if (!alive || suspended || composing || imageJob || !preparedHtml
      || preparedHtml.generation !== generation || !unchanged(preparedHtml.raw)) {
      event.preventDefault();
      revokeHtml();
    }
  });
  listen(win, "pagehide", () => {
    suspended = true;
    composing = false;
    releaseImages();
    invalidate();
    for (const kind of ["html", "pdf"]) { clients[kind]?.dispose(); clients[kind] = null; }
    els.allowHtml.checked = false;
    status("paused; source retained");
  });
  listen(win, "pageshow", event => {
    if (event.persisted) { suspended = false; schedulePreview(true); }
  });
  if (!els.markdown.value) els.markdown.value = sampleMarkdown;
  if (!directorySupported)
    els.imageFolderHelp.textContent = "Folder selection is unavailable in this browser. Select individual images above; only their filenames are authorized.";
  originalSource = cleanSource = els.markdown.value;
  schedulePreview();
  return Object.freeze({
    renderPreview: () => run("html"), downloadPdf: () => run("pdf"), openMarkdown, downloadMarkdown,
    importImages, clearImages, cancelImages,
    dispose() {
      if (!alive) return;
      alive = false;
      releaseImages();
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
