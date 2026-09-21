// Production normalization and panel logic. DOM/session/renderers below are
// explicit doubles; Node's EventTarget, Blob and UTF-8 operations are real.

import assert from "node:assert/strict";
import test from "node:test";
import {
  createRenderSettingsControls,
  normalizeRenderSettings,
} from "../demo/flow_render_settings.mjs";
import { withFlowExports } from "../flow_export.mjs";
import { createFlowAdapter } from "../flow_session.mjs";

class Element extends EventTarget {
  constructor(tag, root) {
    super();
    this.tagName = tag.toUpperCase();
    this.root = root;
    this.children = [];
    this.style = {};
    this.attributes = {};
    this.value = "";
    this.disabled = false;
  }
  append(...children) {
    for (const node of children) {
      this.children.push(node);
      node.parentNode = this;
    }
  }
  insertBefore(node, anchor) {
    const index = this.children.indexOf(anchor);
    assert.ok(index >= 0);
    this.children.splice(index, 0, node);
    node.parentNode = this;
  }
  setAttribute(key, value) {
    this.attributes[key] = value;
  }
  querySelector(id) {
    if (id === `#${this.id}`) return this;
    for (const child of this.children) {
      const found = child.querySelector(id);
      if (found) return found;
    }
    return null;
  }
  click() {
    const event = new Event("click", { cancelable: true });
    this.dispatchEvent(event);
    return event;
  }
}
class Root extends Element {
  constructor() {
    super("document");
    this.root = this;
  }
  createElement(tag) {
    return new Element(tag, this);
  }
}
function fixture(options = {}) {
  const root = new Root(),
    sourceEditor = root.createElement("textarea"),
    anchor = root.createElement("section");
  sourceEditor.id = "source";
  sourceEditor.value = "# Original Markdown";
  sourceEditor.selectionStart = 2;
  sourceEditor.selectionEnd = 5;
  anchor.id = "export-controls";
  root.append(sourceEditor, anchor);
  const applied = [],
    invalidated = [];
  let inputEvents = 0;
  sourceEditor.addEventListener("input", () => inputEvents++);
  const config = {
    root,
    sourceEditor,
    onApply: (settings, change) => applied.push({ settings, change }),
    onInvalidate: () => invalidated.push(true),
    ...options,
  };
  let controls = createRenderSettingsControls(config);
  const el = (key) => root.querySelector(`#${key}`);
  return {
    root,
    sourceEditor,
    applied,
    invalidated,
    el,
    get controls() {
      return controls;
    },
    get inputEvents() {
      return inputEvents;
    },
    set(key, value, notify = true) {
      el(`render-${key}`).value = String(value);
      if (notify) el("render-settings-form").dispatchEvent(new Event("input"));
    },
    submit() {
      el("render-settings-form").dispatchEvent(new Event("submit", { cancelable: true }));
    },
    remount(initial) {
      controls.dispose();
      controls = createRenderSettingsControls({ ...config, initial });
    },
  };
}
test("defaults preserve worker metrics and omit unspecified renderer options", () => {
  const s = normalizeRenderSettings();
  assert.deepEqual(s.preview, { font: "sans", bodySize: 14, codeSize: 13, lineHeight: 20 });
  assert.deepEqual(s.html, { maxOutputBytes: 67108864, darkMode: "auto" });
  assert.deepEqual(s.pdf, { maxOutputBytes: 67108864, pageNumbers: true, metadataEpochSeconds: 0 });
  assert.equal(s.values.baseFontSize, null);
  assert.equal(s.values.toc, null);
  for (const value of [s, s.values, s.preview, s.html, s.pdf])
    assert.equal(Object.isFrozen(value), true);
});
test("all publishing options reach only the appropriate export format", () => {
  const s = normalizeRenderSettings({
    font: "serif",
    bodySize: 18,
    codeSize: 15,
    lineHeight: 27,
    title: "Title <literal>",
    author: "Émilie",
    lang: "fr-CA",
    toc: true,
    tocDepth: 4,
    pageNumbers: false,
    codeLineNumbers: true,
    baseFontSize: 13,
    headingScale: 1.4,
    tableFontSize: 10,
    fitToPages: 5,
    microtype: "protrusion",
    darkMode: "disabled",
  });
  assert.equal(s.pdf.author, "Émilie");
  assert.equal(s.pdf.fitToPages, 5);
  assert.equal(s.pdf.pageNumbers, false);
  assert.equal(s.pdf.codeLineNumbers, true);
  assert.equal(s.pdf.baseFontSize, 13);
  assert.equal(s.pdf.headingScale, 1.4);
  assert.equal(s.pdf.tableFontSize, 10);
  assert.equal(s.pdf.microtype, "protrusion");
  assert.equal(s.pdf.darkMode, undefined);
  assert.equal(s.html.author, undefined);
  assert.equal(s.html.baseFontSize, undefined);
  assert.equal(s.html.darkMode, "disabled");
  for (const options of [s.html, s.pdf]) {
    assert.equal(options.lang, "fr-CA");
    assert.equal(options.tocDepth, 4);
    assert.equal(options.toc, true);
    assert.equal(options.title, "Title <literal>");
    assert.equal(options.font, undefined);
    assert.equal(options.allowRawHtml, undefined);
  }
});
for (const [key, value] of [
  ["font", "url(font.ttf)"],
  ["bodySize", 7],
  ["bodySize", 49],
  ["codeSize", 41],
  ["lineHeight", 73],
  ["lineHeight", 10],
  ["title", null],
  ["lang", "English UK"],
  ["author", "\ud800"],
  ["toc", "false"],
  ["tocDepth", 2.5],
  ["baseFontSize", "12"],
  ["baseFontSize", ""],
  ["baseFontSize", NaN],
  ["baseFontSize", 25],
  ["headingScale", 1],
  ["tableFontSize", 4],
  ["fitToPages", 1001],
  ["microtype", "invalid"],
  ["darkMode", null],
  ["allowRawHtml", true],
  ["pdfImages", []],
  ["workerUrl", "https://example.invalid"],
]) {
  test(`rejects invalid or authority-bearing ${key}: ${String(value)}`, () => {
    assert.throws(() => normalizeRenderSettings({ [key]: value }));
  });
}
test("empty metadata and explicit null options preserve defaults; text is bounded", () => {
  const s = normalizeRenderSettings({
    pageNumbers: null,
    title: "",
    author: "",
    lang: "",
    microtype: null,
  });
  assert.equal(s.pdf.pageNumbers, undefined);
  assert.equal(s.pdf.author, undefined);
  assert.throws(() => normalizeRenderSettings({ title: "x".repeat(4097) }));
  assert.throws(() => normalizeRenderSettings({ lang: "a".repeat(65) }));
});
test("panel mounts next to exports with associated labels, status and no startup side effects", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  assert.equal(f.root.children[1].id, "render-settings");
  assert.equal(f.applied.length, 0);
  assert.equal(f.inputEvents, 0);
  assert.equal(f.el("render-settings-status").attributes.role, "status");
  assert.match(f.el("render-bodySize").attributes["aria-describedby"], /render-settings-status/);
  assert.equal(f.el("render-settings-apply").disabled, true);
  assert.equal(f.controls.dirty, false);
  assert.equal(f.sourceEditor.value, "# Original Markdown");
});
test("draft fields never change applied settings and block export until apply", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  const previous = f.controls.settings;
  f.set("font", "serif");
  f.set("bodySize", 18);
  f.set("lineHeight", 26);
  f.set("title", "New title");
  assert.equal(f.controls.settings, previous);
  assert.equal(f.applied.length, 0);
  assert.ok(f.invalidated.length > 0);
  assert.throws(() => f.controls.exportOptions("pdf"), { code: "UNAPPLIED_SETTINGS" });
  f.submit();
  assert.equal(f.applied.length, 1);
  assert.equal(f.applied[0].change.previewChanged, true);
  assert.equal(f.controls.settings.preview.font, "serif");
  assert.equal(f.controls.exportOptions("pdf").title, "New title");
  assert.equal(f.controls.dirty, false);
  assert.equal(f.inputEvents, 0);
  assert.equal(f.sourceEditor.value, "# Original Markdown");
  assert.equal(f.sourceEditor.selectionStart, 2);
  assert.equal(f.sourceEditor.selectionEnd, 5);
});
test("publishing-only apply does not request a typography change", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("author", "Author");
  f.submit();
  assert.equal(f.applied[0].change.previewChanged, false);
  assert.equal(f.controls.exportOptions("pdf").author, "Author");
  assert.equal(f.controls.exportOptions("html").author, undefined);
});
test("invalid form preserves all field text and previous applied settings", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  const previous = f.controls.settings;
  f.set("title", "Keep this draft");
  f.set("lineHeight", 9);
  f.submit();
  assert.equal(f.controls.settings, previous);
  assert.equal(f.applied.length, 0);
  assert.equal(f.el("render-title").value, "Keep this draft");
  assert.equal(f.el("render-lineHeight").value, "9");
  assert.match(
    f.el("render-settings-status").textContent,
    /Previous applied settings and source were retained/,
  );
  f.set("lineHeight", 24);
  f.submit();
  assert.equal(f.controls.settings.pdf.title, "Keep this draft");
});
test("unannounced raw field changes are checked again at export admission", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("baseFontSize", 15, false);
  assert.throws(() => f.controls.exportOptions("pdf"), { code: "UNAPPLIED_SETTINGS" });
  f.controls.apply();
  assert.equal(f.controls.exportOptions("pdf").baseFontSize, 15);
});
test("discard restores the applied form; reset deliberately restores defaults", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("title", "Saved setting");
  f.submit();
  f.set("title", "Draft title");
  f.el("render-settings-discard").click();
  assert.equal(f.controls.dirty, false);
  assert.equal(f.el("render-title").value, "Saved setting");
  assert.equal(f.applied.length, 1);
  f.el("render-settings-reset").click();
  assert.equal(f.applied.length, 2);
  assert.equal(f.controls.settings.pdf.title, undefined);
  f.set("author", "Unapplied");
  f.controls.reset();
  assert.equal(f.el("render-author").value, "");
  assert.equal(f.applied.length, 2);
});
test("preview presets preserve publishing fields and require Apply", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("author", "Draft author");
  f.el("render-preset-reading").click();
  assert.equal(f.controls.settings.preview.font, "sans");
  assert.equal(f.el("render-font").value, "serif");
  assert.equal(f.el("render-author").value, "Draft author");
  f.submit();
  assert.equal(f.controls.settings.preview.lineHeight, 27);
  f.el("render-preset-compact").click();
  f.submit();
  assert.equal(f.controls.settings.preview.bodySize, 12);
  assert.equal(f.controls.settings.pdf.author, "Draft author");
});
test("new document resets publishing metadata and draft fields but retains applied preview preferences", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("font", "serif");
  f.set("title", "Old title");
  f.set("lang", "de");
  f.submit();
  f.set("author", "Pending old author");
  f.sourceEditor.dispatchEvent(new Event("fmd-document-replaced"));
  assert.equal(f.controls.settings.preview.font, "serif");
  assert.equal(f.controls.settings.pdf.title, undefined);
  assert.equal(f.controls.settings.html.lang, undefined);
  assert.equal(f.el("render-author").value, "");
  assert.equal(f.controls.dirty, false);
  assert.equal(f.inputEvents, 0);
});
test("composition blocks apply and disposal removes listeners; remount re-enables inputs", () => {
  const f = fixture();
  f.set("title", "Draft");
  f.el("render-settings-form").dispatchEvent(new Event("compositionstart"));
  assert.throws(() => f.controls.apply(), { code: "SETTINGS_BUSY" });
  assert.equal(f.el("render-settings-apply").disabled, true);
  f.el("render-settings-form").dispatchEvent(new Event("compositionend"));
  f.submit();
  const saved = f.controls.settings.values,
    panel = f.el("render-settings");
  f.controls.dispose();
  f.submit();
  assert.equal(f.applied.length, 1);
  assert.equal(f.el("render-title").disabled, true);
  assert.throws(() => f.controls.exportOptions("pdf"), { code: "SESSION_DISPOSED" });
  f.remount(saved);
  assert.equal(f.el("render-settings"), panel);
  assert.equal(f.el("render-title").disabled, false);
  assert.equal(f.controls.exportOptions("pdf").title, "Draft");
  f.controls.dispose();
});
test("host rejection keeps the previous applied configuration and editable draft", (t) => {
  const f = fixture({
    onApply() {
      throw new Error("private host details");
    },
  });
  t.after(() => f.controls.dispose());
  f.set("title", "Draft");
  f.submit();
  assert.equal(f.controls.settings.pdf.title, undefined);
  assert.equal(f.el("render-title").value, "Draft");
  assert.doesNotMatch(f.el("render-settings-status").textContent, /private host/);
});
test("applied settings pass through real export and adapter validation without changing source or authority", async () => {
  const calls = [],
    source = "# Exact source\n";
  const raw = {
    revision: "1",
    layoutRevision: "1",
    source,
    free() {},
    snapshotJson(revision, layoutRevision, offset) {
      return JSON.stringify({
        schemaVersion: 1,
        revision,
        layoutRevision,
        offset,
        total: 0,
        items: [],
        nextOffset: null,
      });
    },
  };
  const adapter = createFlowAdapter(raw);
  const render = (format) => async (markdown, options) => {
    calls.push({ format, markdown, options });
    return {
      format,
      mimeType: format === "pdf" ? "application/pdf" : "text/html; charset=utf-8",
      bytes: new Uint8Array([1, 2]),
      diagnostics: [],
    };
  };
  const settings = normalizeRenderSettings({
    font: "serif",
    title: "Published",
    author: "Author",
    lang: "nl",
    baseFontSize: 13,
    toc: true,
  });
  const session = withFlowExports(
    adapter,
    { html: render("html"), pdf: render("pdf") },
    settings.preview.font,
  );
  for (const format of ["html", "pdf"]) {
    const output = await session.exportDocument(format, settings[format], session.token);
    assert.equal(output.font, "serif");
    assert.equal(output.sourceLengthBytes, new TextEncoder().encode(source).length);
  }
  for (const call of calls) {
    assert.equal(call.markdown, source);
    assert.equal(call.options.allowRawHtml, false);
    assert.deepEqual(call.options.pdfImages, []);
    assert.equal(call.options.font, "serif");
  }
  assert.equal(calls[0].options.author, undefined);
  assert.equal(calls[1].options.author, "Author");
  assert.equal(calls[1].options.baseFontSize, 13);
  assert.equal(session.source, source);
  session.dispose();
});

test("settings runtime and guide are included in both package assemblers", async () => {
  const { readFile } = await import("node:fs/promises");
  const manifest = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8"));
  for (const path of ["demo/flow_render_settings.mjs", "SETTINGS.md"]) {
    assert.equal(manifest.files.filter((value) => value === path).length, 1);
    assert.ok((await readFile(new URL(`../${path}`, import.meta.url), "utf8")).length > 0);
  }
  for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
    const script = await readFile(new URL(`../../scripts/${name}`, import.meta.url), "utf8");
    const loops = [...script.matchAll(/for file in ([\s\S]*?); do\n\s+cp "wasm\/(.*?)\$file"/g)];
    const copied = loops.flatMap(([, names, prefix]) =>
      names
        .replace(/\\\n/g, " ")
        .trim()
        .split(/\s+/)
        .map((value) => prefix + value),
    );
    for (const path of ["demo/flow_render_settings.mjs", "SETTINGS.md"])
      assert.ok(copied.includes(path), `${name}: ${path}`);
  }
});
