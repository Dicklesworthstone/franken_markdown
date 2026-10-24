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
// is exact: the same call is used. Document selectors are scoped to the host
// and embedded fonts are decoded and registered under per-render family names.
// Those registrations are retired on replacement/disconnect, without touching
// other elements or host-owned fonts. A font failure retains the previous view.
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
let fontScope = 0;

// Complete-document selectors do not match inside a shadow tree, and browsers
// do not register its @font-face rules. Let the browser parse our renderer CSS;
// adapt only document selectors and the renderer's two font-family variables.
// Unique families prevent one document subset or serif choice affecting peers.
async function shadowDocument(html, doc) {
  const template = doc.createElement("template");
  template.innerHTML = `<style>:host{display:block}</style>${html}`;
  const sheets = [...template.content.querySelectorAll("style")].map(node => {
    const sheet = new doc.defaultView.CSSStyleSheet();
    sheet.replaceSync(node.textContent);
    return { node, sheet };
  });
  const families = new Map(), faces = [], scope = ++fontScope;
  const visit = (rules, work) => {
    for (const rule of rules) {
      work(rule);
      if (rule.cssRules) visit(rule.cssRules, work);
    }
  };
  for (const { sheet } of sheets) visit(sheet.cssRules, rule => {
    if (rule.type !== 5) return; // CSSRule.FONT_FACE_RULE
    const family = rule.style.getPropertyValue("font-family");
    if (!families.has(family)) families.set(family, `fmd-view-${scope}-${families.size}`);
    const descriptors = {};
    for (const [key, property] of Object.entries({ style: "font-style", weight: "font-weight",
      stretch: "font-stretch", display: "font-display", unicodeRange: "unicode-range",
      featureSettings: "font-feature-settings", variationSettings: "font-variation-settings" })) {
      const value = rule.style.getPropertyValue(property);
      if (value) descriptors[key] = value;
    }
    faces.push(new doc.defaultView.FontFace(families.get(family), rule.style.getPropertyValue("src"), descriptors));
    rule.style.setProperty("font-family", families.get(family));
  });
  for (const { node, sheet } of sheets) {
    visit(sheet.cssRules, rule => {
      if ([":root", "html", "body"].includes(rule.selectorText)) rule.selectorText = ":host";
      if (!rule.style) return;
      for (const property of ["--fmd-font-body", "--fmd-font-mono"]) {
        const value = rule.style.getPropertyValue(property);
        for (const [family, alias] of families) {
          if (value === family || value.startsWith(`${family},`))
            rule.style.setProperty(property, alias + value.slice(family.length));
        }
      }
    });
    node.textContent = [...sheet.cssRules].map(rule => rule.cssText).join("\n");
  }
  // Decode before registration/publication: a superseded attempt never gains
  // document font authority, and a bad font leaves the previous view intact.
  await Promise.all(faces.map(face => face.load()));
  return { fragment: template.content, faces };
}

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
  #releaseFonts = null;

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
    this.#releaseFonts?.();
    this.#releaseFonts = null;
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
      const prepared = await shadowDocument(html, this.ownerDocument);
      if (!this.#current(request)) return;
      const fontSet = this.ownerDocument.fonts;
      const release = () => { for (const face of prepared.faces) fontSet.delete(face); };
      const previous = this.#releaseFonts;
      try {
        for (const face of prepared.faces) fontSet.add(face);
        if (!this.#current(request)) { release(); return; }
        this.#releaseFonts = release;
        this.#root.replaceChildren(prepared.fragment);
      } catch (error) {
        release();
        if (this.#releaseFonts === release) this.#releaseFonts = previous;
        throw error;
      }
      previous?.();
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
