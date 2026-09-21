import { bookError } from "../book_worker.mjs";
import {
  createBookCollection,
  normalizeBookProject,
  readBookFiles,
  readBookProject,
} from "./book_collection.mjs";
import { bookEditorOffset } from "./book_source_search.mjs";

/** UI orchestration is separately testable; the host supplies the real worker.
 * Nothing in this controller parses Markdown or inserts rendered HTML.
 */
export function createBookControls({
  root,
  worker,
  confirm = () => true,
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
  const unlisten = [],
    sourceListeners = new Set();
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
    el["cancel-export"].disabled = !preparing;
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
    capture();
    invalidate();
    const revision = collection.revision,
      view = signature();
    reading = true;
    buttons();
    const fence = () => {
      alive();
      if (revision !== collection.revision || view !== signature())
        throw bookError(
          "STALE_SOURCE",
          "The collection changed during import; nothing was installed.",
        );
    };
    try {
      if (mode === "project") {
        const project = await readBookProject(files[0]);
        fence();
        const approved = await confirm(
          "Replace this collection with the source project? Save the current project first. All current image access will be revoked.",
        );
        fence();
        if (approved !== true) return false;
        collection.replaceProject(project);
        active = 0;
        showOptions();
        showChapter();
        onProjectReplaced();
        el.status.textContent =
          "Source project reopened. Source roles, chapter order and text are restored; reauthorize its image files before exporting.";
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
  ])
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
        el.status.textContent =
          "Source retained in memory; reauthorize images after returning to this page.";
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
