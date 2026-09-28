import { bookError } from "../book_worker.mjs";
import { normalizePdfPage } from "../pdf_page.mjs";
import {
  createBookCollection,
  normalizeBookProject,
  readBookFiles,
  readBookProject,
  readPortableBookProject,
  discardPortableBookProject,
} from "./book_collection.mjs";
import { bookEditorOffset } from "./book_source_search.mjs";

const DEFAULT_CONFIRMATION = () => true;

/** UI orchestration is separately testable; the host supplies the real worker.
 * Nothing in this controller parses Markdown or inserts rendered HTML.
 */
export function createBookControls({
  root,
  worker,
  confirm = DEFAULT_CONFIRMATION,
  urls = URL,
  collection = createBookCollection(),
  onProjectReplaced = () => {},
}) {
  const ids = [
    "source-role",
    "add-include",
    "import-includes",
    "import-include-folder",
    "chapters",
    "chapter-path",
    "chapter-source",
    "add-chapter",
    "move-up",
    "move-down",
    "remove-chapter",
    "import-files",
    "import-folder",
    "open-project",
    "save-project",
    "save-chapter",
    "revoke-images",
    "image-list",
    "title",
    "author",
    "lang",
    "font",
    "dark-mode",
    "font-scale",
    "toc",
    "page-numbers",
    "export-pdf",
    "export-epub",
    "export-site",
    "cancel-export",
    "download",
    "status",
  ];
  const el = Object.fromEntries(ids.map((id) => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some((value) => !value))
    throw new Error("Book controls are missing required elements.");
  installPortableBookPanel(root);
  for (const id of ["save-portable", "open-portable", "cancel-portable", "portable-review"])
    el[id] = root.querySelector(`#${id}`);
  const unlisten = [],
    sourceListeners = new Set();
  let pageSetup = null;
  let portableAbort = null;
  let lastSourceBusy = false;
  let composing = false;
  let disposed = false,
    active = 0,
    generation = 0,
    reading = false,
    preparing = false,
    url = null,
    published = null;
  const alive = () => {
    if (disposed) throw bookError("SESSION_DISPOSED", "Book controls are disposed.");
  };
  const options = () => ({
    ...collection.options,
    title: el.title.value,
    author: el.author.value,
    lang: el.lang.value,
    font: el.font.value,
    darkMode: el["dark-mode"].value,
    fontScale: Number(el["font-scale"].value),
    toc: el.toc.checked,
    pageNumbers: el["page-numbers"].checked,
  });
  const signature = () =>
    JSON.stringify([
      active,
      el["chapter-path"].value,
      el["chapter-source"].value,
      options(),
      el["font-scale"].value,
      el["source-role"].value,
    ]);
  function buttons() {
    const files = collection.files,
      count = files.length;
    const hasChapters = files.some((file) => file.role !== "include");
    el["move-up"].disabled = active <= 0;
    el["move-down"].disabled = active >= count - 1;
    el["remove-chapter"].disabled = el["save-chapter"].disabled = count === 0;
    el["chapter-path"].disabled = el["chapter-source"].disabled = count === 0;
    el["source-role"].disabled = count === 0 || composing || reading;
    el["revoke-images"].disabled = collection.images.length === 0;
    for (const kind of ["pdf", "epub", "site"])
      el[`export-${kind}`].disabled = !hasChapters || preparing || reading;
    for (const kind of [
      "import-files",
      "import-folder",
      "import-includes",
      "import-include-folder",
      "open-project",
    ])
      el[kind].disabled = reading;
    el["cancel-export"].disabled = !preparing && !(reading && portableAbort);
    if (el["save-portable"]) el["save-portable"].disabled = reading || composing || preparing;
    if (el["open-portable"]) el["open-portable"].disabled = reading || composing;
    if (el["cancel-portable"]) el["cancel-portable"].disabled = portableAbort === null;
    pageSetup?.refresh();
    const nextBusy = reading || composing;
    if (lastSourceBusy !== nextBusy) {
      lastSourceBusy = nextBusy;
      for (const listener of sourceListeners) {
        try {
          listener();
        } catch {
          /* Observers cannot block editing. */
        }
      }
    }
  }
  function revoke() {
    published = null;
    el.download.hidden = true;
    el.download.removeAttribute("href");
    el.download.removeAttribute("download");
    if (url !== null) {
      const previous = url;
      url = null;
      urls.revokeObjectURL(previous);
    }
  }
  function invalidate() {
    portableAbort?.abort();
    portableAbort = null;
    if (el["portable-review"]) el["portable-review"].textContent = "No portable restore is under review.";
    generation++;
    preparing = false;
    worker.cancel();
    revoke();
    if (!disposed) buttons();
  }
  function report(error) {
    if (!disposed)
      el.status.textContent = `${error.code ?? "BOOK_ERROR"}: ${error.message ?? "Operation failed."} Source remains in this workbench.`;
  }
  function list() {
    const selected = active;
    let chapterNumber = 0;
    el.chapters.replaceChildren(
      ...collection.files.map((file, index) => {
        const item = root.createElement("option");
        item.value = String(index);
        item.textContent =
          file.role === "include" ? `[include] ${file.path}` : `${++chapterNumber}. ${file.path}`;
        return item;
      }),
    );
    el.chapters.value = String(selected);
    el["image-list"].textContent =
      collection.images.map((image) => `${image.destination} (${image.size} bytes)`).join("\n") ||
      "No images authorized.";
    buttons();
  }
  function showChapter() {
    const file = collection.files[active];
    el["source-role"].value = file?.role ?? "chapter";
    el["chapter-path"].value = file?.path ?? "";
    el["chapter-source"].value = file?.source ?? "";
    list();
  }
  function showOptions() {
    const value = collection.options;
    for (const key of ["title", "author", "lang", "font"]) el[key].value = value[key];
    el["dark-mode"].value = value.darkMode;
    el["font-scale"].value = String(value.fontScale);
    el.toc.checked = value.toc;
    el["page-numbers"].checked = value.pageNumbers;
    pageSetup?.discard();
  }
  function capture(configure = true) {
    alive();
    if (collection.files.length) {
      collection.edit(active, el["chapter-path"].value, el["chapter-source"].value);
      collection.setRole(active, el["source-role"].value);
    }
    const next = options();
    if (configure && JSON.stringify(next) !== JSON.stringify(collection.options))
      collection.configure(next);
  }
  function publish(output, ticket, revision, view) {
    if (
      disposed ||
      ticket !== generation ||
      revision !== collection.revision ||
      view !== signature()
    ) {
      throw bookError("STALE_SOURCE", "The collection changed; this output was not published.");
    }
    revoke();
    url = urls.createObjectURL(output.blob);
    published = { revision, view };
    el.download.href = url;
    el.download.download = output.filename;
    el.download.textContent = `Download ${output.filename}`;
    el.download.hidden = false;
    el.status.textContent = `Prepared ${output.filename}. Click Download to save it. Preparation alone has not saved any file.`;
  }
  async function prepare(format) {
    let ticket = null;
    try {
      alive();
      if (reading) throw bookError("BOOK_BUSY", "A local import is in progress.");
      invalidate();
      capture();
      const input = collection.snapshot(),
        revision = collection.revision,
        view = signature();
      ticket = ++generation;
      preparing = true;
      buttons();
      el.status.textContent = `Preparing ${format.toUpperCase()} in the worker. Editing or cancelling terminates this export.`;
      const result = await worker.render(input.files, format, input.options);
      const name =
        (input.options.title
          .replace(/[^a-z0-9_-]+/gi, "-")
          .replace(/^-+|-+$/g, "")
          .slice(0, 80) || "book") +
        "." +
        result.extension;
      publish({ filename: name, blob: result.blob() }, ticket, revision, view);
    } catch (error) {
      if (ticket === null || ticket === generation) report(error);
      throw error;
    } finally {
      if (!disposed && ticket === generation) {
        preparing = false;
        buttons();
      }
    }
  }

  async function preparePortable() {
    let ticket = null, job = null;
    try {
      alive();
      if (reading || composing)
        throw bookError("BOOK_BUSY", "Finish importing or composing text before preparing a portable backup.");
      invalidate();
      capture();
      const revision = collection.revision, view = signature();
      ticket = ++generation;
      job = new AbortController();
      portableAbort = job;
      preparing = true;
      buttons();
      el.status.textContent = "Preparing a portable backup with the current images and fonts. Nothing is uploaded or saved automatically.";
      const output = await collection.portableDownload({ signal: job.signal });
      publish(output, ticket, revision, view);
    } catch (error) {
      if (ticket === null || ticket === generation) report(error);
      throw error;
    } finally {
      if (portableAbort === job) portableAbort = null;
      if (!disposed && ticket === generation) { preparing = false; buttons(); }
    }
  }

  function prepareSource(project) {
    alive();
    invalidate();
    capture(project);
    const output = project ? collection.projectDownload() : collection.chapterDownload(active);
    publish(output, generation, collection.revision, signature());
  }
  async function importFiles(files, mode = "files") {
    alive();
    if (reading) throw bookError("BOOK_BUSY", "A local import is already in progress.");
    if (mode === "portable" && (composing || files.length !== 1))
      throw bookError("BOOK_BUSY", "Finish composing text and choose one portable book file.");
    if (mode === "portable" && confirm === DEFAULT_CONFIRMATION)
      throw bookError("RESOURCE_AUTHORIZATION_REQUIRED", "Portable restore requires an explicit confirmation handler.");
    capture();
    invalidate();
    const revision = collection.revision,
      view = signature(),
      job = mode === "portable" ? new AbortController() : null;
    let review = null;
    portableAbort = job;
    reading = true;
    buttons();
    const fence = () => {
      alive();
      if (job?.signal.aborted)
        throw bookError("ABORTED", "Portable import cancelled; nothing was installed.");
      if (revision !== collection.revision || view !== signature() || (job && composing))
        throw bookError(
          "STALE_SOURCE",
          "The collection changed during import; nothing was installed.",
        );
    };
    try {
      if (mode === "project" || mode === "portable") {
        const project = job
          ? (review = await readPortableBookProject(files[0], { signal: job.signal }))
          : await readBookProject(files[0]);
        fence();
        if (review && el["portable-review"]) {
          el["portable-review"].textContent = portableReviewText(review);
        }
        const message = review
          ? `Replace this entire collection with "${review.title}"? Save the current book first. Restore ${review.chapters} chapters and ${review.includeSources} include-only sources, and authorize ${review.images.length} embedded images (${review.imageBytes} bytes) and ${review.fonts.length} embedded font roles (${review.fontBytes} bytes) for this session? Current resources will be replaced. Only approve files and resource rights you trust.`
          : "Replace this collection with the source project? Save the current project first. All current image access will be revoked.";
        const approved = job
          ? await portableApproval(confirm(message), job.signal)
          : await confirm(message);
        fence();
        if (approved !== true) {
          if (job) el.status.textContent = "Portable restore declined. The current collection is unchanged.";
          return false;
        }
        if (job) collection.replacePortableProject(project, revision, { authorizeResources: true });
        else collection.replaceProject(project);
        active = 0;
        showOptions();
        showChapter();
        onProjectReplaced();
        el.status.textContent = job
          ? "Portable book restored. Sources, settings, embedded images and font roles are installed. Prepare a new preview or publication to validate them with the engine."
          : "Source project reopened. Source roles, chapter order and text are restored; reauthorize its image files before exporting.";
      } else {
        const batch = await readBookFiles(files, {
          folder: mode === "folder" || mode === "include-folder",
          role: mode === "includes" || mode === "include-folder" ? "include" : "chapter",
        });
        fence();
        if (batch.chapters.length || batch.includeSources?.length || batch.images.length)
          collection.append(batch);
        showChapter();
        el.status.textContent = `Imported ${batch.chapters.length} chapters, ${batch.includeSources?.length ?? 0} include-only sources and ${batch.images.length} images; ignored ${batch.ignored} other files. Check chapter order and relative paths before publishing.`;
      }
      return true;
    } finally {
      discardPortableBookProject(review);
      if (portableAbort === job) portableAbort = null;
      reading = false;
      if (!disposed) buttons();
    }
  }
  function invoke(work) {
    try {
      Promise.resolve(work()).catch(report);
    } catch (error) {
      report(error);
    }
  }
  function on(id, type, handler) {
    el[id].addEventListener(type, handler);
    unlisten.push(() => el[id].removeEventListener(type, handler));
  }
  const unsubscribe = collection.subscribe(() => {
    invalidate();
    list();
  });
  on("chapter-source", "compositionstart", () => {
    composing = true;
    if (portableAbort) invalidate();
    buttons();
  });
  on("chapter-source", "compositionend", () => {
    composing = false;
    buttons();
  });
  on("chapter-source", "input", () => invoke(capture));
  on("chapter-path", "input", () => invoke(capture));
  on("source-role", "change", () =>
    invoke(() => {
      invalidate();
      try {
        capture();
      } finally {
        el["source-role"].value = collection.files[active]?.role ?? "chapter";
      }
    }),
  );
  for (const key of [
    "title",
    "author",
    "lang",
    "font",
    "dark-mode",
    "font-scale",
    "toc",
    "page-numbers",
  ]) {
    on(
      key,
      key === "toc" || key === "page-numbers" || key === "font" || key === "dark-mode"
        ? "change"
        : "input",
      () => {
        invalidate();
        invoke(capture);
      },
    );
  }
  on("chapters", "change", () =>
    invoke(() => {
      const next = Number(el.chapters.value);
      capture();
      active = next;
      showChapter();
    }),
  );
  on("add-chapter", "click", () =>
    invoke(() => {
      capture();
      const used = new Set(collection.files.map((file) => file.path));
      let n = 1;
      while (used.has(`chapter-${n}.md`)) n++;
      collection.append({
        chapters: [{ path: `chapter-${n}.md`, source: `# Chapter ${n}\n\n` }],
        images: [],
      });
      active = collection.files.length - 1;
      showChapter();
    }),
  );
  on("add-include", "click", () =>
    invoke(() => {
      capture();
      const used = new Set(collection.files.map((file) => file.path));
      let n = 1;
      while (used.has(`include-${n}.md`)) n++;
      collection.append({
        chapters: [],
        images: [],
        includeSources: [{ path: `include-${n}.md`, source: "" }],
      });
      active = collection.files.length - 1;
      showChapter();
    }),
  );
  for (const [id, delta] of [
    ["move-up", -1],
    ["move-down", 1],
  ])
    on(id, "click", () =>
      invoke(() => {
        capture();
        active = collection.move(active, delta);
        showChapter();
      }),
    );
  on("remove-chapter", "click", () =>
    invoke(async () => {
      capture();
      const revision = collection.revision,
        view = signature(),
        index = active;
      if (
        (await confirm(
          "Remove this source from the collection? Download it first to keep edits. Includes that refer to it may stop resolving.",
        )) !== true
      )
        return;
      if (disposed || revision !== collection.revision || view !== signature())
        throw bookError(
          "STALE_SOURCE",
          "The source changed during confirmation; it was not removed.",
        );
      collection.remove(index);
      active = Math.max(0, Math.min(index, collection.files.length - 1));
      showChapter();
    }),
  );
  on("revoke-images", "click", () =>
    invoke(() => {
      collection.revokeImages();
      el.status.textContent = "All image access revoked and prepared exports invalidated.";
    }),
  );
  for (const [id, mode] of [
    ["import-files", "files"],
    ["import-folder", "folder"],
    ["import-includes", "includes"],
    ["import-include-folder", "include-folder"],
    ["open-project", "project"],
    ["open-portable", "portable"],
  ].filter(([id]) => el[id]))
    on(id, "change", () => {
      const files = Array.from(el[id].files ?? []);
      if (!files.length) return;
      invoke(async () => {
        try {
          await importFiles(files, mode);
        } finally {
          el[id].value = "";
        }
      });
    });
  if (el["save-portable"]) on("save-portable", "click", () => invoke(preparePortable));
  if (el["cancel-portable"]) on("cancel-portable", "click", () => {
    invalidate();
    el.status.textContent = "Portable operation cancelled. Source remains editable.";
  });
  on("save-project", "click", () => invoke(() => prepareSource(true)));
  on("save-chapter", "click", () => invoke(() => prepareSource(false)));
  for (const format of ["pdf", "epub", "site"])
    on(`export-${format}`, "click", () => {
      void prepare(format).catch(() => {});
    });
  on("cancel-export", "click", () => {
    invalidate();
    el.status.textContent = "Export cancelled. All source remains editable.";
  });
  on("download", "click", (event) => {
    if (
      disposed ||
      !published ||
      published.revision !== collection.revision ||
      published.view !== signature()
    ) {
      event.preventDefault();
      if (!disposed) {
        invalidate();
        el.status.textContent = "Output is stale. Prepare it again from the current collection.";
      }
    }
  });
  showOptions();
  showChapter();
  revoke();
  try {
    pageSetup = createBookPageControls({ root, collection, capture: () => capture(),
      isBusy: () => reading || composing });
  } catch {
    el.status.textContent = "PDF page setup is unavailable. Existing book settings, source editing and exports remain available.";
  }
  return Object.freeze({
    prepare,
    preparePortable,
    prepareSource,
    importFiles,
    checkpoint() {
      alive();
      return JSON.stringify([collection.revision, signature()]);
    },
    captureProject() {
      capture();
      return collection.project();
    },
    get currentChapter() {
      alive();
      return active;
    },
    get sourceBusy() {
      alive();
      return reading || composing;
    },
    subscribeSourceState(listener) {
      alive();
      sourceListeners.add(listener);
      return () => sourceListeners.delete(listener);
    },
    /** Publish a validated source-only transaction without restoring a project
     * or changing image authority. Every observer sees one complete revision. */
    applySources(files, checkpoint) {
      alive();
      if (reading || composing)
        throw bookError(
          "BOOK_BUSY",
          "Finish importing or composing text before changing book source.",
        );
      if (checkpoint !== JSON.stringify([collection.revision, signature()]))
        throw bookError("STALE_SOURCE", "The editor changed; no source transaction was installed.");
      const editor = el["chapter-source"],
        start = editor.selectionStart,
        end = editor.selectionEnd,
        scroll = editor.scrollTop;
      const revision = collection.replaceSources(files, collection.revision);
      showChapter();
      if (Number.isInteger(start) && Number.isInteger(end))
        editor.setSelectionRange(
          Math.min(start, editor.value.length),
          Math.min(end, editor.value.length),
        );
      if (Number.isFinite(scroll)) editor.scrollTop = scroll;
      return revision;
    },
    /** Search offsets address original source, not the textarea's normalized
     * view. Validate everything before changing the active chapter or focus. */
    selectSourceRange(index, start, end, checkpoint) {
      alive();
      if (reading || composing)
        throw bookError(
          "BOOK_BUSY",
          "Finish importing or composing text before navigating source.",
        );
      if (checkpoint !== JSON.stringify([collection.revision, signature()]))
        throw bookError("STALE_SOURCE", "The editor changed; run the search again.");
      const file = collection.files[index];
      if (!Number.isInteger(index) || !file || start > end)
        throw bookError("INVALID_SELECTION", "Choose a valid source match.");
      const from = bookEditorOffset(file.source, start),
        to = bookEditorOffset(file.source, end);
      active = index;
      showChapter();
      el["chapter-source"].focus();
      el["chapter-source"].setSelectionRange(from, to);
      el.status.textContent = `Selected source in ${file.path}. Search navigation did not change the book.`;
    },
    replaceProject(project, checkpoint) {
      alive();
      if (reading)
        throw bookError("BOOK_BUSY", "A local import is in progress; recovery did not replace it.");
      if (checkpoint !== JSON.stringify([collection.revision, signature()]))
        throw bookError(
          "STALE_SOURCE",
          "The editor changed during recovery; nothing was installed.",
        );
      // Validate before retiring downloads or changing the current document.
      const validated = normalizeBookProject(project);
      invalidate();
      collection.replaceProject(validated);
      active = 0;
      showOptions();
      showChapter();
      el.status.textContent =
        "Saved source reopened. Images and prepared downloads were revoked; authorize image files again before publishing.";
    },
    suspend() {
      if (!disposed) {
        pageSetup?.discard();
        collection.revokeImages();
        collection.revokeFonts();
        el.status.textContent =
          "Source retained in memory; reauthorize images and fonts after returning to this page.";
      }
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      invalidate();
      unsubscribe();
      for (const remove of unlisten) remove();
      sourceListeners.clear();
      pageSetup?.dispose();
      worker.dispose();
      collection.dispose();
    },
  });
}

// Added to the already-packaged publisher controller: both assemblers ship this
// path, so portable backup needs no new loader, WASM build, or renderer module.
function installPortableBookPanel(root) {
  const section = root.querySelector("#publish-title")?.parentElement;
  if (!section || root.querySelector("#save-portable")) return;
  const panel = root.createElement("div");
  const heading = root.createElement("h3");
  heading.textContent = "Portable backup and restore";
  const help = root.createElement("p");
  help.id = "portable-help";
  help.textContent = "Unlike source-only projects and local saves, a portable backup embeds all currently authorized images and supplied fonts with the exact chapter/include source and settings. It is unencrypted JSON, not a vault. Share only resources you have permission to distribute. No URLs are fetched. A restored book must still be checked by the publishing engine. Maximum file size: 192 MiB; normal source, image and font limits still apply.";
  const save = root.createElement("button");
  save.id = "save-portable"; save.type = "button";
  save.textContent = "Prepare portable backup (includes images and fonts)";
  save.setAttribute("aria-describedby", "portable-help status");
  const label = root.createElement("label");
  label.setAttribute("for", "open-portable"); label.textContent = "Reopen a portable book and review resource authorization";
  const input = root.createElement("input");
  input.id = "open-portable"; input.type = "file"; input.accept = ".json,application/json";
  input.setAttribute("aria-describedby", "portable-help portable-review status");
  const cancel = root.createElement("button");
  cancel.id = "cancel-portable"; cancel.type = "button"; cancel.disabled = true;
  cancel.textContent = "Cancel portable operation";
  const review = root.createElement("pre");
  review.id = "portable-review"; review.setAttribute("aria-label", "Portable resource review");
  review.textContent = "No portable restore is under review.";
  panel.append(heading, help, save, label, input, cancel, review);
  section.append(panel);
}
function portableReviewText(review) {
  return [
    `${review.title}: ${review.chapters} chapters, ${review.includeSources} include-only sources (${review.sourceBytes} source bytes).`,
    "Sources in saved order:",
    ...review.sources.map(source => `  [${source.role}] ${source.path}`),
    `Images (${review.imageBytes} bytes):`,
    ...review.images.map(image => `  ${image.destination} (${image.size} bytes)`),
    `Font roles (${review.fontBytes} bytes):`,
    ...review.fonts.map(font => `  ${font.slot}: ${font.name}, weight ${font.weight ?? "default"} (${font.size} bytes)`),
    "Restoring replaces the entire collection and authorizes only these embedded resource bytes.",
  ].join("\n");
}
// An asynchronous host confirmation may outlive cancellation/page disposal.
// Reject that wait promptly, release the private review, and observe late errors.
function portableApproval(promise, signal) {
  return new Promise((resolve, reject) => {
    const cleanup = () => signal.removeEventListener("abort", abort);
    const abort = () => { cleanup(); reject(bookError("ABORTED", "Portable restore cancelled during confirmation.")); };
    signal.addEventListener("abort", abort, { once: true });
    Promise.resolve(promise).then(value => { cleanup(); resolve(value); }, error => { cleanup(); reject(error); });
    if (signal.aborted) abort();
  });
}

// Page authoring stays in the already-shipped publisher controller. Geometry
// admission is the shared API contract; this is not a second page-layout engine.
const PAGE_UNITS = Object.freeze({ pt: 1, in: 72, mm: 72 / 25.4 });
const PAGE_FIELDS = ["width", "height", "top", "right", "bottom", "left"];
const PAGE_PRESETS = Object.freeze({
  letter: normalizePdfPage({ size: "letter" }).size,
  a4: normalizePdfPage({ size: "a4" }).size,
  trade: Object.freeze({ widthPt: 432, heightPt: 648 }),
});
const pageKey = value => JSON.stringify(value);
const pageNumber = value => String(Number(value.toPrecision(12)));

/** Optional workbench panel. The caller owns source capture and busy state.
 * Draft controls never change exported geometry until explicit Apply succeeds. */
export function createBookPageControls({ root, collection, capture, isBusy }) {
  const section = root.querySelector("#publish-title")?.parentElement;
  if (!section || root.querySelector("#book-page-settings")) return null;
  if (typeof capture !== "function" || typeof isBusy !== "function"
      || typeof collection?.setPage !== "function") throw new TypeError("Page setup needs a current book host.");
  const el = {}, panel = root.createElement("fieldset");
  panel.id = "book-page-settings";
  panel.setAttribute("aria-describedby", "book-page-help book-page-current book-page-status");
  const make = (tag, id, text, parent = panel) => {
    const node = root.createElement(tag);
    if (id) { node.id = `book-page-${id}`; el[id] = node; }
    if (text !== undefined) node.textContent = text;
    parent.append(node);
    return node;
  };
  make("legend", null, "PDF page setup");
  make("p", "help", "Choose paper, orientation and four margins for the actual book PDF and its proof. Drafts do not affect exports or saves until Apply page setup. HTML/EPUB reading layout is unchanged. Units change only the displayed numbers, not font scale. Reopen or page suspension discards unapplied page drafts.");
  function select(id, label, options) {
    make("label", null, label).setAttribute("for", `book-page-${id}`);
    const node = make("select", id);
    for (const [value, text] of options) make("option", null, text, node).value = value;
    return node;
  }
  select("size", "Paper", [["default", "Renderer default (Letter, 72 pt margins)"],
    ["letter", "US Letter"], ["a4", "A4"], ["trade", "6 × 9 inch book"], ["custom", "Custom dimensions"]]);
  select("unit", "Units for dimensions and margins", [["pt", "Points"], ["in", "Inches"], ["mm", "Millimetres"]]);
  select("orientation", "Orientation (margins keep their named sides)",
    [["keep", "As entered"], ["portrait", "Portrait"], ["landscape", "Landscape"]]);
  const grid = make("div"); grid.className = "grid";
  for (const field of PAGE_FIELDS) {
    const cell = make("div", null, undefined, grid);
    const label = field === "width" || field === "height" ? `Paper ${field}` : `${field[0].toUpperCase() + field.slice(1)} margin`;
    make("label", null, label, cell).setAttribute("for", `book-page-${field}`);
    const input = make("input", field, undefined, cell);
    input.type = "text"; input.inputMode = "decimal"; input.autocomplete = "off";
    input.setAttribute("aria-describedby", "book-page-unit book-page-help book-page-status");
  }
  make("p", null, "Dimensions: 144–14,400 points. Margins must leave at least 72 points of content width and height. Use a decimal point, not a comma.");
  make("p", "current");
  const status = make("p", "status"); status.setAttribute("role", "status");
  const actions = make("div");
  for (const [id, label] of [["apply", "Apply page setup"], ["discard", "Discard page draft"]]) {
    make("button", id, label, actions).type = "button";
  }
  let closed = false, unit = "pt", baseline;
  const displayed = new Map(), listeners = [];
  const blocked = () => closed || isBusy();
  function check() {
    if (closed) throw bookError("SESSION_DISPOSED", "PDF page controls are disposed.");
    if (isBusy()) throw bookError("BOOK_BUSY", "Finish importing or composing text before applying page setup.");
  }
  function write(field, points, nextUnit = unit) {
    const text = pageNumber(points / PAGE_UNITS[nextUnit]);
    displayed.set(field, { text, points, unit: nextUnit });
    el[field].value = text;
  }
  function points(field) {
    const raw = el[field].value.trim(), known = displayed.get(field);
    // Retain the exact original points when a formatted field is untouched.
    // Repeated unit changes or reopening must not drift A4/custom geometry.
    if (known && known.unit === unit && raw === known.text) return known.points;
    if (!/^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(raw))
      throw bookError("INVALID_OPTIONS", `Enter a nonnegative decimal number for ${field}.`);
    const value = Number(raw) * PAGE_UNITS[unit];
    if (!Number.isFinite(value)) throw bookError("INVALID_OPTIONS", `The ${field} value is too large.`);
    return value;
  }
  function draft() {
    if (el.unit.value !== unit) throw bookError("INVALID_OPTIONS", "Choose the units again before applying page setup.");
    const size = el.size.value;
    if (size === "default") return undefined;
    if (size !== "custom" && !Object.hasOwn(PAGE_PRESETS, size))
      throw bookError("INVALID_OPTIONS", "Choose a supported paper size.");
    const orientation = el.orientation.value;
    if (!["keep", "portrait", "landscape"].includes(orientation))
      throw bookError("INVALID_OPTIONS", "Choose a supported orientation.");
    return normalizePdfPage({
      size: size === "custom" ? { widthPt: points("width"), heightPt: points("height") } : PAGE_PRESETS[size],
      orientation: orientation === "keep" ? undefined : orientation,
      margins: Object.fromEntries(PAGE_FIELDS.slice(2).map(side => [`${side}Pt`, points(side)])),
    });
  }
  function describe(value) {
    const page = value ?? normalizePdfPage({});
    const { widthPt: w, heightPt: h } = page.size, m = page.margins;
    return `${pageNumber(w)} × ${pageNumber(h)} pt; content ${pageNumber(w - m.leftPt - m.rightPt)} × ${pageNumber(h - m.topPt - m.bottomPt)} pt; margins top/right/bottom/left ${[m.topPt, m.rightPt, m.bottomPt, m.leftPt].map(pageNumber).join(" / ")} pt.`;
  }
  function refresh() {
    const disabled = blocked(), automatic = el.size.value === "default";
    for (const id of ["size", "unit", "apply", "discard"]) el[id].disabled = disabled;
    el.orientation.disabled = disabled || automatic;
    for (const field of PAGE_FIELDS)
      el[field].disabled = disabled || automatic || ((field === "width" || field === "height") && el.size.value !== "custom");
    if (!closed) el.current.textContent = `Current PDF: ${collection.options.page === undefined ? "renderer defaults; " : ""}${describe(collection.options.page)}`;
  }
  function review() {
    try {
      const value = draft();
      status.textContent = pageKey(value) === baseline
        ? "Page draft matches the current PDF settings. No page change is pending."
        : `Unapplied page draft: ${describe(value)} Apply to use it for PDF proofing, exports and saved projects.`;
    } catch (error) {
      status.textContent = `${error.code ?? "INVALID_OPTIONS"}: ${error.message} Current PDF settings are unchanged.`;
    }
    refresh();
  }
  function discard() {
    if (closed) return;
    const value = collection.options.page, page = value ?? normalizePdfPage({});
    baseline = pageKey(value); el.unit.value = unit;
    let preset = "custom";
    for (const [name, size] of Object.entries(PAGE_PRESETS)) {
      if ((page.size.widthPt === size.widthPt && page.size.heightPt === size.heightPt)
          || (page.size.widthPt === size.heightPt && page.size.heightPt === size.widthPt)) { preset = name; break; }
    }
    el.size.value = value === undefined ? "default" : preset;
    el.orientation.value = preset === "custom" ? "keep" : page.size.widthPt > page.size.heightPt ? "landscape" : "portrait";
    const size = preset === "custom" ? page.size : PAGE_PRESETS[preset];
    write("width", size.widthPt); write("height", size.heightPt);
    for (const side of PAGE_FIELDS.slice(2)) write(side, page.margins[`${side}Pt`]);
    review();
  }
  const signature = () => JSON.stringify([el.size.value, el.unit.value, el.orientation.value, ...PAGE_FIELDS.map(field => el[field].value)]);
  function apply() {
    check();
    const value = draft(), selected = signature(), previous = baseline;
    if (pageKey(collection.options.page) !== previous)
      throw bookError("STALE_SOURCE", "PDF settings changed; review the current page before applying.");
    // Invalid page drafts fail above, before source capture. Valid raw editor
    // settings remain owned by the ordinary publisher capture path.
    capture();
    check();
    if (signature() !== selected || pageKey(collection.options.page) !== previous)
      throw bookError("STALE_SOURCE", "The page draft changed during capture; nothing was applied.");
    const before = collection.revision, installed = collection.setPage(value, before);
    discard();
    status.textContent = installed === before ? "PDF page settings were already current; no page change was made."
      : "PDF page setup applied. Generate a new book PDF proof or export. Save the project to retain this setup.";
    return installed;
  }
  function run(work) {
    try { work(); }
    catch (error) { status.textContent = `${error.code ?? "PAGE_ERROR"}: ${error.message}`; refresh(); }
  }
  function on(node, event, action) {
    node.addEventListener(event, action); listeners.push(() => node.removeEventListener(event, action));
  }
  on(el.apply, "click", () => run(apply));
  on(el.discard, "click", discard);
  on(el.size, "change", () => run(() => {
    const selected = el.size.value;
    const size = selected === "default" ? PAGE_PRESETS.letter : PAGE_PRESETS[selected];
    if (size) { write("width", size.widthPt); write("height", size.heightPt); }
    if (selected === "default") {
      el.orientation.value = "portrait";
      for (const side of PAGE_FIELDS.slice(2)) write(side, 72);
    }
    review();
  }));
  on(el.unit, "change", () => run(() => {
    const selected = el.unit.value;
    try {
      if (!Object.hasOwn(PAGE_UNITS, selected)) throw bookError("INVALID_OPTIONS", "Choose points, inches or millimetres.");
      const values = PAGE_FIELDS.map(points); // Complete admission before any field changes.
      PAGE_FIELDS.forEach((field, i) => write(field, values[i], selected));
      unit = selected;
    } catch (error) { el.unit.value = unit; throw error; }
    review();
  }));
  on(el.orientation, "change", review);
  for (const field of PAGE_FIELDS) on(el[field], "input", review);
  discard();
  section.append(panel);
  const unsubscribe = collection.subscribe(() => {
    if (pageKey(collection.options.page) !== baseline) discard();
    else refresh();
  });
  return Object.freeze({ apply, discard, refresh,
    dispose() {
      if (closed) return;
      closed = true; unsubscribe();
      for (const remove of listeners) remove();
      refresh();
    },
  });
}
