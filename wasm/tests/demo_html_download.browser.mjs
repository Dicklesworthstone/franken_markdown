// Run the actual main demo, worker and renderer facade with caller-supplied
// generated bindings. No renderer fixture or main-thread fallback is installed.
// A released package proves compatibility with that release, not current-source
// native/WASM qualification. The output records both generated artifact hashes.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, mkdir } from "node:fs/promises";
import { resolve, sep, extname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

assert.ok(process.env.FMD_WASM_PKG, "FMD_WASM_PKG must name an existing generated pkg directory");
const pkg = resolve(process.env.FMD_WASM_PKG);
const wasmRoot = fileURLToPath(new URL("../", import.meta.url));
const output = process.env.FMD_BROWSER_OUTPUT ?? "/tmp/fmd-demo-html-download";
await mkdir(output, { recursive: true });
const generated = {};
for (const name of ["franken_markdown.js", "franken_markdown_bg.wasm"]) {
  const bytes = await readFile(resolve(pkg, name));
  generated[name] = { bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
}
const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const browser = await chromium.launch({ headless: true,
  ...(process.env.FMD_CHROMIUM ? { executablePath: process.env.FMD_CHROMIUM } : {}) });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, acceptDownloads: true,
  userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)" });
const checks = [], errors = [], denied = [];
const check = (name, actual, expected) => { assert.deepEqual(actual, expected, name); checks.push(name); };
let page;
try {
  await context.route("**/*", async route => {
    const url = new URL(route.request().url());
    if (url.origin !== "https://fmd-tests.invalid" || !url.pathname.startsWith("/wasm/")) {
      denied.push(url.href); return route.abort();
    }
    const relative = decodeURIComponent(url.pathname.slice("/wasm/".length));
    const root = relative.startsWith("pkg/") ? pkg : wasmRoot;
    const path = resolve(root, relative.startsWith("pkg/") ? relative.slice(4) : relative);
    if (!path.startsWith(root.replace(/\/$/, "") + sep)) { denied.push(url.href); return route.abort(); }
    try {
      const body = await readFile(path);
      const contentType = ({ ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript",
        ".wasm": "application/wasm", ".css": "text/css" })[extname(path)] ?? "application/octet-stream";
      await route.fulfill({ status: 200, contentType, body });
    } catch (error) { errors.push(`Missing route ${url.pathname}: ${error.code}`); await route.abort(); }
  });
  await context.addInitScript(() => {
    const post = Worker.prototype.postMessage;
    window.demoWorkerMessages = 0;
    Worker.prototype.postMessage = function (...args) {
      window.demoWorkerMessages++;
      return Reflect.apply(post, this, args);
    };
    const revoke = URL.revokeObjectURL;
    window.demoRevokedUrls = [];
    URL.revokeObjectURL = function (url) { window.demoRevokedUrls.push(url); return revoke.call(this, url); };
  });
  page = await context.newPage(); page.setDefaultTimeout(10000);
  page.on("pageerror", error => errors.push(error.message));
  const download = page.locator("#download-html"), source = page.locator("#markdown");
  await page.goto("https://fmd-tests.invalid/wasm/demo/index.html");
  await download.waitFor({ state: "visible" });
  check("actual module worker initialized generated renderer", await page.evaluate(() => demoWorkerMessages > 0), true);
  assert.match(await page.locator("#preview").evaluate(node => node.srcdoc), /data:font\/[^;]+;base64,/);
  checks.push("default stylesheet embeds fonts in the actual renderer output");
  const html = '<span id="raw-proof">raw café😀</span>';
  await source.fill(`# Browser export\n\nUnicode café😀.\n\n${html}\n\n| A | B |\n|---|---|\n|one|two|`);
  await page.locator("#title").fill("Browser export");
  await page.locator("#custom-css").fill("body { color: rgb(12, 34, 56); }");
  await page.locator("#font").selectOption("serif");
  await page.locator("#dark-mode").selectOption("disabled");
  await download.waitFor({ state: "visible" });
  const preview = await page.locator("#preview").evaluate(node => node.srcdoc);
  check("preview retains sandbox", await page.locator("#preview").getAttribute("sandbox"), "");
  assert.match(preview, /body \{ color: rgb\(12, 34, 56\); \}/); checks.push("custom CSS retained in actual renderer output");
  assert.doesNotMatch(preview, /@font-face/); checks.push("custom stylesheet replaces the default theme and fonts");
  assert.match(preview, /<span>raw café😀<\/span>/);
  assert.doesNotMatch(preview, /id="raw-proof"/); checks.push("default safe-markup sanitizer removes unapproved source attributes");
  const before = await page.evaluate(() => demoWorkerMessages);
  for (let i = 0; i < 2; i++) {
    const event = page.waitForEvent("download"); await download.click(); const saved = await event;
    check(`HTML download ${i + 1} name`, saved.suggestedFilename(), "browser-export.html");
    const path = resolve(output, `download-${i + 1}.html`); await saved.saveAs(path);
    check(`HTML download ${i + 1} bytes equal sandboxed preview`, await readFile(path), Buffer.from(preview));
  }
  check("HTML activation starts no extra worker operation", await page.evaluate(() => demoWorkerMessages), before);
  const oldUrl = await download.getAttribute("href");
  await page.locator("#title").evaluate(node => { node.value = "Silent change"; });
  check("silent input change prevents native anchor activation", await download.evaluate(node =>
    node.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))), false);
  check("silent change hides stale link", await download.isHidden(), true);
  check("silent change revokes its exact URL", await page.evaluate(url => demoRevokedUrls.includes(url), oldUrl), true);
  await page.locator("#render").click(); await download.waitFor({ state: "visible" });
  await page.locator("#allow-html").check(); await download.waitFor({ state: "visible" });
  assert.match(await page.locator("#preview").evaluate(node => node.srcdoc), /<span id="raw-proof">raw café😀<\/span>/);
  checks.push("raw HTML remains explicit opt-in in the generated document");
  const htmlUrl = await download.getAttribute("href");
  const pdfEvent = page.waitForEvent("download"); await page.locator("#download-pdf").click(); const pdf = await pdfEvent;
  const pdfPath = resolve(output, "download.pdf"); await pdf.saveAs(pdfPath);
  check("independent generated PDF download has PDF header", (await readFile(pdfPath)).subarray(0, 5).toString(), "%PDF-");
  check("PDF completion retains the HTML URL", await download.getAttribute("href"), htmlUrl);
  await source.evaluate(node => node.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true })));
  check("synthetic composition hides completed HTML link", await download.isHidden(), true);
  check("composition revokes HTML URL", await page.evaluate(url => demoRevokedUrls.includes(url), htmlUrl), true);
  await source.evaluate(node => node.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true })));
  await download.waitFor({ state: "visible" });
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
  check("synthetic page suspension hides the link", await download.isHidden(), true);
  check("suspension resets raw HTML opt-in", await page.locator("#allow-html").isChecked(), false);
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true })));
  await download.waitFor({ state: "visible" });
  check("resume uses retained source", await source.inputValue(), `# Browser export\n\nUnicode café😀.\n\n${html}\n\n| A | B |\n|---|---|\n|one|two|`);
  // Real File input, real generated-engine HTML/PDF; no file handles or network grants.
  const picker = page.locator("#markdown-file"), fileStatus = page.locator("#file-status");
  const localText = '# Local café\r\n\r\nOpened from a UTF-8 file.\r\n\r\n<span id="file-raw-proof">file markup</span>\r\n';
  const local = { name: "Local Notes.md", mimeType: "text/markdown", buffer: Buffer.from(localText) };
  const retainedSource = await source.inputValue(), retainedUrl = await download.getAttribute("href");
  page.once("dialog", dialog => dialog.dismiss());
  await picker.setInputFiles(local);
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.includes("Open cancelled"));
  check("declining replacement preserves edited source", await source.inputValue(), retainedSource);
  check("declining replacement preserves usable export", await download.getAttribute("href"), retainedUrl);
  await page.locator("#custom-css").fill("");
  await page.locator("#allow-html").check();
  await download.waitFor({ state: "visible" });
  const replacedUrl = await download.getAttribute("href");
  page.once("dialog", dialog => dialog.accept());
  await picker.setInputFiles(local);
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.startsWith("Opened Local Notes.md"));
  await download.waitFor({ state: "visible" });
  check("UTF-8 file enters the textarea with browser newline normalization", await source.inputValue(), localText.replace(/\r\n/g, "\n"));
  check("file input cleared for same-file retry", await picker.inputValue(), "");
  check("filename becomes export title", await page.locator("#title").inputValue(), "Local Notes");
  check("import resets prior raw HTML opt-in", await page.locator("#allow-html").isChecked(), false);
  check("import retires exact previous export URL", await page.evaluate(url => demoRevokedUrls.includes(url), replacedUrl), true);
  const localPreview = await page.locator("#preview").evaluate(node => node.srcdoc);
  assert.match(localPreview, /Opened from a UTF-8 file/); checks.push("file source reaches actual generated HTML renderer");
  assert.doesNotMatch(localPreview, /id="file-raw-proof"/); checks.push("imported markup inherits safe rendering default");
  const htmlEvent = page.waitForEvent("download"); await download.click(); const localHtml = await htmlEvent;
  check("local file HTML download name", localHtml.suggestedFilename(), "local-notes.html");
  const localHtmlPath = resolve(output, "local-file.html"); await localHtml.saveAs(localHtmlPath);
  check("local file HTML download equals actual sandboxed preview bytes", await readFile(localHtmlPath), Buffer.from(localPreview));
  const localPdfEvent = page.waitForEvent("download"); await page.locator("#download-pdf").click(); const localPdf = await localPdfEvent;
  check("local file PDF download name", localPdf.suggestedFilename(), "local-notes.pdf");
  const localPdfPath = resolve(output, "local-file.pdf"); await localPdf.saveAs(localPdfPath);
  check("local file actual-engine PDF header", (await readFile(localPdfPath)).subarray(0, 5).toString(), "%PDF-");
  const currentUrl = await download.getAttribute("href");
  await picker.setInputFiles({ name: "bad.md", mimeType: "text/markdown", buffer: Buffer.from([0xc3, 0x28]) });
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.includes("INVALID_UNICODE"));
  check("invalid UTF-8 leaves current editor unchanged", await source.inputValue(), localText.replace(/\r\n/g, "\n"));
  check("invalid UTF-8 retains current HTML export", await download.getAttribute("href"), currentUrl);
  await picker.setInputFiles({ name: "large.md", mimeType: "text/markdown", buffer: Buffer.alloc(4 * 1024 * 1024 + 1) });
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.includes("BUDGET_EXCEEDED"));
  check("over-budget local file retains current HTML export", await download.getAttribute("href"), currentUrl);
  await picker.setInputFiles(local);
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.startsWith("Opened Local Notes.md"));
  await download.waitFor({ state: "visible" });
  check("same-file retry uses the clean imported baseline", await source.inputValue(), localText.replace(/\r\n/g, "\n"));
  check("identical import still revokes prior export", await page.evaluate(url => demoRevokedUrls.includes(url), currentUrl), true);
  // Only the file-read completion is delayed; generated renderer/worker remain real.
  await page.evaluate(() => {
    const read = File.prototype.arrayBuffer;
    File.prototype.arrayBuffer = function () {
      const bytes = read.call(this);
      if (this.name !== "delayed.md") return bytes;
      File.prototype.arrayBuffer = read;
      return new Promise((resolve, reject) => { window.demoReleaseFile = () => bytes.then(resolve, reject); });
    };
  });
  await picker.setInputFiles({ name: "delayed.md", mimeType: "text/markdown", buffer: Buffer.from("# Old local file") });
  await page.waitForFunction(() => typeof window.demoReleaseFile === "function");
  await source.fill("# More recent edits");
  await page.evaluate(async () => { await window.demoReleaseFile(); });
  check("controlled delayed file cannot replace more recent editor input", await source.inputValue(), "# More recent edits");
  check("delayed file does not claim a successful open", (await fileStatus.textContent()).includes("Opened delayed.md"), false);
  const sourceDownload = page.locator("#download-markdown");
  await download.waitFor({ state: "visible" });
  const sourceExportWorkerCount = await page.evaluate(() => demoWorkerMessages);
  const retainedHtml = await download.getAttribute("href");
  const markdownEvent = page.waitForEvent("download"); await sourceDownload.click(); const markdown = await markdownEvent;
  const editedMarkdownPath = resolve(output, "current-draft.md"); await markdown.saveAs(editedMarkdownPath);
  check("source download keeps the last imported filename", markdown.suggestedFilename(), "Local Notes.md");
  check("source download captures the current edited draft", await readFile(editedMarkdownPath), Buffer.from("# More recent edits"));
  check("Markdown download starts no worker request", await page.evaluate(() => demoWorkerMessages), sourceExportWorkerCount);
  check("Markdown download preserves the completed HTML URL", await download.getAttribute("href"), retainedHtml);
  const originalMarkdown = "\ufeff# Draft source\r\n\r\nCafe\u0301 😀\r\n";
  page.once("dialog", dialog => dialog.accept());
  await picker.setInputFiles({ name: "Draft.markdown", mimeType: "text/markdown", buffer: Buffer.from(originalMarkdown) });
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.startsWith("Opened Draft.markdown"));
  check("imported BOM is retained while textarea normalizes CRLF", await source.inputValue(), originalMarkdown.replace(/\r\n/g, "\n"));
  const originalEvent = page.waitForEvent("download"); await sourceDownload.click(); const originalDownload = await originalEvent;
  const originalPath = resolve(output, "original-source.markdown"); await originalDownload.saveAs(originalPath);
  check("unchanged source download retains the original filename", originalDownload.suggestedFilename(), "Draft.markdown");
  check("unchanged source download preserves exact UTF-8 BOM and CRLF bytes", await readFile(originalPath), Buffer.from(originalMarkdown));
  const editedSource = "# Edited source\n\nKeep these changes café😀.\n";
  await source.fill(editedSource);
  const editedEvent = page.waitForEvent("download"); await sourceDownload.click(); const editedDownload = await editedEvent;
  const editedPath = resolve(output, "source-edited.markdown"); await editedDownload.saveAs(editedPath);
  check("edited source download contains exact current textarea bytes", await readFile(editedPath), Buffer.from(editedSource));
  await source.fill(originalMarkdown.replace(/\r\n/g, "\n"));
  const revertEvent = page.waitForEvent("download"); await sourceDownload.click(); const reverted = await revertEvent;
  const revertedPath = resolve(output, "source-reverted.markdown"); await reverted.saveAs(revertedPath);
  check("exact source revert restores original BOM/CRLF download bytes", await readFile(revertedPath), Buffer.from(originalMarkdown));
  await source.fill(editedSource);
  page.once("dialog", dialog => dialog.accept());
  await picker.setInputFiles(editedPath);
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.startsWith("Opened source-edited.markdown"));
  check("saved Markdown reopens without losing source edits", await source.inputValue(), editedSource);
  check("reopened source takes the selected file identity", await page.locator("#title").inputValue(), "source-edited");
  await source.evaluate(node => node.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true })));
  check("source download is paused during synthetic composition", await sourceDownload.isDisabled(), true);
  await source.evaluate(node => node.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true })));
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
  check("source download is paused during synthetic suspension", await sourceDownload.isDisabled(), true);
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true })));
  check("source download resumes with retained source", await sourceDownload.isEnabled(), true);
  // An actual worker-resource failure; Markdown export uses no renderer fallback.
  const failedPage = await context.newPage(); let abortedWorkers = 0;
  await failedPage.route("**/document_worker_entry.js", route => { abortedWorkers++; return route.abort("failed"); });
  await failedPage.goto("https://fmd-tests.invalid/wasm/demo/index.html");
  await failedPage.locator("#markdown").fill("# Preserve despite worker failure\n\nLocal draft.\n");
  await failedPage.waitForFunction(() => document.querySelector("#status").textContent === "render failed");
  check("worker-failure control exercised a real module fetch failure", abortedWorkers > 0, true);
  check("failed worker leaves source download enabled", await failedPage.locator("#download-markdown").isEnabled(), true);
  const failedWorkerCount = await failedPage.evaluate(() => demoWorkerMessages);
  const recoveryEvent = failedPage.waitForEvent("download"); await failedPage.locator("#download-markdown").click(); const recovery = await recoveryEvent;
  const recoveryPath = resolve(output, "renderer-failed-source.md"); await recovery.saveAs(recoveryPath);
  check("source download survives actual worker failure with exact edited bytes", await readFile(recoveryPath), Buffer.from("# Preserve despite worker failure\n\nLocal draft.\n"));
  check("source download does not retry the failed renderer", await failedPage.evaluate(() => demoWorkerMessages), failedWorkerCount);
  await failedPage.close();
  await download.waitFor({ state: "visible" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: resolve(output, "mobile.png"), fullPage: true });
  check("mobile page has no horizontal overflow", await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  check("mobile file-open control is visible", await page.locator("#open-markdown").isVisible(), true);
  check("mobile source-download control is visible", await sourceDownload.isVisible(), true);
  check("browser errors", errors, []); check("unapproved network requests", denied, []);
  console.log(JSON.stringify({ browser: browser.version(), pkg, generated, checks: checks.length, names: checks,
    output, limitation: "Real file inputs and downloads, including one aborted worker-module fetch; one controlled delayed File.arrayBuffer and synthetic composition/page events, not real IME or actual bfcache. Generated artifact provenance must be qualified separately." }, null, 2));
} catch (error) {
  console.error(JSON.stringify({ completedChecks: checks, errors, denied,
    state: page && await page.evaluate(() => ({ status: document.querySelector("#status")?.textContent,
      diagnostics: document.querySelector("#diagnostics")?.textContent,
      download: document.querySelector("#download-html")?.outerHTML,
      workerMessages: window.demoWorkerMessages })) }, null, 2));
  throw error;
} finally { await context.close(); await browser.close(); }
