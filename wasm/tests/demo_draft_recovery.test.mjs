// Production primary-demo controller and shared recovery owner. DOM, rendering
// and transaction completion are explicit doubles; the companion browser probe
// exercises real IndexedDB, textareas, reloads and Markdown downloads.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { createDemoController } from "../demo/demo.js";

const html = await readFile(new URL("../demo/index.html", import.meta.url), "utf8");
const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
const tick = () => new Promise(resolve => setImmediate(resolve));
const gate = () => {
  let resolve;
  const promise = new Promise(yes => { resolve = yes; });
  return { resolve, promise };
};
async function until(check) {
  for (let count = 0; count < 100; count++) {
    if (check()) return;
    await tick();
  }
  assert.ok(check(), "asynchronous controller operation did not settle");
}
const fail = (code, message = code) => Object.assign(new Error(message), { code });
function storage(document = null) {
  const db = {
    record: { schemaVersion: 1, version: document ? 1 : 0, document },
    reads: 0, writes: [], clears: 0, opens: 0, closes: 0,
    readGate: null, saveGate: null, openGate: null, openError: null, saveError: null,
    async open() {
      db.opens++;
      if (db.openGate) await db.openGate.promise;
      if (db.openError) throw db.openError;
      let closed = false;
      const active = () => { if (closed) throw fail("STORAGE_CLOSED"); };
      const expected = version => {
        active();
        if (version !== db.record.version) throw fail("DRAFT_CONFLICT", "Another tab committed first.");
      };
      return {
        async read() {
          active(); db.reads++;
          const result = structuredClone(db.record);
          if (db.readGate) await db.readGate.promise;
          return result; // Deliberately allow a late read after disposal.
        },
        async save(next, version) {
          active(); db.writes.push(structuredClone(next));
          if (db.saveGate) await db.saveGate.promise;
          expected(version);
          if (db.saveError) throw db.saveError;
          db.record = { schemaVersion: 1, version: version + 1, document: structuredClone(next) };
          return structuredClone(db.record);
        },
        async clear(version) {
          expected(version); db.clears++;
          db.record = { schemaVersion: 1, version: version + 1, document: null };
          return structuredClone(db.record);
        },
        dispose() { if (!closed) { closed = true; db.closes++; } },
      };
    },
  };
  return db;
}
function fixture(t, db = storage(), { draftDelayMs = 60000, openStore = () => db.open() } = {}) {
  const urls = [], revoked = [], downloads = [], calls = [], prompts = [];
  class Element extends EventTarget {
    _value = ""; checked = false; disabled = false; textContent = ""; srcdoc = ""; children = [];
    constructor(tag = "div") { super(); this.tag = tag; }
    get value() { return this._value; }
    set value(value) { this._value = this.tag === "textarea" ? String(value).replace(/\r\n?/g, "\n") : String(value); }
    focus() {}
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
    id === "markdown" || id === "custom-css" ? "textarea" : id === "download-html" ? "a" : "div",
  )]));
  els.markdown.value = "# Original";
  els.font.value = "sans"; els["dark-mode"].value = "auto"; els.title.value = "Original title";
  els["image-folder"].webkitdirectory = true;
  const doc = { querySelector: selector => els[selector.slice(1)], createElement: tag => new Element(tag), body: new Element("body") };
  const win = new EventTarget(), timers = new Map();
  let sequence = 0;
  win.approve = true;
  win.confirm = message => { prompts.push(message); return win.approve; };
  win.setTimeout = fn => { const id = ++sequence; timers.set(id, fn); return id; };
  win.clearTimeout = id => timers.delete(id);
  win.URL = {
    createObjectURL(blob) { const url = `blob:draft-${urls.length}`; urls.push({ url, blob }); return url; },
    revokeObjectURL(url) { revoked.push(url); },
  };
  const factory = () => ({ disposed: false, dispose() { this.disposed = true; },
    render(kind, source, options, controls) {
      const pending = gate();
      calls.push({ kind, source, options, controls, resolve: pending.resolve });
      return pending.promise; // Deliberately ignore cancellation to test ownership.
    },
  });
  const controller = createDemoController(doc, win, factory, { openStore, draftDelayMs });
  t.after(() => controller.dispose());
  const output = (call, text = call.source) => ({
    sourceLength: new TextEncoder().encode(call.source).length, bytes: new TextEncoder().encode(text),
    diagnostics: [], text: () => text, blob: () => new Blob([text]), filename: base => `${base}.${call.kind}`,
  });
  const input = (value, id = "markdown") => { els[id].value = value; els[id].dispatchEvent(new Event("input")); };
  const enable = (value = true) => { els["remember-draft"].checked = value; els["remember-draft"].dispatchEvent(new Event("change")); };
  const click = id => els[id].click();
  const ready = () => until(() => !els["refresh-draft"].disabled);
  const idle = () => until(() => !els["refresh-draft"].disabled);
  const downloaded = () => urls.find(item => item.url === downloads.at(-1)?.url)?.blob;
  return { controller, els, db, win, calls, urls, revoked, downloads, prompts, output, input, enable, click, ready, idle, downloaded };
}

test("autosave is opt-in and only acknowledgment of the latest source is reported as saved", async t => {
  const f = fixture(t); await f.ready();
  f.input("# First edited source"); await tick();
  assert.equal(f.db.writes.length, 0);
  assert.equal(f.els["remember-draft"].checked, false);
  assert.match(f.els["draft-status"].textContent, /Autosave is off/);
  f.db.saveGate = gate(); f.enable(); f.click("save-draft");
  await until(() => f.db.writes.length === 1);
  assert.match(f.els["draft-status"].textContent, /Waiting for a local draft transaction/);
  assert.equal(f.db.record.document, null);
  f.input("# More recent source café😀");
  f.db.saveGate.resolve(); await f.idle();
  assert.equal(f.db.writes.length, 2, "the pending save captures the newest source after its first acknowledgment");
  assert.deepEqual(f.db.record.document, { filename: "franken-markdown.md", source: "# More recent source café😀" });
  assert.match(f.els["draft-status"].textContent, /acknowledged.*revision 2/);
});

test("untouched imported source keeps BOM and CRLF in storage and excludes settings and image authority", async t => {
  const f = fixture(t); await f.ready();
  const source = "\uFEFF# Imported café😀\r\n\r\nline\rnext\r\n";
  assert.equal(await f.controller.openMarkdown(new File([source], "Original Notes.md")), true);
  assert.equal(f.els.markdown.value, source.replace(/\r\n?/g, "\n"));
  f.els["allow-html"].checked = true; f.input("Secret author", "author");
  assert.equal(await f.controller.importImages([new File(["<svg/>"], "chart.svg")]), true);
  f.enable(); f.click("save-draft"); await f.idle();
  assert.deepEqual(f.db.record.document, { filename: "Original Notes.md", source });
  assert.deepEqual(Object.keys(f.db.record.document).sort(), ["filename", "source"]);
  f.input(`${f.els.markdown.value}edited\n`); f.click("save-draft"); await f.idle();
  assert.equal(f.db.record.document.source, f.els.markdown.value, "actual edits persist the current textarea text");
});

test("recovery is explicit and restores exact source bytes while retiring trust, images, exports and pending renders", async t => {
  const source = "\uFEFF# Recovered\r\n\r\n![chart](chart.svg)\r\n";
  const f = fixture(t, storage({ source, filename: "Recovered Notes.md" })); await f.ready();
  assert.equal(f.els.markdown.value, "# Original");
  assert.equal(f.els["remember-draft"].disabled, true);
  assert.match(f.els["draft-status"].textContent, /available.*editor has not been replaced/);
  await f.controller.importImages([new File(["<svg/>"], "chart.svg")]);
  f.els["allow-html"].checked = true;
  const preview = f.controller.renderPreview(), oldCall = f.calls.at(-1);
  oldCall.resolve(f.output(oldCall)); await preview;
  f.controller.downloadMarkdown();
  const oldUrls = f.urls.map(item => item.url);
  const pdf = f.controller.downloadPdf(), late = f.calls.at(-1);
  f.click("restore-draft"); await f.idle();
  assert.equal(f.els.markdown.value, source.replace(/\r\n/g, "\n"));
  assert.equal(f.els.title.value, "Recovered Notes");
  assert.equal(f.els["allow-html"].checked, false);
  assert.equal(f.els["image-references"].textContent, "");
  assert.equal(f.els["download-html"].hidden, true);
  assert.ok(oldUrls.every(url => f.revoked.includes(url)));
  late.resolve(f.output(late)); assert.equal(await pdf, false);
  assert.equal(f.els["remember-draft"].checked, false);
  assert.match(f.els["file-status"].textContent, /^Restored Recovered Notes.md/);
  assert.match(f.els["draft-status"].textContent, /acknowledged.*revision 1/);
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(f.downloads.at(-1).filename, "Recovered Notes.md");
  assert.deepEqual(new Uint8Array(await f.downloaded().arrayBuffer()), new TextEncoder().encode(source));
  const nextPreview = f.controller.renderPreview(), fresh = f.calls.at(-1);
  assert.equal(fresh.options.allowRawHtml, false);
  assert.equal(fresh.options.pdfImages, undefined);
  fresh.resolve(f.output(fresh)); await nextPreview;
});

test("declining recovery preserves the editor and its existing image and raw HTML choices", async t => {
  const f = fixture(t, storage({ source: "saved", filename: "saved.md" })); await f.ready();
  f.input("Current unsaved source");
  await f.controller.importImages([new File(["<svg/>"], "chart.svg")]);
  f.els["allow-html"].checked = true; f.win.approve = false;
  f.click("restore-draft"); await f.idle();
  assert.equal(f.els.markdown.value, "Current unsaved source");
  assert.equal(f.els["allow-html"].checked, true);
  assert.match(f.els["image-references"].textContent, /chart.svg/);
  assert.equal(f.els["restore-draft"].disabled, false);
  assert.equal(f.db.writes.length, 0);
});

for (const action of ["input", "file-open intent", "composition start"]) {
  test(`a late recovery read cannot replace newer ${action}`, async t => {
    const f = fixture(t, storage({ source: "old recovery", filename: "old.md" })); await f.ready();
    f.db.readGate = gate(); f.click("restore-draft"); await until(() => f.db.reads === 2);
    let opened = null, fileRead = null;
    if (action === "input") f.input("Newer typed source");
    else if (action === "composition start") {
      f.els.markdown.dispatchEvent(new Event("compositionstart"));
      f.els.markdown.dispatchEvent(new Event("compositionend"));
    } else {
      fileRead = gate(); const file = new File(["Newer file"], "newer.md");
      file.arrayBuffer = () => fileRead.promise;
      opened = f.controller.openMarkdown(file);
    }
    f.db.readGate.resolve(); await f.idle();
    assert.match(f.els["draft-status"].textContent, /STALE_SOURCE/);
    assert.equal(f.els.markdown.value, action === "input" ? "Newer typed source" : "# Original");
    assert.equal(f.prompts.length, 0, "stale recovery is rejected before asking to replace anything");
    if (opened) {
      fileRead.resolve(new TextEncoder().encode("Newer file").buffer);
      assert.equal(await opened, true);
      assert.equal(f.els.markdown.value, "Newer file");
    }
  });
}

test("literal in-progress IME source can be saved without rendering or permitting recovery replacement", async t => {
  const f = fixture(t); await f.ready(); f.enable();
  f.els.markdown.dispatchEvent(new Event("compositionstart"));
  f.input("# 正在输入 café😀"); f.click("save-draft"); await f.idle();
  assert.equal(f.db.record.document.source, "# 正在输入 café😀");
  assert.equal(f.calls.length, 0, "composition still suppresses preview rendering");
  assert.equal(f.controller.downloadMarkdown(), false);
  f.click("refresh-draft"); await f.idle();
  f.click("restore-draft"); await f.idle();
  assert.match(f.els["draft-status"].textContent, /SOURCE_BUSY/);
  assert.equal(f.prompts.length, 0);
  f.els.markdown.dispatchEvent(new Event("compositionend"));
  f.click("restore-draft"); await f.idle();
  assert.match(f.els["draft-status"].textContent, /acknowledged/);
});

test("a newer explicit restore cancels a pending source-file read", async t => {
  const f = fixture(t, storage({ source: "Recovered instead", filename: "stored.md" })); await f.ready();
  const delayed = gate(), file = new File(["Older file"], "older.md");
  file.arrayBuffer = () => delayed.promise;
  const opened = f.controller.openMarkdown(file);
  f.click("restore-draft"); await f.idle();
  delayed.resolve(new TextEncoder().encode("Older file").buffer);
  assert.equal(await opened, false);
  assert.equal(f.els.markdown.value, "Recovered instead");
  assert.match(f.els["file-status"].textContent, /^Restored stored.md/);
});

test("turning autosave off retains its stored copy; confirmed forgetting keeps current edits and stops saving", async t => {
  const f = fixture(t); await f.ready(); f.input("Saved source"); f.enable();
  f.click("save-draft"); await f.idle(); f.enable(false); f.input("Still in the editor");
  assert.equal(f.db.record.document.source, "Saved source");
  assert.equal(f.els["save-draft"].disabled, true);
  f.win.approve = false; f.click("forget-draft"); await f.idle();
  assert.equal(f.db.record.document.source, "Saved source");
  f.win.approve = true; f.click("forget-draft"); await f.idle();
  assert.equal(f.db.record.document, null);
  assert.equal(f.els.markdown.value, "Still in the editor");
  assert.equal(f.els["remember-draft"].checked, false);
  assert.equal(f.db.writes.length, 1);
  assert.equal(f.db.clears, 1);
});

test("denied storage and a failed save keep Markdown editing and downloading usable", async t => {
  const db = storage(); db.openError = fail("STORAGE_UNAVAILABLE", "Permission denied");
  const f = fixture(t, db); await f.ready();
  assert.match(f.els["draft-status"].textContent, /STORAGE_UNAVAILABLE.*Download Markdown/);
  f.input("Source survives unavailable storage");
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(await f.downloaded().text(), f.els.markdown.value);
  db.openError = null; f.click("refresh-draft"); await f.idle();
  db.saveError = fail("STORAGE_FULL", "Storage full"); f.enable();
  f.click("save-draft"); await f.idle();
  assert.match(f.els["draft-status"].textContent, /STORAGE_FULL/);
  assert.equal(f.els["remember-draft"].checked, false);
  assert.equal(db.record.document, null);
  assert.equal(f.controller.downloadMarkdown(), true);
  assert.equal(await f.downloaded().text(), "Source survives unavailable storage");
});

test("another tab's committed revision cannot be silently overwritten", async t => {
  const f = fixture(t); await f.ready(); f.enable();
  f.db.record = { schemaVersion: 1, version: 1, document: { filename: "other.md", source: "Other tab" } };
  f.input("My source"); f.click("save-draft"); await f.idle();
  assert.match(f.els["draft-status"].textContent, /DRAFT_CONFLICT/);
  assert.equal(f.db.record.document.source, "Other tab");
  assert.equal(f.els.markdown.value, "My source");
  f.click("refresh-draft"); await f.idle();
  assert.match(f.els["draft-status"].textContent, /other.md.*available/);
  assert.equal(f.els.markdown.value, "My source");
});

for (const boundary of ["page suspension", "disposal"]) {
  test(`${boundary} cannot acknowledge a pending write`, async t => {
    const f = fixture(t); await f.ready(); f.enable();
    f.db.saveGate = gate(); f.input("Not yet committed"); f.click("save-draft");
    await until(() => f.db.writes.length === 1);
    if (boundary === "page suspension") f.win.dispatchEvent(new Event("pagehide"));
    else f.controller.dispose();
    f.db.saveGate.resolve(); await tick(); await tick();
    assert.equal(f.db.record.document, null);
    assert.match(f.els["draft-status"].textContent, /pause does not save current changes/);
    assert.equal(f.els["remember-draft"].checked, false);
    assert.equal(f.els["refresh-draft"].disabled, true);
    assert.equal(f.els.markdown.value, "Not yet committed");
    assert.equal(f.db.closes, 1);
  });
}

test("bfcache resume retains the edited source and lossless baseline but requires fresh recovery consent", async t => {
  const f = fixture(t); await f.ready();
  const source = "\uFEFF# Local\r\n";
  await f.controller.openMarkdown(new File([source], "local.md"));
  f.enable(); f.click("save-draft"); await f.idle();
  f.input("Unsaved in-memory changes");
  f.win.dispatchEvent(new Event("pagehide"));
  const show = new Event("pageshow"); show.persisted = true; f.win.dispatchEvent(show);
  await f.ready();
  assert.equal(f.els.markdown.value, "Unsaved in-memory changes");
  assert.equal(f.els["remember-draft"].checked, false);
  assert.match(f.els["draft-status"].textContent, /local.md.*available/);
  assert.equal(f.db.writes.length, 1, "resume does not fabricate an unload commit");
  f.click("restore-draft"); await f.idle();
  f.controller.downloadMarkdown();
  assert.deepEqual(new Uint8Array(await f.downloaded().arrayBuffer()), new TextEncoder().encode(source));
  assert.equal(f.db.opens, 2); assert.equal(f.db.closes, 1);
});

test("suspension during a recovery read or database open retires that owner before bfcache resumption", async t => {
  const db = storage({ source: "Stored before suspend", filename: "old.md" });
  const f = fixture(t, db); await f.ready();
  db.readGate = gate(); f.click("restore-draft"); await until(() => db.reads === 2);
  f.win.dispatchEvent(new Event("pagehide")); db.readGate.resolve(); await tick(); await tick();
  assert.equal(f.els.markdown.value, "# Original");
  assert.equal(f.prompts.length, 0);
  db.readGate = null; db.openGate = gate();
  const show = new Event("pageshow"); show.persisted = true; f.win.dispatchEvent(show);
  await until(() => db.opens === 2);
  f.win.dispatchEvent(new Event("pagehide")); db.openGate.resolve(); await tick(); await tick();
  assert.equal(db.closes, 2, "a late opened connection is closed without installation");
  assert.equal(f.els["refresh-draft"].disabled, true);
});

for (const operation of ["database open", "recovery read", "manual save"]) {
  test(`late pre-suspension ${operation} cannot disable the resumed recovery owner`, async t => {
    const db = storage({ source: "Stored source", filename: "stored.md" }), hold = gate();
    if (operation === "database open") db.openGate = hold;
    const f = fixture(t, db);
    if (operation === "database open") await until(() => db.opens === 1);
    else {
      await f.ready();
      if (operation === "recovery read") {
        db.readGate = hold; f.click("restore-draft"); await until(() => db.reads === 2);
      } else {
        f.click("restore-draft"); await f.idle(); f.enable(); f.input("Old pending source");
        db.saveGate = hold; f.click("save-draft"); await until(() => db.writes.length === 1);
      }
    }
    f.win.dispatchEvent(new Event("pagehide"));
    db.openGate = db.readGate = db.saveGate = null;
    const show = new Event("pageshow"); show.persisted = true; f.win.dispatchEvent(show);
    await f.ready(); f.click("restore-draft"); await f.idle(); f.enable();
    assert.equal(f.els["remember-draft"].checked, true);
    assert.equal(f.els["refresh-draft"].disabled, false);
    hold.resolve(); await tick(); await tick();
    assert.equal(f.els["remember-draft"].checked, true, "old completion cannot revoke new consent");
    assert.equal(f.els["remember-draft"].disabled, false);
    assert.equal(f.els["refresh-draft"].disabled, false);
    assert.equal(f.els.markdown.value, "Stored source");
    f.input("New owner's work"); f.click("save-draft"); await f.idle();
    assert.equal(db.record.document.source, "New owner's work", "resumed controls remain usable");
    assert.equal(db.record.version, 2);
  });
}
