import { createRenderer } from "../franken_markdown.js";

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

const els = {
  markdown: requiredElement("#markdown"),
  render: requiredElement("#render"),
  downloadPdf: requiredElement("#download-pdf"),
  preview: requiredElement("#preview"),
  diagnostics: requiredElement("#diagnostics"),
  status: requiredElement("#status"),
  sourceSize: requiredElement("#source-size"),
  previewMeta: requiredElement("#preview-meta"),
  font: requiredElement("#font"),
  darkMode: requiredElement("#dark-mode"),
  title: requiredElement("#title"),
  author: requiredElement("#author"),
  customCss: requiredElement("#custom-css"),
  allowHtml: requiredElement("#allow-html"),
  lineNumbers: requiredElement("#line-numbers")
};

let renderer = null;
let renderTimer = 0;
let lastPdfUrl = null;

function requiredElement(selector) {
  const element = document.querySelector(selector);
  if (element === null) {
    throw new Error(`franken_markdown demo is missing required element ${selector}`);
  }
  return element;
}

els.markdown.value = sampleMarkdown;
setBusy(true, "loading wasm package");
createRenderer()
  .then((created) => {
    renderer = created;
    setBusy(false, "ready");
    schedulePreview();
  })
  .catch((error) => {
    setBusy(false, "wasm load failed");
    showError(error);
  });

els.render.addEventListener("click", () => {
  void renderPreview();
});

els.downloadPdf.addEventListener("click", () => {
  void downloadPdf();
});

for (const input of [
  els.markdown,
  els.font,
  els.darkMode,
  els.title,
  els.author,
  els.customCss,
  els.allowHtml,
  els.lineNumbers
]) {
  input.addEventListener("input", schedulePreview);
  input.addEventListener("change", schedulePreview);
}

function schedulePreview() {
  window.clearTimeout(renderTimer);
  updateSourceSize();
  renderTimer = window.setTimeout(() => {
    void renderPreview();
  }, 180);
}

async function renderPreview() {
  if (renderer === null) {
    return;
  }
  const markdown = els.markdown.value;
  const options = renderOptions();
  if (markdown.trim() === "") {
    els.preview.srcdoc = "";
    els.previewMeta.textContent = "empty source";
    renderDiagnostics([]);
    return;
  }
  setBusy(true, "rendering html");
  try {
    const output = await renderer.renderHtml(markdown, options);
    els.preview.srcdoc = output.text();
    els.previewMeta.textContent = `${output.sourceLength} source bytes -> ${output.bytes.byteLength} html bytes`;
    renderDiagnostics(output.diagnostics);
    setBusy(false, "ready");
  } catch (error) {
    setBusy(false, "render failed");
    showError(error);
  }
}

async function downloadPdf() {
  if (renderer === null) {
    return;
  }
  const markdown = els.markdown.value;
  if (markdown.trim() === "") {
    showError(new Error("Markdown input is empty; add content before downloading PDF."));
    return;
  }
  setBusy(true, "rendering pdf");
  try {
    const output = await renderer.renderPdf(markdown, renderOptions());
    if (lastPdfUrl !== null) {
      URL.revokeObjectURL(lastPdfUrl);
    }
    lastPdfUrl = URL.createObjectURL(output.blob());
    const link = document.createElement("a");
    link.href = lastPdfUrl;
    link.download = output.filename(filenameBase());
    document.body.appendChild(link);
    link.click();
    link.remove();
    els.previewMeta.textContent = `${output.sourceLength} source bytes -> ${output.bytes.byteLength} pdf bytes`;
    renderDiagnostics(output.diagnostics);
    setBusy(false, "pdf ready");
  } catch (error) {
    setBusy(false, "pdf failed");
    showError(error);
  }
}

function renderOptions() {
  const customCss = els.customCss.value.trim() === "" ? undefined : els.customCss.value;
  return {
    font: els.font.value,
    darkMode: els.darkMode.value,
    title: textOrUndefined(els.title.value),
    author: textOrUndefined(els.author.value),
    customCss,
    allowRawHtml: els.allowHtml.checked,
    codeLineNumbers: els.lineNumbers.checked,
    metadataEpochSeconds: 1700000000
  };
}

function textOrUndefined(value) {
  const text = String(value).trim();
  return text === "" ? undefined : text;
}

function filenameBase() {
  return textOrUndefined(els.title.value)
    ?.toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    || "franken-markdown";
}

function renderDiagnostics(diagnostics) {
  els.diagnostics.replaceChildren();
  if (diagnostics.length === 0) {
    const item = document.createElement("li");
    item.className = "empty";
    item.textContent = "No diagnostics.";
    els.diagnostics.appendChild(item);
    return;
  }
  for (const diagnostic of diagnostics) {
    const item = document.createElement("li");
    item.className = diagnostic.severity === "error" ? "error" : "";
    item.textContent = `${diagnostic.severity} ${diagnostic.start}-${diagnostic.end}: ${diagnostic.message}`;
    els.diagnostics.appendChild(item);
  }
}

function showError(error) {
  els.diagnostics.replaceChildren();
  const item = document.createElement("li");
  item.className = "error";
  item.textContent = error instanceof Error ? error.message : String(error);
  els.diagnostics.appendChild(item);
}

function updateSourceSize() {
  els.sourceSize.textContent = `${new TextEncoder().encode(els.markdown.value).byteLength} bytes`;
}

function setBusy(busy, message) {
  els.status.textContent = message;
  els.render.disabled = busy;
  els.downloadPdf.disabled = busy;
}
