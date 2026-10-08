// Production controllers, proof session, façade and worker protocol, with
// explicit DOM/source/native doubles. This is not a browser or renderer proof.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { createBookPdfControls } from "../demo/book_pdf_controls.mjs";
import { fixture } from "./book_retained_exports_fixture.mjs";

const dataModule = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
// Link unchanged controller bodies against explicit host-only dependencies.
// Every executed render/retention path below uses the production JS modules.
export async function publisherModule(path = new URL("../demo/book_controls.mjs", import.meta.url)) {
  let source = await readFile(path, "utf8");
  const unsupported = `() => { throw Object.assign(Error("host import unavailable in this fixture"), { code: "FIXTURE_IMPORT" }); }`;
  const host = dataModule(`export const createBookCollection = ${unsupported};
    export const normalizeBookProject = value => structuredClone(value);
    export const readBookFiles = ${unsupported}, readBookProject = ${unsupported}, readPortableBookProject = ${unsupported};
    export const discardPortableBookProject = () => {};`);
  const imports = {
    "../book_worker.mjs": new URL("../book_worker.mjs", import.meta.url).href,
    "../pdf_page.mjs": new URL("../pdf_page.mjs", import.meta.url).href,
    "./book_collection.mjs": host,
    "./book_source_search.mjs": dataModule("export const bookEditorOffset = (_, offset) => offset;"),
  };
  for (const [specifier, link] of Object.entries(imports)) {
    assert(source.includes(`from "${specifier}"`));
    source = source.replace(`from "${specifier}"`, `from "${link}"`);
  }
  return import(dataModule(source));
}

export class Element extends EventTarget {
  value = ""; checked = false; disabled = false; hidden = false;
  textContent = ""; children = []; style = {}; attributes = new Map();
  selectionStart = 0; selectionEnd = 0; scrollTop = 0;
  setAttribute(key, value) { this.attributes.set(key, value); }
  removeAttribute(key) { this.attributes.delete(key); delete this[key]; }
  replaceChildren(...children) { this.children = children; }
  appendChild(child) { this.children.push(child); return child; }
  append(...children) { this.children.push(...children); }
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
  focus() {}
  fire(type) { return this.dispatchEvent(new Event(type, { cancelable: true })); }
}
export function sourceHost({ legacy = false } = {}) {
  const state = {
    files: [{ path: "one.md", source: "# One" }, { path: "two.md", source: "# Two" },
      { path: "part.txt", source: "Shared", role: "include" }],
    options: { title: "Manual", author: "Writer", lang: "en", font: "sans", darkMode: "disabled",
      fontScale: 1, toc: true, pageNumbers: true },
    images: [{ destination: "figure.svg", bytes: new Uint8Array([1, 2, 3]) }],
    fonts: [{ slot: "body-regular", bytes: new Uint8Array([4, 5]) }],
    revision: 0, configuration: 0,
  };
  const listeners = new Set();
  const changed = (sourceOnly = false) => {
    state.revision++; if (!sourceOnly) state.configuration++;
    for (const listener of listeners) listener();
  };
  const collection = {
    get revision() { return state.revision; },
    get renderConfigurationRevision() { return legacy ? undefined : state.configuration; },
    get files() { return structuredClone(state.files); },
    get options() { return structuredClone(state.options); },
    get images() { return state.images.map(({ destination, bytes }) => ({ destination, size: bytes.length })); },
    get fonts() { return state.fonts.map(({ slot, bytes }) => ({ slot, size: bytes.length })); },
    subscribe(fn) { listeners.add(fn); return () => listeners.delete(fn); },
    edit(index, path, source) {
      const old = state.files[index];
      if (old.path === path && old.source === source) return;
      state.files[index] = { ...old, path, source }; changed(old.path === path);
    },
    setRole(index, role) {
      const old = state.files[index];
      if ((old.role ?? "chapter") === role) return;
      state.files[index] = { path: old.path, source: old.source, ...(role === "include" ? { role } : {}) };
      changed();
    },
    configure(options) { state.options = structuredClone(options); changed(); },
    append(batch) {
      state.files.push(...batch.chapters, ...(batch.includeSources ?? []).map(f => ({ ...f, role: "include" })));
      state.images.push(...batch.images); changed();
    },
    move(index, delta) {
      const next = index + delta;
      [state.files[index], state.files[next]] = [state.files[next], state.files[index]];
      changed(); return next;
    },
    remove(index) { state.files.splice(index, 1); changed(); },
    replaceSources(files, revision) {
      assert.equal(revision, state.revision);
      state.files = structuredClone(files); changed(true); return state.revision;
    },
    revokeImages() { state.images = []; changed(); },
    revokeFonts() { state.fonts = []; changed(); },
    replaceProject(project) { state.files = structuredClone(project.files); state.options = structuredClone(project.options);
      state.images = []; state.fonts = []; changed(); },
    snapshot() {
      const files = state.files.filter(f => f.role !== "include");
      const includeSources = state.files.filter(f => f.role === "include").map(({ path, source }) => ({ path, source }));
      return prepareBookInput(files, { ...state.options, includeSources, images: state.images, fontAssets: state.fonts });
    },
    project() { return structuredClone({ schemaVersion: 2, files: state.files, options: state.options }); },
    dispose() { listeners.clear(); },
  };
  return { state, collection, changed };
}

export function transport({ legacy = false, hold = false } = {}) {
  const endpoints = [];
  const client = createBookWorkerClient({ retainBook: true, timeoutMs: 5000,
    workerFactory() {
      const native = fixture({ pdfEnvelope: true });
      const endpoint = new EventTarget(), scope = new EventTarget();
      endpoint.native = native; endpoint.terminated = 0;
      endpoint.terminate = () => { endpoint.terminated++; native.retained.clear(); };
      endpoint.postMessage = (value, transfer) => {
        const data = structuredClone(value, { transfer });
        queueMicrotask(() => scope.dispatchEvent(new MessageEvent("message", { data })));
      };
      scope.postMessage = (value, transfer) => {
        const data = structuredClone(value, { transfer });
        if (!endpoint.terminated) queueMicrotask(() => endpoint.dispatchEvent(new MessageEvent("message", { data })));
      };
      installBookWorker(scope, { ...native.engine,
        renderRetainedBook: async (...args) => {
          const result = await native.retained.render(...args);
          if (hold && args[0].some(file => file.source === "HOLD")) {
            endpoint.held = true;
            await new Promise(resolve => { endpoint.release = resolve; });
          }
          return result;
        }, clearRetainedBook: native.retained.clear });
      endpoints.push(endpoint); return endpoint;
    },
  });
  // Legacy clients lack cancellation that distinguishes running from idle.
  const worker = legacy ? { render: client.render, cancel: client.cancel, dispose: client.dispose } : client;
  return { endpoints, client, worker };
}

export async function workbench(t, config = {}) {
  const host = sourceHost(config), publishing = transport(config), proofing = transport(config);
  const elements = new Map();
  const root = {
    querySelector(selector) {
      if (selector === "#publish-title") return null; // Optional page/portable panels are not this fixture's subject.
      if (!elements.has(selector)) elements.set(selector, new Element());
      return elements.get(selector);
    }, createElement() { return new Element(); },
  };
  const el = id => root.querySelector(`#${id}`);
  const liveUrls = new Map(), revoked = [];
  let serial = 0;
  const urls = { createObjectURL(blob) { const url = `blob:fixture/${++serial}`; liveUrls.set(url, blob); return url; },
    revokeObjectURL(url) { revoked.push(url); liveUrls.delete(url); } };
  const { createBookControls } = await publisherModule();
  const controls = createBookControls({ root, collection: host.collection, worker: publishing.worker, urls });
  const pdf = createBookPdfControls({ root, collection: host.collection, controls, worker: proofing.worker,
    urls, navigator: { pdfViewerEnabled: false } });
  t.after(() => { pdf.dispose(); controls.dispose(); publishing.client.dispose(); proofing.client.dispose(); });
  return { ...host, publishing, proofing, controls, pdf, el, root, liveUrls, revoked,
    edit(text) { el("chapter-source").value = text; el("chapter-source").fire("input"); },
    async published() {
      const text = await liveUrls.get(el("download").href).text();
      return JSON.parse(text.replace(/^%PDF-1.7\n/, ""));
    },
  };
}
