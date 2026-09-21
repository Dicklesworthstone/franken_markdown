// Production controller + reading API, explicit native session/painter doubles.

import assert from "node:assert/strict";
import test from "node:test";
import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { readFlowDocument } from "../flow_reading.mjs";
import { deferred, node, ReadingSession } from "./flow_reading_fixtures.mjs";

const input = (source, more = {}) => ({ source, width: 400, height: 300, ...more });
const tick = () => new Promise((resolve) => setImmediate(resolve));
const code = (value) => (error) => error.code === value;
function fixture(readDocument = readFlowDocument, createAssets = null) {
  const f = { sessions: [], paints: [], readGate: null, failedReads: false };
  f.controller = createPreviewController({
    readDocument,
    createAssets,
    async createSession(source, layout) {
      const s = new ReadingSession();
      s.layoutOptions = layout;
      const contents = (text) => {
        s.source = text;
        s.roots = [
          node("paragraph", text, {
            enclosingSourceSpan: { startByte: 0, endByte: new TextEncoder().encode(text).length },
          }),
        ];
      };
      contents(source);
      s.dispose = () => {
        s.disposed = true;
      };
      s.replaceSource = async (text, options) => {
        assert.equal(options.expectedRevision, s.revision);
        if (text === "rejected")
          throw Object.assign(new Error("unsupported input"), { code: "LAYOUT_ERROR" });
        s.edit();
        contents(text);
      };
      const reflow = s.reflow.bind(s);
      s.reflow = (options) => {
        if (options) s.layoutOptions = { ...s.layoutOptions, ...options };
        reflow();
      };
      const read = s.readingOrder.bind(s);
      s.readingOrder = async (options) => {
        if (f.readGate) {
          const gate = f.readGate;
          f.readGate = null;
          await gate.promise;
        }
        if (f.failedReads)
          throw Object.assign(new Error("malformed tree"), { code: "INVALID_READING_DATA" });
        return read(options);
      };
      f.sessions.push(s);
      return s;
    },
    painter: {
      async render(s, options) {
        assert.deepEqual(options.token, s.token);
        f.paints.push(s.source);
        return { ...s.token, width: options.width, height: options.height };
      },
      clear() {},
      dispose() {},
    },
  });
  return f;
}

test("publishes matching semantic/pixel tokens and reuses snapshot only for non-layout viewport changes", async () => {
  const f = fixture(),
    c = f.controller;
  c.update(input("aé😀z"));
  await c.whenIdle();
  const document = c.state.document;
  assert.deepEqual(document.token, {
    revision: c.state.frame.revision,
    layoutRevision: c.state.frame.layoutRevision,
  });
  assert.deepEqual(c.locateReading(0, document, "aé😀z").sourceRange, { start: 0, end: 5 });
  c.update(input("aé😀z", { scrollY: 20, height: 400 }));
  await c.whenIdle();
  assert.equal(c.state.document, document);
  assert.equal(f.sessions[0].calls.length, 1);
  c.update(input("aé😀z", { width: 800 }));
  await c.whenIdle();
  assert.notEqual(c.state.document, document);
  assert.equal(f.sessions[0].calls.length, 2);
  assert.throws(() => c.locateReading(0, document, "aé😀z"), code("STALE_REVISION"));
  c.dispose();
});

test("source navigation rejects unsubmitted textarea edits and rejected/currently pending source changes", async () => {
  const f = fixture(),
    c = f.controller;
  c.update(input("original"));
  await c.whenIdle();
  const document = c.state.document;
  assert.throws(() => c.locateReading(0, document, "not-yet-submitted"), code("STALE_REVISION"));
  c.update(input("rejected"));
  assert.throws(() => c.locateReading(0, document, "original"), code("STALE_REVISION"));
  await c.whenIdle();
  assert.equal(c.state.document, document);
  assert.equal(c.state.status, "error");
  assert.throws(() => c.locateReading(0, document, "rejected"), code("STALE_REVISION"));
  c.dispose();
});

test("semantic admission failure does not block usable Canvas and is not retried on every scroll", async () => {
  const f = fixture((s, options) =>
      readFlowDocument(s, { ...options, limits: { maxTextUnits: 4 } }),
    ),
    c = f.controller;
  c.update(input("larger reading tree"));
  await c.whenIdle();
  assert.equal(c.state.status, "ready");
  assert.equal(c.state.document, null);
  assert.equal(c.state.readingError.code, "READING_LIMIT");
  assert.deepEqual(f.paints, ["larger reading tree"]);
  c.update(input("larger reading tree", { scrollY: 10 }));
  await c.whenIdle();
  assert.equal(f.sessions[0].calls.length, 1);
  c.update(input("fits"));
  await c.whenIdle();
  assert.equal(c.state.document.text, "fits");
  assert.equal(c.state.readingError, null);
  c.dispose();
});

test("a late reading response cannot publish an older desired source", async () => {
  const f = fixture(),
    c = f.controller,
    gate = deferred();
  f.readGate = gate;
  c.update(input("old"));
  await tick();
  c.update(input("newest"));
  gate.resolve();
  await c.whenIdle();
  assert.equal(c.state.document.text, "newest");
  assert.deepEqual(f.paints, ["newest"]);
  c.dispose();
});

test("restart/disposal revoke old semantic documents even when tokens repeat", async () => {
  const f = fixture(),
    c = f.controller;
  c.update(input("text"));
  await c.whenIdle();
  const old = c.state.document;
  c.restart();
  assert.equal(c.state.document, null);
  await c.whenIdle();
  assert.deepEqual(c.state.document.token, old.token);
  assert.notEqual(c.state.document, old);
  assert.throws(() => c.locateReading(0, old, "text"), code("STALE_REVISION"));
  c.dispose();
  assert.equal(c.state.document, null);
  assert.throws(() => old.find("text"), code("SESSION_DISPOSED"));
});

test("a foreign session snapshot with the same token cannot be published by a reader callback", async () => {
  const foreign = await readFlowDocument(
    new ReadingSession([node("paragraph", "foreign private text")]),
  );
  const f = fixture(() => foreign),
    c = f.controller;
  c.update(input("host text"));
  await c.whenIdle();
  assert.equal(c.state.status, "ready");
  assert.equal(c.state.document, null);
  assert.equal(c.state.reading, "");
  assert.equal(c.state.readingError.code, "INVALID_ARGUMENT");
  c.dispose();
});

test("image layout completion racing a reading page republishes one consistent semantic/pixel snapshot", async () => {
  const imageGate = deferred(),
    readGate = deferred();
  const f = fixture(readFlowDocument, (s) => ({
      async loadPending() {
        await imageGate.promise;
        s.reflow();
        return { loaded: 1, failed: 0, skipped: 0, errors: [] };
      },
      whenIdle() {
        return Promise.resolve();
      },
      synchronize() {},
      dispose() {},
      resolveImage() {
        return null;
      },
    })),
    c = f.controller;
  c.update(input("unchanged"));
  await c.whenIdle();
  const before = c.state.document;
  f.readGate = readGate;
  c.update(input("unchanged", { width: 800 }));
  await tick();
  imageGate.resolve();
  await tick();
  readGate.resolve();
  await c.whenIdle();
  assert.equal(c.state.status, "ready");
  assert.equal(c.state.readingError, null);
  assert.equal(c.state.document.token.layoutRevision, c.state.frame.layoutRevision);
  assert.equal(c.state.document.token.layoutRevision, f.sessions[0].token.layoutRevision);
  assert.notEqual(c.state.document, before);
  assert.equal(f.sessions[0].revision, "1");
  c.dispose();
});

test("disposing during a reading page never publishes its late semantic tree", async () => {
  const f = fixture(),
    c = f.controller,
    gate = deferred();
  f.readGate = gate;
  c.update(input("pending"));
  await tick();
  c.dispose();
  gate.resolve();
  await tick();
  assert.equal(c.state.status, "disposed");
  assert.equal(c.state.document, null);
  assert.equal(f.paints.length, 0);
});
