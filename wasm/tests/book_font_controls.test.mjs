import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { createBookFontPanel, createBookFontControls } from "../demo/book_font_controls.mjs";
import { BOOK_FONT_SLOTS } from "../demo/book_font_assets.mjs";
import { host, worker, deferred, result, tick } from "./book_font_test_support.mjs";

// Only DOM/source host/native transport are adapters. Both authoring modules
// and the production font store run unchanged; no native font parser is faked.
class Element extends EventTarget {
  constructor(root, tag) { super(); this.root = root; this.tagName = tag; this.children = []; this.attrs = {}; this.style = {}; this._value = ""; this.files = []; }
  set id(value) { this._id = value; this.root.nodes.set(value, this); } get id() { return this._id; }
  set value(value) { this._value = String(value); if (this.type === "file" && value === "") this.files = []; } get value() { return this._value; }
  set innerHTML(_) { assert.fail("font controls must not insert HTML"); }
  appendChild(node) { this.children.push(node); node.parentElement = this; return node; }
  insertBefore(node, next) { this.children.splice(this.children.indexOf(next), 0, node); node.parentElement = this; return node; }
  setAttribute(key, value) { this.attrs[key] = String(value); } getAttribute(key) { return this.attrs[key] ?? null; }
  emit(type) { this.dispatchEvent(new Event(type)); }
  click() { if (!this.disabled) this.emit("click"); }
}
function dom() {
  const root = { nodes: new Map(), createElement(tag) { return new Element(this, tag); }, querySelector(selector) { return this.nodes.get(selector.slice(1)) ?? null; } };
  const body = root.createElement("body"), section = root.createElement("section"), heading = root.createElement("h2");
  heading.id = "publish-title"; section.appendChild(heading); body.appendChild(section);
  for (const id of ["chapter-source", "title", "source-role"]) { const input = root.createElement("input"); input.id = id; body.appendChild(input); }
  return root;
}
const font = (name = "font.ttf") => new File([new Uint8Array([0,1,2,255])], name);
function fixture({ confirm = () => true, delay = false } = {}) {
  const book = host(), engine = worker({ delay }), root = dom(); createBookFontPanel(root);
  const ui = createBookFontControls({ root, ...book, worker: engine, confirm });
  const get = id => root.querySelector(`#${id}`);
  const choose = (slot = "body-regular", name, weight = "") => {
    const file = get(`book-font-file-${slot}`); file.files = [font(name)]; file.emit("change");
    get(`book-font-weight-${slot}`).value = weight; get(`book-font-weight-${slot}`).emit("input");
  };
  return { ...book, root, engine, ui, get, choose };
}
async function until(check) { for (let i = 0; i < 100 && !check(); i++) await tick(); assert(check()); }

test("panel is idempotent, labels every role and keeps resource ownership explicit", () => {
  const f = fixture(), panel = f.get("book-font-panel"); createBookFontPanel(f.root);
  assert.equal(f.get("book-font-panel"), panel);
  assert.equal(panel.getAttribute("aria-labelledby"), "book-font-title");
  assert.equal(f.get("book-font-status").getAttribute("role"), "status");
  assert.match(f.get("book-font-save-help").textContent, /not source-only projects/);
  for (const slot of BOOK_FONT_SLOTS) {
    assert.equal(f.get(`book-font-file-${slot}`).accept, ".ttf,font/ttf");
    assert.equal(f.get(`book-font-weight-${slot}`).getAttribute("aria-describedby"), "book-font-weight-help");
    assert.equal(f.get(`book-font-remove-${slot}`).disabled, true);
  }
  assert.equal(f.get("book-font-assign").disabled, true); f.ui.dispose();
});

test("real assign button batches roles, pins weights and renders names only as text", async () => {
  const f = fixture(); f.choose("body-regular", '<hostile&name>.ttf', "450"); f.choose("body-bold", "bold.ttf", "700");
  f.get("book-font-assign").click(); await until(() => f.collection.fonts.length === 2);
  assert.equal(f.engine.jobs.length, 1); assert.equal(f.engine.jobs[0].options.fontAssets[0].weight, 450);
  assert.match(f.get("book-font-current-body-regular").textContent, /<hostile&name>\.ttf/);
  assert.match(f.get("book-font-status").textContent, /portable backup/);
  assert.equal(f.get("book-font-file-body-regular").files.length, 0);
  assert.equal(f.get("book-font-weight-body-regular").value, "450");
  assert.equal(f.get("book-font-remove-body-bold").disabled, false); f.ui.dispose();
});

test("invalid pins reject before preflight and keep the selected local file for correction", async () => {
  const f = fixture();
  for (const pin of ["0", "1001", "1.5", "1e2", "-1", "NaN", "Infinity"]) {
    f.choose("body-regular", "font.ttf", pin);
    await assert.rejects(f.ui.assign(), { code: "INVALID_FONT_WEIGHT" });
    assert.equal(f.get("book-font-file-body-regular").files.length, 1);
  }
  assert.equal(f.engine.jobs.length, 0); assert.equal(f.collection.fonts.length, 0);
  f.get("book-font-weight-body-regular").value = ""; await f.ui.assign();
  assert.equal(f.collection.fonts[0].weight, undefined); f.ui.dispose();
});

test("silent file and weight changes before native completion cannot install stale drafts", async () => {
  for (const change of [f => f.get("book-font-weight-body-regular").value = "700",
    f => f.get("book-font-file-body-regular").files = [font("different.ttf")]]) {
    const f = fixture({ delay: true }); f.choose(); const pending = f.ui.assign();
    await until(() => f.engine.jobs.length === 1); change(f); f.engine.jobs[0].resolve(result());
    await assert.rejects(pending, { code: "STALE_SOURCE" }); assert.equal(f.collection.fonts.length, 0); f.ui.dispose();
  }
});

test("changing a selected file with an event cancels the owned worker without blocking retry", async () => {
  const f = fixture({ delay: true }); f.choose(); const pending = f.ui.assign();
  await until(() => f.engine.jobs.length === 1); f.choose("body-regular", "retry.ttf");
  await assert.rejects(pending, { code: "FONT_CANCELLED" }); assert(f.engine.jobs[0].signal.aborted);
  const retry = f.ui.assign(); await until(() => f.engine.jobs.length === 2);
  f.engine.jobs[0].resolve(result()); await tick(); assert.equal(f.collection.fonts.length, 0);
  f.engine.jobs[1].resolve(result()); await retry;
  assert.equal(f.collection.fonts[0].name, "retry.ttf"); f.ui.dispose();
});

test("cancel and discard do not revoke an installed font or change source", async () => {
  const f = fixture(); f.choose(); await f.ui.assign();
  const before = f.store.snapshot(), source = f.collection.files[0].source;
  f.choose("body-bold"); f.get("book-font-discard").click();
  assert.equal(f.get("book-font-file-body-bold").files.length, 0);
  assert.deepEqual(f.store.snapshot(), before); assert.equal(f.collection.files[0].source, source);
  assert.equal(f.get("book-font-assign").disabled, true); f.ui.dispose();
});

test("remove and revoke-all buttons ask explicitly and update the inventory", async () => {
  let approved = false, asks = 0;
  const f = fixture({ confirm: () => { asks++; return approved; } });
  f.choose(); f.choose("mono-regular"); await f.ui.assign();
  f.get("book-font-remove-body-regular").click(); await until(() => asks === 1); await tick();
  assert.equal(f.collection.fonts.length, 2);
  approved = true; f.get("book-font-remove-body-regular").click(); await until(() => f.collection.fonts.length === 1);
  assert.deepEqual(f.collection.fonts.map(x => x.slot), ["mono-regular"]);
  f.get("book-font-clear").click(); await until(() => !f.collection.fonts.length);
  assert.equal(asks, 3); assert.match(f.get("book-font-inventory").textContent, /0 supplied roles/); f.ui.dispose();
});

test("asynchronous confirmation is cancelled when the editor changes", async () => {
  const question = deferred(); let asked = false;
  const f = fixture({ confirm: () => { asked = true; return question.promise; } });
  f.choose(); await f.ui.assign(); const pending = f.ui.remove("body-regular");
  await until(() => asked); f.get("chapter-source").emit("input");
  await assert.rejects(pending, { code: "FONT_CANCELLED" }); question.resolve(true); await tick();
  assert.equal(f.collection.fonts.length, 1); f.ui.dispose();
});

test("composition/import state prevents changes and retires work already validating", async () => {
  const f = fixture({ delay: true }); f.choose(); f.busy(true);
  assert.equal(f.get("book-font-assign").disabled, true);
  await assert.rejects(f.ui.assign(), { code: "BOOK_BUSY" }); f.busy(false);
  const pending = f.ui.assign(); await until(() => f.engine.jobs.length === 1); f.busy(true);
  await assert.rejects(pending, { code: "FONT_CANCELLED" }); assert(f.engine.jobs[0].signal.aborted);
  f.engine.jobs[0].resolve(result()); f.ui.dispose();
});

test("native failure leaves old fonts and selected replacement available for explicit retry", async () => {
  const f = fixture({ delay: true });
  f.collection.setFonts([{ slot: "body-regular", name: "old.ttf", bytes: new Uint8Array([1]) }], 0);
  f.choose(); const pending = f.ui.assign(); await until(() => f.engine.jobs.length === 1);
  f.engine.jobs[0].reject(Object.assign(Error("<invalid font>"), { code: "INVALID_FONT" }));
  await assert.rejects(pending); assert.equal(f.collection.fonts[0].name, "old.ttf");
  assert.match(f.get("book-font-status").textContent, /<invalid font>/);
  assert.equal(f.get("book-font-file-body-regular").files.length, 1); f.ui.dispose();
});

test("suspension drops file grants and cancels work; disposal detaches listeners and worker", async () => {
  const f = fixture({ delay: true }); f.choose(); const pending = f.ui.assign();
  await until(() => f.engine.jobs.length === 1); f.ui.suspend();
  await assert.rejects(pending, { code: "FONT_CANCELLED" });
  assert.equal(f.get("book-font-file-body-regular").files.length, 0);
  assert.equal(f.get("book-font-file-body-regular").disabled, true);
  f.ui.resume(); assert.equal(f.get("book-font-file-body-regular").disabled, false);
  f.ui.dispose(); f.ui.dispose(); assert.equal(f.engine.disposals, 1); assert.deepEqual(f.subscribers(), [0,0]);
  f.get("book-font-assign").emit("click"); f.engine.jobs[0].resolve(result()); await tick();
  assert.equal(f.engine.jobs.length, 1); assert.equal(f.collection.fonts.length, 0);
});

test("portable-restored metadata and resource revocation refresh without retaining stale drafts", () => {
  const f = fixture(); f.choose();
  f.collection.setFonts([{ slot: "body-bold", name: "restored.ttf", weight: 600, bytes: new Uint8Array([7,8]) }], 0);
  assert.equal(f.get("book-font-file-body-regular").files.length, 0);
  assert.match(f.get("book-font-current-body-bold").textContent, /restored\.ttf.*600/);
  f.collection.revokeFonts(); assert.match(f.get("book-font-current-body-bold").textContent, /No supplied font/); f.ui.dispose();
});
