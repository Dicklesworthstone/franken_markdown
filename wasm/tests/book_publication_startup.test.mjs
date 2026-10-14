import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

// Execute the complete application orchestration body with explicit factory and
// window adapters. This checks wiring/ownership, not normal ESM package startup.
const source = readFileSync(new URL("../demo/book.js", import.meta.url), "utf8");
const body = source.replace(/^import\s+[\s\S]*?from\s+"[^"\n]+";\n/gm, "");
function setup({ fail = false } = {}) {
  const events = new Map(), calls = [], nodes = new Map();
  const document = { querySelector(selector) {
    if (!nodes.has(selector)) nodes.set(selector, { textContent: "" });
    return nodes.get(selector);
  } };
  const collection = {}, controls = {
    suspend() { calls.push("host.suspend"); }, dispose() { calls.push("host.dispose"); },
  };
  const inert = () => ({ suspend() {}, resume() {}, dispose() {}, detach() {} });
  const globals = {
    document, window: { confirm: () => true, addEventListener(type, fn) { events.set(type, fn); } },
    createBookCollection: () => collection,
    createBookControls: () => controls,
    createBookWorker: () => (calls.push("worker"), inert()),
    createBookPublicationControls(input) {
      calls.push("profile.create");
      assert.equal(input.root, document); assert.equal(input.controls, controls); assert.equal(input.collection, collection);
      assert.equal(Object.hasOwn(input, "worker"), false, "profile authoring owns no worker");
      if (fail) throw Error("fixture authoring initialization failure");
      return { suspend() { calls.push("profile.suspend"); }, resume() { calls.push("profile.resume"); },
        dispose() { calls.push("profile.dispose"); } };
    },
  };
  for (const name of ["createBookImageControls", "createBookFontControls", "createBookLibraryControls", "createBookPdfControls",
    "createBookPreviewControls", "createBookSearchControls", "createBookInspectionControls", "createBookLinkControls"])
    globals[name] = inert;
  for (const name of ["createBookImagePanel", "createBookFontPanel", "createBookPdfPanel", "createBookInspectionPanel", "createBookLinkPanel"])
    globals[name] = () => {};
  vm.runInNewContext(body, globals, { filename: "book.js", timeout: 1000 });
  return { calls, events, nodes };
}

test("the real publisher creates data-only profile controls and brackets host suspension/disposal", () => {
  assert.match(source, /import \{[^}]*createBookPublicationControls[^}]*\} from "\.\/book_pdf_controls\.mjs"/);
  const h = setup(); assert.equal(h.calls.filter(value => value === "profile.create").length, 1);
  assert.equal(h.calls.filter(value => value === "worker").length, 6, "no additional renderer for authoring");
  h.events.get("pagehide")({ persisted: true });
  assert(h.calls.indexOf("profile.suspend") < h.calls.indexOf("host.suspend"));
  h.events.get("pageshow")({ persisted: true }); assert(h.calls.includes("profile.resume"));
  h.events.get("pagehide")({ persisted: false });
  assert(h.calls.indexOf("profile.dispose") < h.calls.indexOf("host.dispose"));
});

test("profile initialization failure leaves the existing publisher and lifecycle usable", () => {
  const h = setup({ fail: true });
  assert.match(h.nodes.get("#book-publication-status").textContent, /Applied settings, source editing and ordinary publication remain available/);
  assert.equal(h.calls.filter(value => value === "worker").length, 6);
  h.events.get("pagehide")({ persisted: true }); h.events.get("pageshow")({ persisted: true });
  h.events.get("pagehide")({ persisted: false }); assert(h.calls.includes("host.dispose"));
});
