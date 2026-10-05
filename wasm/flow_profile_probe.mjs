// Executable contract for generated render-only/full WASM artifacts. Importing
// this module does not load WASM; DSR supplies independently built bindings.
import assert from "node:assert/strict";

export function probeFlowProfile(bindings, profile) {
  assert(["render", "full"].includes(profile), "unknown WASM profile");
  assert.equal(typeof bindings.renderHtml, "function", "HTML core is missing");
  assert.equal(typeof bindings.renderPdf, "function", "PDF core is missing");
  const hasFlow = typeof bindings.FmdFlowSession === "function";
  assert.equal(hasFlow, profile === "full", `wrong persistent-editor ABI for ${profile}`);
  if (profile === "render") {
    assert.equal(Object.hasOwn(bindings, "FmdFlowSession"), false,
      "render-only bindings must omit the class, not expose a nonfunctional stub");
  }

  const outputs = [];
  for (const source of ["# Profile probe\n\nHello **world**.",
    "# Typography\n\nCafé, naïve and λ.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n"]) {
    for (const format of ["html", "pdf"]) {
      const result = (format === "html" ? bindings.renderHtml : bindings.renderPdf)(source);
      try {
        assert.equal(result.format, format);
        assert.equal(result.mimeType, format === "html" ? "text/html; charset=utf-8" : "application/pdf");
        const bytes = result.bytes;
        assert(bytes instanceof Uint8Array && bytes.length > 0, "missing rendered bytes");
        if (format === "pdf") assert.equal(new TextDecoder().decode(bytes.subarray(0, 5)), "%PDF-");
        else assert.match(new TextDecoder().decode(bytes), /<html[\s>]/i);
        outputs.push(new Uint8Array(bytes));
      } finally {
        result.free();
      }
    }
  }

  if (hasFlow) {
    const source = "# Full profile\n\nFirst paragraph.";
    const session = new bindings.FmdFlowSession(source, "sans", 800, 14, 13, 20);
    try {
      assert.equal(session.source, source);
      const oldRevision = session.revision;
      const snapshot = JSON.parse(session.snapshotJson(oldRevision, session.layoutRevision, 0, 32, false));
      assert.equal(snapshot.schemaVersion, 1);
      assert.equal(snapshot.revision, oldRevision);
      assert(snapshot.items.length > 0, "full profile must materialize a display");
      session.replaceSource(oldRevision, "# Edited profile", false);
      assert.equal(session.source, "# Edited profile");
      assert.notEqual(session.revision, oldRevision, "full profile must commit edits");
    } finally {
      session.free();
    }
  }
  return outputs;
}

export function assertProfileParity(render, full) {
  assert.equal(render.length, 4, "render profile corpus is incomplete");
  assert.equal(full.length, render.length, "profile corpora differ");
  for (let index = 0; index < render.length; index++) {
    assert.deepEqual(full[index], render[index], `profile output differs at corpus item ${index}`);
  }
}
