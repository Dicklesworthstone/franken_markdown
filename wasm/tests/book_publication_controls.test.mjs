import assert from "node:assert/strict";
import test from "node:test";
import { createBookCollection, BOOK_PUBLICATION_FIELDS, normalizeBookPublication } from "../demo/book_collection.mjs";
import { createBookPublicationControls, createBookPublicationPanel } from "../demo/book_pdf_controls.mjs";

// Explicit DOM adapter; collection, resource stores, profile admission and UI
// are production code. No renderer is invoked and no output fidelity is claimed.
class Element extends EventTarget {
  constructor(tag) { super(); this.tagName = tag; this.children = []; this.attributes = new Map(); }
  id = ""; textContent = ""; checked = false; disabled = false; style = {}; parentElement = null; _value = "";
  set value(value) {
    const text = String(value);
    this._value = this.tagName === "select" ? (this.children.some(child => child.value === text) ? text : "")
      : this.tagName === "textarea" ? text.replace(/\r\n?/g, "\n") : text;
  }
  get value() { return this._value; }
  appendChild(child) { child.parentElement = this; this.children.push(child); return child; }
  insertBefore(child, sibling) { child.parentElement = this; this.children.splice(this.children.indexOf(sibling), 0, child); }
  replaceChildren(...children) {
    for (const child of this.children) child.parentElement = null;
    this.children = []; for (const child of children) this.appendChild(child);
    if (this.tagName === "select") this._value = children[0]?.value ?? "";
  }
  setAttribute(key, value) { this.attributes.set(key, value); }
  getAttribute(key) { return this.attributes.get(key); }
  querySelector(selector) {
    if (`#${this.id}` === selector) return this;
    for (const child of this.children) { const found = child.querySelector(selector); if (found) return found; }
    return null;
  }
  emit(type) { this.dispatchEvent(new Event(type)); }
}
function rootDocument() {
  const root = new Element("document"); root.createElement = tag => new Element(tag);
  const publish = root.appendChild(new Element("section"));
  publish.appendChild(new Element("h2")).id = "publish-title";
  return root;
}
const profile = book => Object.fromEntries(BOOK_PUBLICATION_FIELDS.filter(key => book.options[key] !== undefined).map(key => [key, book.options[key]]));
function setup(t, initial = {}) {
  const root = rootDocument(), book = createBookCollection(), listeners = new Set();
  book.append({ chapters: [{ path: "one.md", source: "\ufeff# Book\r\n😀" }],
    includeSources: [{ path: "shared.txt", source: "\r\ntext" }],
    images: [{ destination: "images/logo.png", bytes: new Uint8Array([1, 2]) }] });
  book.setFonts([{ slot: "body-regular", name: "fixture.ttf", bytes: new Uint8Array([3]) }], book.revision);
  book.setPublication(initial, book.revision);
  let busy = false;
  const controls = {
    get sourceBusy() { return busy; },
    captureProject() { throw Error("Profile authoring must not capture or edit source text"); },
    subscribeSourceState(fn) { listeners.add(fn); return () => listeners.delete(fn); },
  };
  const ui = createBookPublicationControls({ root, controls, collection: book });
  t.after(() => { ui.dispose(); book.dispose(); });
  const el = id => root.querySelector(`#book-publication-${id}`);
  return { root, book, ui, el, listeners,
    set(id, value, event = "input") {
      if (typeof value === "boolean") el(id).checked = value; else el(id).value = value;
      if (event) el(id).emit(event);
    },
    busy(value) { busy = value; for (const fn of listeners) fn(); },
  };
}

test("panel installs once, has explicit labels, and leaves model/default schema untouched", t => {
  const h = setup(t), revision = h.book.revision;
  const panel = createBookPublicationPanel(h.root);
  assert.equal(panel, h.el("settings")); assert.equal(h.root.children.filter(child => child === panel).length, 1);
  assert.equal(h.el("baseFontSize").type, "text", "invalid numeric input cannot be silently sanitized to default");
  assert.equal(h.el("status").getAttribute("role"), "status");
  assert.equal(h.el("footer-left").maxLength, 4096);
  assert.equal(h.book.project().schemaVersion, 2); assert.equal(h.book.revision, revision);
  assert.match(h.el("current").textContent, /Native defaults/);
  assert.equal(h.el("header-image").children.length, 2);
});

test("complete UI draft applies every existing native profile field in one observable revision", t => {
  const h = setup(t), before = h.book.snapshot(), revision = h.book.revision;
  const values = { baseFontSize: "12.5", headingScale: "1.3", tableFontSize: "9", tocDepth: "4", fitToPages: "42",
    metadataEpochSeconds: "0", codeLineNumbers: true, htmlFontFormat: "woff2", skipFirstPage: true,
    "header-left": "{title}", "header-right": "{author}", "footer-center": "{page} / {pages}",
    "header-rule": true, "footer-rule": true, "header-image": "images/logo.png", "header-position": "right", "header-height": "18" };
  for (const [id, value] of Object.entries(values)) h.set(id, value);
  for (const flag of ["homogeneous", "antiriver", "pareto", "optimal-pagination", "protrusion", "expansion"]) h.set(`type-${flag}`, true);
  assert.equal(h.book.revision, revision); assert.deepEqual(h.book.snapshot(), before);
  const observed = []; h.book.subscribe(() => observed.push(h.book.project()));
  assert.equal(h.ui.apply(), revision + 1); assert.equal(observed.length, 1);
  assert.equal(observed[0].schemaVersion, 4);
  assert.deepEqual(profile(h.book), normalizeBookPublication({
    typography: "homogeneous,antiriver,pareto,optimal-pagination,protrusion,expansion",
    baseFontSize: 12.5, headingScale: 1.3, tableFontSize: 9, tocDepth: 4, fitToPages: 42,
    metadataEpochSeconds: 0, codeLineNumbers: true, htmlFontFormat: "woff2",
    running: { header: { left: "{title}", right: "{author}", rule: true, image: { dest: "images/logo.png", position: "right", heightPt: 18 } },
      footer: { center: "{page} / {pages}", rule: true }, skipFirstPage: true },
  }));
  const after = h.book.snapshot(); assert.deepEqual(after.files, before.files);
  for (const key of ["images", "fontAssets", "includeSources", "title", "page"]) assert.deepEqual(after.options[key], before.options[key]);
  assert.match(h.el("status").textContent, /No source text was changed/);
});

test("draft edits and discard never change the capture or saved source project", t => {
  const h = setup(t, { baseFontSize: 10, running: { header: { left: "Applied" } } });
  const before = h.book.project(), revision = h.book.revision;
  h.set("baseFontSize", "23"); h.set("header-left", "DRAFT");
  assert.deepEqual(h.book.project(), before); assert.equal(h.book.snapshot().options.baseFontSize, 10);
  h.ui.discard(); assert.equal(h.el("baseFontSize").value, "10"); assert.equal(h.el("header-left").value, "Applied");
  assert.equal(h.book.revision, revision);
});

test("invalid numbers, modes, Unicode or running fields preserve applied settings and the draft", t => {
  const h = setup(t, { baseFontSize: 11 }), before = h.book.project();
  const bad = [["baseFontSize", "0x10"], ["baseFontSize", "NaN"], ["baseFontSize", "5"],
    ["baseFontSize", "1e309"], ["tocDepth", "2.5"], ["fitToPages", "0"], ["fitToPages", "4294967296"],
    ["metadataEpochSeconds", "-1"], ["metadataEpochSeconds", "9007199254740992"],
    ["header-left", "😀".repeat(1025)], ["footer-right", "\ud800"]];
  for (const [id, value] of bad) {
    h.ui.discard(); h.set(id, value);
    assert.throws(() => h.ui.apply()); assert.deepEqual(h.book.project(), before);
    assert.equal(h.el(id).value, value); assert.match(h.el("status").textContent, /unchanged/);
  }
});

test("numeric defaults are explicit and zero epoch is distinct from absence", t => {
  const h = setup(t); h.set("metadataEpochSeconds", "0"); h.ui.apply(); assert.equal(h.book.options.metadataEpochSeconds, 0);
  h.set("metadataEpochSeconds", " "); h.ui.apply(); assert.equal(h.book.options.metadataEpochSeconds, undefined);
  assert.equal(h.book.project().schemaVersion, 2);
});

test("native default reset is staged, then removes only the profile on Apply", t => {
  const h = setup(t, { baseFontSize: 14, running: { skipFirstPage: true } });
  h.book.setPage({ size: "a4", margins: 54 }, h.book.revision);
  const before = h.book.snapshot(), revision = h.book.revision;
  h.ui.defaults(); assert.equal(h.book.revision, revision); assert.equal(h.book.options.baseFontSize, 14);
  assert.match(h.el("status").textContent, /staged/);
  h.ui.apply(); assert.deepEqual(profile(h.book), {}); assert.equal(h.book.project().schemaVersion, 3);
  for (const key of ["page", "images", "fontAssets", "title"]) assert.deepEqual(h.book.snapshot().options[key], before.options[key]);
});

test("unchanged controls preserve exact saved CRLF templates and typography spelling", t => {
  const initial = { typography: "pareto, homogeneous,pareto", running: { footer: { left: "A\r\nB\rC" } } };
  const h = setup(t, initial), revision = h.book.revision, expected = profile(h.book);
  assert.equal(h.el("footer-left").value, "A\nB\nC", "matches browser textarea newline normalization");
  assert.equal(h.ui.apply(), revision); assert.deepEqual(profile(h.book), expected);
  h.set("baseFontSize", "12"); h.ui.apply();
  assert.equal(h.book.options.running.footer.left, initial.running.footer.left);
  assert.equal(h.book.options.typography, expected.typography);
});

test("unrelated source and include edits preserve an unapplied profile draft", t => {
  const h = setup(t); h.set("baseFontSize", "12");
  h.book.edit(0, "one.md", "New source"); h.book.edit(1, "shared.txt", "New include");
  assert.equal(h.el("apply").disabled, false); assert.equal(h.el("baseFontSize").value, "12");
  h.ui.apply(); assert.equal(h.book.files[0].source, "New source"); assert.equal(h.book.files[1].source, "New include");
});

test("concurrent settings and resource changes reject a dirty draft instead of overwriting", t => {
  const changes = [h => h.book.setPublication({ baseFontSize: 10 }, h.book.revision),
    h => h.book.configure({ ...h.book.options, title: "Other" }),
    h => h.book.setPage({ size: "a4" }, h.book.revision), h => h.book.revokeImages(),
    h => h.book.replaceProject(h.book.project()), h => h.book.edit(0, "renamed.md", "New")];
  for (const change of changes) {
    const h = setup(t); h.set("baseFontSize", "13", null); // no input event: raw draft still fenced
    change(h); assert(h.el("apply").disabled); assert.equal(h.el("baseFontSize").value, "13");
    assert.throws(() => h.ui.apply(), { code: "STALE_SOURCE" });
    h.ui.discard(); assert.equal(h.el("apply").disabled, false);
  }
});

test("a pristine panel follows recovered profiles and retains unauthorized logo references", t => {
  const h = setup(t); const source = h.book.project();
  source.schemaVersion = 4; source.options.running = { header: { image: { dest: "restored/logo.svg", position: "right", heightPt: 16 } } };
  h.book.replaceProject(source);
  assert.equal(h.el("header-image").value, "restored/logo.svg");
  assert.match(h.el("header-image").children.at(-1).textContent, /not authorized/);
  assert.match(h.el("resources").textContent, /restored\/logo.svg/);
  const revision = h.book.revision; h.ui.apply(); assert.equal(h.book.revision, revision);
  assert.deepEqual(h.book.images, []); assert.deepEqual(h.book.fonts, []);
  h.book.append({ chapters: [], images: [{ destination: "restored/logo.svg", bytes: new Uint8Array([1]) }] });
  assert.equal(h.el("header-image").value, "restored/logo.svg");
  assert(!h.el("resources").textContent.includes("not authorized"));
});

test("logos use admitted paths and clear their geometry only on explicit removal", t => {
  const h = setup(t); h.set("footer-image", "images/logo.png", "change");
  assert.equal(h.el("footer-height").disabled, false);
  h.set("footer-height", "65536"); assert.throws(() => h.ui.apply());
  h.set("footer-height", "18"); h.set("footer-position", "left", "change"); h.ui.apply();
  assert.equal(h.book.options.running.footer.image.heightPt, 18);
  h.set("footer-image", "", "change"); assert(h.el("footer-height").disabled);
  h.ui.apply(); assert.equal(h.book.options.running, undefined);
  assert.equal(h.book.images.length, 1);
});

test("composition leaves the active field editable but blocks all profile transactions", t => {
  const h = setup(t); h.el("header-left").emit("compositionstart");
  assert.equal(h.el("header-left").disabled, false); assert.equal(h.el("apply").disabled, true);
  h.set("header-left", "日本語");
  for (const operation of [h.ui.apply, h.ui.defaults, h.ui.discard]) assert.throws(operation, { code: "BOOK_BUSY" });
  h.el("header-left").emit("compositionend"); h.ui.apply();
  assert.equal(h.book.options.running.header.left, "日本語");
});

test("main-editor composition and imports block authoring until source state settles", t => {
  const h = setup(t); h.set("tocDepth", "2"); h.busy(true);
  assert(h.el("apply").disabled); assert(h.el("header-left").disabled);
  assert.throws(() => h.ui.apply(), { code: "BOOK_BUSY" });
  h.busy(false); h.ui.apply(); assert.equal(h.book.options.tocDepth, 2);
});

test("suspension discards drafts, resume uses current state, disposal removes subscriptions", t => {
  const h = setup(t, { baseFontSize: 10 }); h.set("baseFontSize", "20"); h.ui.suspend();
  assert.throws(() => h.ui.apply(), { code: "PROFILE_CLOSED" }); assert(h.el("apply").disabled);
  h.book.setPublication({ baseFontSize: 12 }, h.book.revision); h.ui.resume();
  assert.equal(h.el("baseFontSize").value, "12");
  h.ui.dispose(); h.ui.dispose(); assert.equal(h.listeners.size, 0);
  const revision = h.book.revision; h.el("defaults").emit("click"); h.el("apply").emit("click");
  assert.equal(h.book.revision, revision); assert.throws(() => h.ui.apply(), { code: "PROFILE_CLOSED" });
});

test("failed click application reports the actual validation error without model mutation", t => {
  const h = setup(t), revision = h.book.revision;
  h.set("tableFontSize", "99"); h.el("apply").emit("click");
  assert.match(h.el("status").textContent, /tableFontSize/); assert.equal(h.book.revision, revision);
  h.set("tableFontSize", "9"); h.el("apply").emit("click"); assert.equal(h.book.options.tableFontSize, 9);
});

test("observer edits during application cannot leave a false applied-profile summary", t => {
  const h = setup(t); h.set("baseFontSize", "12"); let once = false;
  h.book.subscribe(() => { if (!once) { once = true; h.book.configure({ ...h.book.options, baseFontSize: 15 }); } });
  h.ui.apply(); assert.equal(h.el("baseFontSize").value, "15");
  assert.match(h.el("status").textContent, /changed again/);
});


test("source-only edits and ordinary draft input do not rebuild unchanged logo options", t => {
  const h = setup(t), nodes = [...h.el("header-image").children];
  h.book.edit(0, "one.md", "typing source"); h.set("baseFontSize", "12");
  assert.deepEqual(h.el("header-image").children, nodes);
  assert.equal(h.el("header-image").children[1], nodes[1]);
});


test("discard restores silently changed logo selection even when cached choices are unchanged", t => {
  const h = setup(t), before = h.book.project();
  h.set("header-image", "images/logo.png", null);
  h.ui.discard(); assert.equal(h.el("header-image").value, "");
  h.ui.apply(); assert.deepEqual(h.book.project(), before);
});
