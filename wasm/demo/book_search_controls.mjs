import { bookError } from "../book_worker.mjs";
import { findBookSource, planBookReplacement } from "./book_source_search.mjs";

const PAGE = 50,
  HISTORY_BYTES = 64 * 1024 * 1024,
  HISTORY_ENTRIES = 10;
const clip = (value) => JSON.stringify(value.slice(0, 200)) + (value.length > 200 ? "…" : "");

/** Source find/replace with an explicit review and bounded session-only batch
 * history. This is independent of the textarea's native typing undo manager.
 * It never restores a source project, persists text, or retains image bytes.
 */
export function createBookSearchControls({ root, controls, collection, confirm = () => false }) {
  const ids = [
    "search-query",
    "search-replacement",
    "search-case",
    "search-scope",
    "search-run",
    "search-results",
    "search-open",
    "search-previous",
    "search-next",
    "search-page-previous",
    "search-page-next",
    "search-review-one",
    "search-review-all",
    "search-apply",
    "search-discard",
    "search-review",
    "search-status",
    "search-undo",
    "search-redo",
    "search-history",
  ];
  const el = Object.fromEntries(ids.map((id) => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some((value) => !value))
    throw new Error("Missing book source search controls.");
  let disposed = false,
    suspended = false,
    composing = false,
    installing = false;
  let search = null,
    stamp = null,
    queryStamp = null,
    review = null,
    selected = 0,
    page = 0,
    serial = 0,
    confirming = null;
  let past = [],
    future = [],
    expectedRevision = collection.revision;
  const listeners = [];
  const queryKey = () =>
    JSON.stringify([el["search-query"].value, el["search-case"].checked, el["search-scope"].value]);
  const busy = () =>
    disposed || suspended || composing || confirming !== null || controls.sourceBusy;
  const alive = () => {
    if (disposed || suspended) throw bookError("SEARCH_CLOSED", "Source search is not active.");
  };
  function admitted() {
    alive();
    if (composing || controls.sourceBusy || confirming !== null)
      throw bookError(
        "BOOK_BUSY",
        "Finish the current import, text composition or confirmation first.",
      );
  }
  function buttons() {
    const blocked = busy(),
      count = search?.matches.length ?? 0;
    el["search-run"].disabled = blocked;
    for (const id of ["search-results", "search-open", "search-review-one", "search-review-all"])
      el[id].disabled = blocked || !count;
    el["search-previous"].disabled = blocked || !count || selected === 0;
    el["search-next"].disabled = blocked || !count || selected >= count - 1;
    el["search-page-previous"].disabled = blocked || !count || page === 0;
    el["search-page-next"].disabled = blocked || !count || (page + 1) * PAGE >= count;
    el["search-apply"].disabled = blocked || !review;
    el["search-discard"].disabled = !review;
    el["search-undo"].disabled = blocked || !past.length;
    el["search-redo"].disabled = blocked || !future.length;
    el["search-history"].textContent =
      `${past.length} replacement batches can be undone; ${future.length} can be redone. Other source/settings/asset edits clear this session history.`;
  }
  function discard() {
    serial++;
    confirming = null;
    review = null;
    el["search-review"].textContent = "No replacement is prepared.";
  }
  function clearSearch() {
    discard();
    search = null;
    stamp = queryStamp = null;
    selected = page = 0;
    el["search-results"].replaceChildren();
  }
  function clearHistory() {
    past = [];
    future = [];
    expectedRevision = collection.revision;
  }
  function changed() {
    if (disposed) return;
    clearSearch();
    if (!installing) clearHistory();
    el["search-status"].textContent =
      "The book changed. Search results and any pending replacement were cleared.";
    buttons();
  }
  function report(error) {
    if (!disposed) {
      el["search-status"].textContent =
        `${error.code ?? "SEARCH_ERROR"}: ${error.message ?? "Source operation failed."}`;
      buttons();
    }
  }
  function invoke(work) {
    try {
      Promise.resolve(work()).catch(report);
    } catch (error) {
      report(error);
    }
  }
  function showResults() {
    const count = search?.matches.length ?? 0;
    el["search-results"].replaceChildren(
      ...(search?.matches.slice(page * PAGE, (page + 1) * PAGE) ?? []).map((match, i) => {
        const option = root.createElement("option"),
          file = search.files[match.chapter];
        option.value = String(page * PAGE + i);
        option.textContent = `${page * PAGE + i + 1}. ${file.path}:${match.line}:${match.column} — ${clip(file.source.slice(match.start, Math.min(file.source.length, match.end + 60)))}`;
        return option;
      }),
    );
    el["search-results"].value = String(selected);
    el["search-status"].textContent = count
      ? `${count} literal source matches. Showing ${page * PAGE + 1}–${Math.min((page + 1) * PAGE, count)}; selected ${selected + 1}. Rendered output is not searched.`
      : "No matches in the selected source scope.";
    buttons();
  }
  function find() {
    admitted();
    clearSearch();
    const project = controls.captureProject(),
      scope = el["search-scope"].value;
    if (scope !== "all" && scope !== "current")
      throw bookError("INVALID_SEARCH", "Choose the whole book or current chapter.");
    search = findBookSource(project, el["search-query"].value, {
      matchCase: el["search-case"].checked,
      chapter: scope === "all" ? null : controls.currentChapter,
    });
    stamp = controls.checkpoint();
    queryStamp = queryKey();
    showResults();
    return search;
  }
  function current() {
    admitted();
    if (!search || stamp !== controls.checkpoint() || queryStamp !== queryKey()) {
      clearSearch();
      buttons();
      throw bookError(
        "STALE_SOURCE",
        "The source or search options changed. Run the search again.",
      );
    }
  }
  function open(index = selected) {
    current();
    if (!Number.isInteger(index) || index < 0 || index >= search.matches.length)
      throw bookError("INVALID_SELECTION", "Choose an existing search result.");
    const match = search.matches[index];
    controls.selectSourceRange(match.chapter, match.start, match.end, stamp);
    stamp = controls.checkpoint();
    selected = index;
    page = Math.floor(index / PAGE);
    // Navigating within the same immutable search does not invalidate a review.
    if (review) review.stamp = stamp;
    showResults();
  }
  function prepare(all = true) {
    current();
    discard();
    const plan = planBookReplacement(search, el["search-replacement"].value, all ? null : selected);
    review = { plan, stamp, query: queryKey(), replacement: el["search-replacement"].value };
    const lines = [
      `Review ${plan.count} changes in ${plan.chapters.length} chapters. Nothing has changed yet.`,
      `Find: ${clip(search.query)}`,
      `Replace with: ${clip(review.replacement)}`,
      `Source bytes: ${plan.beforeBytes} → ${plan.afterBytes}`,
    ];
    for (const chapter of plan.chapters) {
      const before = plan.before[chapter.index].source,
        after = plan.after[chapter.index].source;
      let offset = 0;
      while (offset < before.length && offset < after.length && before[offset] === after[offset])
        offset++;
      const start = Math.max(0, offset - 40);
      lines.push(
        `\n${chapter.path}: ${chapter.count} changes (${chapter.beforeBytes} → ${chapter.afterBytes} bytes)`,
        `Before (first change): ${clip(before.slice(start, offset + 160))}`,
        `After (first change): ${clip(after.slice(start, offset + 160))}`,
      );
    }
    el["search-review"].textContent = lines.join("\n");
    buttons();
    return plan;
  }
  function install(files, checkpoint) {
    installing = true;
    try {
      const revision = controls.applySources(files, checkpoint);
      if (collection.revision !== revision) {
        clearHistory();
        throw bookError(
          "STALE_SOURCE",
          "Another edit followed the source transaction; replacement history was cleared.",
        );
      }
      expectedRevision = revision;
    } finally {
      installing = false;
    }
  }
  function trimHistory() {
    let bytes = past.reduce((total, plan) => total + plan.beforeBytes + plan.afterBytes, 0);
    while (past.length > HISTORY_ENTRIES || bytes > HISTORY_BYTES) {
      const removed = past.shift();
      bytes -= removed.beforeBytes + removed.afterBytes;
    }
  }
  async function apply() {
    admitted();
    const captured = review;
    const check = () => {
      alive();
      if (
        !captured ||
        review !== captured ||
        controls.sourceBusy ||
        composing ||
        captured.stamp !== controls.checkpoint() ||
        captured.query !== queryKey() ||
        captured.replacement !== el["search-replacement"].value
      ) {
        throw bookError(
          "STALE_SOURCE",
          "The reviewed source or replacement changed; nothing was applied. Review again.",
        );
      }
    };
    check();
    const ticket = ++serial;
    confirming = ticket;
    buttons();
    try {
      const accepted = await confirm(
        `Apply the reviewed ${captured.plan.count} source changes across ${captured.plan.chapters.length} chapters? This edits Markdown, including code and link text. Undo replacement is available until another edit.`,
      );
      alive();
      if (ticket !== serial)
        throw bookError(
          "STALE_SOURCE",
          "The pending replacement was cancelled or changed. Nothing was applied.",
        );
      check();
      if (accepted !== true) return false;
      install(captured.plan.after, captured.stamp);
      past.push(captured.plan);
      future = [];
      trimHistory();
      el["search-status"].textContent =
        `Applied ${captured.plan.count} changes in ${captured.plan.chapters.length} chapters atomically. Images were preserved; previous exports and previews are stale. Use Undo replacement to reverse this batch.`;
      return true;
    } catch (error) {
      if (review === captured) discard();
      throw error;
    } finally {
      if (confirming === ticket) confirming = null;
      buttons();
    }
  }
  function historyStep(redo) {
    admitted();
    // Capture previously undispatched keystrokes first; they must invalidate
    // history, never be overwritten by it. Chapter navigation alone is safe.
    const revision = expectedRevision;
    controls.captureProject();
    if (revision !== collection.revision) {
      clearHistory();
      buttons();
      throw bookError("STALE_SOURCE", "Newer edits prevent replacement undo/redo.");
    }
    const from = redo ? future : past,
      to = redo ? past : future,
      plan = from.at(-1);
    if (!plan)
      throw bookError("NO_HISTORY", "No replacement batch is available in that direction.");
    install(redo ? plan.after : plan.before, controls.checkpoint());
    from.pop();
    to.push(plan);
    el["search-status"].textContent =
      `${redo ? "Redid" : "Undid"} ${plan.count} source changes as one batch. Images and presentation settings are unchanged.`;
    buttons();
  }
  function on(target, type, handler) {
    target.addEventListener(type, handler);
    listeners.push(() => target.removeEventListener(type, handler));
  }
  const unsubscribe = collection.subscribe(changed),
    unsubscribeSource = controls.subscribeSourceState(buttons);
  for (const id of [
    "chapter-source",
    "chapter-path",
    "title",
    "author",
    "lang",
    "font",
    "dark-mode",
    "font-scale",
    "toc",
    "page-numbers",
  ]) {
    const input = root.querySelector(`#${id}`);
    on(input, "input", changed);
    on(input, "change", changed);
  }
  on(root.querySelector("#chapters"), "change", () => {
    clearSearch();
    buttons();
  });
  for (const id of ["chapter-source", "search-query", "search-replacement"]) {
    const input = root.querySelector(`#${id}`);
    on(input, "compositionstart", () => {
      composing = true;
      discard();
      buttons();
    });
    on(input, "compositionend", () => {
      composing = false;
      buttons();
    });
  }
  for (const id of ["search-query", "search-case", "search-scope"]) {
    for (const event of ["input", "change"])
      on(el[id], event, () => {
        clearSearch();
        buttons();
      });
  }
  for (const event of ["input", "change"])
    on(el["search-replacement"], event, () => {
      discard();
      buttons();
    });
  on(el["search-run"], "click", () => invoke(find));
  on(el["search-results"], "change", () => invoke(() => open(Number(el["search-results"].value))));
  on(el["search-open"], "click", () => invoke(() => open()));
  on(el["search-previous"], "click", () => invoke(() => open(selected - 1)));
  on(el["search-next"], "click", () => invoke(() => open(selected + 1)));
  for (const [id, delta] of [
    ["search-page-previous", -1],
    ["search-page-next", 1],
  ])
    on(el[id], "click", () =>
      invoke(() => {
        current();
        const next = page + delta;
        if (next >= 0 && next * PAGE < search.matches.length) {
          page = next;
          selected = page * PAGE;
          showResults();
        }
      }),
    );
  on(el["search-review-one"], "click", () => invoke(() => prepare(false)));
  on(el["search-review-all"], "click", () => invoke(() => prepare(true)));
  on(el["search-apply"], "click", () => invoke(apply));
  on(el["search-discard"], "click", () => {
    discard();
    buttons();
  });
  on(el["search-undo"], "click", () => invoke(() => historyStep(false)));
  on(el["search-redo"], "click", () => invoke(() => historyStep(true)));
  clearSearch();
  buttons();
  return Object.freeze({
    find,
    open,
    prepare,
    apply,
    undo: () => historyStep(false),
    redo: () => historyStep(true),
    suspend() {
      if (disposed) return;
      suspended = true;
      composing = false;
      clearSearch();
      clearHistory();
      buttons();
    },
    resume() {
      if (disposed) return;
      suspended = false;
      buttons();
    },
    dispose() {
      if (disposed) return;
      clearSearch();
      clearHistory();
      disposed = true;
      unsubscribe();
      unsubscribeSource();
      for (const remove of listeners) remove();
      buttons();
    },
  });
}
