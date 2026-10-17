// Explicit DOM/source-host boundary. Collection, profile controls, serializer,
// byte stores and option validation are the production modules, not doubles.
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookPublicationControls } from "../demo/book_pdf_controls.mjs";

export class Element extends EventTarget {
  constructor(tag) { super(); this.tagName = tag; this.children = []; this.attributes = new Map(); }
  id = ""; textContent = ""; checked = false; disabled = false; hidden = false; style = {}; parentElement = null; _value = "";
  set value(value) {
    const text = String(value);
    this._value = this.tagName === "select" ? (this.children.some(child => child.value === text) ? text : "")
      : this.tagName === "textarea" ? text.replace(/\r\n?/g, "\n") : text;
  }
  get value() { return this._value; }
  appendChild(child) { child.parentElement = this; this.children.push(child); return child; }
  insertBefore(child, sibling) { child.parentElement = this; this.children.splice(this.children.indexOf(sibling), 0, child); }
  replaceChildren(...children) {
    for (const child of this.children) child.parentElement = null;
    this.children = []; for (const child of children) this.appendChild(child);
    if (this.tagName === "select") this._value = children[0]?.value ?? "";
  }
  setAttribute(key, value) { this.attributes.set(key, value); }
  getAttribute(key) { return this.attributes.get(key); }
  removeAttribute(key) { this.attributes.delete(key); delete this[key]; }
  querySelector(selector) {
    if (`#${this.id}` === selector) return this;
    for (const child of this.children) { const found = child.querySelector(selector); if (found) return found; }
    return null;
  }
  emit(type) { return this.dispatchEvent(new Event(type, { cancelable: true })); }
}
export function rootDocument() {
  const root = new Element("document"); root.createElement = tag => new Element(tag);
  const publish = root.appendChild(new Element("section"));
  publish.appendChild(new Element("h2")).id = "publish-title";
  return root;
}
export function collection(t) {
  const book = createBookCollection();
  book.append({ chapters: [{ path: "guide/one.md", source: "\ufeff# One\r\n\r\n{{#include ../shared.txt}}" },
    { path: "two.md", source: "# Two\r\n😀" }],
    includeSources: [{ path: "shared.txt", source: "Exact\r\n本文" }],
    images: [{ destination: "assets/logo.png", bytes: Uint8Array.of(1, 2, 3) }] });
  book.setFonts([{ slot: "body-regular", name: "fixture.ttf", bytes: Uint8Array.of(4, 5) }], book.revision);
  book.setPage({ size: "a4", margins: 36 }, book.revision);
  t.after(() => book.dispose()); return book;
}
export function setup(t, profile = {}) {
  const book = collection(t), root = rootDocument(), listeners = new Set();
  book.setPublication(profile, book.revision);
  let busy = false, captures = 0, checkpoint = 0;
  const controls = {
    get sourceBusy() { return busy; },
    captureProject() { captures++; return book.project(); },
    checkpoint: () => checkpoint,
    subscribeSourceState(fn) { listeners.add(fn); return () => listeners.delete(fn); },
  };
  const ui = createBookPublicationControls({ root, controls, collection: book });
  t.after(() => ui.dispose());
  const el = id => root.querySelector(`#book-publication-${id}`);
  return { book, root, ui, controls, el, listeners,
    get captures() { return captures; },
    input(id, value, event = "change") {
      if (typeof value === "boolean") el(id).checked = value; else el(id).value = value;
      if (event) el(id).emit(event);
    },
    busy(value) { busy = value; for (const fn of listeners) fn(); },
    editCheckpoint() { checkpoint++; },
  };
}
