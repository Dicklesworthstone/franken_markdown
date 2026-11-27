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
for (const profile of ["commonmark-gfm", "gfm-plus"]) {
  test(`${profile} belongs to preview creation, with no per-export grammar override`, () => {
    const s = normalizeRenderSettings({ profile, font: "serif" });
    assert.equal(s.values.profile, profile);
    assert.equal(s.preview.profile, profile);
    assert.equal(s.preview.font, "serif");
    for (const format of ["html", "pdf", "epub", "svg"])
      assert.equal(Object.hasOwn(s[format], "profile"), false);
    assert.equal(normalizeRenderSettings({ profile: null }).values.profile, null);
    assert.equal(Object.hasOwn(normalizeRenderSettings({ profile: null }).preview, "profile"), false);
  });
}
test("profile settings reject accessors and inherited values without evaluating them", () => {
  let reads = 0;
  const accessor = { get profile() { reads++; return "gfm-plus"; } };
  for (const input of [accessor, Object.create(accessor)])
    assert.throws(() => normalizeRenderSettings(input), { code: "INVALID_OPTIONS" });
  assert.equal(reads, 0);
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
  assert.equal(s.html.author, "Émilie");
  assert.equal(s.epub.author, "Émilie");
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
test("all running text slots and rules are literal, frozen PDF-only settings", () => {
  const values = {
    headerLeft: "  {title} — Édition 😀  ", headerCenter: "{section}", headerRight: "{chapter}",
    footerLeft: "{author}\r\n{date}", footerCenter: "{page} / {pages}", footerRight: "{unknown}",
    headerRule: true, footerRule: false, skipFirstPage: true,
  };
  const settings = normalizeRenderSettings(values), defaults = normalizeRenderSettings();
  assert.deepEqual(settings.pdf.running, {
    header: { left: values.headerLeft, center: "{section}", right: "{chapter}", rule: true },
    footer: { left: values.footerLeft, center: "{page} / {pages}", right: "{unknown}", rule: false },
    skipFirstPage: true,
  });
  for (const options of [settings.pdf.running, settings.pdf.running.header, settings.pdf.running.footer])
    assert(Object.isFrozen(options));
  for (const format of ["preview", "html", "epub", "svg"])
    assert.deepEqual(settings[format], defaults[format]);
  assert.deepEqual(normalizeRenderSettings(JSON.parse(JSON.stringify(settings.values))), settings);
  values.headerLeft = "Changed caller input";
  assert.equal(settings.pdf.running.header.left, "  {title} — Édition 😀  ");
  assert.equal(Object.hasOwn(defaults.pdf, "running"), false);
});

test("running slots preserve native page-number precedence without synthesizing templates", () => {
  for (const pageNumbers of [true, false, null]) {
    for (const footerCenter of ["", " ", "{page} / {pages}", "{date}"]) {
      const settings = normalizeRenderSettings({ pageNumbers, footerCenter, headerLeft: "Title" });
      assert.equal(settings.pdf.pageNumbers, pageNumbers === null ? undefined : pageNumbers);
      assert.deepEqual(settings.pdf.running, { header: { left: "Title" },
        ...(footerCenter === "" ? {} : { footer: { center: footerCenter } }) });
    }
    assert.throws(() => normalizeRenderSettings({ pageNumbers, skipFirstPage: true }),
      { code: "INVALID_OPTIONS" });
  }
  assert.deepEqual(normalizeRenderSettings({ footerCenter: "{page}", skipFirstPage: true }).pdf.running,
    { footer: { center: "{page}" }, skipFirstPage: true });
  assert.deepEqual(normalizeRenderSettings({ headerRule: true, skipFirstPage: true }).pdf.running,
    { header: { rule: true }, skipFirstPage: true });
  assert.deepEqual(normalizeRenderSettings({ headerRule: false, footerRule: false, skipFirstPage: false }).pdf.running,
    { header: { rule: false }, footer: { rule: false }, skipFirstPage: false });
});

test("running templates use shared Unicode and aggregate limits and reject unsupported values", () => {
  const slots = ["headerLeft", "headerCenter", "headerRight", "footerLeft", "footerCenter", "footerRight"];
  for (const key of slots) {
    for (const value of [null, true, 4, [], {}, "x".repeat(4097), "😀".repeat(1025)])
      assert.throws(() => normalizeRenderSettings({ [key]: value }), { code: "INVALID_OPTIONS" }, key);
    assert.throws(() => normalizeRenderSettings({ [key]: "\ud800" }), { code: "INVALID_UNICODE" }, key);
    assert.equal(normalizeRenderSettings({ [key]: "😀".repeat(1024) }).values[key], "😀".repeat(1024));
  }
  for (const key of ["headerRule", "footerRule", "skipFirstPage"])
    for (const value of ["true", 0, {}, []])
      assert.throws(() => normalizeRenderSettings({ [key]: value }), { code: "INVALID_OPTIONS" }, key);
  const maximum = Object.fromEntries(slots.slice(0, 4).map(key => [key, "x".repeat(4096)]));
  assert.equal(normalizeRenderSettings(maximum).pdf.running.footer.left.length, 4096);
  assert.throws(() => normalizeRenderSettings({ ...maximum, footerCenter: "x" }), { code: "BUDGET_EXCEEDED" });
  for (const input of [{ running: { header: { left: "Hidden nested option" } } },
    { headerImage: { dest: "logo.png" } }, { skipFirstPage: true, footerRule: false }])
    assert.throws(() => normalizeRenderSettings(input), { code: "INVALID_OPTIONS" });
});

test("running controls apply atomically, invalidate publication, and leave preview and source intact", t => {
  const f = fixture(); t.after(() => f.controls.dispose());
  const before = f.controls.settings;
  const slots = { headerLeft: "{title}", headerCenter: "{section}", headerRight: "{chapter}",
    footerLeft: "{author}", footerCenter: "{page} / {pages}", footerRight: "Édition 😀" };
  for (const [key, value] of Object.entries(slots)) {
    const field = f.el(`render-${key}`);
    assert.equal(field.tagName, "TEXTAREA");
    assert.equal(field.maxLength, 4096);
    assert.equal(field.parentNode.children[0].attributes.for, field.id);
    assert.match(field.attributes["aria-describedby"], /render-running-help/);
    f.set(key, value);
  }
  for (const [key, value] of [["headerRule", "true"], ["footerRule", "false"], ["skipFirstPage", "true"]]) {
    assert.deepEqual(f.el(`render-${key}`).children.map(option => option.value), ["", "true", "false"]);
    f.set(key, value);
  }
  assert.equal(f.controls.settings, before);
  assert.equal(f.invalidated.length, 9);
  assert.throws(() => f.controls.exportOptions("pdf"), { code: "UNAPPLIED_SETTINGS" });
  assert.throws(() => f.controls.capture(), { code: "UNAPPLIED_SETTINGS" });
  f.set("footerRight", "😀".repeat(1025)); f.submit();
  assert.equal(f.controls.settings, before);
  assert.equal(f.applied.length, 0);
  assert.equal(f.el("render-footerRight").value, "😀".repeat(1025));
  assert.equal(f.el("render-headerLeft").value, "{title}");
  assert.match(f.el("render-settings-status").textContent, /4096 UTF-8 bytes/);
  f.set("footerRight", slots.footerRight); f.submit();
  assert.equal(f.applied.length, 1);
  assert.equal(f.applied[0].change.previewChanged, false);
  assert.deepEqual(f.controls.exportOptions("pdf").running, {
    header: { left: "{title}", center: "{section}", right: "{chapter}", rule: true },
    footer: { left: "{author}", center: "{page} / {pages}", right: "Édition 😀", rule: false },
    skipFirstPage: true,
  });
  assert.equal(f.controls.capture().headerLeft, "{title}");
  assert.equal(f.controls.dirty, false);
  assert.equal(f.sourceEditor.value, "# Original Markdown");
  assert.equal(f.sourceEditor.selectionStart, 2); assert.equal(f.sourceEditor.selectionEnd, 5);
  assert.equal(f.inputEvents, 0);
});

test("running settings survive remount and restore, discard drafts, and reset at document boundaries", t => {
  const f = fixture(); t.after(() => f.controls.dispose());
  const imported = { font: "serif", headerLeft: "{title}", footerCenter: "{page}",
    headerRule: true, footerRule: false, skipFirstPage: true };
  f.controls.restore(imported);
  const captured = JSON.parse(JSON.stringify(f.controls.capture()));
  const options = f.controls.exportOptions("pdf");
  f.remount(captured);
  assert.deepEqual(f.controls.exportOptions("pdf"), options);
  f.set("headerLeft", "Unapplied");
  f.el("render-settings-discard").click();
  assert.equal(f.el("render-headerLeft").value, "{title}");
  assert.equal(f.controls.dirty, false);
  f.set("headerLeft", "Another draft");
  f.controls.restore(captured);
  assert.equal(f.el("render-headerLeft").value, "{title}");
  assert.equal(f.controls.dirty, false);
  f.controls.reset();
  assert.equal(Object.hasOwn(f.controls.exportOptions("pdf"), "running"), false);
  f.controls.restore(imported);
  f.set("footerRight", "Old document's draft");
  f.sourceEditor.dispatchEvent(new Event("fmd-document-replaced"));
  assert.equal(f.controls.settings.preview.font, "serif");
  assert.equal(Object.hasOwn(f.controls.exportOptions("pdf"), "running"), false);
  for (const key of ["headerLeft", "headerCenter", "headerRight", "footerLeft", "footerCenter", "footerRight",
    "headerRule", "footerRule", "skipFirstPage"])
    assert.equal(f.el(`render-${key}`).value, "");
  assert.equal(f.controls.dirty, false);
  assert.equal(f.sourceEditor.value, "# Original Markdown"); assert.equal(f.inputEvents, 0);
});

test("unchanged textarea views retain exact imported running CRLF while edited templates use the view", t => {
  const f = fixture(); t.after(() => f.controls.dispose());
  const slots = ["headerLeft", "headerCenter", "headerRight", "footerLeft", "footerCenter", "footerRight"];
  // Real textarea values normalize CR and CRLF on assignment. Model that
  // browser boundary while exercising production restore/write/draft logic.
  for (const key of slots) {
    let view = "";
    Object.defineProperty(f.el(`render-${key}`), "value", {
      get: () => view, set: value => { view = String(value).replace(/\r\n?/g, "\n"); },
    });
  }
  const imported = Object.fromEntries(slots.map(key => [key, `${key}\r\n{page}\rEnd`]));
  f.controls.restore(imported);
  assert.equal(f.controls.dirty, false);
  for (const key of slots) assert.equal(f.el(`render-${key}`).value, `${key}\n{page}\nEnd`);
  f.set("headerRule", "true"); f.submit();
  for (const key of slots) assert.equal(f.controls.capture()[key], imported[key]);
  assert.equal(f.controls.exportOptions("pdf").running.footer.left, imported.footerLeft);
  f.set("headerLeft", "Edited\r\n{page}"); f.submit();
  assert.equal(f.controls.capture().headerLeft, "Edited\n{page}");
  assert.equal(f.controls.exportOptions("pdf").running.footer.left, imported.footerLeft);
});

for (const [key, value] of [
  ["profile", ""],
  ["profile", "gfm"],
  ["profile", " commonmark-gfm "],
  ["profile", true],
  ["profile", {}],
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
test("dialect selector stages, applies, discards and resets grammar without changing Markdown", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  assert.deepEqual(f.el("render-profile").children.map(option => option.value),
    ["", "commonmark-gfm", "gfm-plus"]);
  assert.equal(f.el("render-profile").value, "");
  assert.match(f.el("render-profile").attributes["aria-describedby"], /render-settings-help/);
  f.set("profile", "commonmark-gfm");
  assert.equal(f.controls.settings.values.profile, null);
  assert.throws(() => f.controls.exportOptions("html"), { code: "UNAPPLIED_SETTINGS" });
  f.submit();
  assert.equal(f.applied.at(-1).change.previewChanged, true);
  assert.equal(f.controls.settings.preview.profile, "commonmark-gfm");
  f.set("profile", "gfm-plus");
  f.el("render-settings-discard").click();
  assert.equal(f.el("render-profile").value, "commonmark-gfm");
  f.set("profile", "");
  f.submit();
  assert.equal(f.applied.at(-1).change.previewChanged, true, "removing a profile must rebuild the preview");
  assert.equal(Object.hasOwn(f.controls.settings.preview, "profile"), false);
  f.set("profile", "gfm-plus");
  f.submit();
  f.controls.reset();
  assert.equal(f.applied.at(-1).change.previewChanged, true);
  assert.equal(f.controls.settings.values.profile, null);
  assert.equal(f.el("render-profile").value, "");
  assert.equal(f.sourceEditor.value, "# Original Markdown");
  assert.equal(f.sourceEditor.selectionStart, 2);
  assert.equal(f.sourceEditor.selectionEnd, 5);
  assert.equal(f.inputEvents, 0);
});
test("preview presets and same-document remount retain the applied dialect", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("profile", "commonmark-gfm");
  f.submit();
  f.el("render-preset-reading").click();
  assert.equal(f.el("render-profile").value, "commonmark-gfm");
  f.submit();
  const initial = f.controls.settings.values;
  f.remount(initial);
  assert.equal(f.el("render-profile").disabled, false);
  assert.equal(f.controls.settings.preview.profile, "commonmark-gfm");
  assert.equal(f.controls.settings.preview.font, "serif");
  assert.equal(f.controls.dirty, false);
});
test("opening plain Markdown resets the prior dialect and draft while preserving typography", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("profile", "commonmark-gfm");
  f.set("font", "serif");
  f.set("bodySize", 18);
  f.set("lineHeight", 27);
  f.submit();
  f.set("profile", "gfm-plus");
  f.sourceEditor.dispatchEvent(new Event("fmd-document-replaced"));
  assert.deepEqual(f.controls.settings.preview, { font: "serif", bodySize: 18, codeSize: 13, lineHeight: 27 });
  assert.equal(f.controls.settings.values.profile, null);
  assert.equal(f.el("render-profile").value, "");
  assert.equal(f.applied.at(-1).change.previewChanged, true);
  assert.equal(f.controls.dirty, false);
  assert.equal(f.inputEvents, 0);
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
  assert.equal(f.controls.exportOptions("html").author, "Author");
  assert.equal(f.controls.exportOptions("epub").author, "Author");
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
  f.set("pageSize", "a4");
  f.set("marginLeftPt", 36);
  f.set("baselineGridPt", 12);
  f.set("optimalPagination", "true");
  f.submit();
  f.set("author", "Pending old author");
  f.sourceEditor.dispatchEvent(new Event("fmd-document-replaced"));
  assert.equal(f.controls.settings.preview.font, "serif");
  assert.equal(f.controls.settings.pdf.title, undefined);
  assert.equal(f.controls.settings.html.lang, undefined);
  assert.equal(f.controls.settings.pdf.page, undefined);
  assert.equal(f.controls.settings.pdf.baselineGridPt, undefined);
  assert.equal(f.controls.settings.pdf.optimalPagination, undefined);
  for (const key of ["pageSize", "marginLeftPt", "baselineGridPt", "optimalPagination"])
    assert.equal(f.el(`render-${key}`).value, "");
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
    pageSize: "a4",
    pageOrientation: "landscape",
    marginLeftPt: 36,
    baselineGridPt: 12,
    strictPagination: true,
    optimalPagination: true,
    footnoteBacklinks: true,
    splitFootnotes: false,
    typographyHomogeneous: true,
    typographyAntiriver: false,
    typographyPareto: true,
    typographyThemeSpacing: true,
    pdfLineHeight: 1.85,
    tableCellPaddingXEm: 0,
    tableCellPaddingYEm: 1.25,
    microtype: "expansion",
    headerLeft: "{title}", headerRight: "{section}", footerCenter: "{page} / {pages}",
    headerRule: true, footerRule: false, skipFirstPage: true,
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
  assert.equal(calls[0].options.author, "Author");
  assert.equal(calls[1].options.author, "Author");
  assert.equal(calls[1].options.baseFontSize, 13);
  assert.equal(calls[0].options.page, undefined);
  assert.equal(calls[0].options.typography, undefined);
  assert.equal(calls[0].options.running, undefined);
  assert.deepEqual(calls[1].options.running, {
    header: { left: "{title}", right: "{section}", rule: true },
    footer: { center: "{page} / {pages}", rule: false }, skipFirstPage: true,
  });
  assert.deepEqual(calls[1].options.page, {
    size: { widthPt: 297 * 72 / 25.4, heightPt: 210 * 72 / 25.4 },
    margins: { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 36 },
  });
  assert.equal(calls[1].options.baselineGridPt, 12);
  assert.equal(calls[1].options.strictPagination, true);
  assert.equal(calls[1].options.optimalPagination, true);
  assert.equal(calls[1].options.footnoteBacklinks, true);
  assert.equal(calls[1].options.splitFootnotes, false);
  assert.equal(calls[1].options.microtype, "expansion");
  assert.deepEqual(calls[1].options.typography, {
    homogeneous: true, antiriver: false, pareto: true, themeSpacing: true,
    lineHeight: 1.85, tableCellPaddingXEm: 0, tableCellPaddingYEm: 1.25,
  });
  assert.equal(session.source, source);
  session.dispose();
});

test("long-document PDF controls preserve other formats and carry exact spacing", () => {
  const defaults = normalizeRenderSettings();
  const settings = normalizeRenderSettings({
    footnoteBacklinks: true, splitFootnotes: true, baselineGridPt: 12,
    optimalPagination: false, strictPagination: false,
    pdfLineHeight: 2, tableCellPaddingXEm: 0, tableCellPaddingYEm: 0.75,
  });
  assert.equal(settings.pdf.footnoteBacklinks, true);
  assert.equal(settings.pdf.splitFootnotes, true);
  assert.equal(settings.pdf.baselineGridPt, 12);
  assert.deepEqual(settings.pdf.typography, {
    lineHeight: 2, tableCellPaddingXEm: 0, tableCellPaddingYEm: 0.75,
  });
  for (const name of ["preview", "html", "epub", "svg"])
    assert.deepEqual(settings[name], defaults[name]);
  assert.deepEqual(normalizeRenderSettings(settings.values), settings);
  assert.ok(Object.isFrozen(settings.pdf.typography));
  const disabled = normalizeRenderSettings({
    footnoteBacklinks: false, splitFootnotes: false, typographyThemeSpacing: false,
  });
  assert.equal(disabled.pdf.footnoteBacklinks, false);
  assert.equal(disabled.pdf.splitFootnotes, false);
  assert.deepEqual(disabled.pdf.typography, { themeSpacing: false });
});

for (const values of [
  { footnoteBacklinks: "true" }, { splitFootnotes: 1 },
  { typographyThemeSpacing: "true" }, { pdfLineHeight: "2" },
  { pdfLineHeight: 0.99 }, { pdfLineHeight: 4.01 }, { pdfLineHeight: NaN },
  { tableCellPaddingXEm: -0.01 }, { tableCellPaddingYEm: 8.01 },
  { tableCellPaddingYEm: Infinity },
  { typographyThemeSpacing: false, pdfLineHeight: 2 },
  { typographyThemeSpacing: false, tableCellPaddingXEm: 0 },
]) {
  test(`long-document settings reject ${JSON.stringify(values)} before publication`, () => {
    assert.throws(() => normalizeRenderSettings(values), { code: "INVALID_OPTIONS" });
  });
}

test("long-document drafts apply atomically, survive remount and reset", (t) => {
  const f = fixture(); t.after(() => f.controls.dispose());
  const before = f.controls.settings;
  f.set("footnoteBacklinks", "true"); f.set("splitFootnotes", "true");
  f.set("strictPagination", "true"); f.set("pdfLineHeight", "2.1");
  f.set("baselineGridPt", "12"); f.set("fitToPages", "2"); f.submit();
  assert.equal(f.controls.settings, before);
  assert.match(f.el("render-settings-status").textContent, /baseline grids.*fitToPages/i);
  assert.equal(f.el("render-pdfLineHeight").value, "2.1");
  assert.equal(f.el("render-fitToPages").value, "2");
  assert.equal(f.applied.length, 0);
  assert.throws(() => f.controls.exportOptions("pdf"), /Apply or discard/);
  f.set("fitToPages", "");
  f.set("tableCellPaddingYEm", "0"); f.submit();
  assert.equal(f.applied.length, 1);
  assert.equal(f.applied[0].change.previewChanged, false);
  assert.equal(f.controls.exportOptions("pdf").splitFootnotes, true);
  assert.equal(f.controls.exportOptions("pdf").strictPagination, true);
  assert.equal(f.controls.exportOptions("pdf").baselineGridPt, 12);
  assert.equal(f.controls.exportOptions("pdf").typography.lineHeight, 2.1);
  assert.equal(f.controls.exportOptions("pdf").typography.tableCellPaddingYEm, 0);
  f.remount(f.controls.settings.values);
  assert.equal(f.el("render-footnoteBacklinks").value, "true");
  assert.equal(f.el("render-splitFootnotes").value, "true");
  assert.equal(f.el("render-strictPagination").value, "true");
  assert.equal(f.el("render-baselineGridPt").value, "12");
  assert.equal(f.el("render-tableCellPaddingYEm").value, "0");
  f.el("render-settings-reset").click();
  assert.equal(f.controls.exportOptions("pdf").splitFootnotes, undefined);
  assert.equal(f.controls.exportOptions("pdf").typography, undefined);
  assert.equal(f.sourceEditor.value, "# Original Markdown"); assert.equal(f.inputEvents, 0);
});

test("paper and independent margins configure PDF without changing the flowing preview or HTML", () => {
  const original = normalizeRenderSettings();
  const settings = normalizeRenderSettings({
    pageSize: "a4", pageOrientation: "landscape", marginTopPt: 36,
    marginRightPt: 24.25, marginBottomPt: 48, marginLeftPt: 0,
  });
  assert.deepEqual(settings.preview, original.preview);
  assert.deepEqual(settings.html, original.html);
  assert.deepEqual(settings.pdf.page, {
    size: { widthPt: 297 * 72 / 25.4, heightPt: 210 * 72 / 25.4 },
    margins: { topPt: 36, rightPt: 24.25, bottomPt: 48, leftPt: 0 },
  });
  for (const value of [settings.pdf.page, settings.pdf.page.size, settings.pdf.page.margins])
    assert.equal(Object.isFrozen(value), true);
  assert.deepEqual(normalizeRenderSettings(settings.values), settings);
});

test("custom paper preserves dimensions and orientation while optional margins use the shared defaults", () => {
  const preserved = normalizeRenderSettings({ pageSize: "custom", pageWidthPt: 720.125, pageHeightPt: 504.25 });
  assert.deepEqual(preserved.pdf.page.size, { widthPt: 720.125, heightPt: 504.25 });
  const portrait = normalizeRenderSettings({ ...preserved.values, pageOrientation: "portrait", marginLeftPt: 20 });
  assert.deepEqual(portrait.pdf.page.size, { widthPt: 504.25, heightPt: 720.125 });
  assert.deepEqual(portrait.pdf.page.margins, { topPt: 72, rightPt: 72, bottomPt: 72, leftPt: 20 });
  const marginOnly = normalizeRenderSettings({ marginTopPt: 0 });
  assert.deepEqual(marginOnly.pdf.page.size, { widthPt: 612, heightPt: 792 });
  assert.equal(marginOnly.pdf.page.margins.topPt, 0);
});

for (const [name, settings] of [
  ["incomplete custom dimensions", { pageSize: "custom", pageWidthPt: 600 }],
  ["dimensions with named paper", { pageSize: "a4", pageWidthPt: 600, pageHeightPt: 900 }],
  ["dimensions without custom paper", { pageWidthPt: 600 }],
  ["unknown paper", { pageSize: "a3" }],
  ["invalid orientation", { pageOrientation: "sideways" }],
  ["non-numeric margin", { marginLeftPt: "36" }],
  ["infinite margin", { marginBottomPt: Infinity }],
  ["negative margin", { marginTopPt: -1 }],
  ["horizontal content too small", { pageSize: "letter", marginLeftPt: 300, marginRightPt: 241 }],
  ["vertical content too small", { pageSize: "a5", marginTopPt: 500 }],
  ["undersized custom paper", { pageSize: "custom", pageWidthPt: 143, pageHeightPt: 600 }],
  ["oversized custom paper", { pageSize: "custom", pageWidthPt: 14401, pageHeightPt: 600 }],
]) {
  test(`page setup rejects ${name} before export`, () => {
    assert.throws(() => normalizeRenderSettings(settings));
  });
}

test("typography controls preserve defaults, explicit off, expansion and fixed-grid validation", () => {
  const settings = normalizeRenderSettings({
    baselineGridPt: 14.125, optimalPagination: true, strictPagination: true,
    typographyHomogeneous: true, typographyAntiriver: true, typographyPareto: false,
    microtype: "expansion",
  });
  assert.equal(settings.pdf.baselineGridPt, 14.125);
  assert.equal(settings.pdf.optimalPagination, true);
  assert.equal(settings.pdf.strictPagination, true);
  assert.equal(settings.pdf.microtype, "expansion");
  assert.deepEqual(settings.pdf.typography, { homogeneous: true, antiriver: true, pareto: false });
  assert.deepEqual(settings.html, normalizeRenderSettings().html);
  assert.throws(() => normalizeRenderSettings({ ...settings.values, fitToPages: 2 }), /baseline grids/);
  assert.throws(() => normalizeRenderSettings({ baselineGridPt: 0.5 }));
  assert.throws(() => normalizeRenderSettings({ strictPagination: "true" }));
  assert.throws(() => normalizeRenderSettings({ typographyPareto: 1 }));
  const disabled = normalizeRenderSettings({ baselineGridPt: 0, fitToPages: 2, strictPagination: false });
  assert.equal(disabled.pdf.baselineGridPt, 0);
  assert.equal(disabled.pdf.strictPagination, false);
});

test("page controls apply atomically, retain invalid drafts and reset without changing source", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("pageSize", "a5");
  f.set("pageOrientation", "landscape");
  f.set("marginTopPt", 36);
  f.set("baselineGridPt", 12);
  f.set("strictPagination", "true");
  f.set("typographyAntiriver", "true");
  f.submit();
  assert.equal(f.applied.length, 1);
  assert.equal(f.applied[0].change.previewChanged, false);
  assert.equal(f.controls.exportOptions("pdf").page.margins.topPt, 36);
  assert.equal(f.controls.exportOptions("pdf").strictPagination, true);
  const applied = f.controls.settings;
  f.set("fitToPages", 2);
  f.submit();
  assert.equal(f.controls.settings, applied);
  assert.equal(f.el("render-fitToPages").value, "2");
  assert.equal(f.applied.length, 1);
  assert.throws(() => f.controls.exportOptions("pdf"), /Apply or discard/);
  f.el("render-settings-discard").click();
  assert.equal(f.el("render-fitToPages").value, "");
  assert.equal(f.el("render-pageSize").value, "a5");
  f.el("render-settings-reset").click();
  for (const key of ["pageSize", "pageOrientation", "marginTopPt", "baselineGridPt", "strictPagination", "typographyAntiriver"])
    assert.equal(f.el(`render-${key}`).value, "");
  assert.equal(f.controls.exportOptions("pdf").page, undefined);
  assert.equal(f.controls.exportOptions("pdf").typography, undefined);
  assert.equal(f.sourceEditor.value, "# Original Markdown");
  assert.equal(f.inputEvents, 0);
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


test("four-format settings preserve defaults and route only supported publication fields", () => {
  const defaults = normalizeRenderSettings();
  assert.deepEqual(defaults.epub, { maxOutputBytes: 64 * 1024 * 1024, darkMode: "auto" });
  assert.deepEqual(defaults.svg, { maxOutputBytes: 64 * 1024 * 1024, maxWidthPt: 612 });
  const settings = normalizeRenderSettings({
    title: "Collected essays", lang: "de-DE", toc: true, tocDepth: 3,
    darkMode: "disabled", svgWidthPt: 960.5, author: "Publication author", pageSize: "a5",
  });
  assert.deepEqual(settings.epub, {
    maxOutputBytes: 64 * 1024 * 1024, title: "Collected essays", lang: "de-DE",
    toc: true, tocDepth: 3, darkMode: "disabled", author: "Publication author",
  });
  assert.deepEqual(settings.svg, {
    maxOutputBytes: 64 * 1024 * 1024, maxWidthPt: 960.5, toc: true, tocDepth: 3,
  });
  assert.equal(settings.html.maxWidthPt, undefined);
  assert.equal(settings.pdf.maxWidthPt, undefined);
  assert.equal(settings.pdf.darkMode, undefined);
  assert.equal(settings.epub.author, "Publication author");
  assert.equal(settings.epub.page, undefined);
  assert.equal(Object.isFrozen(settings.epub), true);
  assert.equal(Object.isFrozen(settings.svg), true);
  assert.deepEqual(normalizeRenderSettings(settings.values), settings);
});

for (const width of [143, 14401, NaN, Infinity, -Infinity, "900", false]) {
  test("SVG publication width rejects " + String(width), () => {
    assert.throws(() => normalizeRenderSettings({ svgWidthPt: width }), { code: "INVALID_OPTIONS" });
  });
}
test("SVG publication width accepts native bounds without changing preview or PDF", () => {
  const defaults = normalizeRenderSettings();
  for (const width of [144, 612, 14400]) {
    const settings = normalizeRenderSettings({ svgWidthPt: width });
    assert.equal(settings.svg.maxWidthPt, width);
    assert.deepEqual(settings.preview, defaults.preview);
    assert.deepEqual(settings.pdf, defaults.pdf);
    assert.deepEqual(settings.html, defaults.html);
  }
});

test("EPUB and SVG settings apply, remount, discard and reset with the current document", (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("title", "Edition");
  f.set("lang", "fr");
  f.set("toc", "true");
  f.set("tocDepth", "4");
  f.set("darkMode", "disabled");
  f.set("svgWidthPt", "900");
  assert.throws(() => f.controls.exportOptions("epub"), { code: "UNAPPLIED_SETTINGS" });
  assert.throws(() => f.controls.exportOptions("svg"), { code: "UNAPPLIED_SETTINGS" });
  f.submit();
  assert.equal(f.controls.exportOptions("epub").title, "Edition");
  assert.equal(f.controls.exportOptions("epub").darkMode, "disabled");
  assert.equal(f.controls.exportOptions("svg").maxWidthPt, 900);
  const saved = f.controls.settings.values;
  f.remount(saved);
  assert.equal(f.el("render-svgWidthPt").value, "900");
  assert.equal(f.controls.exportOptions("svg").tocDepth, 4);
  f.set("svgWidthPt", "1200");
  f.el("render-settings-discard").click();
  assert.equal(f.el("render-svgWidthPt").value, "900");
  f.sourceEditor.dispatchEvent(new Event("fmd-document-replaced", { cancelable: true }));
  assert.equal(f.el("render-svgWidthPt").value, "");
  assert.equal(f.controls.exportOptions("svg").maxWidthPt, 612);
  assert.equal(f.controls.exportOptions("epub").title, undefined);
  f.set("svgWidthPt", "1440");
  f.submit();
  f.el("render-settings-reset").click();
  assert.equal(f.controls.exportOptions("svg").maxWidthPt, 612);
  assert.equal(f.sourceEditor.value, "# Original Markdown");
  assert.equal(f.inputEvents, 0);
});

test("publication settings composition invalidates output and owns the final input turn", async (t) => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  const form = f.el("render-settings-form");
  form.dispatchEvent(new Event("compositionstart"));
  assert.equal(f.invalidated.length, 1);
  for (const format of ["html", "pdf", "epub", "svg"])
    assert.throws(() => f.controls.exportOptions(format), { code: "SETTINGS_BUSY" });
  form.dispatchEvent(new Event("compositionend"));
  for (const format of ["html", "pdf", "epub", "svg"])
    assert.throws(() => f.controls.exportOptions(format), { code: "SETTINGS_BUSY" });
  f.set("title", "Final title"); // Browser ordering may deliver this after compositionend.
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.throws(() => f.controls.exportOptions("epub"), { code: "UNAPPLIED_SETTINGS" });
  f.submit();
  assert.equal(f.controls.exportOptions("epub").title, "Final title");
  assert.equal(f.controls.exportOptions("svg").maxWidthPt, 612);
  assert.equal(f.inputEvents, 0);
});

test("publication settings cannot export reentrantly while an apply callback owns admission", () => {
  let controls, refused = 0;
  const f = fixture({ onApply() {
    for (const format of ["html", "pdf", "epub", "svg"]) {
      assert.throws(() => controls.exportOptions(format), { code: "SETTINGS_BUSY" });
      refused++;
    }
  } });
  controls = f.controls;
  f.set("svgWidthPt", "900");
  f.submit();
  assert.equal(refused, 4);
  assert.equal(controls.exportOptions("svg").maxWidthPt, 900);
  controls.dispose();
});

test("archival publishing selection applies independently of preview and resets on source replacement", t => {
  const f = fixture();
  t.after(() => f.controls.dispose());
  f.set("pdfPolicy", "2b-strict");
  assert.throws(() => f.controls.exportOptions("pdf"), { code: "UNAPPLIED_SETTINGS" });
  f.submit();
  assert.equal(f.applied.at(-1).change.previewChanged, false);
  assert.equal(f.controls.exportOptions("pdf").pdfAStrict, true);
  for (const format of ["epub", "svg", "html"]) assert.equal("pdfA" in f.controls.exportOptions(format), false);
  f.sourceEditor.dispatchEvent(new Event("fmd-document-replaced"));
  assert.equal(f.el("render-pdfPolicy").value, "off");
  assert.equal("pdfA" in f.controls.exportOptions("pdf"), false);
});
