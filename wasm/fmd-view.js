// <fmd-view> — drop-in Markdown renderer web component (bead uito).
//
// Framework-free custom element over the package's createRenderer surface:
//
//   <fmd-view src="./README.md"></fmd-view>
//   <fmd-view>Six **bytes** of markdown</fmd-view>
//
// Attributes:
//   src       - fetch this URL as the Markdown source (slot content is the
//               fallback when src is absent/unset)
//   font      - "sans" | "serif"
//   dark-mode - "auto" | "disabled"
//
// The rendered document lands in a shadow root (style-isolated; the engine's
// self-contained HTML carries its own CSS). A "fmd-rendered" CustomEvent with
// { bytes, sourceLength, diagnostics } fires after each successful render;
// "fmd-error" fires on fetch/render failure. Bytes parity with renderHtml()
// is exact: the same call is used.
//
// Zero dependencies; the shared wasm payload is the package's.

import { createRenderer } from "./franken_markdown.js";

let rendererPromise = null;

function sharedRenderer(wasmInput) {
  if (!rendererPromise) {
    rendererPromise = createRenderer(wasmInput);
  }
  return rendererPromise;
}

const SLOT_RENDER = Symbol("slot source");

class FmdView extends HTMLElement {
  static get observedAttributes() {
    return ["src", "font", "dark-mode"];
  }

  #root = null;
  #lastSrc = undefined;

  constructor() {
    super();
    this.#root = this.attachShadow({ mode: "open" });
    this.#root.innerHTML = "<style>:host{display:block}</style><slot></slot>";
  }

  connectedCallback() {
    this.#render();
  }

  attributeChangedCallback(name, oldV, newV) {
    if (oldV === newV || !this.#root) {
      return;
    }
    if (name === "src" && newV !== this.#lastSrc) {
      this.#render();
      return;
    }
    if (name === "font" || name === "dark-mode") {
      this.#render();
    }
  }

  async #source() {
    const src = this.getAttribute("src");
    if (src) {
      const res = await fetch(src);
      if (!res.ok) {
        throw new Error(`fmd-view: fetch ${src} -> HTTP ${res.status}`);
      }
      return await res.text();
    }
    // Slot content as inline Markdown. Preserve text exactly.
    const slot = this.querySelector("script[type='text/markdown']");
    if (slot) {
      return slot.textContent ?? "";
    }
    return this.textContent ?? "";
  }

  async #render() {
    const srcAttr = this.getAttribute("src");
    this.#lastSrc = srcAttr;
    let markdown;
    try {
      markdown = await this.#source();
    } catch (err) {
      this.dispatchEvent(new CustomEvent("fmd-error", { detail: { error: String(err) } }));
      return;
    }
    let renderer;
    try {
      // If the host already initialized the engine with explicit wasm bytes,
      // a package-level export would be needed; default path resolves the
      // bundled artifact via the wrapper's own URL resolution.
      renderer = await sharedRenderer(undefined);
    } catch (err) {
      this.dispatchEvent(new CustomEvent("fmd-error", { detail: { error: String(err) } }));
      return;
    }
    const out = await renderer.renderHtml(markdown, {
      font: this.getAttribute("font") === "serif" ? "serif" : "sans",
      darkMode: this.getAttribute("dark-mode") === "disabled" ? "disabled" : "auto",
    });
    const html = out.text();
    this.#root.innerHTML = `<style>:host{display:block}</style>${html}`;
    this.dispatchEvent(
      new CustomEvent("fmd-rendered", {
        detail: {
          bytes: out.bytes.byteLength,
          sourceLength: out.sourceLength,
          diagnostics: out.diagnostics,
        },
      })
    );
  }
}

const ELEMENT_NAME = "fmd-view";

/**
 * Register <fmd-view> on a customElements registry (defaults to global).
 * Idempotent; returns the element name.
 */
export function registerFmdView(registry = globalThis.customElements) {
  if (!registry.get(ELEMENT_NAME)) {
    registry.define(ELEMENT_NAME, FmdView);
  }
  return ELEMENT_NAME;
}

// Auto-register in browser globals (no-op under Node/bundlers without it).
if (typeof globalThis.customElements !== "undefined") {
  registerFmdView();
}
