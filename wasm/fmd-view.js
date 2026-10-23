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
// Each connected render owns a source/options snapshot. New attributes, inline
// Markdown edits and disconnection revoke older work; canceled/stale attempts
// cannot publish HTML or events. Fetches are aborted when superseded. Inline
// child/text changes are observed only while src is absent/empty; a first
// script[type="text/markdown"] supplies the source when present. Failed shared
// engine initialization is retried by the next render (including reconnect).
//
// Zero dependencies; the shared wasm payload is the package's.

import { createRenderer } from "./franken_markdown.js";

let rendererPromise = null;

function sharedRenderer(wasmInput) {
  if (!rendererPromise) {
    const pending = Promise.resolve().then(() => createRenderer(wasmInput));
    rendererPromise = pending;
    // A transient engine-load failure must not poison all future elements.
    // Keep a newer initialization's ownership if a caller has already retried.
    pending.catch(() => {
      if (rendererPromise === pending) rendererPromise = null;
    });
  }
  return rendererPromise;
}

class FmdView extends HTMLElement {
  static get observedAttributes() {
    return ["src", "font", "dark-mode"];
  }

  #root = null;
  #request = null;
  #observer = null;
  #lastInline = undefined;

  constructor() {
    super();
    this.#root = this.attachShadow({ mode: "open" });
    this.#root.innerHTML = "<style>:host{display:block}</style><slot></slot>";
  }

  connectedCallback() {
    if (!this.isConnected) return;
    this.#observer ??= new MutationObserver(() => {
      if (this.isConnected && !this.getAttribute("src") && this.#inlineSource() !== this.#lastInline) {
        this.#startRender();
      }
    });
    this.#observer.observe(this, {
      childList: true,
      characterData: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["type"],
    });
    this.#startRender();
  }

  disconnectedCallback() {
    this.#observer?.disconnect();
    this.#cancelRender();
  }

  attributeChangedCallback(_name, oldV, newV) {
    if (oldV !== newV && this.isConnected) {
      this.#startRender();
    }
  }

  #inlineSource() {
    // Slot content as inline Markdown. Preserve text exactly.
    const slot = this.querySelector("script[type='text/markdown']");
    if (slot) {
      return slot.textContent ?? "";
    }
    return this.textContent ?? "";
  }

  #cancelRender() {
    const previous = this.#request;
    this.#request = null;
    previous?.controller.abort();
  }

  #current(request) {
    return this.isConnected && this.#request === request
      // MutationObserver runs asynchronously. Recheck inline source so a
      // resolved old promise cannot publish before the mutation callback runs.
      && (request.src || this.#inlineSource() === request.markdown);
  }

  #startRender() {
    this.#cancelRender();
    if (!this.isConnected) return;
    const src = this.getAttribute("src");
    const markdown = src ? null : this.#inlineSource();
    this.#lastInline = markdown;
    const request = {
      src,
      markdown,
      options: {
        font: this.getAttribute("font") === "serif" ? "serif" : "sans",
        darkMode: this.getAttribute("dark-mode") === "disabled" ? "disabled" : "auto",
      },
      controller: new AbortController(),
    };
    this.#request = request;
    void this.#render(request);
  }

  async #render(request) {
    try {
      let markdown = request.markdown;
      if (request.src) {
        const res = await fetch(request.src, { signal: request.controller.signal });
        if (!this.#current(request)) return;
        if (!res.ok) {
          throw new Error(`fmd-view: fetch ${request.src} -> HTTP ${res.status}`);
        }
        markdown = await res.text();
      }
      if (!this.#current(request)) return;
      const renderer = await sharedRenderer(undefined);
      if (!this.#current(request)) return;
      const out = await renderer.renderHtml(markdown, request.options);
      if (!this.#current(request)) return;
      const html = out.text();
      const detail = {
        bytes: out.bytes.byteLength,
        sourceLength: out.sourceLength,
        diagnostics: out.diagnostics,
      };
      if (!this.#current(request)) return;
      this.#root.innerHTML = `<style>:host{display:block}</style>${html}`;
      if (this.#current(request)) {
        this.dispatchEvent(new CustomEvent("fmd-rendered", { detail }));
      }
    } catch (err) {
      if (this.#current(request)) {
        this.dispatchEvent(new CustomEvent("fmd-error", { detail: { error: String(err) } }));
      }
    } finally {
      if (this.#request === request) {
        this.#request = null;
      }
    }
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
