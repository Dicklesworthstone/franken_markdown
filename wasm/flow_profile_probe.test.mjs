// Test the probe's failure detection with explicit binding doubles, not a
// substitute for compiling the two Rust features and executing real WASM.
import assert from "node:assert/strict";
import test from "node:test";
import { probeFlowProfile, assertProfileParity } from "./flow_profile_probe.mjs";
const utf8 = value => new TextEncoder().encode(value);
function fixture(full = false) {
  let frees = 0, sessionFrees = 0;
  const bindings = Object.fromEntries(["html", "pdf"].map(format => [
    format === "html" ? "renderHtml" : "renderPdf", source => ({
      format, mimeType: format === "html" ? "text/html; charset=utf-8" : "application/pdf",
      bytes: utf8(format === "html" ? `<html>${source}</html>` : `%PDF-double:${source}`),
      free() { frees++; },
    }),
  ]));
  if (full) bindings.FmdFlowSession = class {
    constructor(source) { this.source = source; this.revision = "1"; this.layoutRevision = "1"; }
    snapshotJson(revision) { return JSON.stringify({ schemaVersion: 1, revision, items: [{}] }); }
    replaceSource(_revision, source) { this.source = source; this.revision = "2"; }
    free() { sessionFrees++; }
  };
  return { bindings, get frees() { return frees; }, get sessionFrees() { return sessionFrees; } };
}

test("accepts render-only bindings and frees every result", () => {
  const f = fixture(); assert.equal(probeFlowProfile(f.bindings, "render").length, 4);
  assert.equal(f.frees, 4); assert.equal(f.sessionFrees, 0);
});
test("full contract exercises editing, display and cleanup with matching bytes", () => {
  const render = fixture(), full = fixture(true);
  assertProfileParity(probeFlowProfile(render.bindings, "render"), probeFlowProfile(full.bindings, "full"));
  assert.equal(full.frees, 4); assert.equal(full.sessionFrees, 1);
});
test("detects feature leakage and missing full ABI before rendering", () => {
  for (const [full, profile] of [[true, "render"], [false, "full"]]) {
    const f = fixture(full); assert.throws(() => probeFlowProfile(f.bindings, profile));
    assert.equal(f.frees, 0);
  }
  const f = fixture(); f.bindings.FmdFlowSession = undefined;
  assert.throws(() => probeFlowProfile(f.bindings, "render"), /omit the class/);
});
test("rejects unknown profiles and missing core renderers", () => {
  assert.throws(() => probeFlowProfile(fixture().bindings, "other"));
  assert.throws(() => probeFlowProfile({}, "render"));
});
test("detects invalid output and still frees the returned native result", () => {
  const f = fixture(); let freed = false;
  f.bindings.renderHtml = () => ({ format: "pdf", free() { freed = true; } });
  assert.throws(() => probeFlowProfile(f.bindings, "render")); assert.equal(freed, true);
});
test("rejects non-editing full stubs and still frees the session", () => {
  const f = fixture(true); f.bindings.FmdFlowSession.prototype.replaceSource = function() {};
  assert.throws(() => probeFlowProfile(f.bindings, "full")); assert.equal(f.sessionFrees, 1);
});
test("parity requires the whole corpus and fails on a single changed byte", () => {
  const a = probeFlowProfile(fixture().bindings, "render");
  assert.throws(() => assertProfileParity([], []));
  const b = a.map(bytes => bytes.slice()); b[3][0] ^= 1;
  assert.throws(() => assertProfileParity(a, b));
});
