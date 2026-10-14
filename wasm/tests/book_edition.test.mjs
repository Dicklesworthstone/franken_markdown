import assert from "node:assert/strict";
import test from "node:test";
import { createBookEditionSession } from "../demo/book_edition.mjs";
import { createBookPublicationClient } from "../book_publication.mjs";

// Native format generation and the workbench codec are explicit test adapters.
// Both production ownership modules above execute through unchanged ESM imports.
const types = { pdf: "application/pdf", epub: "application/epub+zip", site: "application/zip" };
const code = value => cause => cause?.code === value;
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
function output(format, sourceLength = 12) {
  const bytes = new TextEncoder().encode(`fixture-${format}`), mimeType = types[format];
  return { format: `book-${format}`, sourceLength, bytes, mimeType,
    blob: () => new Blob([bytes], { type: mimeType }) };
}
function edition(formats = ["pdf", "epub", "site"]) {
  const outputs = formats.map(format => output(format));
  return { format: "book-publication", sourceLength: 12,
    byteLength: outputs.reduce((sum, value) => sum + value.bytes.length, 0), outputs };
}
function harness(overrides = {}) {
  const live = new Map(), revoked = [], calls = [], states = [];
  let serial = 0, revision = 0, raw = "", blocked = false, captures = 0, cancelled = 0, disposed = 0;
  const input = { files: [{ path: "one.md", source: "# One\r\n\n![x](x.png)" }], options: {
    title: "Manual", includeSources: [{ path: "inc.txt", source: "inclusion" }],
    images: [{ destination: "x.png", bytes: new Uint8Array([1, 2]) }],
    fontAssets: [{ slot: "body", bytes: new Uint8Array([3, 4]) }],
    page: { size: "a4" }, typography: "protrusion", metadataEpochSeconds: 1700000000,
  } };
  const urls = {
    createObjectURL(blob) { const url = `blob:edition/${++serial}`; live.set(url, blob); return url; },
    revokeObjectURL(url) { revoked.push(url); live.delete(url); },
  };
  const publisher = {
    render(...args) { calls.push(args); return Promise.resolve(edition(args[1])); },
    cancel() { cancelled++; }, dispose() { disposed++; },
  };
  const h = { live, revoked, calls, states, input, urls, publisher,
    get captures() { return captures; }, get cancelled() { return cancelled; }, get disposed() { return disposed; },
    bump() { revision++; }, raw(value) { raw = value; }, blocked(value) { blocked = value; },
  };
  Object.assign(publisher, overrides.publisher);
  h.session = createBookEditionSession({ publisher, urls,
    capture() { captures++; overrides.capture?.(h); return structuredClone(input); },
    checkpoint: () => JSON.stringify([revision, raw]), isBlocked: () => blocked,
    onState(value) { states.push(value); overrides.onState?.(value, h); },
    ...overrides.session,
  });
  return h;
}

test("captures full source/settings/resource input once and publishes all URLs together", async () => {
  const h = harness();
  assert.equal(h.captures, 0); assert.equal(h.calls.length, 0); assert.equal(h.live.size, 0);
  const state = await h.session.prepare();
  assert.equal(h.captures, 1); assert.equal(h.calls.length, 1);
  assert.deepEqual(h.calls[0].slice(0, 3), [h.input.files, ["pdf", "epub", "site"], h.input.options]);
  assert.notEqual(h.calls[0][2].images[0].bytes, h.input.options.images[0].bytes);
  assert.equal(state.phase, "ready"); assert.equal(state.downloads.length, 3);
  assert.equal(h.states.filter(s => s.downloads.length === 1 || s.downloads.length === 2).length, 0);
  assert.deepEqual(state.downloads.map(d => d.filename), ["book.pdf", "book.epub", "book-site.zip"]);
  for (const d of state.downloads) {
    assert.equal(await h.live.get(d.href).text(), `fixture-${d.format}`);
    assert.equal(h.session.authorizeDownload(d.href), true);
  }
  assert(Object.isFrozen(state.downloads)); assert(Object.isFrozen(state.downloads[0]));
  assert.equal("source" in state, false);
  h.session.dispose(); assert.equal(h.live.size, 0); assert.equal(h.disposed, 1);
});

test("capture's own collection notifications are included rather than cancelling its edition", async () => {
  const h = harness({ capture(h) { h.bump(); h.session.changed(); } });
  assert.equal((await h.session.prepare()).phase, "ready");
  assert.equal(h.cancelled, 0); h.session.dispose();
});

test("format selection is owned, supports subsets, and an empty selection starts no renderer", async () => {
  const h = harness(), selected = ["site", "pdf"];
  h.session.setFormats(selected); selected[0] = "epub";
  assert.deepEqual((await h.session.prepare()).downloads.map(d => d.format), ["site", "pdf"]);
  h.session.setFormats([]); assert.equal(h.live.size, 0); assert.equal(h.session.state.canPrepare, false);
  await assert.rejects(h.session.prepare(), code("INVALID_FORMAT")); assert.equal(h.calls.length, 1);
  h.session.dispose();
});

for (const bad of [["preview"], ["pdf", "pdf"], ["pdf", "epub", "site", "pdf"], new Array(1), null]) {
  test(`rejects invalid selection without replacing the ready edition: ${JSON.stringify(bad)}`, async () => {
    const h = harness(); await h.session.prepare(); const links = h.session.state.downloads;
    assert.throws(() => h.session.setFormats(bad), code("INVALID_FORMAT"));
    assert.deepEqual(h.session.state.downloads, links); assert.equal(h.live.size, 3); h.session.dispose();
  });
}

test("format accessor is refused without executing it", () => {
  const h = harness(); let invoked = false; const value = [];
  Object.defineProperty(value, "0", { get() { invoked = true; return "pdf"; } });
  assert.throws(() => h.session.setFormats(value), code("INVALID_FORMAT"));
  assert.equal(invoked, false); h.session.dispose();
});

test("overlap and capture reentrancy cannot cancel the active publication", async () => {
  const pending = deferred(); let reentered;
  const h = harness({ publisher: { render() { return pending.promise; } },
    capture(h) { reentered = assert.rejects(h.session.prepare(), code("BOOK_BUSY")); } });
  const running = h.session.prepare();
  await assert.rejects(h.session.prepare(), code("BOOK_BUSY")); await reentered;
  assert.equal(h.session.state.busy, true); assert.equal(h.cancelled, 0);
  pending.resolve(edition()); await running; h.session.dispose();
});

for (const method of ["changed", "invalidate", "cancel", "suspend", "dispose", "formats"]) {
  test(`${method} promptly retires an uncooperative publisher and fences its late result`, async () => {
    const pending = deferred(); const h = harness({ publisher: { render() { return pending.promise; } } });
    const running = h.session.prepare();
    const rejected = assert.rejects(running, cause => !!cause?.code);
    if (method === "formats") h.session.setFormats(["pdf"]); else h.session[method]();
    await rejected; assert.equal(h.session.state.busy, false); assert.equal(h.live.size, 0);
    pending.resolve(edition()); await new Promise(resolve => setImmediate(resolve));
    assert.equal(h.live.size, 0); assert.equal(h.session.state.downloads.length, 0);
    h.session.dispose();
  });
}

test("late rejection after cancellation is consumed and cannot overwrite a newer edition", async () => {
  const pending = deferred(); let calls = 0;
  const h = harness({ publisher: { render() { return ++calls === 1 ? pending.promise : Promise.resolve(edition()); } } });
  const old = h.session.prepare(); const rejected = assert.rejects(old, code("EXPORT_CANCELLED"));
  h.session.cancel(); await rejected; await h.session.prepare();
  const links = h.session.state.downloads;
  pending.reject(new Error("late old worker")); await new Promise(resolve => setImmediate(resolve));
  assert.equal(h.session.state.phase, "ready"); assert.deepEqual(h.session.state.downloads, links);
  h.session.dispose();
});

test("raw edits without collection notifications are fenced at completion and download", async () => {
  const pending = deferred(); const h = harness({ publisher: { render() { return pending.promise; } } });
  const job = h.session.prepare(); h.raw("invalid half-typed path"); pending.resolve(edition());
  await assert.rejects(job, code("STALE_SOURCE")); assert.equal(h.live.size, 0);
  h.publisher.render = () => Promise.resolve(edition()); await h.session.prepare();
  const href = h.session.state.downloads[0].href; h.raw("other raw edit");
  assert.equal(h.session.authorizeDownload(href), false); assert.equal(h.live.size, 0);
  h.session.dispose();
});

test("an old link cannot discard a newer ready edition", async () => {
  const h = harness(); await h.session.prepare(); const old = h.session.state.downloads[0].href;
  await h.session.prepare(); const next = h.session.state.downloads;
  assert.equal(h.session.authorizeDownload(old), false); assert.equal(h.live.size, 3);
  assert.deepEqual(h.session.state.downloads, next); h.session.dispose();
});

test("import/composition blocking prevents capture and revokes a ready edition on activation", async () => {
  const h = harness(); h.blocked(true);
  assert.equal(h.session.state.canPrepare, false);
  await assert.rejects(h.session.prepare(), code("BOOK_BUSY")); assert.equal(h.captures, 0);
  h.blocked(false); await h.session.prepare(); const link = h.session.state.downloads[0].href;
  h.blocked(true); assert.equal(h.session.authorizeDownload(link), false); assert.equal(h.live.size, 0);
  h.session.dispose();
});

for (const change of [
  result => { result.outputs.pop(); },
  result => { result.outputs[1].format = "book-pdf"; },
  result => { result.outputs[1].sourceLength++; },
  result => { result.byteLength++; },
  result => { result.sourceLength = NaN; },
  result => { result.byteLength = 128 * 1024 * 1024 + 1; },
  result => { result.outputs[2].mimeType = "text/html"; },
  result => { result.outputs[2].blob = () => new Blob([], { type: "application/zip" }); },
  result => { result.outputs[2].blob = () => new Blob(["x"], { type: "text/html" }); },
]) {
  test(`malformed edition is refused before any URL allocation: ${change}`, async () => {
    const result = edition(); change(result);
    const h = harness({ publisher: { render: () => Promise.resolve(result) } });
    await assert.rejects(h.session.prepare(), code("WORKER_PROTOCOL_ERROR"));
    assert.equal(h.live.size, 0); assert.equal(h.revoked.length, 0);
    assert.equal(h.session.state.phase, "error"); h.session.dispose();
  });
}

test("URL allocation failure releases the entire staged set and permits a fresh retry", async () => {
  const h = harness(); const create = h.urls.createObjectURL; let count = 0;
  h.urls.createObjectURL = blob => { if (++count === 3) throw new Error("URL allocation failed"); return create(blob); };
  await assert.rejects(h.session.prepare(), /URL allocation failed/);
  assert.equal(h.live.size, 0); assert.equal(h.revoked.length, 2);
  assert.equal(h.states.some(s => s.downloads.length), false);
  h.urls.createObjectURL = create; await h.session.prepare(); assert.equal(h.live.size, 3); h.session.dispose();
});

test("a checkpoint change during URL staging cannot publish any download", async () => {
  const h = harness(); const create = h.urls.createObjectURL;
  h.urls.createObjectURL = blob => { const url = create(blob); h.bump(); return url; };
  await assert.rejects(h.session.prepare(), code("STALE_SOURCE"));
  assert.equal(h.live.size, 0); assert.equal(h.revoked.length, 1); h.session.dispose();
});

test("capture and Blob errors preserve source and can be retried", async () => {
  let first = true;
  const h = harness({ capture() { if (first) { first = false; throw new Error("invalid page"); } } });
  await assert.rejects(h.session.prepare(), /invalid page/); assert.equal(h.calls.length, 0);
  await h.session.prepare(); const before = structuredClone(h.input);
  h.publisher.render = () => { const result = edition(); result.outputs[2].blob = () => { throw new Error("Blob failed"); }; return result; };
  await assert.rejects(h.session.prepare(), /Blob failed/);
  assert.deepEqual(h.input, before); assert.equal(h.live.size, 0); h.session.dispose();
});

test("suspension discards resources and never starts work on resume; disposal is permanent", async () => {
  const h = harness(); await h.session.prepare(); h.session.suspend(); h.session.suspend();
  assert.equal(h.live.size, 0); assert.equal(h.session.state.phase, "suspended");
  await assert.rejects(h.session.prepare(), code("WORKBENCH_SUSPENDED"));
  h.session.resume(); h.session.resume(); assert.equal(h.calls.length, 1);
  await h.session.prepare(); h.session.dispose(); h.session.dispose();
  assert.equal(h.disposed, 1); assert.equal(h.live.size, 0);
  await assert.rejects(h.session.prepare(), code("SESSION_DISPOSED"));
});

test("throwing view and cleanup adapters cannot hide valid output or skip URL retirement", async () => {
  const h = harness({ onState() { throw new Error("view failed"); } });
  await h.session.prepare(); assert.equal(h.session.state.phase, "ready");
  const revoke = h.urls.revokeObjectURL;
  h.urls.revokeObjectURL = url => { revoke(url); throw new Error("revocation adapter failed"); };
  h.session.dispose(); assert.equal(h.live.size, 0); assert.equal(h.revoked.length, 3);
});

for (const failSecond of [false, true]) {
  test(`production coordinator integration: ${failSecond ? "second-format failure publishes nothing" : "one capture, empty deltas and complete delivery"}`, async () => {
    const calls = []; let releases = 0, revision = 0;
    const publisher = createBookPublicationClient({
      captureInput: (files, options) => structuredClone({ files, options }),
      createClient: () => ({
        async render(files, format, options) {
          calls.push({ files, format, options });
          return { ...output(format), retainedInputRevision: ++revision };
        },
        async renderSourceUpdate(changes, format, options) {
          calls.push({ changes, format, options });
          if (failSecond) throw Object.assign(new Error("native fixture failure"), { code: "NATIVE_FAILED" });
          return { ...output(format), retainedInputRevision: ++revision };
        },
        dispose() { releases++; },
      }),
    });
    const h = harness({ session: { publisher } });
    if (failSecond) {
      await assert.rejects(h.session.prepare(), code("NATIVE_FAILED"));
      assert.equal(h.live.size, 0); assert.equal(calls.length, 2);
      assert.equal(h.states.some(s => s.downloads.length > 0), false);
    } else {
      await h.session.prepare(); assert.equal(h.live.size, 3); assert.equal(calls.length, 3);
      assert.deepEqual(calls[0].options, h.input.options);
      assert.deepEqual(calls[1], { changes: [], format: "epub", options: { expectedRevision: 1 } });
      assert.deepEqual(calls[2], { changes: [], format: "site", options: { expectedRevision: 2 } });
    }
    assert.equal(h.captures, 1); assert.equal(releases, 1); h.session.dispose();
  });
}
