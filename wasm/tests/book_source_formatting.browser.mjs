// Execute the unmodified book app and module graph in an installed Chromium.
// Route only repository WASM-shell files; do not build/load a renderer, publish,
// write source files, exercise OS pickers, or represent synthetic IME as real IME.
// FMD_PLAYWRIGHT_MODULE may name an already-installed Playwright ESM entrypoint.
import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { resolve, sep, extname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const wasmRoot = fileURLToPath(new URL("../", import.meta.url));
const output = process.env.FMD_BROWSER_OUTPUT ?? "/tmp/fmd-book-source-formatting";
await mkdir(output, { recursive: true });
const browser = await chromium.launch({ headless: true,
  ...(process.env.FMD_CHROMIUM ? { executablePath: process.env.FMD_CHROMIUM } : {}) });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 },
  userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)" });
const checks = [], errors = [], denied = [];
try {
  await context.route("**/*", async route => {
    const url = new URL(route.request().url());
    const path = resolve(wasmRoot, `.${decodeURIComponent(url.pathname.slice("/wasm".length))}`);
    if (url.origin !== "https://fmd-tests.invalid" || !url.pathname.startsWith("/wasm/") || !path.startsWith(wasmRoot.replace(/\/$/, "") + sep)) {
      denied.push(url.href); return route.abort();
    }
    try {
      const body = await readFile(path);
      const contentType = ({ ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".css": "text/css" })[extname(path)] ?? "application/octet-stream";
      await route.fulfill({ status: 200, contentType, body });
    } catch (error) { errors.push(`Missing fixture route ${url.pathname}: ${error.code}`); await route.abort(); }
  });
  const page = await context.newPage(); page.setDefaultTimeout(10000);
  page.on("pageerror", error => errors.push(error.message));
  const check = (name, observed, expected) => { assert.deepEqual(observed, expected, name); checks.push(name); };
  await page.goto("https://fmd-tests.invalid/wasm/demo/book.html");
  const editor = page.locator("#chapter-source"), toolbar = page.locator("[data-source-formatting]");
  const button = command => page.locator(`[data-source-command="${command}"]`);
  await toolbar.waitFor();
  check("one toolbar with twelve native buttons", await toolbar.locator("button").count(), 12);
  check("empty book disables every formatting button", await toolbar.locator("button:disabled").count(), 12);
  await page.locator("#add-chapter").click();
  await editor.fill("café😀"); await editor.press("ControlOrMeta+a");
  await button("bold").click(); check("real button formats selected Unicode", await editor.inputValue(), "**café😀**");
  await editor.press("ControlOrMeta+z"); check("existing keyboard undo reverses one formatting edit", await editor.inputValue(), "café😀");
  await editor.press("ControlOrMeta+Shift+z"); check("existing keyboard redo restores formatting", await editor.inputValue(), "**café😀**");
  const examples = [
    ["bold", "text", "**text**"], ["italic", "text", "*text*"], ["strike", "text", "~~text~~"],
    ["inline-code", "a`b", "``a`b``"], ["code-block", "text", "```\ntext\n```"],
    ["heading-1", "text", "# text"], ["heading-2", "text", "## text"], ["heading-3", "text", "### text"],
    ["bullet-list", "one\ntwo", "- one\n- two"], ["ordered-list", "one\ntwo", "1. one\n2. two"],
    ["task-list", "one\ntwo", "- [ ] one\n- [ ] two"], ["blockquote", "one\ntwo", "> one\n> two"],
  ];
  for (const [command, source, expected] of examples) {
    await editor.fill(source); await editor.press("ControlOrMeta+a"); await button(command).click();
    check(`${command} button applies source command`, await editor.inputValue(), expected);
    await page.locator("#book-history-undo").click(); check(`${command} is one book-history transaction`, await editor.inputValue(), source);
  }
  await page.locator("#add-include").click(); await editor.fill("shared"); await editor.press("ControlOrMeta+a");
  await editor.press("ControlOrMeta+b"); check("include-only source supports formatting shortcut", await editor.inputValue(), "**shared**");
  check("formatting preserves include role", await page.locator("#source-role").inputValue(), "include");
  await editor.press("ControlOrMeta+z"); check("include-only formatting uses book undo", await editor.inputValue(), "shared");
  await editor.press("ControlOrMeta+a");
  // Fault injection only: real button/focus/event paths still execute normally.
  await editor.evaluate(node => node.addEventListener("beforeinput", event => event.preventDefault(), { once: true }));
  await button("italic").click(); check("beforeinput veto leaves source intact", await editor.inputValue(), "shared");
  await button("bold").focus();
  await editor.evaluate(node => node.addEventListener("focus", () => {
    const chapters = document.querySelector("#chapters"); chapters.value = "0"; chapters.dispatchEvent(new Event("change"));
  }, { once: true }));
  await button("bold").click(); check("focus-driven chapter switch refuses stale formatting", await editor.inputValue(), "one\ntwo");
  check("stale chapter refusal is explained", (await page.locator("#status").textContent()).includes("STALE_SOURCE"), true);
  await editor.evaluate(node => { node.readOnly = true; });
  await page.waitForFunction(() => document.querySelector('[data-source-command="bold"]').disabled);
  check("readonly state disables toolbar", await toolbar.locator("button:disabled").count(), 12);
  await editor.evaluate(node => { node.readOnly = false; });
  // These exercise lifecycle event handlers, not actual bfcache eviction/restore.
  await page.evaluate(() => { window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })); });
  check("suspension removes toolbar", await toolbar.count(), 0);
  await page.evaluate(() => { window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true })); });
  check("persisted restoration installs exactly twelve buttons", await toolbar.locator("button").count(), 12);
  // Synthetic composition is fault injection, not a claim of real IME testing.
  for (const field of ["#chapter-source", "#chapter-path"]) {
    await page.locator(field).dispatchEvent("compositionstart");
    check(`${field} composition blocks formatting`, await toolbar.locator("button:disabled").count(), 12);
    await page.evaluate(() => {
      window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
      window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
    });
    await page.waitForFunction(() => !document.querySelector('[data-source-command="bold"]').disabled);
    await editor.fill("recovered source"); await editor.press("ControlOrMeta+a"); await button("bold").click();
    check(`${field} interrupted composition permits fresh formatting`, await editor.inputValue(), "**recovered source**");
    await page.locator("#book-history-undo").click();
    check(`${field} interrupted composition permits fresh undo`, await editor.inputValue(), "recovered source");
  }
  // Delay only two named test files at the real File.arrayBuffer boundary.
  // The app, read/decode logic, lifecycle handlers and source host stay intact.
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer;
    window.heldSourceReads = new Map(); window.finishedSourceReads = new Set();
    File.prototype.arrayBuffer = function () {
      if (!["retired-lifecycle.svg", "fresh-lifecycle.md"].includes(this.name)) return original.call(this);
      const file = this;
      return new Promise(resolve => window.heldSourceReads.set(file.name, async () => {
        const bytes = await original.call(file); resolve(bytes); window.finishedSourceReads.add(file.name);
      }));
    };
  });
  await page.locator("#import-files").setInputFiles({ name: "retired-lifecycle.svg", mimeType: "image/svg+xml", buffer: Buffer.from("<svg/>") });
  await page.waitForFunction(() => window.heldSourceReads.has("retired-lifecycle.svg"));
  check("pending read disables ordinary import controls", await page.locator("#import-files").isDisabled(), true);
  await page.evaluate(() => {
    window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
    window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
  });
  check("suspension clears old file selection", await page.locator("#import-files").inputValue(), "");
  check("returning permits a fresh import before the old read completes", await page.locator("#import-files").isDisabled(), false);
  await page.locator("#import-files").setInputFiles({ name: "fresh-lifecycle.md", mimeType: "text/markdown", buffer: Buffer.from("# Fresh imported source") });
  await page.waitForFunction(() => window.heldSourceReads.has("fresh-lifecycle.md"));
  const pendingStatus = await page.locator("#status").textContent();
  await page.evaluate(async () => {
    await window.heldSourceReads.get("retired-lifecycle.svg")();
    // Drain the read/decode/finalizer microtasks; no arbitrary wall-clock wait.
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  check("retired finalizer does not unlock the fresh import", await page.locator("#import-files").isDisabled(), true);
  check("retired callback preserves the fresh file selection", await page.locator("#import-files").evaluate(node => node.files[0]?.name), "fresh-lifecycle.md");
  check("retired callback preserves current status", await page.locator("#status").textContent(), pendingStatus);
  check("retired image has no resource grant", await page.locator("#image-list").textContent(), "No images authorized.");
  await page.evaluate(() => window.heldSourceReads.get("fresh-lifecycle.md")());
  await page.waitForFunction(() => [...document.querySelector("#chapters").options].some(option => option.textContent.includes("fresh-lifecycle.md")));
  check("fresh import finishes normally", await page.locator("#import-files").isDisabled(), false);
  check("fresh import clears its own file selection", await page.locator("#import-files").inputValue(), "");
  await page.locator("#chapters").selectOption({ label: "2. fresh-lifecycle.md" });
  check("fresh imported bytes reach the real source editor", await editor.inputValue(), "# Fresh imported source");
  // Execute the existing complete native storage suite, using its unique test
  // databases in this isolated browser profile. No user database is opened.
  const storage = await page.evaluate(async () => (await import("/wasm/tests/book_library_store.browser.mjs")).runBookLibraryStoreTests());
  for (const result of storage.results) check(`native IndexedDB: ${result.name}`, result.passed ? true : result.error, true);
  // Full application save/open with native storage. Delay only delivery of the
  // next readonly transaction acknowledgment after its real requests complete.
  const dialogs = []; page.on("dialog", dialog => {
    dialogs.push(dialog.message());
    void dialog.accept().catch(error => errors.push(`Dialog acknowledgment failed: ${error.message}`));
  });
  await page.locator("#library-name").fill("Composition recovery"); await page.locator("#library-save").click();
  await page.waitForFunction(() => document.querySelector("#library-status").textContent.includes("Saved locally"));
  check("real library save creates an acknowledged named revision", await page.locator("#library-books option").count(), 1);
  await editor.fill("unsaved current source");
  await page.locator("#import-files").setInputFiles({ name: "keep-composition.svg", mimeType: "image/svg+xml", buffer: Buffer.from("<svg/>") });
  await page.waitForFunction(() => document.querySelector("#image-list").textContent.includes("keep-composition.svg"));
  await page.evaluate(() => {
    const original = IDBDatabase.prototype.transaction;
    const complete = Object.getOwnPropertyDescriptor(IDBTransaction.prototype, "oncomplete");
    window.holdNextSavedRead = true; window.savedReadReady = false;
    IDBDatabase.prototype.transaction = function (...args) {
      const tx = original.apply(this, args);
      if (window.holdNextSavedRead && tx.mode === "readonly" && tx.objectStoreNames.contains("snapshots")) {
        window.holdNextSavedRead = false;
        Object.defineProperty(tx, "oncomplete", {
          set(handler) { complete.set.call(tx, event => {
            window.releaseSavedRead = () => handler.call(tx, event); window.savedReadReady = true;
          }); },
          get() { return complete.get.call(tx); },
        });
      }
      return tx;
    };
  });
  const dialogsBeforeRecovery = dialogs.length;
  await page.locator("#library-open").click(); await page.waitForFunction(() => window.savedReadReady);
  await editor.dispatchEvent("compositionstart"); await page.evaluate(() => window.releaseSavedRead());
  await page.waitForFunction(() => document.querySelector("#library-status").textContent.includes("BOOK_BUSY"));
  check("composition begun during native storage read preserves current source", await editor.inputValue(), "unsaved current source");
  check("blocked saved recovery preserves current image access", (await page.locator("#image-list").textContent()).includes("keep-composition.svg"), true);
  check("blocked saved recovery does not ask for replacement approval", dialogs.length, dialogsBeforeRecovery);
  await editor.dispatchEvent("compositionend"); await page.locator("#library-open").click();
  await page.waitForFunction(() => document.querySelector("#library-status").textContent.includes("Opened saved revision"));
  await page.locator("#chapters").selectOption({ label: "2. fresh-lifecycle.md" });
  check("fresh recovery succeeds after composition ends", await editor.inputValue(), "# Fresh imported source");
  check("approved saved recovery revokes old image access", await page.locator("#image-list").textContent(), "No images authorized.");
  // A pending confirmation can be asynchronous in an embedding host. Replace
  // that callback only; the real library/source controllers still own mutation.
  await editor.fill("unfinished new-book source");
  await page.evaluate(() => {
    window.originalConfirm = window.confirm; window.pendingBookConfirm = null;
    window.confirm = () => new Promise(resolve => { window.pendingBookConfirm = () => resolve(true); });
  });
  await page.locator("#library-new").click(); await page.waitForFunction(() => typeof window.pendingBookConfirm === "function");
  await page.locator("#chapter-path").dispatchEvent("compositionstart"); await page.evaluate(() => window.pendingBookConfirm());
  await page.waitForFunction(() => document.querySelector("#library-status").textContent.includes("BOOK_BUSY"));
  check("composition begun during new-book confirmation preserves the book", await page.locator("#chapters option").count(), 3);
  check("blocked new-book confirmation preserves current source", await editor.inputValue(), "unfinished new-book source");
  await page.locator("#chapter-path").dispatchEvent("compositionend");
  await page.evaluate(() => { window.confirm = window.originalConfirm; }); await page.locator("#library-new").click();
  await page.waitForFunction(() => document.querySelector("#chapters").options.length === 0);
  check("fresh new-book approval succeeds after composition ends", await editor.inputValue(), "");
  check("new book detaches the previous save target", (await page.locator("#library-current").textContent()).includes("not attached"), true);
  await page.locator("#add-chapter").click(); await editor.fill("ready for a fresh edit");
  await editor.press("ControlOrMeta+a"); await button("bold").click();
  check("editing remains usable after approved replacement", await editor.inputValue(), "**ready for a fresh edit**");
  const projectBytes = Buffer.from(JSON.stringify({ schemaVersion: 1,
    files: [{ path: "replacement.md", source: "# Approved project" }], options: {} }));
  await page.evaluate(() => {
    const original = File.prototype.arrayBuffer; window.projectReadReady = false;
    File.prototype.arrayBuffer = function () {
      if (this.name !== "composition-project.json") return original.call(this);
      const file = this;
      return new Promise(resolve => {
        window.releaseProjectRead = async () => resolve(await original.call(file)); window.projectReadReady = true;
      });
    };
  });
  await page.locator("#open-project").setInputFiles({ name: "composition-project.json", mimeType: "application/json", buffer: projectBytes });
  await page.waitForFunction(() => window.projectReadReady); await editor.dispatchEvent("compositionstart");
  await page.evaluate(() => window.releaseProjectRead());
  await page.waitForFunction(() => document.querySelector("#status").textContent.includes("BOOK_BUSY"));
  check("composition begun during source-project decoding preserves the editor", await editor.inputValue(), "**ready for a fresh edit**");
  check("blocked source-project import preserves source paths", (await page.locator("#chapters option").allTextContents()).some(text => text.includes("replacement.md")), false);
  await editor.dispatchEvent("compositionend");
  await page.locator("#open-project").setInputFiles({ name: "approved-project.json", mimeType: "application/json", buffer: projectBytes });
  await page.waitForFunction(() => document.querySelector("#chapter-path").value === "replacement.md");
  check("fresh source-project import succeeds after composition ends", await editor.inputValue(), "# Approved project");
  await editor.press("ControlOrMeta+a"); await button("bold").click();
  check("formatting works after fresh source-project import", await editor.inputValue(), "**# Approved project**");
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 }); await toolbar.scrollIntoViewIfNeeded();
    check(`${width}px viewport has no horizontal page overflow`, await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    check(`${width}px toolbar has 44px button targets`, await toolbar.locator("button").evaluateAll(nodes => nodes.every(node => node.getBoundingClientRect().height >= 44)), true);
    await toolbar.screenshot({ path: resolve(output, `toolbar-${width}.png`) });
  }
  check("no browser runtime errors or missing module requests", errors, []);
  check("no external requests", denied, []);
  console.log(JSON.stringify({ browser: await browser.version(), checks: checks.length, passed: checks,
    scope: "actual book app and native IndexedDB; controlled File/transaction/confirmation delays and synthetic composition/lifecycle; no generated WASM, real IME, OS file I/O or actual bfcache proof", output }, null, 2));
} finally {
  // Teardown may dismiss beforeunload itself; no approval callback owns it.
  for (const page of context.pages()) page.removeAllListeners("dialog");
  await context.close(); await browser.close();
}
