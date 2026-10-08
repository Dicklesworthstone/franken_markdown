// Production controller/facade/transport with explicit source-host, DOM and
// native-ABI doubles. These helpers do not parse Markdown or render documents.
import { readFile } from "node:fs/promises";
import { prepareBookInput } from "../book_session.mjs";
import { createBookWorkerClient, installBookWorker } from "../book_worker.mjs";
import { nativeFixture } from "./book_worker_delta_fixture.mjs";

export class Element extends EventTarget {
  value = ""; checked = false; disabled = false; hidden = true;
  children = []; textContent = ""; selectionStart = 0; selectionEnd = 0; scrollTop = 0;
  replaceChildren(...children) { this.children = children; }
  removeAttribute(key) { delete this[key]; }
  setAttribute(key, value) { this[key] = value; }
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
  focus() {}
  fire(type) { return this.dispatchEvent(new Event(type, { cancelable: true })); }
}
export function sourceHost() {
  let revision = 0, configuration = 0;
  const listeners = new Set();
  const state = { files: [{ path: "first.md", source: "# First" }, { path: "second.md", source: "# Second" },
    { path: "shared.txt", source: "Shared", role: "include" }],
    options: { title: "Manual", author: "Writer", lang: "en", font: "sans", darkMode: "disabled",
      fontScale: 1, toc: true, pageNumbers: true },
    images: [{ destination: "figure.svg", bytes: new Uint8Array([1, 2, 3]) }],
    fonts: [{ slot: "body-regular", bytes: new Uint8Array([4, 5, 6]) }],
    snapshots: 0, projects: 0 };
  const changed = (sourceOnly = false) => { revision++; if (!sourceOnly) configuration++;
    for (const listener of listeners) listener(); };
  const collection = {
    get revision() { return revision; }, get renderConfigurationRevision() { return configuration; },
    get files() { return structuredClone(state.files); }, get options() { return structuredClone(state.options); },
    get images() { return state.images.map(a => ({ destination: a.destination, size: a.bytes.length })); },
    subscribe(fn) { listeners.add(fn); return () => listeners.delete(fn); },
    edit(index, path, source) { const old = state.files[index];
      if (old.path === path && old.source === source) return;
      state.files[index] = { ...old, path, source }; changed(old.path === path); },
    setRole(index, role) { if ((state.files[index].role ?? "chapter") !== role) { state.files[index].role = role; changed(); } },
    configure(options) { state.options = structuredClone(options); changed(); },
    move(index, delta) { const next = index + delta;
      [state.files[index], state.files[next]] = [state.files[next], state.files[index]]; changed(); return next; },
    append(batch) { state.files.push(...batch.chapters, ...(batch.includeSources ?? []).map(f => ({ ...f, role: "include" })));
      state.images.push(...batch.images); changed(); },
    remove(index) { state.files.splice(index, 1); changed(); },
    replaceSources(files, expected) { if (expected !== revision) throw Error("stale fixture");
      state.files = structuredClone(files); changed(true); return revision; },
    revokeImages() { state.images = []; changed(); }, revokeFonts() { state.fonts = []; changed(); },
    project() { state.projects++; return structuredClone({ schemaVersion: 2, files: state.files, options: state.options }); },
    snapshot() { state.snapshots++;
      return prepareBookInput(state.files.filter(f => f.role !== "include"), { ...state.options,
        includeSources: state.files.filter(f => f.role === "include").map(({ path, source }) => ({ path, source })),
        images: state.images, fontAssets: state.fonts }); },
    dispose() { listeners.clear(); },
  };
  return { state, collection, changed };
}
export function transport(t, { hold = false, acknowledge = true, mutatePosted, ...config } = {}) {
  const endpoints = [];
  const worker = createBookWorkerClient({ retainBook: true, timeoutMs: 5000, ...config, workerFactory() {
    const native = nativeFixture(), endpoint = new EventTarget(), scope = new EventTarget();
    endpoint.native = native; endpoint.terminated = 0; endpoint.messages = [];
    endpoint.terminate = () => { endpoint.terminated++; native.retained.clear(); };
    endpoint.postMessage = (value, transfer) => {
      const bytes = transfer.reduce((n, buffer) => n + buffer.byteLength, 0);
      const data = structuredClone(value, { transfer });
      endpoint.messages.push({ data: structuredClone(data), bytes });
      mutatePosted?.(value);
      queueMicrotask(() => scope.dispatchEvent(new MessageEvent("message", { data })));
    };
    scope.postMessage = (value, transfer) => {
      const data = structuredClone(value, { transfer });
      if (!acknowledge) delete data.retainedInputRevision;
      if (!endpoint.terminated) queueMicrotask(() => endpoint.dispatchEvent(new MessageEvent("message", { data })));
    };
    installBookWorker(scope, { ...native.engine, renderRetainedBook: async (...args) => {
      const result = await native.engine.renderRetainedBook(...args);
      if (hold && args[0].some(file => file.source === "HOLD")) {
        endpoint.held = true; await new Promise(resolve => { endpoint.release = resolve; });
      }
      // Header-only proof admission fixture. The body is native-double JSON,
      // not a valid/typeset PDF; lifecycle tests never claim PDF conformance.
      return args[2] === "pdf" ? { ...result, bytes: new Uint8Array([...new TextEncoder().encode("%PDF-1.7\n"), ...result.bytes]) } : result;
    } });
    endpoints.push(endpoint); return endpoint;
  } });
  t.after(() => worker.dispose());
  return { worker, endpoints };
}

export async function publisher(t, config) {
  const host = sourceHost(), wire = transport(t, config), nodes = new Map(), urls = new Map();
  const root = { createElement: () => new Element(), querySelector(selector) {
    if (selector === "#publish-title") return null;
    if (!nodes.has(selector)) nodes.set(selector, new Element());
    return nodes.get(selector);
  } };
  const dataModule = source => `data:text/javascript;base64,${Buffer.from(source).toString("base64")}`;
  // Keep the complete production controller body. Only the source-host factory
  // and unrelated import/search dependencies are explicitly injected doubles.
  let source = await readFile(new URL("../demo/book_controls.mjs", import.meta.url), "utf8");
  source = source.replace('from "../book_worker.mjs"', `from "${new URL("../book_worker.mjs", import.meta.url)}"`)
    .replace('from "../pdf_page.mjs"', `from "${new URL("../pdf_page.mjs", import.meta.url)}"`)
    .replace('from "./book_source_search.mjs"', `from "${dataModule("export const bookEditorOffset=(_,n)=>n;")}"`)
    .replace('from "./book_collection.mjs"', `from "${dataModule(`const unavailable=()=>{throw Error('fixture import not provided')};
      export const createBookCollection=unavailable, readBookFiles=unavailable, readBookProject=unavailable,
      readPortableBookProject=unavailable, discardPortableBookProject=()=>{};
      export const normalizeBookProject=p=>structuredClone(p);`)}"`);
  const { createBookControls } = await import(dataModule(source));
  let serial = 0;
  const controls = createBookControls({ root, collection: host.collection, worker: wire.worker,
    urls: { createObjectURL(blob) { const id = `blob:fixture/${++serial}`; urls.set(id, blob); return id; },
      revokeObjectURL(id) { urls.delete(id); } } });
  t.after(() => controls.dispose());
  const el = id => root.querySelector(`#${id}`);
  return { ...host, ...wire, controls, el, urls,
    edit(source) { el("chapter-source").value = source; el("chapter-source").fire("input"); },
    async output() { return JSON.parse((await urls.get(el("download").href).text()).replace(/^%PDF-1.7\n/, "")); },
  };
}
