// Real collection, publisher controller, search and rewrite engine. Only the
// browser DOM, confirmation, object-URL registry and export worker are doubles.

import assert from "node:assert/strict";
import test from "node:test";
import { createBookCollection } from "../demo/book_collection.mjs";
import { createBookControls } from "../demo/book_controls.mjs";
import { createBookSearchControls } from "../demo/book_search_controls.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));
const gate = () => {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
const ids =
  `chapters chapter-path chapter-source add-chapter move-up move-down remove-chapter import-files import-folder open-project save-project save-chapter revoke-images image-list title author lang font dark-mode font-scale toc page-numbers export-pdf export-epub export-site cancel-export download status search-query search-replacement search-case search-scope search-run search-results search-open search-previous search-next search-page-previous search-page-next search-review-one search-review-all search-apply search-discard search-review search-status search-undo search-redo search-history`.split(
    " ",
  );
class Element extends EventTarget {
  #value = "";
  constructor(id = "") {
    super();
    this.id = id;
    this.checked = false;
    this.disabled = false;
    this.children = [];
    this.attrs = new Map();
    this.selectionStart = 0;
    this.selectionEnd = 0;
    this.scrollTop = 0;
  }
  set value(value) {
    this.#value = ["chapter-source", "search-query", "search-replacement"].includes(this.id)
      ? String(value).replace(/\r\n?/g, "\n")
      : String(value);
  }
  get value() {
    return this.#value;
  }
  set innerHTML(_) {
    throw new Error("Untrusted source must never become markup");
  }
  replaceChildren(...values) {
    this.children = values;
  }
  removeAttribute(name) {
    this.attrs.delete(name);
  }
  setAttribute(name, value) {
    this.attrs.set(name, value);
  }
  setSelectionRange(start, end) {
    this.selectionStart = start;
    this.selectionEnd = end;
  }
  focus() {
    this.focused = true;
  }
  click() {
    this.dispatchEvent(new Event("click"));
  }
}
function setup(t, sources = ["cat cat", "Cat cat"], confirm = () => true) {
  const el = Object.fromEntries(ids.map((id) => [id, new Element(id)]));
  const root = { querySelector: (key) => el[key.slice(1)], createElement: () => new Element() };
  const model = createBookCollection();
  model.append({
    chapters: sources.map((source, i) => ({ path: `chapter-${i}.md`, source })),
    images: [{ destination: "image.svg", bytes: new Uint8Array([1, 2, 3]) }],
  });
  const jobs = [],
    revoked = [];
  let cancels = 0,
    urls = 0;
  const worker = {
    cancel() {
      cancels++;
    },
    dispose() {},
    render(files, format, options) {
      const job = gate();
      jobs.push({ ...job, files, format, options });
      return job.promise;
    },
  };
  const controls = createBookControls({
    root,
    collection: model,
    worker,
    confirm,
    urls: {
      createObjectURL: () => `blob:example-${++urls}`,
      revokeObjectURL: (value) => revoked.push(value),
    },
  });
  el["search-query"].value = "cat";
  el["search-replacement"].value = "dog";
  el["search-case"].checked = true;
  el["search-scope"].value = "all";
  const search = createBookSearchControls({ root, controls, collection: model, confirm });
  t.after(() => {
    search.dispose();
    controls.dispose();
  });
  return {
    el,
    model,
    controls,
    search,
    jobs,
    revoked,
    get cancels() {
      return cancels;
    },
    input(id, value, type = "input") {
      el[id].value = value;
      el[id].dispatchEvent(new Event(type));
    },
  };
}
async function apply(h, query = "cat", replacement = "dog") {
  h.el["search-query"].value = query;
  h.el["search-replacement"].value = replacement;
  h.search.find();
  h.search.prepare();
  return h.search.apply();
}
test("search is explicit, navigates real editor ranges and leaves source/images untouched", (t) => {
  const h = setup(t, ["\ufeff😀cat\r\ncat", "Cat cat"]),
    before = h.model.files,
    revision = h.model.revision;
  assert.equal(h.el["search-results"].children.length, 0);
  assert.equal(h.search.find().matches.length, 3);
  h.search.open(1);
  assert.equal(h.el["chapter-source"].selectionStart, 7);
  assert.equal(h.el["chapter-source"].selectionEnd, 10);
  h.search.open(2);
  assert.equal(h.controls.currentChapter, 1);
  assert.equal(h.el["chapter-source"].selectionStart, 4);
  assert(h.el["chapter-source"].focused);
  assert.deepEqual(h.model.files, before);
  assert.equal(h.model.revision, revision);
  assert.equal(h.model.images.length, 1);
});
test("result controls page through all matches without injecting source markup", (t) => {
  const h = setup(t, ["<script>x</script> ".repeat(130)]);
  h.el["search-query"].value = "<script>";
  h.search.find();
  assert.equal(h.el["search-results"].children.length, 50);
  assert.match(h.el["search-results"].children[0].textContent, /<script>/);
  h.el["search-page-next"].click();
  assert.equal(h.el["search-results"].children[0].value, "50");
  h.el["search-open"].click();
  assert.equal(h.el["chapter-source"].selectionStart, 50 * 19);
  h.el["search-page-next"].click();
  assert.equal(h.el["search-results"].children.length, 30);
  assert(h.el["search-page-next"].disabled);
});
test("current-chapter search and review-one affect only the chosen original range", async (t) => {
  const h = setup(t);
  h.input("chapters", "1", "change");
  h.el["search-scope"].value = "current";
  h.el["search-case"].checked = false;
  assert.equal(h.search.find().matches.length, 2);
  h.search.open(1);
  const plan = h.search.prepare(false);
  assert.equal(plan.count, 1);
  await h.search.apply();
  assert.deepEqual(
    h.model.files.map((f) => f.source),
    ["cat cat", "Cat dog"],
  );
});
test("review shows counts and before/after excerpts without changing source or downloads", (t) => {
  const h = setup(t);
  h.controls.prepareSource(true);
  const before = h.model.files,
    revision = h.model.revision;
  h.search.find();
  const plan = h.search.prepare();
  assert.equal(plan.count, 3);
  assert.match(h.el["search-review"].textContent, /Before \(first change\)/);
  assert.match(h.el["search-review"].textContent, /After \(first change\)/);
  assert.equal(h.model.revision, revision);
  assert.deepEqual(h.model.files, before);
  assert.equal(h.el.download.hidden, false);
  assert.equal(h.revoked.length, 0);
});
test("apply updates the actual editor, invalidates downloads, notifies complete state and preserves image bytes", async (t) => {
  const h = setup(t),
    observed = [],
    settings = h.model.options;
  h.controls.prepareSource(true);
  h.model.subscribe(() => observed.push(h.model.files.map((f) => f.source)));
  await apply(h);
  assert.deepEqual(observed, [["dog dog", "Cat dog"]]);
  assert.equal(h.el["chapter-source"].value, "dog dog");
  assert.equal(h.el.download.hidden, true);
  assert.equal(h.revoked.length, 1);
  assert.deepEqual(h.model.options, settings);
  assert.deepEqual(h.model.snapshot().options.images[0].bytes, new Uint8Array([1, 2, 3]));
  assert(!h.el["search-undo"].disabled);
  assert(h.el["search-redo"].disabled);
  assert(h.el["search-apply"].disabled);
});
test("several replacement batches undo and redo atomically with exact BOM/CRLF restoration", async (t) => {
  const h = setup(t, ["\ufeffcat\r\ncat\r\n", "cat\n"]),
    before = h.model.files;
  await apply(h);
  await apply(h, "dog", "fox");
  h.search.undo();
  assert.equal(h.model.files[0].source, "\ufeffdog\r\ndog\r\n");
  h.search.undo();
  assert.deepEqual(h.model.files, before);
  h.search.redo();
  h.search.redo();
  assert.deepEqual(
    h.model.files.map((f) => f.source),
    ["\ufefffox\r\nfox\r\n", "fox\n"],
  );
  assert.equal(h.model.images.length, 1);
});
test("chapter navigation and searching do not destroy replacement history", async (t) => {
  const h = setup(t);
  await apply(h);
  h.input("chapters", "1", "change");
  h.el["search-query"].value = "dog";
  h.search.find();
  h.search.open(0);
  h.search.undo();
  assert.deepEqual(
    h.model.files.map((f) => f.source),
    ["cat cat", "Cat cat"],
  );
});
test("a new replacement after undo drops only the redo branch", async (t) => {
  const h = setup(t);
  await apply(h);
  await apply(h, "dog", "fox");
  h.search.undo();
  await apply(h, "dog", "wolf");
  assert.throws(() => h.search.redo(), { code: "NO_HISTORY" });
  h.search.undo();
  h.search.undo();
  assert.equal(h.model.files[0].source, "cat cat");
});
test("replacement history is bounded to ten batches", async (t) => {
  const h = setup(t, ["value0"]);
  for (let i = 0; i < 12; i++) await apply(h, `value${i}`, `value${i + 1}`);
  for (let i = 0; i < 10; i++) h.search.undo();
  assert.equal(h.model.files[0].source, "value2");
  assert.throws(() => h.search.undo(), { code: "NO_HISTORY" });
});
test("rejecting confirmation changes nothing and leaves the review available", async (t) => {
  const h = setup(t, ["cat"], () => false),
    before = h.model.files;
  h.search.find();
  h.search.prepare();
  assert.equal(await h.search.apply(), false);
  assert.deepEqual(h.model.files, before);
  assert(!h.el["search-apply"].disabled);
  assert(h.el["search-undo"].disabled);
});
test("asynchronous confirmation cannot overwrite intervening source edits", async (t) => {
  const decision = gate(),
    h = setup(t, ["cat"], () => decision.promise);
  h.search.find();
  h.search.prepare();
  const pending = h.search.apply();
  const rejected = assert.rejects(pending, { code: "STALE_SOURCE" });
  h.input("chapter-source", "newer");
  decision.resolve(true);
  await rejected;
  assert.equal(h.model.files[0].source, "newer");
});
test("changed raw replacement text without an event invalidates confirmation", async (t) => {
  const decision = gate(),
    h = setup(t, ["cat"], () => decision.promise);
  h.search.find();
  h.search.prepare();
  const pending = h.search.apply();
  const rejected = assert.rejects(pending, { code: "STALE_SOURCE" });
  h.el["search-replacement"].value = "not reviewed";
  decision.resolve(true);
  await rejected;
  assert.equal(h.model.files[0].source, "cat");
  assert(h.el["search-apply"].disabled);
});
test("undispatched editor or settings changes fence apply and navigation", async (t) => {
  const h = setup(t);
  h.search.find();
  h.search.prepare();
  h.el["chapter-source"].value = "uncaptured";
  await assert.rejects(h.search.apply(), { code: "STALE_SOURCE" });
  assert.throws(() => h.search.open(1), { code: "STALE_SOURCE" });
  assert.equal(h.el["chapter-source"].value, "uncaptured");
  assert.equal(h.model.files[0].source, "cat cat");
  h.search.find();
  h.search.prepare();
  h.el["font-scale"].value = "invalid";
  await assert.rejects(h.search.apply(), { code: "STALE_SOURCE" });
});
test("undispatched keystrokes are captured, not overwritten by replacement undo", async (t) => {
  const h = setup(t);
  await apply(h);
  h.el["chapter-source"].value = "latest typed source";
  assert.throws(() => h.search.undo(), { code: "STALE_SOURCE" });
  assert.equal(h.model.files[0].source, "latest typed source");
});
test("source, settings, image revocation and recovery each clear batch history", async (t) => {
  for (const mutate of [
    (h) => h.input("chapter-source", "new"),
    (h) => h.input("title", "New title"),
    (h) => h.model.revokeImages(),
    (h) =>
      h.controls.replaceProject(
        { schemaVersion: 1, files: [{ path: "new.md", source: "new" }] },
        h.controls.checkpoint(),
      ),
  ]) {
    const h = setup(t);
    await apply(h);
    mutate(h);
    assert.throws(() => h.search.undo(), { code: "NO_HISTORY" });
  }
});
test("IME composition blocks search, replace, history and source navigation", async (t) => {
  const h = setup(t);
  await apply(h);
  h.el["search-query"].value = "dog";
  h.el["search-replacement"].value = "fox";
  h.search.find();
  h.search.prepare();
  h.el["chapter-source"].dispatchEvent(new Event("compositionstart"));
  assert.throws(() => h.search.find(), { code: "BOOK_BUSY" });
  assert.throws(() => h.search.undo(), { code: "BOOK_BUSY" });
  assert.throws(() => h.controls.selectSourceRange(0, 0, 1, h.controls.checkpoint()), {
    code: "BOOK_BUSY",
  });
  assert.throws(() => h.controls.applySources(h.model.files, h.controls.checkpoint()), {
    code: "BOOK_BUSY",
  });
  h.el["chapter-source"].dispatchEvent(new Event("compositionend"));
  assert(!h.el["search-run"].disabled);
});
test("query/replacement composition cancels a pending review without auto-searching", async (t) => {
  const h = setup(t);
  h.search.find();
  h.search.prepare();
  h.el["search-replacement"].dispatchEvent(new Event("compositionstart"));
  await assert.rejects(h.search.apply(), { code: "BOOK_BUSY" });
  assert(h.el["search-apply"].disabled);
  h.el["search-replacement"].dispatchEvent(new Event("compositionend"));
  assert(h.el["search-apply"].disabled);
});
test("pending imports disable source actions then re-enable them on completion", async (t) => {
  const h = setup(t),
    read = gate();
  class SlowFile extends File {
    async arrayBuffer() {
      await read.promise;
      return super.arrayBuffer();
    }
  }
  const pending = h.controls.importFiles([new SlowFile(["extra cat"], "extra.md")]);
  assert(h.el["search-run"].disabled);
  assert.throws(() => h.search.find(), { code: "BOOK_BUSY" });
  read.resolve();
  await pending;
  assert(!h.el["search-run"].disabled);
  assert.equal(h.search.find().matches.length, 4);
});
test("failed imports also release the source-action busy state", async (t) => {
  const h = setup(t);
  await assert.rejects(h.controls.importFiles([new File([new Uint8Array([0xff])], "broken.md")]));
  assert(!h.el["search-run"].disabled);
  assert.equal(h.search.find().matches.length, 3);
});
test("in-flight export replies cannot republish pre-replacement output", async (t) => {
  const h = setup(t),
    pending = h.controls.prepare("pdf");
  const rejected = assert.rejects(pending, { code: "STALE_SOURCE" });
  await apply(h);
  h.jobs[0].resolve({ extension: "pdf", blob: () => new Blob(["old PDF"]) });
  await rejected;
  assert.equal(h.el.download.hidden, true);
  assert(h.cancels > 0);
});
test("invalid settings prevent searching stale model values", (t) => {
  const h = setup(t);
  h.el["font-scale"].value = "invalid";
  assert.throws(() => h.search.find(), { code: "INVALID_OPTIONS" });
  assert.equal(h.el["search-results"].children.length, 0);
});
test("noop and failed replacements preserve existing undo history", async (t) => {
  const h = setup(t);
  await apply(h);
  h.el["search-query"].value = "dog";
  h.el["search-replacement"].value = "dog";
  h.search.find();
  assert.throws(() => h.search.prepare(), { code: "NO_CHANGE" });
  h.search.undo();
  assert.equal(h.model.files[0].source, "cat cat");
});
test("discard invalidates an outstanding confirmation without touching source", async (t) => {
  const decision = gate(),
    h = setup(t, ["cat"], () => decision.promise);
  h.search.find();
  h.search.prepare();
  const pending = h.search.apply();
  const rejected = assert.rejects(pending, { code: "STALE_SOURCE" });
  h.el["search-discard"].click();
  decision.resolve(true);
  await rejected;
  assert.equal(h.model.files[0].source, "cat");
});
test("suspension clears results/history and fences late confirmation; resume starts empty", async (t) => {
  const decision = gate(),
    h = setup(t, ["cat"], () => decision.promise);
  h.search.find();
  h.search.prepare();
  const pending = h.search.apply(),
    rejected = assert.rejects(pending, { code: "SEARCH_CLOSED" });
  h.search.suspend();
  decision.resolve(true);
  await rejected;
  h.search.resume();
  assert.equal(h.el["search-results"].children.length, 0);
  assert(h.el["search-undo"].disabled);
  assert.equal(h.model.files[0].source, "cat");
});
test("dispose removes listeners, clears private text and prevents delayed mutation", async (t) => {
  const decision = gate(),
    h = setup(t, ["cat"], () => decision.promise);
  h.search.find();
  h.search.prepare();
  const pending = h.search.apply(),
    rejected = assert.rejects(pending);
  h.search.dispose();
  decision.resolve(true);
  await rejected;
  h.el["search-run"].click();
  assert.equal(h.el["search-results"].children.length, 0);
  assert.equal(h.model.files[0].source, "cat");
  assert(h.el["search-run"].disabled);
  assert(!h.el["search-review"].textContent.includes("cat"));
});
test("public source APIs reject stale checkpoints and invalid ranges before changing selection", (t) => {
  const h = setup(t),
    checkpoint = h.controls.checkpoint(),
    active = h.controls.currentChapter;
  h.el["chapter-source"].value = "unreported";
  assert.throws(() => h.controls.applySources(h.model.files, checkpoint), { code: "STALE_SOURCE" });
  assert.throws(() => h.controls.selectSourceRange(1, 0, 1, checkpoint), { code: "STALE_SOURCE" });
  assert.equal(h.controls.currentChapter, active);
  assert.equal(h.el["chapter-source"].value, "unreported");
  assert.throws(() => h.controls.selectSourceRange(1, 0, 999, h.controls.checkpoint()), {
    code: "INVALID_SELECTION",
  });
});
test("large-book replacement history prunes at its source-byte budget, not just batch count", async (t) => {
  const body = "x".repeat(4 * 1024 * 1024 - 10),
    h = setup(t, ["value0" + body, "value0" + body]);
  for (let i = 0; i < 5; i++) await apply(h, `value${i}`, `value${i + 1}`);
  for (let i = 0; i < 4; i++) h.search.undo();
  assert(h.model.files[0].source.startsWith("value1"));
  assert.throws(() => h.search.undo(), { code: "NO_HISTORY" });
});
