import assert from "node:assert/strict";
import test from "node:test";
import { fixture, MemoryFileHandle, deferred, draftStorage, until, tick } from "./demo_file_helpers.mjs";

test("editable open and verified save preserve exact BOM, Unicode and mixed newlines", async t => {
  const f = fixture(t), source = "\uFEFF# café😀\r\nline\rnext\n";
  f.win.picked = new MemoryFileHandle("原稿.md", source);
  f.els["allow-html"].checked = true;
  await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]);
  assert.deepEqual(await f.files.open(), { opened: true, filename: "原稿.md" });
  assert.equal(f.els.markdown.value, source.replace(/\r\n?/g, "\n"));
  assert.equal(f.els["allow-html"].checked, false); assert.equal(f.els["image-references"].textContent, "");
  assert.equal(f.files.state.phase, "saved"); assert.equal(f.files.state.dirty, false);
  assert.equal(f.win.picked.counts.permission ?? 0, 0, "opening does not request write access");
  await f.files.save();
  assert.equal(f.win.picked.source, source);
  f.controller.downloadMarkdown();
  assert.equal(f.downloads.at(-1).filename, "原稿.md");
  assert.deepEqual(new Uint8Array(await f.urls.at(-1).blob.arrayBuffer()), new TextEncoder().encode(source));
});

test("saving current source preserves image access, settings and prepared HTML/workspace bytes", async t => {
  const f = fixture(t); await f.files.open(); f.input("# Edited\n\n![A](chart.svg)");
  await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]);
  f.input("serif", "font"); f.input("Custom title", "title");
  const preview = f.controller.renderPreview(), call = f.renders.at(-1); call.resolve(f.output(call, "HTML")); await preview;
  const workspace = f.controller.prepareWorkspace(), build = f.workspaces.at(-1); build.resolve(f.output(build, "WORKSPACE")); await workspace;
  const urls = f.urls.map(item => item.url), images = f.els["image-references"].textContent;
  f.win.picked.hooks.create = (_, options) => assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
  const result = await f.files.save();
  assert.equal(result.current, true); assert.equal(f.win.picked.source, f.els.markdown.value);
  assert.equal(f.els.font.value, "serif"); assert.equal(f.els.title.value, "Custom title");
  assert.equal(f.els["image-references"].textContent, images);
  assert.equal(f.els["download-html"].hidden, false); assert.equal(f.els["download-workspace"].hidden, false);
  assert(urls.every(url => !f.revoked.includes(url))); assert.equal(f.renders.length, 1); assert.equal(f.workspaces.length, 1);
  assert.equal(f.win.picked.counts.permission, 1); assert.equal(f.win.picked.counts.close, 1);
});

test("Save as adopts only a verified new file and preserves renderer settings and resources", async t => {
  const f = fixture(t); await f.files.open(); f.input("# New file");
  await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]);
  const before = f.els["image-references"].textContent, title = f.els.title.value;
  f.win.saveTarget = new MemoryFileHandle("New Notes.md", "");
  assert.deepEqual(await f.files.saveAs(), { saved: true, current: true, filename: "New Notes.md" });
  assert.equal(f.files.state.filename, "New Notes.md"); assert.equal(f.files.state.phase, "saved");
  assert.equal(f.els.title.value, title); assert.equal(f.els["image-references"].textContent, before);
  assert.equal(f.win.picked.source, "# Disk\n"); assert.equal(f.win.saveTarget.source, "# New file");
  f.controller.downloadMarkdown(); assert.equal(f.downloads.at(-1).filename, "New Notes.md");
});

test("same-size external edits refuse overwrite and same-entry Save as cannot bypass conflict", async t => {
  const f = fixture(t); f.win.picked.source = "aaa"; await f.files.open(); f.input("my changes");
  f.win.picked.source = "bbb";
  await assert.rejects(f.files.save(), { code: "FILE_CHANGED" });
  assert.equal(f.win.picked.source, "bbb"); assert.equal(f.els.markdown.value, "my changes");
  assert.equal(f.files.state.phase, "conflict"); assert.equal(f.els["direct-file-save"].disabled, true);
  assert.equal(f.win.picked.counts.create ?? 0, 0);
  f.win.saveTarget = new MemoryFileHandle(f.win.picked.name, "", f.win.picked.entry);
  await assert.rejects(f.files.saveAs(), { code: "FILE_CHANGED" });
  assert.equal(f.win.saveTarget.counts.create ?? 0, 0);
  f.win.saveTarget = new MemoryFileHandle("recovered.md", "");
  assert.equal((await f.files.saveAs()).current, true); assert.equal(f.win.saveTarget.source, "my changes");
});

test("Reload explicitly replaces source and grants only after confirmation and a second disk comparison", async t => {
  const f = fixture(t); await f.files.open(); f.input("unsaved");
  await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]);
  f.win.picked.source = "\uFEFF# External\r\n"; f.win.approve = false;
  assert.deepEqual(await f.files.reload(), { opened: false });
  assert.equal(f.els.markdown.value, "unsaved"); assert.match(f.els["image-references"].textContent, /chart/);
  const approval = deferred(); f.win.approve = () => approval.promise;
  const pending = f.files.reload(); await until(() => f.prompts.length === 3);
  f.win.picked.source = "Changed during confirmation"; approval.resolve(true);
  await assert.rejects(pending, { code: "FILE_CHANGED" }); assert.equal(f.els.markdown.value, "unsaved");
  f.win.approve = true; assert.equal((await f.files.reload()).opened, true);
  assert.equal(f.els.markdown.value, "Changed during confirmation"); assert.equal(f.els["image-references"].textContent, "");
});

test("overwriting a nonempty Save as target requires explicit confirmation", async t => {
  const f = fixture(t); f.win.saveTarget = new MemoryFileHandle("existing.md", "keep this"); f.win.approve = false;
  assert.deepEqual(await f.files.saveAs(), { saved: false, current: false });
  assert.equal(f.win.saveTarget.source, "keep this"); assert.equal(f.win.saveTarget.counts.create ?? 0, 0);
  assert.match(f.prompts[0], /Overwrite.*existing.md/); assert.equal(f.files.state.filename, null);
  f.win.approve = true; assert.equal((await f.files.saveAs()).saved, true);
  assert.equal(f.win.saveTarget.source, "# Original");
});

for (const change of ["source edit", "edit away and back", "composition", "source access"]) {
  test(`${change} while a write is staged aborts before close`, async t => {
    const f = fixture(t, { observeAccess: true }); await f.files.open(); f.input("captured");
    const pendingWrite = deferred(); f.win.picked.hooks.write = () => pendingWrite.promise;
    const pending = f.files.save(); await until(() => f.win.picked.counts.write === 1);
    if (change === "source edit") f.input("newer");
    else if (change === "edit away and back") { f.input("newer"); f.input("captured"); }
    else if (change === "composition") { f.els.markdown.dispatchEvent(new Event("compositionstart")); f.els.markdown.dispatchEvent(new Event("compositionend")); }
    else { f.els.markdown.readOnly = true; f.els.markdown.readOnly = false; }
    pendingWrite.resolve(); await assert.rejects(pending, { code: "STALE_SOURCE" });
    assert.equal(f.win.picked.source, "# Disk\n"); assert.equal(f.win.picked.counts.close ?? 0, 0);
    assert.equal(f.win.picked.counts.abort, 1); assert.equal(f.files.state.phase, "edited");
  });
}

test("renderer settings and resource changes do not cancel an unchanged source save", async t => {
  const f = fixture(t); await f.files.open(); f.input("captured");
  const delayed = deferred(); f.win.picked.hooks.write = () => delayed.promise;
  const saving = f.files.save(); await until(() => f.win.picked.counts.write === 1);
  f.input("Another title", "title"); await f.controller.importImages([new File(["<svg/>"] , "chart.svg")]);
  delayed.resolve(); assert.equal((await saving).current, true);
  assert.equal(f.win.picked.source, "captured"); assert.match(f.els["image-references"].textContent, /chart/);
});

test("newer edits during close remain unsaved while the captured write is accurately acknowledged", async t => {
  const f = fixture(t); await f.files.open(); f.input("captured");
  const delayed = deferred(); f.win.picked.hooks.close = () => delayed.promise;
  const saving = f.files.save(); await until(() => f.win.picked.counts.close === 1);
  f.input("newer edits"); delayed.resolve();
  assert.deepEqual(await saving, { saved: true, current: false, filename: "notes.md" });
  assert.equal(f.win.picked.source, "captured"); assert.equal(f.els.markdown.value, "newer edits");
  assert.equal(f.files.state.phase, "edited"); assert.match(f.els["direct-file-status"].textContent, /newer editor changes were not saved/);
});

test("unverifiable post-close contents remain uncertain and block automatic current-file retries", async t => {
  const f = fixture(t); await f.files.open(); f.input("captured");
  f.win.picked.hooks.committed = () => { f.win.picked.source = "external after close"; };
  await assert.rejects(f.files.save(), { code: "FILE_SAVE_UNCERTAIN" });
  assert.equal(f.files.state.phase, "uncertain"); assert.equal(f.els.markdown.value, "captured");
  await assert.rejects(f.files.save(), { code: "FILE_CHANGED" }); assert.equal(f.win.picked.counts.close, 1);
  assert.equal(f.files.backup().downloaded, true); assert.equal(f.files.state.phase, "uncertain");
});

test("ordinary file import disconnects even an identical connected source and cancels an older picker", async t => {
  const f = fixture(t); await f.files.open();
  assert.equal(await f.controller.openMarkdown(new File(["# Disk\n"], "notes.md")), true);
  assert.equal(f.files.state.filename, null);
  f.win.openGate = deferred(); const pending = f.files.open();
  assert.equal(await f.controller.openMarkdown(new File(["newer import"], "newer.md")), true);
  f.win.openGate.resolve([new MemoryFileHandle("old.md", "old picker")]);
  await assert.rejects(pending, { code: "STALE_SOURCE" });
  assert.equal(f.els.markdown.value, "newer import"); assert.equal(f.files.state.filename, null);
});

test("source-only draft recovery disconnects a file and never imports its write authority", async t => {
  const db = draftStorage({ filename: "stored.md", source: "\uFEFF# Stored\r\n" });
  const f = fixture(t, { db }); await f.ready(); await f.files.open();
  f.els["restore-draft"].click(); await until(() => !f.els["refresh-draft"].disabled);
  assert.equal(f.els.markdown.value, "\uFEFF# Stored\n"); assert.equal(f.files.state.filename, null);
  assert.equal(f.win.picked.source, "# Disk\n"); assert.equal(f.win.picked.counts.create ?? 0, 0);
});

test("direct file-open intent fences a pending recovery read before its picker returns", async t => {
  const db = draftStorage({ filename: "stored.md", source: "Older stored source" });
  const f = fixture(t, { db }); await f.ready(); db.readGate = deferred();
  f.els["restore-draft"].click(); await until(() => db.reads === 2);
  f.win.openGate = deferred(); const opened = f.files.open();
  db.readGate.resolve(); await until(() => !f.els["refresh-draft"].disabled);
  assert.equal(f.els.markdown.value, "# Original"); assert.match(f.els["draft-status"].textContent, /STALE_SOURCE/);
  f.win.openGate.resolve([f.win.picked]); assert.equal((await opened).opened, true);
});

test("Save as updates the opt-in source draft filename without persisting a handle or resources", async t => {
  const f = fixture(t); await f.ready(); await f.files.open();
  f.els["remember-draft"].checked = true; f.els["remember-draft"].dispatchEvent(new Event("change"));
  f.win.saveTarget = new MemoryFileHandle("renamed.md", ""); await f.files.saveAs();
  f.els["save-draft"].click(); await until(() => !f.els["refresh-draft"].disabled);
  assert.deepEqual(f.db.record.document, { filename: "renamed.md", source: "# Disk\n" });
  assert.deepEqual(Object.keys(f.db.record.document).sort(), ["filename", "source"]);
});

test("an ordinary import during a staged save aborts the old write and drops its connection", async t => {
  const f = fixture(t); await f.files.open(); f.input("captured");
  const delayed = deferred(); f.win.picked.hooks.write = () => delayed.promise;
  const saving = f.files.save(); await until(() => f.win.picked.counts.write === 1);
  assert.equal(await f.controller.openMarkdown(new File(["newer import"], "new.md")), true);
  delayed.resolve(); await assert.rejects(saving, { code: "STALE_SOURCE" });
  assert.equal(f.win.picked.source, "# Disk\n"); assert.equal(f.els.markdown.value, "newer import"); assert.equal(f.files.state.filename, null);
});

test("pending direct source replacement retires workspace preparation and blocks its publication", async t => {
  const f = fixture(t), pending = f.controller.prepareWorkspace(), old = f.workspaces[0];
  f.win.openGate = deferred(); const opening = f.files.open();
  assert.equal(old.controls.signal.aborted, true); assert.equal(f.els["prepare-workspace"].disabled, true);
  assert.equal(await f.controller.prepareWorkspace(), false);
  old.resolve(f.output(old)); assert.equal(await pending, false); assert.equal(f.urls.length, 0);
  f.win.openGate.resolve([f.win.picked]); await opening;
  const fresh = f.controller.prepareWorkspace(), build = f.workspaces[1];
  assert.equal(build.source, "# Disk\n"); build.resolve(f.output(build)); assert.equal(await fresh, true);
});

for (const boundary of ["page suspension", "disposal"]) test(`${boundary} aborts a staged write without affecting the editor`, async t => {
  const f = fixture(t); await f.files.open(); f.input("captured");
  const delayed = deferred(); f.win.picked.hooks.write = () => delayed.promise;
  const saving = f.files.save(); await until(() => f.win.picked.counts.write === 1);
  if (boundary === "page suspension") f.win.dispatchEvent(new Event("pagehide")); else f.controller.dispose();
  delayed.resolve(); await assert.rejects(saving, { code: "SESSION_DISPOSED" });
  assert.equal(f.win.picked.source, "# Disk\n"); assert.equal(f.els.markdown.value, "captured");
  assert.equal(f.files.state.filename, null); assert.equal(f.win.picked.counts.close ?? 0, 0);
});

test("bfcache resume creates a new authority owner and ignores an old file-open result", async t => {
  const f = fixture(t); f.win.openGate = deferred(); const delayed = f.win.openGate;
  const opening = f.files.open(); f.win.dispatchEvent(new Event("pagehide")); f.resume();
  assert.equal(f.files.state.filename, null);
  f.win.openGate = null; f.win.picked = new MemoryFileHandle("new.md", "New page source");
  await f.files.open(); const status = f.els["direct-file-status"].textContent;
  delayed.resolve([new MemoryFileHandle("old.md", "Old page source")]);
  await assert.rejects(opening, { code: "SESSION_DISPOSED" });
  assert.equal(f.els.markdown.value, "New page source"); assert.equal(f.files.state.filename, "new.md");
  assert.equal(f.els["direct-file-status"].textContent, status);
});

for (const change of ["silent focus edit", "input away and back", "temporary access lock"]) test(`${change} during installation cannot inherit the selected file handle`, async t => {
  const f = fixture(t, { observeAccess: true });
  if (change === "silent focus edit") f.els.markdown.addEventListener("focus", () => { f.els.markdown.value = "Newer focus text"; }, { once: true });
  else f.els.markdown.addEventListener("input", function once() {
    f.els.markdown.removeEventListener("input", once);
    if (change === "input away and back") { f.input("Changed"); f.input("# Disk\n"); }
    else { f.els.markdown.readOnly = true; f.els.markdown.readOnly = false; }
  });
  await assert.rejects(f.files.open(), { code: "STALE_SOURCE" });
  assert.equal(f.files.state.filename, null); assert.equal(f.win.picked.counts.create ?? 0, 0);
  assert.equal(f.els.markdown.value, change === "silent focus edit" ? "Newer focus text" : "# Disk\n");
});

test("Save as filename callback reentry retains newer source without adopting the verified file", async t => {
  const f = fixture(t); await f.files.open(); f.input("captured"); f.controller.downloadMarkdown();
  const revoke = f.win.URL.revokeObjectURL; let once = true;
  f.win.URL.revokeObjectURL = url => { revoke(url); if (once) { once = false; f.input("Newer during rename"); } };
  const result = await f.files.saveAs();
  assert.deepEqual(result, { saved: true, current: false, filename: "saved.md" });
  assert.equal(f.win.saveTarget.source, "captured"); assert.equal(f.els.markdown.value, "Newer during rename");
  assert.equal(f.files.state.filename, null);
});

test("keyboard save invokes native pickers synchronously and repeated/composing keys do not start extra work", async t => {
  const f = fixture(t); f.win.saveGate = deferred();
  const event = f.key(); assert.equal(event.defaultPrevented, true);
  assert.equal(f.pickers.length, 1, "picker starts before returning from the original key event");
  f.key({ repeat: true }); f.key({ isComposing: true }); assert.equal(f.pickers.length, 1);
  f.win.saveGate.resolve(f.win.saveTarget); await until(() => f.files.state.filename === "saved.md" && !f.files.state.busy);
  f.input("saved by keyboard"); f.key(); await until(() => f.files.state.phase === "saved" && !f.files.state.busy);
  assert.equal(f.win.saveTarget.source, "saved by keyboard"); assert.equal(f.pickers.length, 1);
});

test("unsupported pickers retain ordinary imports and source-download keyboard recovery", async t => {
  const f = fixture(t, { supported: false });
  assert.equal(f.els["direct-file-open"].disabled, true); assert.equal(f.els["direct-file-save-as"].disabled, true);
  assert.match(f.els["direct-file-status"].textContent, /pickers are unavailable/);
  assert.equal(await f.controller.openMarkdown(new File(["source"], "input.md")), true);
  f.key(); assert.equal(f.downloads.length, 1); assert.equal(f.downloads[0].filename, "input.md");
  assert.equal(f.files.state.filename, null); assert.equal(f.files.state.dirty, true);
});

test("Open and Save as picker availability are detected independently", async t => {
  const f = fixture(t, { openSupported: false, saveSupported: true });
  assert.equal(f.els["direct-file-open"].disabled, true); assert.equal(f.els["direct-file-save-as"].disabled, false);
  assert.equal((await f.files.saveAs()).saved, true); assert.equal(f.win.saveTarget.source, "# Original");
});

test("beforeunload warns for unsaved source and pending writes, not a verified current baseline", async t => {
  const f = fixture(t); const warn = () => { const event = new Event("beforeunload", { cancelable: true }); f.win.dispatchEvent(event); return event.defaultPrevented; };
  assert.equal(warn(), false); f.input("edited"); assert.equal(warn(), true);
  await f.files.saveAs(); assert.equal(warn(), false);
  f.input("new"); assert.equal(warn(), true); f.win.dispatchEvent(new Event("pagehide")); assert.equal(warn(), false);
});

test("permission denial and non-Markdown targets leave editable source and prior file contents intact", async t => {
  const f = fixture(t); await f.files.open(); f.input("edited"); f.win.picked.permission = "denied";
  await assert.rejects(f.files.save(), { code: "FILE_PERMISSION_DENIED" }); assert.equal(f.win.picked.counts.create ?? 0, 0);
  f.win.saveTarget = new MemoryFileHandle("output.html", "do not overwrite");
  await assert.rejects(f.files.saveAs(), { code: "INVALID_FILENAME" }); assert.equal(f.win.saveTarget.source, "do not overwrite");
  assert.equal(f.els.markdown.value, "edited"); assert.equal(f.controller.downloadMarkdown(), true);
  await tick();
});

test("renderer startup failure and concurrent workspace preparation leave file saving source-owned", async t => {
  const f = fixture(t, { renderFailure: Object.assign(new Error("No rendering worker"), { code: "WORKER_UNAVAILABLE" }) });
  await f.files.open(); f.input("# Save without a preview\n");
  await f.controller.renderPreview();
  assert.match(f.els.diagnostics.children[0].textContent, /WORKER_UNAVAILABLE/);
  const preparing = f.controller.prepareWorkspace();
  assert.equal(f.workspaces.length, 1);
  assert.equal(f.els["direct-file-save"].disabled, false);
  const result = await f.files.save();
  assert.equal(result.current, true); assert.equal(f.win.picked.source, "# Save without a preview\n");
  assert.equal(f.workspaces[0].controls.signal.aborted, false);
  f.workspaces[0].resolve(f.output(f.workspaces[0], "<!doctype html>workspace"));
  assert.equal(await preparing, true);
  assert.equal(f.els["download-workspace"].hidden, false);
  assert.equal(f.files.state.phase, "saved");
});
