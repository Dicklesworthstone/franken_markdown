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
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: resolve(output, "mobile.png"), fullPage: true });
  check("mobile page has no horizontal overflow", await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  check("browser errors", errors, []); check("unapproved network requests", denied, []);
  console.log(JSON.stringify({ browser: browser.version(), pkg, generated, checks: checks.length, names: checks,
    output, limitation: "Synthetic composition/page events, not real IME or actual bfcache. Generated artifact provenance must be qualified separately." }, null, 2));
} catch (error) {
  console.error(JSON.stringify({ completedChecks: checks, errors, denied,
    state: page && await page.evaluate(() => ({ status: document.querySelector("#status")?.textContent,
      diagnostics: document.querySelector("#diagnostics")?.textContent,
      download: document.querySelector("#download-html")?.outerHTML,
      workerMessages: window.demoWorkerMessages })) }, null, 2));
  throw error;
} finally { await context.close(); await browser.close(); }
