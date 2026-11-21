// Production custom element in an isolated Node VM. DOM/font output and the
// renderer boundary are explicit doubles: these checks prove grammar routing,
// request ownership and error propagation, not browser layout or Rust parsing.
import assert from "node:assert/strict";
import test from "node:test";
import vm from "node:vm";
import { readFile } from "node:fs/promises";
import { normalizeAuthoringProfile } from "./render_typography.mjs";

const component = await readFile(new URL("./fmd-view.js", import.meta.url), "utf8");
const gate = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
async function until(check) {
  for (let i = 0; i < 100; i++) {
    if (check()) return;
    await new Promise(resolve => setImmediate(resolve));
  }
  assert.ok(check(), "component request did not settle");
}
async function fixture(t, { delayedInit = false, delayedFetch = false } = {}) {
  const calls = [], fetches = [], initializations = [], rendered = [], errors = [];
  const init = gate(), registrations = new Map();
  class Root extends EventTarget {
    innerHTML = "";
    replaceChildren(fragment) { this.innerHTML = fragment.html; }
  }
  const doc = {
    fonts: new Set(),
    createElement(tag) {
      assert.equal(tag, "template");
      const template = { content: { html: "", querySelectorAll: () => [] } };
      Object.defineProperty(template, "innerHTML", { set(value) { template.content.html = value; } });
      return template;
    },
  };
  class Element extends EventTarget {
    attributes = new Map();
    isConnected = false;
    textContent = "Term\n: definition\n";
    ownerDocument = doc;
    baseURI = "https://docs.example/embed.html";
    getAttribute(name) { return this.attributes.get(name) ?? null; }
    setAttribute(name, value) {
      const old = this.getAttribute(name);
      this.attributes.set(name, String(value));
      if (this.constructor.observedAttributes.includes(name))
        this.attributeChangedCallback(name, old, String(value));
    }
    removeAttribute(name) {
      const old = this.getAttribute(name);
      this.attributes.delete(name);
      if (this.constructor.observedAttributes.includes(name))
        this.attributeChangedCallback(name, old, null);
    }
    attachShadow() { this.shadowRoot = new Root(); return this.shadowRoot; }
    querySelector() { return null; }
    connect() { this.isConnected = true; this.connectedCallback(); }
    remove() { this.isConnected = false; this.disconnectedCallback(); }
  }
  const renderer = {
    renderHtml(source, options) {
      // Use the real shared facade validator at the mocked renderer boundary.
      normalizeAuthoringProfile(options.profile);
      const call = { source, options, ...gate() };
      calls.push(call);
      return call.promise;
    },
  };
  const registry = { get: name => registrations.get(name), define: (name, value) => registrations.set(name, value) };
  const context = vm.createContext({
    HTMLElement: Element, customElements: registry, AbortController, URL,
    MutationObserver: class { observe() {} disconnect() {} },
    CustomEvent: class extends Event { constructor(type, options) { super(type); this.detail = options.detail; } },
    fetch(url, options) {
      const request = { url, options, ...gate() }; fetches.push(request);
      if (!delayedFetch) request.resolve({ ok: true, url: "https://docs.example/guide.md", text: async () => "Fetched\n: definition\n" });
      return request.promise;
    },
  });
  const module = new vm.SourceTextModule(component, { context });
  await module.link(specifier => {
    assert.equal(specifier, "./franken_markdown.js");
    return new vm.SyntheticModule(["createRenderer"], function () {
      this.setExport("createRenderer", (...args) => {
        initializations.push(args);
        return delayedInit ? init.promise : Promise.resolve(renderer);
      });
    }, { context });
  });
  await module.evaluate();
  const View = registrations.get("fmd-view"), element = new View();
  element.addEventListener("fmd-rendered", event => rendered.push(event.detail));
  element.addEventListener("fmd-error", event => errors.push(event.detail));
  t.after(() => element.remove());
  const output = (call, label = call.source) => {
    const html = "<p>" + label + "</p>";
    return { bytes: new TextEncoder().encode(html), sourceLength: new TextEncoder().encode(call.source).length,
      diagnostics: [], text: () => html };
  };
  return { element, calls, fetches, initializations, rendered, errors, output, ready: () => init.resolve(renderer) };
}

for (const profile of ["commonmark-gfm", "gfm-plus"]) for (const fetched of [false, true]) {
  test("fmd-view captures " + profile + " for " + (fetched ? "fetched" : "inline") + " Markdown", async t => {
    const f = await fixture(t);
    f.element.setAttribute("profile", profile);
    if (fetched) f.element.setAttribute("src", "./guide.md");
    f.element.connect();
    await until(() => f.calls.length === 1);
    const call = f.calls[0];
    assert.equal(call.options.profile, profile);
    assert.equal(call.source, fetched ? "Fetched\n: definition\n" : f.element.textContent);
    call.resolve(f.output(call));
    await until(() => f.rendered.length === 1);
    assert.deepEqual(f.errors, []);
  });
}

test("changing dialect retires a pending render and removing it restores the exact default request", async t => {
  const f = await fixture(t);
  f.element.setAttribute("profile", "commonmark-gfm"); f.element.connect();
  await until(() => f.calls.length === 1);
  f.element.setAttribute("profile", "gfm-plus");
  await until(() => f.calls.length === 2);
  assert.equal(f.calls[0].options.profile, "commonmark-gfm");
  assert.equal(f.calls[1].options.profile, "gfm-plus");
  f.calls[1].resolve(f.output(f.calls[1], "GFM+ current"));
  await until(() => f.rendered.length === 1);
  f.calls[0].resolve(f.output(f.calls[0], "obsolete CommonMark"));
  await new Promise(resolve => setImmediate(resolve));
  assert.match(f.element.shadowRoot.innerHTML, /GFM\+ current/);
  assert.doesNotMatch(f.element.shadowRoot.innerHTML, /obsolete/);
  assert.equal(f.rendered.length, 1);
  f.element.removeAttribute("profile");
  await until(() => f.calls.length === 3);
  assert.deepEqual(Object.keys(f.calls[2].options), ["font", "darkMode"]);
  f.calls[2].resolve(f.output(f.calls[2], "default current"));
  await until(() => f.rendered.length === 2);
  assert.match(f.element.shadowRoot.innerHTML, /default current/);
});

test("dialect changes during initialization dispatch only the current captured request", async t => {
  const f = await fixture(t, { delayedInit: true });
  f.element.connect();
  await until(() => f.initializations.length === 1);
  f.element.setAttribute("profile", "commonmark-gfm");
  f.ready();
  await until(() => f.calls.length === 1);
  assert.equal(f.calls[0].options.profile, "commonmark-gfm");
  f.calls[0].resolve(f.output(f.calls[0]));
  await until(() => f.rendered.length === 1);
  assert.equal(f.initializations.length, 1);
});

test("dialect changes abort obsolete fetch ownership before source can reach the renderer", async t => {
  const f = await fixture(t, { delayedFetch: true });
  f.element.setAttribute("src", "./guide.md");
  f.element.setAttribute("profile", "commonmark-gfm"); f.element.connect();
  assert.equal(f.fetches.length, 1);
  f.element.setAttribute("profile", "gfm-plus");
  assert.equal(f.fetches[0].options.signal.aborted, true);
  assert.equal(f.fetches.length, 2);
  f.fetches[1].resolve({ ok: true, url: "https://docs.example/guide.md", text: async () => "current source" });
  await until(() => f.calls.length === 1);
  assert.equal(f.calls[0].options.profile, "gfm-plus");
  assert.equal(f.calls[0].source, "current source");
  f.calls[0].resolve(f.output(f.calls[0]));
  await until(() => f.rendered.length === 1);
  f.fetches[0].resolve({ ok: true, text: async () => "obsolete source" });
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.calls.length, 1); assert.equal(f.rendered.length, 1);
});

test("unsupported explicit grammar preserves the prior view and never retries another dialect", async t => {
  const f = await fixture(t); f.element.connect();
  await until(() => f.calls.length === 1);
  f.calls[0].resolve(f.output(f.calls[0], "existing view"));
  await until(() => f.rendered.length === 1);
  f.element.setAttribute("profile", "gfm-plus");
  await until(() => f.calls.length === 2);
  f.calls[1].reject(Object.assign(new Error("Matching profiled bindings required"), { code: "UNSUPPORTED_WASM_PACKAGE" }));
  await until(() => f.errors.length === 1);
  assert.match(f.errors[0].error, /Matching profiled bindings required/);
  assert.match(f.element.shadowRoot.innerHTML, /existing view/);
  assert.equal(f.calls.length, 2); assert.equal(f.rendered.length, 1);
});

for (const profile of ["", "gfm", " gfm-plus "]) test("invalid fmd-view profile " + JSON.stringify(profile) + " is not coerced to the default", async t => {
  const f = await fixture(t); f.element.setAttribute("profile", profile); f.element.connect();
  await until(() => f.errors.length === 1);
  assert.match(f.errors[0].error, /profile must be commonmark-gfm or gfm-plus/);
  assert.equal(f.calls.length, 0); assert.equal(f.rendered.length, 0);
});
