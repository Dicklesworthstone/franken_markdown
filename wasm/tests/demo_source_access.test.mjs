// Actual main-demo source owner and shared draft controls. The existing fixture
// doubles DOM mutation delivery, File reads and storage acknowledgment; it does
// not establish real-browser or generated-WASM acceptance.
import assert from "node:assert/strict";
import test from "node:test";
import { fixture, deferred, draftStorage, tick, until } from "./demo_file_helpers.mjs";

function delayedFile(source = "# Incoming", name = "incoming.md") {
  const file = new File([source], name), gate = deferred();
  const read = file.arrayBuffer.bind(file);
  let reads = 0;
  file.arrayBuffer = async () => { reads++; await gate.promise; return read(); };
  return { file, gate, get reads() { return reads; } };
}
const transitions = [
  ["readonly", f => { f.els.markdown.readOnly = true; }, f => { f.els.markdown.readOnly = false; }],
  ["disabled", f => { f.els.markdown.disabled = true; }, f => { f.els.markdown.disabled = false; }],
  ["detached", f => { f.access.mount(f.els.markdown, false); }, f => { f.access.mount(f.els.markdown, true); }],
  ["readonly then writable", f => { f.els.markdown.readOnly = true; f.els.markdown.readOnly = false; }, () => {}],
  ["disabled then enabled", f => { f.els.markdown.disabled = true; f.els.markdown.disabled = false; }, () => {}],
  ["removed then reattached", f => { f.access.mount(f.els.markdown, false); f.access.mount(f.els.markdown, true); }, () => {}],
];

for (const [name, change, restoreAccess] of transitions) {
  test(`ordinary file reads cannot replace source after it becomes ${name}`, async t => {
    const f = fixture(t, { observeAccess: true }); await f.ready();
    f.els["allow-html"].checked = true;
    const before = f.els.markdown.value, title = f.els.title.value;
    const incoming = delayedFile(), pending = f.controller.openMarkdown(incoming.file);
    assert.equal(incoming.reads, 1);
    change(f);
    assert(f.access.pendingRecords > 0, "publication must drain pending observer records");
    incoming.gate.resolve();
    assert.equal(await pending, false);
    assert.equal(f.els.markdown.value, before);
    assert.equal(f.els.title.value, title);
    assert.equal(f.els["allow-html"].checked, true);
    restoreAccess(f);
    assert.equal(await f.controller.openMarkdown(new File(["# Fresh file"], "fresh.md")), true);
    assert.equal(f.els.markdown.value, "# Fresh file");
    assert.equal(f.els["allow-html"].checked, false);
  });

  test(`browser recovery cannot replace source after it becomes ${name}`, async t => {
    const stored = { source: "# Stored recovery", filename: "recovery.md" };
    const db = draftStorage(stored), f = fixture(t, { observeAccess: true, db }); await f.ready();
    f.els["allow-html"].checked = true;
    const before = f.els.markdown.value, title = f.els.title.value;
    const pending = deferred(); db.readGate = pending;
    f.els["restore-draft"].click();
    await until(() => db.reads === 2);
    change(f);
    assert(f.access.pendingRecords > 0, "recovery must drain pending observer records");
    pending.resolve();
    await f.ready();
    assert.equal(f.els.markdown.value, before);
    assert.equal(f.els.title.value, title);
    assert.equal(f.els["allow-html"].checked, true);
    assert.deepEqual(db.record.document, stored);
    assert.equal(db.writes.length, 0);
    assert.match(f.els["draft-status"].textContent, /EDITOR_BUSY|STALE_SOURCE/);
    restoreAccess(f); f.access.deliver(); db.readGate = null;
    f.els["restore-draft"].click(); await f.ready();
    assert.equal(f.els.markdown.value, stored.source);
    assert.equal(f.els["allow-html"].checked, false);
    assert.deepEqual(db.record.document, stored);
  });
}

test("locked source refuses ordinary file admission while exact Markdown backups remain usable", async t => {
  const f = fixture(t, { observeAccess: true }); await f.ready();
  const source = "\uFEFF# Preserve café\r\n\r\noriginal line\r\n";
  assert.equal(await f.controller.openMarkdown(new File([source], "preserve.md")), true);
  f.els.markdown.readOnly = true; f.access.deliver();
  assert.equal(f.els["open-markdown"].disabled, true);
  assert.equal(f.els["markdown-file"].disabled, true);
  assert.equal(f.els["download-markdown"].disabled, false);
  const incoming = delayedFile();
  assert.equal(await f.controller.openMarkdown(incoming.file), false);
  assert.equal(incoming.reads, 0);
  assert.equal(f.controller.downloadMarkdown(), true);
  const download = f.downloads.at(-1);
  assert.equal(download.filename, "preserve.md");
  const blob = f.urls.find(item => item.url === download.url).blob;
  assert.equal(new TextDecoder("utf-8", { ignoreBOM: true }).decode(await blob.arrayBuffer()), source);
  assert.equal(f.els.markdown.value, source.replace(/\r\n?/g, "\n"));
  f.els.markdown.readOnly = false;
  assert.equal(await f.controller.openMarkdown(new File(["# New"], "new.md")), true);
  assert.equal(f.els.markdown.value, "# New");
});

test("ordinary file confirmation cannot revive permission after a same-task lock and unlock", async t => {
  const f = fixture(t, { observeAccess: true }); await f.ready();
  f.input("# Unsaved work");
  f.win.approve = () => { f.els.markdown.readOnly = true; f.els.markdown.readOnly = false; return true; };
  assert.equal(await f.controller.openMarkdown(new File(["# Incoming"], "incoming.md")), false);
  assert.equal(f.prompts.length, 1);
  assert.equal(f.els.markdown.value, "# Unsaved work");
  f.win.approve = true;
  assert.equal(await f.controller.openMarkdown(new File(["# Fresh"], "fresh.md")), true);
  assert.equal(f.els.markdown.value, "# Fresh");
});

test("draft confirmation cannot revive permission after same-task removal and reattachment", async t => {
  const db = draftStorage({ source: "# Recovery", filename: "recovery.md" });
  const f = fixture(t, { observeAccess: true, db }); await f.ready();
  f.win.approve = () => { f.access.mount(f.els.markdown, false); f.access.mount(f.els.markdown, true); return true; };
  f.els["restore-draft"].click(); await f.ready();
  assert.equal(f.prompts.length, 1);
  assert.equal(f.els.markdown.value, "# Original");
  assert.match(f.els["draft-status"].textContent, /STALE_SOURCE/);
  f.win.approve = true;
  f.els["restore-draft"].click(); await f.ready();
  assert.equal(f.els.markdown.value, "# Recovery");
});

test("access changes do not dirty an acknowledged draft or invalidate an unchanged source backup", async t => {
  const f = fixture(t, { observeAccess: true }); await f.ready();
  f.els["remember-draft"].checked = true;
  f.els["remember-draft"].dispatchEvent(new Event("change"));
  f.els["save-draft"].click(); await f.ready();
  assert.equal(f.db.writes.length, 1);
  assert.equal(f.els["save-draft"].disabled, true);
  assert.equal(f.controller.downloadMarkdown(), true);
  const backup = f.downloads.at(-1).url;
  const stored = structuredClone(f.db.record);
  f.els.markdown.readOnly = true; f.els.markdown.readOnly = false;
  f.access.deliver(); await tick();
  assert.equal(f.els["remember-draft"].checked, true);
  assert.equal(f.els["save-draft"].disabled, true);
  assert.deepEqual(f.db.record, stored);
  assert.equal(f.db.writes.length, 1);
  assert.equal(f.revoked.includes(backup), false);
  assert.match(f.els["draft-status"].textContent, /acknowledged.*revision 1/);
});

test("disposal disconnects source-access observers and late mutations cannot touch a successor UI", async t => {
  const f = fixture(t, { observeAccess: true }); await f.ready();
  assert(f.access.observerCount > 0);
  f.controller.dispose();
  assert.equal(f.access.observerCount, 0);
  f.els["open-markdown"].disabled = false;
  f.els["file-status"].textContent = "New owner";
  f.els.markdown.readOnly = true; f.access.deliver();
  assert.equal(f.els["open-markdown"].disabled, false);
  assert.equal(f.els["file-status"].textContent, "New owner");
});
