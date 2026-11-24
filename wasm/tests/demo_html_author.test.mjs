// Production demo controller with the existing DOM/dialog/render adapters.
// Native metadata emission is covered by the Rust and generated-binding tests.
import assert from "node:assert/strict";
import test from "node:test";
import { fixture } from "./demo_file_helpers.mjs";

const source = "---\nauthor: Frontmatter creator\n---\n# Publication\n";
const author = "  Émilie & Research 😀  ";

test("HTML preview and publication retain the configured author and revoke stale output", async t => {
  const f = fixture(t);
  f.input(source); f.input(author, "author"); f.input("gfm-plus", "profile");
  const pending = f.controller.renderPreview(), call = f.renders.at(-1);
  assert.equal(call.kind, "html"); assert.equal(call.source, source);
  assert.equal(call.options.author, author); assert.equal(call.options.profile, "gfm-plus");
  call.resolve(f.output(call, "<html><head></head><body>Publication</body></html>"));
  assert.equal(await pending, true);
  const url = f.els["download-html"].href;
  assert.equal(f.els["download-html"].hidden, false);
  f.input("Revised creator", "author");
  assert.equal(f.els["download-html"].hidden, true); assert.ok(f.revoked.includes(url));
  assert.equal(f.els.markdown.value, source);
});

test("blank HTML editor authors permit source inheritance and whitespace stays verbatim", async t => {
  for (const configured of ["", "  "]) {
    const f = fixture(t);
    f.input(source); f.input(configured, "author");
    const pending = f.controller.renderPreview(), call = f.renders.at(-1);
    assert.equal(call.source, source);
    assert.equal(Object.hasOwn(call.options, "author"), configured !== "");
    if (configured !== "") assert.equal(call.options.author, configured);
    call.resolve(f.output(call));
    assert.equal(await pending, true);
  }
});

test("HTML author edits during rendering reject stale metadata without losing source", async t => {
  const f = fixture(t);
  f.input(source); f.input(author, "author");
  const pending = f.controller.renderPreview(), call = f.renders.at(-1);
  // Programmatic changes are also caught by the completion snapshot guard.
  f.els.author.value = "Later creator";
  assert.equal(call.options.author, author);
  call.resolve(f.output(call));
  assert.equal(await pending, false);
  assert.equal(f.els["download-html"].hidden, true);
  assert.equal(f.els.markdown.value, source);
});

test("unsupported HTML author metadata leaves editable source and no misleading download", async t => {
  const f = fixture(t);
  f.input(source); f.input(author, "author");
  const pending = f.controller.renderPreview(), call = f.renders.at(-1);
  call.reject(Object.assign(new Error("HTML author metadata requires matching WASM"),
    { code: "UNSUPPORTED_WASM_PACKAGE" }));
  assert.equal(await pending, false);
  assert.equal(f.els["download-html"].hidden, true); assert.equal(f.urls.length, 0);
  assert.equal(f.els.markdown.value, source); assert.equal(f.els.markdown.readOnly, false);
  assert.match(f.els.diagnostics.children[0].textContent, /UNSUPPORTED_WASM_PACKAGE/);
});
