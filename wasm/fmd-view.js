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
// "fmd-error" fires on fetch/render failure. Byte counts come from the exact
// renderHtml() result. Relative fetched-document URLs are resolved against its
// final HTTP(S) URL, and local fragments navigate within this shadow tree.
// Document selectors are scoped to the host
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

// Relative URLs in fetched Markdown belong to that document, including after
// redirects. Only ordinary HTTP(S) bases are rebased: opaque data/blob sources
// and inline Markdown keep their existing host-document URL behavior. Explicit
// schemes (including embedded image data) retain the native renderer's policy.
function resourceBase(value, fallback) {
  try {
    const url = new URL(value, fallback);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    url.hash = "";
    return url.href;
  } catch { return null; }
}

function rebaseResources(fragment, base) {
  if (!base) return;
  for (const element of fragment.querySelectorAll("a[href], img[src]")) {
    const attribute = element.localName === "a" ? "href" : "src";
    const value = element.getAttribute(attribute);
    if (/^[a-z][a-z0-9+.-]*:/i.test(value)) continue;
    try {
      const url = new URL(value, base);
      if (url.protocol === "http:" || url.protocol === "https:")
        element.setAttribute(attribute, url.href);
    } catch { /* Preserve URLs the browser cannot resolve from this base. */ }
  }
}

// Complete-document selectors do not match inside a shadow tree, and browsers
// do not register its @font-face rules. Let the browser parse our renderer CSS;
// adapt only document selectors and the renderer's two font-family variables.
// Unique families prevent one document subset or serif choice affecting peers.
async function shadowDocument(html, doc, sourceUrl) {
  const template = doc.createElement("template");
  template.innerHTML = `<style>:host{display:block}</style>${html}`;
  rebaseResources(template.content, sourceUrl);
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
  #publishedSourceUrl = null;

  constructor() {
    super();
    this.#root = this.attachShadow({ mode: "open" });
    this.#root.innerHTML = "<style>:host{display:block}</style><slot></slot>";
    // Native Enter activation already produces click. A separate key handler
    // would activate links twice and interfere with ordinary keyboard behavior.
    this.#root.addEventListener("click", event => this.#navigateFragment(event));
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

  #navigateFragment(event) {
    if (!this.isConnected || event.defaultPrevented || event.button !== 0
        || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const anchor = event.composedPath().find(node => node.localName === "a"
      && node.getRootNode() === this.#root && node.hasAttribute("href"));
    if (!anchor || anchor.hasAttribute("download")) return;
    const browsingTarget = anchor.getAttribute("target")
      ?? this.ownerDocument.querySelector("base[target]")?.getAttribute("target");
    if (browsingTarget && browsingTarget.toLowerCase() !== "_self") return;
    const href = anchor.getAttribute("href");
    let fragment;
    if (href.startsWith("#")) fragment = href.slice(1);
    else {
      if (!this.#publishedSourceUrl) return;
      try {
        const url = new URL(href, this.#publishedSourceUrl);
        fragment = url.hash.slice(1);
        url.hash = "";
        if (url.href !== this.#publishedSourceUrl) return;
      } catch { return; }
    }
    if (!fragment) return;
    let id = fragment;
    try { id = decodeURIComponent(fragment); } catch { /* A literal percent may be an ID. */ }
    const target = this.#root.getElementById(id);
    if (!target) return;
    event.preventDefault();
    // Move the keyboard reading position as well as the viewport. Heading and
    // footnote containers are not normally focusable; preserve author tabindex
    // and retire only the temporary value once focus leaves this target.
    if (!target.hasAttribute("tabindex")) {
      target.setAttribute("tabindex", "-1");
      const restore = () => {
        if (target.getAttribute("tabindex") === "-1") target.removeAttribute("tabindex");
      };
      target.addEventListener("blur", restore, { once: true });
      target.focus({ preventScroll: true });
      if (this.#root.activeElement !== target) {
        target.removeEventListener("blur", restore);
        restore();
      }
    } else target.focus({ preventScroll: true });
    target.scrollIntoView({ block: "start", inline: "nearest" });
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
      let sourceUrl = null;
      if (request.src) {
        const requestedUrl = resourceBase(request.src, this.baseURI);
        const res = await fetch(request.src, { signal: request.controller.signal });
        if (!this.#current(request)) return;
        if (!res.ok) {
          throw new Error(`fmd-view: fetch ${request.src} -> HTTP ${res.status}`);
        }
        sourceUrl = res.url ? resourceBase(res.url) : requestedUrl;
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
      const prepared = await shadowDocument(html, this.ownerDocument, sourceUrl);
      if (!this.#current(request)) return;
      const fontSet = this.ownerDocument.fonts;
      const release = () => { for (const face of prepared.faces) fontSet.delete(face); };
      const previous = this.#releaseFonts;
      try {
        for (const face of prepared.faces) fontSet.add(face);
        if (!this.#current(request)) { release(); return; }
        this.#releaseFonts = release;
        this.#root.replaceChildren(prepared.fragment);
        this.#publishedSourceUrl = sourceUrl;
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
