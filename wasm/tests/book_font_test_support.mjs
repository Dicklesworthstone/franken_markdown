import { createBookFontStore } from "../demo/book_font_assets.mjs";

// Explicit source/collection adapter around the PRODUCTION font store. Tests
// exercise the real authoring controller, not Rust font parsing or PDF output.
export function host() {
  const store = createBookFontStore(), changes = new Set(), state = new Set();
  let revision = 0, source = "\ufeff# Book\r\n\nOriginal source", view = source, busy = false;
  const images = [{ destination: "figure.png", bytes: new Uint8Array([1, 2, 3]) }];
  const changed = () => { revision++; for (const listener of changes) listener(); };
  const collection = {
    get revision() { return revision; }, get fonts() { return store.list(); },
    get files() { return [{ path: "book.md", source }]; }, get images() { return images; },
    setFonts(values, expected) {
      const fence = () => { if (expected !== revision) throw Object.assign(Error("stale"), { code: "STALE_SOURCE" }); };
      fence(); store.set(values, fence); changed();
    },
    revokeFont(slot) { if (store.remove(slot)) changed(); },
    revokeFonts() { if (store.clear()) changed(); },
    subscribe(listener) { changes.add(listener); return () => changes.delete(listener); },
  };
  const controls = {
    get sourceBusy() { return busy; },
    captureProject() { if (source !== view) { source = view; changed(); } return { source }; },
    checkpoint() { return JSON.stringify([revision, view]); },
    subscribeSourceState(listener) { state.add(listener); return () => state.delete(listener); },
  };
  return { collection, controls, store,
    edit(value, notify = true) { view = value; if (notify) { source = view; changed(); } },
    busy(value) { busy = value; for (const listener of state) listener(); },
    change: changed, subscribers: () => [changes.size, state.size],
  };
}
export function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
export const tick = () => new Promise(resolve => setTimeout(resolve, 0));
export function result() {
  return { format: "book-pdf", mimeType: "application/pdf", bytes: new TextEncoder().encode("%PDF-test adapter, not a PDF") };
}
export function worker({ delay = false, reject = null } = {}) {
  const jobs = [];
  let disposed = 0;
  return { jobs, get disposals() { return disposed; },
    render(files, format, options, { signal }) {
      const promise = deferred();
      jobs.push({ files, format, options, signal, ...promise });
      return delay ? promise.promise : reject ? Promise.reject(reject) : Promise.resolve(result());
    },
    cancel() { throw Error("Global worker cancellation must not be used"); },
    dispose() { disposed++; },
  };
}
