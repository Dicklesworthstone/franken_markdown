// Actual demo and file-session code; DOM, dialogs, disk and rendering are
// explicit adapters. Native Node File/Blob/UTF-8 behavior is exercised.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createDemoController } from "../demo/demo.js";
import { deferred, MemoryFileHandle } from "./flow_file_fixtures.mjs";
import { sourceAccessFixture } from "./flow_access_fixture.mjs";
export { deferred, MemoryFileHandle };
const html = await readFile(new URL("../demo/index.html", import.meta.url), "utf8");
const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
export const tick = () => new Promise(resolve => setImmediate(resolve));
export async function until(check) {
  for (let count = 0; count < 100; count++) { if (check()) return; await tick(); }
  assert.ok(check(), "The controlled asynchronous action did not settle.");
}
export function draftStorage(document = null) {
  const db = { record: { schemaVersion: 1, version: document ? 1 : 0, document }, reads: 0, writes: [], readGate: null,
    async open() { return {
      async read() { db.reads++; const record = structuredClone(db.record); if (db.readGate) await db.readGate.promise; return record; },
      async save(next, version) {
        assert.equal(version, db.record.version);
        db.writes.push(structuredClone(next));
        return structuredClone(db.record = { schemaVersion: 1, version: version + 1, document: structuredClone(next) });
      },
      async clear(version) { assert.equal(version, db.record.version); return db.record = { schemaVersion: 1, version: version + 1, document: null }; },
      dispose() {},
    }; },
  };
  return db;
}
export function fixture(t, { supported = true, openSupported = supported, saveSupported = supported,
  observeAccess = false, db = draftStorage(), renderFailure = null } = {}) {
  const downloads = [], urls = [], revoked = [], renders = [], workspaces = [], prompts = [], pickers = [], timers = new Map();
  class Element extends EventTarget {
    _value = ""; checked = false; disabled = false; readOnly = false; textContent = ""; srcdoc = ""; children = [];
    constructor(tag = "div") { super(); this.tag = tag; }
    get value() { return this._value; }
    set value(value) { this._value = this.tag === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
    focus() { this.dispatchEvent(new Event("focus")); }
    appendChild(child) { this.children.push(child); child.parent = this; return child; }
    replaceChildren(...children) { this.children = children; }
    removeAttribute(name) { delete this[name]; }
    remove() { if (this.parent) this.parent.children = this.parent.children.filter(child => child !== this); }
    click() {
      if (this.disabled) return;
      const accepted = this.dispatchEvent(new Event("click", { cancelable: true }));
      if (accepted && this.tag === "a" && this.href) downloads.push({ url: this.href, filename: this.download });
    }
  }
  const els = Object.fromEntries(ids.map(id => [id, new Element(
    id === "markdown" || id === "custom-css" ? "textarea" : ["download-html", "download-workspace"].includes(id) ? "a" : "div",
  )]));
  els.markdown.value = "# Original"; els.font.value = "sans"; els["dark-mode"].value = "auto";
  els.title.value = "Original title"; els.author.value = "Author"; els["line-numbers"].checked = true;
  els["image-folder"].webkitdirectory = true;
  const access = observeAccess ? sourceAccessFixture([els.markdown]) : null;
  const doc = { querySelector: selector => els[selector.slice(1)], createElement: tag => new Element(tag), body: new Element("body") };
  const win = new EventTarget(); let sequence = 0;
  win.approve = true; win.isSecureContext = supported;
  win.picked = new MemoryFileHandle(); win.saveTarget = new MemoryFileHandle("saved.md", "");
  win.confirm = message => { prompts.push(message); return typeof win.approve === "function" ? win.approve(message) : win.approve; };
  if (openSupported) win.showOpenFilePicker = options => { pickers.push({ kind: "open", options }); return win.openGate ? win.openGate.promise : Promise.resolve([win.picked]); };
  if (saveSupported) win.showSaveFilePicker = options => { pickers.push({ kind: "save", options }); return win.saveGate ? win.saveGate.promise : Promise.resolve(win.saveTarget); };
  win.setTimeout = fn => { const id = ++sequence; timers.set(id, fn); return id; };
  win.clearTimeout = id => timers.delete(id);
  win.URL = { createObjectURL(blob) { const url = `blob:file-${urls.length}`; urls.push({ url, blob }); return url; }, revokeObjectURL(url) { revoked.push(url); } };
  const factory = () => { if (renderFailure) throw renderFailure; return { disposed: false, dispose() { this.disposed = true; }, render(kind, source, options, controls) {
    const pending = deferred(); renders.push({ kind, source, options, controls, ...pending }); return pending.promise;
  } }; };
  const workspaceFactory = () => ({ disposed: false, dispose() { this.disposed = true; }, prepare(source, options, controls) {
    const pending = deferred(); workspaces.push({ source, options, controls, ...pending }); return pending.promise;
  } });
  const controller = createDemoController(doc, win, factory, { workspaceFactory, openStore: () => db.open(), draftDelayMs: 60000 });
  t.after(() => controller.dispose());
  const input = (value, id = "markdown") => { els[id].value = value; els[id].dispatchEvent(new Event("input")); };
  const output = (call, text = call.source) => ({ bytes: new TextEncoder().encode(text), sourceLength: new TextEncoder().encode(call.source).length,
    diagnostics: [], text: () => text, blob: () => new Blob([text]), filename: base => `${base}.${call.kind ?? "html"}` });
  const key = (fields = {}) => {
    const event = new Event("keydown", { cancelable: true });
    for (const [name, value] of Object.entries({ key: "s", ctrlKey: true, ...fields })) Object.defineProperty(event, name, { value });
    win.dispatchEvent(event); return event;
  };
  const resume = () => { const event = new Event("pageshow"); Object.defineProperty(event, "persisted", { value: true }); win.dispatchEvent(event); };
  const ready = () => until(() => !els["refresh-draft"].disabled);
  return { controller, get files() { return controller.fileAccess; }, els, doc, win, access, db, prompts, pickers,
    downloads, urls, revoked, renders, workspaces, input, output, key, resume, ready };
}
