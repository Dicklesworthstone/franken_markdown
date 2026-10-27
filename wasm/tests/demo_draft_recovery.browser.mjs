// Actual primary-demo page/controller, native IndexedDB, file inputs, textarea
// normalization, reloads, two tabs and source downloads. The rendering boundary
// is explicitly doubled: this probe does NOT qualify worker/WASM rendering.
// Composition and bfcache transitions below are synthetic browser events.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, sep, extname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";

const wasmRoot = fileURLToPath(new URL("../", import.meta.url));
const output = process.env.FMD_BROWSER_OUTPUT ?? "/tmp/fmd-demo-draft-recovery";
const baseline = process.env.FMD_DRAFT_BASELINE_REF;
const origin = "https://fmd-draft-tests.invalid";
const draftName = "franken-markdown-demo-source-draft-v1";
const flowName = "franken-markdown-source-draft-v1";
const baselineFiles = new Map();
if (baseline) {
  for (const file of ["demo/index.html", "demo/demo.js"]) {
    const { stdout } = await promisify(execFile)("git", ["show", `${baseline}:wasm/${file}`],
      { cwd: resolve(wasmRoot, ".."), maxBuffer: 2 * 1024 * 1024 });
    baselineFiles.set(file, stdout);
  }
}
await mkdir(output, { recursive: true });
const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const browser = await chromium.launch({ headless: true,
  ...(process.env.FMD_CHROMIUM ? { executablePath: process.env.FMD_CHROMIUM } : {}) });
const context = await browser.newContext({ acceptDownloads: true, viewport: { width: 1360, height: 1050 },
  userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)" });
const checks = [], errors = [], denied = [];
const check = (name, actual, expected) => { assert.deepEqual(actual, expected, name); checks.push(name); };
const rendererDouble = `
export function createWorkerRenderer() {
  return { disposed: false, dispose() { this.disposed = true; },
    async render(kind, source, options) {
      (window.demoDraftRenderCalls ??= []).push({ kind, source,
        allowRawHtml: options.allowRawHtml, images: options.pdfImages?.length ?? 0 });
      const escaped = source.replaceAll("&", "&amp;").replaceAll("<", "&lt;");
      const text = "<!doctype html><html><body><pre>" + escaped + "</pre></body></html>";
      const bytes = new TextEncoder().encode(text);
      return { bytes, sourceLength: new TextEncoder().encode(source).length, diagnostics: [],
        text: () => text, blob: () => new Blob([bytes], { type: "text/html" }), filename: base => base + "." + kind };
    }
  };
}`;
async function ready(page) {
  await page.waitForFunction(() => document.querySelector("#refresh-draft")?.disabled === false);
}
async function status(page, text) {
  await page.waitForFunction(value => document.querySelector("#draft-status").textContent.includes(value), text);
}
async function readDraft(page, name = draftName) {
  return page.evaluate(async name => {
    const { openDraftStore } = await import("./flow_draft_store.mjs");
    const store = await openDraftStore({ name });
    try { return await store.read(); } finally { store.dispose(); }
  }, name);
}
async function downloadSource(page, filename) {
  const pending = page.waitForEvent("download");
  await page.locator("#download-markdown").click();
  const saved = await pending, path = resolve(output, filename);
  await saved.saveAs(path);
  return { name: saved.suggestedFilename(), bytes: await readFile(path) };
}
async function restore(page) {
  page.once("dialog", dialog => dialog.accept());
  await page.locator("#restore-draft").click();
  await status(page, "acknowledged");
}
try {
  await context.route("**/*", async route => {
    const url = new URL(route.request().url());
    if (url.origin !== origin || !url.pathname.startsWith("/wasm/")) {
      denied.push(url.href); return route.abort();
    }
    const relative = decodeURIComponent(url.pathname.slice("/wasm/".length));
    const path = resolve(wasmRoot, relative);
    if (!path.startsWith(wasmRoot.replace(/\/$/, "") + sep)) {
      denied.push(url.href); return route.abort();
    }
    try {
      const body = relative === "document_worker.mjs" ? rendererDouble
        : baselineFiles.get(relative) ?? await readFile(path);
      const contentType = ({ ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript",
        ".css": "text/css" })[extname(path)] ?? "application/octet-stream";
      await route.fulfill({ status: 200, contentType, body });
    } catch (error) { errors.push(`Missing route ${url.pathname}: ${error.code}`); await route.abort(); }
  });
  const page = await context.newPage(); page.setDefaultTimeout(10000);
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${origin}/wasm/demo/index.html`);
  if (baseline) {
    await page.locator("#markdown").fill("# Source lost by the baseline after reload");
    await page.reload();
    check("baseline reload loses the unsaved edited source", await page.locator("#markdown").inputValue()
      === "# Source lost by the baseline after reload", false);
  }
  // The SAME first feature assertion deliberately fails on the pre-change demo.
  check("primary demo exposes opt-in source recovery", await page.locator("#remember-draft").count(), 1);
  await ready(page);
  check("autosave is initially off", await page.locator("#remember-draft").isChecked(), false);
  check("no source is stored without consent", (await readDraft(page)).document, null);
  await page.evaluate(async () => {
    const { openDraftStore } = await import("./flow_draft_store.mjs");
    const store = await openDraftStore();
    try { await store.save({ source: "Flow's independent source", filename: "flow.md" }, 0); }
    finally { store.dispose(); }
  });
  const source = "\uFEFF# Local café😀\r\n\r\n![chart](chart.svg)\r\n<span id=\"local\">markup</span>\r\n";
  const normalized = source.replace(/\r\n/g, "\n");
  await page.locator("#markdown-file").setInputFiles({ name: "Local Notes.md", mimeType: "text/markdown", buffer: Buffer.from(source) });
  await page.waitForFunction(() => document.querySelector("#file-status").textContent.startsWith("Opened Local Notes.md"));
  check("real textarea normalizes imported CRLF", await page.locator("#markdown").inputValue(), normalized);
  await page.locator("#allow-html").check();
  await page.locator("#author").fill("Author not stored");
  await page.locator(".image-controls").evaluate(node => { node.open = true; });
  await page.locator("#image-files").setInputFiles({ name: "chart.svg", mimeType: "image/svg+xml",
    buffer: Buffer.from('<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>') });
  await page.waitForFunction(() => document.querySelector("#image-status").textContent.startsWith("1 local image authorized"));
  await page.locator("#remember-draft").check(); await status(page, "acknowledged");
  check("real transaction retains exact BOM/CRLF source and filename only", (await readDraft(page)).document,
    { filename: "Local Notes.md", source });
  check("main demo leaves Flow's namespace unchanged", (await readDraft(page, flowName)).document,
    { filename: "flow.md", source: "Flow's independent source" });
  await page.locator("#remember-draft").uncheck();
  await page.locator("#markdown").fill("# Changes after turning autosave off");
  await page.waitForTimeout(550);
  check("disabling autosave keeps the previous stored copy", (await readDraft(page)).document.source, source);
  await page.reload(); await ready(page);
  check("reload offers recovery without replacing the editor", (await page.locator("#markdown").inputValue()) === normalized, false);
  check("reload does not restore autosave consent", await page.locator("#remember-draft").isChecked(), false);
  check("available draft blocks accidental overwrite", await page.locator("#remember-draft").isDisabled(), true);
  await restore(page);
  check("explicit recovery restores the real textarea", await page.locator("#markdown").inputValue(), normalized);
  check("recovery never restores raw HTML trust", await page.locator("#allow-html").isChecked(), false);
  check("recovery never restores prior image access", await page.locator("#image-references").textContent(), "");
  check("recovery does not restore author settings", await page.locator("#author").inputValue(), "FMD");
  await page.locator("#download-html").waitFor({ state: "visible" });
  check("post-recovery renderer receives no image grant", await page.evaluate(() => demoDraftRenderCalls.at(-1).images), 0);
  const saved = await downloadSource(page, "recovered-original.md");
  check("real source download retains original filename", saved.name, "Local Notes.md");
  check("real source download retains BOM, Unicode and original CRLF bytes", saved.bytes, Buffer.from(source));
  await page.screenshot({ path: resolve(output, "recovered-draft.png"), fullPage: true });

  const second = await context.newPage(); second.setDefaultTimeout(10000);
  second.on("pageerror", error => errors.push(error.message));
  await second.goto(`${origin}/wasm/demo/index.html`); await ready(second); await restore(second);
  await page.locator("#remember-draft").check(); await second.locator("#remember-draft").check();
  await page.locator("#markdown").fill("# First tab's newly committed revision"); await status(page, "revision 2");
  await second.locator("#markdown").fill("# Second tab's competing source"); await status(second, "DRAFT_CONFLICT");
  check("native IndexedDB compare-and-write preserves the first tab's revision", (await readDraft(second)).document.source,
    "# First tab's newly committed revision");
  check("conflict leaves the second editor intact", await second.locator("#markdown").inputValue(), "# Second tab's competing source");
  check("conflict turns autosave off", await second.locator("#remember-draft").isChecked(), false);
  check("conflicted source still downloads", (await downloadSource(second, "conflicted-source.md")).bytes,
    Buffer.from("# Second tab's competing source"));
  await second.locator("#refresh-draft").click(); await ready(second);
  second.once("dialog", dialog => dialog.dismiss()); await second.locator("#forget-draft").click(); await ready(second);
  check("declining forget retains the stored revision", (await readDraft(second)).document.source, "# First tab's newly committed revision");
  second.once("dialog", dialog => dialog.accept()); await second.locator("#forget-draft").click(); await ready(second);
  check("confirmed forget clears only the stored source", (await readDraft(second)).document, null);
  check("forget preserves current editor text", await second.locator("#markdown").inputValue(), "# Second tab's competing source");
  check("forget leaves autosave off", await second.locator("#remember-draft").isChecked(), false);

  await second.locator("#remember-draft").check(); await status(second, "revision 4");
  await second.evaluate(() => {
    const editor = document.querySelector("#markdown");
    editor.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    editor.value = "# 正在输入 source before suspension";
    editor.dispatchEvent(new InputEvent("input", { bubbles: true, isComposing: true, inputType: "insertCompositionText" }));
    window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
  });
  await second.waitForTimeout(550);
  check("synthetic suspension does not claim or perform a pending unload save", (await readDraft(second)).document.source,
    "# Second tab's competing source");
  assert.match(await second.locator("#draft-status").textContent(), /pause does not save current changes/);
  checks.push("pause status explicitly distinguishes acknowledgment from in-memory source");
  await second.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true })));
  await ready(second);
  check("synthetic bfcache resume retains in-progress source", await second.locator("#markdown").inputValue(), "# 正在输入 source before suspension");
  check("resumed recovery still requires consent", await second.locator("#remember-draft").isChecked(), false);
  check("resume offers the previously committed copy", await second.locator("#restore-draft").isEnabled(), true);

  // Real malformed persisted data: storage validation must fail closed while
  // retaining the user's new editor source and ordinary download capability.
  await second.evaluate(async name => {
    await new Promise((resolve, reject) => {
      const request = indexedDB.open(name);
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result, tx = db.transaction("draft", "readwrite");
        tx.objectStore("draft").put({ schemaVersion: 1, version: 5,
          document: { filename: "bad.md", source: "Corrupt grant", images: ["unwanted"] } }, "current");
        tx.oncomplete = () => { db.close(); resolve(); };
        tx.onerror = () => { db.close(); reject(tx.error); };
      };
    });
  }, draftName);
  await second.locator("#refresh-draft").click(); await status(second, "CORRUPT_DRAFT");
  check("corrupt recovery cannot replace current source", await second.locator("#markdown").inputValue(), "# 正在输入 source before suspension");
  check("source download remains usable after a genuine storage validation failure", (await downloadSource(second, "after-corrupt-recovery.md")).bytes,
    Buffer.from("# 正在输入 source before suspension"));
  check("Flow draft remains independent after forget, conflicts and corrupted main recovery", (await readDraft(second, flowName)).document,
    { filename: "flow.md", source: "Flow's independent source" });
  check("browser reports no page errors", errors, []);
  check("no request leaves the local fixture origin", denied, []);
  const receipt = { checks, qualification: "real Chromium IndexedDB, File/Blob, textareas, reloads and downloads; explicit renderer double; synthetic IME/page-transition events",
    browser: browser.version(), errors, denied };
  await writeFile(resolve(output, "receipt.json"), `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(JSON.stringify({ passed: checks.length, output, qualification: receipt.qualification }));
} finally { await context.close(); await browser.close(); }
