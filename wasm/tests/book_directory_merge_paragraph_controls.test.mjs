import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, copyFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

// Import the complete production UI and merger without source rewriting. Only
// the disk session and DOM/source host are explicit adapters. This checks the
// controller's review/approval wiring, not browser or filesystem acceptance.
const directoryBoundary = `
export const BOOK_DIRECTORY_LIMITS = { files: 256, fileBytes: 4194304 };
export class BookDirectoryError extends Error { constructor(code,message){super(message);this.code=code;} }
export const fixture = { review:null, decisions:null, applies:0 };
export function createBookDirectorySession({read,onState}) {
 const state={files:1,folder:'fixture',phase:'ready',message:'test adapter',canRefresh:true,canSave:true,busy:null,dirty:0};
 const reconcile=async(confirm,apply)=>{
  const captured=read();state.busy='review';
  try {
   const decisions=await confirm(fixture.review);fixture.decisions=decisions;
   if(decisions===false)return false;
   const next=captured.files.map(file=>{
    const selected=decisions.find(item=>item.path===file.path);
    if(!selected)return file;
    const reviewed=fixture.review.files.find(item=>item.path===file.path);
    return {...file,source:selected.choice==='edited'?selected.source:reviewed[selected.choice]};
   });
   apply(next,captured.stamp);fixture.applies++;return true;
  } finally {state.busy=null;onState(state);}
 };
 return {state,reconcile,refresh:reconcile,changed(){},detach(){state.folder=null;},dispose(){},
  bind(){throw Error('unexpected picker');},save(){throw Error('unexpected disk writer');}};
}
`;
class Element extends EventTarget {
  constructor(tag) { super(); this.tagName = tag; this.children = []; this.attributes = new Map(); }
  id = ""; parentElement = null; hidden = false; disabled = false; checked = false; _value = ""; _text = "";
  set value(value) { this._value = this.tagName === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
  get value() { return this._value; }
  set textContent(value) { this._text = String(value); this.children = []; }
  get textContent() { return this._text + this.children.map(node => node.textContent).join(""); }
  appendChild(node) { node.parentElement = this; this.children.push(node); return node; }
  setAttribute(key, value) { this.attributes.set(key, String(value)); }
  querySelector(selector) {
    if (`#${this.id}` === selector) return this;
    for (const node of this.children) { const found = node.querySelector(selector); if (found) return found; }
    return null;
  }
  focus() { let root = this; while (root.parentElement) root = root.parentElement; root.activeElement = this; }
  emit(type, target = this) { const event = new Event(type, { cancelable: true }); Object.defineProperty(event, "target", { value: target }); this.dispatchEvent(event); }
  click() { if (!this.disabled) this.emit("click"); }
}
const base = "red fox follows blue bird near tall tree\r\n";
const local = "green fox follows gold bird near tall tree\r\n";
const disk = "red fox follows small bird near short tree\r\n";
async function setup(t, texts = [base, local, disk]) {
  const dir = await mkdtemp(join(tmpdir(), "fmd-paragraph-controls-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  for (const name of ["book_directory_controls.mjs", "book_source_merge.mjs"])
    await copyFile(new URL(`../demo/${name}`, import.meta.url), join(dir, name));
  await writeFile(join(dir, "book_directory_session.mjs"), directoryBoundary);
  const { createBookDirectoryControls } = await import(pathToFileURL(join(dir, "book_directory_controls.mjs")));
  const { fixture } = await import(pathToFileURL(join(dir, "book_directory_session.mjs")));
  const root = new Element("document"); root.createElement = tag => new Element(tag);
  const section = root.appendChild(new Element("section"));
  section.appendChild(new Element("h2")).id = "publish-title";
  for (const [id, value] of [["chapter-path", "a.md"], ["chapter-source", texts[1]], ["source-role", "chapter"]]) {
    const node = root.appendChild(new Element(id === "chapter-source" ? "textarea" : "input")); node.id = id; node.value = value;
  }
  const changes = new Set(), busyListeners = new Set();
  let epoch = 0, revision = 0, source = texts[1], busy = false;
  const collection = {
    get files() { return [{ path: "a.md", source }]; },
    get sourceProjectRevision() { return epoch; },
    subscribe(fn) { changes.add(fn); return () => changes.delete(fn); },
  };
  const checkpoint = () => JSON.stringify([epoch, revision, root.querySelector("#chapter-source").value]);
  const controls = { currentChapter: 0, checkpoint,
    get sourceBusy() { return busy; },
    subscribeSourceState(fn) { busyListeners.add(fn); return () => busyListeners.delete(fn); },
    applySources(files, expected) {
      assert.equal(checkpoint(), expected); source = files[0].source; revision++;
      root.querySelector("#chapter-source").value = source;
      for (const fn of changes) fn();
    },
  };
  fixture.review = { folder: "fixture", changedSources: 0, conflicts: 1,
    files: [{ path: "a.md", action: "conflict", reason: "overlapping-edits", base: texts[0], local: texts[1], disk: texts[2],
      editorBytes: new TextEncoder().encode(texts[1]).length, diskBytes: new TextEncoder().encode(texts[2]).length }] };
  const window = new Element("window"); window.showDirectoryPicker = () => { throw Error("unexpected permission request"); };
  const ui = createBookDirectoryControls({ root, controls, collection, window }); t.after(() => ui.dispose());
  const el = id => root.querySelector(`#book-directory-${id}`);
  const set = (id, value) => { el(id).value = value; el("merge").emit("change", el(id)); };
  return { root, ui, fixture, el, set, get source() { return source; },
    busy() { busy = true; for (const fn of busyListeners) fn(); },
    replace() { epoch++; revision++; source = "new project"; for (const fn of changes) fn(); },
  };
}
test("folder review shows a contested word and approval retains surrounding edits from both sides", async t => {
  const h = await setup(t), pending = h.ui.reconcile();
  assert.equal(h.source, local); assert.equal(h.fixture.applies, 0);
  assert.match(h.el("merge").textContent, /base line 1, UTF-16 offsets \[16, 20\)/);
  assert.match(h.el("merge").textContent, /base:\nblue/);
  h.set("merge-0-choice", "regions"); assert(h.el("approve").disabled);
  h.set("merge-0-hunk-0", "local"); assert.equal(h.el("approve").disabled, false);
  h.el("approve").click(); await pending;
  assert.equal(h.source, "green fox follows gold bird near short tree\r\n");
  assert.equal(h.fixture.applies, 1);
  assert.deepEqual(h.fixture.decisions, [{ path: "a.md", choice: "edited", source: h.source }]);
});
test("whole-file version selection is still explicit and intentionally replaces all local edits", async t => {
  const h = await setup(t), pending = h.ui.reconcile();
  h.set("merge-0-choice", "disk"); h.el("approve").click(); await pending;
  assert.equal(h.source, disk);
});
test("multiple word conflicts block approval until every region has a selected version", async t => {
  const h = await setup(t, ["red fox blue bird tall tree\n", "green fox gold bird high tree\n", "red fox small bird short tree\n"]);
  const pending = h.ui.reconcile(); h.set("merge-0-choice", "regions");
  h.set("merge-0-hunk-0", "local"); assert(h.el("approve").disabled); h.el("approve").click();
  assert.equal(h.fixture.applies, 0);
  h.set("merge-0-hunk-1", "disk"); h.el("approve").click(); await pending;
  assert.equal(h.source, "green fox gold bird short tree\n");
});
test("cancelling a refined review never submits partial source choices", async t => {
  const h = await setup(t), pending = h.ui.reconcile();
  h.set("merge-0-choice", "regions"); h.set("merge-0-hunk-0", "disk");
  h.el("decline").click(); assert.equal(await pending, false);
  assert.equal(h.source, local); assert.equal(h.fixture.applies, 0);
});
test("source input retires a refined review before an old approval can overwrite it", async t => {
  const h = await setup(t), pending = h.ui.reconcile();
  h.set("merge-0-choice", "regions"); h.set("merge-0-hunk-0", "disk");
  const editor = h.root.querySelector("#chapter-source"); editor.value = "new raw editor";
  h.root.emit("input", editor); h.el("approve").click();
  assert.equal(await pending, false); assert.equal(editor.value, "new raw editor"); assert.equal(h.fixture.applies, 0);
});
for (const action of ["busy", "replace", "suspend", "dispose"]) {
  test(`${action} retires pending word choices without applying them`, async t => {
    const h = await setup(t), pending = h.ui.reconcile();
    h.set("merge-0-choice", "regions"); h.set("merge-0-hunk-0", "local");
    if (action === "busy") h.busy(); else if (action === "replace") h.replace(); else h.ui[action]();
    h.el("approve").click(); assert.equal(await pending, false); assert.equal(h.fixture.applies, 0);
    assert.equal(h.source, action === "replace" ? "new project" : local);
  });
}
test("composition inside a review blocks approval until the composition ends", async t => {
  const h = await setup(t), pending = h.ui.reconcile();
  h.set("merge-0-choice", "regions"); h.set("merge-0-hunk-0", "local");
  h.root.emit("compositionstart", h.el("merge-0-source")); assert(h.el("approve").disabled);
  h.el("approve").click(); assert.equal(h.fixture.applies, 0);
  h.root.emit("compositionend", h.el("merge-0-source")); h.el("approve").click(); await pending;
  assert.equal(h.source, "green fox follows gold bird near short tree\r\n");
});
