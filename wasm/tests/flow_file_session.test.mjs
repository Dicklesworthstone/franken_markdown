import assert from "node:assert/strict";
import test from "node:test";
import { documentSnapshot } from "../demo/flow_document.mjs";
import { createFileDocumentSession } from "../demo/flow_file_session.mjs";
import { deferred, MemoryFileHandle } from "./flow_file_fixtures.mjs";

function host(initial = { filename: "draft.md", source: "# Draft\n" }, extra = {}) {
  let value = initial,
    session,
    blocked = false;
  const states = [];
  const options = {
    readDocument: () => value,
    replaceDocument: (next) => {
      value = documentSnapshot(next);
      session.detach();
      session.changed();
    },
    renameDocument: (filename) => {
      value = { ...value, filename };
      session.changed();
    },
    isBlocked: () => blocked,
    onState: (state) => states.push(state),
    ...extra,
  };
  session = createFileDocumentSession(options);
  return {
    session,
    states,
    get document() {
      return value;
    },
    edit(source, notify = true) {
      value = { ...value, source };
      if (notify) session.changed();
    },
    rename(filename) {
      value = { ...value, filename };
      session.changed();
    },
    replace(source = "Other") {
      value = { filename: "other.md", source };
      session.detach();
    },
    block(value) {
      blocked = value;
      session.changed();
    },
    async open(handle = new MemoryFileHandle()) {
      await session.open(
        () => Promise.resolve([handle]),
        () => true,
      );
      return handle;
    },
  };
}
const code = (value) => ({ code: value });

test("open preserves UTF-8 BOM, mixed line endings, Unicode and source-only state", async () => {
  const h = host(),
    source = "\uFEFF# Title\r\n日本語 😀\rEnd\n";
  const file = await h.open(new MemoryFileHandle("稿.md", source));
  assert.deepEqual(h.document, { filename: "稿.md", source });
  assert.equal(h.session.state.phase, "saved");
  assert.equal(h.session.state.dirty, false);
  assert.equal(file.log.includes("permission"), false);
  const json = JSON.stringify(h.session.state);
  assert.equal(json.includes(source), false);
  assert.equal(json.includes("handle"), false);
  assert.equal(Object.isFrozen(h.session.state), true);
});

test("unchanged original bytes survive direct save, including BOM and CRLF", async () => {
  const h = host(),
    file = await h.open(new MemoryFileHandle("notes.md", "\uFEFFx\r\ny\r"));
  const before = file.entry.bytes.slice();
  await h.session.save();
  assert.deepEqual(file.entry.bytes, before);
});

test("edited save stages a Blob, requests exclusive replacement, truncates and verifies", async () => {
  const h = host(),
    file = await h.open();
  h.edit("é");
  file.hooks.create = (_, options) =>
    assert.deepEqual(options, { keepExistingData: false, mode: "exclusive" });
  file.hooks.permission = (_, options) => assert.deepEqual(options, { mode: "readwrite" });
  file.hooks.write = (_, blob) => {
    assert.ok(blob instanceof Blob);
    assert.equal(blob.size, 2);
  };
  const result = await h.session.save();
  assert.deepEqual(result, { saved: true, current: true, filename: "notes.md" });
  assert.equal(file.source, "é");
  assert.equal(h.session.state.phase, "saved");
  assert.equal(file.log.includes("abort"), false);
  assert.ok(file.counts.read >= 6);
});

test("same length external edit with unchanged mtime refuses overwrite", async () => {
  const h = host(),
    file = await h.open(new MemoryFileHandle("notes.md", "aaa"));
  h.edit("mine");
  file.source = "bbb";
  await assert.rejects(h.session.save(), code("FILE_CHANGED"));
  assert.equal(file.source, "bbb");
  assert.equal(h.document.source, "mine");
  assert.equal(file.counts.create ?? 0, 0);
  assert.equal(h.session.state.phase, "conflict");
  assert.equal(h.session.state.canSave, false);
});

for (const stage of ["create", "write"])
  test(`external change during ${stage} aborts before close`, async () => {
    const h = host(),
      file = await h.open();
    h.edit("mine");
    file.hooks[stage] = () => {
      file.source = "external";
    };
    await assert.rejects(h.session.save(), code("FILE_CHANGED"));
    assert.equal(file.source, "external");
    assert.equal(file.counts.close ?? 0, 0);
    assert.equal(file.counts.abort, 1);
  });

for (const stage of ["permission", "create", "write"])
  test(`source edit during ${stage} never commits older source`, async () => {
    const h = host(),
      file = await h.open();
    h.edit("mine");
    file.hooks[stage] = () => h.edit("newer");
    await assert.rejects(h.session.save(), code("STALE_SOURCE"));
    assert.equal(h.document.source, "newer");
    assert.equal(file.source, "# Disk\n");
    assert.equal(file.counts.close ?? 0, 0);
  });

test("ABA edit and unannounced raw source mutation are both fenced", async () => {
  const h = host(),
    file = await h.open();
  h.edit("mine");
  file.hooks.permission = () => {
    h.edit("other");
    h.edit("mine");
  };
  await assert.rejects(h.session.save(), code("STALE_SOURCE"));
  file.hooks.permission = () => h.edit("raw", false);
  await assert.rejects(h.session.save(), code("STALE_SOURCE"));
  assert.equal(file.counts.create ?? 0, 0);
});

test("newer edits during close survive and are correctly left dirty", async () => {
  const h = host(),
    file = await h.open();
  h.edit("saved snapshot");
  file.hooks.close = () => h.edit("newer unsaved source");
  assert.equal((await h.session.save()).current, false);
  assert.equal(file.source, "saved snapshot");
  assert.equal(h.document.source, "newer unsaved source");
  assert.equal(h.session.state.phase, "edited");
  assert.equal(h.session.state.canSave, true);
  delete file.hooks.close;
  await h.session.save();
  assert.equal(file.source, "newer unsaved source");
});

test("replacement during close cannot reconnect the old file", async () => {
  const h = host(),
    file = await h.open();
  h.edit("captured");
  file.hooks.close = () => h.replace();
  const result = await h.session.save();
  assert.equal(result.current, false);
  assert.equal(file.source, "captured");
  assert.equal(h.document.source, "Other");
  assert.equal(h.session.state.filename, null);
});

for (const stage of ["close", "committed"])
  test(`uncertain ${stage} failure blocks retry on current file`, async () => {
    const h = host(),
      file = await h.open();
    h.edit("mine");
    file.hooks[stage] = () => {
      throw new Error("private OS details");
    };
    await assert.rejects(h.session.save(), code("FILE_SAVE_UNCERTAIN"));
    assert.equal(h.session.state.phase, "uncertain");
    assert.equal(h.session.state.canSave, false);
    assert.equal(h.session.state.message.includes("private OS"), false);
    assert.equal(h.document.source, "mine");
  });

test("read-back mismatch never claims saved", async () => {
  const h = host(),
    file = await h.open();
  h.edit("mine");
  file.hooks.committed = () => {
    file.source = "changed after close";
  };
  await assert.rejects(h.session.save(), code("FILE_SAVE_UNCERTAIN"));
  assert.equal(h.session.state.phase, "uncertain");
});

test("permission denied, writer lock and write failure keep source and baseline", async () => {
  const h = host(),
    file = await h.open();
  h.edit("mine");
  file.permission = "denied";
  await assert.rejects(h.session.save(), code("FILE_PERMISSION_DENIED"));
  assert.equal(file.counts.create ?? 0, 0);
  file.permission = "granted";
  file.hooks.create = () => {
    throw new DOMException("busy", "NoModificationAllowedError");
  };
  await assert.rejects(h.session.save(), code("FILE_LOCKED"));
  delete file.hooks.create;
  file.hooks.write = () => {
    throw new DOMException("full", "QuotaExceededError");
  };
  await assert.rejects(h.session.save(), code("FILE_WRITE_FAILED"));
  assert.equal(file.counts.abort, 1);
  assert.equal(file.source, "# Disk\n");
  delete file.hooks.write;
  await h.session.save();
  assert.equal(file.source, "mine");
});

test("renaming editor filename requires Save as rather than overwrite old name", async () => {
  const h = host(),
    file = await h.open();
  h.rename("different.md");
  assert.equal(h.session.state.phase, "renamed");
  assert.equal(h.session.state.canSave, false);
  await assert.rejects(h.session.save(), code("FILENAME_CHANGED"));
  assert.equal(file.counts.create ?? 0, 0);
});

test("Save as creates and connects a new file without replacing source", async () => {
  const h = host(),
    file = new MemoryFileHandle("new.md", "");
  await h.session.saveAs(() => file);
  assert.equal(file.source, "# Draft\n");
  assert.equal(h.document.filename, "new.md");
  assert.equal(h.session.state.phase, "saved");
  h.edit("next");
  await h.session.save();
  assert.equal(file.source, "next");
});

test("Save as cannot silently replace nonempty target; cancellation retains current file", async () => {
  const h = host(),
    old = await h.open(),
    target = new MemoryFileHandle("other.md", "valuable");
  h.edit("mine");
  assert.equal((await h.session.saveAs(() => target)).saved, false);
  assert.equal(target.source, "valuable");
  assert.equal(h.session.state.filename, old.name);
  assert.equal(target.counts.create ?? 0, 0);
});

test("Save as freezes target during confirmation and cannot bypass same-entry conflict", async () => {
  const h = host(),
    old = await h.open();
  h.edit("mine");
  old.source = "external";
  await assert.rejects(h.session.save(), code("FILE_CHANGED"));
  const alias = new MemoryFileHandle(old.name, "", old.entry);
  await assert.rejects(
    h.session.saveAs(
      () => alias,
      () => true,
    ),
    code("FILE_CHANGED"),
  );
  assert.equal(alias.counts.create ?? 0, 0);
  const target = new MemoryFileHandle("copy.md", "before");
  await assert.rejects(
    h.session.saveAs(
      () => target,
      () => {
        target.source = "after";
        return true;
      },
    ),
    code("FILE_CHANGED"),
  );
  assert.equal(target.source, "after");
  const copy = new MemoryFileHandle("safe.md", "");
  await h.session.saveAs(() => copy);
  assert.equal(copy.source, "mine");
  assert.equal(old.source, "external");
  assert.equal(h.session.state.phase, "saved");
});

test("Save as to current file without prior conflict still compares original baseline", async () => {
  const h = host(),
    old = await h.open();
  h.edit("mine");
  old.source = "external";
  await assert.rejects(
    h.session.saveAs(
      () => old,
      () => true,
    ),
    code("FILE_CHANGED"),
  );
  assert.equal(old.source, "external");
});

test("newer edits during Save as close are not renamed or connected to saved snapshot", async () => {
  const h = host(),
    target = new MemoryFileHandle("new.md", "");
  target.hooks.close = () => h.edit("next draft");
  assert.equal((await h.session.saveAs(() => target)).current, false);
  assert.equal(h.document.filename, "draft.md");
  assert.equal(h.document.source, "next draft");
  assert.equal(h.session.state.filename, null);
});

test("reload is explicit, cancellation keeps edits, successful reload resolves conflict", async () => {
  const h = host(),
    file = await h.open();
  h.edit("mine");
  file.source = "external";
  await assert.rejects(h.session.save(), code("FILE_CHANGED"));
  assert.equal((await h.session.reload(() => false)).opened, false);
  assert.equal(h.document.source, "mine");
  await h.session.reload(() => true);
  assert.equal(h.document.source, "external");
  assert.equal(h.session.state.phase, "saved");
});

test("open and reload do not install disk version that changed during confirmation", async () => {
  const h = host(),
    file = new MemoryFileHandle();
  await assert.rejects(
    h.session.open(
      () => [file],
      () => {
        file.source = "external";
        return true;
      },
    ),
    code("FILE_CHANGED"),
  );
  assert.equal(h.document.source, "# Draft\n");
  await h.open(file);
  h.edit("mine");
  await assert.rejects(
    h.session.reload(() => {
      file.source = "again";
      return true;
    }),
    code("FILE_CHANGED"),
  );
  assert.equal(h.document.source, "mine");
});

test("picker and confirmation races preserve source and existing file connection", async () => {
  const h = host(),
    old = await h.open(),
    selected = new MemoryFileHandle("new.md"),
    pending = deferred();
  const opening = h.session.open(
    () => pending.promise,
    () => true,
  );
  h.edit("typed during picker");
  pending.resolve([selected]);
  await assert.rejects(opening, code("STALE_SOURCE"));
  assert.equal(h.session.state.filename, old.name);
  await assert.rejects(
    h.session.open(
      () => [selected],
      () => {
        h.edit("during confirmation");
        return true;
      },
    ),
    code("STALE_SOURCE"),
  );
});

test("operations serialize, composition refuses start, picker called in same turn", async () => {
  const h = host(),
    pending = deferred();
  let calls = 0;
  const opening = h.session.open(
    () => {
      calls++;
      return pending.promise;
    },
    () => true,
  );
  assert.equal(calls, 1);
  await assert.rejects(h.session.save(), code("FILE_BUSY"));
  pending.resolve([new MemoryFileHandle()]);
  await opening;
  h.block(true);
  await assert.rejects(h.session.save(), code("FILE_BUSY"));
  h.block(false);
});

test("cancelling a picker does not disconnect current file or reveal errors", async () => {
  const h = host(),
    old = await h.open();
  await assert.rejects(
    h.session.open(() => {
      throw new DOMException("secret", "AbortError");
    }),
    code("FILE_CANCELLED"),
  );
  assert.equal(h.session.state.filename, old.name);
  assert.equal(h.session.state.message.includes("secret"), false);
});

for (const source of ["\ud800", "x".repeat(4 * 1024 * 1024 + 1)])
  test("invalid editor source is refused before opening writable", async () => {
    const h = host(),
      file = await h.open();
    h.edit(source);
    await assert.rejects(h.session.save());
    assert.equal(file.counts.permission ?? 0, 0);
    assert.equal(file.counts.create ?? 0, 0);
    assert.equal(h.session.state.phase, "invalid");
    assert.equal(h.document.source, source);
  });

test("malformed UTF-8, oversized disk files and executable filenames never replace editor", async () => {
  const h = host(),
    invalid = new MemoryFileHandle();
  invalid.entry.bytes = new Uint8Array([0xc0, 0xaf]);
  await assert.rejects(h.open(invalid), code("INVALID_UNICODE"));
  const large = new MemoryFileHandle("big.md", "x".repeat(4 * 1024 * 1024 + 1));
  await assert.rejects(h.open(large), code("BUDGET_EXCEEDED"));
  for (const name of ["evil.html", "script.js", "../bad.md"])
    await assert.rejects(h.open(new MemoryFileHandle(name)), code("INVALID_FILENAME"));
  assert.equal(h.document.filename, "draft.md");
});

test("dispose aborts a staged writer; pending picker can never resurrect authority", async () => {
  const h = host(),
    file = await h.open();
  h.edit("mine");
  const entered = deferred(),
    resume = deferred();
  file.hooks.write = async () => {
    entered.resolve();
    await resume.promise;
  };
  const saving = h.session.save();
  await entered.promise;
  h.session.dispose();
  resume.resolve();
  await assert.rejects(saving);
  assert.equal(file.source, "# Disk\n");
  assert.equal(file.counts.abort, 1);
  assert.equal(file.counts.close ?? 0, 0);
  assert.equal(h.session.state.filename, null);
  h.session.dispose();
  const other = host(),
    picker = deferred(),
    opening = other.session.open(
      () => picker.promise,
      () => true,
    );
  other.session.dispose();
  picker.resolve([file]);
  await assert.rejects(opening, code("SESSION_DISPOSED"));
  assert.equal(other.document.filename, "draft.md");
});

test("observer failure does not change completed save outcome", async () => {
  const h = host(undefined, {
    onState() {
      throw new Error("view failed");
    },
  });
  const file = await h.open();
  h.edit("mine");
  assert.equal((await h.session.save()).saved, true);
  assert.equal(file.source, "mine");
});

test("changed handle name cannot silently redirect a connected save", async () => {
  const h = host(),
    file = await h.open();
  h.edit("mine");
  file.name = "renamed.md";
  await assert.rejects(h.session.save(), code("FILE_CHANGED"));
  assert.equal(file.counts.create ?? 0, 0);
  assert.equal(h.session.state.filename, "notes.md");
});

test("failed host rename reports verified bytes but does not connect an unsafe identity", async () => {
  const h = host(undefined, {
    renameDocument() {
      throw new Error("host unavailable");
    },
  });
  const file = new MemoryFileHandle("new.md", "");
  const result = await h.session.saveAs(() => file);
  assert.equal(result.saved, true);
  assert.equal(result.current, false);
  assert.equal(file.source, "# Draft\n");
  assert.equal(h.session.state.filename, null);
});

test("late completed save leaves replacement document's status alone", async () => {
  const h = host(),
    file = await h.open();
  h.edit("captured");
  let status;
  file.hooks.close = () => {
    h.replace();
    status = h.session.state.message;
  };
  await h.session.save();
  assert.equal(h.session.state.message, status);
});
