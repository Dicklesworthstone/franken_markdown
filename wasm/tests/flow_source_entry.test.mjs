// Execute the real source entrypoint without any renderer/WASM artifact and
// with storage unavailable. Elements/window are explicit EventTarget doubles;
// File, Blob and object-URL lifetime are real Node implementations.

import assert from "node:assert/strict";
import { File } from "node:buffer";
import test from "node:test";
import { serializeFlowProject, parseFlowProject } from "../demo/flow_project.mjs";
import { getRenderSettingsBinding } from "../demo/flow_render_settings.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));
let sequence = 0;
class Element extends EventTarget {
  constructor(tag = "div") { super(); this.tagName = tag.toUpperCase(); }
  #value = "";
  style = {};
  children = [];
  querySelector(selector) {
    if (selector === "#" + this.id) return this;
    for (const child of this.children) {
      const found = child.querySelector(selector); if (found) return found;
    }
    return null;
  }
  parentNode = null;
  setAttribute(name, value) { this[name] = value; }
  append(...children) { for (const child of children) { child.parentNode = this; this.children.push(child); } }
  insertBefore(child, next) {
    child.parentNode = this;
    const index = this.children.indexOf(next);
    if (index < 0) this.children.push(child); else this.children.splice(index, 0, child);
  }
  get nextSibling() { return this.parentNode?.children[this.parentNode.children.indexOf(this) + 1] ?? null; }
  remove() {
    if (this.parentNode) this.parentNode.children.splice(this.parentNode.children.indexOf(this), 1);
    this.parentNode = null;
  }
  setRangeText(text, start, end) { this.value = this.value.slice(0, start) + text + this.value.slice(end); }
  disabled = false;
  checked = false;
  hidden = false;
  textContent = "";
  files = [];
  selectionStart = 0;
  selectionEnd = 0;
  selectionDirection = "none";
  get value() {
    return this.#value;
  }
  set value(next) {
    this.#value = this.tagName === "INPUT" ? next.replace(/[\r\n]/g, "") : next.replace(/\r\n?/g, "\n");
    this.setSelectionRange(this.#value.length, this.#value.length);
  }
  setSelectionRange(start, end, direction = "none") {
    this.selectionStart = start;
    this.selectionEnd = end;
    this.selectionDirection = direction;
  }
  focus() {}
  select() {
    this.setSelectionRange(0, this.value.length);
  }
  removeAttribute(name) {
    delete this[name];
  }
  click() {
    const event = new Event("click", { cancelable: true });
    this.dispatchEvent(event);
    return event;
  }
}
async function setup(t) {
  const names = [
    "source",
    "source-filename",
    "source-status",
    "open-markdown",
    "prepare-markdown",
    "source-download",
    "remember-draft",
    "refresh-draft",
    "restore-draft",
    "forget-draft",
    "save-draft",
    "draft-status",
    "source-undo",
    "source-redo",
    "source-find",
    "source-find-insensitive",
    "source-find-previous",
    "source-find-next",
    "source-replacement",
    "source-replace",
    "source-replace-all",
    "source-edit-status",
    "export-controls",
    "open-flow-project",
    "prepare-flow-project",
    "cancel-flow-project",
    "flow-project-download",
    "flow-project-status",
  ];
  const nodes = Object.fromEntries(names.map((name) => [name, new Element()]));
  nodes.source.value = "# Existing source";
  nodes["source-filename"].value = "document.md";
  const host = new EventTarget();
  host.confirm = () => true;
  host.URL = URL;
  const previous = Object.fromEntries(
    ["window", "document", "indexedDB"].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  const doc = {
    defaultView: host,
    querySelector: (query) => nodes[query.slice(1)] ?? parent.querySelector(query),
    createElement(tag) { const element = new Element(tag); element.ownerDocument = doc; return element; },
  };
  for (const [id, node] of Object.entries(nodes)) { node.ownerDocument = doc; node.id = id; }
  const parent = doc.createElement();
  parent.append(nodes.source, nodes["export-controls"]);
  Object.defineProperties(globalThis, {
    window: { configurable: true, value: host },
    document: { configurable: true, value: doc },
    indexedDB: { configurable: true, value: null },
  });
  t.after(() => {
    host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: false }));
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  });
  await import(`../demo/flow-source.js?test=${++sequence}`);
  await tick();
  return { nodes, host, doc };
}

function applySettings(doc, values) {
  for (const [key, value] of Object.entries(values))
    doc.querySelector("#render-" + key).value = value === null ? "" : String(value);
  const form = doc.querySelector("#render-settings-form");
  form.dispatchEvent(new Event("input"));
  form.dispatchEvent(new Event("submit", { cancelable: true }));
}
async function openProject(nodes, file) {
  nodes["open-flow-project"].files = [file];
  nodes["open-flow-project"].dispatchEvent(new Event("change"));
  await tick(); await tick();
}
async function readProject(nodes) {
  nodes["prepare-flow-project"].click();
  assert.equal(nodes["flow-project-download"].hidden, false, nodes["flow-project-status"].textContent);
  return parseFlowProject(await (await fetch(nodes["flow-project-download"].href)).text());
}
function projectFile(source = "# Recovered", settings = {}) {
  return new File([serializeFlowProject({ filename: "recovered.md", source }, settings)], "recovered.fmdflow.json");
}
test("source entry opens and downloads Markdown with no WASM and no IndexedDB", async (t) => {
  const { nodes } = await setup(t);
  assert(nodes["draft-status"].textContent.includes("STORAGE_UNAVAILABLE"));
  assert.equal(nodes["prepare-markdown"].disabled, false);
  nodes["prepare-markdown"].click();
  assert.equal(await (await fetch(nodes["source-download"].href)).text(), "# Existing source");
  const previous = nodes["source-download"].href,
    sequence = [];
  nodes.source.addEventListener("fmd-document-replaced", () => sequence.push("revoke"));
  nodes.source.addEventListener("input", () => sequence.push("input"));
  const input = new Promise((resolve) =>
    nodes.source.addEventListener("input", resolve, { once: true }),
  );
  nodes["open-markdown"].files = [new File(["# New source"], "new.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change"));
  await input;
  await tick();
  assert.deepEqual(sequence, ["revoke", "input"]);
  assert.equal(nodes.source.value, "# New source");
  assert.equal(nodes["source-filename"].value, "new.md");
  await assert.rejects(fetch(previous));
});
test("pagehide revokes downloads and bfcache reentry preserves untouched original file bytes", async (t) => {
  const { nodes, host } = await setup(t);
  const original = "\ufeff# Restored\r\nline\rend\n";
  const input = new Promise((resolve) =>
    nodes.source.addEventListener("input", resolve, { once: true }),
  );
  nodes["open-markdown"].files = [new File([original], "mixed.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change"));
  await input;
  await tick();
  nodes["prepare-markdown"].click();
  const old = nodes["source-download"].href;
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  assert.equal(nodes["source-download"].hidden, true);
  await assert.rejects(fetch(old));
  host.dispatchEvent(Object.assign(new Event("pageshow"), { persisted: true }));
  await tick();
  nodes["prepare-markdown"].click();
  const bytes = new Uint8Array(await (await fetch(nodes["source-download"].href)).arrayBuffer());
  assert.deepEqual(bytes, new TextEncoder().encode(original));
  assert.equal(nodes["remember-draft"].checked, false);
});
test("programmatic edit after prepare prevents stale source download activation", async (t) => {
  const { nodes } = await setup(t);
  nodes["prepare-markdown"].click();
  const url = nodes["source-download"].href;
  nodes.source.value = "changed without input";
  assert.equal(nodes["source-download"].click().defaultPrevented, true);
  assert.equal(nodes["source-download"].hidden, true);
  await assert.rejects(fetch(url));
});
test("live entrypoint replacements and keyboard undo invalidate downloads without a renderer", async (t) => {
  const { nodes } = await setup(t);
  nodes["prepare-markdown"].click();
  const url = nodes["source-download"].href;
  nodes["source-find"].value = "source";
  nodes["source-find"].dispatchEvent(new Event("input"));
  nodes["source-replacement"].value = "Markdown";
  nodes["source-replace-all"].click();
  assert.equal(nodes.source.value, "# Existing Markdown");
  assert.equal(nodes["source-download"].hidden, true);
  await assert.rejects(fetch(url));
  assert.equal(nodes["source-undo"].disabled, false);
  const key = Object.assign(new Event("keydown", { cancelable: true }), {
    key: "z",
    ctrlKey: true,
  });
  nodes.source.dispatchEvent(key);
  assert.equal(key.defaultPrevented, true);
  assert.equal(nodes.source.value, "# Existing source");
  nodes["source-redo"].click();
  assert.equal(nodes.source.value, "# Existing Markdown");
  const input = new Promise((resolve) =>
    nodes.source.addEventListener("input", resolve, { once: true }),
  );
  nodes["open-markdown"].files = [new File(["# Different document"], "different.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change"));
  await input;
  await tick();
  nodes["source-undo"].click();
  assert.equal(nodes.source.value, "# Different document");
  assert.equal(nodes["source-undo"].disabled, true);
  assert.equal(nodes["source-redo"].disabled, true);
});
test("bfcache restarts editing once with a fresh history, not stale document transactions", async (t) => {
  const { nodes, host } = await setup(t);
  nodes.source.value = "first edit";
  nodes.source.dispatchEvent(new Event("input"));
  assert.equal(nodes["source-undo"].disabled, false);
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  const key = Object.assign(new Event("keydown", { cancelable: true }), {
    key: "z",
    ctrlKey: true,
  });
  nodes.source.dispatchEvent(key);
  assert.equal(key.defaultPrevented, false);
  host.dispatchEvent(Object.assign(new Event("pageshow"), { persisted: true }));
  await tick();
  assert.equal(nodes["source-undo"].disabled, true);
  nodes.source.value = "second edit";
  nodes.source.dispatchEvent(new Event("input"));
  nodes["source-undo"].click();
  assert.equal(nodes.source.value, "first edit");
  assert.equal(nodes["source-undo"].disabled, true);
  nodes["source-redo"].click();
  assert.equal(nodes.source.value, "second edit");
});

test("formatting enters existing history and revokes a prepared source download", async (t) => {
  const { nodes } = await setup(t);
  nodes["prepare-markdown"].click();
  const url = nodes["source-download"].href;
  nodes.source.setSelectionRange(11, 17, "backward");
  const key = Object.assign(new Event("keydown", { cancelable: true }), { key: "b", ctrlKey: true });
  nodes.source.dispatchEvent(key);
  assert(key.defaultPrevented);
  assert.equal(nodes.source.value, "# Existing **source**");
  assert(nodes["source-download"].hidden);
  await assert.rejects(fetch(url));
  nodes["source-undo"].click();
  assert.equal(nodes.source.value, "# Existing source");
  assert.equal(nodes.source.selectionStart, 11);
  assert.equal(nodes.source.selectionEnd, 17);
  assert.equal(nodes.source.selectionDirection, "backward");
  assert(nodes["source-undo"].disabled);
});

test("source entry saves and reopens exact Markdown and publishing settings with no renderer or storage", async t => {
  const { nodes, host, doc } = await setup(t), original = "\ufeff# Saved\r\nTerm\r: definition\n";
  nodes["open-markdown"].files = [new File([original], "saved.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change")); await tick(); await tick();
  applySettings(doc, { profile: "commonmark-gfm", font: "serif", title: "Saved title", author: "Saved author",
    pageSize: "a5", marginTopPt: 36, marginBottomPt: 40, splitFootnotes: true,
    footnoteBacklinks: true, pdfLineHeight: 1.6, tableCellPaddingXEm: 0 });
  const saved = await readProject(nodes);
  assert.equal(saved.document.source, original);
  assert.equal(saved.document.filename, "saved.md");
  assert.equal(saved.settings.splitFootnotes, true);
  assert.equal(saved.settings.tableCellPaddingXEm, 0);
  assert.equal(saved.settings.profile, "commonmark-gfm");
  const bytes = await (await fetch(nodes["flow-project-download"].href)).arrayBuffer();
  nodes.source.value = "# Later edit"; nodes.source.dispatchEvent(new Event("input"));
  applySettings(doc, { profile: "gfm-plus", title: "Other title", pageSize: "letter" });
  let confirmations = 0;
  host.confirm = () => { confirmations++; return true; };
  const observed = [];
  nodes.source.addEventListener("fmd-document-replaced", () => {
    observed.push({ source: nodes.source.value, profile: doc.querySelector("#render-profile").value,
      title: doc.querySelector("#render-title").value });
  });
  await openProject(nodes, new File([bytes], "saved.fmdflow.json"));
  assert.equal(confirmations, 1);
  assert.deepEqual(observed, [{ source: original.replace(/\r\n?/g, "\n"), profile: "commonmark-gfm", title: "Saved title" }]);
  assert.equal(nodes["source-undo"].disabled, true);
  assert.deepEqual(await readProject(nodes), saved);
  nodes["prepare-markdown"].click();
  assert.deepEqual(new Uint8Array(await (await fetch(nodes["source-download"].href)).arrayBuffer()), new TextEncoder().encode(original));
  assert.match(nodes["draft-status"].textContent, /STORAGE_UNAVAILABLE/);
});

test("malformed project or refusal leaves source and the applied profile intact", async t => {
  const { nodes, host, doc } = await setup(t);
  applySettings(doc, { title: "Keep", profile: "gfm-plus", splitFootnotes: true });
  const before = await readProject(nodes); let approvals = 0;
  host.confirm = () => { approvals++; return false; };
  const invalid = JSON.parse(serializeFlowProject({ filename: "x.md", source: "# New" }, {}));
  invalid.settings.pageSize = "custom";
  await openProject(nodes, new File([JSON.stringify(invalid)], "invalid.fmdflow.json"));
  assert.equal(approvals, 0);
  assert.deepEqual(await readProject(nodes), before);
  await openProject(nodes, projectFile());
  assert.equal(approvals, 1);
  assert.deepEqual(await readProject(nodes), before);
});

test("editor changes during a project read retire the old replacement", async t => {
  const { nodes, host, doc } = await setup(t);
  applySettings(doc, { title: "Current", profile: "commonmark-gfm" });
  let release, approvals = 0;
  const wait = new Promise(resolve => { release = resolve; });
  const file = projectFile("# Imported", { title: "Wrong" });
  file.arrayBuffer = async () => { await wait; return File.prototype.arrayBuffer.call(file); };
  host.confirm = () => { approvals++; return true; };
  nodes["open-flow-project"].files = [file];
  nodes["open-flow-project"].dispatchEvent(new Event("change"));
  nodes.source.value = "# New local edit"; nodes.source.dispatchEvent(new Event("input"));
  release(); await tick(); await tick();
  assert.equal(approvals, 0);
  assert.equal(nodes.source.value, "# New local edit");
  assert.equal(doc.querySelector("#render-title").value, "Current");
  assert.equal(doc.querySelector("#render-profile").value, "commonmark-gfm");
});

test("settings change-and-change-back while confirmation is pending cannot replace the document", async t => {
  const { nodes, host, doc } = await setup(t);
  applySettings(doc, { title: "Original" });
  let approve, asked;
  const confirmed = new Promise(resolve => { asked = resolve; });
  host.confirm = () => { asked(); return new Promise(resolve => { approve = resolve; }); };
  nodes["open-flow-project"].files = [projectFile("# Imported", { title: "Imported" })];
  nodes["open-flow-project"].dispatchEvent(new Event("change")); await confirmed;
  const title = doc.querySelector("#render-title"), form = doc.querySelector("#render-settings-form");
  title.value = "Changed"; form.dispatchEvent(new Event("input"));
  title.value = "Original"; form.dispatchEvent(new Event("input"));
  approve(true); await tick(); await tick();
  assert.equal(nodes.source.value, "# Existing source");
  assert.equal((await readProject(nodes)).settings.title, "Original");
});

test("silent editor or settings changes are checked again before project download activation", async t => {
  const { nodes, doc } = await setup(t);
  for (const mutate of [() => { nodes.source.value = "Silent source"; },
    () => { doc.querySelector("#render-title").value = "Unapplied title"; }]) {
    await readProject(nodes);
    const previous = nodes["flow-project-download"].href;
    mutate();
    assert.equal(nodes["flow-project-download"].click().defaultPrevented, true);
    assert.equal(nodes["flow-project-download"].hidden, true);
    await assert.rejects(fetch(previous));
  }
});

test("unapplied or invalid settings block project saving while plain Markdown recovery stays available", async t => {
  const { nodes, doc } = await setup(t);
  doc.querySelector("#render-pageSize").value = "custom";
  doc.querySelector("#render-settings-form").dispatchEvent(new Event("input"));
  nodes["prepare-flow-project"].click();
  assert.equal(nodes["flow-project-download"].hidden, true);
  assert.match(nodes["flow-project-status"].textContent, /UNAPPLIED_SETTINGS/);
  nodes["prepare-markdown"].click();
  assert.equal(await (await fetch(nodes["source-download"].href)).text(), "# Existing source");
});

test("plain Markdown still resets document settings after a project restore", async t => {
  const { nodes, doc } = await setup(t);
  await openProject(nodes, projectFile("# Project", { profile: "commonmark-gfm", title: "Saved", font: "serif", splitFootnotes: true }));
  nodes["open-markdown"].files = [new File(["# Plain"], "plain.md")];
  nodes["open-markdown"].dispatchEvent(new Event("change")); await tick(); await tick();
  const current = await readProject(nodes);
  assert.equal(current.settings.profile, null);
  assert.equal(current.settings.title, "");
  assert.equal(current.settings.splitFootnotes, null);
  assert.equal(current.settings.font, "serif");
  assert.equal(doc.querySelector("#render-profile").value, "");
});

test("bfcache releases project URLs and resumes one source/settings owner with its applied profile", async t => {
  const { nodes, host, doc } = await setup(t);
  applySettings(doc, { title: "Retained", profile: "commonmark-gfm", splitFootnotes: true });
  const saved = await readProject(nodes), old = nodes["flow-project-download"].href;
  host.dispatchEvent(Object.assign(new Event("pagehide"), { persisted: true }));
  assert.equal(nodes["flow-project-download"].hidden, true);
  assert.equal(nodes["prepare-flow-project"].disabled, true);
  await assert.rejects(fetch(old));
  host.dispatchEvent(Object.assign(new Event("pageshow"), { persisted: true })); await tick();
  assert.deepEqual(await readProject(nodes), saved);
  assert.equal(doc.querySelector("#render-title").value, "Retained");
  assert.equal(getRenderSettingsBinding(nodes.source).exportOptions("pdf").splitFootnotes, true);
});

test("project source composition keeps the final input turn fenced", async t => {
  const { nodes } = await setup(t);
  nodes.source.dispatchEvent(new Event("compositionstart"));
  nodes["prepare-flow-project"].click(); assert.equal(nodes["flow-project-download"].hidden, true);
  nodes.source.dispatchEvent(new Event("compositionend"));
  nodes["prepare-flow-project"].click(); assert.equal(nodes["flow-project-download"].hidden, true);
  nodes.source.value = "# Final composed source"; nodes.source.dispatchEvent(new Event("input"));
  await new Promise(resolve => setTimeout(resolve, 5));
  assert.equal((await readProject(nodes)).document.source, "# Final composed source");
});

test("settings composition cannot be captured as a completed project", async t => {
  const { nodes, doc } = await setup(t), form = doc.querySelector("#render-settings-form");
  form.dispatchEvent(new Event("compositionstart"));
  nodes["prepare-flow-project"].click(); assert.equal(nodes["flow-project-download"].hidden, true);
  assert.match(nodes["flow-project-status"].textContent, /SETTINGS_BUSY/);
  form.dispatchEvent(new Event("compositionend"));
  nodes["prepare-flow-project"].click(); assert.equal(nodes["flow-project-download"].hidden, true);
  await new Promise(resolve => setTimeout(resolve, 5));
  await readProject(nodes);
});

test("source lock prevents project replacement but permits a complete project backup", async t => {
  const { nodes, host } = await setup(t); let approvals = 0;
  nodes.source.readOnly = true;
  host.confirm = () => { approvals++; return true; };
  await openProject(nodes, projectFile());
  assert.equal(approvals, 0);
  assert.equal(nodes.source.value, "# Existing source");
  assert.match(nodes["flow-project-status"].textContent, /EDITOR_BUSY/);
  assert.equal((await readProject(nodes)).document.source, "# Existing source");
});

test("preview observer failures never block project recovery or change the applied export profile", async t => {
  const { nodes, doc } = await setup(t), binding = getRenderSettingsBinding(nodes.source);
  const events = [], release = binding.subscribe(event => { events.push(event.kind); throw new Error("failed Canvas"); });
  t.after(release);
  applySettings(doc, { title: "Keep working", profile: "commonmark-gfm" });
  assert.equal(binding.exportOptions("pdf").title, "Keep working");
  assert.equal((await readProject(nodes)).settings.profile, "commonmark-gfm");
  await openProject(nodes, projectFile("# Restored without preview", { title: "Restored", profile: "gfm-plus" }));
  assert.equal(binding.exportOptions("epub").title, "Restored");
  assert.equal((await readProject(nodes)).document.source, "# Restored without preview");
  assert(events.includes("draft") && events.includes("applied"));
});


test("multiline project metadata stays exact through DOM normalization, unrelated edits and repeated saves", async t => {
  const { nodes, doc } = await setup(t), metadata = { title: "A\r\nB\rC\nD", author: "First\r\nLast" };
  await openProject(nodes, projectFile("# Multiline metadata", metadata));
  const title = doc.querySelector("#render-title"), author = doc.querySelector("#render-author");
  assert.equal(title.tagName, "TEXTAREA"); assert.equal(author.tagName, "TEXTAREA");
  assert.equal(title.value, metadata.title.replace(/\r\n?/g, "\n"));
  assert.equal(author.value, metadata.author.replace(/\r\n?/g, "\n"));
  const binding = getRenderSettingsBinding(nodes.source);
  assert.equal(binding.exportOptions("pdf").title, metadata.title);
  assert.equal(binding.exportOptions("epub").author, metadata.author);
  let saved = await readProject(nodes);
  assert.equal(saved.settings.title, metadata.title); assert.equal(saved.settings.author, metadata.author);
  applySettings(doc, { pageSize: "a5" });
  doc.querySelector("#render-settings-discard").click();
  saved = await readProject(nodes);
  assert.equal(saved.settings.title, metadata.title); assert.equal(saved.settings.author, metadata.author);
  applySettings(doc, { title: "Edited\r\nmetadata" });
  assert.equal((await readProject(nodes)).settings.title, "Edited\nmetadata");
  assert.equal(binding.exportOptions("pdf").author, metadata.author);
});
