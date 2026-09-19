// Production bootstrap with explicit DOM doubles. This is not browser CSP proof.
import test from "node:test";
import assert from "node:assert/strict";
import { Script } from "node:vm";
import { bookPreviewFrame, mountBookPreview } from "../book_preview_frame.mjs";
const token = "a".repeat(32);
const htmlNS = "http://www.w3.org/1999/xhtml", svgNS = "http://www.w3.org/2000/svg", mathNS = "http://www.w3.org/1998/Math/MathML";
class Node {
  constructor(tag, attrs = {}, namespaceURI = htmlNS) { this.localName = tag; this.namespaceURI = namespaceURI; this.values = new Map(Object.entries(attrs)); }
  get attributes() { return [...this.values].map(([name, value]) => ({ localName: name.includes(":") ? name.split(":").at(-1) : name, value, key: name })); }
  getAttribute(name) { return this.values.get(name) ?? null; }
  setAttribute(name, value) { this.values.set(name, value); }
  removeAttributeNode(attr) { this.values.delete(attr.key); }
  remove() { this.removed = true; }
  closest(tag) { return this.localName === tag ? this : null; }
}
function mount(nodes, fragment = "", destination = null) {
  const events = new Map(), messages = [], body = { replaceChildren(value) { this.value = value; } };
  const doc = { body, documentElement: new Node("html"), getElementById: () => destination,
    createElement(tag) { assert.equal(tag, "template"); return { set innerHTML(value) { assert(value.includes("html")); }, content: { querySelectorAll() { return nodes; } } }; },
    addEventListener(type, listener) { events.set(type, listener); } };
  mountBookPreview(doc, { parent: { postMessage(message, origin) { assert.equal(origin, "*"); messages.push(message); } } }, '<html lang="fr"><body>fixture</body></html>', token, fragment);
  return { events, messages, doc };
}
test("frame HTML puts CSP first and treats closing-script payloads only as data", () => {
  const attack = '</script><script>parent.stolen=true</script><meta http-equiv="refresh" content="0;url=https://example.invalid">';
  const frame = bookPreviewFrame(attack, token, '</script>');
  assert.equal([...frame.matchAll(/<script\b/g)].length, 1);
  assert.equal([...frame.matchAll(/<\/script>/g)].length, 1);
  assert(frame.indexOf("Content-Security-Policy") < frame.indexOf("<script"));
  assert(frame.includes("default-src 'none'")); assert(frame.includes(`script-src 'nonce-${token}'`));
  assert(frame.includes("form-action 'none'")); assert(!frame.includes("parent.stolen=true</script>"));
  const script = frame.match(/<script[^>]*>([\s\S]*)<\/script>/)[1]; assert.doesNotThrow(() => new Script(script));
  assert.throws(() => bookPreviewFrame("hi", '" onload="x'), /Invalid/);
});
test("active elements, foreign namespaces, handlers and unauthorized resources are removed before insertion", () => {
  const script = new Node("script"), meta = new Node("meta", { "http-equiv": "refresh" }), iframe = new Node("iframe", { srcdoc: "payload" });
  const image = new Node("img", { src: "https://private.invalid/pixel", onload: "steal()", srcset: "other" });
  const safe = new Node("img", { src: "data:image/png;base64,AQ==", alt: "Chart" });
  const animate = new Node("animate", {}, svgNS), weird = new Node("div", {}, "unknown");
  const h = mount([script, meta, iframe, image, safe, animate, weird]);
  for (const node of [script, meta, iframe, animate, weird]) assert(node.removed);
  assert.equal(image.attributes.length, 0); assert.equal(safe.getAttribute("alt"), "Chart"); assert(safe.getAttribute("src").startsWith("data:"));
  assert(h.doc.body.value); assert.equal(h.doc.documentElement.getAttribute("lang"), "fr");
  assert.equal(h.messages.at(-1).kind, "ready"); assert(h.messages.at(-1).restricted >= 8);
});
test("link destinations cannot navigate natively and relay only through the private map", () => {
  const a = new Node("a", { href: "end.html#end", target: "_top", download: "x", ping: "https://bad.invalid" });
  const h = mount([a]); assert.equal(a.getAttribute("href"), "#"); assert.equal(a.attributes.length, 1);
  for (const type of ["click", "auxclick"]) {
    let prevented = false, stopped = false;
    h.events.get(type)({ target: a, preventDefault() { prevented = true; }, stopPropagation() { stopped = true; } });
    assert(prevented && stopped); assert.equal(h.messages.at(-1).href, "end.html#end"); assert.equal(h.messages.at(-1).channel, token);
  }
});
test("static MathML/SVG and disabled GFM checkboxes remain readable", () => {
  const fraction = new Node("mfrac", {}, mathNS), path = new Node("path", { d: "M0 0L10 10" }, svgNS);
  const use = new Node("use", { "xlink:href": "#symbol" }, svgNS), external = new Node("use", { href: "https://evil.invalid/a.svg#x" }, svgNS);
  const check = new Node("input", { type: "checkbox", checked: "" }), input = new Node("input", { type: "text" });
  mount([fraction, path, use, external, check, input]);
  for (const node of [fraction, path, use, external, check]) assert(!node.removed);
  assert.equal(use.getAttribute("href"), "#symbol"); assert.equal(external.getAttribute("href"), null);
  assert.equal(check.getAttribute("disabled"), ""); assert(input.removed);
});
test("fragment navigation uses IDs, not selectors, and missing fragments remain visible after ready", () => {
  let scrolled = false, focused = false; const node = new Node("h2"); node.scrollIntoView = () => { scrolled = true; }; node.focus = () => { focused = true; };
  mount([node], 'id[not-a-selector]', node); assert(scrolled && focused);
  const h = mount([], "missing"); assert.deepEqual(h.messages.map(message => message.kind), ["ready", "missing-fragment"]);
});
test("element budget refuses publication without installing partial content", () => {
  const h = mount({ length: 50001 }); assert.equal(h.doc.body.value, undefined); assert.equal(h.messages.at(-1).kind, "error");
});
