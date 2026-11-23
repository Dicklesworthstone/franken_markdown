// Real revision-review controller; DOM and native worker outputs are explicit
// fixtures. Native profile semantics/parity live in analysis_profile_smoke.mjs.
import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { installRevisionReview } from "./demo/revision_review.mjs";

class Element extends EventTarget {
  constructor(id = "") {
    super(); this.id = id; this.value = ""; this.hidden = false; this.disabled = false;
    this.attributes = new Map(); this.children = []; this.textContent = "";
  }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  removeAttribute(name) { this.attributes.delete(name); if (name === "srcdoc") delete this.srcdoc; }
  append(...nodes) { this.children.push(...nodes); }
  appendChild(node) { this.append(node); return node; }
  replaceChildren(...nodes) { this.children = nodes; }
  click() { this.dispatchEvent(new Event("click")); }
  remove() {}
}
const turn = () => new Promise(resolve => setImmediate(resolve));
function gate() { let resolve; const promise = new Promise(done => { resolve = done; }); return {promise, resolve}; }
async function fixture(t, {delay = false, corrupt} = {}) {
  const markup = await readFile(new URL("demo/review.html", import.meta.url), "utf8");
  const elements = new Map([...markup.matchAll(/\bid="([^"]+)"/g)].map(match => [match[1], new Element(match[1])]));
  // The old page has no control; keep the same host surface for its regression.
  if (!elements.has("review-profile")) elements.set("review-profile", new Element("review-profile"));
  const window = new EventTarget();
  Object.assign(window, {setTimeout() { return 1; }, clearTimeout() {}, confirm() { return true; }});
  const document = {defaultView: window, body: new Element(), getElementById(id) { return elements.get(id); },
    createDocumentFragment() { return new Element(); }, createElement() { return new Element(); }};
  const node = id => elements.get(id);
  node("review-results").hidden = true;
  node("before-name").value = "Before Ω"; node("after-name").value = "After";
  node("before-source").value = "[Term\n: definition](/old)";
  node("after-source").value = "[Term\n: definition](/new)";
  const calls = [], workers = [];
  const controller = installRevisionReview(document, {rendererFactory() {
    const worker = {disposed: false, dispose() {this.disposed = true;},
      async compare(oldSource, newSource, options) {
        const pending = gate(), call = {oldSource, newSource, options: {...options}, pending}; calls.push(call);
        if (delay) await pending.promise;
        const receipt = options.profile === undefined ? {} : {profile: options.profile};
        const value = {...receipt,
          report: {...receipt, stats: {unchanged_blocks: 0, inserted_blocks: 1, deleted_blocks: 1,
            modified_blocks: 0, words_inserted: 1, words_deleted: 1, similarity_ratio: 0.5}},
          html: {...receipt, bytes: new TextEncoder().encode('<!doctype html><html><head></head><body>fixture</body></html>'), diagnostics: []},
          json: {bytes: new TextEncoder().encode(JSON.stringify({fixture: true, ...receipt}))}};
        corrupt?.(value);
        return value;
      }};
    workers.push(worker); return worker;
  }});
  t.after(() => controller.dispose());
  return {node, window, document, controller, calls, workers, markup};
}

test("review page exposes a labeled grammar choice without changing source inputs", async t => {
  const f = await fixture(t);
  assert.match(f.markup, /id="review-profile"/);
  assert.match(f.markup, /value="commonmark-gfm"/); assert.match(f.markup, /value="gfm-plus"/);
  assert.match(f.markup, /for="review-profile"/);
});

for (const profile of ["", "commonmark-gfm", "gfm-plus"]) test(`review captures ${profile || "legacy default"} for its worker request`, async t => {
  const f = await fixture(t);
  f.node("review-profile").value = profile;
  f.node("compare").click(); await turn();
  assert.equal(f.calls.length, 1);
  assert.deepEqual(f.calls[0].options, {oldName: "Before Ω", newName: "After", ...(profile ? {profile} : {})});
  assert.equal(f.node("review-results").hidden, false);
  assert.equal(f.node("download-review-json").disabled, false);
  assert.equal(f.workers[0].disposed, true);
});

test("changing grammar retires in-flight work and permits a fresh comparison", async t => {
  const f = await fixture(t, {delay: true});
  f.node("review-profile").value = "commonmark-gfm";
  f.node("compare").click(); await turn();
  f.node("review-profile").value = "gfm-plus";
  f.node("review-profile").dispatchEvent(new Event("change"));
  assert.equal(f.workers[0].disposed, true);
  assert.equal(f.node("review-results").hidden, true);
  assert.equal(f.node("compare").disabled, false);
  f.node("compare").click(); await turn();
  assert.equal(f.calls.length, 2);
  f.calls[0].pending.resolve(); await turn();
  assert.equal(f.node("review-results").hidden, true);
  f.calls[1].pending.resolve(); await turn();
  assert.equal(f.calls[1].options.profile, "gfm-plus");
  assert.equal(f.node("review-results").hidden, false);
  assert.equal(f.node("before-source").value, "[Term\n: definition](/old)");
});

test("silent grammar changes cannot publish a response or activate an old download", async t => {
  const f = await fixture(t, {delay: true});
  f.node("review-profile").value = "commonmark-gfm";
  f.node("compare").click(); await turn();
  f.node("review-profile").value = "gfm-plus";
  f.calls[0].pending.resolve(); await turn();
  assert.equal(f.node("review-results").hidden, true);
  f.node("compare").click(); await turn();
  f.calls[1].pending.resolve(); await turn();
  assert.equal(f.node("review-results").hidden, false);
  f.node("review-profile").value = "commonmark-gfm";
  f.node("download-review-html").click(); await turn();
  assert.equal(f.node("review-results").hidden, true);
  assert.equal(f.document.body.children.length, 0);
});

test("invalid grammar refuses worker allocation and preserves both drafts", async t => {
  const f = await fixture(t);
  f.node("review-profile").value = "gfm";
  f.node("compare").click(); await turn();
  assert.equal(f.calls.length, 0); assert.equal(f.workers.length, 0);
  assert.match(f.node("review-status").textContent, /dialect/i);
  assert.equal(f.node("after-source").value, "[Term\n: definition](/new)");
});

for (const target of ["envelope", "report", "html"]) test(`review refuses a missing ${target} dialect receipt`, async t => {
  const f = await fixture(t, {corrupt(value) { delete (target === "envelope" ? value : value[target]).profile; }});
  f.node("review-profile").value = "commonmark-gfm";
  f.node("compare").click(); await turn();
  assert.equal(f.node("review-results").hidden, true);
  assert.equal(f.node("download-review-json").disabled, true);
  assert.match(f.node("review-status").textContent, /dialect/i);
});

test("composition and page suspension disable grammar changes until the source is ready", async t => {
  const f = await fixture(t);
  f.node("before-source").dispatchEvent(new Event("compositionstart"));
  assert.equal(f.node("review-profile").disabled, true);
  f.node("before-source").dispatchEvent(new Event("compositionend"));
  assert.equal(f.node("review-profile").disabled, false);
  f.window.dispatchEvent(new Event("pagehide"));
  assert.equal(f.node("review-profile").disabled, true);
  f.window.dispatchEvent(new Event("pageshow"));
  assert.equal(f.node("review-profile").disabled, false);
});
