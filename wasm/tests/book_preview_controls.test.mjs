// Actual preview controller/codec/frame builder; explicit DOM, editor and worker
// doubles. Native-browser HTML parsing/CSP and generated WASM are separate gates.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createBookPreviewControls } from "../demo/book_preview_controls.mjs";
const tick = () => new Promise(resolve => setImmediate(resolve));
const gate = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
class Element extends EventTarget {
  value = ""; checked = false; disabled = false; children = []; contentWindow = {}; attributes = new Map();
  setAttribute(name, value) { this.attributes.set(name, value); }
  removeAttribute(name) { this.attributes.delete(name); }
  replaceChildren(...children) { this.children = children; }
  click() { this.dispatchEvent(new Event("click")); }
}
const required = [...readFileSync(new URL("../demo/book.html", import.meta.url), "utf8").matchAll(/id="([^"]+)"/g)].map(match => match[1]);
const pages = [{ path: "one.html", source: "guide/one.md", title: "One", html: '<h1 id="one">One</h1>' }, { path: "two.html", source: "two.md", title: "Two", html: '<h1 id="two">Two</h1>' }];
const output = (value = pages) => ({ format: "book-preview", bytes: new TextEncoder().encode(JSON.stringify({ schema: "fmd-book-preview-v1", pages: value })) });
function setup(t) {
  const el = Object.fromEntries(required.map(id => [id, new Element()]));
  const root = { querySelector: query => el[query.slice(1)], createElement: () => new Element() }, host = new EventTarget();
  const jobs = [], timers = new Map(), subscriptions = new Set(); let serial = 0, rev = 0, cancels = 0, invalid = false, disposed = false;
  const files = [{ path: "guide/one.md", source: "# One" }, { path: "two.md", source: "# Two" }], images = [{ destination: "figure.svg", bytes: new Uint8Array([1]) }];
  const collection = { files, subscribe(fn) { subscriptions.add(fn); return () => subscriptions.delete(fn); }, snapshot() { return { files: structuredClone(files), options: { images: structuredClone(images) } }; } };
  const controls = { captureProject() { if (invalid) throw new Error("Invalid current setting"); }, checkpoint: () => `${rev}:${el["chapter-source"].value}:${el["font-scale"].value}` };
  const worker = { cancel() { cancels++; }, dispose() { disposed = true; }, render(f, format, options) { const pending = gate(); jobs.push({ ...pending, files: f, format, options }); return pending.promise; } };
  let nonce = 0;
  const view = createBookPreviewControls({ root, controls, collection, worker, window: host,
    crypto: { getRandomValues(bytes) { bytes.fill(++nonce); return bytes; } },
    timers: { setTimeout(fn, ms) { timers.set(++serial, { fn, ms }); return serial; }, clearTimeout(id) { timers.delete(id); } } });
  t.after(() => view.dispose());
  const fire = async ms => { for (const [id, job] of [...timers]) if (job.ms === ms) { timers.delete(id); job.fn(); } await tick(); };
  const message = (value, { origin = "null", source = el["preview-frame"].contentWindow, channel } = {}) => {
    channel ??= el["preview-frame"].srcdoc.match(/nonce="([a-f0-9]+)"/)?.[1];
    host.dispatchEvent(Object.assign(new Event("message"), { source, origin, data: { schemaVersion: 1, channel, ...value } }));
  };
  return { view, el, jobs, files, images, message, fire, timers, get cancels() { return cancels; }, get disposed() { return disposed; },
    invalid(value) { invalid = value; }, changed() { rev++; for (const fn of subscriptions) fn(); } };
}
async function built(h) { const pending = h.view.build(); h.jobs.at(-1).resolve(output()); await pending; h.message({ kind: "ready" }); }
test("explicit build routes all chapters/assets through preview format and uses an opaque sandbox", async t => {
  const h = setup(t); assert.equal(h.jobs.length, 0); await built(h);
  assert.equal(h.jobs[0].format, "preview"); assert.deepEqual(h.jobs[0].files, h.files); assert.deepEqual(h.jobs[0].options.images, h.images);
  assert.equal(h.el["preview-frame"].attributes.get("sandbox"), "allow-scripts");
  assert.equal(h.el["preview-pages"].children.length, 2); assert.match(h.el["preview-status"].textContent, /not PDF pagination/);
});
test("chapter selection, next/previous and generated cross-links preserve the editor", async t => {
  const h = setup(t); await built(h); const original = structuredClone(h.files);
  h.el["preview-next"].click(); assert.equal(h.el["preview-pages"].value, "1");
  h.el["preview-previous"].click(); assert.equal(h.el["preview-pages"].value, "0");
  h.message({ kind: "link", href: "two.html#two" }); assert.equal(h.el["preview-pages"].value, "1");
  assert.deepEqual(h.files, original); assert.equal(h.jobs.length, 1);
  h.message({ kind: "missing-fragment" }); assert.match(h.el["preview-status"].textContent, /fragment was not found/);
});
test("messages from other origins, windows, old channels and external links cannot navigate", async t => {
  const h = setup(t); await built(h); const before = h.el["preview-frame"].srcdoc;
  for (const options of [{ origin: "https://example.org" }, { source: {} }, { channel: "f".repeat(32) }]) h.message({ kind: "link", href: "two.html" }, options);
  h.message({ kind: "link", href: "https://example.org" }); assert.equal(h.el["preview-frame"].srcdoc, before); assert.match(h.el["preview-status"].textContent, /No navigation/);
});
test("edits clear existing content immediately and stale worker completions cannot replace newer previews", async t => {
  const h = setup(t); await built(h);
  const old = h.view.build(), check = assert.rejects(old, { code: "STALE_SOURCE" }); h.changed();
  assert(!h.el["preview-frame"].srcdoc.includes("script nonce"));
  const newer = h.view.build(); h.jobs[2].resolve(output()); await newer; const current = h.el["preview-frame"].srcdoc;
  h.jobs[1].resolve(output()); await check; assert.equal(h.el["preview-frame"].srcdoc, current); assert(h.cancels > 0);
});
test("unreported raw DOM edits fence navigation and worker publication", async t => {
  const h = setup(t); await built(h); h.el["font-scale"].value = "invalid";
  h.message({ kind: "link", href: "two.html" }); assert.match(h.el["preview-status"].textContent, /STALE_SOURCE/); assert(h.el["preview-pages"].disabled);
  const work = h.view.build(); h.el["chapter-source"].value = "unreported"; h.jobs.at(-1).resolve(output());
  await assert.rejects(work, { code: "STALE_SOURCE" }); assert.equal(h.el["chapter-source"].value, "unreported");
});
test("automatic preview coalesces edits and pauses through composition", async t => {
  const h = setup(t); h.el["preview-auto"].checked = true; h.changed(); h.changed();
  assert.equal([...h.timers.values()].filter(job => job.ms === 600).length, 1);
  h.el["chapter-source"].dispatchEvent(new Event("compositionstart")); await h.fire(600); assert.equal(h.jobs.length, 0);
  h.el["chapter-source"].dispatchEvent(new Event("compositionend")); await h.fire(600); assert.equal(h.jobs.length, 1);
  h.jobs[0].resolve(output()); await tick(); assert(h.el["preview-frame"].srcdoc.includes("script nonce"));
});
test("invalid current settings never render old model values", async t => {
  const h = setup(t); h.invalid(true); await assert.rejects(h.view.build(), /Invalid current setting/);
  assert.equal(h.jobs.length, 0); assert.match(h.el["preview-status"].textContent, /Invalid current setting/);
});
test("wrong format and chapter maps fail without displaying any partial result", async t => {
  for (const result of [{ ...output(), format: "book-site" }, output([...pages].reverse()), output([{ ...pages[0], source: "different.md" }, pages[1]])]) {
    const h = setup(t), pending = h.view.build(); h.jobs[0].resolve(result);
    await assert.rejects(pending, { code: "INVALID_BOOK_PREVIEW" }); assert(h.el["preview-pages"].disabled);
  }
});
test("cancel and suspension clear previews, cancel workers and switch automatic preview off", async t => {
  const h = setup(t); await built(h); h.el["preview-auto"].checked = true;
  h.el["preview-cancel"].click(); assert(!h.el["preview-auto"].checked); assert(!h.el["preview-frame"].srcdoc.includes("script nonce"));
  h.view.suspend(); await assert.rejects(h.view.build(), { code: "PREVIEW_CLOSED" }); h.view.resume(); await built(h);
  assert.equal(h.jobs.length, 2);
});
test("reader readiness requires a verified message, not an iframe load event", async t => {
  const h = setup(t), pending = h.view.build(); h.jobs[0].resolve(output()); await pending;
  h.el["preview-frame"].dispatchEvent(new Event("load")); await h.fire(10000);
  assert.match(h.el["preview-status"].textContent, /PREVIEW_FRAME_FAILED/); assert(h.el["preview-pages"].disabled);
});
test("dispose fences late replies and removes subscriptions and event handlers", async t => {
  const h = setup(t), pending = h.view.build(); h.view.dispose(); h.jobs[0].resolve(output());
  await assert.rejects(pending, { code: "STALE_SOURCE" }); assert(h.disposed); assert.equal(h.timers.size, 0);
  h.el["preview-build"].click(); assert.equal(h.jobs.length, 1);
});
