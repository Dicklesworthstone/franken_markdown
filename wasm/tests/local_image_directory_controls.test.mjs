// Runs the actual entrypoint body and folder controls. DOM, renderer, worker,
// and surrounding controllers are explicit doubles; File/Blob and grants are real.
import assert from "node:assert/strict";
import test from "node:test";
import { File } from "node:buffer";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { createDirectoryImageControls, createLocalImageSources } from "../demo/local_image_sources.mjs";

class Element extends EventTarget {
  #value = "";
  constructor(document, tag) {
    super();
    this.ownerDocument = document;
    this.tagName = tag;
    this.children = [];
    this.attributes = new Map();
    this.files = [];
    this.disabled = false;
    this.style = {};
    this.textContent = "";
    this.clientWidth = 640;
    this.clientHeight = 480;
    this.scrollTop = 0;
    this.selectionStart = this.selectionEnd = 0;
    if (tag === "input" && document.folders) this.webkitdirectory = false;
  }
  get value() { return this.#value; }
  set value(value) {
    this.#value = value;
    if (this.type === "file" && value === "") this.files = [];
  }
  append(...children) {
    for (const child of children) { child.parentNode = this; this.children.push(child); }
  }
  remove() {
    if (this.parentNode) this.parentNode.children = this.parentNode.children.filter((child) => child !== this);
    this.parentNode = null;
  }
  setAttribute(name, value) { this.attributes.set(name, value); }
  setRangeText(text, start, end) {
    this.value = this.value.slice(0, start) + text + this.value.slice(end);
    this.selectionStart = this.selectionEnd = start + text.length;
  }
  focus() { this.focused = true; }
  emit(type, fields = {}) {
    const event = new Event(type);
    for (const [key, value] of Object.entries(fields)) Object.defineProperty(event, key, { value });
    this.dispatchEvent(event);
  }
}
function dom(folders = true) {
  const document = {
    folders,
    roots: [],
    createElement(tag) { return new Element(this, tag); },
    querySelector(selector) {
      const visit = (nodes) => {
        for (const node of nodes) {
          if (selector === `#${node.id}`) return node;
          const child = visit(node.children);
          if (child) return child;
        }
        return null;
      };
      return visit(this.roots);
    },
  };
  for (const id of ["source", "viewport", "preview", "extent", "status", "reading", "hit", "restart",
    "images", "image-controls", "image-status", "insert-images", "clear-images", "export-html", "export-pdf",
    "export-download", "export-status", "reader-panel", "find-text", "find-insensitive", "find-whole-word",
    "find-previous", "find-next", "outline", "reading-source", "reading-status"]) {
    const node = document.createElement(id === "images" ? "input" : "div");
    node.id = id;
    if (id === "images") node.type = "file";
    document.roots.push(node);
  }
  return document;
}
function file(path, bytes = [7]) {
  const image = new File([new Uint8Array(bytes)], path.split("/").at(-1));
  Object.defineProperty(image, "webkitRelativePath", { value: path });
  return image;
}
const controls = () => ({ signal: new AbortController().signal, maxBytes: 8 * 1024 * 1024 });
function fixture({ folders = true } = {}) {
  const document = dom(folders), window = new Element(document, "window");
  const events = [], previews = [], exports = [], assets = [], painters = [], frames = new Map();
  let frameId = 0;
  const noop = () => {};
  const createExportControls = () => {
    const value = { update: noop, invalidate() { events.push("invalidate-export"); }, dispose() { events.push("dispose-export"); } };
    exports.push(value);
    return value;
  };
  const createPreviewController = (options) => {
    const value = { options, disposed: false, fail: false, update: noop,
      restart() {
        events.push("restart");
        if (this.fail) throw new Error("test restart failure");
        this.options.painter.clear();
        this.assets = options.createAssets({});
      },
      dispose() { this.disposed = true; events.push("dispose-preview"); options.painter.clear(); },
    };
    previews.push(value);
    value.assets = options.createAssets({});
    return value;
  };
  const bindings = {
    document, window, Event, console, Math,
    createLocalImageSources, createDirectoryImageControls, createExportControls, createPreviewController,
    FlowImageAssets: class { constructor(session, options) { this.options = options; assets.push(this); } },
    FlowCanvasRenderer: class { constructor() { painters.push(this); } clear() { events.push("clear-pixels"); } },
    createReadingControls: () => ({ update: noop, dispose: noop }),
    createRenderSettingsControls: () => ({ settings: { values: {}, preview: {} }, dispose: noop }),
    createWorkerFlowSession: noop, readFlowDocument: noop,
    requestAnimationFrame: (callback) => { frames.set(++frameId, callback); return frameId; },
    cancelAnimationFrame: (id) => frames.delete(id),
    ResizeObserver: class { observe() {} disconnect() {} },
  };
  const entry = readFileSync(new URL("../demo/flow-canvas.js", import.meta.url), "utf8");
  // Supply the actual imported grant/control functions and explicit dependency
  // doubles. No executable entrypoint statements are removed or rewritten.
  const body = entry.replace(/^import [^;\n]+;\n/gm, "");
  assert(!body.includes("import "));
  runInNewContext(body, bindings, { filename: "flow-canvas.js" });
  const get = (id) => document.querySelector(`#${id}`);
  get("source").value = "# Source retained\n\n![plot](../images/plot.png)";
  const select = (files, path = "docs/guide.md") => {
    get("image-folder").files = files;
    get("image-folder").emit("change");
    get("image-document-path").value = path;
  };
  return { get, events, previews, exports, assets, painters, frames, window, select };
}

test("folder selection is staged until Apply and feeds retained encoded bytes to the actual entrypoint", async () => {
  const f = fixture();
  const source = f.get("source").value;
  f.select([file("project/images/plot.png", [7, 8])]);
  assert.equal(f.assets.length, 0);
  assert.equal(f.get("apply-image-folder").disabled, false);
  assert.match(f.get("image-status").textContent, /pending/);
  f.get("apply-image-folder").emit("click");
  assert.equal(f.assets.length, 1);
  assert.equal(f.assets[0].options.retainSourceBytes, true);
  assert.deepEqual(await f.assets[0].options.load({ url: "../images/plot.png" }, controls()), new Uint8Array([7, 8]));
  assert.equal(f.get("source").value, source);
  assert.equal(f.get("insert-images").disabled, false);
  assert(f.events.indexOf("invalidate-export") < f.events.indexOf("restart"));
  assert.match(f.get("image-status").textContent, /project.*docs\/guide.md/);
});

test("pending base edits do not mutate an active grant; applying replaces its resolution", async () => {
  const f = fixture();
  f.select([file("root/docs/plot.png", [1]), file("root/other/plot.png", [2])]);
  f.get("apply-image-folder").emit("click");
  const first = f.assets.at(-1);
  f.get("image-document-path").value = "other/guide.md";
  assert.deepEqual(await first.options.load({ url: "plot.png" }, controls()), new Uint8Array([1]));
  f.get("apply-image-folder").emit("click");
  const next = f.assets.at(-1);
  assert.notEqual(first, next);
  assert.deepEqual(await next.options.load({ url: "plot.png" }, controls()), new Uint8Array([2]));
});

test("invalid folder or base rejects before old pixels, export or grant are changed", async () => {
  const f = fixture();
  f.select([file("root/a.png")], "guide.md");
  f.get("apply-image-folder").emit("click");
  const count = f.events.length, active = f.assets.at(-1);
  f.get("image-document-path").value = "../guide.md";
  f.get("apply-image-folder").emit("click");
  assert.equal(f.events.length, count);
  assert.match(f.get("image-status").textContent, /previous grant is unchanged/);
  f.select([file("root/a.png"), file("different/b.png")], "guide.md");
  f.get("apply-image-folder").emit("click");
  assert.equal(f.events.length, count);
  assert.deepEqual(await active.options.load({ url: "a.png" }, controls()), new Uint8Array([7]));
});

test("switching between flat and folder grants clears the other picker without rewriting source", async () => {
  const f = fixture();
  f.get("images").files = [new File(["flat"], "flat.png")];
  f.get("images").emit("change");
  f.select([file("root/images/plot.png")]);
  f.get("apply-image-folder").emit("click");
  assert.equal(f.get("images").files.length, 0);
  assert.equal(await f.assets.at(-1).options.load({ url: "flat.png" }, controls()), null);
  f.get("images").files = [new File(["other"], "other.png")];
  f.get("images").emit("change");
  assert.equal(f.get("image-folder").files.length, 0);
  assert.equal(f.get("image-document-path").value, "");
  assert.equal(f.get("apply-image-folder").disabled, true);
  assert.equal(await f.assets.at(-1).options.load({ url: "../images/plot.png" }, controls()), null);
});

test("insertion uses escaped relative references and publishes the ordinary source input event", () => {
  const f = fixture();
  f.select([file("root/images/a[b].png")]);
  f.get("apply-image-folder").emit("click");
  const source = f.get("source"), original = source.value;
  source.selectionStart = source.selectionEnd = original.length;
  let inputs = 0;
  source.addEventListener("input", () => inputs++);
  f.get("insert-images").emit("click");
  assert.equal(source.value, original + "\n\n![a\\[b\\].png](../images/a%5Bb%5D.png)\n");
  assert.equal(inputs, 1);
  assert(source.focused);
});

test("explicit revocation drops pending folder metadata and native session assets", () => {
  const f = fixture();
  f.select([file("root/images/plot.png")]);
  f.get("apply-image-folder").emit("click");
  const original = f.get("source").value;
  f.get("clear-images").emit("click");
  assert.equal(f.previews.at(-1).assets, null);
  assert.equal(f.get("image-folder").files.length, 0);
  assert.equal(f.get("image-document-path").value, "");
  assert(f.get("insert-images").disabled && f.get("clear-images").disabled);
  assert.equal(f.get("source").value, original);
});

test("opening or restoring another document revokes the folder and its export bytes", () => {
  const f = fixture();
  f.select([file("root/images/plot.png")]);
  f.get("apply-image-folder").emit("click");
  f.get("source").value = "# Newly opened document";
  f.get("source").emit("fmd-document-replaced");
  assert.equal(f.previews.at(-1).assets, null);
  assert.equal(f.get("image-folder").files.length, 0);
  assert.equal(f.get("image-document-path").value, "");
  assert.equal(f.get("source").value, "# Newly opened document");
  assert.match(f.get("image-status").textContent, /No local images/);
});

test("suspension fences late pickers and source events; bfcache starts without prior image authority", () => {
  const f = fixture();
  f.select([file("root/images/plot.png")]);
  f.get("apply-image-folder").emit("click");
  const source = f.get("source").value, previousCount = f.previews.length;
  f.window.emit("pagehide", { persisted: true });
  assert(f.previews.at(-1).disposed);
  assert.equal(f.frames.size, 0);
  assert(f.get("image-folder").disabled);
  f.get("images").files = [new File(["late"], "late.png")];
  f.get("images").emit("change");
  f.select([file("root/late.png")]);
  f.get("apply-image-folder").emit("click");
  f.get("source").emit("fmd-document-replaced");
  f.get("restart").emit("click");
  assert.equal(f.previews.length, previousCount);
  f.window.emit("pageshow", { persisted: true });
  assert.equal(f.previews.length, previousCount + 1);
  assert.equal(f.previews.at(-1).assets, null);
  assert.equal(f.get("image-folder").files.length, 0);
  assert.equal(f.get("image-document-path").value, "");
  assert.equal(f.get("source").value, source);
  assert.equal(f.get("apply-image-folder").disabled, true);
});

test("restart failures revoke stale presentation instead of claiming unchanged authorization", () => {
  const f = fixture();
  f.previews.at(-1).fail = true;
  f.select([file("root/a.png")], "guide.md");
  f.get("apply-image-folder").emit("click");
  assert(f.previews.at(-1).disposed);
  assert.match(f.get("status").textContent, /image access changed/);
  assert.doesNotMatch(f.get("image-status").textContent, /previous grant is unchanged/);
  assert(f.events.includes("invalidate-export") && f.events.includes("clear-pixels"));
});

test("unsupported folder selection leaves flat file loading fully available", async () => {
  const f = fixture({ folders: false });
  assert(f.get("image-folder").disabled && f.get("apply-image-folder").disabled);
  assert.match(f.get("image-folder-help").textContent, /unavailable/);
  f.get("images").files = [new File(["flat"], "flat.png")];
  f.get("images").emit("change");
  assert.equal(new TextDecoder().decode(await f.assets.at(-1).options.load({ url: "flat.png" }, controls())), "flat");
});

test("permanent disposal removes folder UI and callbacks", () => {
  const f = fixture();
  f.select([file("root/a.png")], "guide.md");
  const apply = f.get("apply-image-folder");
  f.window.emit("pagehide", { persisted: false });
  assert.equal(f.get("apply-image-folder"), null);
  apply.emit("click");
  assert.equal(f.assets.length, 0);
});

test("a throwing host activation never fabricates rollback of its side effects", () => {
  const document = dom(), status = document.querySelector("#image-status");
  const ui = createDirectoryImageControls({
    container: document.querySelector("#image-controls"), status,
    onChange() { throw new Error("host failed after revocation"); },
  });
  const folder = document.querySelector("#image-folder");
  folder.files = [file("root/a.png")];
  folder.emit("change");
  document.querySelector("#apply-image-folder").emit("click");
  assert.match(status.textContent, /revoke image access before retrying/);
  assert.doesNotMatch(status.textContent, /previous grant is unchanged/);
  ui.dispose();
  ui.dispose();
});
