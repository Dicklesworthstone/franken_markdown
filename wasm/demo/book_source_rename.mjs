// The browser owns a reviewable transaction; only the native parser/expander
// decides which destinations and include directives need replacement.
import { bookPath } from "./book_collection.mjs";
import { prepareBookSourceRename, parseBookSourceRename } from "../book_source_rename.mjs";

const fail = (code, message) => Object.assign(new Error(message), { code });

export function createBookSourceRenamePanel(root) {
  if (root.querySelector("#book-rename-panel")) return;
  const parent = root.querySelector("#arrange-title")?.parentElement;
  if (!parent) throw Error("Missing book source editor.");
  const panel = root.createElement("fieldset"); panel.id = "book-rename-panel";
  const add = (tag, id, text, target = panel) => {
    const node = root.createElement(tag); if (id) node.id = id;
    if (text !== undefined) node.textContent = text;
    target.appendChild(node); return node;
  };
  add("legend", null, "Rename a source and update references");
  add("p", "book-rename-help", "Move the selected chapter or include-only source to a new book path. The document engine updates incoming links, outgoing relative links and include directives together while preserving currently supplied image bindings. Code examples stay literal. Prepare a rename, review affected files, then Apply; book-wide undo restores the whole transaction. Image/font keys and files on your computer are not moved.");
  add("p", "book-rename-current", "Select a source to rename.");
  const label = add("label", null, "New book-relative source path"); label.setAttribute("for", "book-rename-path");
  const input = add("input", "book-rename-path"); input.type = "text"; input.spellcheck = false;
  input.autocomplete = "off"; input.setAttribute("aria-describedby", "book-rename-help book-rename-status");
  const buttons = add("p");
  for (const [id, title] of [["prepare", "Prepare rename"], ["apply", "Apply rename"], ["cancel", "Cancel rename"]]) {
    const button = add("button", "book-rename-" + id, title, buttons); button.type = "button";
  }
  add("ul", "book-rename-changes");
  add("p", "book-rename-status", "No rename is prepared.").setAttribute("role", "status");
  parent.appendChild(panel);
}

export function createBookSourceRenameControls({ root, controls, collection, worker }) {
  createBookSourceRenamePanel(root);
  const el = Object.fromEntries(["path", "current", "prepare", "apply", "cancel", "changes", "status"]
    .map(key => [key, root.querySelector("#book-rename-" + key)]));
  if (Object.values(el).some(value => !value) || typeof worker?.renameSource !== "function"
      || typeof controls.applySourceTree !== "function") throw Error("Matching source rename controls are required.");
  let disposed = false, suspended = false, composing = false, compositionTimer = null;
  let ticket = 0, operation = null, prepared = null, installing = false;
  const removers = [];
  const active = () => !disposed && !suspended;
  const stamp = () => JSON.stringify([collection.sourceProjectRevision, controls.checkpoint(),
    [...collection.imageDestinations].sort()]);
  const check = () => {
    if (!active()) throw fail("RENAME_CLOSED", "Source rename is not active.");
    if (controls.sourceBusy || composing) throw fail("BOOK_BUSY", "Finish importing or composing text before renaming a source.");
  };
  function refresh() {
    if (disposed) return;
    const file = collection.files[controls.currentChapter], blocked = !active() || controls.sourceBusy || composing;
    el.current.textContent = file ? "Selected source: " + file.path : "Select a source to rename.";
    el.prepare.disabled = blocked || !file || !!operation || !el.path.value;
    el.apply.disabled = blocked || !prepared || !!operation;
    el.cancel.disabled = !active() || (!operation && !prepared);
    el.path.disabled = !active() || !file;
  }
  function retire(message) {
    ticket++; operation = null; prepared = null; worker.cancel();
    el.changes.replaceChildren();
    if (message && !disposed) el.status.textContent = message;
    refresh();
  }
  function observe() {
    if (!active() || installing) return;
    const owned = operation ?? prepared;
    if (owned && (controls.sourceBusy || composing || owned.stamp !== stamp() || owned.path !== el.path.value)) {
      retire("The book or destination changed. Prepare a fresh rename; no prepared source edits were applied.");
    } else refresh();
  }
  async function prepare() {
    check(); retire();
    const nextPath = bookPath(el.path.value), project = controls.captureProject();
    check();
    const selected = project.files[controls.currentChapter];
    if (!selected) throw fail("UNKNOWN_BOOK_SOURCE", "Select a source in this book.");
    if (selected.role !== "include" && !/\.(md|markdown)$/i.test(nextPath)) {
      throw fail("INVALID_PATH", "Published chapter paths must end in .md or .markdown.");
    }
    const chapters = project.files.filter(file => file.role !== "include");
    const options = { profile: project.options.profile, imageDestinations: collection.imageDestinations,
      includeSources: project.files.filter(file => file.role === "include") };
    const input = prepareBookSourceRename(chapters, selected.path, nextPath, options);
    // A Worker-compatible host can run code while capturing the request. Keep
    // receipt authority in separate containers it never receives.
    const expected = { oldPath: input.oldPath, newPath: input.newPath,
      files: input.files.map(file => ({ ...file })),
      options: { includeSources: input.options.includeSources.map(file => ({ ...file })),
        imageDestinations: Object.freeze([...input.options.imageDestinations]) } };
    const job = { id: ++ticket, stamp: stamp(), checkpoint: controls.checkpoint(), path: el.path.value,
      files: project.files, input: expected };
    operation = job; refresh();
    el.status.textContent = "Preparing the complete source rename with the document engine…";
    try {
      const output = await worker.renameSource(input.files, input.oldPath, input.newPath, input.options);
      if (operation !== job || job.id !== ticket || !active()) return null;
      check();
      if (job.stamp !== stamp() || job.path !== el.path.value) {
        retire("The book or destination changed. Prepare a fresh rename; no source edits were applied."); return null;
      }
      const report = parseBookSourceRename(output.bytes, job.input);
      let chapter = 0, include = 0;
      const files = job.files.map(file => file.role === "include"
        ? { ...report.includeSources[include++], role: "include" } : { ...report.files[chapter++] });
      prepared = { ...job, report, result: files }; operation = null;
      el.changes.replaceChildren();
      for (let i = 0; i < files.length; i++) {
        const before = job.files[i], after = files[i];
        if (before.path === after.path && before.source === after.source) continue;
        const row = root.createElement("li");
        row.textContent = (before.path === after.path ? before.path : before.path + " → " + after.path)
          + (before.source === after.source ? " (path only)" : " (source references updated)");
        el.changes.appendChild(row);
      }
      el.status.textContent = report.changedSourceCount
        ? `Ready to rename ${report.oldPath} to ${report.newPath}: ${report.changedSourceCount} source file(s), ${report.editCount} reference edit(s). Review the affected paths, then Apply rename.`
        : "The source already has that path. No changes are needed.";
      refresh(); return report;
    } catch (error) {
      if (operation !== job || !active()) return null;
      operation = null; prepared = null; refresh();
      el.status.textContent = `${error.code ?? "BOOK_RENAME_FAILED"}: ${error.message ?? "Rename failed."} No source edits were applied.`;
      throw error;
    }
  }
  function apply() {
    check();
    const plan = prepared;
    if (!plan || plan.stamp !== stamp() || plan.path !== el.path.value) {
      retire(); throw fail("STALE_SOURCE", "Prepare a rename for the current book and destination before applying it.");
    }
    prepared = null; installing = true;
    let revision;
    try { revision = controls.applySourceTree(plan.result, plan.checkpoint); }
    finally { installing = false; refresh(); }
    const now = collection.files;
    const exact = revision === collection.revision && now.length === plan.result.length
      && now.every((file, i) => file.path === plan.result[i].path && file.source === plan.result[i].source
        && (file.role ?? "chapter") === (plan.result[i].role ?? "chapter"));
    el.changes.replaceChildren(); el.path.value = "";
    el.status.textContent = exact
      ? `Renamed ${plan.report.oldPath} to ${plan.report.newPath} and updated ${plan.report.editCount} reference(s) in one source transaction. Book-wide undo can restore it; save the book to retain it.`
      : "The rename was applied, then the book changed again. Review current source before saving or publishing.";
    refresh(); return revision;
  }
  function invoke(fn) {
    const rejected = error => {
      if (active()) el.status.textContent = `${error.code ?? "BOOK_RENAME_FAILED"}: ${error.message ?? "Rename failed."}`;
    };
    try { Promise.resolve(fn()).catch(rejected); } catch (error) { rejected(error); }
  }
  const listen = (node, type, fn) => {
    node.addEventListener(type, fn); removers.push(() => node.removeEventListener(type, fn));
  };
  listen(el.prepare, "click", () => invoke(prepare));
  listen(el.apply, "click", () => invoke(apply));
  listen(el.cancel, "click", () => retire("Rename cancelled. Source is unchanged."));
  listen(el.path, "input", observe);
  listen(el.path, "compositionstart", () => {
    clearTimeout(compositionTimer); compositionTimer = null; composing = true; observe();
  });
  // Some browsers deliver final input after compositionend. Keep the operation
  // fenced through that task, as the source host does for its editor fields.
  listen(el.path, "compositionend", () => {
    clearTimeout(compositionTimer);
    compositionTimer = setTimeout(() => { compositionTimer = null; composing = false; observe(); }, 0);
  });
  for (const id of ["chapter-source", "chapter-path", "chapters", "source-role"]) {
    const node = root.querySelector("#" + id);
    if (node) for (const type of ["input", "change", "compositionstart"]) listen(node, type, observe);
  }
  const unsubscribe = collection.subscribe(observe), unsubscribeBusy = controls.subscribeSourceState(observe);
  refresh();
  return Object.freeze({ prepare, apply,
    cancel() { retire("Rename cancelled. Source is unchanged."); },
    suspend() { if (!disposed) { suspended = true; clearTimeout(compositionTimer); compositionTimer = null;
      composing = false; retire("Rename cancelled while the page was inactive."); } },
    resume() { if (!disposed) { suspended = false; refresh(); } },
    dispose() { if (disposed) return; disposed = true; clearTimeout(compositionTimer); compositionTimer = null;
      composing = false; retire(); unsubscribe(); unsubscribeBusy();
      for (const remove of removers) remove(); worker.dispose(); },
  });
}
