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
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 900 }); await toolbar.scrollIntoViewIfNeeded();
    check(`${width}px viewport has no horizontal page overflow`, await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    check(`${width}px toolbar has 44px button targets`, await toolbar.locator("button").evaluateAll(nodes => nodes.every(node => node.getBoundingClientRect().height >= 44)), true);
    await toolbar.screenshot({ path: resolve(output, `toolbar-${width}.png`) });
  }
  check("no browser runtime errors or missing module requests", errors, []);
  check("no external requests", denied, []);
  console.log(JSON.stringify({ browser: await browser.version(), checks: checks.length, passed: checks,
    scope: "actual book application/source controls; no generated WASM, real IME, OS file I/O or actual bfcache proof", output }, null, 2));
} finally { await context.close(); await browser.close(); }
