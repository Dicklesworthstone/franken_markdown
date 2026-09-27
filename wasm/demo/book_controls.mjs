import { bookError } from "../book_worker.mjs";
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
