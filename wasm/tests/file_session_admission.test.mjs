import assert from "node:assert/strict";
import test from "node:test";
import { createFileDocumentSession } from "../demo/flow_file_session.mjs";
import { createBookFileSession } from "../demo/book_file_session.mjs";
import { createFileDocumentSession as createBundledMarkdownSession } from "../../ios/Renderer/demo/flow_file_session.mjs";
import { createBookFileSession as createBundledBookSession } from "../../ios/Renderer/demo/book_file_session.mjs";

// Only host notifications and pickers are controlled here. Admission must stop
// before a file handle, permission request or writer is acquired.
function fixture(kind) {
  let session, blocked = false, guardHook = null, readHook = null, observerHook = null;
  const notices = [], markdown = kind.endsWith("Markdown");
  const create = { Markdown: createFileDocumentSession, Book: createBookFileSession,
    "Bundled Markdown": createBundledMarkdownSession, "Bundled Book": createBundledBookSession }[kind];
  const source = markdown
    ? { filename: "café.md", source: "# Draft 日本語 😀\r\n" }
    : { source: '{"files":[],"title":"café 日本語 😀"}\n', key: "initial", stamp: "0" };
  const read = () => {
    const value = source, hook = readHook;
    readHook = null;
    hook?.();
    return value;
  };
  const isBlocked = () => {
    const value = blocked, hook = guardHook;
    guardHook = null;
    const result = hook?.();
    return typeof result === "boolean" ? result : value;
  };
  const onState = value => {
    notices.push(value);
    const hook = observerHook;
    observerHook = null;
    hook?.(value);
  };
  session = markdown
    ? create({
        readDocument: read,
        replaceDocument() { assert.fail("Admission must not install source."); },
        renameDocument() { assert.fail("Admission must not rename source."); },
        isBlocked, onState,
      })
    : create({
        read,
        decode() { assert.fail("Admission must not read a disk project."); },
        replace() { assert.fail("Admission must not install a project."); },
        isBlocked, onState,
      });
  return {
    session, notices,
    hook(boundary, callback) {
      if (boundary === "access") guardHook = callback;
      else readHook = callback;
    },
    observe(callback) { observerHook = callback; },
    block() { blocked = true; },
  };
}
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}
const request = (session, operation, pick) => operation === "open"
  ? session.open(pick, () => true)
  : session.saveAs(pick, () => true);

for (const kind of ["Markdown", "Book", "Bundled Markdown", "Bundled Book"]) {
  for (const action of ["detach", "changed", "dispose"]) {
    test(kind + " startup " + action + " is not hidden by a blocked access result", async t => {
      const h = fixture(kind);
      t.after(() => h.session.dispose());
      let picks = 0;
      h.hook("access", () => { h.session[action](); return true; });
      await assert.rejects(
        h.session.open(() => { picks++; return []; }, () => true),
        { code: action === "dispose" ? "SESSION_DISPOSED" : "STALE_SOURCE" },
      );
      assert.equal(picks, 0);
      assert.equal(h.session.state.busy, null);
      assert.equal(h.notices.some(value => value.busy !== null), false);
    });
  }

  for (const boundary of ["access", "snapshot"]) {
    for (const action of ["detach", "changed", "dispose"]) {
      test(kind + " " + boundary + " admission rejects " + action + " before opening a picker", async t => {
        const h = fixture(kind);
        t.after(() => h.session.dispose());
        let picks = 0;
        h.hook(boundary, () => h.session[action]());
        await assert.rejects(
          h.session.open(() => { picks++; return []; }, () => true),
          { code: action === "dispose" ? "SESSION_DISPOSED" : "STALE_SOURCE" },
        );
        assert.equal(picks, 0);
        assert.equal(h.session.state.busy, null);
        assert.equal(h.notices.some(value => value.busy !== null), false);
        if (action === "detach") assert.match(h.session.state.message, /disconnected/i);
      });
    }

    test(kind + " Save as cannot borrow authority revoked during " + boundary + " admission", async t => {
      const h = fixture(kind);
      t.after(() => h.session.dispose());
      let picks = 0;
      h.hook(boundary, () => h.session.detach());
      await assert.rejects(
        h.session.saveAs(() => { picks++; return null; }, () => true),
        { code: "STALE_SOURCE" },
      );
      assert.equal(picks, 0);
      assert.equal(h.session.state.filename, null);
      assert.match(h.session.state.message, /disconnected/i);
    });

    test(kind + " nested " + boundary + " request keeps exclusive operation ownership", async t => {
      const h = fixture(kind), pending = deferred(), picks = [];
      t.after(() => h.session.dispose());
      let nested;
      h.hook(boundary, () => {
        nested = h.session.open(() => { picks.push("nested"); return pending.promise; }, () => true);
      });
      try {
        await assert.rejects(
          h.session.saveAs(() => { picks.push("outer"); return null; }, () => true),
          { code: "FILE_BUSY" },
        );
        assert.deepEqual(picks, ["nested"]);
        assert.match(h.session.state.busy, /^Open/);
        await assert.rejects(
          h.session.open(() => { picks.push("third"); return []; }, () => true),
          { code: "FILE_BUSY" },
        );
        assert.deepEqual(picks, ["nested"]);
      } finally {
        pending.resolve([]);
        await assert.rejects(nested, { code: "INVALID_FILE_HANDLE" });
      }
      assert.equal(h.session.state.busy, null);
    });

    test(kind + " " + boundary + " admission notices a newly locked editor before picking", async t => {
      const h = fixture(kind);
      t.after(() => h.session.dispose());
      let picks = 0;
      h.hook(boundary, () => h.block());
      await assert.rejects(
        h.session.open(() => { picks++; return []; }, () => true),
        { code: "STALE_SOURCE" },
      );
      assert.equal(picks, 0);
      assert.equal(h.session.state.busy, null);
    });
  }

  test(kind + " startup observer cannot launch a picker after disconnecting", async t => {
    const h = fixture(kind);
    t.after(() => h.session.dispose());
    let picks = 0;
    h.observe(value => { if (value.busy !== null) h.session.detach(); });
    await assert.rejects(
      h.session.open(() => { picks++; return []; }, () => true),
      { code: "STALE_SOURCE" },
    );
    assert.equal(picks, 0);
    assert.equal(h.session.state.filename, null);
    assert.match(h.session.state.message, /disconnected/i);
  });

  for (const operation of ["open", "saveAs"]) {
    test(kind + " " + operation + " keeps the initiating user gesture and serializes pending access", async t => {
      const h = fixture(kind), pending = deferred();
      t.after(() => h.session.dispose());
      let picks = 0;
      const job = request(h.session, operation, () => { picks++; return pending.promise; });
      assert.equal(picks, 1, "the picker must run before the call yields");
      const rejected = assert.rejects(job, { code: "FILE_CANCELLED" });
      await assert.rejects(
        h.session.open(() => { picks++; return []; }, () => true),
        { code: "FILE_BUSY" },
      );
      assert.equal(picks, 1);
      pending.resolve(Promise.reject(Object.assign(new Error("cancelled"), { name: "AbortError" })));
      await rejected;
      assert.equal(h.session.state.busy, null);
    });
  }
}
